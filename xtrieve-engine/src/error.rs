//! Btrieve status codes and error handling
//!
//! Btrieve uses numeric status codes (0-172+) to indicate success or failure.
//! This module provides a complete mapping of all known status codes.

use thiserror::Error;

/// Btrieve status codes - these match the original Btrieve 5.1 exactly
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum StatusCode {
    /// Operation completed successfully
    Success = 0,
    /// Invalid operation code
    InvalidOperation = 1,
    /// I/O error occurred
    IoError = 2,
    /// File not open
    FileNotOpen = 3,
    /// Key value not found
    KeyNotFound = 4,
    /// Duplicate key value (when duplicates not allowed)
    DuplicateKey = 5,
    /// Invalid key number
    InvalidKeyNumber = 6,
    /// Different key number than expected
    DifferentKeyNumber = 7,
    /// Invalid positioning (no current record)
    InvalidPositioning = 8,
    /// End of file reached
    EndOfFile = 9,
    /// Modifiable key value changed
    ModifiableKeyChanged = 10,
    /// Invalid file name
    InvalidFileName = 11,
    /// File not found
    FileNotFound = 12,
    /// Extended file error
    ExtendedFileError = 13,
    /// Pre-image open error
    PreImageOpenError = 14,
    /// Pre-image I/O error
    PreImageIoError = 15,
    /// Expansion error
    ExpansionError = 16,
    /// Close error
    CloseError = 17,
    /// Disk full
    DiskFull = 18,
    /// Unrecoverable error
    UnrecoverableError = 19,
    /// Record manager inactive
    RecordManagerInactive = 20,
    /// Key buffer too short
    KeyBufferTooShort = 21,
    /// Data buffer too short
    DataBufferTooShort = 22,
    /// Position block length error
    PositionBlockLengthError = 23,
    /// Page size error
    PageSizeError = 24,
    /// Create I/O error
    CreateIoError = 25,
    /// Number of keys error
    NumberOfKeysError = 26,
    /// Invalid key position
    InvalidKeyPosition = 27,
    /// Invalid record length
    InvalidRecordLength = 28,
    /// Invalid key length
    InvalidKeyLength = 29,
    /// Not a Btrieve file
    NotBtrieveFile = 30,
    /// File already extended
    FileAlreadyExtended = 31,
    /// Extend I/O error
    ExtendIoError = 32,
    /// Invalid extension name
    InvalidExtensionName = 33,
    /// Directory error
    DirectoryError = 34,
    /// Transaction error
    TransactionError = 35,
    /// Transaction is active
    TransactionActive = 36,
    /// Transaction control file I/O error
    TransactionControlFileIoError = 37,
    /// End/Abort transaction error
    EndAbortTransactionError = 38,
    /// Transaction max files exceeded
    TransactionMaxFiles = 39,
    /// Operation not allowed
    OperationNotAllowed = 40,
    /// Incomplete accelerated access
    IncompleteAcceleratedAccess = 41,
    /// Invalid record address
    InvalidRecordAddress = 42,
    /// Null key path
    NullKeyPath = 43,
    /// Inconsistent key flags
    InconsistentKeyFlags = 44,
    /// Access denied
    AccessDenied = 45,
    /// Maximum open files exceeded
    MaxOpenFiles = 46,
    /// Invalid alternate collating sequence
    InvalidACS = 47,
    /// Key type error
    KeyTypeError = 48,
    /// Owner already set
    OwnerAlreadySet = 49,
    /// Invalid owner
    InvalidOwner = 50,
    /// Error writing cache
    CacheWriteError = 51,
    /// Invalid interface
    InvalidInterface = 52,
    /// Variable page error
    VariablePageError = 54,
    /// Autoincrement error
    AutoincrementError = 55,
    /// Incomplete index
    IncompleteIndex = 56,
    /// Expanded memory error
    ExpandedMemoryError = 57,
    /// Compress buffer too short
    CompressBufferTooShort = 58,
    /// File already exists
    FileAlreadyExists = 59,
    /// Reject count reached
    RejectCountReached = 60,
    /// Work space too small
    WorkSpaceTooSmall = 61,
    /// Descriptor bad
    DescriptorBad = 62,
    /// Extended get buffer too small
    ExtendedGetBufferTooSmall = 63,
    /// Get/Step extended error
    GetStepExtendedError = 64,
    /// Invalid extended insert buffer
    InvalidExtendedInsertBuffer = 65,
    /// Optimize limit reached
    OptimizeLimitReached = 66,
    /// Invalid extractor
    InvalidExtractor = 67,
    /// RI violation
    RiViolation = 68,
    /// RI referenced file cannot be opened
    RiReferenceFileError = 69,
    /// RI referenced file is out of sync
    RiOutOfSync = 70,
    /// Wait lock error (record locked by another)
    WaitLockError = 78,
    /// Record in use (locked)
    RecordInUse = 79,
    /// File in use
    FileInUse = 80,
    /// File table full
    FileTableFull = 81,
    /// Handle table full
    HandleTableFull = 82,
    /// Incompatible mode error
    IncompatibleMode = 83,
    /// Device table full
    DeviceTableFull = 84,
    /// Server error
    ServerError = 85,
    /// Transaction table full
    TransactionTableFull = 86,
    /// Incompatible lock type
    IncompatibleLockType = 87,
    /// Permission error
    PermissionError = 88,
    /// Session no longer valid
    SessionInvalid = 89,
    /// Communications environment error
    CommunicationsError = 90,
    /// Data message too small
    DataMessageTooSmall = 91,
    /// Internal transaction error
    InternalTransactionError = 92,
    /// Requester can't access
    RequesterCantAccess = 93,
    /// Record locked
    RecordLocked = 94,
    /// Lost position
    LostPosition = 95,
    /// Read outside transaction
    ReadOutsideTransaction = 96,
    /// Record/page level conflict
    RecordPageConflict = 97,
    /// Deadlock detected
    DeadlockDetected = 78,
    /// Lock timeout
    LockTimeout = 79,
    /// File gone
    FileGone = 99,
    /// Server crash - locks lost
    ServerCrashLocksLost = 100,

    // Status codes 101-171 are additional error conditions
    /// Unknown status code
    Unknown = 65535,
}

