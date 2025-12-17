//! File Control Record (FCR) - Btrieve 5.1 file header
//!
//! The FCR is always stored in page 0 and contains metadata about the file:
//! - Record length and page size
//! - Number of records
//! - Key specifications
//! - File flags
//!
//! Layout based on real DOS Btrieve 5.1 files:
//! - Offset 0x08: page_size (u16)
//! - Offset 0x14: num_keys (u16)
//! - Offset 0x16: record_length (u16)
//! - Offset 0x1C: num_records (u32)
//! - Offset 0x20: num_pages (u32)
//! - Offset 0x24: first_data_page (u32)
//! - Key specs at offset 0x110 (16 bytes each)

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor, Write};

use super::key::KeySpec;

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

/// File Control Record - header of a Btrieve 5.1 file
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

    /// Key area offset in Btrieve 5.1 FCR
    const KEY_AREA_OFFSET: usize = 0x110;

    /// Parse FCR from page 0 data (Btrieve 5.1 format)
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 0x30 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "FCR data too short",
            ));
        }

        // Parse Btrieve 5.1 FCR fields
        let page_size = u16::from_le_bytes([data[0x08], data[0x09]]);
        let num_keys = u16::from_le_bytes([data[0x14], data[0x15]]);
        let record_length = u16::from_le_bytes([data[0x16], data[0x17]]);
        let num_records = u32::from_le_bytes([data[0x1C], data[0x1D], data[0x1E], data[0x1F]]);
        let num_pages = u32::from_le_bytes([data[0x20], data[0x21], data[0x22], data[0x23]]);
        let first_data_page = u32::from_le_bytes([data[0x24], data[0x25], data[0x26], data[0x27]]);

        // Parse key specifications (start at offset 0x110 in Btrieve 5.1)
        let mut keys = Vec::with_capacity(num_keys as usize);
        let mut index_roots = Vec::with_capacity(num_keys as usize);
        let mut autoincrement_values = Vec::with_capacity(num_keys as usize);

        for i in 0..num_keys as usize {
            // Key spec layout in Btrieve 5.1 FCR (at KEY_AREA_OFFSET + i*16):
            // Bytes 0-7: unknown/reserved
            // Byte 8-9: key_position (u16, 1-based)
            // Byte 10-11: key_length (u16)
            // Byte 12-13: key_flags (u16)
            // Byte 14-15: unknown
            let spec_start = Self::KEY_AREA_OFFSET + (i * 16);
            if spec_start + 16 > data.len() {
                break;
            }

            let key_position = u16::from_le_bytes([data[spec_start + 8], data[spec_start + 9]]);
            let key_length = u16::from_le_bytes([data[spec_start + 10], data[spec_start + 11]]);
            let raw_flags = u16::from_le_bytes([data[spec_start + 12], data[spec_start + 13]]);

            // Convert 1-based position to 0-based
            let position = if key_position > 0 {
                key_position - 1
            } else {
                0
            };

            // Convert Btrieve 5.1 flags to our KeyFlags
            let mut flags = super::key::KeyFlags::empty();
            if (raw_flags & 0x0001) != 0 {
                flags |= super::key::KeyFlags::DUPLICATES;
            }
            if (raw_flags & 0x0002) != 0 {
                flags |= super::key::KeyFlags::MODIFIABLE;
            }

            let key_spec = KeySpec {
                position,
                length: key_length,
                flags,
                key_type: super::key::KeyType::UnsignedBinary,
                null_value: 0,
                acs_number: 0,
                unique_count: 0,
            };

            keys.push(key_spec);
            index_roots.push(1); // Index root is typically page 1 for Btrieve 5.1
            autoincrement_values.push(0);
        }

        Ok(FileControlRecord {
            record_length,
            page_size,
            num_keys,
            num_records,
            flags: FileFlags::empty(),
            num_pages,
            unused_pages: 0,
            keys,
            first_data_page,
            last_data_page: first_data_page,
            first_free_page: 0,
            index_roots,
            preimage_file: None,
            autoincrement_values,
        })
    }

    /// Serialize FCR to bytes for writing to page 0 (Btrieve 5.1 format)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; self.page_size as usize];

        // Write Btrieve 5.1 FCR header
        // Offset 0x04: version (10 for Btrieve 5.1)
        buf[0x04] = 0x0A;
        buf[0x05] = 0x00;

        // Offset 0x08: page_size
        buf[0x08..0x0A].copy_from_slice(&self.page_size.to_le_bytes());

        // Offset 0x14: num_keys
        buf[0x14..0x16].copy_from_slice(&self.num_keys.to_le_bytes());

        // Offset 0x16: record_length
        buf[0x16..0x18].copy_from_slice(&self.record_length.to_le_bytes());

        // Offset 0x1C: num_records
        buf[0x1C..0x20].copy_from_slice(&self.num_records.to_le_bytes());

        // Offset 0x20: num_pages
        buf[0x20..0x24].copy_from_slice(&self.num_pages.to_le_bytes());

        // Offset 0x24: first_data_page
        buf[0x24..0x28].copy_from_slice(&self.first_data_page.to_le_bytes());

        // Write key specifications at offset 0x110
        for (i, key) in self.keys.iter().enumerate() {
            let spec_start = Self::KEY_AREA_OFFSET + (i * 16);
            if spec_start + 16 > buf.len() {
                break;
            }

            // Key position (1-based)
            let position = key.position + 1;
            buf[spec_start + 8..spec_start + 10].copy_from_slice(&position.to_le_bytes());

            // Key length
            buf[spec_start + 10..spec_start + 12].copy_from_slice(&key.length.to_le_bytes());

            // Key flags
            let mut raw_flags: u16 = 0;
            if key.flags.contains(super::key::KeyFlags::DUPLICATES) {
                raw_flags |= 0x0001;
            }
            if key.flags.contains(super::key::KeyFlags::MODIFIABLE) {
                raw_flags |= 0x0002;
            }
            buf[spec_start + 12..spec_start + 14].copy_from_slice(&raw_flags.to_le_bytes());
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
