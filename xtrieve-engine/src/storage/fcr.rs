//! File Control Record (FCR) - Btrieve file header
//!
//! The FCR is always stored in page 0 and contains metadata about the file:
//! - Record length and page size
//! - Number of records
//! - Key specifications
//! - File flags

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor};

use super::key::KeySpec;

/// Btrieve file signature bytes (identifies a valid Btrieve file)
pub const BTRIEVE_SIGNATURE: [u8; 4] = [0x00, 0x00, 0x00, 0x00]; // Version 5.x has specific pattern

bitflags::bitflags! {
    /// File-level flags stored in FCR
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FileFlags: u16 {
        /// Variable-length records allowed
        const VARIABLE_LENGTH = 0x0001;
        /// Blank truncation enabled
        const BLANK_TRUNCATION = 0x0002;
        /// Pre-image logging enabled
        const PREIMAGE = 0x0004;
        /// Data compression enabled
        const COMPRESSED = 0x0008;
        /// Key-only file (no data, just keys)
        const KEY_ONLY = 0x0010;
        /// 10% free space allocation
        const FREE_SPACE_10 = 0x0040;
        /// 20% free space allocation
        const FREE_SPACE_20 = 0x0080;
        /// 30% free space allocation
        const FREE_SPACE_30 = 0x00C0;
    }
}

/// File Control Record - header of a Btrieve file
#[derive(Debug, Clone)]
pub struct FileControlRecord {
    /// Fixed record length in bytes
    pub record_length: u16,
    /// Page size (512, 1024, 2048, or 4096)
    pub page_size: u16,
    /// Number of keys (indexes) defined
    pub num_keys: u16,
    /// Total number of records in file
    pub num_records: u32,
    /// File flags
    pub flags: FileFlags,
    /// Number of pages currently allocated
    pub num_pages: u32,
    /// Number of unused pages in free list
    pub unused_pages: u16,
    /// Key specifications
    pub keys: Vec<KeySpec>,
    /// First data page number
    pub first_data_page: u32,
    /// Last data page number
    pub last_data_page: u32,
    /// First free page number
    pub first_free_page: u32,
    /// Root page for each index
    pub index_roots: Vec<u32>,
    /// Pre-image file name (if enabled)
    pub preimage_file: Option<String>,
    /// Next auto-increment value per key
    pub autoincrement_values: Vec<u32>,
}

impl FileControlRecord {
    /// Minimum FCR size (without keys)
    pub const BASE_SIZE: usize = 64;

    /// Maximum number of keys
    pub const MAX_KEYS: usize = 24;

