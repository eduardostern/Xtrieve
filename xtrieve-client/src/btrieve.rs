//! High-level Btrieve 5.10a-compatible API.

use crate::client::{BtrieveRequest, BtrieveResponse, XtrieveClient};
use xtrieve_engine::{status_codes, BtrieveError, BtrieveResult};

/// Genuine Btrieve 5.10a operation codes.
pub mod op {
    pub const OPEN: u16 = 0x00;
    pub const CLOSE: u16 = 0x01;
    pub const INSERT: u16 = 0x02;
    pub const UPDATE: u16 = 0x03;
    pub const DELETE: u16 = 0x04;
    pub const GET_EQUAL: u16 = 0x05;
    pub const GET_NEXT: u16 = 0x06;
    pub const GET_PREVIOUS: u16 = 0x07;
    pub const GET_GREATER: u16 = 0x08;
    pub const GET_GE: u16 = 0x09;
    pub const GET_LESS: u16 = 0x0A;
    pub const GET_LE: u16 = 0x0B;
    pub const GET_FIRST: u16 = 0x0C;
    pub const GET_LAST: u16 = 0x0D;
    pub const CREATE: u16 = 0x0E;
    pub const STAT: u16 = 0x0F;
    pub const EXTEND: u16 = 0x10;
    pub const SET_DIRECTORY: u16 = 0x11;
    pub const GET_DIRECTORY: u16 = 0x12;
    pub const BEGIN_TRANSACTION: u16 = 0x13;
    pub const END_TRANSACTION: u16 = 0x14;
    pub const ABORT_TRANSACTION: u16 = 0x15;
    pub const GET_POSITION: u16 = 0x16;
    pub const GET_DIRECT: u16 = 0x17;
    pub const STEP_NEXT: u16 = 0x18;
    pub const STOP: u16 = 0x19;
    pub const VERSION: u16 = 0x1A;
    pub const UNLOCK: u16 = 0x1B;
    pub const RESET: u16 = 0x1C;
    pub const SET_OWNER: u16 = 0x1D;
    pub const CLEAR_OWNER: u16 = 0x1E;
    pub const CREATE_SUPPLEMENTAL: u16 = 0x1F;
    pub const DROP_SUPPLEMENTAL: u16 = 0x20;
    pub const STEP_FIRST: u16 = 0x21;
    pub const STEP_LAST: u16 = 0x22;
    pub const STEP_PREVIOUS: u16 = 0x23;
}

/// A record retrieved from a Btrieve file.
#[derive(Debug, Clone)]
pub struct BtrieveRecord {
    pub data: Vec<u8>,
    /// Echo of the request key buffer; genuine Get does not produce a key.
    pub key: Vec<u8>,
}

pub struct BtrieveFile {
    client: XtrieveClient,
    position_block: Vec<u8>,
    current_key: i16,
    record_length: u16,
}

impl BtrieveFile {
    pub fn open(mut client: XtrieveClient, path: &str, mode: i16) -> BtrieveResult<Self> {
        let request = BtrieveRequest {
            operation_code: op::OPEN,
            key_number: mode,
            file_path: path.to_string(),
            position_block: vec![0x20; 128],
            ..Default::default()
        };
        let response = client.execute(request)?;
        ensure_success(&response)?;

        let mut file = BtrieveFile {
            client,
            position_block: response.position_block,
            current_key: 0,
            record_length: 0,
        };
        file.record_length = file.stat()?.record_length;
        Ok(file)
    }

