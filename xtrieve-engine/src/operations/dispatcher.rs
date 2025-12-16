//! Operation dispatcher - routes Btrieve operation codes to handlers
//!
//! This is the main entry point for all Btrieve operations.

use std::path::PathBuf;
use std::sync::Arc;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::{
    cursor::{Cursor, PositionBlock},
    locking::{LockManager, LockType, SessionId},
    open_files::{OpenFileTable, OpenMode},
    page_cache::PageCache,
};
use crate::storage::fcr::FileControlRecord;
use crate::storage::key::KeySpec;

/// Btrieve operation codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum OperationCode {
    // File operations
    Open = 0,
    Close = 1,
    Create = 14,
    Stat = 15,
    Extend = 17,
    SetOwner = 29,
    ClearOwner = 30,

    // Record operations
    Insert = 2,
    Update = 3,
    Delete = 4,

    // Key-based retrieval
    GetEqual = 5,
    GetNext = 6,
    GetPrevious = 7,
    GetGreater = 8,
    GetGreaterOrEqual = 9,
    GetLessThan = 10,
    GetLessOrEqual = 11,
    GetFirst = 12,
    GetLast = 13,

    // Physical access
    StepNext = 24,
    StepFirst = 33,
    StepLast = 34,
    StepPrevious = 35,

    // Position operations
    GetPosition = 22,
    GetDirect = 23,
    GetByPercentage = 26,
    FindPercentage = 27,

    // Transaction operations
    BeginTransaction = 19,
    EndTransaction = 20,
    AbortTransaction = 21,

    // Index operations
    CreateSupplementalIndex = 31,
    DropSupplementalIndex = 32,

    // Extended operations
    GetNextExtended = 36,
    GetPreviousExtended = 37,
    StepNextExtended = 38,
    StepPreviousExtended = 39,
    InsertExtended = 40,
    GetKey = 50,

    // Utility operations
    Stop = 25,
    Reset = 28,
    Unlock = 53,
    Version = 54,

    // Unknown/invalid
    Unknown = 255,
}

impl OperationCode {
    pub fn from_raw(code: u32) -> Self {
        match code {
            0 => OperationCode::Open,
            1 => OperationCode::Close,
            2 => OperationCode::Insert,
            3 => OperationCode::Update,
            4 => OperationCode::Delete,
            5 => OperationCode::GetEqual,
            6 => OperationCode::GetNext,
            7 => OperationCode::GetPrevious,
            8 => OperationCode::GetGreater,
            9 => OperationCode::GetGreaterOrEqual,
            10 => OperationCode::GetLessThan,
            11 => OperationCode::GetLessOrEqual,
            12 => OperationCode::GetFirst,
            13 => OperationCode::GetLast,
            14 => OperationCode::Create,
            15 => OperationCode::Stat,
            17 => OperationCode::Extend,
            19 => OperationCode::BeginTransaction,
            20 => OperationCode::EndTransaction,
            21 => OperationCode::AbortTransaction,
            22 => OperationCode::GetPosition,
            23 => OperationCode::GetDirect,
            24 => OperationCode::StepNext,
            25 => OperationCode::Stop,
            26 => OperationCode::GetByPercentage,
            27 => OperationCode::FindPercentage,
            28 => OperationCode::Reset,
            29 => OperationCode::SetOwner,
            30 => OperationCode::ClearOwner,
            31 => OperationCode::CreateSupplementalIndex,
            32 => OperationCode::DropSupplementalIndex,
            33 => OperationCode::StepFirst,
            34 => OperationCode::StepLast,
            35 => OperationCode::StepPrevious,
            36 => OperationCode::GetNextExtended,
            37 => OperationCode::GetPreviousExtended,
            38 => OperationCode::StepNextExtended,
            39 => OperationCode::StepPreviousExtended,
            40 => OperationCode::InsertExtended,
            50 => OperationCode::GetKey,
            _ => OperationCode::Unknown,
        }
    }

    /// Check if this operation requires a positioned cursor
    pub fn requires_position(&self) -> bool {
        matches!(
            self,
            OperationCode::Update
                | OperationCode::Delete
                | OperationCode::GetNext
                | OperationCode::GetPrevious
                | OperationCode::StepNext
                | OperationCode::StepPrevious
                | OperationCode::GetPosition
        )
    }