    /// Parse FCR from page 0 data
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < Self::BASE_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "FCR data too short",
            ));
        }

        let mut cursor = Cursor::new(data);

        // Skip page header (12 bytes)
        cursor.set_position(12);

        // Read FCR fields
        let record_length = cursor.read_u16::<LittleEndian>()?;
        let page_size = cursor.read_u16::<LittleEndian>()?;
        let num_keys = cursor.read_u16::<LittleEndian>()?;
        let num_records = cursor.read_u32::<LittleEndian>()?;
        let raw_flags = cursor.read_u16::<LittleEndian>()?;
        let flags = FileFlags::from_bits_truncate(raw_flags);
        let _reserved1 = cursor.read_u16::<LittleEndian>()?;
        let num_pages = cursor.read_u32::<LittleEndian>()?;
        let unused_pages = cursor.read_u16::<LittleEndian>()?;
        let _reserved2 = cursor.read_u16::<LittleEndian>()?;

        // Read page pointers
        let first_data_page = cursor.read_u32::<LittleEndian>()?;
        let last_data_page = cursor.read_u32::<LittleEndian>()?;
        let first_free_page = cursor.read_u32::<LittleEndian>()?;

        // Read key specifications
        let key_offset = 64usize; // Keys start after base FCR
        let mut keys = Vec::with_capacity(num_keys as usize);
        let mut index_roots = Vec::with_capacity(num_keys as usize);
        let mut autoincrement_values = Vec::with_capacity(num_keys as usize);

        for i in 0..num_keys as usize {
            let spec_start = key_offset + (i * 24); // Each key spec is 24 bytes in FCR
            if spec_start + 24 > data.len() {
                break;
            }

            // Parse key specification (first 16 bytes)
            let key_spec = KeySpec::from_bytes(&data[spec_start..spec_start + 16])?;
            keys.push(key_spec);

            // Index root page (4 bytes after key spec)
            let root_offset = spec_start + 16;
            if root_offset + 4 <= data.len() {
                let root = Cursor::new(&data[root_offset..])
                    .read_u32::<LittleEndian>()
                    .unwrap_or(0);
                index_roots.push(root);
            } else {
                index_roots.push(0);
            }

            // Autoincrement value (4 bytes after root)
            let auto_offset = spec_start + 20;
            if auto_offset + 4 <= data.len() {
                let auto_val = Cursor::new(&data[auto_offset..])
                    .read_u32::<LittleEndian>()
                    .unwrap_or(0);
                autoincrement_values.push(auto_val);
            } else {
                autoincrement_values.push(0);
            }
        }

        Ok(FileControlRecord {
            record_length,
            page_size,
            num_keys,
            num_records,
            flags,
            num_pages,
            unused_pages,
            keys,
            first_data_page,
            last_data_page,
            first_free_page,
            index_roots,
            preimage_file: None,
            autoincrement_values,
        })
    }

    /// Serialize FCR to bytes for writing to page 0
    pub fn to_bytes(&self) -> Vec<u8> {
        let key_area_size = self.num_keys as usize * 24;
        let total_size = Self::BASE_SIZE + key_area_size;
        let mut buf = vec![0u8; total_size.max(self.page_size as usize)];

        // Page header (FCR type)
        buf[0] = 0x00; // FCR page type

        // FCR fields at offset 12
        let mut cursor = Cursor::new(&mut buf[12..]);
        cursor.write_u16::<LittleEndian>(self.record_length).unwrap();
        cursor.write_u16::<LittleEndian>(self.page_size).unwrap();
        cursor.write_u16::<LittleEndian>(self.num_keys).unwrap();
        cursor.write_u32::<LittleEndian>(self.num_records).unwrap();
        cursor.write_u16::<LittleEndian>(self.flags.bits()).unwrap();
        cursor.write_u16::<LittleEndian>(0).unwrap(); // reserved
        cursor.write_u32::<LittleEndian>(self.num_pages).unwrap();
        cursor.write_u16::<LittleEndian>(self.unused_pages).unwrap();
        cursor.write_u16::<LittleEndian>(0).unwrap(); // reserved
        cursor.write_u32::<LittleEndian>(self.first_data_page).unwrap();
        cursor.write_u32::<LittleEndian>(self.last_data_page).unwrap();
        cursor.write_u32::<LittleEndian>(self.first_free_page).unwrap();

        // Key specifications
        for (i, key) in self.keys.iter().enumerate() {
            let spec_start = 64 + (i * 24);
            let key_bytes = key.to_bytes();
            buf[spec_start..spec_start + key_bytes.len()].copy_from_slice(&key_bytes);

            // Index root
            let root_offset = spec_start + 16;
            let root = self.index_roots.get(i).copied().unwrap_or(0);
            (&mut buf[root_offset..]).write_u32::<LittleEndian>(root).unwrap();

            // Autoincrement value
            let auto_offset = spec_start + 20;
            let auto_val = self.autoincrement_values.get(i).copied().unwrap_or(0);
            (&mut buf[auto_offset..]).write_u32::<LittleEndian>(auto_val).unwrap();
        }

        buf
    }

    /// Check if file uses variable-length records
    pub fn is_variable_length(&self) -> bool {
        self.flags.contains(FileFlags::VARIABLE_LENGTH)
    }

    /// Check if pre-image logging is enabled
    pub fn has_preimage(&self) -> bool {
        self.flags.contains(FileFlags::PREIMAGE)
    }

    /// Get the free space threshold percentage
    pub fn free_space_threshold(&self) -> u8 {
        let bits = self.flags.bits() & 0x00C0;
        match bits {
            0x0040 => 10,
            0x0080 => 20,
            0x00C0 => 30,
            _ => 5, // Default 5%
        }
    }

    /// Create a new FCR with default settings
    pub fn new(record_length: u16, page_size: u16, keys: Vec<KeySpec>) -> Self {
        let num_keys = keys.len() as u16;
        let index_roots = vec![0; keys.len()];
        let autoincrement_values = vec![0; keys.len()];

        FileControlRecord {
            record_length,
            page_size,
            num_keys,
            num_records: 0,
            flags: FileFlags::empty(),
            num_pages: 1, // Just FCR page initially
            unused_pages: 0,
            keys,
            first_data_page: 0,
            last_data_page: 0,
            first_free_page: 0,
            index_roots,
            preimage_file: None,
            autoincrement_values,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::key::{KeyFlags, KeyType};

    #[test]
    fn test_fcr_roundtrip() {
        let key = KeySpec {
            position: 0,
            length: 10,
            flags: KeyFlags::DUPLICATES,
            key_type: KeyType::String,
            null_value: 0,
            acs_number: 0,
            unique_count: 0,
        };

        let fcr = FileControlRecord::new(100, 4096, vec![key]);
        let bytes = fcr.to_bytes();
        let parsed = FileControlRecord::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.record_length, 100);
        assert_eq!(parsed.page_size, 4096);
        assert_eq!(parsed.num_keys, 1);
        assert_eq!(parsed.keys[0].position, 0);
        assert_eq!(parsed.keys[0].length, 10);
    }

    #[test]
    fn test_file_flags() {
        let flags = FileFlags::VARIABLE_LENGTH | FileFlags::PREIMAGE;
        assert!(flags.contains(FileFlags::VARIABLE_LENGTH));
        assert!(flags.contains(FileFlags::PREIMAGE));
        assert!(!flags.contains(FileFlags::COMPRESSED));
    }
}
