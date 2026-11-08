//! Error and status types.
//!
//! Btrieve 5.10a statuses are plain `u16` values.  The engine deliberately
//! does not translate them through a second enum: the on-wire status is the
//! authentic engine status.

use thiserror::Error;

pub type Status = u16;

#[derive(Debug, Error)]
pub enum BtrieveError {
    #[error("Btrieve status {0}")]
    Status(Status),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl BtrieveError {
    pub fn status_code(&self) -> Status {
        match self {
            BtrieveError::Status(code) => *code,
            BtrieveError::Io(_) => btr_engine::status::IO_ERROR,
            BtrieveError::Internal(_) => btr_engine::status::IO_ERROR,
        }
    }
}

pub type BtrieveResult<T> = Result<T, BtrieveError>;
