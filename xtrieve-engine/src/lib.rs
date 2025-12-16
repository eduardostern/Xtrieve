//! Xtrieve Engine - Btrieve 5.1 Compatible ISAM Database Engine
//!
//! This crate provides the core storage engine for reading and writing
//! Btrieve 5.1 compatible database files.

pub mod error;
pub mod storage;
pub mod file_manager;
pub mod operations;

pub use error::{BtrieveError, BtrieveResult, StatusCode};