    /// Check if this is a read operation
    pub fn is_read(&self) -> bool {
        matches!(
            self,
            OperationCode::GetEqual
                | OperationCode::GetNext
                | OperationCode::GetPrevious
                | OperationCode::GetGreater
                | OperationCode::GetGreaterOrEqual
                | OperationCode::GetLessThan
                | OperationCode::GetLessOrEqual
                | OperationCode::GetFirst
                | OperationCode::GetLast
                | OperationCode::StepNext
                | OperationCode::StepFirst
                | OperationCode::StepLast
                | OperationCode::StepPrevious
                | OperationCode::GetDirect
                | OperationCode::Stat
        )
    }

    /// Check if this is a write operation
    pub fn is_write(&self) -> bool {
        matches!(
            self,
            OperationCode::Insert | OperationCode::Update | OperationCode::Delete
        )
    }
}

/// Request structure for operations
#[derive(Debug, Clone)]
pub struct OperationRequest {
    pub operation: OperationCode,
    pub file_path: Option<String>,
    pub position_block: Vec<u8>,
    pub data_buffer: Vec<u8>,
    pub key_buffer: Vec<u8>,
    pub key_number: i32,
    pub data_length: u32,
    pub key_length: u32,
    pub open_mode: i32,
    pub lock_bias: i32,
}

impl Default for OperationRequest {
    fn default() -> Self {
        OperationRequest {
            operation: OperationCode::Unknown,
            file_path: None,
            position_block: Vec::new(),
            data_buffer: Vec::new(),
            key_buffer: Vec::new(),
            key_number: 0,
            data_length: 0,
            key_length: 0,
            open_mode: 0,
            lock_bias: 0,
        }
    }
}

/// Response structure for operations
#[derive(Debug, Clone)]
pub struct OperationResponse {
    pub status: StatusCode,
    pub position_block: Vec<u8>,
    pub data_buffer: Vec<u8>,
    pub key_buffer: Vec<u8>,
    pub data_length: u32,
    pub key_length: u32,
}

impl OperationResponse {
    pub fn success() -> Self {
        OperationResponse {
            status: StatusCode::Success,
            position_block: Vec::new(),
            data_buffer: Vec::new(),
            key_buffer: Vec::new(),
            data_length: 0,
            key_length: 0,
        }
    }

    pub fn error(status: StatusCode) -> Self {
        OperationResponse {
            status,
            position_block: Vec::new(),
            data_buffer: Vec::new(),
            key_buffer: Vec::new(),
            data_length: 0,
            key_length: 0,
        }
    }

    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data_length = data.len() as u32;
        self.data_buffer = data;
        self
    }

    pub fn with_key(mut self, key: Vec<u8>) -> Self {
        self.key_length = key.len() as u32;
        self.key_buffer = key;
        self
    }

    pub fn with_position(mut self, position: Vec<u8>) -> Self {
        self.position_block = position;
        self
    }
}

/// The Xtrieve engine - main coordinator for all operations
pub struct Engine {
    /// Open file table
    pub files: Arc<OpenFileTable>,
    /// Page cache
    pub cache: Arc<PageCache>,
    /// Lock manager
    pub locks: Arc<LockManager>,
}

impl Engine {
    /// Create a new engine instance
    pub fn new(cache_size: usize) -> Self {
        Engine {
            files: Arc::new(OpenFileTable::new()),
            cache: Arc::new(PageCache::new(cache_size)),
            locks: Arc::new(LockManager::default()),
        }
    }

