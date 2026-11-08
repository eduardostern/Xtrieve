//! Adapter between Xtrieve's request envelope and the byte-exact engine.

use std::path::PathBuf;

use parking_lot::Mutex;

use crate::error::Status;

pub type SessionId = u64;

/// One operation as received by the daemon.
#[derive(Debug, Clone)]
pub struct OperationRequest {
    /// Base Btrieve operation code, without lock bias.
    pub operation: u16,
    /// 0, 100, 200, 300, or 400.
    pub lock_bias: u16,
    /// Native 128-byte caller/FCB state block.
    pub position_block: Vec<u8>,
    /// Record contents for writes, or a zero-filled capacity buffer for reads.
    pub data_buffer: Vec<u8>,
    /// Key value or filename.
    pub key_buffer: Vec<u8>,
    /// Key index, Open mode, or Create -1.
    pub key_number: i16,
    /// Resolved path for Open/Create, bypassing DOS filename parsing.
    pub path: Option<PathBuf>,
}

impl Default for OperationRequest {
    fn default() -> Self {
        Self {
            operation: 0,
            lock_bias: 0,
            position_block: vec![0u8; 128],
            data_buffer: Vec::new(),
            key_buffer: Vec::new(),
            key_number: 0,
            path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperationResponse {
    pub status: Status,
    pub position_block: Vec<u8>,
    pub data_buffer: Vec<u8>,
    pub key_buffer: Vec<u8>,
    pub data_length: u32,
    pub key_length: u16,
}

impl OperationResponse {
    pub fn success() -> Self {
        Self {
            status: btr_engine::status::SUCCESS,
            position_block: vec![0u8; 128],
            data_buffer: Vec::new(),
            key_buffer: Vec::new(),
            data_length: 0,
            key_length: 0,
        }
    }

    pub fn error(status: Status) -> Self {
        Self {
            status,
            position_block: vec![0u8; 128],
            data_buffer: Vec::new(),
            key_buffer: Vec::new(),
            data_length: 0,
            key_length: 0,
        }
    }
}

/// Shared server engine.  One btr-engine instance owns every session's
/// handles, cursors, locks, and transactions.
pub struct Engine {
    inner: Mutex<btr_engine::Engine>,
}

impl Engine {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let mut engine = btr_engine::Engine::new();
        engine.set_current_directory(data_dir);
        Self {
            inner: Mutex::new(engine),
        }
    }

    pub fn set_transaction_log(&self, path: impl Into<PathBuf>) {
        self.inner.lock().set_transaction_log(path);
    }

    pub fn execute(&self, session: SessionId, request: OperationRequest) -> OperationResponse {
        if request.position_block.len() != btr_engine::POSITION_BLOCK_SIZE {
            return OperationResponse::error(btr_engine::status::POSITION_BLOCK_LENGTH);
        }
        if request.data_buffer.len() > u16::MAX as usize {
            return OperationResponse::error(btr_engine::status::DATA_BUFFER_LENGTH);
        }
        let raw_op = match request.operation.checked_add(request.lock_bias) {
            Some(op) => op,
            None => return OperationResponse::error(btr_engine::status::INVALID_OPERATION),
        };

        let mut position = [0u8; btr_engine::POSITION_BLOCK_SIZE];
        position.copy_from_slice(&request.position_block);

        let mut data = request.data_buffer.clone();
        let mut data_len = data.len() as u16;
        let mut key = [0u8; 255];
        let key_input = request.key_buffer.len().min(key.len());
        key[..key_input].copy_from_slice(&request.key_buffer[..key_input]);

        // The DOS bridge historically transmitted key_num as an unsigned
        // byte, turning Open modes -1..-4 into 255..252.
        let key_number = if matches!(request.operation, 0x00 | 0x0E) && request.key_number >= 128 {
            (request.key_number as u8 as i8) as i16
        } else {
            request.key_number
        };

        let status = self.inner.lock().call_for_session_at_path(
            session,
            request.path.as_deref(),
            raw_op,
            &mut position,
            &mut data,
            &mut data_len,
            &mut key,
            key_number,
        );

        let key_length = match raw_op & 0xFF {
            // Get Directory writes a NUL-terminated path.
            0x12 => key.iter().position(|byte| *byte == 0).unwrap_or(key.len()),
            // Insert Extended writes the selected key; trim its NUL padding.
            0x28 => key
                .iter()
                .rposition(|byte| *byte != 0)
                .map(|index| index + 1)
                .unwrap_or(0),
            // Ordinary operations leave the caller's key buffer intact.
            _ => key_input,
        };

        let include_data =
            status == btr_engine::status::SUCCESS || matches!(raw_op & 0xFF, 0x24..=0x28);
        let data_length = if include_data { data_len as u32 } else { 0 };
        let data_buffer = if include_data {
            data[..data_len as usize].to_vec()
        } else {
            Vec::new()
        };

        OperationResponse {
            status,
            position_block: position.to_vec(),
            data_buffer,
            key_buffer: key[..key_length].to_vec(),
            data_length,
            key_length: key_length as u16,
        }
    }

    /// Abort the session's transaction and close every handle it owns.
    pub fn end_session(&self, session: SessionId) {
        let _ = self.inner.lock().close_session(session);
    }

    pub fn shutdown(&self) {
        self.inner.lock().shutdown();
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(".")
    }
}
