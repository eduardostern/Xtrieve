//! Record management for Btrieve data pages
//!
//! Records are stored in data pages. Each data page has a slot directory
//! that tracks the position and status of records within the page.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor};

/// Physical address of a record (page number + slot)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordAddress {
    /// Page number containing the record
    pub page: u32,
    /// Slot index within the page
    pub slot: u16,
}

impl RecordAddress {
    /// Create a new record address
    pub fn new(page: u32, slot: u16) -> Self {
        RecordAddress { page, slot }
    }

    /// Pack into a 6-byte representation
    pub fn to_bytes(&self) -> [u8; 6] {
        let mut buf = [0u8; 6];
        (&mut buf[0..4]).write_u32::<LittleEndian>(self.page).unwrap();
        (&mut buf[4..6]).write_u16::<LittleEndian>(self.slot).unwrap();
        buf
    }

    /// Unpack from a 6-byte representation
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < 6 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Record address too short",
            ));
        }
        let page = Cursor::new(&data[0..4]).read_u32::<LittleEndian>()?;
        let slot = Cursor::new(&data[4..6]).read_u16::<LittleEndian>()?;
        Ok(RecordAddress { page, slot })
    }

    /// Convert to a 4-byte position (as used by Get Position operation)
    /// This uses the legacy Btrieve format: high 20 bits = page, low 12 bits = offset
    pub fn to_position(&self, page_size: u16) -> u32 {
        // Calculate byte offset within page
        // Simplified: slot * estimated slot size
        let offset = (self.slot as u32) * 4; // Rough estimate
        (self.page << 12) | (offset & 0xFFF)
    }

    /// Convert from a 4-byte position
    pub fn from_position(position: u32) -> Self {
        let page = position >> 12;
        let slot = ((position & 0xFFF) / 4) as u16;
        RecordAddress { page, slot }
    }
}

/// A record with its data and metadata
#[derive(Debug, Clone)]
pub struct Record {
    /// Record address
    pub address: RecordAddress,
    /// Record data
    pub data: Vec<u8>,
    /// Whether this is a fragment (variable-length overflow)
    pub is_fragment: bool,
}

impl Record {
    /// Create a new record
    pub fn new(address: RecordAddress, data: Vec<u8>) -> Self {
        Record {
            address,
            data,
            is_fragment: false,
        }
    }

    /// Get record length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if record is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Slot entry in a data page's directory
#[derive(Debug, Clone, Copy)]
pub struct SlotEntry {
    /// Offset of record data from start of page
    pub offset: u16,
    /// Length of record data
    pub length: u16,
    /// Slot flags
    pub flags: u8,
}

impl SlotEntry {
    /// Size of a slot entry in bytes
    pub const SIZE: usize = 5;

    /// Slot is in use
    pub const FLAG_IN_USE: u8 = 0x01;
    /// Slot contains a fragment pointer
    pub const FLAG_FRAGMENT: u8 = 0x02;
    /// Slot is deleted (tombstone)
    pub const FLAG_DELETED: u8 = 0x04;

    /// Read a slot entry from bytes
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < Self::SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Slot entry too short",
            ));
        }
        let mut cursor = Cursor::new(data);
        let offset = cursor.read_u16::<LittleEndian>()?;
        let length = cursor.read_u16::<LittleEndian>()?;
        let flags = cursor.read_u8()?;
        Ok(SlotEntry { offset, length, flags })
    }

    /// Write slot entry to bytes
    pub fn to_bytes(&self) -> [u8; 5] {
        let mut buf = [0u8; 5];
        (&mut buf[0..2]).write_u16::<LittleEndian>(self.offset).unwrap();
        (&mut buf[2..4]).write_u16::<LittleEndian>(self.length).unwrap();
        buf[4] = self.flags;
        buf
    }

    /// Check if slot is in use
    pub fn is_in_use(&self) -> bool {
        (self.flags & Self::FLAG_IN_USE) != 0
    }

    /// Check if slot contains a fragment
    pub fn is_fragment(&self) -> bool {
        (self.flags & Self::FLAG_FRAGMENT) != 0
    }

    /// Check if slot is deleted
    pub fn is_deleted(&self) -> bool {
        (self.flags & Self::FLAG_DELETED) != 0
    }
}