    /// Execute a Btrieve operation
    pub fn execute(
        &self,
        session: SessionId,
        request: OperationRequest,
    ) -> OperationResponse {
        let result = match request.operation {
            OperationCode::Open => self.op_open(session, &request),
            OperationCode::Close => self.op_close(session, &request),
            OperationCode::Create => self.op_create(session, &request),
            OperationCode::Stat => self.op_stat(session, &request),
            OperationCode::Insert => self.op_insert(session, &request),
            OperationCode::Update => self.op_update(session, &request),
            OperationCode::Delete => self.op_delete(session, &request),
            OperationCode::GetEqual => self.op_get_equal(session, &request),
            OperationCode::GetNext => self.op_get_next(session, &request),
            OperationCode::GetPrevious => self.op_get_previous(session, &request),
            OperationCode::GetGreater => self.op_get_greater(session, &request),
            OperationCode::GetGreaterOrEqual => self.op_get_greater_or_equal(session, &request),
            OperationCode::GetLessThan => self.op_get_less_than(session, &request),
            OperationCode::GetLessOrEqual => self.op_get_less_or_equal(session, &request),
            OperationCode::GetFirst => self.op_get_first(session, &request),
            OperationCode::GetLast => self.op_get_last(session, &request),
            OperationCode::GetPosition => self.op_get_position(session, &request),
            OperationCode::GetDirect => self.op_get_direct(session, &request),
            OperationCode::StepFirst => self.op_step_first(session, &request),
            OperationCode::StepLast => self.op_step_last(session, &request),
            OperationCode::StepNext => self.op_step_next(session, &request),
            OperationCode::StepPrevious => self.op_step_previous(session, &request),
            OperationCode::BeginTransaction => self.op_begin_transaction(session, &request),
            OperationCode::EndTransaction => self.op_end_transaction(session, &request),
            OperationCode::AbortTransaction => self.op_abort_transaction(session, &request),
            OperationCode::Reset => self.op_reset(session, &request),
            OperationCode::Unknown => Err(BtrieveError::Status(StatusCode::InvalidOperation)),
            _ => Err(BtrieveError::Status(StatusCode::InvalidOperation)),
        };

        match result {
            Ok(response) => response,
            Err(e) => OperationResponse::error(e.status_code()),
        }
    }

    /// Shutdown the engine gracefully
    pub fn shutdown(&self) {
        // Flush all dirty pages
        let dirty = self.cache.clear();
        for (path, page) in dirty {
            if let Some(file) = self.files.get(&PathBuf::from(&path)) {
                let _ = file.read().write_page(&page);
            }
        }

        // Close all files
        self.files.close_all();
    }
}

// Operation implementations - these are stubs that will call into the specific modules
impl Engine {
    fn op_open(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::file_ops::open(self, session, req)
    }

    fn op_close(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::file_ops::close(self, session, req)
    }

    fn op_create(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::file_ops::create(self, session, req)
    }

    fn op_stat(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::file_ops::stat(self, session, req)
    }

    fn op_insert(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::record_ops::insert(self, session, req)
    }

    fn op_update(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::record_ops::update(self, session, req)
    }

    fn op_delete(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::record_ops::delete(self, session, req)
    }

    fn op_get_equal(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_equal(self, session, req)
    }

    fn op_get_next(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_next(self, session, req)
    }

    fn op_get_previous(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_previous(self, session, req)
    }

    fn op_get_greater(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_greater(self, session, req)
    }

    fn op_get_greater_or_equal(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_greater_or_equal(self, session, req)
    }

    fn op_get_less_than(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_less_than(self, session, req)
    }

    fn op_get_less_or_equal(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_less_or_equal(self, session, req)
    }

    fn op_get_first(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_first(self, session, req)
    }

    fn op_get_last(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::key_ops::get_last(self, session, req)
    }

    fn op_get_position(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::position_ops::get_position(self, session, req)
    }

    fn op_get_direct(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::position_ops::get_direct(self, session, req)
    }

    fn op_step_first(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::step_ops::step_first(self, session, req)
    }

    fn op_step_last(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::step_ops::step_last(self, session, req)
    }

    fn op_step_next(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::step_ops::step_next(self, session, req)
    }

    fn op_step_previous(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::step_ops::step_previous(self, session, req)
    }

    fn op_begin_transaction(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::transaction_ops::begin_transaction(self, session, req)
    }

    fn op_end_transaction(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::transaction_ops::end_transaction(self, session, req)
    }

    fn op_abort_transaction(&self, session: SessionId, req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        super::transaction_ops::abort_transaction(self, session, req)
    }

    fn op_reset(&self, _session: SessionId, _req: &OperationRequest) -> BtrieveResult<OperationResponse> {
        // Reset operation - typically does nothing in modern implementations
        Ok(OperationResponse::success())
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(1000)
    }
}
