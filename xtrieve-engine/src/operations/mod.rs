//! Btrieve operation handlers
//!
//! This module implements all Btrieve operation codes (0-50+).

pub mod dispatcher;
pub mod file_ops;
pub mod record_ops;
pub mod key_ops;
pub mod step_ops;
pub mod position_ops;
pub mod transaction_ops;

pub use dispatcher::{Engine, OperationCode, OperationRequest, OperationResponse};