/// Data page structure
#[derive(Debug, Clone)]
pub struct DataPage {
    /// Page number
    pub page_number: u32,
    /// Page size
    pub page_size: u16,
    /// Next data page in chain
    pub next_page: u32,
    /// Previous data page in chain
    pub prev_page: u32,
    /// Number of slots
    pub slot_count: u16,
    /// Free space in page
    pub free_space: u16,
    /// Slot directory (at end of page, grows backward)
    pub slots: Vec<SlotEntry>,
    /// Raw page data
    data: Vec<u8>,
}

impl DataPage {
    /// Header size for data pages
    pub const HEADER_SIZE: usize = 16;

    /// Parse a data page from raw bytes
    pub fn from_bytes(page_number: u32, data: Vec<u8>) -> io::Result<Self> {
        let page_size = data.len() as u16;

        if data.len() < Self::HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Data page too short",
            ));
        }

        let mut cursor = Cursor::new(&data);

        // Skip page type byte
        let _page_type = cursor.read_u8()?;
        let _reserved = cursor.read_u8()?;
        let slot_count = cursor.read_u16::<LittleEndian>()?;
        let next_page = cursor.read_u32::<LittleEndian>()?;
        let prev_page = cursor.read_u32::<LittleEndian>()?;
        let free_space = cursor.read_u16::<LittleEndian>()?;

        // Read slot directory from end of page
        let mut slots = Vec::with_capacity(slot_count as usize);
        let slot_dir_start = page_size as usize - (slot_count as usize * SlotEntry::SIZE);

        for i in 0..slot_count as usize {
            let slot_offset = slot_dir_start + (i * SlotEntry::SIZE);
            if slot_offset + SlotEntry::SIZE <= data.len() {
                let slot = SlotEntry::from_bytes(&data[slot_offset..])?;
                slots.push(slot);
            }
        }

        Ok(DataPage {
            page_number,
            page_size,
            next_page,
            prev_page,
            slot_count,
            free_space,
            slots,
            data,
        })
    }

    /// Get record data for a slot
    pub fn get_record(&self, slot: u16) -> Option<&[u8]> {
        let entry = self.slots.get(slot as usize)?;
        if !entry.is_in_use() || entry.is_deleted() {
            return None;
        }
        let start = entry.offset as usize;
        let end = start + entry.length as usize;
        if end <= self.data.len() {
            Some(&self.data[start..end])
        } else {
            None
        }
    }

    /// Find next valid slot after given slot
    pub fn next_slot(&self, slot: u16) -> Option<u16> {
        for i in (slot + 1)..self.slot_count {
            if let Some(entry) = self.slots.get(i as usize) {
                if entry.is_in_use() && !entry.is_deleted() {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Find previous valid slot before given slot
    pub fn prev_slot(&self, slot: u16) -> Option<u16> {
        if slot == 0 {
            return None;
        }
        for i in (0..slot).rev() {
            if let Some(entry) = self.slots.get(i as usize) {
                if entry.is_in_use() && !entry.is_deleted() {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Find first valid slot
    pub fn first_slot(&self) -> Option<u16> {
        for i in 0..self.slot_count {
            if let Some(entry) = self.slots.get(i as usize) {
                if entry.is_in_use() && !entry.is_deleted() {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Find last valid slot
    pub fn last_slot(&self) -> Option<u16> {
        for i in (0..self.slot_count).rev() {
            if let Some(entry) = self.slots.get(i as usize) {
                if entry.is_in_use() && !entry.is_deleted() {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Count valid records in page
    pub fn record_count(&self) -> u16 {
        self.slots
            .iter()
            .filter(|s| s.is_in_use() && !s.is_deleted())
            .count() as u16
    }

    /// Calculate usable space for new records
    pub fn usable_space(&self) -> u16 {
        self.free_space.saturating_sub(SlotEntry::SIZE as u16)
    }

    /// Check if a record of given length can fit
    pub fn can_fit(&self, record_length: u16) -> bool {
        self.usable_space() >= record_length
    }

    /// Serialize page back to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.data.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_address_roundtrip() {
        let addr = RecordAddress::new(12345, 67);
        let bytes = addr.to_bytes();
        let parsed = RecordAddress::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.page, 12345);
        assert_eq!(parsed.slot, 67);
    }

    #[test]
    fn test_slot_entry_roundtrip() {
        let slot = SlotEntry {
            offset: 100,
            length: 50,
            flags: SlotEntry::FLAG_IN_USE,
        };
        let bytes = slot.to_bytes();
        let parsed = SlotEntry::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.offset, 100);
        assert_eq!(parsed.length, 50);
        assert!(parsed.is_in_use());
        assert!(!parsed.is_deleted());
    }
}
