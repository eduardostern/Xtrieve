use thiserror::Error;

use crate::page::PageKind;

pub type Result<T> = std::result::Result<T, FormatError>;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("file is too small to contain a Btrieve page")]
    TooSmall,

    #[error("invalid page size {0}; expected 512, 1024, 2048, or 4096")]
    InvalidPageSize(u16),

    #[error("file size {size} is not a multiple of page size {page_size}")]
    InvalidFileSize { size: usize, page_size: usize },

    #[error("field at page offset 0x{offset:x} with size {size} exceeds page size {page_size}")]
    OutOfBounds {
        offset: usize,
        size: usize,
        page_size: usize,
    },

    #[error("record address 0x{0:08x} is outside the file")]
    InvalidRecordAddress(u32),

    #[error(
        "record address 0x{address:08x} points at page {page} of kind {kind:?}, not a data page"
    )]
    NotDataPage {
        address: u32,
        page: u32,
        kind: PageKind,
    },

    #[error("record at address 0x{0:08x} overlaps the page header")]
    RecordInHeader(u32),

    #[error("record length {actual} does not match the file's logical length {expected}")]
    RecordLengthMismatch { expected: u16, actual: usize },

    #[error(
        "key {index} at position {position} length {length} exceeds record length {record_length}"
    )]
    KeyOutOfRange {
        index: usize,
        position: u16,
        length: u16,
        record_length: usize,
    },

    #[error("duplicate key value for a unique key")]
    DuplicateKey,

    #[error("invalid file specification: {0}")]
    InvalidSpec(&'static str),

    #[error("unsupported feature: {0}")]
    Unsupported(&'static str),

    #[error("a transaction is active but its preimage (.PRE) file is missing")]
    MissingPreimage,

    #[error("invalid preimage file: {0}")]
    InvalidPreimage(&'static str),
}
