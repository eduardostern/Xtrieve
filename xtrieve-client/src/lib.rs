//! Xtrieve Client Library
//!
//! Provides a Btrieve-compatible API for accessing Xtrieve database files.

pub mod client;
pub mod btrieve;

pub use client::XtrieveClient;
pub use btrieve::{BtrieveFile, BtrieveRecord};
pub use xtrieve_engine::{BtrieveError, BtrieveResult, StatusCode};

pub mod proto {
    tonic::include_proto!("xtrieve");
}
