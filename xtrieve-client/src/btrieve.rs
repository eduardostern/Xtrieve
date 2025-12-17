//! High-level Btrieve-compatible API
//!
//! This module provides a familiar API for developers who have used Btrieve.

use crate::client::{XtrieveClient, BtrieveRequest};
use xtrieve_engine::{BtrieveError, BtrieveResult, StatusCode};

/// Operation codes (matching Btrieve)
pub mod op {
    pub const OPEN: u32 = 0;
    pub const CLOSE: u32 = 1;
    pub const INSERT: u32 = 2;
    pub const UPDATE: u32 = 3;
    pub const DELETE: u32 = 4;
    pub const GET_EQUAL: u32 = 5;
    pub const GET_NEXT: u32 = 6;
    pub const GET_PREVIOUS: u32 = 7;
    pub const GET_GREATER: u32 = 8;
    pub const GET_GE: u32 = 9;
    pub const GET_LESS: u32 = 10;
    pub const GET_LE: u32 = 11;
    pub const GET_FIRST: u32 = 12;
    pub const GET_LAST: u32 = 13;
    pub const CREATE: u32 = 14;
    pub const STAT: u32 = 15;
    pub const BEGIN_TRANSACTION: u32 = 19;
    pub const END_TRANSACTION: u32 = 20;
    pub const ABORT_TRANSACTION: u32 = 21;
    pub const GET_POSITION: u32 = 22;
    pub const GET_DIRECT: u32 = 23;
    pub const STEP_NEXT: u32 = 24;
    pub const STEP_FIRST: u32 = 33;
    pub const STEP_LAST: u32 = 34;
    pub const STEP_PREVIOUS: u32 = 35;
}

/// A record retrieved from a Btrieve file
#[derive(Debug, Clone)]
pub struct BtrieveRecord {
    /// Record data
    pub data: Vec<u8>,
    /// Current key value
    pub key: Vec<u8>,
}

/// Handle to an open Btrieve file
pub struct BtrieveFile {
    client: XtrieveClient,
    file_path: String,
    position_block: Vec<u8>,
    current_key: i32,
}

impl BtrieveFile {
    /// Open a Btrieve file
    pub fn open(mut client: XtrieveClient, path: &str, mode: i32) -> BtrieveResult<Self> {
        let request = BtrieveRequest {
            operation_code: op::OPEN,
            file_path: path.to_string(),
            open_mode: mode,
            ..Default::default()
        };

        let response = client.execute(request)?;

        Ok(BtrieveFile {
            client,
            file_path: path.to_string(),
            position_block: response.position_block,
            current_key: 0,
        })
    }

    /// Close the file
    pub fn close(mut self) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::CLOSE,
            position_block: self.position_block.clone(),
            file_path: self.file_path.clone(),
            ..Default::default()
        };

        self.client.execute(request)?;
        Ok(())
    }

    /// Insert a record
    pub fn insert(&mut self, data: &[u8]) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::INSERT,
            position_block: self.position_block.clone(),
            data_buffer: data.to_vec(),
            data_buffer_length: data.len() as u32,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;
        Ok(())
    }

    /// Update the current record
    pub fn update(&mut self, data: &[u8]) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::UPDATE,
            position_block: self.position_block.clone(),
            data_buffer: data.to_vec(),
            data_buffer_length: data.len() as u32,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;
        Ok(())
    }

    /// Delete the current record
    pub fn delete(&mut self) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::DELETE,
            position_block: self.position_block.clone(),
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;
        Ok(())
    }

    /// Set the current key number for subsequent operations
    pub fn set_key(&mut self, key_number: i32) {
        self.current_key = key_number;
    }

    /// Get Equal - find record by exact key match
    pub fn get_equal(&mut self, key: &[u8]) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::GET_EQUAL,
            position_block: self.position_block.clone(),
            key_buffer: key.to_vec(),
            key_buffer_length: key.len() as u32,
            key_number: self.current_key,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: response.key_buffer,
        })
    }

    /// Get Next - get next record in key order
    pub fn get_next(&mut self) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::GET_NEXT,
            position_block: self.position_block.clone(),
            key_number: self.current_key,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: response.key_buffer,
        })
    }

    /// Get Previous - get previous record in key order
    pub fn get_previous(&mut self) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::GET_PREVIOUS,
            position_block: self.position_block.clone(),
            key_number: self.current_key,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: response.key_buffer,
        })
    }

    /// Get First - get first record in key order
    pub fn get_first(&mut self) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::GET_FIRST,
            position_block: self.position_block.clone(),
            key_number: self.current_key,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: response.key_buffer,
        })
    }

    /// Get Last - get last record in key order
    pub fn get_last(&mut self) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::GET_LAST,
            position_block: self.position_block.clone(),
            key_number: self.current_key,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: response.key_buffer,
        })
    }

    /// Get Greater - get first record with key greater than given
    pub fn get_greater(&mut self, key: &[u8]) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::GET_GREATER,
            position_block: self.position_block.clone(),
            key_buffer: key.to_vec(),
            key_buffer_length: key.len() as u32,
            key_number: self.current_key,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: response.key_buffer,
        })
    }

    /// Get Greater or Equal
    pub fn get_greater_or_equal(&mut self, key: &[u8]) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::GET_GE,
            position_block: self.position_block.clone(),
            key_buffer: key.to_vec(),
            key_buffer_length: key.len() as u32,
            key_number: self.current_key,
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: response.key_buffer,
        })
    }

    /// Step First - get first record physically
    pub fn step_first(&mut self) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::STEP_FIRST,
            position_block: self.position_block.clone(),
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: Vec::new(),
        })
    }

    /// Step Next - get next record physically
    pub fn step_next(&mut self) -> BtrieveResult<BtrieveRecord> {
        let request = BtrieveRequest {
            operation_code: op::STEP_NEXT,
            position_block: self.position_block.clone(),
            ..Default::default()
        };

        let response = self.client.execute(request)?;
        self.position_block = response.position_block;

        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: Vec::new(),
        })
    }

    /// Get file statistics
    pub fn stat(&mut self) -> BtrieveResult<FileStatistics> {
        let request = BtrieveRequest {
            operation_code: op::STAT,
            position_block: self.position_block.clone(),
            file_path: self.file_path.clone(),
            ..Default::default()
        };

        let response = self.client.execute(request)?;

        // Parse statistics from data buffer
        let data = &response.data_buffer;
        if data.len() < 12 {
            return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
        }

        Ok(FileStatistics {
            record_length: u16::from_le_bytes([data[0], data[1]]),
            page_size: u16::from_le_bytes([data[2], data[3]]),
            num_keys: u16::from_le_bytes([data[4], data[5]]),
            num_records: u32::from_le_bytes([data[6], data[7], data[8], data[9]]),
        })
    }

    /// Begin transaction
    pub fn begin_transaction(&mut self) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::BEGIN_TRANSACTION,
            position_block: self.position_block.clone(),
            ..Default::default()
        };

        self.client.execute(request)?;
        Ok(())
    }

    /// End (commit) transaction
    pub fn end_transaction(&mut self) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::END_TRANSACTION,
            position_block: self.position_block.clone(),
            ..Default::default()
        };

        self.client.execute(request)?;
        Ok(())
    }

    /// Abort (rollback) transaction
    pub fn abort_transaction(&mut self) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::ABORT_TRANSACTION,
            position_block: self.position_block.clone(),
            ..Default::default()
        };

        self.client.execute(request)?;
        Ok(())
    }
}

