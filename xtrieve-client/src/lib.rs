//! Xtrieve Client Library
//!
//! Provides a Btrieve-compatible API for accessing Xtrieve database files.

pub mod btrieve;
pub mod client;

pub use btrieve::{BtrieveFile, BtrieveRecord};
#[cfg(feature = "async")]
pub use client::AsyncXtrieveClient;
pub use client::{BtrieveRequest, BtrieveResponse, XtrieveClient};
pub use xtrieve_engine::{status_codes, BtrieveError, BtrieveResult, Status};
