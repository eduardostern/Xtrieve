//! Open file table - manages Btrieve files that are currently open
//!
//! Each open file has associated metadata, page cache entries, and cursors.
//! Supports pre-imaging for transaction rollback.

use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::storage::fcr::FileControlRecord;
use crate::storage::page::Page;

/// Open mode flags (match Btrieve)
#[derive(Debug, Clone, Copy)]
pub struct OpenMode {
    /// Read-only mode
    pub read_only: bool,
    /// Exclusive access (no other opens)
    pub exclusive: bool,
    /// Accelerated mode (fewer flushes)
    pub accelerated: bool,
}

impl OpenMode {
    pub fn from_raw(mode: i32) -> Self {
        OpenMode {
            read_only: (mode & 0x01) != 0,        // -1 = normal, -2 = read-only
            exclusive: (mode & 0x04) != 0,       // -4 = exclusive
            accelerated: (mode & 0x10) != 0,     // Accelerated mode
        }
    }

    pub fn read_write() -> Self {
        OpenMode {
            read_only: false,
            exclusive: false,
            accelerated: false,
        }
    }

    pub fn read_only() -> Self {
        OpenMode {
            read_only: true,
            exclusive: false,
            accelerated: false,
        }
    }
}

/// Per-session pre-image for transaction rollback (Btrieve 5.1 style)
/// Stores OLD page data before modification - for restore on abort
struct SessionPreImage {
    /// The pre-image file handle
    file: File,
    /// Pages that have been pre-imaged (to avoid duplicates)
    pages: HashSet<u32>,
}

/// An open Btrieve file
pub struct OpenFile {
    /// File path
    pub path: PathBuf,
    /// File Control Record
    pub fcr: FileControlRecord,
    /// Open mode
    pub mode: OpenMode,
    /// Underlying file handle
    file: RwLock<File>,
    /// Reference count (number of opens)
    pub ref_count: u32,
    /// Per-session pre-image files for transaction rollback
    /// Key: session_id, Value: pre-image file storing OLD data
    session_preimages: RwLock<HashMap<u64, SessionPreImage>>,
}

