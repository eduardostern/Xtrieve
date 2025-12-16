//! Open file table - manages Btrieve files that are currently open
//!
//! Each open file has associated metadata, page cache entries, and cursors.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::storage::fcr::FileControlRecord;
use crate::storage::page::{Page, PageIO, PageType};

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

        // Page size is at offset 14 (after 12-byte page header + 2-byte record length)
        let page_size = u16::from_le_bytes([header[14], header[15]]);

        // Validate page size
        if !crate::storage::page::PAGE_SIZES.contains(&page_size) {
            return Err(BtrieveError::InvalidFormat(format!(
                "Invalid page size: {}",
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
        })
    }

    /// Read a page from the file
    pub fn read_page(&self, page_number: u32) -> BtrieveResult<Page> {
        let mut file = self.file.write();
        let offset = (page_number as u64) * (self.fcr.page_size as u64);

        file.seek(SeekFrom::Start(offset))?;

        let mut data = vec![0u8; self.fcr.page_size as usize];
        file.read_exact(&mut data).map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                BtrieveError::Status(StatusCode::InvalidRecordAddress)
            } else {
                BtrieveError::Io(e)
            }
        })?;

        Ok(Page::from_data(page_number, data))
    }

    /// Write a page to the file
    pub fn write_page(&self, page: &Page) -> BtrieveResult<()> {
        if self.mode.read_only {
            return Err(BtrieveError::Status(StatusCode::AccessDenied));
        }

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