    pub fn close(mut self) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::CLOSE,
            position_block: self.position_block,
            ..Default::default()
        };
        let response = self.client.execute(request)?;
        ensure_success(&response)
    }

    pub fn insert(&mut self, data: &[u8]) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::INSERT,
            position_block: self.position_block.clone(),
            data_buffer: data.to_vec(),
            data_buffer_length: data.len() as u32,
            ..Default::default()
        };
        let response = self.client.execute(request)?;
        ensure_success(&response)?;
        self.position_block = response.position_block;
        Ok(())
    }

    pub fn update(&mut self, data: &[u8]) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::UPDATE,
            position_block: self.position_block.clone(),
            data_buffer: data.to_vec(),
            data_buffer_length: data.len() as u32,
            ..Default::default()
        };
        let response = self.client.execute(request)?;
        ensure_success(&response)?;
        self.position_block = response.position_block;
        Ok(())
    }

    pub fn delete(&mut self) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: op::DELETE,
            position_block: self.position_block.clone(),
            ..Default::default()
        };
        let response = self.client.execute(request)?;
        ensure_success(&response)?;
        self.position_block = response.position_block;
        Ok(())
    }

    pub fn set_key(&mut self, key_number: i16) {
        self.current_key = key_number;
    }

    fn record_request(&self, operation: u16) -> BtrieveRequest {
        let capacity = self.record_length.max(1) as usize;
        BtrieveRequest {
            operation_code: operation,
            position_block: self.position_block.clone(),
            data_buffer: vec![0u8; capacity],
            data_buffer_length: capacity as u32,
            key_number: self.current_key,
            ..Default::default()
        }
    }

    fn read(&mut self, operation: u16, key: Option<&[u8]>) -> BtrieveResult<BtrieveRecord> {
        let mut request = self.record_request(operation);
        if let Some(key) = key {
            request.key_buffer = key.to_vec();
            request.key_buffer_length = key.len() as u32;
        }
        let response = self.client.execute(request)?;
        ensure_success(&response)?;
        self.position_block = response.position_block;
        Ok(BtrieveRecord {
            data: response.data_buffer,
            key: response.key_buffer,
        })
    }

    pub fn get_equal(&mut self, key: &[u8]) -> BtrieveResult<BtrieveRecord> {
        self.read(op::GET_EQUAL, Some(key))
    }

    pub fn get_next(&mut self) -> BtrieveResult<BtrieveRecord> {
        self.read(op::GET_NEXT, None)
    }

    pub fn get_previous(&mut self) -> BtrieveResult<BtrieveRecord> {
        self.read(op::GET_PREVIOUS, None)
    }

    pub fn get_first(&mut self) -> BtrieveResult<BtrieveRecord> {
        self.read(op::GET_FIRST, None)
    }

    pub fn get_last(&mut self) -> BtrieveResult<BtrieveRecord> {
        self.read(op::GET_LAST, None)
    }

    pub fn get_greater(&mut self, key: &[u8]) -> BtrieveResult<BtrieveRecord> {
        self.read(op::GET_GREATER, Some(key))
    }

    pub fn get_greater_or_equal(&mut self, key: &[u8]) -> BtrieveResult<BtrieveRecord> {
        self.read(op::GET_GE, Some(key))
    }

    pub fn step_first(&mut self) -> BtrieveResult<BtrieveRecord> {
        self.read(op::STEP_FIRST, None)
    }

    pub fn step_next(&mut self) -> BtrieveResult<BtrieveRecord> {
        self.read(op::STEP_NEXT, None)
    }

    pub fn stat(&mut self) -> BtrieveResult<FileStatistics> {
        let request = BtrieveRequest {
            operation_code: op::STAT,
            position_block: self.position_block.clone(),
            data_buffer: vec![0u8; 4096],
            data_buffer_length: 4096,
            ..Default::default()
        };
        let response = self.client.execute(request)?;
        ensure_success(&response)?;
        let data = &response.data_buffer;
        if data.len() < 10 {
            return Err(BtrieveError::Status(status_codes::DATA_BUFFER_LENGTH));
        }
        Ok(FileStatistics {
            record_length: u16::from_le_bytes([data[0], data[1]]),
            page_size: u16::from_le_bytes([data[2], data[3]]),
            num_keys: u16::from_le_bytes([data[4], data[5]]),
            num_records: u32::from_le_bytes([data[6], data[7], data[8], data[9]]),
        })
    }

    pub fn begin_transaction(&mut self) -> BtrieveResult<()> {
        self.transaction(op::BEGIN_TRANSACTION)
    }

    pub fn end_transaction(&mut self) -> BtrieveResult<()> {
        self.transaction(op::END_TRANSACTION)
    }

    pub fn abort_transaction(&mut self) -> BtrieveResult<()> {
        self.transaction(op::ABORT_TRANSACTION)
    }

    fn transaction(&mut self, operation: u16) -> BtrieveResult<()> {
        let request = BtrieveRequest {
            operation_code: operation,
            position_block: self.position_block.clone(),
            ..Default::default()
        };
        let response = self.client.execute(request)?;
        ensure_success(&response)?;
        self.position_block = response.position_block;
        Ok(())
    }
}