/// File statistics returned by stat operation
#[derive(Debug, Clone)]
pub struct FileStatistics {
    pub record_length: u16,
    pub page_size: u16,
    pub num_keys: u16,
    pub num_records: u32,
}

/// Create a new Btrieve file
pub fn create_file(
    mut client: XtrieveClient,
    path: &str,
    record_length: u16,
    page_size: u16,
    keys: Vec<KeyDefinition>,
) -> BtrieveResult<()> {
    // Build data buffer with file spec
    let mut data = Vec::new();
    data.extend_from_slice(&record_length.to_le_bytes());
    data.extend_from_slice(&page_size.to_le_bytes());
    data.extend_from_slice(&(keys.len() as u16).to_le_bytes());
    data.extend_from_slice(&[0u8; 4]); // reserved/flags

    // Add key specifications
    for key in &keys {
        data.extend_from_slice(&key.position.to_le_bytes());
        data.extend_from_slice(&key.length.to_le_bytes());
        data.extend_from_slice(&key.flags.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]); // unique_count placeholder
        data.push(key.key_type);
        data.push(key.null_value);
        data.push(0); // acs_number
        data.push(0); // reserved
    }

    let request = BtrieveRequest {
        operation_code: op::CREATE,
        file_path: path.to_string(),
        data_buffer: data,
        data_buffer_length: 10 + (keys.len() as u32 * 16),
        ..Default::default()
    };

    client.execute(request)?;
    Ok(())
}

/// Key definition for creating files
#[derive(Debug, Clone)]
pub struct KeyDefinition {
    pub position: u16,
    pub length: u16,
    pub flags: u16,
    pub key_type: u8,
    pub null_value: u8,
}

impl KeyDefinition {
    /// Create a string key
    pub fn string(position: u16, length: u16, duplicates: bool, modifiable: bool) -> Self {
        let mut flags = 0u16;
        if duplicates { flags |= 0x0001; }
        if modifiable { flags |= 0x0002; }

        KeyDefinition {
            position,
            length,
            flags,
            key_type: 0, // String
            null_value: 0,
        }
    }

    /// Create an integer key
    pub fn integer(position: u16, length: u16, duplicates: bool, modifiable: bool) -> Self {
        let mut flags = 0u16;
        if duplicates { flags |= 0x0001; }
        if modifiable { flags |= 0x0002; }

        KeyDefinition {
            position,
            length,
            flags,
            key_type: 1, // Integer
            null_value: 0,
        }
    }

    /// Create an unsigned integer key
    pub fn unsigned(position: u16, length: u16, duplicates: bool, modifiable: bool) -> Self {
        let mut flags = 0u16;
        if duplicates { flags |= 0x0001; }
        if modifiable { flags |= 0x0002; }

        KeyDefinition {
            position,
            length,
            flags,
            key_type: 14, // Unsigned binary
            null_value: 0,
        }
    }

    /// Create an autoincrement key
    pub fn autoincrement(position: u16, length: u16) -> Self {
        KeyDefinition {
            position,
            length,
            flags: 0, // No duplicates, not modifiable
            key_type: 15, // Autoincrement
            null_value: 0,
        }
    }
}