impl StatusCode {
    /// Create a StatusCode from a raw u16 value
    pub fn from_raw(code: u16) -> Self {
        match code {
            0 => StatusCode::Success,
            1 => StatusCode::InvalidOperation,
            2 => StatusCode::IoError,
            3 => StatusCode::FileNotOpen,
            4 => StatusCode::KeyNotFound,
            5 => StatusCode::DuplicateKey,
            6 => StatusCode::InvalidKeyNumber,
            7 => StatusCode::DifferentKeyNumber,
            8 => StatusCode::InvalidPositioning,
            9 => StatusCode::EndOfFile,
            10 => StatusCode::ModifiableKeyChanged,
            11 => StatusCode::InvalidFileName,
            12 => StatusCode::FileNotFound,
            13 => StatusCode::ExtendedFileError,
            14 => StatusCode::PreImageOpenError,
            15 => StatusCode::PreImageIoError,
            16 => StatusCode::ExpansionError,
            17 => StatusCode::CloseError,
            18 => StatusCode::DiskFull,
            19 => StatusCode::UnrecoverableError,
            20 => StatusCode::RecordManagerInactive,
            21 => StatusCode::KeyBufferTooShort,
            22 => StatusCode::DataBufferTooShort,
            23 => StatusCode::PositionBlockLengthError,
            24 => StatusCode::PageSizeError,
            25 => StatusCode::CreateIoError,
            26 => StatusCode::NumberOfKeysError,
            27 => StatusCode::InvalidKeyPosition,
            28 => StatusCode::InvalidRecordLength,
            29 => StatusCode::InvalidKeyLength,
            30 => StatusCode::NotBtrieveFile,
            31 => StatusCode::FileAlreadyExtended,
            32 => StatusCode::ExtendIoError,
            33 => StatusCode::InvalidExtensionName,
            34 => StatusCode::DirectoryError,
            35 => StatusCode::TransactionError,
            36 => StatusCode::TransactionActive,
            37 => StatusCode::TransactionControlFileIoError,
            38 => StatusCode::EndAbortTransactionError,
            39 => StatusCode::TransactionMaxFiles,
            40 => StatusCode::OperationNotAllowed,
            41 => StatusCode::IncompleteAcceleratedAccess,
            42 => StatusCode::InvalidRecordAddress,
            43 => StatusCode::NullKeyPath,
            44 => StatusCode::InconsistentKeyFlags,
            45 => StatusCode::AccessDenied,
            46 => StatusCode::MaxOpenFiles,
            47 => StatusCode::InvalidACS,
            48 => StatusCode::KeyTypeError,
            49 => StatusCode::OwnerAlreadySet,
            50 => StatusCode::InvalidOwner,
            51 => StatusCode::CacheWriteError,
            52 => StatusCode::InvalidInterface,
            54 => StatusCode::VariablePageError,
            55 => StatusCode::AutoincrementError,
            56 => StatusCode::IncompleteIndex,
            57 => StatusCode::ExpandedMemoryError,
            58 => StatusCode::CompressBufferTooShort,
            59 => StatusCode::FileAlreadyExists,
            60 => StatusCode::RejectCountReached,
            61 => StatusCode::WorkSpaceTooSmall,
            62 => StatusCode::DescriptorBad,
            63 => StatusCode::ExtendedGetBufferTooSmall,
            64 => StatusCode::GetStepExtendedError,
            65 => StatusCode::InvalidExtendedInsertBuffer,
            66 => StatusCode::OptimizeLimitReached,
            67 => StatusCode::InvalidExtractor,
            68 => StatusCode::RiViolation,
            69 => StatusCode::RiReferenceFileError,
            70 => StatusCode::RiOutOfSync,
            78 => StatusCode::DeadlockDetected,
            79 => StatusCode::RecordInUse,
            80 => StatusCode::FileInUse,
            81 => StatusCode::FileTableFull,
            82 => StatusCode::HandleTableFull,
            83 => StatusCode::IncompatibleMode,
            84 => StatusCode::DeviceTableFull,
            85 => StatusCode::ServerError,
            86 => StatusCode::TransactionTableFull,
            87 => StatusCode::IncompatibleLockType,
            88 => StatusCode::PermissionError,
            89 => StatusCode::SessionInvalid,
            90 => StatusCode::CommunicationsError,
            91 => StatusCode::DataMessageTooSmall,
            92 => StatusCode::InternalTransactionError,
            93 => StatusCode::RequesterCantAccess,
            94 => StatusCode::RecordLocked,
            95 => StatusCode::LostPosition,
            96 => StatusCode::ReadOutsideTransaction,
            97 => StatusCode::RecordPageConflict,
            99 => StatusCode::FileGone,
            100 => StatusCode::ServerCrashLocksLost,
            _ => StatusCode::Unknown,
        }
    }

