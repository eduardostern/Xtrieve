//! Btrieve-style file set management
//!
//! Manages the complete file set for a Btrieve database:
//! - .DAT - Main data file (FCR + data pages)
//! - .IX# - Index files (one per key: .IX0, .IX1, etc.)
//! - .PRE - Pre-image file (for transaction rollback)
//!
//! This follows the original Btrieve 5.1 architecture.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use super::fcr::FileControlRecord;
use super::key::KeySpec;
use super::page::Page;

/// File extensions
pub const DATA_EXT: &str = "DAT";
pub const INDEX_EXT_PREFIX: &str = "IX";
pub const PREIMAGE_EXT: &str = "PRE";

/// Index file header (stored in page 0 of each .IX# file)
#[derive(Debug, Clone)]
pub struct IndexFileHeader {
    /// Signature to identify as Xtrieve index file
    pub signature: [u8; 4],
    /// Page size (must match data file)
    pub page_size: u16,
    /// Key number this index represents
    pub key_number: u16,
    /// Copy of key specification
    pub key_spec: KeySpec,
    /// Root page of B+ tree (0 if empty)
    pub root_page: u32,
    /// Number of pages in this index file
    pub num_pages: u32,
    /// Number of entries in the index
    pub num_entries: u32,
    /// Unique key counter (for duplicate key handling)
    pub unique_count: u32,
}

impl IndexFileHeader {
    pub const SIZE: usize = 64;
    pub const SIGNATURE: [u8; 4] = [b'X', b'I', b'D', b'X']; // "XIDX"

    pub fn new(page_size: u16, key_number: u16, key_spec: KeySpec) -> Self {
        IndexFileHeader {
            signature: Self::SIGNATURE,
            page_size,
            key_number,
            key_spec,
            root_page: 0,
            num_pages: 1, // Just header page initially
            num_entries: 0,
            unique_count: 0,
        }
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < Self::SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Index header too short"));
        }

        let mut cursor = std::io::Cursor::new(data);

        let mut signature = [0u8; 4];
        cursor.read_exact(&mut signature)?;

        if signature != Self::SIGNATURE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid index signature"));
        }

        let page_size = cursor.read_u16::<LittleEndian>()?;
        let key_number = cursor.read_u16::<LittleEndian>()?;

        // Read key spec (16 bytes)
        let mut key_bytes = [0u8; 16];
        cursor.read_exact(&mut key_bytes)?;
        let key_spec = KeySpec::from_bytes(&key_bytes)?;

        let root_page = cursor.read_u32::<LittleEndian>()?;
        let num_pages = cursor.read_u32::<LittleEndian>()?;
        let num_entries = cursor.read_u32::<LittleEndian>()?;
        let unique_count = cursor.read_u32::<LittleEndian>()?;

        Ok(IndexFileHeader {
            signature,
            page_size,
            key_number,
            key_spec,
            root_page,
            num_pages,
            num_entries,
            unique_count,
        })
    }

    pub fn to_bytes(&self, page_size: u16) -> Vec<u8> {
        let mut buf = vec![0u8; page_size as usize];

        buf[0..4].copy_from_slice(&self.signature);

        let mut cursor = std::io::Cursor::new(&mut buf[4..]);
        cursor.write_u16::<LittleEndian>(self.page_size).unwrap();
        cursor.write_u16::<LittleEndian>(self.key_number).unwrap();

        // Key spec (16 bytes)
        let key_bytes = self.key_spec.to_bytes();
        cursor.write_all(&key_bytes).unwrap();

        cursor.write_u32::<LittleEndian>(self.root_page).unwrap();
        cursor.write_u32::<LittleEndian>(self.num_pages).unwrap();
        cursor.write_u32::<LittleEndian>(self.num_entries).unwrap();
        cursor.write_u32::<LittleEndian>(self.unique_count).unwrap();

        buf
    }
}

/// Pre-image record (one per modified page)
#[derive(Debug, Clone)]
pub struct PreImageRecord {
    /// Source: 0 = data file, 1+ = index file number
    pub source: u8,
    /// Page number in the source file
    pub page_number: u32,
    /// Original page data before modification
    pub original_data: Vec<u8>,
}

