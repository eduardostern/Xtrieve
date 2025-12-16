//! File manager for Btrieve files
//!
//! Manages open files, page caching, and locking.

pub mod open_files;
pub mod page_cache;
pub mod locking;
pub mod cursor;

pub use open_files::{OpenFile, OpenFileTable};
pub use page_cache::PageCache;
pub use locking::{LockManager, LockType};
pub use cursor::{Cursor, CursorState};
