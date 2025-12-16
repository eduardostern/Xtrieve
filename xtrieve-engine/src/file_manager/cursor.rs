//! Cursor (position block) management for Btrieve operations
//!
//! A cursor maintains the current position in a file, including:
//! - Current record address
//! - Current key value
//! - Current key number
//! - Navigation state

use std::path::PathBuf;

use crate::storage::record::RecordAddress;

/// Cursor state flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorState {
    /// Cursor is not positioned
    Unpositioned,
    /// Cursor is positioned on a valid record
    Positioned,
    /// Cursor is at end of file (after last record)
    AtEnd,
    /// Cursor is at beginning of file (before first record)
    AtBeginning,
    /// Record was deleted, cursor needs repositioning
    Deleted,
}

/// A cursor (position block) for navigating a Btrieve file
#[derive(Debug, Clone)]
pub struct Cursor {
    /// File path this cursor is associated with
    pub file_path: PathBuf,
    /// Current state
    pub state: CursorState,
    /// Current record address
    pub record_address: Option<RecordAddress>,
    /// Current key number
    pub key_number: i32,
    /// Current key value
    pub key_value: Vec<u8>,
    /// Current record data (cached)
    pub record_data: Vec<u8>,
    /// Index position within leaf node
    pub leaf_index: usize,
    /// Current leaf page
    pub leaf_page: u32,
    /// Physical position (for step operations)
    pub physical_position: Option<RecordAddress>,
}

impl Cursor {
    /// Create a new unpositioned cursor
    pub fn new(file_path: PathBuf, key_number: i32) -> Self {
        Cursor {
            file_path,
            state: CursorState::Unpositioned,
            record_address: None,
            key_number,
            key_value: Vec::new(),
            record_data: Vec::new(),
            leaf_index: 0,
            leaf_page: 0,
            physical_position: None,
        }
    }

    /// Check if cursor is positioned on a valid record
    pub fn is_positioned(&self) -> bool {
        matches!(self.state, CursorState::Positioned)
    }

    /// Position cursor on a record
    pub fn position(
        &mut self,
        address: RecordAddress,
        key_value: Vec<u8>,
        record_data: Vec<u8>,
    ) {
        self.state = CursorState::Positioned;
        self.record_address = Some(address);
        self.key_value = key_value;
        self.record_data = record_data;
    }

    /// Position cursor with leaf info (for efficient Next/Prev)
    pub fn position_with_leaf(
        &mut self,
        address: RecordAddress,
        key_value: Vec<u8>,
        record_data: Vec<u8>,
        leaf_page: u32,
        leaf_index: usize,
    ) {
        self.position(address, key_value, record_data);
        self.leaf_page = leaf_page;
        self.leaf_index = leaf_index;
    }

    /// Mark cursor as at end of file
    pub fn set_at_end(&mut self) {
        self.state = CursorState::AtEnd;
        self.record_address = None;
    }

    /// Mark cursor as at beginning
    pub fn set_at_beginning(&mut self) {
        self.state = CursorState::AtBeginning;
        self.record_address = None;
    }

    /// Invalidate cursor (e.g., after delete)
    pub fn invalidate(&mut self) {
        self.state = CursorState::Deleted;
    }

    /// Reset cursor to unpositioned
    pub fn reset(&mut self) {
        self.state = CursorState::Unpositioned;
        self.record_address = None;
        self.key_value.clear();
        self.record_data.clear();
        self.leaf_index = 0;
        self.leaf_page = 0;
    }

    /// Change key number (invalidates position unless same key)
    pub fn set_key_number(&mut self, key_number: i32) {
        if key_number != self.key_number {
            self.reset();
            self.key_number = key_number;
        }
    }

    /// Get current record data
    pub fn current_record(&self) -> Option<&[u8]> {
        if self.is_positioned() {
            Some(&self.record_data)
        } else {
            None
        }
    }

    /// Get current key value
    pub fn current_key(&self) -> Option<&[u8]> {
        if self.is_positioned() {
            Some(&self.key_value)
        } else {
            None
        }
    }
}

/// Position block as transmitted over gRPC
/// This is a serialized form of the cursor state
#[derive(Debug, Clone, Default)]
pub struct PositionBlock {
    /// Raw 128-byte position block (Btrieve compatible)
    pub data: [u8; 128],
}