impl PreImageRecord {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(5 + self.original_data.len());
        buf.push(self.source);
        buf.extend_from_slice(&self.page_number.to_le_bytes());
        buf.extend_from_slice(&(self.original_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.original_data);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<(Self, usize)> {
        if data.len() < 9 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Pre-image record too short"));
        }

        let source = data[0];
        let page_number = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
        let data_len = u32::from_le_bytes([data[5], data[6], data[7], data[8]]) as usize;

        if data.len() < 9 + data_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Pre-image data truncated"));
        }

        let original_data = data[9..9 + data_len].to_vec();
        let total_len = 9 + data_len;

        Ok((PreImageRecord { source, page_number, original_data }, total_len))
    }
}

/// Pre-image file header
#[derive(Debug, Clone)]
pub struct PreImageHeader {
    /// Signature
    pub signature: [u8; 4],
    /// Transaction ID
    pub transaction_id: u64,
    /// Session ID that owns this pre-image
    pub session_id: u64,
    /// Base file name (without extension)
    pub base_name: String,
}

impl PreImageHeader {
    pub const SIGNATURE: [u8; 4] = [b'X', b'P', b'R', b'E']; // "XPRE"

    pub fn new(transaction_id: u64, session_id: u64, base_name: &str) -> Self {
        PreImageHeader {
            signature: Self::SIGNATURE,
            transaction_id,
            session_id,
            base_name: base_name.to_string(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(&self.signature);
        buf.extend_from_slice(&self.transaction_id.to_le_bytes());
        buf.extend_from_slice(&self.session_id.to_le_bytes());

        let name_bytes = self.base_name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(name_bytes);

        buf
    }

    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 24 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Pre-image header too short"));
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if signature != Self::SIGNATURE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid pre-image signature"));
        }

        let transaction_id = u64::from_le_bytes([
            data[4], data[5], data[6], data[7],
            data[8], data[9], data[10], data[11],
        ]);
        let session_id = u64::from_le_bytes([
            data[12], data[13], data[14], data[15],
            data[16], data[17], data[18], data[19],
        ]);
        let name_len = u32::from_le_bytes([data[20], data[21], data[22], data[23]]) as usize;

        if data.len() < 24 + name_len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Pre-image name truncated"));
        }

        let base_name = String::from_utf8_lossy(&data[24..24 + name_len]).to_string();

        Ok(PreImageHeader {
            signature,
            transaction_id,
            session_id,
            base_name,
        })
    }
}

/// Manages a complete Btrieve file set
pub struct BtrieveFileSet {
    /// Base path (without extension)
    pub base_path: PathBuf,
    /// Data file handle
    pub data_file: Option<File>,
    /// Index file handles
    pub index_files: Vec<Option<File>>,
    /// Pre-image file handle (only during transactions)
    pub preimage_file: Option<File>,
    /// Cached FCR
    pub fcr: Option<FileControlRecord>,
    /// Cached index headers
    pub index_headers: Vec<Option<IndexFileHeader>>,
    /// Page size
    pub page_size: u16,
}

impl BtrieveFileSet {
    /// Get the data file path
    pub fn data_path(base: &Path) -> PathBuf {
        base.with_extension(DATA_EXT)
    }

    /// Get an index file path
    pub fn index_path(base: &Path, key_num: usize) -> PathBuf {
        base.with_extension(format!("{}{}", INDEX_EXT_PREFIX, key_num))
    }

    /// Get the pre-image file path
    pub fn preimage_path(base: &Path) -> PathBuf {
        base.with_extension(PREIMAGE_EXT)
    }

    /// Create a new file set
    pub fn create(
        base_path: PathBuf,
        record_length: u16,
        page_size: u16,
        keys: Vec<KeySpec>,
    ) -> io::Result<Self> {
        let num_keys = keys.len();

        // Create FCR
        let fcr = FileControlRecord::new(record_length, page_size, keys.clone());

        // Create and write data file
        let data_path = Self::data_path(&base_path);
        let mut data_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&data_path)?;

        let fcr_bytes = fcr.to_bytes();
        data_file.write_all(&fcr_bytes)?;
        data_file.sync_all()?;

        // Create index files
        let mut index_files = Vec::with_capacity(num_keys);
        let mut index_headers = Vec::with_capacity(num_keys);

        for (i, key_spec) in keys.iter().enumerate() {
            let index_path = Self::index_path(&base_path, i);
            let mut index_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&index_path)?;

            let header = IndexFileHeader::new(page_size, i as u16, key_spec.clone());
            let header_bytes = header.to_bytes(page_size);
            index_file.write_all(&header_bytes)?;
            index_file.sync_all()?;

