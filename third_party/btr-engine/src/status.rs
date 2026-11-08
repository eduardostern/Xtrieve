//! Btrieve status codes used by the engine.  Names and numbers follow
//! Appendix B of the 5.0 manual and the 5.10a catalog.

pub type Status = u16;

pub const SUCCESS: Status = 0;
pub const INVALID_OPERATION: Status = 1;
pub const IO_ERROR: Status = 2;
pub const FILE_NOT_OPEN: Status = 3;
pub const KEY_NOT_FOUND: Status = 4;
pub const DUPLICATE_KEY: Status = 5;
pub const INVALID_KEY_NUMBER: Status = 6;
pub const DIFFERENT_KEY_NUMBER: Status = 7;
pub const INVALID_POSITIONING: Status = 8;
pub const END_OF_FILE: Status = 9;
pub const MODIFIABLE_KEY_ERROR: Status = 10;
pub const INVALID_FILE_NAME: Status = 11;
pub const FILE_NOT_FOUND: Status = 12;
pub const EXTENDED_FILE_ERROR: Status = 13;
pub const PREIMAGE_OPEN_ERROR: Status = 14;
pub const PREIMAGE_IO_ERROR: Status = 15;
pub const CLOSE_ERROR: Status = 17;
pub const KEY_BUFFER_TOO_SHORT: Status = 21;
pub const DATA_BUFFER_LENGTH: Status = 22;
pub const POSITION_BLOCK_LENGTH: Status = 23;
pub const PAGE_SIZE_ERROR: Status = 24;
pub const CREATE_IO_ERROR: Status = 25;
pub const NUMBER_OF_KEYS: Status = 26;
pub const INVALID_KEY_POSITION: Status = 27;
pub const INVALID_RECORD_LENGTH: Status = 28;
pub const INVALID_KEY_LENGTH: Status = 29;
pub const NOT_A_BTRIEVE_FILE: Status = 30;
pub const EXTEND_IO_ERROR: Status = 32;
pub const INVALID_EXTENSION_NAME: Status = 34;
pub const DIRECTORY_ERROR: Status = 35;
pub const TRANSACTION_ERROR: Status = 36;
pub const TRANSACTION_ACTIVE: Status = 37;
pub const TRANSACTION_FILE_IO_ERROR: Status = 38;
pub const END_ABORT_ERROR: Status = 39;
pub const OPERATION_NOT_ALLOWED: Status = 41;
pub const INVALID_RECORD_ADDRESS: Status = 43;
pub const ACCESS_DENIED: Status = 46;
pub const OWNER_ALREADY_SET: Status = 50;
pub const INVALID_OWNER: Status = 51;
pub const FILE_ALREADY_EXISTS: Status = 59;
pub const REJECT_COUNT_REACHED: Status = 60;
pub const WORK_SPACE_TOO_SMALL: Status = 61;
pub const INCORRECT_DESCRIPTOR: Status = 62;
pub const FILTER_LIMIT_REACHED: Status = 64;
pub const INCORRECT_FIELD_OFFSET: Status = 65;
pub const LOST_POSITION: Status = 82;
pub const RECORD_IN_USE: Status = 84;
pub const FILE_IN_USE: Status = 85;
pub const FILE_TABLE_FULL: Status = 86;
pub const HANDLE_TABLE_FULL: Status = 87;
pub const INCOMPATIBLE_MODE: Status = 88;

/// Map a format-layer error to the closest Btrieve status.
pub fn from_format(error: &btr_format::FormatError) -> Status {
    use btr_format::FormatError as E;
    match error {
        E::Io(_) => IO_ERROR,
        E::TooSmall | E::InvalidFileSize { .. } | E::NotDataPage { .. } => NOT_A_BTRIEVE_FILE,
        E::InvalidPageSize(_) => PAGE_SIZE_ERROR,
        E::OutOfBounds { .. } | E::InvalidRecordAddress(_) => INVALID_RECORD_ADDRESS,
        E::RecordInHeader(_) => INVALID_RECORD_LENGTH,
        E::RecordLengthMismatch { .. } => DATA_BUFFER_LENGTH,
        E::KeyOutOfRange { .. } => INVALID_KEY_POSITION,
        E::DuplicateKey => DUPLICATE_KEY,
        E::InvalidSpec(text) => {
            if text.contains("key") {
                INVALID_KEY_POSITION
            } else if text.contains("record") {
                INVALID_RECORD_LENGTH
            } else {
                INVALID_OPERATION
            }
        }
        E::Unsupported(_) => INVALID_OPERATION,
        E::MissingPreimage => PREIMAGE_OPEN_ERROR,
        E::InvalidPreimage(_) => PREIMAGE_IO_ERROR,
    }
}
