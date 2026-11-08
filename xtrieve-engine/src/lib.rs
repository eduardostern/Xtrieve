//! Xtrieve engine adapter.
//!
//! Runtime storage is the clean-room Btrieve/N 5.10a engine in
//! `third_party/btr-engine`.  This crate keeps the daemon's request/response
//! envelope and the Xtrieve TCP protocol.

pub mod error;
pub mod operations;
pub mod protocol;

pub use btr_engine::status as status_codes;
pub use error::{BtrieveError, BtrieveResult, Status};
pub use operations::{Engine, OperationRequest, OperationResponse};
pub use protocol::{Request, Response, DEFAULT_PORT, POSITION_BLOCK_SIZE};

/// Native Btrieve caller/FCB state block.
pub type PositionBlock = [u8; POSITION_BLOCK_SIZE];