/// File statistics returned by Stat.
#[derive(Debug, Clone)]
pub struct FileStatistics {
    pub record_length: u16,
    pub page_size: u16,
    pub num_keys: u16,
    pub num_records: u32,
}

/// Create a genuine 5.10a file specification.
pub fn create_file(
    mut client: XtrieveClient,
    path: &str,
    record_length: u16,
    page_size: u16,
    keys: Vec<KeyDefinition>,
) -> BtrieveResult<()> {
    let mut data = vec![0u8; 16];
    data[0..2].copy_from_slice(&record_length.to_le_bytes());
    data[2..4].copy_from_slice(&page_size.to_le_bytes());
    data[4..6].copy_from_slice(&(keys.len() as u16).to_le_bytes());
    // Bytes 10..12 carry file flags, 14..16 carry preallocation.

    for key in &keys {
        let mut block = [0u8; 16];
        block[0..2].copy_from_slice(&key.position.to_le_bytes());
        block[2..4].copy_from_slice(&key.length.to_le_bytes());
        block[4..6].copy_from_slice(&key.flags.to_le_bytes());
        block[10] = key.key_type;
        block[11] = key.null_value;
        data.extend_from_slice(&block);
    }

    let request = BtrieveRequest {
        operation_code: op::CREATE,
        file_path: path.to_string(),
        data_buffer_length: data.len() as u32,
        data_buffer: data,
        position_block: vec![0x20; 128],
        ..Default::default()
    };
    let response = client.execute(request)?;
    ensure_success(&response)
}

fn ensure_success(response: &BtrieveResponse) -> BtrieveResult<()> {
    if response.status_code == status_codes::SUCCESS {
        Ok(())
    } else {
        Err(BtrieveError::Status(response.status_code))
    }
}

/// Key definition for Create.
#[derive(Debug, Clone)]
pub struct KeyDefinition {
    pub position: u16,
    pub length: u16,
    pub flags: u16,
    pub key_type: u8,
    pub null_value: u8,
}

impl KeyDefinition {
    pub fn string(position: u16, length: u16, duplicates: bool, modifiable: bool) -> Self {
        let mut flags = 0u16;
        if duplicates {
            flags |= 0x0001;
        }
        if modifiable {
            flags |= 0x0002;
        }
        KeyDefinition {
            position,
            length,
            flags,
            key_type: 0,
            null_value: 0,
        }
    }

    pub fn integer(position: u16, length: u16, duplicates: bool, modifiable: bool) -> Self {
        let mut flags = 0u16;
        if duplicates {
            flags |= 0x0001;
        }
        if modifiable {
            flags |= 0x0002;
        }
        KeyDefinition {
            position,
            length,
            flags,
            key_type: 1,
            null_value: 0,
        }
    }

    pub fn unsigned(position: u16, length: u16, duplicates: bool, modifiable: bool) -> Self {
        let mut flags = 0u16;
        if duplicates {
            flags |= 0x0001;
        }
        if modifiable {
            flags |= 0x0002;
        }
        KeyDefinition {
            position,
            length,
            flags,
            key_type: 14,
            null_value: 0,
        }
    }

    pub fn autoincrement(position: u16, length: u16) -> Self {
        KeyDefinition {
            position,
            length,
            flags: 0,
            key_type: 15,
            null_value: 0,
        }
    }
}
