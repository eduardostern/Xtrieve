//! Record management for Btrieve data pages
//!
//! Records are stored in data pages. Each data page has a slot directory
//! that tracks the position and status of records within the page.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor, Write};

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
    /// With Btrieve 5.1 format, slot contains the absolute file offset
    pub fn to_position(&self, _page_size: u16) -> u32 {
        // In Btrieve 5.1 format, slot contains the file offset
        self.slot as u32
    }

    /// Convert from a 4-byte position
    /// With Btrieve 5.1 format, position is the absolute file offset
    pub fn from_position(position: u32) -> Self {
        // Store file offset in slot field (page=0 indicates file offset format)
        RecordAddress { page: 0, slot: position as u16 }
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
    /// First free slot index (0xFFFF = none) - head of free list
    pub first_free_slot: u16,
    /// Slot directory (at end of page, grows backward)
    pub slots: Vec<SlotEntry>,
    /// Raw page data
    data: Vec<u8>,
}

impl DataPage {
    /// Header size for data pages (includes first_free_slot at offset 16-17)
    pub const HEADER_SIZE: usize = 18;
    /// Value indicating no free slots in free list
    pub const NO_FREE_SLOT: u16 = 0xFFFF;

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
        let first_free_slot = cursor.read_u16::<LittleEndian>()?;

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
            first_free_slot,
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

    /// Create a new empty data page
    pub fn new(page_number: u32, page_size: u16) -> Self {
        let mut data = vec![0u8; page_size as usize];

        // Page header layout:
        // [0]     page_type
        // [1]     reserved
        // [2..4]  slot_count (u16)
        // [4..8]  next_page (u32)
        // [8..12] prev_page (u32)
        // [12..14] (unused in original, now part of header)
        // [14..16] free_space (u16)
        // [16..18] first_free_slot (u16) - head of free list
        data[0] = 0x02; // Data page type
        data[1] = 0x00; // Reserved
        // slot_count at [2..4] = 0
        // next_page at [4..8] = 0
        // prev_page at [8..12] = 0

        // Free space = page_size - header
        let free_space = page_size - Self::HEADER_SIZE as u16;
        data[14..16].copy_from_slice(&free_space.to_le_bytes());

        // First free slot = 0xFFFF (none)
        data[16..18].copy_from_slice(&Self::NO_FREE_SLOT.to_le_bytes());

        DataPage {
            page_number,
            page_size,
            next_page: 0,
            prev_page: 0,
            slot_count: 0,
            free_space,
            first_free_slot: Self::NO_FREE_SLOT,
            slots: Vec::new(),
            data,
        }
    }