            index_files.push(Some(index_file));
            index_headers.push(Some(header));
        }

        Ok(BtrieveFileSet {
            base_path,
            data_file: Some(data_file),
            index_files,
            preimage_file: None,
            fcr: Some(fcr),
            index_headers,
            page_size,
        })
    }

    /// Open an existing file set
    pub fn open(base_path: PathBuf) -> io::Result<Self> {
        // Open data file
        let data_path = Self::data_path(&base_path);
        let mut data_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&data_path)?;

        // Read FCR
        let mut fcr_buf = vec![0u8; 4096]; // Read first page
        data_file.read_exact(&mut fcr_buf)?;
        let fcr = FileControlRecord::from_bytes(&fcr_buf)?;
        let page_size = fcr.page_size;
        let num_keys = fcr.num_keys as usize;

        // Open index files
        let mut index_files = Vec::with_capacity(num_keys);
        let mut index_headers = Vec::with_capacity(num_keys);

        for i in 0..num_keys {
            let index_path = Self::index_path(&base_path, i);
            if index_path.exists() {
                let mut index_file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&index_path)?;

                // Read index header
                let mut header_buf = vec![0u8; page_size as usize];
                index_file.read_exact(&mut header_buf)?;
                let header = IndexFileHeader::from_bytes(&header_buf)?;

                index_files.push(Some(index_file));
                index_headers.push(Some(header));
            } else {
                index_files.push(None);
                index_headers.push(None);
            }
        }

        // Check for orphaned pre-image file (crash recovery)
        let preimage_path = Self::preimage_path(&base_path);
        if preimage_path.exists() {
            // TODO: Perform crash recovery by rolling back
            tracing::warn!("Found orphaned pre-image file, should recover: {:?}", preimage_path);
        }

        Ok(BtrieveFileSet {
            base_path,
            data_file: Some(data_file),
            index_files,
            preimage_file: None,
            fcr: Some(fcr),
            index_headers,
            page_size,
        })
    }

    /// Start a transaction (create pre-image file)
    pub fn begin_transaction(&mut self, transaction_id: u64, session_id: u64) -> io::Result<()> {
        let preimage_path = Self::preimage_path(&self.base_path);
        let mut preimage_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&preimage_path)?;

        let base_name = self.base_path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let header = PreImageHeader::new(transaction_id, session_id, &base_name);
        preimage_file.write_all(&header.to_bytes())?;
        preimage_file.sync_all()?;

        self.preimage_file = Some(preimage_file);
        Ok(())
    }

    /// Save a pre-image before modifying a page
    pub fn save_preimage(&mut self, source: u8, page_number: u32, original_data: &[u8]) -> io::Result<()> {
        if let Some(ref mut preimage) = self.preimage_file {
            let record = PreImageRecord {
                source,
                page_number,
                original_data: original_data.to_vec(),
            };
            preimage.seek(SeekFrom::End(0))?;
            preimage.write_all(&record.to_bytes())?;
            preimage.sync_all()?;
        }
        Ok(())
    }

    /// Commit transaction (delete pre-image file)
    pub fn commit_transaction(&mut self) -> io::Result<()> {
        if self.preimage_file.take().is_some() {
            let preimage_path = Self::preimage_path(&self.base_path);
            fs::remove_file(preimage_path)?;
        }

        // Sync all files
        if let Some(ref mut data) = self.data_file {
            data.sync_all()?;
        }
        for index_file in &mut self.index_files {
            if let Some(ref mut f) = index_file {
                f.sync_all()?;
            }
        }

        Ok(())
    }

    /// Abort transaction (restore from pre-image)
    pub fn abort_transaction(&mut self) -> io::Result<()> {
        let preimage_path = Self::preimage_path(&self.base_path);

        if let Some(mut preimage) = self.preimage_file.take() {
            // Read all pre-image records and restore
            preimage.seek(SeekFrom::Start(0))?;

            // Read header
            let mut header_buf = vec![0u8; 256];
            preimage.read_exact(&mut header_buf)?;
            let header = PreImageHeader::from_bytes(&header_buf)?;

            // Read pre-image records
            let mut all_data = Vec::new();
            preimage.read_to_end(&mut all_data)?;

            let mut offset = 0;
            while offset < all_data.len() {
                if let Ok((record, len)) = PreImageRecord::from_bytes(&all_data[offset..]) {
                    // Restore the original page
                    self.write_page_raw(record.source, record.page_number, &record.original_data)?;
                    offset += len;
                } else {
                    break;
                }
            }

            // Delete pre-image file
            drop(preimage);
            fs::remove_file(preimage_path)?;
        }

        Ok(())
    }

    /// Write a page to the specified file (0 = data, 1+ = index)
    fn write_page_raw(&mut self, source: u8, page_number: u32, data: &[u8]) -> io::Result<()> {
        let offset = (page_number as u64) * (self.page_size as u64);

        if source == 0 {
            if let Some(ref mut f) = self.data_file {
                f.seek(SeekFrom::Start(offset))?;
                f.write_all(data)?;
            }
        } else {
            let index = (source - 1) as usize;
            if let Some(Some(ref mut f)) = self.index_files.get_mut(index) {
                f.seek(SeekFrom::Start(offset))?;
                f.write_all(data)?;
            }
        }

        Ok(())
    }

    /// Read a data page
    pub fn read_data_page(&mut self, page_number: u32) -> io::Result<Vec<u8>> {
        let offset = (page_number as u64) * (self.page_size as u64);
        let mut buf = vec![0u8; self.page_size as usize];

        if let Some(ref mut f) = self.data_file {
            f.seek(SeekFrom::Start(offset))?;
            f.read_exact(&mut buf)?;
        }

        Ok(buf)
    }

    /// Write a data page (with pre-imaging if transaction active)
    pub fn write_data_page(&mut self, page_number: u32, data: &[u8]) -> io::Result<()> {
        // Save pre-image if transaction active
        if self.preimage_file.is_some() {
            let original = self.read_data_page(page_number)?;
            self.save_preimage(0, page_number, &original)?;
        }

        let offset = (page_number as u64) * (self.page_size as u64);
        if let Some(ref mut f) = self.data_file {
            f.seek(SeekFrom::Start(offset))?;
            f.write_all(data)?;
        }

        Ok(())
    }

    /// Read an index page
    pub fn read_index_page(&mut self, key_number: usize, page_number: u32) -> io::Result<Vec<u8>> {
        let offset = (page_number as u64) * (self.page_size as u64);
        let mut buf = vec![0u8; self.page_size as usize];

        if let Some(Some(ref mut f)) = self.index_files.get_mut(key_number) {
            f.seek(SeekFrom::Start(offset))?;
            f.read_exact(&mut buf)?;
        }

        Ok(buf)
    }

    /// Write an index page (with pre-imaging if transaction active)
    pub fn write_index_page(&mut self, key_number: usize, page_number: u32, data: &[u8]) -> io::Result<()> {
        // Save pre-image if transaction active
        if self.preimage_file.is_some() {
            let original = self.read_index_page(key_number, page_number)?;
            self.save_preimage((key_number + 1) as u8, page_number, &original)?;
        }

        let offset = (page_number as u64) * (self.page_size as u64);
        if let Some(Some(ref mut f)) = self.index_files.get_mut(key_number) {
            f.seek(SeekFrom::Start(offset))?;
            f.write_all(data)?;
        }

        Ok(())
    }

    /// Update FCR and write to disk
    pub fn update_fcr(&mut self) -> io::Result<()> {
        if let Some(ref fcr) = self.fcr {
            let fcr_bytes = fcr.to_bytes();
            if let Some(ref mut f) = self.data_file {
                f.seek(SeekFrom::Start(0))?;
                f.write_all(&fcr_bytes)?;
            }
        }
        Ok(())
    }

    /// Update index header and write to disk
    pub fn update_index_header(&mut self, key_number: usize) -> io::Result<()> {
        if let Some(Some(ref header)) = self.index_headers.get(key_number) {
            let header_bytes = header.to_bytes(self.page_size);
            if let Some(Some(ref mut f)) = self.index_files.get_mut(key_number) {
                f.seek(SeekFrom::Start(0))?;
                f.write_all(&header_bytes)?;
            }
        }
        Ok(())
    }

    /// Close all files
    pub fn close(&mut self) -> io::Result<()> {
        // Sync and close data file
        if let Some(ref mut f) = self.data_file {
            f.sync_all()?;
        }
        self.data_file = None;

        // Sync and close index files
        for index_file in &mut self.index_files {
            if let Some(ref mut f) = index_file {
                f.sync_all()?;
            }
            *index_file = None;
        }

        // If there's an active transaction, abort it
        if self.preimage_file.is_some() {
            self.abort_transaction()?;
        }

        Ok(())
    }
}

impl Drop for BtrieveFileSet {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