impl OpenFile {
    /// Open an existing Btrieve file
    pub fn open(path: &Path, mode: OpenMode) -> BtrieveResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(!mode.read_only)
            .open(path)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    BtrieveError::Status(StatusCode::FileNotFound)
                } else {
                    BtrieveError::Io(e)
                }
            })?;

        // Read page 0 to determine page size, then read full FCR
        let mut file = file;
        let mut header = [0u8; 64];
        file.read_exact(&mut header).map_err(|_| {
            BtrieveError::Status(StatusCode::NotBtrieveFile)
        })?;

        // Btrieve 5.1: page size is at offset 0x08
        let page_size = u16::from_le_bytes([header[0x08], header[0x09]]);

        // Validate page size
        if !crate::storage::page::PAGE_SIZES.contains(&page_size) {
            return Err(BtrieveError::InvalidFormat(format!(
                "Invalid page size: {} (expected 512, 1024, 2048, or 4096)",
                page_size
            )));
        }

        // Read full page 0
        file.seek(SeekFrom::Start(0))?;
        let mut page_data = vec![0u8; page_size as usize];
        file.read_exact(&mut page_data)?;

        // Parse FCR
        let fcr = FileControlRecord::from_bytes(&page_data)?;

        Ok(OpenFile {
            path: path.to_path_buf(),
            fcr,
            mode,
            file: RwLock::new(file),
            ref_count: 1,
            session_preimages: RwLock::new(HashMap::new()),
        })
    }

    /// Create a new Btrieve file
    pub fn create(path: &Path, fcr: FileControlRecord) -> BtrieveResult<Self> {
        // Check if file exists
        if path.exists() {
            return Err(BtrieveError::Status(StatusCode::FileAlreadyExists));
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        // Write FCR to page 0
        let fcr_data = fcr.to_bytes();
        let mut file = file;
        file.write_all(&fcr_data)?;
        file.flush()?;

        Ok(OpenFile {
            path: path.to_path_buf(),
            fcr,
            mode: OpenMode::read_write(),
            file: RwLock::new(file),
            ref_count: 1,
            session_preimages: RwLock::new(HashMap::new()),
        })
    }

    /// Read a page from the file
    pub fn read_page(&self, page_number: u32) -> BtrieveResult<Page> {
        let mut file = self.file.write();
        let offset = (page_number as u64) * (self.fcr.page_size as u64);
        file.seek(SeekFrom::Start(offset))?;

        let mut data = vec![0u8; self.fcr.page_size as usize];
        file.read_exact(&mut data)?;

        Ok(Page::from_data(page_number, data))
    }

    /// Write a page - Btrieve 5.1 style
    /// If in transaction: save OLD data to .PRE first, then write to main file
    /// Outside transaction: write directly to main file
    pub fn write_page(&self, page: &Page) -> BtrieveResult<()> {
        self.write_page_for_session(page, 0)
    }

    /// Write a page for a specific session
    /// Btrieve 5.1 model: save old data to PRE, then write new data to main file
    pub fn write_page_for_session(&self, page: &Page, session_id: u64) -> BtrieveResult<()> {
        if self.mode.read_only {
            return Err(BtrieveError::Status(StatusCode::AccessDenied));
        }

        // Check if this session has an active transaction
        let has_preimage = {
            let preimages = self.session_preimages.read();
            preimages.contains_key(&session_id)
        };

        // During transaction: save OLD page to PRE before modifying
        if has_preimage && session_id > 0 {
            let mut preimages = self.session_preimages.write();
            if let Some(preimage) = preimages.get_mut(&session_id) {
                // Only save pre-image once per page (first modification wins)
                if !preimage.pages.contains(&page.page_number) {
                    // Read current (old) page data from main file
                    let mut file = self.file.write();
                    let offset = (page.page_number as u64) * (self.fcr.page_size as u64);

                    // Check if page exists (might be new allocation)
                    let file_len = file.seek(SeekFrom::End(0))?;
                    if offset < file_len {
                        file.seek(SeekFrom::Start(offset))?;
                        let mut old_data = vec![0u8; self.fcr.page_size as usize];
                        file.read_exact(&mut old_data)?;

                        // Write old data to PRE file
                        preimage.file.seek(SeekFrom::End(0))?;
                        preimage.file.write_all(&page.page_number.to_le_bytes())?;
                        preimage.file.write_all(&(old_data.len() as u32).to_le_bytes())?;
                        preimage.file.write_all(&old_data)?;
                        preimage.file.flush()?;
                    }
                    preimage.pages.insert(page.page_number);
                }
            }
        }

        // Write new data directly to main file (Btrieve 5.1 style)
        let mut file = self.file.write();
        let offset = (page.page_number as u64) * (self.fcr.page_size as u64);

        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&page.data)?;

        if !self.mode.accelerated {
            file.flush()?;
        }

        Ok(())
    }

    /// Allocate a new page
    pub fn allocate_page(&self) -> BtrieveResult<Page> {
        if self.mode.read_only {
            return Err(BtrieveError::Status(StatusCode::AccessDenied));
        }

        let mut file = self.file.write();
        let end = file.seek(SeekFrom::End(0))?;
        let page_number = (end / self.fcr.page_size as u64) as u32;

        let page = Page::new(page_number, self.fcr.page_size);
        file.write_all(&page.data)?;

        Ok(page)
    }

    /// Flush all writes to disk
    pub fn flush(&self) -> BtrieveResult<()> {
        let file = self.file.write();
        file.sync_all()?;
        Ok(())
    }

    /// Get the number of pages in the file
    pub fn page_count(&self) -> BtrieveResult<u32> {
        let mut file = self.file.write();
        let end = file.seek(SeekFrom::End(0))?;
        Ok((end / self.fcr.page_size as u64) as u32)
    }

    /// Update FCR and write to page 0
    pub fn update_fcr(&mut self) -> BtrieveResult<()> {
        if self.mode.read_only {
            return Err(BtrieveError::Status(StatusCode::AccessDenied));
        }

        let fcr_data = self.fcr.to_bytes();
        let page = Page::from_data(0, fcr_data);
        self.write_page(&page)
    }

    /// Get pre-image file path for a session
    fn preimage_path(&self, session_id: u64) -> PathBuf {
        let mut path = self.path.clone();
        let ext = format!("PRE.{}", session_id);
        path.set_extension(ext);
        path
    }

    /// Begin a transaction for a specific session - create PRE file
    /// Btrieve 5.1: PRE stores OLD data for rollback
    pub fn begin_transaction(&self, session_id: u64) -> BtrieveResult<()> {
        let mut preimages = self.session_preimages.write();

        // Check if session already has a transaction
        if preimages.contains_key(&session_id) {
            return Ok(()); // Already in transaction
        }

        // Create per-session pre-image file
        let pre_path = self.preimage_path(session_id);
        let pre_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&pre_path)?;

        preimages.insert(session_id, SessionPreImage {
            file: pre_file,
            pages: HashSet::new(),
        });

        Ok(())
    }

    /// Commit transaction - just delete PRE file
    /// Btrieve 5.1: changes already written to main file, PRE no longer needed
    pub fn commit_transaction(&self, session_id: u64) -> BtrieveResult<()> {
        let mut preimages = self.session_preimages.write();

        // Remove session's pre-image
        if preimages.remove(&session_id).is_some() {
            // Sync main file
            let file = self.file.write();
            file.sync_all()?;

            // Delete PRE file - changes are committed
            let pre_path = self.preimage_path(session_id);
            let _ = fs::remove_file(&pre_path);
        }

        Ok(())
    }

    /// Abort transaction - restore pages from PRE to main file
    /// Btrieve 5.1: PRE contains OLD data, restore it to undo changes
    pub fn abort_transaction(&self, session_id: u64) -> BtrieveResult<()> {
        let mut preimages = self.session_preimages.write();

        // Get and remove session's pre-image
        let preimage = match preimages.remove(&session_id) {
            Some(p) => p,
            None => return Ok(()), // Not in transaction
        };

        let SessionPreImage { mut file, pages: _ } = preimage;

        // Restore all pages from PRE to main file
        file.seek(SeekFrom::Start(0))?;
        let mut main_file = self.file.write();

        loop {
            // Read page_number (4 bytes)
            let mut page_num_buf = [0u8; 4];
            if file.read_exact(&mut page_num_buf).is_err() {
                break; // End of file
            }
            let page_number = u32::from_le_bytes(page_num_buf);

            // Read data_len (4 bytes)
            let mut len_buf = [0u8; 4];
            if file.read_exact(&mut len_buf).is_err() {
                break;
            }
            let data_len = u32::from_le_bytes(len_buf) as usize;

            // Read original (old) data
            let mut old_data = vec![0u8; data_len];
            if file.read_exact(&mut old_data).is_err() {
                break;
            }

            // Restore original page to main file
            let offset = (page_number as u64) * (self.fcr.page_size as u64);
            main_file.seek(SeekFrom::Start(offset))?;
            main_file.write_all(&old_data)?;
        }

        main_file.sync_all()?;
        drop(main_file);

        // Delete PRE file
        let pre_path = self.preimage_path(session_id);
        let _ = fs::remove_file(&pre_path);

        Ok(())
    }

    /// Check if a specific session has an active transaction
    pub fn is_in_transaction(&self, session_id: u64) -> bool {
        let preimages = self.session_preimages.read();
        preimages.contains_key(&session_id)
    }

    /// Check if any session has an active transaction
    pub fn has_active_transactions(&self) -> bool {
        let preimages = self.session_preimages.read();
        !preimages.is_empty()
    }
}