    /// Insert a record into this page
    /// Returns the slot number, or None if record doesn't fit
    ///
    /// Btrieve behavior: Uses free list for O(1) lookup of deleted slots.
    /// First tries to reuse space from free list, only allocates new space if empty.
    pub fn insert_record(&mut self, record_data: &[u8]) -> Option<u16> {
        let record_len = record_data.len() as u16;

        // Btrieve: Check free list first (O(1) - just check head of list)
        if self.first_free_slot != Self::NO_FREE_SLOT {
            let free_idx = self.first_free_slot as usize;
            if let Some(slot) = self.slots.get_mut(free_idx) {
                if slot.is_deleted() && slot.length >= record_len {
                    let record_offset = slot.offset as usize;

                    // Read next free slot from the deleted record's data area
                    // (stored in first 2 bytes when deleted)
                    let next_free = if slot.length >= 2 {
                        u16::from_le_bytes([
                            self.data[record_offset],
                            self.data[record_offset + 1],
                        ])
                    } else {
                        Self::NO_FREE_SLOT
                    };

                    // Write record data at the deleted slot's location
                    self.data[record_offset..record_offset + record_len as usize]
                        .copy_from_slice(record_data);

                    // Clear any remaining space if new record is shorter
                    if record_len < slot.length {
                        let pad_start = record_offset + record_len as usize;
                        let pad_end = record_offset + slot.length as usize;
                        self.data[pad_start..pad_end].fill(0);
                    }

                    // Update slot entry: mark as in-use, clear deleted flag
                    slot.flags = SlotEntry::FLAG_IN_USE;

                    // Update slot in page data
                    let slot_offset = self.page_size as usize - ((free_idx + 1) * SlotEntry::SIZE);
                    let slot_bytes = slot.to_bytes();
                    self.data[slot_offset..slot_offset + SlotEntry::SIZE].copy_from_slice(&slot_bytes);

                    // Update free list head to next free slot
                    self.first_free_slot = next_free;
                    self.data[16..18].copy_from_slice(&self.first_free_slot.to_le_bytes());

                    return Some(free_idx as u16);
                }
            }
        }

        // No suitable free slot - allocate new space
        let needed_space = record_len + SlotEntry::SIZE as u16;

        if self.free_space < needed_space {
            return None;
        }

        // Find where to put the record data
        // Records grow from after header, slots grow backward from end
        let slot_dir_start = self.page_size as usize - ((self.slot_count + 1) as usize * SlotEntry::SIZE);
        let data_end = if self.slots.is_empty() {
            Self::HEADER_SIZE
        } else {
            // Find the end of last record (including deleted ones, since they still occupy space)
            self.slots.iter()
                .map(|s| s.offset as usize + s.length as usize)
                .max()
                .unwrap_or(Self::HEADER_SIZE)
        };

        // Check there's room
        if data_end + record_len as usize > slot_dir_start {
            return None;
        }

        // Write record data
        let record_offset = data_end;
        self.data[record_offset..record_offset + record_len as usize].copy_from_slice(record_data);

        // Create slot entry
        let slot = SlotEntry {
            offset: record_offset as u16,
            length: record_len,
            flags: SlotEntry::FLAG_IN_USE,
        };

        let slot_num = self.slot_count;
        self.slots.push(slot);
        self.slot_count += 1;

        // Write slot to slot directory
        let slot_offset = self.page_size as usize - (self.slot_count as usize * SlotEntry::SIZE);
        let slot_bytes = slot.to_bytes();
        self.data[slot_offset..slot_offset + SlotEntry::SIZE].copy_from_slice(&slot_bytes);

        // Update header
        self.free_space -= needed_space;
        self.data[2..4].copy_from_slice(&self.slot_count.to_le_bytes());
        self.data[14..16].copy_from_slice(&self.free_space.to_le_bytes());

        Some(slot_num)
    }

    /// Mark a record as deleted and add to free list
    /// Btrieve behavior: Deleted slots are linked in a free list for O(1) reuse
    pub fn delete_record(&mut self, slot: u16) -> bool {
        if let Some(entry) = self.slots.get_mut(slot as usize) {
            if entry.is_in_use() && !entry.is_deleted() {
                let record_offset = entry.offset as usize;

                // Store current free list head in the deleted record's data area
                // (first 2 bytes become "next free" pointer)
                if entry.length >= 2 {
                    self.data[record_offset..record_offset + 2]
                        .copy_from_slice(&self.first_free_slot.to_le_bytes());
                }

                // Mark slot as deleted
                entry.flags |= SlotEntry::FLAG_DELETED;

                // Update slot in page data
                let slot_offset = self.page_size as usize - ((slot as usize + 1) * SlotEntry::SIZE);
                self.data[slot_offset + 4] = entry.flags;

                // Prepend this slot to free list (new head)
                self.first_free_slot = slot;
                self.data[16..18].copy_from_slice(&self.first_free_slot.to_le_bytes());

                // Add space back to free_space counter
                self.free_space += entry.length;
                self.data[14..16].copy_from_slice(&self.free_space.to_le_bytes());

                return true;
            }
        }
        false
    }

    /// Update record in place (must be same length or smaller)
    pub fn update_record(&mut self, slot: u16, record_data: &[u8]) -> bool {
        if let Some(entry) = self.slots.get(slot as usize) {
            if entry.is_in_use() && !entry.is_deleted() {
                if record_data.len() <= entry.length as usize {
                    let start = entry.offset as usize;
                    self.data[start..start + record_data.len()].copy_from_slice(record_data);
                    // Pad with zeros if new record is shorter
                    if record_data.len() < entry.length as usize {
                        let end = start + entry.length as usize;
                        self.data[start + record_data.len()..end].fill(0);
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Set next page pointer
    pub fn set_next_page(&mut self, page: u32) {
        self.next_page = page;
        self.data[4..8].copy_from_slice(&page.to_le_bytes());
    }

    /// Set previous page pointer
    pub fn set_prev_page(&mut self, page: u32) {
        self.prev_page = page;
        self.data[8..12].copy_from_slice(&page.to_le_bytes());
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