    /// Get the raw status code value
    pub fn as_raw(&self) -> u16 {
        *self as u16
    }

    /// Check if this is a success status
    pub fn is_success(&self) -> bool {
        matches!(self, StatusCode::Success)
    }

    /// Check if this indicates end of file/key sequence
    pub fn is_eof(&self) -> bool {
        matches!(self, StatusCode::EndOfFile | StatusCode::KeyNotFound)
    }
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.as_raw(), match self {
            StatusCode::Success => "Success",
            StatusCode::InvalidOperation => "Invalid operation",
            StatusCode::IoError => "I/O error",
            StatusCode::FileNotOpen => "File not open",
            StatusCode::KeyNotFound => "Key value not found",
            StatusCode::DuplicateKey => "Duplicate key value",
            StatusCode::InvalidKeyNumber => "Invalid key number",
            StatusCode::DifferentKeyNumber => "Different key number",
            StatusCode::InvalidPositioning => "Invalid positioning",
            StatusCode::EndOfFile => "End of file",
            StatusCode::FileNotFound => "File not found",
            StatusCode::NotBtrieveFile => "Not a Btrieve file",
            StatusCode::DiskFull => "Disk full",
            StatusCode::RecordInUse => "Record in use",
            StatusCode::FileInUse => "File in use",
            StatusCode::DeadlockDetected => "Deadlock detected",
            _ => "Error",
        })
    }
}

/// Main error type for the Xtrieve engine
#[derive(Error, Debug)]
pub enum BtrieveError {
    #[error("Btrieve status {0}")]
    Status(StatusCode),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid file format: {0}")]
    InvalidFormat(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl BtrieveError {
    /// Get the Btrieve status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            BtrieveError::Status(code) => *code,
            BtrieveError::Io(_) => StatusCode::IoError,
            BtrieveError::InvalidFormat(_) => StatusCode::NotBtrieveFile,
            BtrieveError::Internal(_) => StatusCode::UnrecoverableError,
        }
    }
}

impl From<StatusCode> for BtrieveError {
    fn from(code: StatusCode) -> Self {
        BtrieveError::Status(code)
    }
}

/// Result type for Btrieve operations
pub type BtrieveResult<T> = Result<T, BtrieveError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_code_roundtrip() {
        for code in [0, 1, 4, 5, 9, 12, 30, 78, 79, 80] {
            let status = StatusCode::from_raw(code);
            assert_eq!(status.as_raw(), code);
        }
    }

    #[test]
    fn test_success_check() {
        assert!(StatusCode::Success.is_success());
        assert!(!StatusCode::KeyNotFound.is_success());
    }

    #[test]
    fn test_eof_check() {
        assert!(StatusCode::EndOfFile.is_eof());
        assert!(StatusCode::KeyNotFound.is_eof());
        assert!(!StatusCode::Success.is_eof());
    }
}
