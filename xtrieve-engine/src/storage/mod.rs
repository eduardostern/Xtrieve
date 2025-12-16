//! Storage layer for Btrieve 5.1 file format
//!
//! This module handles the low-level binary format of Btrieve files:
//! - Page I/O
//! - FCR (File Control Record) parsing
//! - Key specifications
//! - B+ tree index structures
//! - Record management

pub mod page;
pub mod fcr;
pub mod key;
pub mod record;
pub mod btree;

pub use page::{Page, PageType, PAGE_SIZES};
pub use fcr::FileControlRecord;
pub use key::{KeySpec, KeyType, KeyFlags};
pub use record::Record;
pub use btree::BTree;
