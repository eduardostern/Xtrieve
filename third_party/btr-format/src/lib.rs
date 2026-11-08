//! Reader for Novell Btrieve/N 5.10a `.BTR` files.
//!
//! The layout is derived from the official 5.10a distribution and from
//! differential probes run against the real engine; see
//! `docs/btrieve-5.10a-file-format.md`.  Fields whose meaning is still
//! uncertain are exposed with `aux`/`unknown` names rather than guessed.

pub mod data;
pub mod error;
pub mod fcr;
pub mod file;
pub mod index;
pub mod key;
pub mod page;
pub mod recover;
pub mod write;

pub use data::Record;
pub use error::{FormatError, Result};
pub use fcr::{Fcr, FileFlags, KeyDef};
pub use file::{BtrieveFile, KeyStat, PageInfo, Stat};
pub use index::{IndexEntry, IndexPage};
pub use key::{compare_composite, compare_segment, Acs, KeySegment};
pub use page::{
    Page, PageHeader, PageId, PageKind, PageSize, DATA_PAGE_FLAG, KEY_DEFS_OFFSET, KEY_DEF_SIZE,
    PAGE_HEADER_SIZE,
};
pub use recover::{
    default_preimage_path, recover_file, recover_with_preimage, RecoveryReport,
    FILE_OPEN_MARKER_OFFSET, TRANSACTION_STATE_OFFSET,
};
pub use write::{
    cleared_chunk_page, compressed_chunk_count, compressed_chunk_pages, delete_fixed_at_in_place,
    delete_fixed_at_in_place_with_journal, delete_record, delete_record_at,
    insert_fixed_at_in_place, insert_records, removed_entry_page_image, touch_variable_chain_aux,
    touch_variable_update, update_compressed_at_in_place, update_fixed_at_in_place, update_record,
    update_record_at, variable_chunk_count, variable_chunk_pages, FileSpec, KeySpec, Writer,
};