impl PositionBlock {
    /// Create empty position block
    pub fn new() -> Self {
        PositionBlock { data: [0; 128] }
    }

    /// Create from a cursor
    pub fn from_cursor(cursor: &Cursor) -> Self {
        let mut block = PositionBlock::new();

        // Store state
        block.data[0] = cursor.state as u8;

        // Store key number
        block.data[1..5].copy_from_slice(&(cursor.key_number as i32).to_le_bytes());

        // Store record address if positioned
        if let Some(addr) = cursor.record_address {
            block.data[5..9].copy_from_slice(&addr.page.to_le_bytes());
            block.data[9..11].copy_from_slice(&addr.slot.to_le_bytes());
        }

        // Store leaf position
        block.data[11..15].copy_from_slice(&cursor.leaf_page.to_le_bytes());
        block.data[15..19].copy_from_slice(&(cursor.leaf_index as u32).to_le_bytes());

        // Store key value (truncated if too long)
        let key_len = cursor.key_value.len().min(100);
        block.data[20] = key_len as u8;
        if key_len > 0 {
            block.data[21..21 + key_len].copy_from_slice(&cursor.key_value[..key_len]);
        }

        block
    }

    /// Restore cursor state from position block
    pub fn to_cursor(&self, file_path: PathBuf) -> Cursor {
        let state = match self.data[0] {
            1 => CursorState::Positioned,
            2 => CursorState::AtEnd,
            3 => CursorState::AtBeginning,
            4 => CursorState::Deleted,
            _ => CursorState::Unpositioned,
        };

        let key_number = i32::from_le_bytes([
            self.data[1],
            self.data[2],
            self.data[3],
            self.data[4],
        ]);

        let record_address = if state == CursorState::Positioned {
            let page = u32::from_le_bytes([
                self.data[5],
                self.data[6],
                self.data[7],
                self.data[8],
            ]);
            let slot = u16::from_le_bytes([self.data[9], self.data[10]]);
            Some(RecordAddress::new(page, slot))
        } else {
            None
        };

        let leaf_page = u32::from_le_bytes([
            self.data[11],
            self.data[12],
            self.data[13],
            self.data[14],
        ]);

        let leaf_index = u32::from_le_bytes([
            self.data[15],
            self.data[16],
            self.data[17],
            self.data[18],
        ]) as usize;

        let key_len = self.data[20] as usize;
        let key_value = if key_len > 0 {
            self.data[21..21 + key_len].to_vec()
        } else {
            Vec::new()
        };

        Cursor {
            file_path,
            state,
            record_address,
            key_number,
            key_value,
            record_data: Vec::new(), // Not stored in position block
            leaf_index,
            leaf_page,
            physical_position: None,
        }
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut block = PositionBlock::new();
        let len = data.len().min(128);
        block.data[..len].copy_from_slice(&data[..len]);
        block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_positioning() {
        let mut cursor = Cursor::new(PathBuf::from("test.dat"), 0);

        assert!(!cursor.is_positioned());

        let addr = RecordAddress::new(1, 0);
        cursor.position(addr, b"key".to_vec(), b"data".to_vec());

        assert!(cursor.is_positioned());
        assert_eq!(cursor.record_address, Some(addr));
        assert_eq!(cursor.current_key(), Some(b"key".as_slice()));
        assert_eq!(cursor.current_record(), Some(b"data".as_slice()));
    }

    #[test]
    fn test_position_block_roundtrip() {
        let mut cursor = Cursor::new(PathBuf::from("test.dat"), 2);
        let addr = RecordAddress::new(100, 5);
        cursor.position_with_leaf(
            addr,
            b"mykey".to_vec(),
            b"record data".to_vec(),
            50,
            3,
        );

        let block = PositionBlock::from_cursor(&cursor);
        let restored = block.to_cursor(PathBuf::from("test.dat"));

        assert!(restored.is_positioned());
        assert_eq!(restored.key_number, 2);
        assert_eq!(restored.record_address, Some(addr));
        assert_eq!(restored.leaf_page, 50);
        assert_eq!(restored.leaf_index, 3);
        assert_eq!(restored.key_value, b"mykey".to_vec());
    }
}