/// Table of all open files
pub struct OpenFileTable {
    files: RwLock<HashMap<PathBuf, Arc<RwLock<OpenFile>>>>,
}

impl OpenFileTable {
    pub fn new() -> Self {
        OpenFileTable {
            files: RwLock::new(HashMap::new()),
        }
    }

    /// Open a file (or increment ref count if already open)
    pub fn open(&self, path: &Path, mode: OpenMode) -> BtrieveResult<Arc<RwLock<OpenFile>>> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Check if already open
        {
            let files = self.files.read();
            if let Some(file) = files.get(&canonical) {
                let mut f = file.write();
                f.ref_count += 1;
                return Ok(file.clone());
            }
        }

        // Open new file
        let open_file = OpenFile::open(path, mode)?;
        let open_file = Arc::new(RwLock::new(open_file));

        let mut files = self.files.write();
        files.insert(canonical, open_file.clone());

        Ok(open_file)
    }

    /// Create a new file
    pub fn create(
        &self,
        path: &Path,
        fcr: FileControlRecord,
    ) -> BtrieveResult<Arc<RwLock<OpenFile>>> {
        let canonical = path
            .parent()
            .map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
            .unwrap_or_default()
            .join(path.file_name().unwrap_or_default());

        // Check if already open
        {
            let files = self.files.read();
            if files.contains_key(&canonical) {
                return Err(BtrieveError::Status(StatusCode::FileInUse));
            }
        }

        // Create new file
        let open_file = OpenFile::create(path, fcr)?;
        let open_file = Arc::new(RwLock::new(open_file));

        let mut files = self.files.write();
        files.insert(canonical, open_file.clone());

        Ok(open_file)
    }

    /// Close a file (decrement ref count)
    pub fn close(&self, path: &Path) -> BtrieveResult<bool> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        let mut files = self.files.write();
        if let Some(file) = files.get(&canonical) {
            let mut f = file.write();
            f.ref_count = f.ref_count.saturating_sub(1);

            if f.ref_count == 0 {
                // Flush before closing
                let _ = f.flush();
                drop(f);
                files.remove(&canonical);
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Get an open file
    pub fn get(&self, path: &Path) -> Option<Arc<RwLock<OpenFile>>> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let files = self.files.read();
        files.get(&canonical).cloned()
    }

    /// Get number of open files
    pub fn len(&self) -> usize {
        self.files.read().len()
    }

    /// Check if any files are open
    pub fn is_empty(&self) -> bool {
        self.files.read().is_empty()
    }

    /// Close all files
    pub fn close_all(&self) {
        let mut files = self.files.write();
        for (_, file) in files.drain() {
            let f = file.write();
            let _ = f.flush();
        }
    }
}

impl Default for OpenFileTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::key::{KeySpec, KeyFlags, KeyType};
    use tempfile::tempdir;

    #[test]
    fn test_create_and_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.dat");

        // Create a file
        let key = KeySpec {
            position: 0,
            length: 10,
            flags: KeyFlags::empty(),
            key_type: KeyType::String,
            null_value: 0,
            acs_number: 0,
            unique_count: 0,
        };

        let fcr = FileControlRecord::new(100, 4096, vec![key]);
        let _file = OpenFile::create(&path, fcr).unwrap();
        drop(_file);

        // Reopen
        let file = OpenFile::open(&path, OpenMode::read_only()).unwrap();
        assert_eq!(file.fcr.record_length, 100);
        assert_eq!(file.fcr.page_size, 4096);
        assert_eq!(file.fcr.num_keys, 1);
    }
}
