//! Persistent Btrieve/N 5.10a engine.
//!
//! The engine exposes the DOS API's operation numbers and statuses through a
//! Rust call interface.  Open files are tracked by handles stored in the
//! caller's 128-byte position block; cursors are kept in memory, so keys may
//! be longer than the position block would otherwise allow.
//!
//! Implemented operations: Open, Close, Insert, Update, Delete, all keyed Get
//! variants (including the +50 key-only bias), Get Position/Direct, Step
//! First/Last/Next/Previous, Create, Stat, Set/Get Directory, Begin/End/Abort
//! Transaction, Version, Unlock, Reset, Stop, Set/Clear Owner, and Create/Drop
//! Supplemental Index.  Extend and the extended-filter get/insert/step
//! operations return status 1 (invalid operation).

pub mod create;
pub mod extended;
pub mod position;
pub mod status;
pub mod txn;
pub mod view;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use btr_format::write::OwnerInfo;
use btr_format::{
    cleared_chunk_page, compressed_chunk_count, compressed_chunk_pages,
    delete_fixed_at_in_place_with_journal, insert_fixed_at_in_place, removed_entry_page_image,
    touch_variable_chain_aux, touch_variable_update, update_compressed_at_in_place,
    update_fixed_at_in_place, variable_chunk_count, variable_chunk_pages, Acs, BtrieveFile,
    FileSpec, FormatError, KeySegment, KeySpec, Writer, KEY_DEFS_OFFSET, KEY_DEF_SIZE,
};

pub use position::{PositionBlock, POSITION_BLOCK_SIZE};
pub use status::Status;
use status::*;

use crate::txn::{
    append_operation_images, append_preimage_group, changed_pages, clear_open_marker,
    clear_transaction_marker, operation_image_group, page_is_data, preimage_path, reset_log,
    set_page_aux, set_transaction_markers, start_preimage, write_log, MutationKind,
};
use crate::view::{
    build_physical_view, build_view, first_after, last_before, lower_bound, upper_bound,
};

const MAX_OPEN_FILES: usize = 255;

/// Per-caller transaction namespace.  The C ABI uses session 0; a server can
/// keep one session per connected client.
pub type SessionId = u64;
pub const DEFAULT_SESSION: SessionId = 0;

/// True when a file image carries an encrypted owner (format word 0xFD00 and
/// encryption bit in the owner mode).
fn image_owner_encrypted(bytes: &[u8]) -> bool {
    bytes.len() > 0x38
        && u16::from_le_bytes([bytes[6], bytes[7]]) == 0xFD00
        && bytes[0x37] & 0x02 != 0
}

fn owner_hash_from_image(bytes: &[u8]) -> Option<[u8; 9]> {
    bytes.get(0x2E..0x37)?.try_into().ok()
}

/// Count index pages that changed, grouped by key number.  The DOS engine
/// journals a page image for every page its B-tree code touches while deleting
/// or inserting an entry; a page that is freed or rewritten always differs.
fn changed_index_page_counts(old: &BtrieveFile, new: &BtrieveFile) -> Vec<usize> {
    let mut counts = vec![0usize; old.fcr.key_defs.len().max(new.fcr.key_defs.len())];
    let page_size = old.page_size.bytes();
    for page in &old.index_pages {
        let start = page.page_number as usize * page_size;
        if start + page_size > old.bytes.len() {
            continue;
        }
        let old_page = &old.bytes[start..start + page_size];
        let changed = if start + page_size <= new.bytes.len() {
            old_page != &new.bytes[start..start + page_size]
        } else {
            true
        };
        if changed {
            if let Some(count) = counts.get_mut(page.key_index) {
                *count += 1;
            }
        }
    }
    counts
}

fn journal_chunk_count(bytes: &[u8], file: &BtrieveFile, address: u32) -> usize {
    if file.fcr.file_flags.compression() {
        compressed_chunk_count(bytes, file, address).unwrap_or(0)
    } else if file.fcr.file_flags.variable_length() {
        variable_chunk_count(bytes, file, address).unwrap_or(0)
    } else {
        0
    }
}

/// Owner encryption XORs every byte of pages 1.. with a mask derived from the
/// owner hash.  The transformation is symmetric, so this decrypts plaintext
/// files and encrypts working images.
fn crypt_owner_pages(bytes: &mut [u8], hash: &[u8; 9], page_size: usize) {
    if page_size == 0 {
        return;
    }
    for page in 1..bytes.len() / page_size {
        let start = page * page_size;
        for offset in 0..page_size {
            bytes[start + offset] ^= btr_format::fcr::owner_mask_byte(hash, offset, page_size);
        }
    }
}

#[derive(Debug)]
struct OpenFile {
    path: PathBuf,
    share_key: PathBuf,
    read_only: bool,
    session: SessionId,
}

#[derive(Debug, Clone)]
struct Cursor {
    handle: u32,
    key_number: u16,
    established: bool,
    last_key: Vec<u8>,
    last_address: u32,
    /// Physical currency, independent of the logical key position.
    physical_address: u32,
    physical_established: bool,
    /// Neighbouring records in this key value's duplicate chain, when the
    /// current key is a duplicate key.
    duplicate_previous: Option<u32>,
    duplicate_next: Option<u32>,
    /// Number of duplicate-key chains the last Insert appended to.
    inserted_duplicate_chains: usize,
    /// B-tree page path from root to the record's key entry.
    tree_path: Vec<u32>,
    /// Replacement record address when Delete removes an interior entry.
    delete_replacement: Option<u32>,
    /// One stale path entry the engine leaves after a merge.
    path_extra: Option<u32>,
    /// Display address for a key-only record (inline record offset - 4).
    key_only_display: Option<u32>,
    /// Total page images journaled to the transient preimage file since this
    /// handle opened the file.  The DOS engine keeps this running count and
    /// mirrors its high-water mark into FCR +0x70 until Close.
    preimage_pages: u32,
    /// The last successful Update changed a modifiable key value.
    key_changed: bool,
    /// The open file has no key definitions.
    keyless: bool,
    /// The last Insert's selected key was excluded from the index.
    insert_excluded: bool,
    /// The deleted record had no entry in any key tree.
    unindexed_delete: bool,
}

impl Cursor {
    fn new(handle: u32) -> Self {
        Self {
            handle,
            key_number: 0,
            established: false,
            last_key: Vec::new(),
            last_address: 0,
            physical_address: 0,
            physical_established: false,
            duplicate_previous: None,
            duplicate_next: None,
            inserted_duplicate_chains: 0,
            tree_path: Vec::new(),
            delete_replacement: None,
            path_extra: None,
            key_only_display: None,
            preimage_pages: 0,
            key_changed: false,
            keyless: false,
            insert_excluded: false,
            unindexed_delete: false,
        }
    }
}

#[derive(Debug, Clone)]
struct ShareState {
    handles: usize,
    exclusive: bool,
    accelerated: bool,
    /// Handle whose mutation last wrote the transient preimage.  The DOS
    /// engine only removes `.PRE`/clears +0x70 when that handle is the last
    /// one closed.
    journal_owner: Option<u32>,
}

#[derive(Debug)]
struct Transaction {
    snapshots: HashMap<PathBuf, Vec<u8>>,
    /// Transaction operation counter written into the FCR and preimage.
    operation_counters: HashMap<PathBuf, u16>,
    preimage_started: HashSet<PathBuf>,
    /// Total preimage page count (FCR image plus appended images).
    preimage_pages: HashMap<PathBuf, u16>,
    log_path: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct Engine {
    files: HashMap<u32, OpenFile>,
    cursors: HashMap<u32, Cursor>,
    sharing: HashMap<PathBuf, ShareState>,
    record_locks: HashMap<(PathBuf, u32), u32>,
    transactions: HashMap<SessionId, Transaction>,
    /// Canonical file path -> session whose transaction currently owns it.
    transaction_owners: HashMap<PathBuf, SessionId>,
    /// Per-session Set Directory value.
    directories: HashMap<SessionId, PathBuf>,
    current_dir: Option<PathBuf>,
    transaction_log: Option<PathBuf>,
    active_session: SessionId,
}

#[derive(Debug, Clone)]
struct Loaded {
    raw: Vec<u8>,
    file: BtrieveFile,
    spec: FileSpec,
    records: Vec<(u32, Vec<u8>)>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            transaction_log: std::env::var_os("BTRIEVE_TRN").map(PathBuf::from),
            ..Self::default()
        }
    }

    /// Set the directory used to resolve relative file names.
    pub fn set_current_directory(&mut self, directory: impl Into<PathBuf>) {
        self.current_dir = Some(directory.into());
    }

    /// Set the transaction log path written by End Transaction.
    pub fn set_transaction_log(&mut self, path: impl Into<PathBuf>) {
        self.transaction_log = Some(path.into());
    }

    fn session_dir(&self) -> Option<&PathBuf> {
        self.directories
            .get(&self.active_session)
            .or(self.current_dir.as_ref())
    }

    fn resolve(&self, name: &str) -> PathBuf {
        let path = PathBuf::from(name);
        if path.is_absolute() {
            path
        } else if let Some(directory) = self.session_dir() {
            directory.join(path)
        } else {
            path
        }
    }

    fn canonical(&self, path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn in_transaction(&self) -> bool {
        self.transactions.contains_key(&self.active_session)
    }

    fn current_transaction_mut(&mut self) -> Option<&mut Transaction> {
        let session = self.active_session;
        self.transactions.get_mut(&session)
    }

    fn transaction_log_path(&self, session: SessionId, target: Option<&Path>) -> Option<PathBuf> {
        let base = self.transaction_log.clone().or_else(|| {
            target
                .and_then(Path::parent)
                .map(|parent| parent.join("BTRIEVE.TRN"))
        });
        let base = base.or_else(|| {
            self.directories
                .get(&session)
                .or(self.current_dir.as_ref())
                .map(|directory| directory.join("BTRIEVE.TRN"))
        })?;
        if session == DEFAULT_SESSION {
            Some(base)
        } else {
            let mut name = base.as_os_str().to_os_string();
            name.push(format!(".{session}"));
            Some(PathBuf::from(name))
        }
    }

    fn check_transaction_access(&self, share_key: &Path) -> Result<(), Status> {
        if let Some(owner) = self.transaction_owners.get(share_key) {
            if *owner != self.active_session {
                return Err(FILE_IN_USE);
            }
        }
        Ok(())
    }

    fn claim_file_for_transaction(&mut self, path: &Path) -> Result<(), Status> {
        if !self.in_transaction() {
            return Ok(());
        }
        let share_key = self.canonical(path);
        match self.transaction_owners.get(&share_key) {
            Some(owner) if *owner != self.active_session => Err(FILE_IN_USE),
            Some(_) => Ok(()),
            None => {
                self.transaction_owners
                    .insert(share_key, self.active_session);
                Ok(())
            }
        }
    }

    fn release_transaction_ownership(&mut self, session: SessionId) {
        self.transaction_owners.retain(|_, owner| *owner != session);
    }

    /// Validate the caller's handle and session without applying the
    /// file-transaction visibility check.  Close, Unlock, and session cleanup
    /// use this so another session's transaction cannot prevent cleanup.
    fn handle_raw(&self, position: &PositionBlock) -> Result<u32, Status> {
        let reference = position::decode(position).ok_or(FILE_NOT_OPEN)?;
        let open = self.files.get(&reference.handle).ok_or(FILE_NOT_OPEN)?;
        if open.session != self.active_session {
            return Err(FILE_NOT_OPEN);
        }
        Ok(reference.handle)
    }

    fn handle(&self, position: &PositionBlock) -> Result<u32, Status> {
        self.handle_raw(position)
    }

    /// File-level transaction visibility is checked centrally before dispatch;
    /// individual handlers use `handle()` so an error is not turned into
    /// status 3 by their `let Ok(handle) = ... else` pattern.
    fn check_position_transaction(&self, position: &PositionBlock) -> Option<Status> {
        let reference = position::decode(position)?;
        let open = self.files.get(&reference.handle)?;
        if open.session != self.active_session {
            return None;
        }
        self.check_transaction_access(&open.share_key).err()
    }

    fn open_file(&self, handle: u32) -> Result<&OpenFile, Status> {
        self.files.get(&handle).ok_or(FILE_NOT_OPEN)
    }

    fn cursor(&mut self, position: &PositionBlock, handle: u32) -> Cursor {
        let token = position::decode(position)
            .map(|reference| reference.token)
            .unwrap_or(0);
        let cursor = self
            .cursors
            .entry(token)
            .or_insert_with(|| Cursor::new(handle));
        if cursor.handle != handle {
            *cursor = Cursor::new(handle);
        }
        cursor.clone()
    }

    fn store_cursor(&mut self, position: &PositionBlock, cursor: Cursor) {
        let token = position::decode(position)
            .map(|reference| reference.token)
            .unwrap_or(0);
        self.cursors.insert(token, cursor);
    }

    fn update_open_marker(&mut self, path: &Path, _op: u16, cursor: &Cursor) {
        if self.in_transaction() {
            return;
        }
        if let Ok(mut bytes) = std::fs::read(path) {
            if bytes.len() >= 0x72 {
                let current = u16::from_le_bytes([bytes[0x70], bytes[0x71]]);
                let value = current.max(cursor.preimage_pages.min(u16::MAX as u32) as u16);
                bytes[0x70..0x72].copy_from_slice(&value.to_le_bytes());
                let _ = std::fs::write(path, bytes);
            }
        }
    }

    /// Outside a transaction the DOS engine still overwrites `<file>.PRE`
    /// with this operation's ordered page-image group and raises FCR +0x70 to
    /// the group's page count (as a high-water mark until Close).
    fn preimage_target(&self, path: &Path, file: &BtrieveFile) -> PathBuf {
        if let Some(extension) = &file.fcr.extension_name {
            if let Some(extension_path) = self.resolve_extension_path(extension) {
                return preimage_path(&extension_path);
            }
        }
        preimage_path(path)
    }

    #[allow(clippy::too_many_arguments)]
    fn write_transient_preimage(
        &self,
        path: &Path,
        old: &[u8],
        old_file: &BtrieveFile,
        kind: MutationKind,
        selected_key: Option<usize>,
        address: u32,
        index_pages: &[Vec<u8>],
        excluded: bool,
    ) -> Result<Vec<u8>, Status> {
        let new = std::fs::read(path).map_err(|_| IO_ERROR)?;
        let page_size = old_file.page_size.bytes();
        let encrypted = old_file.fcr.owner_present && old_file.fcr.owner_mode & 0x02 != 0;
        let mut new_view = new.clone();
        if encrypted {
            crypt_owner_pages(&mut new_view, &old_file.fcr.owner_hash, page_size);
        }
        let aux = if new_view.len() >= 6 {
            u16::from_le_bytes([new_view[4], new_view[5]]).wrapping_sub(1)
        } else {
            0
        };
        let duplicate = FileSpec::from_file(old_file)
            .map(|spec| spec.keys.iter().any(|key| key.has_duplicates()))
            .unwrap_or(false);
        let group = if kind == MutationKind::Insert && excluded {
            self.excluded_insert_preimage_group(old_file, old, address, aux)
        } else if kind == MutationKind::Delete && duplicate {
            self.duplicate_delete_preimage_group(old_file, old, address, aux)
        } else if kind == MutationKind::Insert && duplicate {
            BtrieveFile::open(path)
                .ok()
                .map(|new_file| {
                    self.duplicate_insert_preimage_group(
                        old_file,
                        old,
                        &new_file,
                        &new_view,
                        address,
                        selected_key,
                        aux,
                    )
                })
                .unwrap_or_default()
        } else if kind == MutationKind::Delete {
            self.delete_preimage_group(old_file, old, address, index_pages, aux)
        } else if kind == MutationKind::Update
            && (old_file.fcr.file_flags.variable_length() || old_file.fcr.file_flags.compression())
        {
            self.update_preimage_group(old_file, old, &new_view, address, aux)
        } else if kind == MutationKind::Update {
            BtrieveFile::open(path)
                .ok()
                .and_then(|new_file| {
                    self.key_change_preimage_group(
                        old_file, old, &new_file, &new_view, address, aux,
                    )
                })
                .unwrap_or_else(|| {
                    operation_image_group(
                        old,
                        &new_view,
                        page_size,
                        kind,
                        aux,
                        old_file,
                        selected_key,
                    )
                })
        } else {
            operation_image_group(old, &new_view, page_size, kind, aux, old_file, selected_key)
        };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.preimage_target(path, old_file))
            .map_err(|_| PREIMAGE_IO_ERROR)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|_| PREIMAGE_IO_ERROR)?;
        file.write_all(&group).map_err(|_| PREIMAGE_IO_ERROR)?;
        Ok(new)
    }

    fn update_preimage_group(
        &self,
        old_file: &BtrieveFile,
        old: &[u8],
        new: &[u8],
        address: u32,
        aux: u16,
    ) -> Vec<u8> {
        fn stamp(group: &mut Vec<u8>, image: &[u8], page_size: usize, aux: u16) {
            if image.len() < page_size {
                return;
            }
            let mut image = image.to_vec();
            let flags = u16::from_le_bytes([image[4], image[5]]) & 0x8000;
            image[4..6].copy_from_slice(&(flags | aux).to_le_bytes());
            group.extend_from_slice(&image);
        }
        fn push_page(group: &mut Vec<u8>, old: &[u8], page: usize, page_size: usize, aux: u16) {
            let start = page * page_size;
            if start + page_size <= old.len() {
                stamp(group, &old[start..start + page_size], page_size, aux);
            }
        }
        let page_size = old_file.page_size.bytes();
        let mut group = Vec::new();
        push_page(
            &mut group,
            old,
            address as usize / page_size,
            page_size,
            aux,
        );
        let compressed = old_file.fcr.file_flags.compression();
        let old_pages = if compressed {
            compressed_chunk_pages(old, old_file, address).unwrap_or_default()
        } else if old_file.fcr.file_flags.variable_length() {
            variable_chunk_pages(old, old_file, address).unwrap_or_default()
        } else {
            Vec::new()
        };
        for page in &old_pages {
            push_page(&mut group, old, *page, page_size, aux);
        }
        if compressed {
            for page in &old_pages {
                push_page(&mut group, old, *page, page_size, aux);
            }
        }
        let new_pages = if compressed {
            compressed_chunk_pages(new, old_file, address).unwrap_or_default()
        } else if old_file.fcr.file_flags.variable_length() {
            variable_chunk_pages(new, old_file, address).unwrap_or_default()
        } else {
            Vec::new()
        };
        for page in new_pages.iter().rev() {
            if let Some(image) = cleared_chunk_page(old, old_file, address, *page, aux) {
                stamp(&mut group, &image, page_size, aux);
            }
        }
        group
    }

    fn excluded_insert_preimage_group(
        &self,
        old_file: &BtrieveFile,
        old: &[u8],
        address: u32,
        aux: u16,
    ) -> Vec<u8> {
        fn stamp(group: &mut Vec<u8>, image: &[u8], page_size: usize, aux: u16) {
            if image.len() < page_size {
                return;
            }
            let mut image = image.to_vec();
            let flags = u16::from_le_bytes([image[4], image[5]]) & 0x8000;
            image[4..6].copy_from_slice(&(flags | aux).to_le_bytes());
            group.extend_from_slice(&image);
        }
        let page_size = old_file.page_size.bytes();
        let mut group = Vec::new();
        let data_start = (address as usize / page_size) * page_size;
        if data_start + page_size <= old.len() {
            stamp(
                &mut group,
                &old[data_start..data_start + page_size],
                page_size,
                aux,
            );
        }
        for page in &old_file.index_pages {
            let start = page.page_number as usize * page_size;
            if start + page_size <= old.len() {
                stamp(&mut group, &old[start..start + page_size], page_size, aux);
            }
        }
        group
    }

    #[allow(clippy::too_many_arguments)]
    fn duplicate_insert_preimage_group(
        &self,
        old_file: &BtrieveFile,
        old: &[u8],
        new_file: &BtrieveFile,
        new: &[u8],
        address: u32,
        selected_key: Option<usize>,
        aux: u16,
    ) -> Vec<u8> {
        fn stamp(group: &mut Vec<u8>, image: &[u8], page_size: usize, aux: u16) {
            if image.len() < page_size {
                return;
            }
            let mut image = image.to_vec();
            let flags = u16::from_le_bytes([image[4], image[5]]) & 0x8000;
            image[4..6].copy_from_slice(&(flags | aux).to_le_bytes());
            group.extend_from_slice(&image);
        }
        let page_size = old_file.page_size.bytes();
        let mut group = Vec::new();
        let data_page = address as usize / page_size;
        let key_only = old_file.fcr.file_flags.key_only();
        if !key_only && (data_page + 1) * page_size <= old.len() {
            stamp(
                &mut group,
                &old[data_page * page_size..(data_page + 1) * page_size],
                page_size,
                aux,
            );
        }
        for page in old_file
            .index_pages
            .iter()
            .filter(|page| selected_key.map_or(true, |key| page.key_index == key))
        {
            let start = page.page_number as usize * page_size;
            if start + page_size <= old.len() {
                stamp(&mut group, &old[start..start + page_size], page_size, aux);
            }
        }
        let logical = old_file.fcr.logical_record_length as usize;
        let physical = old_file.fcr.physical_record_length as usize;
        let slot = address as usize % page_size;
        let mut intermediate = if key_only {
            Vec::new()
        } else if (data_page + 1) * page_size <= old.len() {
            old[data_page * page_size..(data_page + 1) * page_size].to_vec()
        } else if (data_page + 1) * page_size <= new.len() {
            new[data_page * page_size..(data_page + 1) * page_size].to_vec()
        } else {
            Vec::new()
        };
        if !intermediate.is_empty()
            && slot + physical <= intermediate.len()
            && slot + physical <= new.len()
        {
            if old_file.fcr.file_flags.compression() {
                intermediate[slot..slot + physical].fill(0xFF);
            } else {
                intermediate[slot..slot + logical].copy_from_slice(
                    &new[data_page * page_size + slot..data_page * page_size + slot + logical],
                );
                intermediate[slot + logical..slot + physical].fill(0xFF);
            }
        }
        if !intermediate.is_empty() {
            stamp(&mut group, &intermediate, page_size, aux);
        }
        let chunk_pages = if old_file.fcr.file_flags.compression() {
            compressed_chunk_pages(new, old_file, address).unwrap_or_default()
        } else if old_file.fcr.file_flags.variable_length() {
            variable_chunk_pages(new, old_file, address).unwrap_or_default()
        } else {
            Vec::new()
        };
        for page in chunk_pages {
            let start = page * page_size;
            if start + page_size <= old.len() {
                stamp(&mut group, &old[start..start + page_size], page_size, aux);
            }
        }
        for page in new_file
            .index_pages
            .iter()
            .filter(|page| selected_key.map_or(true, |key| page.key_index == key))
        {
            let start = page.page_number as usize * page_size;
            if start + page_size <= new.len() {
                stamp(&mut group, &new[start..start + page_size], page_size, aux);
            }
        }
        group
    }

    fn duplicate_delete_preimage_group(
        &self,
        old_file: &BtrieveFile,
        old: &[u8],
        address: u32,
        aux: u16,
    ) -> Vec<u8> {
        fn stamp(group: &mut Vec<u8>, image: &[u8], page_size: usize, aux: u16) {
            if image.len() < page_size {
                return;
            }
            let mut image = image.to_vec();
            let flags = u16::from_le_bytes([image[4], image[5]]) & 0x8000;
            image[4..6].copy_from_slice(&(flags | aux).to_le_bytes());
            group.extend_from_slice(&image);
        }
        let page_size = old_file.page_size.bytes();
        let data_page = address as usize / page_size;
        let data_image = old
            .get(data_page * page_size..(data_page + 1) * page_size)
            .map(|image| image.to_vec());
        let chunk_pages = if old_file.fcr.file_flags.compression() {
            compressed_chunk_pages(old, old_file, address).unwrap_or_default()
        } else if old_file.fcr.file_flags.variable_length() {
            variable_chunk_pages(old, old_file, address).unwrap_or_default()
        } else {
            Vec::new()
        };
        let duplicate_keys: Vec<usize> = FileSpec::from_file(old_file)
            .map(|spec| {
                spec.keys
                    .iter()
                    .enumerate()
                    .filter(|(_, key)| key.has_duplicates())
                    .map(|(index, _)| index)
                    .collect()
            })
            .unwrap_or_default();
        let mut index_pages: Vec<usize> = old_file
            .index_pages
            .iter()
            .filter(|page| {
                duplicate_keys.contains(&page.key_index)
                    || page
                        .entries
                        .iter()
                        .any(|entry| entry.record_address == address)
            })
            .map(|page| page.page_number as usize)
            .collect();
        index_pages.sort_unstable();
        index_pages.dedup();
        let mut group = Vec::new();
        if let Some(image) = &data_image {
            stamp(&mut group, image, page_size, aux);
        }
        if old_file.fcr.file_flags.compression() {
            for page in &chunk_pages {
                let start = *page * page_size;
                if start + page_size <= old.len() {
                    stamp(&mut group, &old[start..start + page_size], page_size, aux);
                }
            }
        }
        for page in &index_pages {
            let start = *page * page_size;
            if start + page_size <= old.len() {
                stamp(&mut group, &old[start..start + page_size], page_size, aux);
            }
        }
        if let Some(image) = &data_image {
            stamp(&mut group, image, page_size, aux);
        }
        for page in &chunk_pages {
            let start = *page * page_size;
            if start + page_size <= old.len() {
                stamp(&mut group, &old[start..start + page_size], page_size, aux);
            }
        }
        group
    }

    fn key_change_preimage_group(
        &self,
        old_file: &BtrieveFile,
        old: &[u8],
        new_file: &BtrieveFile,
        new: &[u8],
        address: u32,
        aux: u16,
    ) -> Option<Vec<u8>> {
        fn stamp(group: &mut Vec<u8>, image: &[u8], page_size: usize, aux: u16) {
            if image.len() < page_size {
                return;
            }
            let mut image = image.to_vec();
            let flags = u16::from_le_bytes([image[4], image[5]]) & 0x8000;
            image[4..6].copy_from_slice(&(flags | aux).to_le_bytes());
            group.extend_from_slice(&image);
        }
        let page_size = old_file.page_size.bytes();
        let mut group = Vec::new();
        let data_start = (address as usize / page_size) * page_size;
        stamp(
            &mut group,
            &old[data_start..data_start + page_size],
            page_size,
            aux,
        );
        let mut changed = false;
        for index in 0..old_file.fcr.key_defs.len() {
            let old_entry = old_file.index_pages.iter().find_map(|page| {
                (page.key_index == index)
                    .then(|| {
                        page.entries
                            .iter()
                            .find(|entry| entry.record_address == address)
                    })
                    .flatten()
            });
            let new_entry = new_file.index_pages.iter().find_map(|page| {
                (page.key_index == index)
                    .then(|| {
                        page.entries
                            .iter()
                            .find(|entry| entry.record_address == address)
                    })
                    .flatten()
            });
            let (Some(old_entry), Some(new_entry)) = (old_entry, new_entry) else {
                continue;
            };
            let key = &old_file.fcr.key_defs[index];
            let mut old_value = old_entry.key.clone();
            let mut new_value = new_entry.key.clone();
            old_value.truncate(key.length as usize);
            new_value.truncate(key.length as usize);
            if old_value == new_value {
                continue;
            }
            changed = true;
            let old_page = old_file
                .index_pages
                .iter()
                .find(|page| page.key_index == index)?;
            let new_page = new_file
                .index_pages
                .iter()
                .find(|page| page.key_index == index)?;
            let old_start = old_page.page_number as usize * page_size;
            stamp(
                &mut group,
                &old[old_start..old_start + page_size],
                page_size,
                aux,
            );
            if let Some(image) = removed_entry_page_image(old, old_file, index, address, aux) {
                stamp(&mut group, &image, page_size, aux);
            }
            let new_start = new_page.page_number as usize * page_size;
            stamp(
                &mut group,
                &new[new_start..new_start + page_size],
                page_size,
                aux,
            );
        }
        changed.then_some(group)
    }

    fn delete_preimage_group(
        &self,
        old_file: &BtrieveFile,
        old: &[u8],
        address: u32,
        index_images: &[Vec<u8>],
        aux: u16,
    ) -> Vec<u8> {
        fn stamp(group: &mut Vec<u8>, image: &[u8], page_size: usize, aux: u16) {
            if image.len() < page_size {
                return;
            }
            let mut image = image.to_vec();
            let flags = u16::from_le_bytes([image[4], image[5]]) & 0x8000;
            image[4..6].copy_from_slice(&(flags | aux).to_le_bytes());
            group.extend_from_slice(&image);
        }
        fn push_page(group: &mut Vec<u8>, old: &[u8], page: usize, page_size: usize, aux: u16) {
            let start = page * page_size;
            if start + page_size <= old.len() {
                stamp(group, &old[start..start + page_size], page_size, aux);
            }
        }
        let page_size = old_file.page_size.bytes();
        let mut group = Vec::new();
        push_page(
            &mut group,
            old,
            address as usize / page_size,
            page_size,
            aux,
        );
        let compressed_pages = if old_file.fcr.file_flags.compression() {
            compressed_chunk_pages(old, old_file, address).unwrap_or_default()
        } else {
            Vec::new()
        };
        if old_file.fcr.file_flags.compression() {
            for page in &compressed_pages {
                push_page(&mut group, old, *page, page_size, aux);
            }
        }
        for image in index_images {
            stamp(&mut group, image, page_size, aux);
        }
        if old_file.fcr.file_flags.compression() {
            for page in &compressed_pages {
                push_page(&mut group, old, *page, page_size, aux);
            }
        } else if old_file.fcr.file_flags.variable_length() {
            let pages = variable_chunk_pages(old, old_file, address).unwrap_or_default();
            for page in pages {
                push_page(&mut group, old, page, page_size, aux);
            }
        }
        group
    }

    fn failed_insert_block(&mut self, position: &mut PositionBlock, handle: u32) {
        let aux = self
            .files
            .get(&handle)
            .and_then(|file| std::fs::read(&file.path).ok())
            .and_then(|bytes| (bytes.len() >= 6).then(|| u16::from_le_bytes([bytes[4], bytes[5]])))
            .unwrap_or(0);
        position[0] = position::OPEN_MARKER;
        position[1] = 0x02;
        position[2] = 0;
        position[0x33] = 0;
        position[0x34] = handle as u8;
        position[0x35] = 0;
        position[0x36..0x38].copy_from_slice(&aux.to_le_bytes());
        position[0x3E..0x40].copy_from_slice(&aux.to_le_bytes());
    }

    fn update_cursor_block(
        &mut self,
        op: u16,
        key_only: bool,
        position: &mut PositionBlock,
        page_aux_override: Option<u16>,
    ) {
        let Some(reference) = position::decode(position) else {
            return;
        };
        let handle = reference.handle;
        let Some(cursor) = self.cursors.get(&handle).cloned() else {
            return;
        };
        let aux = self
            .files
            .get(&handle)
            .and_then(|file| std::fs::read(&file.path).ok())
            .and_then(|bytes| (bytes.len() >= 6).then(|| u16::from_le_bytes([bytes[4], bytes[5]])))
            .unwrap_or(0);
        let address = if cursor.physical_established {
            cursor.physical_address
        } else {
            cursor.last_address
        };
        let key_only_info = self
            .files
            .get(&handle)
            .and_then(|open| BtrieveFile::open(&open.path).ok())
            .and_then(|file| {
                if !file.fcr.file_flags.key_only() {
                    return None;
                }
                let spec = FileSpec::from_file(&file).ok()?;
                key_only_entry(&file, &spec, cursor.key_number as usize, address)
            });
        let block_op = op + if key_only { 0x32 } else { 0 };
        position[0] = position::OPEN_MARKER;
        position[1] = block_op as u8;
        position[2] = 0;
        if op == 0x04 {
            let display = cursor.delete_replacement.unwrap_or(address);
            position::write_address(&mut position[3..7], (0xFFFF_0000u32) | (display & 0xFFFF));
        } else if matches!(op, 0x02 | 0x28) && key_only_info.is_some() {
            position::write_address(&mut position[3..7], cursor.key_only_display.unwrap_or(0));
        } else if let Some((display, _)) = key_only_info {
            position::write_address(&mut position[3..7], display);
        } else {
            position::write_address(&mut position[3..7], address);
        }
        let get_like = matches!(op, 0x05..=0x0D | 0x17 | 0x24 | 0x25);
        let step_like = matches!(op, 0x18 | 0x21..=0x23 | 0x26 | 0x27);
        if let Some((_, serial)) = key_only_info {
            position::write_address(&mut position[7..0x0B], 0xFFFF_0000u32 | (serial & 0xFFFF));
            position::write_address(&mut position[0x0B..0x0F], 0xFFFF_0000);
        } else if get_like {
            let previous = cursor.duplicate_previous.unwrap_or(0xFFFF_FFFF);
            let next = cursor.duplicate_next.unwrap_or(0xFFFF_FFFF);
            position::write_address(&mut position[7..0x0B], previous);
            position::write_address(&mut position[0x0B..0x0F], next);
        } else if step_like || matches!(op, 0x02 | 0x28) {
            position[0x07] = 0xFF;
            position[0x08] = 0xFF;
            position[0x0B] = 0xFF;
            position[0x0C] = 0xFF;
        }
        if !step_like && !cursor.keyless && !(op == 0x04 && cursor.unindexed_delete) {
            position[0x0F..0x11].copy_from_slice(&cursor.key_number.to_le_bytes());
            let depth = cursor.tree_path.len().saturating_sub(1);
            position[0x11..0x13].copy_from_slice(&(depth as u16).to_le_bytes());
            for (index, page) in cursor.tree_path.iter().enumerate() {
                let offset = 0x13 + index * 4;
                if offset + 4 > position.len() {
                    break;
                }
                position[offset..offset + 2].copy_from_slice(&0u16.to_le_bytes());
                position[offset + 2..offset + 4].copy_from_slice(&(*page as u16).to_le_bytes());
            }
            if let Some(extra) = cursor.path_extra {
                let offset = 0x13 + cursor.tree_path.len() * 4;
                if offset + 4 <= position.len() {
                    position[offset..offset + 2].copy_from_slice(&0u16.to_le_bytes());
                    position[offset + 2..offset + 4].copy_from_slice(&(extra as u16).to_le_bytes());
                }
            }
        }
        position[0x33] = 0;
        position[0x34] = handle as u8;
        position[0x35] = 0;
        position[0x36..0x38].copy_from_slice(&aux.to_le_bytes());
        if op == 0x04 {
            let high = 0xFFFFu16.wrapping_sub((address >> 16) as u16);
            position::write_address(
                &mut position[0x38..0x3C],
                ((high as u32) << 16) | (address & 0xFFFF),
            );
        } else if matches!(op, 0x02 | 0x28) && key_only_info.is_some() {
            position::write_address(
                &mut position[0x38..0x3C],
                cursor.key_only_display.unwrap_or(0),
            );
        } else if let Some((display, _)) = key_only_info {
            position::write_address(&mut position[0x38..0x3C], display);
        } else {
            position::write_address(&mut position[0x38..0x3C], address);
        }
        if matches!(op, 0x02 | 0x28) {
            if !cursor.insert_excluded {
                let value: u16 = if cursor.inserted_duplicate_chains > 0 {
                    0xFFFF
                } else {
                    0
                };
                position[0x3C..0x3E].copy_from_slice(&value.to_le_bytes());
            }
        } else if op == 0x03 && cursor.key_changed {
            position[0x3C..0x3E].fill(0);
        }
        if matches!(op, 0x02 | 0x03 | 0x04 | 0x28) {
            let page_aux = page_aux_override.unwrap_or_else(|| {
                if self.in_transaction() {
                    aux
                } else {
                    aux.wrapping_sub(1)
                }
            });
            position[0x3E..0x40].copy_from_slice(&page_aux.to_le_bytes());
        } else {
            let current = u16::from_le_bytes([position[0x3E], position[0x3F]]) & 0x7FFF;
            let page_aux = self
                .files
                .get(&handle)
                .and_then(|file| std::fs::read(&file.path).ok())
                .and_then(|bytes| {
                    if bytes.len() < 10 {
                        return None;
                    }
                    let page_size = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
                    if page_size == 0 {
                        return None;
                    }
                    let page = if let Some((display, _)) = key_only_info {
                        display as usize / page_size
                    } else {
                        address as usize / page_size
                    };
                    let offset = page * page_size + 4;
                    (offset + 2 <= bytes.len())
                        .then(|| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) & 0x7FFF)
                })
                .unwrap_or(current);
            let flag = if key_only_info.is_some() { 0 } else { 0x8000 };
            position[0x3E..0x40].copy_from_slice(&(page_aux | flag).to_le_bytes());
        }
    }

    fn load(&self, path: &Path) -> Result<Loaded, Status> {
        let raw = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Err(FILE_NOT_FOUND),
            Err(_) => return Err(IO_ERROR),
        };
        let file = BtrieveFile::from_bytes(path, raw.clone())
            .map_err(|error| status::from_format(&error))?;
        let spec = FileSpec::from_file(&file).map_err(|error| status::from_format(&error))?;
        let records = file
            .records()
            .map_err(|error| status::from_format(&error))?
            .into_iter()
            .map(|record| (record.address, record.data))
            .collect();
        Ok(Loaded {
            raw,
            file,
            spec,
            records,
        })
    }

    fn write_rebuilt(
        &mut self,
        path: &Path,
        writer: Writer,
        old_bytes: &[u8],
        page_size: usize,
        mutation: Option<(MutationKind, Option<usize>, &BtrieveFile)>,
    ) -> Result<Vec<u8>, Status> {
        self.claim_file_for_transaction(path)?;
        let mut bytes = writer
            .finish()
            .map_err(|error| status::from_format(&error))?;
        // The extension name field carries the engine's preimage-name
        // residue.  Rebuilds preserve it byte-for-byte.
        if old_bytes.len() >= 0xD8 && bytes.len() >= 0xD8 {
            bytes[0x98..0xD8].copy_from_slice(&old_bytes[0x98..0xD8]);
        }
        if let Some(transaction) = self.current_transaction_mut() {
            if !transaction.snapshots.contains_key(path) {
                let previous = if old_bytes.len() >= 6 {
                    u16::from_le_bytes([old_bytes[4], old_bytes[5]])
                } else {
                    0
                };
                let counter = previous.wrapping_add(1);
                transaction
                    .snapshots
                    .insert(path.to_path_buf(), old_bytes.to_vec());
                transaction
                    .operation_counters
                    .insert(path.to_path_buf(), counter);
            }
            let counter = transaction
                .operation_counters
                .get(path)
                .copied()
                .unwrap_or(0);
            if !transaction.preimage_started.contains(path) {
                start_preimage(path, old_bytes, page_size, counter)
                    .map_err(|_| PREIMAGE_IO_ERROR)?;
                transaction.preimage_started.insert(path.to_path_buf());
                transaction.preimage_pages.insert(path.to_path_buf(), 1);
            }
            // Preimage images are plaintext; an encrypted owner file's live
            // pages are ciphertext.
            let encrypted = image_owner_encrypted(old_bytes);
            let hash = owner_hash_from_image(old_bytes);
            let mut old_view = old_bytes.to_vec();
            let mut new_view = bytes.clone();
            if let (true, Some(hash)) = (encrypted, hash) {
                crypt_owner_pages(&mut old_view, &hash, page_size);
                crypt_owner_pages(&mut new_view, &hash, page_size);
            }
            let images = if let Some((kind, selected, old_file)) = mutation {
                append_operation_images(
                    path, &old_view, &new_view, page_size, kind, counter, old_file, selected,
                )
                .map_err(|_| PREIMAGE_IO_ERROR)?
            } else {
                append_preimage_group(path, &old_view, &new_view, page_size)
                    .map_err(|_| PREIMAGE_IO_ERROR)?
            };
            let pages = transaction
                .preimage_pages
                .get(path)
                .copied()
                .unwrap_or(1)
                .saturating_add(images as u16);
            transaction.preimage_pages.insert(path.to_path_buf(), pages);
            // The DOS engine writes the pre-transaction counter + 1 and
            // FFFF at FCR +0x22 while a transaction is active.
            put_u16(&mut bytes, 0x04, counter);
            put_u16(&mut bytes, 0x22, 0xFFFF);
            set_transaction_markers(&mut bytes, pages);
            if !encrypted {
                for page in changed_pages(old_bytes, &bytes, page_size) {
                    if page * page_size < old_bytes.len() {
                        set_page_aux(&mut bytes, page, page_size, counter);
                    } else {
                        set_page_aux(&mut bytes, page, page_size, 0);
                    }
                }
            }
        } else if !image_owner_encrypted(old_bytes) {
            let pages = bytes.len().div_ceil(page_size);
            for page in 1..pages {
                if page * page_size >= old_bytes.len() {
                    set_page_aux(&mut bytes, page, page_size, 0);
                }
            }
        }
        std::fs::write(path, &bytes).map_err(|_| IO_ERROR)?;
        Ok(bytes)
    }

    /// Finish an in-place mutation.  Outside a transaction this only reloads
    /// the new bytes.  Inside one it writes the operation's preimage images,
    /// pins the FCR counter, and stamps the engine's transaction aux value.
    fn finish_in_place_mutation(
        &mut self,
        path: &Path,
        old: &[u8],
        old_file: &BtrieveFile,
        kind: MutationKind,
        selected_key: Option<usize>,
    ) -> Result<Vec<u8>, Status> {
        self.claim_file_for_transaction(path)?;
        let mut new = std::fs::read(path).map_err(|_| IO_ERROR)?;
        let Some(transaction) = self.current_transaction_mut() else {
            return Ok(new);
        };
        if old.len() < 10 {
            return Err(IO_ERROR);
        }
        let page_size = u16::from_le_bytes([old[8], old[9]]) as usize;
        if page_size == 0 {
            return Err(IO_ERROR);
        }
        let counter = if let Some(counter) = transaction.operation_counters.get(path) {
            *counter
        } else {
            let counter = u16::from_le_bytes([old[4], old[5]]).wrapping_add(1);
            transaction
                .snapshots
                .insert(path.to_path_buf(), old.to_vec());
            transaction
                .operation_counters
                .insert(path.to_path_buf(), counter);
            counter
        };
        if !transaction.preimage_started.contains(path) {
            start_preimage(path, old, page_size, counter).map_err(|_| PREIMAGE_IO_ERROR)?;
            transaction.preimage_started.insert(path.to_path_buf());
            transaction.preimage_pages.insert(path.to_path_buf(), 1);
        }
        // Preimage images and page aux words live in plaintext even for an
        // encrypted owner file; the live file is re-encrypted afterwards.
        let encrypted = old_file.fcr.owner_present && old_file.fcr.owner_mode & 0x02 != 0;
        let old_view = if encrypted {
            old_file.bytes.clone()
        } else {
            old.to_vec()
        };
        let mut new_view = new.clone();
        if encrypted {
            crypt_owner_pages(
                &mut new_view,
                &old_file.fcr.owner_hash,
                old_file.page_size.bytes(),
            );
        }
        let images = append_operation_images(
            path,
            &old_view,
            &new_view,
            page_size,
            kind,
            counter,
            old_file,
            selected_key,
        )
        .map_err(|_| PREIMAGE_IO_ERROR)?;
        let pages = transaction
            .preimage_pages
            .get(path)
            .copied()
            .unwrap_or(1)
            .saturating_add(images as u16);
        transaction.preimage_pages.insert(path.to_path_buf(), pages);
        put_u16(&mut new_view, 0x04, counter);
        put_u16(&mut new_view, 0x22, 0xFFFF);
        set_transaction_markers(&mut new_view, pages);
        for page in changed_pages(&old_view, &new_view, page_size) {
            if page * page_size < old_view.len() {
                set_page_aux(&mut new_view, page, page_size, counter);
            } else {
                // Pages allocated by this operation start with a zero aux;
                // End stamps new data pages and leaves new index pages zero.
                set_page_aux(&mut new_view, page, page_size, 0);
            }
        }
        if encrypted {
            crypt_owner_pages(
                &mut new_view,
                &old_file.fcr.owner_hash,
                old_file.page_size.bytes(),
            );
        }
        new = new_view;
        std::fs::write(path, &new).map_err(|_| IO_ERROR)?;
        Ok(new)
    }

    /// A rejected Update still advances the FCR operation counter once.  The
    /// DOS engine writes the incremented counter even though it leaves the
    /// record and page aux values untouched.
    fn mark_journal_owner(&mut self, handle: u32) {
        let Some(open) = self.files.get(&handle) else {
            return;
        };
        let key = open.share_key.clone();
        if let Some(shared) = self.sharing.get_mut(&key) {
            shared.journal_owner = Some(handle);
        }
    }

    fn failed_operation(&mut self, path: &Path) {
        if self.in_transaction() {
            return;
        }
        let Ok(mut bytes) = std::fs::read(path) else {
            return;
        };
        if bytes.len() < 6 {
            return;
        }
        let counter = u16::from_le_bytes([bytes[4], bytes[5]]).wrapping_add(1);
        put_u16(&mut bytes, 0x04, counter);
        let _ = std::fs::write(path, bytes);
    }

    fn resolve_extension_path(&self, dos: &str) -> Option<PathBuf> {
        let bytes = dos.as_bytes();
        if bytes.len() < 4
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || bytes[2] != b'\\'
        {
            return None;
        }
        let remainder = &dos[3..];
        if remainder.is_empty() {
            return None;
        }
        let base = self
            .session_dir()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut path = base;
        for component in remainder.split('\\') {
            if component.is_empty() {
                continue;
            }
            path.push(component);
        }
        Some(path)
    }

    fn op_extend(&mut self, position: &PositionBlock, key: &mut [u8]) -> Status {
        if self.in_transaction() {
            return OPERATION_NOT_ALLOWED;
        }
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let data_path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let loaded = match self.load(&data_path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        if loaded.file.fcr.extension_name.is_some() {
            return 31;
        }
        let Some(name) = c_string(key) else {
            return INVALID_EXTENSION_NAME;
        };
        if name.is_empty() {
            return INVALID_EXTENSION_NAME;
        }
        let stored = &name[..name.len().min(64)];
        let Some(extension_path) = self.resolve_extension_path(stored) else {
            return INVALID_EXTENSION_NAME;
        };
        if self.canonical(&extension_path) == self.canonical(&data_path) {
            return INVALID_EXTENSION_NAME;
        }
        if let Some(parent) = extension_path.parent() {
            if !parent.as_os_str().is_empty() && std::fs::create_dir_all(parent).is_err() {
                return EXTEND_IO_ERROR;
            }
        }
        let created = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&extension_path);
        if created.is_err() {
            return EXTEND_IO_ERROR;
        }
        drop(created);

        let mut bytes = loaded.raw;
        if bytes.len() < 0xD8 {
            return EXTEND_IO_ERROR;
        }
        // The DOS engine's extension buffer carries the data file's preimage
        // name at +0x9C.  Extend writes the extension path over it, so when
        // the path is shorter than 12 bytes the tail of "<name>.PRE" is still
        // visible in the FCR.  Reproduce that exactly, including the residue.
        let preimage = data_path
            .file_name()
            .map(|name| Path::new(name).with_extension("PRE"))
            .and_then(|path| path.to_str().map(str::to_owned))
            .unwrap_or_default();
        let preimage_bytes = preimage.as_bytes();
        let preimage_length = preimage_bytes.len().min(0xD8 - 0x9C);
        bytes[0x9C..0x9C + preimage_length].copy_from_slice(&preimage_bytes[..preimage_length]);
        let path_bytes = stored.as_bytes();
        bytes[0x98..0x98 + path_bytes.len()].copy_from_slice(path_bytes);
        if path_bytes.len() < 64 {
            bytes[0x98 + path_bytes.len()] = 0;
        }
        let counter = u16::from_le_bytes([bytes[4], bytes[5]]).wrapping_add(2);
        put_u16(&mut bytes, 0x04, counter);
        put_u16(&mut bytes, 0x24, 2);
        if std::fs::write(&data_path, bytes).is_err() {
            return EXTEND_IO_ERROR;
        }
        SUCCESS
    }

    #[allow(clippy::too_many_arguments)]
    fn op_extended_get(
        &mut self,
        previous: bool,
        lock_bias: u8,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        key: &mut [u8],
        key_number: i16,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let capacity = (*data_len as usize).min(data.len());
        let request = match extended::Request::parse(data, capacity) {
            Ok(request) => request,
            Err(status) => return status,
        };
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        if loaded.spec.keys.is_empty() || key_number < 0 {
            return INVALID_KEY_NUMBER;
        }
        let key_index = key_number as usize;
        let Some(definition) = loaded.spec.keys.get(key_index) else {
            return INVALID_KEY_NUMBER;
        };
        if key.len() < definition.length as usize {
            return KEY_BUFFER_TOO_SHORT;
        }
        let view = match build_view(&loaded.file, &loaded.spec, key_index as u16) {
            Ok(view) => view,
            Err(status) => return status,
        };
        let mut cursor = self.cursor(position, handle);
        if !cursor.established {
            return INVALID_POSITIONING;
        }
        if cursor.key_number as usize != key_index {
            return DIFFERENT_KEY_NUMBER;
        }
        let start = if previous {
            last_before(
                &view,
                &cursor.last_key,
                cursor.last_address,
                &loaded.spec,
                key_index as u16,
            )
        } else {
            let index = first_after(
                &view,
                &cursor.last_key,
                cursor.last_address,
                &loaded.spec,
                key_index as u16,
            );
            (index < view.len()).then_some(index)
        };
        let Some(start) = start else {
            return self.extended_output(data, data_len, capacity, &[], END_OF_FILE);
        };
        let order: Vec<usize> = if previous {
            (0..=start).rev().collect()
        } else {
            (start..view.len()).collect()
        };
        let reject_limit = if request.max_reject == 0 {
            4095
        } else {
            request.max_reject as u32
        };
        let mut blocks: Vec<(u32, Vec<u8>)> = Vec::new();
        let mut rejected = 0u32;
        let mut status = END_OF_FILE;
        let mut last_examined = None;
        let mut last_returned_key: Option<Vec<u8>> = None;
        let mut failed_extraction = false;
        for index in order {
            let entry = &view[index];
            last_examined = Some(index);
            match request.matches(&entry.data, loaded.spec.acs.as_ref()) {
                Ok(false) => {
                    rejected += 1;
                    if rejected >= reject_limit {
                        status = REJECT_COUNT_REACHED;
                        break;
                    }
                }
                Err(error) => {
                    status = error;
                    break;
                }
                Ok(true) => {
                    let image = match request.extract(&entry.data) {
                        Ok(image) => image,
                        Err(error) => {
                            status = error;
                            failed_extraction = true;
                            break;
                        }
                    };
                    if let Err(error) = self.check_lock(&path, entry.address, handle, lock_bias) {
                        status = error;
                        break;
                    }
                    blocks.push((entry.address, image));
                    last_returned_key = Some(entry.key.clone());
                    if blocks.len() as u16 >= request.record_limit {
                        status = SUCCESS;
                        break;
                    }
                }
            }
        }
        if let Some(index) = last_examined {
            let entry = &view[index];
            cursor.handle = handle;
            cursor.key_number = key_index as u16;
            cursor.last_key = entry.key.clone();
            cursor.last_address = entry.address;
            cursor.established = true;
            cursor.physical_address = entry.address;
            cursor.physical_established = true;
            let equal = |left: &[u8], right: &[u8]| {
                definition.compare(left, right, loaded.spec.acs.as_ref()) == Ordering::Equal
            };
            cursor.duplicate_previous = if index > 0 && equal(&view[index - 1].key, &entry.key) {
                Some(view[index - 1].address)
            } else {
                None
            };
            cursor.duplicate_next =
                if index + 1 < view.len() && equal(&view[index + 1].key, &entry.key) {
                    Some(view[index + 1].address)
                } else {
                    None
                };
            cursor.delete_replacement = None;
            cursor.path_extra = None;
            cursor.tree_path = index_tree_path(&loaded.file, &loaded.spec, key_index, &entry.key);
            self.store_cursor(position, cursor);
        }
        if let Some(key_value) = last_returned_key {
            if key_value.len() <= key.len() {
                key[..key_value.len()].copy_from_slice(&key_value);
            }
        }
        let result = self.extended_output(data, data_len, capacity, &blocks, status);
        if failed_extraction && *data_len >= 2 {
            let count = u16::from_le_bytes([data[0], data[1]]).wrapping_add(1);
            data[0..2].copy_from_slice(&count.to_le_bytes());
        }
        result
    }

    fn op_extended_step(
        &mut self,
        previous: bool,
        lock_bias: u8,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let capacity = (*data_len as usize).min(data.len());
        let request = match extended::Request::parse(data, capacity) {
            Ok(request) => request,
            Err(status) => return status,
        };
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        let view = match build_physical_view(&loaded.file) {
            Ok(view) => view,
            Err(status) => return status,
        };
        let mut cursor = self.cursor(position, handle);
        let start = if previous {
            if !cursor.physical_established {
                None
            } else {
                let count = view.partition_point(|entry| entry.address < cursor.physical_address);
                count.checked_sub(1)
            }
        } else {
            let index = if cursor.physical_established {
                view.partition_point(|entry| entry.address <= cursor.physical_address)
            } else {
                0
            };
            (index < view.len()).then_some(index)
        };
        let Some(start) = start else {
            return self.extended_output(data, data_len, capacity, &[], END_OF_FILE);
        };
        let order: Vec<usize> = if previous {
            (0..=start).rev().collect()
        } else {
            (start..view.len()).collect()
        };
        let reject_limit = if request.max_reject == 0 {
            4095
        } else {
            request.max_reject as u32
        };
        let mut blocks: Vec<(u32, Vec<u8>)> = Vec::new();
        let mut rejected = 0u32;
        let mut status = END_OF_FILE;
        let mut last_examined = None;
        let mut failed_extraction = false;
        for index in order {
            let entry = &view[index];
            last_examined = Some(entry.address);
            match request.matches(&entry.data, loaded.spec.acs.as_ref()) {
                Ok(false) => {
                    rejected += 1;
                    if rejected >= reject_limit {
                        status = REJECT_COUNT_REACHED;
                        break;
                    }
                }
                Err(error) => {
                    status = error;
                    break;
                }
                Ok(true) => {
                    let image = match request.extract(&entry.data) {
                        Ok(image) => image,
                        Err(error) => {
                            status = error;
                            failed_extraction = true;
                            break;
                        }
                    };
                    if let Err(error) = self.check_lock(&path, entry.address, handle, lock_bias) {
                        status = error;
                        break;
                    }
                    blocks.push((entry.address, image));
                    if blocks.len() as u16 >= request.record_limit {
                        status = SUCCESS;
                        break;
                    }
                }
            }
        }
        if let Some(address) = last_examined {
            cursor.handle = handle;
            cursor.physical_address = address;
            cursor.physical_established = true;
            self.store_cursor(position, cursor);
        }
        let result = self.extended_output(data, data_len, capacity, &blocks, status);
        if failed_extraction && *data_len >= 2 {
            let count = u16::from_le_bytes([data[0], data[1]]).wrapping_add(1);
            data[0..2].copy_from_slice(&count.to_le_bytes());
        }
        result
    }

    fn extended_output(
        &self,
        data: &mut [u8],
        data_len: &mut u16,
        capacity: usize,
        blocks: &[(u32, Vec<u8>)],
        status: Status,
    ) -> Status {
        let (written, error) = extended::write_result(data, capacity, blocks);
        *data_len = written as u16;
        error.unwrap_or(status)
    }

    fn op_extended_insert(
        &mut self,
        position: &PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        key: &mut [u8],
        key_number: i16,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        let capacity = (*data_len as usize).min(data.len());
        if capacity < 2 {
            return DATA_BUFFER_LENGTH;
        }
        let count = u16::from_le_bytes([data[0], data[1]]) as usize;
        let mut cursor = 2usize;
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            if cursor + 2 > capacity {
                return DATA_BUFFER_LENGTH;
            }
            let length = u16::from_le_bytes([data[cursor], data[cursor + 1]]) as usize;
            cursor += 2;
            if cursor + length > capacity {
                return DATA_BUFFER_LENGTH;
            }
            records.push(data[cursor..cursor + length].to_vec());
            cursor += length;
        }
        if key_number < 0 {
            return INVALID_KEY_NUMBER;
        }
        if !loaded.spec.keys.is_empty() {
            let key_index = key_number as usize;
            let Some(definition) = loaded.spec.keys.get(key_index) else {
                return INVALID_KEY_NUMBER;
            };
            if key.len() < definition.length as usize {
                return KEY_BUFFER_TOO_SHORT;
            }
        }
        let mut inserted = 0u16;
        let mut status = SUCCESS;
        let mut last_record: Option<Vec<u8>> = None;
        for record in records {
            let mut buffer = record;
            let length = buffer.len() as u16;
            let result = self.op_insert(position, &mut buffer, length as usize, key_number);
            if result != SUCCESS {
                status = result;
                break;
            }
            inserted += 1;
            last_record = Some(buffer);
        }
        data[0..2].copy_from_slice(&inserted.to_le_bytes());
        *data_len = 2;
        if let Some(record) = last_record {
            if !loaded.spec.keys.is_empty() {
                let key_index = key_number as usize;
                if let Some(definition) = loaded.spec.keys.get(key_index) {
                    if let Ok(key_value) = definition.build_key(&record) {
                        if key_value.len() <= key.len() {
                            key[..key_value.len()].copy_from_slice(&key_value);
                        }
                    }
                }
            }
        }
        status
    }

    fn check_writable(&self, handle: u32) -> Result<(), Status> {
        let file = self.open_file(handle)?;
        if file.read_only {
            return Err(OPERATION_NOT_ALLOWED);
        }
        Ok(())
    }

    /// The single entry point used by the C ABI.  `data_len` is the buffer
    /// capacity on input and the returned length for operations that produce
    /// data.
    pub fn call(
        &mut self,
        raw_op: u16,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        key: &mut [u8],
        key_number: i16,
    ) -> Status {
        self.call_for_session(
            DEFAULT_SESSION,
            raw_op,
            position,
            data,
            data_len,
            key,
            key_number,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn call_for_session(
        &mut self,
        session: SessionId,
        raw_op: u16,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        key: &mut [u8],
        key_number: i16,
    ) -> Status {
        self.call_for_session_at_path(
            session, None, raw_op, position, data, data_len, key, key_number,
        )
    }

    /// Session-scoped call with an optional resolved path for Open/Create.
    /// The path override avoids the DOS filename parser, which stops at the
    /// first space, for Unix paths supplied by a server transport.
    #[allow(clippy::too_many_arguments)]
    pub fn call_for_session_at_path(
        &mut self,
        session: SessionId,
        path_override: Option<&Path>,
        raw_op: u16,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        key: &mut [u8],
        key_number: i16,
    ) -> Status {
        self.active_session = session;
        let Some((op, lock_bias, key_only)) = normalize_op(raw_op) else {
            return INVALID_OPERATION;
        };
        if matches!(
            op,
            0x02..=0x10 | 0x16..=0x18 | 0x1D..=0x20 | 0x21..=0x28
        ) {
            if let Some(status) = self.check_position_transaction(position) {
                return status;
            }
        }

        let status = match op {
            0x00 => self.op_open(
                path_override,
                position,
                data,
                *data_len as usize,
                key,
                key_number,
            ),
            0x01 => self.op_close(position),
            0x02 => self.op_insert(position, data, *data_len as usize, key_number),
            0x03 => self.op_update(position, data, *data_len as usize, data_len),
            0x04 => self.op_delete(position, data_len),
            0x05..=0x0D => self.op_get(
                op, position, data, data_len, key, key_number, lock_bias, key_only,
            ),
            0x0E => self.op_create(path_override, data, *data_len as usize, key, key_number),
            0x0F => self.op_stat(position, data, data_len, key),
            0x10 => self.op_extend(position, key),
            0x11 => self.op_set_directory(key),
            0x12 => self.op_get_directory(data, data_len, key, key_number),
            0x13 => self.op_begin_transaction(),
            0x14 => self.op_end_transaction(),
            0x15 => self.op_abort_transaction(),
            0x16 => self.op_get_position(position, data, data_len),
            0x17 => self.op_get_direct(position, data, data_len, key_number),
            0x18 => self.op_step(0x06, position, data, data_len, key, key_number, lock_bias),
            0x19 => {
                self.reset_session(self.active_session);
                SUCCESS
            }
            0x1A => self.op_version(data, data_len),
            0x1B => self.op_unlock(position, data, *data_len as usize),
            0x1C => {
                self.reset_session(self.active_session);
                SUCCESS
            }
            0x1D => self.op_set_owner(position, data, *data_len as usize, key, key_number),
            0x1E => self.op_clear_owner(position),
            0x1F => self.op_create_supplemental(position, data, *data_len as usize),
            0x20 => self.op_drop_supplemental(position, key_number),
            0x21 => self.op_step(0x0C, position, data, data_len, key, key_number, lock_bias),
            0x22 => self.op_step(0x0D, position, data, data_len, key, key_number, lock_bias),
            0x23 => self.op_step(0x07, position, data, data_len, key, key_number, lock_bias),
            0x24 => {
                self.op_extended_get(false, lock_bias, position, data, data_len, key, key_number)
            }
            0x25 => {
                self.op_extended_get(true, lock_bias, position, data, data_len, key, key_number)
            }
            0x26 => self.op_extended_step(false, lock_bias, position, data, data_len),
            0x27 => self.op_extended_step(true, lock_bias, position, data, data_len),
            0x28 => self.op_extended_insert(position, data, data_len, key, key_number),
            _ => INVALID_OPERATION,
        };
        if status == SUCCESS
            && matches!(
                op,
                0x02..=0x04
                    | 0x05..=0x0D
                    | 0x17
                    | 0x18
                    | 0x21..=0x28
            )
        {
            self.update_cursor_block(op, key_only, position, None);
            if matches!(op, 0x02 | 0x03 | 0x04 | 0x28) {
                if let Some(reference) = position::decode(position) {
                    if let Some(cursor) = self.cursors.get(&reference.handle).cloned() {
                        if let Some(open) = self.files.get(&reference.handle) {
                            let path = open.path.clone();
                            self.update_open_marker(&path, op, &cursor);
                        }
                    }
                }
            }
        } else if status == DATA_BUFFER_LENGTH && op == 0x03 {
            let aux = position::decode(position)
                .and_then(|reference| self.files.get(&reference.handle))
                .and_then(|file| std::fs::read(&file.path).ok())
                .and_then(|bytes| {
                    (bytes.len() >= 6).then(|| u16::from_le_bytes([bytes[4], bytes[5]]))
                })
                .unwrap_or(0);
            self.update_cursor_block(op, key_only, position, Some(aux));
        } else if status == DATA_BUFFER_LENGTH && op == 0x02 {
            if let Some(reference) = position::decode(position) {
                if self.files.contains_key(&reference.handle) {
                    self.failed_insert_block(position, reference.handle);
                }
            }
        } else if status == END_OF_FILE && matches!(op, 0x24..=0x27) {
            self.update_cursor_block(op, key_only, position, None);
        }
        status
    }

    // ---------------------------------------------------------------- Open

    fn op_open(
        &mut self,
        path_override: Option<&Path>,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: usize,
        key: &mut [u8],
        key_number: i16,
    ) -> Status {
        let path = match path_override {
            Some(path) => path.to_path_buf(),
            None => {
                let name = match c_string(key) {
                    Some(name) if !name.is_empty() => name,
                    _ => return INVALID_FILE_NAME,
                };
                self.resolve(&name)
            }
        };
        let owner_bytes = &data[..data_len.min(data.len())];
        let owner_supplied = data_len > 0;
        let owner = c_string_prefix(owner_bytes);
        if !path.is_file() {
            return FILE_NOT_FOUND;
        }
        let share_key = self.canonical(&path);
        if let Err(status) = self.check_transaction_access(&share_key) {
            return status;
        }

        // The engine recovers an interrupted transaction before parsing.
        match btr_format::recover_file(&path) {
            Ok(_) => {}
            Err(FormatError::MissingPreimage) => return PREIMAGE_OPEN_ERROR,
            Err(FormatError::InvalidPreimage(_)) => return PREIMAGE_IO_ERROR,
            Err(error) => return status::from_format(&error),
        }

        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        if let Some(extension) = &loaded.file.fcr.extension_name {
            match self.resolve_extension_path(extension) {
                Some(extension_path) if extension_path.is_file() => {}
                _ => return EXTENDED_FILE_ERROR,
            }
        }
        let mut read_only = key_number == -2;
        if loaded.file.fcr.owner_present {
            let stored_mode = loaded.file.fcr.owner_mode;
            if owner_supplied {
                let candidate = OwnerInfo::from_name(owner.as_bytes(), stored_mode);
                if candidate.hash != loaded.file.fcr.owner_hash {
                    return INVALID_OWNER;
                }
            } else if stored_mode & 0x01 == 0 {
                return INVALID_OWNER;
            } else {
                read_only = true;
            }
        }

        if self.files.len() >= MAX_OPEN_FILES {
            return FILE_TABLE_FULL;
        }
        let accelerated = key_number == -1;
        let exclusive = key_number == -4;
        if let Some(shared) = self.sharing.get(&share_key) {
            if shared.exclusive || exclusive {
                return FILE_IN_USE;
            }
            if shared.accelerated != accelerated {
                return INCOMPATIBLE_MODE;
            }
        }

        let Some(handle) =
            (1..=MAX_OPEN_FILES as u32).find(|handle| !self.files.contains_key(handle))
        else {
            return FILE_TABLE_FULL;
        };
        let entry = self.sharing.entry(share_key.clone()).or_insert(ShareState {
            handles: 0,
            exclusive,
            accelerated,
            journal_owner: None,
        });
        entry.handles += 1;
        entry.exclusive = exclusive;
        entry.accelerated = accelerated;
        self.files.insert(
            handle,
            OpenFile {
                path,
                share_key,
                read_only,
                session: self.active_session,
            },
        );

        let mut cursor = Cursor::new(handle);
        if loaded.file.bytes.len() >= 0x72 {
            cursor.preimage_pages =
                u16::from_le_bytes([loaded.file.bytes[0x70], loaded.file.bytes[0x71]]) as u32;
        }
        cursor.keyless = loaded.spec.keys.is_empty();
        self.cursors.insert(handle, cursor);
        position::encode_open(position, handle, loaded.file.fcr.aux0);
        SUCCESS
    }

    fn op_close(&mut self, position: &mut PositionBlock) -> Status {
        if self.in_transaction() {
            return OPERATION_NOT_ALLOWED;
        }
        let Ok(handle) = self.handle_raw(position) else {
            return FILE_NOT_OPEN;
        };
        let Some(open) = self.files.remove(&handle) else {
            return FILE_NOT_OPEN;
        };
        self.cursors.retain(|_, cursor| cursor.handle != handle);
        self.record_locks.retain(|_, owner| *owner != handle);
        let cleanup = if let Some(shared) = self.sharing.get_mut(&open.share_key) {
            shared.handles = shared.handles.saturating_sub(1);
            let cleanup = shared.handles == 0 && shared.journal_owner == Some(handle);
            if shared.handles == 0 {
                self.sharing.remove(&open.share_key);
            }
            cleanup
        } else {
            false
        };
        if cleanup {
            if let Ok(mut bytes) = std::fs::read(&open.path) {
                clear_open_marker(&mut bytes);
                let _ = std::fs::write(&open.path, bytes);
            }
            let target = BtrieveFile::open(&open.path)
                .map(|file| self.preimage_target(&open.path, &file))
                .unwrap_or_else(|_| preimage_path(&open.path));
            let _ = std::fs::remove_file(target);
        }
        if let Ok(bytes) = std::fs::read(&open.path) {
            if bytes.len() >= 6 {
                position[0x36..0x38].copy_from_slice(&bytes[4..6]);
            }
        }
        position::close(position);
        SUCCESS
    }

    // ---------------------------------------------------------------- Create

    fn op_create(
        &mut self,
        path_override: Option<&Path>,
        data: &mut [u8],
        data_len: usize,
        key: &mut [u8],
        key_number: i16,
    ) -> Status {
        let spec = match create::parse_create_spec(&data[..data_len.min(data.len())]) {
            Ok(spec) => spec,
            Err(status) => return status,
        };
        let path = match path_override {
            Some(path) => path.to_path_buf(),
            None => {
                let name = match c_string(key) {
                    Some(name) if !name.is_empty() => name,
                    _ => return INVALID_FILE_NAME,
                };
                self.resolve(&name)
            }
        };
        let share_key = self.canonical(&path);
        if let Err(status) = self.check_transaction_access(&share_key) {
            return status;
        }
        if key_number == -1 && path.exists() {
            return FILE_ALREADY_EXISTS;
        }
        let bytes = match spec.create_empty() {
            Ok(bytes) => bytes,
            Err(error) => return status::from_format(&error),
        };
        if std::fs::write(&path, bytes).is_err() {
            return CREATE_IO_ERROR;
        }
        SUCCESS
    }

    // ---------------------------------------------------------------- Stat

    fn op_stat(
        &mut self,
        position: &PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        key: &mut [u8],
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        let segments: usize = loaded.spec.keys.iter().map(|key| key.segments.len()).sum();
        let acs_size = if loaded.file.acs.is_some() {
            Acs::SIZE
        } else {
            0
        };
        let required = 16 + segments * 16 + acs_size;
        let capacity = (*data_len as usize).min(data.len());
        if capacity < required {
            return DATA_BUFFER_LENGTH;
        }
        data[..required].fill(0);
        let record_length = if loaded.file.fcr.file_flags.compression() {
            loaded.file.fcr.compressed_record_length
        } else {
            loaded.file.fcr.logical_record_length
        };
        put_u16(data, 0, record_length);
        put_u16(data, 2, loaded.file.page_size.as_u16());
        put_u16(data, 4, loaded.spec.keys.len() as u16);
        put_u32(data, 6, loaded.file.fcr.record_count);
        put_u16(data, 10, loaded.file.fcr.file_flags.raw());
        let unused = loaded
            .file
            .page_count
            .saturating_sub(loaded.file.fcr.total_pages as u32) as u16;
        put_u16(data, 14, unused);
        let stats = loaded.file.stat();
        let mut offset = 16;
        for (key_index, key_spec) in loaded.spec.keys.iter().enumerate() {
            let unique = stats
                .keys
                .get(key_index)
                .map(|stat| stat.unique_key_count as u32)
                .unwrap_or(0);
            for segment in &key_spec.segments {
                put_u16(data, offset, segment.position);
                put_u16(data, offset + 2, segment.length);
                put_u16(data, offset + 4, segment.flags | 0x0100);
                put_u32(data, offset + 6, unique);
                data[offset + 10] = segment.key_type;
                data[offset + 11] = segment.null_value;
                offset += 16;
            }
        }
        if let Some(acs) = &loaded.file.acs {
            data[offset..offset + acs.raw.len()].copy_from_slice(&acs.raw);
        }
        *data_len = required as u16;
        if !key.is_empty() {
            key[0] = 0;
        }
        SUCCESS
    }

    // ------------------------------------------------------------ Insert

    fn op_insert(
        &mut self,
        position: &PositionBlock,
        data: &mut [u8],
        data_len: usize,
        key_number: i16,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        if let Err(status) = self.claim_file_for_transaction(&path) {
            return status;
        }
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        self.mark_journal_owner(handle);
        if data_len > data.len() {
            return DATA_BUFFER_LENGTH;
        }
        let mut record = data[..data_len].to_vec();
        assign_autoincrement(&mut record, &loaded.spec, &loaded.records);
        let inserted_duplicate_chains = loaded
            .spec
            .keys
            .iter()
            .filter(|key| key.has_duplicates())
            .filter(|key| !key.is_excluded_from_index(&record))
            .filter(|key| {
                let value = match key.build_key(&record) {
                    Ok(value) => value,
                    Err(_) => return false,
                };
                loaded.records.iter().any(|(_, existing)| {
                    key.build_key(existing)
                        .map(|existing| {
                            key.compare(&existing, &value, loaded.spec.acs.as_ref())
                                == Ordering::Equal
                        })
                        .unwrap_or(false)
                })
            })
            .count();
        let selected_excluded = choose_key_number(&loaded.spec, key_number)
            .or(Some(0))
            .map(|index| {
                loaded
                    .spec
                    .keys
                    .get(index)
                    .map(|key| key.is_excluded_from_index(&record))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        // A simple fixed single-key file is inserted in place, matching the
        // DOS engine's free-slot reuse and operation counters.
        let in_place_address = match insert_fixed_at_in_place(&path, &record) {
            Ok(Some(address)) => {
                let selected = choose_key_number(&loaded.spec, key_number).or(Some(0));
                if let Err(status) = self.finish_in_place_mutation(
                    &path,
                    &loaded.raw,
                    &loaded.file,
                    MutationKind::Insert,
                    selected,
                ) {
                    return status;
                }
                Some(address)
            }
            Ok(None) => None,
            Err(error) => {
                let status = status::from_format(&error);
                self.failed_operation(&path);
                if status == DATA_BUFFER_LENGTH {
                    let _ = std::fs::write(preimage_path(&path), b"");
                }
                return status;
            }
        };
        let new_address = if let Some(address) = in_place_address {
            address
        } else {
            let mut records: Vec<Vec<u8>> = loaded
                .records
                .iter()
                .map(|(_, record)| record.clone())
                .collect();
            let old_addresses: std::collections::HashSet<u32> =
                loaded.records.iter().map(|(address, _)| *address).collect();
            records.push(record.clone());
            let writer = match Writer::from_records(loaded.spec.clone(), &records) {
                Ok(writer) => writer,
                Err(error) => {
                    let status = status::from_format(&error);
                    self.failed_operation(&path);
                    if status == DATA_BUFFER_LENGTH {
                        let _ = std::fs::write(preimage_path(&path), b"");
                    }
                    return status;
                }
            };
            let bytes = match self.write_rebuilt(
                &path,
                writer,
                &loaded.raw,
                loaded.file.page_size.bytes(),
                Some((
                    MutationKind::Insert,
                    choose_key_number(&loaded.spec, key_number).or(Some(0)),
                    &loaded.file,
                )),
            ) {
                Ok(bytes) => bytes,
                Err(status) => {
                    if status == DATA_BUFFER_LENGTH {
                        let _ = std::fs::write(preimage_path(&path), b"");
                    }
                    return status;
                }
            };
            let _ = bytes;
            let reloaded = BtrieveFile::from_bytes(&path, std::fs::read(&path).unwrap_or_default())
                .ok()
                .and_then(|file| file.records().ok());
            reloaded
                .as_ref()
                .and_then(|records| {
                    records
                        .iter()
                        .find(|candidate| {
                            candidate.data == record && !old_addresses.contains(&candidate.address)
                        })
                        .map(|candidate| candidate.address)
                })
                .or_else(|| {
                    reloaded.as_ref().and_then(|records| {
                        records
                            .iter()
                            .find(|candidate| !old_addresses.contains(&candidate.address))
                            .map(|candidate| candidate.address)
                    })
                })
                .unwrap_or(0)
        };
        if in_place_address.is_none()
            && !self.in_transaction()
            && loaded.file.fcr.file_flags.variable_length()
            && !loaded.file.fcr.file_flags.compression()
            && record.len() > loaded.file.fcr.logical_record_length as usize
        {
            let counter = u16::from_le_bytes([loaded.raw[4], loaded.raw[5]]).wrapping_add(2);
            if let Err(error) =
                touch_variable_update(&path, new_address, counter, loaded.file.page_count as usize)
            {
                return status::from_format(&error);
            }
        }

        if !self.in_transaction() {
            let selected = choose_key_number(&loaded.spec, key_number).or(Some(0));
            if let Err(status) = self.write_transient_preimage(
                &path,
                &loaded.file.bytes,
                &loaded.file,
                MutationKind::Insert,
                selected,
                new_address,
                &[],
                selected_excluded,
            ) {
                return status;
            }
        }

        // Return the assigned autoincrement values to the caller.
        let copy = record.len().min(data.len());
        data[..copy].copy_from_slice(&record[..copy]);
        let key_index = choose_key_number(&loaded.spec, key_number);
        let mut cursor = self.cursor(position, handle);
        cursor.handle = handle;
        cursor.inserted_duplicate_chains = inserted_duplicate_chains;
        cursor.insert_excluded = selected_excluded;
        let inserted_events = if let Ok(new_file) = BtrieveFile::open(&path) {
            let changed = changed_index_page_counts(&loaded.file, &new_file);
            let old_index_pages: usize = changed.iter().sum();
            let new_primary_page = usize::from(!loaded.spec.keys.is_empty());
            let chunk_pages = if loaded.file.fcr.file_flags.compression()
                || loaded.file.fcr.file_flags.variable_length()
            {
                journal_chunk_count(&new_file.bytes, &new_file, new_address)
            } else {
                0
            };
            let data_page = usize::from(!loaded.file.fcr.file_flags.key_only());
            if selected_excluded {
                (data_page + loaded.file.index_pages.len()) as u32
            } else {
                let duplicate_page = usize::from(
                    inserted_duplicate_chains > 0 && !loaded.file.fcr.file_flags.key_only(),
                );
                (data_page + old_index_pages + new_primary_page + chunk_pages + duplicate_page)
                    as u32
            }
        } else {
            0
        };
        if !self.in_transaction() {
            cursor.preimage_pages = cursor.preimage_pages.max(inserted_events);
        }
        cursor.delete_replacement = None;
        if let Some(index) = key_index {
            if let Ok(key) = loaded.spec.keys[index].build_key(&record) {
                cursor.physical_address = new_address;
                cursor.physical_established = true;
                if !loaded.spec.keys[index].is_excluded_from_index(&record) {
                    cursor.key_number = index as u16;
                    cursor.last_key = key;
                    cursor.last_address = new_address;
                    cursor.established = true;
                } else {
                    cursor.established = false;
                }
            }
        } else {
            cursor.key_number = 0;
            cursor.last_key.clear();
            cursor.last_address = new_address;
            cursor.established = true;
            cursor.physical_address = new_address;
            cursor.physical_established = true;
        }
        if let Some(index) = key_index {
            if let Ok(file) = BtrieveFile::open(&path) {
                if let Ok(spec) = FileSpec::from_file(&file) {
                    if let Some(key) = spec.keys.get(index) {
                        if let Ok(value) = key.build_key(&record) {
                            cursor.tree_path = index_tree_path(&file, &spec, index, &value);
                            cursor.path_extra = None;
                        }
                    }
                }
            }
        }
        self.store_cursor(position, cursor);
        SUCCESS
    }

    // ------------------------------------------------------------ Update

    fn op_update(
        &mut self,
        position: &PositionBlock,
        data: &mut [u8],
        data_len: usize,
        _out_len: &mut u16,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        if let Err(status) = self.claim_file_for_transaction(&path) {
            return status;
        }
        let cursor = self.cursor(position, handle);
        if !cursor.established && !cursor.physical_established {
            return INVALID_POSITIONING;
        }
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        let target_address = if cursor.physical_established {
            cursor.physical_address
        } else {
            cursor.last_address
        };
        let Some(index) = loaded
            .records
            .iter()
            .position(|(address, _)| *address == target_address)
        else {
            return INVALID_POSITIONING;
        };
        self.mark_journal_owner(handle);
        // Key-only files are not updatable or deletable.  The rejected
        // operation still advances the FCR counter once.
        if loaded.file.fcr.file_flags.key_only() {
            let _ = std::fs::write(preimage_path(&path), b"");
            self.failed_operation(&path);
            return OPERATION_NOT_ALLOWED;
        }
        if loaded.spec.keys.iter().any(|key| key.is_manual()) {
            let _ = std::fs::write(preimage_path(&path), b"");
            self.failed_operation(&path);
            if let Ok(mut bytes) = std::fs::read(&path) {
                put_u16(&mut bytes, 0x22, 0xFFFF);
                let _ = std::fs::write(&path, bytes);
            }
            self.files.remove(&handle);
            self.cursors.retain(|_, cursor| cursor.handle != handle);
            self.record_locks.retain(|_, owner| *owner != handle);
            return 0xFFFF;
        }
        // The DOS engine leaves the status word untouched (FFFF), advances
        // the operation counter, creates an empty preimage, and leaves the
        // record bytes unchanged for a zero-key file.
        if loaded.spec.keys.is_empty() {
            let _ = std::fs::write(preimage_path(&path), b"");
            self.failed_operation(&path);
            if let Ok(mut bytes) = std::fs::read(&path) {
                put_u16(&mut bytes, 0x22, 0xFFFF);
                let _ = std::fs::write(&path, bytes);
            }
            // This rejected data-only update leaves the engine's handle in a
            // closed-looking state: Close reports status 3 and does not clear
            // the preimage or +0x70.
            self.files.remove(&handle);
            self.cursors.retain(|_, cursor| cursor.handle != handle);
            self.record_locks.retain(|_, owner| *owner != handle);
            return 0xFFFF;
        }
        if data_len > data.len() {
            return DATA_BUFFER_LENGTH;
        }
        let supplied = data[..data_len].to_vec();
        let old_record = loaded.records[index].1.clone();
        // A short Update copies only the supplied prefix over the current
        // record; the engine's key check sees the remaining old bytes.
        let mut comparison = old_record.clone();
        let overlap = supplied.len().min(comparison.len());
        comparison[..overlap].copy_from_slice(&supplied[..overlap]);
        for key in &loaded.spec.keys {
            if key.flags & 0x0002 == 0 {
                let old_value = key.build_key(&old_record);
                let new_value = key.build_key(&comparison);
                if let (Ok(old), Ok(new)) = (old_value, new_value) {
                    if old != new {
                        self.failed_operation(&path);
                        return MODIFIABLE_KEY_ERROR;
                    }
                }
            }
        }
        let logical = loaded.file.fcr.logical_record_length as usize;
        let fixed = !loaded.file.fcr.file_flags.compression()
            && !loaded.file.fcr.file_flags.variable_length();
        if fixed && supplied.len() < logical {
            // The engine advances the operation counter before discovering
            // that the caller's record image is too short.
            let _ = std::fs::write(preimage_path(&path), b"");
            self.failed_operation(&path);
            return DATA_BUFFER_LENGTH;
        }
        let new_record = if fixed {
            supplied[..logical.min(supplied.len())].to_vec()
        } else if loaded.file.fcr.file_flags.compression()
            && !loaded.file.fcr.file_flags.variable_length()
        {
            // A compressed fixed file always stores compressed_record_length
            // bytes; a short Update replaces only the supplied prefix.
            let mut merged = old_record.clone();
            let overlap = supplied.len().min(merged.len());
            merged[..overlap].copy_from_slice(&supplied[..overlap]);
            merged.truncate(loaded.file.fcr.compressed_record_length as usize);
            if merged.len() < loaded.file.fcr.compressed_record_length as usize {
                merged.resize(loaded.file.fcr.compressed_record_length as usize, 0);
            }
            merged
        } else {
            supplied
        };
        // A variable/compressed Update whose image is shorter than the
        // logical record still advances the counter and stamps the existing
        // chunk pages with the new aux before reporting status 22.
        if !fixed && new_record.len() < logical && !self.in_transaction() {
            let counter = u16::from_le_bytes([loaded.raw[4], loaded.raw[5]]).wrapping_add(1);
            if let Err(error) = touch_variable_chain_aux(&path, target_address, counter) {
                return status::from_format(&error);
            }
            let _ = std::fs::write(preimage_path(&path), b"");
            self.failed_operation(&path);
            return DATA_BUFFER_LENGTH;
        }
        // A simple fixed single-key file is updated in place, exactly as the
        // DOS engine does.  Rebuilding would reset the operation counters and
        // move the record slot.
        let in_place = if loaded.file.fcr.file_flags.compression() {
            match update_compressed_at_in_place(&path, target_address, &new_record) {
                Ok(done) => {
                    if done {
                        if let Err(status) = self.finish_in_place_mutation(
                            &path,
                            &loaded.raw,
                            &loaded.file,
                            MutationKind::Update,
                            None,
                        ) {
                            return status;
                        }
                    }
                    done
                }
                Err(error) => return status::from_format(&error),
            }
        } else {
            match update_fixed_at_in_place(&path, target_address, &new_record) {
                Ok(done) => {
                    if done {
                        if let Err(status) = self.finish_in_place_mutation(
                            &path,
                            &loaded.raw,
                            &loaded.file,
                            MutationKind::Update,
                            None,
                        ) {
                            return status;
                        }
                    }
                    done
                }
                Err(error) => return status::from_format(&error),
            }
        };
        let new_address = if in_place {
            target_address
        } else {
            let old_addresses: std::collections::HashSet<u32> =
                loaded.records.iter().map(|(address, _)| *address).collect();
            let mut records: Vec<Vec<u8>> = loaded
                .records
                .iter()
                .map(|(_, record)| record.clone())
                .collect();
            records[index] = new_record.clone();
            let writer = match Writer::from_records(loaded.spec.clone(), &records) {
                Ok(writer) => writer,
                Err(error) => return status::from_format(&error),
            };
            if let Err(status) = self.write_rebuilt(
                &path,
                writer,
                &loaded.raw,
                loaded.file.page_size.bytes(),
                Some((MutationKind::Update, None, &loaded.file)),
            ) {
                return status;
            }
            let reloaded = BtrieveFile::open(&path)
                .ok()
                .and_then(|file| file.records().ok());
            reloaded
                .as_ref()
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row.data == new_record && !old_addresses.contains(&row.address))
                        .map(|row| row.address)
                })
                .or_else(|| {
                    reloaded
                        .as_ref()
                        .and_then(|rows| rows.get(index).map(|row| row.address))
                })
                .unwrap_or(cursor.last_address)
        };
        if !in_place
            && !self.in_transaction()
            && (loaded.file.fcr.file_flags.variable_length()
                || loaded.file.fcr.file_flags.compression())
        {
            let keys_changed = loaded.spec.keys.iter().any(|key| {
                match (key.build_key(&old_record), key.build_key(&new_record)) {
                    (Ok(old), Ok(new)) => {
                        key.compare(&old, &new, loaded.spec.acs.as_ref()) != Ordering::Equal
                    }
                    _ => true,
                }
            });
            if !keys_changed {
                let counter = u16::from_le_bytes([loaded.raw[4], loaded.raw[5]]).wrapping_add(2);
                if let Err(error) = touch_variable_update(
                    &path,
                    new_address,
                    counter,
                    loaded.file.page_count as usize,
                ) {
                    return status::from_format(&error);
                }
            }
        }
        if !self.in_transaction() {
            if let Err(status) = self.write_transient_preimage(
                &path,
                &loaded.file.bytes,
                &loaded.file,
                MutationKind::Update,
                None,
                target_address,
                &[],
                false,
            ) {
                return status;
            }
        }
        let key_index = if (cursor.key_number as usize) < loaded.spec.keys.len() {
            Some(cursor.key_number as usize)
        } else {
            choose_key_number(&loaded.spec, 0)
        };
        let changed_key_count = loaded
            .spec
            .keys
            .iter()
            .filter(
                |key| match (key.build_key(&old_record), key.build_key(&new_record)) {
                    (Ok(old), Ok(new)) => {
                        key.compare(&old, &new, loaded.spec.acs.as_ref()) != Ordering::Equal
                    }
                    _ => false,
                },
            )
            .count();
        let mut updated = cursor.clone();
        updated.handle = handle;
        updated.delete_replacement = None;
        updated.physical_address = new_address;
        updated.physical_established = true;
        updated.key_changed = changed_key_count > 0;
        let updated_events = if let Ok(new_file) = BtrieveFile::open(&path) {
            let old_chunks = journal_chunk_count(&loaded.file.bytes, &loaded.file, target_address);
            let new_chunks = journal_chunk_count(&new_file.bytes, &new_file, new_address);
            if loaded.file.fcr.file_flags.compression() {
                (1 + 2 * old_chunks + new_chunks) as u32
            } else if loaded.file.fcr.file_flags.variable_length() {
                (1 + old_chunks + new_chunks) as u32
            } else {
                (1 + 3 * changed_key_count) as u32
            }
        } else {
            0
        };
        if !self.in_transaction() {
            updated.preimage_pages = updated.preimage_pages.max(updated_events);
        }
        if let Some(index) = key_index {
            if let Ok(key) = loaded.spec.keys[index].build_key(&new_record) {
                updated.key_number = index as u16;
                updated.last_key = key;
                updated.last_address = new_address;
                updated.established = !loaded.spec.keys[index].is_excluded_from_index(&new_record);
            }
        } else {
            updated.last_address = new_address;
        }
        if let Some(index) = key_index {
            if let Ok(file) = BtrieveFile::open(&path) {
                if let Ok(spec) = FileSpec::from_file(&file) {
                    if let Some(key) = spec.keys.get(index) {
                        if let Ok(value) = key.build_key(&new_record) {
                            updated.tree_path = index_tree_path(&file, &spec, index, &value);
                            updated.path_extra = None;
                        }
                    }
                }
            }
        }
        self.store_cursor(position, updated);
        SUCCESS
    }

    // ------------------------------------------------------------ Delete

    fn op_delete(&mut self, position: &PositionBlock, _data_len: &mut u16) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        if let Err(status) = self.claim_file_for_transaction(&path) {
            return status;
        }
        let cursor = self.cursor(position, handle);
        if !cursor.established && !cursor.physical_established {
            return INVALID_POSITIONING;
        }
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        let target_address = if cursor.physical_established {
            cursor.physical_address
        } else {
            cursor.last_address
        };
        let Some(index) = loaded
            .records
            .iter()
            .position(|(address, _)| *address == target_address)
        else {
            return INVALID_POSITIONING;
        };
        if loaded.file.fcr.file_flags.key_only() {
            self.mark_journal_owner(handle);
            let _ = std::fs::write(preimage_path(&path), b"");
            self.failed_operation(&path);
            return OPERATION_NOT_ALLOWED;
        }
        self.mark_journal_owner(handle);
        let deleted = loaded.records[index].1.clone();
        let deleted_key = if (cursor.key_number as usize) < loaded.spec.keys.len() {
            loaded.spec.keys[cursor.key_number as usize]
                .build_key(&deleted)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let replacement = if (cursor.key_number as usize) < loaded.spec.keys.len() {
            interior_delete_replacement(
                &loaded.file,
                &loaded.spec,
                cursor.key_number as usize,
                &deleted_key,
                target_address,
            )
        } else {
            None
        };
        let underflow_path =
            if replacement.is_none() && (cursor.key_number as usize) < loaded.spec.keys.len() {
                underflow_delete_path(
                    &loaded.file,
                    cursor.key_number as usize,
                    &cursor.tree_path,
                    target_address,
                )
            } else {
                None
            };
        // A simple fixed single-key file is deleted in place, preserving the
        // engine's free-list and operation-counter layout.
        let delete_index_pages;
        let in_place = match delete_fixed_at_in_place_with_journal(&path, target_address) {
            Ok((done, journal)) => {
                delete_index_pages = journal;
                if done {
                    if let Err(status) = self.finish_in_place_mutation(
                        &path,
                        &loaded.raw,
                        &loaded.file,
                        MutationKind::Delete,
                        None,
                    ) {
                        return status;
                    }
                }
                done
            }
            Err(error) => return status::from_format(&error),
        };
        if !in_place {
            let mut records: Vec<Vec<u8>> = loaded
                .records
                .iter()
                .map(|(_, record)| record.clone())
                .collect();
            records.remove(index);
            let writer = match Writer::from_records(loaded.spec.clone(), &records) {
                Ok(writer) => writer,
                Err(error) => return status::from_format(&error),
            };
            if let Err(status) = self.write_rebuilt(
                &path,
                writer,
                &loaded.raw,
                loaded.file.page_size.bytes(),
                Some((MutationKind::Delete, None, &loaded.file)),
            ) {
                return status;
            }
        }
        if !self.in_transaction() {
            if let Err(status) = self.write_transient_preimage(
                &path,
                &loaded.file.bytes,
                &loaded.file,
                MutationKind::Delete,
                None,
                target_address,
                &delete_index_pages,
                false,
            ) {
                return status;
            }
        }
        let mut updated = cursor.clone();
        updated.handle = handle;
        updated.unindexed_delete = !loaded.file.index_pages.iter().any(|page| {
            page.entries
                .iter()
                .any(|entry| entry.record_address == target_address)
        });
        if let Ok(new_file) = BtrieveFile::open(&path) {
            let changed = changed_index_page_counts(&loaded.file, &new_file);
            let index_events: usize = changed
                .iter()
                .map(|&count| count + usize::from(count > 1))
                .sum();
            let old_chunks = journal_chunk_count(&loaded.file.bytes, &loaded.file, target_address);
            let chunk_events = if loaded.file.fcr.file_flags.compression() {
                2 * old_chunks
            } else if loaded.file.fcr.file_flags.variable_length() {
                old_chunks
            } else {
                0
            };
            let duplicate = loaded.spec.keys.iter().any(|key| key.has_duplicates());
            let events = if duplicate {
                let group = self.duplicate_delete_preimage_group(
                    &loaded.file,
                    &loaded.file.bytes,
                    target_address,
                    0,
                );
                group.len() / loaded.file.page_size.bytes()
            } else {
                1 + index_events + chunk_events
            };
            if !self.in_transaction() {
                updated.preimage_pages = updated.preimage_pages.max(events as u32);
            }
        }
        if let Some((predecessor, path)) = replacement {
            updated.delete_replacement = Some(predecessor);
            updated.tree_path = path;
        } else if let Some((path, extra)) = underflow_path {
            updated.tree_path = path;
            updated.path_extra = extra;
        }
        if loaded.spec.keys.len() > 1 {
            let last = loaded.spec.keys.len() - 1;
            if let Ok(value) = loaded.spec.keys[last].build_key(&deleted) {
                updated.tree_path = index_tree_path(&loaded.file, &loaded.spec, last, &value);
                updated.path_extra = None;
            }
        }
        updated.last_key = deleted_key;
        updated.last_address = target_address;
        updated.established = true;
        updated.physical_address = target_address;
        updated.physical_established = true;
        self.store_cursor(position, updated);
        SUCCESS
    }

    // --------------------------------------------------------------- Gets

    #[allow(clippy::too_many_arguments)]
    fn op_get(
        &mut self,
        op: u16,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        key: &mut [u8],
        key_number: i16,
        lock_bias: u8,
        key_only_output: bool,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        if loaded.spec.keys.is_empty() {
            return INVALID_KEY_NUMBER;
        }
        if key_number < 0 || key_number as usize >= loaded.spec.keys.len() {
            return INVALID_KEY_NUMBER;
        }
        let key_index = key_number as u16;
        let definition = &loaded.spec.keys[key_index as usize];
        let needs_search = matches!(op, 0x05 | 0x08 | 0x09 | 0x0A | 0x0B);
        if needs_search && key.len() < definition.length as usize {
            return KEY_BUFFER_TOO_SHORT;
        }
        let view = match build_view(&loaded.file, &loaded.spec, key_index) {
            Ok(view) => view,
            Err(status) => return status,
        };
        let mut cursor = self.cursor(position, handle);
        let needs_previous = matches!(op, 0x06 | 0x07);
        if needs_previous {
            if !cursor.established {
                return INVALID_POSITIONING;
            }
            if cursor.key_number != key_index {
                return DIFFERENT_KEY_NUMBER;
            }
        }
        let found = match op {
            0x05 => {
                let index = lower_bound(&view, key, &loaded.spec, key_index);
                if index < view.len()
                    && definition.compare(&view[index].key, key, loaded.spec.acs.as_ref())
                        == std::cmp::Ordering::Equal
                {
                    Some(index)
                } else {
                    None
                }
            }
            0x08 => {
                let index = upper_bound(&view, key, &loaded.spec, key_index);
                (index < view.len()).then_some(index)
            }
            0x09 => {
                let index = lower_bound(&view, key, &loaded.spec, key_index);
                (index < view.len()).then_some(index)
            }
            0x0A => {
                let index = lower_bound(&view, key, &loaded.spec, key_index);
                index.checked_sub(1)
            }
            0x0B => {
                let index = upper_bound(&view, key, &loaded.spec, key_index);
                index.checked_sub(1)
            }
            0x0C => (!view.is_empty()).then_some(0),
            0x0D => view.len().checked_sub(1),
            0x06 => {
                let index = first_after(
                    &view,
                    &cursor.last_key,
                    cursor.last_address,
                    &loaded.spec,
                    key_index,
                );
                (index < view.len()).then_some(index)
            }
            0x07 => last_before(
                &view,
                &cursor.last_key,
                cursor.last_address,
                &loaded.spec,
                key_index,
            ),
            _ => None,
        };
        let Some(index) = found else {
            // Only Get Equal reports "key not found"; the range and step
            // operations report end of file at either boundary.
            return if op == 0x05 {
                KEY_NOT_FOUND
            } else {
                END_OF_FILE
            };
        };
        let entry = view[index].clone();
        let output = if key_only_output {
            entry.key.clone()
        } else {
            entry.data.clone()
        };
        if (*data_len as usize).min(data.len()) < output.len() {
            return DATA_BUFFER_LENGTH;
        }
        if let Err(status) = self.check_lock(&path, entry.address, handle, lock_bias) {
            return status;
        }
        if let Err(status) = write_output(data, data_len, &output) {
            return status;
        }
        cursor.handle = handle;
        cursor.key_number = key_index;
        cursor.last_key = entry.key.clone();
        cursor.last_address = entry.address;
        cursor.established = true;
        cursor.physical_address = entry.address;
        cursor.physical_established = true;
        let equal = |left: &[u8], right: &[u8]| {
            definition.compare(left, right, loaded.spec.acs.as_ref()) == Ordering::Equal
        };
        cursor.duplicate_previous = if index > 0 && equal(&view[index - 1].key, &entry.key) {
            Some(view[index - 1].address)
        } else {
            None
        };
        cursor.duplicate_next = if index + 1 < view.len() && equal(&view[index + 1].key, &entry.key)
        {
            Some(view[index + 1].address)
        } else {
            None
        };
        cursor.tree_path =
            index_tree_path(&loaded.file, &loaded.spec, key_index as usize, &entry.key);
        cursor.delete_replacement = None;
        cursor.path_extra = None;
        if loaded.file.fcr.file_flags.key_only() {
            cursor.key_only_display = key_only_entry(
                &loaded.file,
                &loaded.spec,
                key_index as usize,
                entry.address,
            )
            .map(|(display, _)| display);
        } else {
            cursor.key_only_display = None;
        }
        self.store_cursor(position, cursor);
        SUCCESS
    }

    #[allow(clippy::too_many_arguments)]
    fn op_step(
        &mut self,
        op: u16,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        _key: &mut [u8],
        _key_number: i16,
        lock_bias: u8,
    ) -> Status {
        if op != 0x0C && op != 0x0D && op != 0x06 && op != 0x07 {
            return INVALID_OPERATION;
        }
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        let view = match build_physical_view(&loaded.file) {
            Ok(view) => view,
            Err(status) => return status,
        };
        let mut cursor = self.cursor(position, handle);
        let index = match op {
            0x0C => (!view.is_empty()).then_some(0),
            0x0D => view.len().checked_sub(1),
            0x06 => {
                let start = if cursor.physical_established {
                    view.partition_point(|entry| entry.address <= cursor.physical_address)
                } else {
                    0
                };
                (start < view.len()).then_some(start)
            }
            0x07 => {
                if !cursor.physical_established {
                    return END_OF_FILE;
                }
                let count = view.partition_point(|entry| entry.address < cursor.physical_address);
                count.checked_sub(1)
            }
            _ => None,
        };
        let Some(index) = index else {
            return END_OF_FILE;
        };
        let entry = &view[index];
        if (*data_len as usize).min(data.len()) < entry.data.len() {
            return DATA_BUFFER_LENGTH;
        }
        if let Err(status) = self.check_lock(&path, entry.address, handle, lock_bias) {
            return status;
        }
        if let Err(status) = write_output(data, data_len, &entry.data) {
            return status;
        }
        cursor.handle = handle;
        cursor.physical_address = entry.address;
        cursor.physical_established = true;
        let key_index = (cursor.key_number as usize).min(loaded.spec.keys.len().saturating_sub(1));
        if !loaded.spec.keys.is_empty() {
            if let Ok(value) = loaded.spec.keys[key_index].build_key(&entry.data) {
                cursor.tree_path = index_tree_path(&loaded.file, &loaded.spec, key_index, &value);
                cursor.path_extra = None;
                if loaded.file.fcr.file_flags.key_only() {
                    cursor.key_only_display =
                        key_only_entry(&loaded.file, &loaded.spec, key_index, entry.address)
                            .map(|(display, _)| display);
                } else {
                    cursor.key_only_display = None;
                }
            }
        }
        self.store_cursor(position, cursor);
        SUCCESS
    }

    fn check_lock(
        &mut self,
        path: &Path,
        address: u32,
        handle: u32,
        bias: u8,
    ) -> Result<(), Status> {
        if bias == 0 {
            return Ok(());
        }
        let key = (path.to_path_buf(), address);
        match self.record_locks.get(&key) {
            Some(owner) if *owner != handle => Err(RECORD_IN_USE),
            Some(_) => Ok(()),
            None => {
                self.record_locks.insert(key, handle);
                Ok(())
            }
        }
    }

    fn op_unlock(&mut self, position: &PositionBlock, data: &mut [u8], data_len: usize) -> Status {
        let Ok(handle) = self.handle_raw(position) else {
            return FILE_NOT_OPEN;
        };
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let address = if data_len >= 4 {
            position::read_address(&data[..4])
        } else {
            let cursor = self.cursor(position, handle);
            cursor
                .physical_established
                .then_some(cursor.physical_address)
                .or_else(|| cursor.established.then_some(cursor.last_address))
        };
        if let Some(address) = address {
            if self
                .record_locks
                .get(&(path.clone(), address))
                .copied()
                .is_some_and(|owner| owner == handle)
            {
                self.record_locks.remove(&(path, address));
            }
        }
        SUCCESS
    }

    fn op_get_position(
        &mut self,
        position: &PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        let cursor = self.cursor(position, handle);
        if !cursor.physical_established && !cursor.established {
            return INVALID_POSITIONING;
        }
        if (*data_len as usize).min(data.len()) < 4 {
            return DATA_BUFFER_LENGTH;
        }
        let address = if cursor.physical_established {
            cursor.physical_address
        } else {
            cursor.last_address
        };
        position::write_address(data, address);
        *data_len = 4;
        SUCCESS
    }

    fn op_get_direct(
        &mut self,
        position: &mut PositionBlock,
        data: &mut [u8],
        data_len: &mut u16,
        key_number: i16,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        if (*data_len as usize).min(data.len()) < 4 {
            return DATA_BUFFER_LENGTH;
        }
        let Some(address) = position::read_address(data) else {
            return INVALID_RECORD_ADDRESS;
        };
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        let Some((_, record)) = loaded
            .records
            .iter()
            .find(|(candidate, _)| *candidate == address)
        else {
            return INVALID_RECORD_ADDRESS;
        };
        if let Err(status) = write_output(data, data_len, record) {
            return status;
        }
        let mut cursor = self.cursor(position, handle);
        cursor.handle = handle;
        cursor.last_address = address;
        cursor.last_key.clear();
        cursor.established = true;
        cursor.physical_address = address;
        cursor.physical_established = true;
        if let Some(index) = choose_key_number(&loaded.spec, key_number) {
            if let Ok(key) = loaded.spec.keys[index].build_key(record) {
                cursor.key_number = index as u16;
                cursor.last_key = key.clone();
                cursor.tree_path = index_tree_path(&loaded.file, &loaded.spec, index, &key);
                cursor.path_extra = None;
                if loaded.file.fcr.file_flags.key_only() {
                    cursor.key_only_display =
                        key_only_entry(&loaded.file, &loaded.spec, index, address)
                            .map(|(display, _)| display);
                } else {
                    cursor.key_only_display = None;
                }
            }
        }
        self.store_cursor(position, cursor);
        SUCCESS
    }

    // -------------------------------------------------------- Transactions

    fn op_begin_transaction(&mut self) -> Status {
        if self.in_transaction() {
            return TRANSACTION_ACTIVE;
        }
        let session = self.active_session;
        let target = self
            .files
            .values()
            .find(|file| file.session == session)
            .map(|file| file.path.clone());
        let log = self.transaction_log_path(session, target.as_deref());
        if let Some(log) = &log {
            if reset_log(log).is_err() {
                return TRANSACTION_FILE_IO_ERROR;
            }
        }
        self.transactions.insert(
            session,
            Transaction {
                snapshots: HashMap::new(),
                operation_counters: HashMap::new(),
                preimage_started: HashSet::new(),
                preimage_pages: HashMap::new(),
                log_path: log,
            },
        );
        SUCCESS
    }

    fn op_end_transaction(&mut self) -> Status {
        let session = self.active_session;
        let Some(transaction) = self.transactions.remove(&session) else {
            return END_ABORT_ERROR;
        };
        for (path, snapshot) in &transaction.snapshots {
            if let Ok(mut bytes) = std::fs::read(path) {
                let page_size = if snapshot.len() >= 10 {
                    u16::from_le_bytes([snapshot[8], snapshot[9]]) as usize
                } else {
                    0
                };
                let Some(counter) = transaction.operation_counters.get(path).copied() else {
                    continue;
                };
                let encrypted = snapshot.len() > 0x38
                    && u16::from_le_bytes([snapshot[6], snapshot[7]]) == 0xFD00
                    && snapshot[0x37] & 0x02 != 0;
                if page_size != 0 && !encrypted {
                    for page in changed_pages(snapshot, &bytes, page_size) {
                        // Split-created index pages keep their zero aux; new
                        // data pages and all pre-existing pages take the
                        // transaction aux.
                        if page * page_size < snapshot.len()
                            || page_is_data(&bytes, page, page_size)
                        {
                            set_page_aux(&mut bytes, page, page_size, counter);
                        }
                    }
                }
                put_u16(&mut bytes, 0x04, counter.wrapping_add(1));
                put_u16(&mut bytes, 0x22, 0);
                clear_transaction_marker(&mut bytes);
                if std::fs::write(path, bytes).is_err() {
                    self.transactions.insert(session, transaction);
                    return TRANSACTION_FILE_IO_ERROR;
                }
            }
        }
        let committed = !transaction.snapshots.is_empty();
        let target = transaction
            .snapshots
            .keys()
            .next()
            .cloned()
            .or_else(|| {
                self.files
                    .values()
                    .find(|file| file.session == session)
                    .map(|file| file.path.clone())
            })
            .unwrap_or_default();
        let log = transaction
            .log_path
            .clone()
            .or_else(|| target.parent().map(|parent| parent.join("BTRIEVE.TRN")));
        let directory = self
            .directories
            .get(&session)
            .or(self.current_dir.as_ref())
            .cloned();
        if let Some(log) = log {
            if write_log(&log, &target, directory.as_deref(), committed).is_err() {
                self.transactions.insert(session, transaction);
                return TRANSACTION_FILE_IO_ERROR;
            }
        }
        self.release_transaction_ownership(session);
        SUCCESS
    }

    fn op_abort_transaction(&mut self) -> Status {
        let session = self.active_session;
        let Some(transaction) = self.transactions.remove(&session) else {
            return END_ABORT_ERROR;
        };
        for (path, snapshot) in &transaction.snapshots {
            let page_size = if snapshot.len() >= 10 {
                u16::from_le_bytes([snapshot[8], snapshot[9]]) as usize
            } else {
                0
            };
            let current = std::fs::read(path).unwrap_or_else(|_| snapshot.clone());
            let encrypted = image_owner_encrypted(snapshot);
            let hash = owner_hash_from_image(snapshot);
            let snapshot_view = if let (true, Some(hash)) = (encrypted, hash) {
                let mut view = snapshot.clone();
                crypt_owner_pages(&mut view, &hash, page_size);
                view
            } else {
                snapshot.clone()
            };
            let mut bytes = if let (true, Some(hash)) = (encrypted, hash) {
                let mut view = current.clone();
                crypt_owner_pages(&mut view, &hash, page_size);
                view
            } else {
                current.clone()
            };
            let changed = if page_size == 0 {
                Vec::new()
            } else {
                changed_pages(&snapshot_view, &bytes, page_size)
            };
            // Abort applies the preimage in reverse exactly like crash
            // recovery, but leaves the preimage in place and keeps pages that
            // the transaction appended.
            if page_size != 0 {
                if let Ok(images) = std::fs::read(preimage_path(path)) {
                    if !images.is_empty() && images.len() % page_size == 0 {
                        for image in images.chunks_exact(page_size).rev() {
                            let page = u16::from_le_bytes([image[2], image[3]]) as usize;
                            if page == 0xFFFF {
                                continue;
                            }
                            let start = page * page_size;
                            if start + page_size <= bytes.len() {
                                bytes[start..start + page_size].copy_from_slice(image);
                            }
                        }
                    } else {
                        bytes = snapshot_view.clone();
                    }
                } else {
                    bytes = snapshot_view.clone();
                    if current.len() > bytes.len() {
                        bytes.extend_from_slice(&current[bytes.len()..]);
                    }
                }
            }
            let counter = transaction
                .operation_counters
                .get(path)
                .copied()
                .unwrap_or(0);
            for page in changed {
                let data = page_is_data(&bytes, page, page_size);
                if page * page_size >= snapshot.len() {
                    // Appended pages were created during the transaction.
                    // Data pages get the abort aux; split-created index pages
                    // keep their zero aux.
                    if data {
                        set_page_aux(&mut bytes, page, page_size, counter.wrapping_sub(1));
                    }
                    continue;
                }
                let aux = if data {
                    counter.wrapping_sub(1)
                } else {
                    counter
                };
                set_page_aux(&mut bytes, page, page_size, aux);
            }
            put_u16(&mut bytes, 0x04, counter);
            put_u16(&mut bytes, 0x22, 0);
            clear_transaction_marker(&mut bytes);
            if let (true, Some(hash)) = (encrypted, hash) {
                crypt_owner_pages(&mut bytes, &hash, page_size);
            }
            if std::fs::write(path, bytes).is_err() {
                self.transactions.insert(session, transaction);
                return END_ABORT_ERROR;
            }
        }
        self.release_transaction_ownership(session);
        SUCCESS
    }

    // ------------------------------------------------------- Owner / DDL

    fn op_set_owner(
        &mut self,
        position: &PositionBlock,
        data: &mut [u8],
        data_len: usize,
        key: &[u8],
        mode: i16,
    ) -> Status {
        if self.in_transaction() {
            return OPERATION_NOT_ALLOWED;
        }
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let name = c_string_prefix(&data[..data_len.min(data.len())]);
        let key_name = c_string_prefix(key);
        if !key_name.is_empty() && key_name != name {
            return INVALID_OWNER;
        }
        if name.len() > 8 || !(0..=3).contains(&mode) {
            return INVALID_OWNER;
        }
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        if loaded.file.fcr.owner_present {
            return OWNER_ALREADY_SET;
        }
        let owner = OwnerInfo::from_name(name.as_bytes(), mode as u8);
        let page_size = loaded.file.page_size.bytes();
        let mut bytes = loaded.raw;
        if bytes.len() < 0x38 {
            return IO_ERROR;
        }
        let counter = u16::from_le_bytes([bytes[4], bytes[5]]).wrapping_add(2);
        put_u16(&mut bytes, 0x04, counter);
        put_u16(&mut bytes, 0x06, 0xFD00);
        bytes[0x2E..0x37].copy_from_slice(&owner.hash);
        bytes[0x37] = mode as u8;
        if mode as u8 & 0x02 != 0 {
            for page in 1..bytes.len() / page_size {
                let start = page * page_size;
                for offset in 0..page_size {
                    bytes[start + offset] ^=
                        btr_format::fcr::owner_mask_byte(&owner.hash, offset, page_size);
                }
            }
        }
        if std::fs::write(&path, bytes).is_err() {
            return IO_ERROR;
        }
        SUCCESS
    }

    fn op_clear_owner(&mut self, position: &PositionBlock) -> Status {
        if self.in_transaction() {
            return OPERATION_NOT_ALLOWED;
        }
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        if !loaded.file.fcr.owner_present {
            return SUCCESS;
        }
        // Clear Owner is an FCR-only operation.  The DOS engine keeps the
        // nine-byte owner hash as residue, sets the mode byte to 1, restores
        // the normal format word, and advances the operation counter by two.
        let mut spec = loaded.spec;
        spec.owner = None;
        let owner_mode = loaded.file.fcr.owner_mode;
        // Encrypted pages must be written back in plaintext.
        let mut bytes = if owner_mode & 0x02 != 0 {
            loaded.file.bytes.clone()
        } else {
            loaded.raw
        };
        if bytes.len() < 0x38 {
            return IO_ERROR;
        }
        let counter = u16::from_le_bytes([bytes[4], bytes[5]]).wrapping_add(2);
        put_u16(&mut bytes, 0x04, counter);
        put_u16(&mut bytes, 0x06, spec.format_word());
        bytes[0x37] = u8::from(owner_mode != 0);
        if std::fs::write(&path, bytes).is_err() {
            return IO_ERROR;
        }
        SUCCESS
    }

    fn op_create_supplemental(
        &mut self,
        position: &PositionBlock,
        data: &mut [u8],
        data_len: usize,
    ) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        self.mark_journal_owner(handle);
        if data_len < 16 {
            return DATA_BUFFER_LENGTH;
        }
        let block = &data[..16];
        let position_in_record = u16::from_le_bytes([block[0], block[1]]);
        let length = u16::from_le_bytes([block[2], block[3]]);
        let flags = u16::from_le_bytes([block[4], block[5]]);
        let key_type = block[10];
        let null_value = block[11];
        if position_in_record == 0 || length == 0 {
            return INVALID_KEY_LENGTH;
        }
        let key = KeySpec::new(position_in_record, length, flags, key_type, null_value);
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        if loaded.spec.keys.len() >= 24 {
            return NUMBER_OF_KEYS;
        }
        let records: Vec<Vec<u8>> = loaded
            .records
            .iter()
            .map(|(_, record)| record.clone())
            .collect();
        let mut writer = match Writer::from_records(loaded.spec.clone(), &records) {
            Ok(writer) => writer,
            Err(error) => return status::from_format(&error),
        };
        if let Err(error) = writer.add_supplemental(key) {
            return status::from_format(&error);
        }
        match self.write_rebuilt(
            &path,
            writer,
            &loaded.raw,
            loaded.file.page_size.bytes(),
            None,
        ) {
            Ok(mut bytes) => {
                // Adding a supplemental index to an existing file does not
                // set the extended-format bit in the FCR.
                put_u16(&mut bytes, 0x06, loaded.file.fcr.format_word);
                if std::fs::write(&path, bytes).is_err() {
                    return IO_ERROR;
                }
                if !self.in_transaction() {
                    let _ = std::fs::write(preimage_path(&path), b"");
                }
                SUCCESS
            }
            Err(status) => status,
        }
    }

    fn op_drop_supplemental(&mut self, position: &PositionBlock, key_number: i16) -> Status {
        let Ok(handle) = self.handle(position) else {
            return FILE_NOT_OPEN;
        };
        if let Err(status) = self.check_writable(handle) {
            return status;
        }
        let path = match self.open_file(handle) {
            Ok(file) => file.path.clone(),
            Err(status) => return status,
        };
        self.mark_journal_owner(handle);
        if key_number <= 0 {
            return INVALID_KEY_NUMBER;
        }
        let loaded = match self.load(&path) {
            Ok(loaded) => loaded,
            Err(status) => return status,
        };
        let index = key_number as usize;
        if !self.in_transaction()
            && index < loaded.spec.keys.len()
            && loaded.spec.keys[index].is_supplemental()
            && !(loaded.file.fcr.owner_present && loaded.file.fcr.owner_mode & 0x02 != 0)
        {
            let page_size = loaded.file.page_size.bytes();
            let mut bytes = loaded.raw.clone();
            let mut pages: Vec<u32> = loaded
                .file
                .index_pages
                .iter()
                .filter(|page| page.key_index == index)
                .map(|page| page.page_number)
                .collect();
            pages.sort_unstable_by(|left, right| right.cmp(left));

            // Remove the key's blocks from the FCR key table.
            let mut offset = KEY_DEFS_OFFSET;
            for definition in loaded.file.fcr.key_defs.iter().take(index) {
                offset += definition.segments.len() * KEY_DEF_SIZE;
            }
            let blocks = loaded.file.fcr.key_defs[index].segments.len() * KEY_DEF_SIZE;
            let total: usize = loaded
                .file
                .fcr
                .key_defs
                .iter()
                .map(|definition| definition.segments.len() * KEY_DEF_SIZE)
                .sum();
            let table_end = KEY_DEFS_OFFSET + total;
            if offset + blocks <= table_end && table_end <= bytes.len() {
                bytes.copy_within(offset + blocks..table_end, offset);
                bytes[table_end - blocks..table_end].fill(0);
            }
            let keys = u16::from_le_bytes([bytes[0x14], bytes[0x15]]);
            if keys != 0xFFFF {
                put_u16(&mut bytes, 0x14, keys.saturating_sub(1));
            }

            let counter = u16::from_le_bytes([bytes[4], bytes[5]]).wrapping_add(2);
            put_u16(&mut bytes, 0x04, counter);

            let old_head = ((loaded.file.fcr.free_links[0] as u32) << 16)
                | loaded.file.fcr.free_links[1] as u32;
            for (position, page) in pages.iter().enumerate() {
                let next = pages.get(position + 1).copied().unwrap_or(old_head);
                let next_value = if next == 0xFFFF_FFFF {
                    0xFFFF_FFFF
                } else {
                    next << 16
                };
                let start = *page as usize * page_size;
                if start + page_size > bytes.len() {
                    return IO_ERROR;
                }
                bytes[start..start + page_size].fill(0);
                put_u16(&mut bytes, start, 0);
                put_u16(&mut bytes, start + 2, *page as u16);
                bytes[start + 8..start + 12].copy_from_slice(&next_value.to_le_bytes());
            }
            let head = pages.first().copied().unwrap_or(old_head);
            put_u16(&mut bytes, 0x0C, (head >> 16) as u16);
            put_u16(&mut bytes, 0x0E, head as u16);
            if std::fs::write(&path, bytes).is_err() {
                return IO_ERROR;
            }
            if !self.in_transaction() {
                let _ = std::fs::write(preimage_path(&path), b"");
            }
            return SUCCESS;
        }
        let records: Vec<Vec<u8>> = loaded
            .records
            .iter()
            .map(|(_, record)| record.clone())
            .collect();
        let mut writer = match Writer::from_records(loaded.spec.clone(), &records) {
            Ok(writer) => writer,
            Err(error) => return status::from_format(&error),
        };
        if let Err(error) = writer.drop_supplemental(key_number as usize) {
            return status::from_format(&error);
        }
        match self.write_rebuilt(
            &path,
            writer,
            &loaded.raw,
            loaded.file.page_size.bytes(),
            None,
        ) {
            Ok(_) => {
                if !self.in_transaction() {
                    let _ = std::fs::write(preimage_path(&path), b"");
                }
                SUCCESS
            }
            Err(status) => status,
        }
    }

    // ---------------------------------------------------------- Directory

    fn op_set_directory(&mut self, key: &mut [u8]) -> Status {
        let Some(name) = c_string(key) else {
            return INVALID_FILE_NAME;
        };
        if name.is_empty() {
            return INVALID_FILE_NAME;
        }
        let path = self.resolve(&name);
        if !path.is_dir() {
            return DIRECTORY_ERROR;
        }
        self.directories.insert(self.active_session, path);
        SUCCESS
    }

    fn op_get_directory(
        &mut self,
        data: &mut [u8],
        data_len: &mut u16,
        key: &mut [u8],
        _drive: i16,
    ) -> Status {
        let directory = self
            .session_dir()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let text = directory.to_string_lossy();
        let bytes = text.as_bytes();
        if key.len() < bytes.len() + 1 {
            return KEY_BUFFER_TOO_SHORT;
        }
        key[..bytes.len()].copy_from_slice(bytes);
        key[bytes.len()] = 0;
        let _ = (data, data_len);
        SUCCESS
    }

    // ------------------------------------------------------------ Misc

    fn op_version(&mut self, data: &mut [u8], data_len: &mut u16) -> Status {
        if (*data_len as usize).min(data.len()) < 5 {
            return DATA_BUFFER_LENGTH;
        }
        data[..5].copy_from_slice(&[5, 0, 10, 0, b'N']);
        *data_len = 5;
        SUCCESS
    }

    fn reset_session(&mut self, session: SessionId) {
        if let Some(transaction) = self.transactions.remove(&session) {
            for (path, snapshot) in transaction.snapshots {
                let _ = std::fs::write(path, snapshot);
            }
        }
        let handles: HashSet<u32> = self
            .files
            .iter()
            .filter(|(_, open)| open.session == session)
            .map(|(handle, _)| *handle)
            .collect();
        let paths: HashSet<PathBuf> = self
            .files
            .values()
            .filter(|open| open.session == session)
            .map(|open| open.path.clone())
            .collect();
        for path in paths {
            if let Ok(mut bytes) = std::fs::read(&path) {
                clear_open_marker(&mut bytes);
                let _ = std::fs::write(&path, bytes);
            }
            let target = BtrieveFile::open(&path)
                .map(|file| self.preimage_target(&path, &file))
                .unwrap_or_else(|_| preimage_path(&path));
            let _ = std::fs::remove_file(target);
        }
        for open in self.files.values().filter(|open| open.session == session) {
            if let Some(shared) = self.sharing.get_mut(&open.share_key) {
                shared.handles = shared.handles.saturating_sub(1);
            }
        }
        self.files.retain(|_, open| open.session != session);
        self.cursors.retain(|handle, _| !handles.contains(handle));
        self.record_locks
            .retain(|(_, owner), _| !handles.contains(owner));
        self.sharing.retain(|_, shared| shared.handles > 0);
        self.transaction_owners.retain(|_, owner| *owner != session);
    }

    /// Close every handle and active transaction belonging to a client
    /// session.  Used by server transports when a connection ends.
    pub fn close_session(&mut self, session: SessionId) -> Status {
        let previous = self.active_session;
        self.active_session = session;
        let status = if self.transactions.contains_key(&session) {
            self.op_abort_transaction()
        } else {
            SUCCESS
        };
        self.reset_session(session);
        self.directories.remove(&session);
        self.active_session = previous;
        status
    }

    pub fn shutdown(&mut self) {
        let sessions = self.active_sessions();
        for session in sessions {
            let _ = self.close_session(session);
        }
    }

    pub fn active_sessions(&self) -> Vec<SessionId> {
        let mut sessions: HashSet<SessionId> =
            self.files.values().map(|open| open.session).collect();
        sessions.extend(self.transactions.keys().copied());
        sessions.extend(self.directories.keys().copied());
        sessions.into_iter().collect()
    }
}

fn normalize_op(raw: u16) -> Option<(u16, u8, bool)> {
    let mut op = raw;
    let mut lock_bias = 0u8;
    while op >= 0x64 {
        op -= 0x64;
        lock_bias = lock_bias.saturating_add(1);
        if lock_bias > 4 {
            return None;
        }
    }
    let mut key_only = false;
    if (0x37..=0x3F).contains(&op) {
        op -= 0x32;
        key_only = true;
    }
    if op > 0x31 {
        return None;
    }
    Some((op, lock_bias, key_only))
}

fn c_string(buffer: &[u8]) -> Option<String> {
    let end = buffer
        .iter()
        .position(|byte| *byte == 0 || *byte == b' ')
        .unwrap_or(buffer.len());
    if end == 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

fn c_string_prefix(buffer: &[u8]) -> String {
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}

/// B-tree pages traversed from the root to the node holding `value`.
fn index_tree_path(
    file: &BtrieveFile,
    spec: &FileSpec,
    key_index: usize,
    value: &[u8],
) -> Vec<u32> {
    let (Some(key), Some(definition)) =
        (spec.keys.get(key_index), file.fcr.key_defs.get(key_index))
    else {
        return Vec::new();
    };
    let mut number = definition.index_page as u32;
    let mut path = Vec::new();
    for _ in 0..64 {
        let Some(page) = file
            .index_pages
            .iter()
            .find(|page| page.key_index == key_index && page.page_number == number)
        else {
            break;
        };
        path.push(page.page_number);
        if page
            .entries
            .iter()
            .any(|entry| key.compare(&entry.key, value, spec.acs.as_ref()) == Ordering::Equal)
        {
            break;
        }
        if page.link_a == 0xFFFF_FFFF {
            break;
        }
        let position = page.entries.partition_point(|entry| {
            key.compare(&entry.key, value, spec.acs.as_ref()) == Ordering::Less
        });
        let child = if position == 0 {
            page.link_b >> 16
        } else if position == page.entries.len() {
            page.link_a >> 16
        } else {
            page.entries[position - 1].next_link >> 16
        };
        if child == 0 || child == 0xFFFF {
            break;
        }
        number = child;
    }
    path
}

/// When Delete removes a record stored in an interior index entry, the engine
/// replaces it with the maximum record of the left subtree.  Return that
/// record's address and the page path to it.
fn interior_delete_replacement(
    file: &BtrieveFile,
    spec: &FileSpec,
    key_index: usize,
    value: &[u8],
    address: u32,
) -> Option<(u32, Vec<u32>)> {
    let key = spec.keys.get(key_index)?;
    let mut path = index_tree_path(file, spec, key_index, value);
    let node_number = *path.last()?;
    let node = file
        .index_pages
        .iter()
        .find(|page| page.key_index == key_index && page.page_number == node_number)?;
    if node.link_a == 0xFFFF_FFFF {
        return None;
    }
    let position = node
        .entries
        .iter()
        .position(|entry| entry.record_address == address)?;
    let mut child = if position == 0 {
        node.link_b >> 16
    } else {
        node.entries[position - 1].next_link >> 16
    };
    for _ in 0..64 {
        if child == 0 || child == 0xFFFF {
            return None;
        }
        let page = file
            .index_pages
            .iter()
            .find(|page| page.key_index == key_index && page.page_number == child)?;
        path.push(page.page_number);
        let _ = key;
        if page.link_a == 0xFFFF_FFFF {
            let predecessor = page.entries.last()?.record_address;
            return Some((predecessor, path));
        }
        child = page.link_a >> 16;
    }
    None
}

fn key_only_entry(
    file: &BtrieveFile,
    spec: &FileSpec,
    key_index: usize,
    serial: u32,
) -> Option<(u32, u32)> {
    let key = spec.keys.get(key_index)?;
    let page_size = file.page_size.bytes();
    for page in file
        .index_pages
        .iter()
        .filter(|page| page.key_index == key_index)
    {
        for (position, entry) in page.entries.iter().enumerate() {
            if entry.record_address == serial {
                let display =
                    page.page_number as usize * page_size + 0x10 + position * key.entry_size() - 4;
                return Some((display as u32, serial));
            }
        }
    }
    None
}

fn underflow_delete_path(
    file: &BtrieveFile,
    key_index: usize,
    path: &[u32],
    address: u32,
) -> Option<(Vec<u32>, Option<u32>)> {
    if path.len() < 2 {
        return None;
    }
    let leaf = *path.last()?;
    let parent = path[path.len() - 2];
    let leaf_page = file
        .index_pages
        .iter()
        .find(|page| page.key_index == key_index && page.page_number == leaf)?;
    if !leaf_page
        .entries
        .iter()
        .any(|entry| entry.record_address == address)
    {
        return None;
    }
    let definition = file.fcr.key_defs.get(key_index)?;
    let minimum = definition.min_keys_per_page as usize;
    if (leaf_page.count as usize).saturating_sub(1) >= minimum {
        return None;
    }
    let parent_page = file
        .index_pages
        .iter()
        .find(|page| page.key_index == key_index && page.page_number == parent)?;
    let mut children = Vec::with_capacity(parent_page.entries.len() + 1);
    children.push(parent_page.link_b >> 16);
    for entry in parent_page
        .entries
        .iter()
        .take(parent_page.entries.len().saturating_sub(1))
    {
        children.push(entry.next_link >> 16);
    }
    children.push(parent_page.link_a >> 16);
    let position = match children.iter().position(|child| *child == leaf) {
        Some(position) => position,
        None => {
            return None;
        }
    };
    let sibling = if position + 1 < children.len() {
        children[position + 1]
    } else if position > 0 {
        children[position - 1]
    } else {
        return None;
    };
    let sibling_page = file
        .index_pages
        .iter()
        .find(|page| page.key_index == key_index && page.page_number == sibling)?;
    let mut result = path.to_vec();
    let extra = if sibling_page.count as usize > minimum {
        if let Some(last) = result.last_mut() {
            *last = parent;
        }
        None
    } else {
        result.pop();
        Some(parent)
    };
    Some((result, extra))
}

fn choose_key_number(spec: &FileSpec, key_number: i16) -> Option<usize> {
    if spec.keys.is_empty() {
        None
    } else if key_number >= 0 && (key_number as usize) < spec.keys.len() {
        Some(key_number as usize)
    } else {
        Some(0)
    }
}

fn put_u16(buffer: &mut [u8], offset: usize, value: u16) {
    if offset + 2 <= buffer.len() {
        buffer[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
}

fn put_u32(buffer: &mut [u8], offset: usize, value: u32) {
    if offset + 4 <= buffer.len() {
        buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

fn write_output(data: &mut [u8], data_len: &mut u16, value: &[u8]) -> Result<(), Status> {
    let capacity = (*data_len as usize).min(data.len());
    if capacity < value.len() {
        return Err(DATA_BUFFER_LENGTH);
    }
    data[..value.len()].copy_from_slice(value);
    *data_len = value.len() as u16;
    Ok(())
}

fn assign_autoincrement(record: &mut [u8], spec: &FileSpec, records: &[(u32, Vec<u8>)]) {
    for key in &spec.keys {
        if key.key_type != 15 || key.segments.len() != 1 {
            continue;
        }
        let segment = &key.segments[0];
        let Some(range) = segment_range(record, segment) else {
            continue;
        };
        if record[range.clone()].iter().all(|byte| *byte == 0) {
            let mut maximum = 0u64;
            for (_, existing) in records {
                if let Some(existing_range) = segment_range(existing, segment) {
                    let value = signed_le_abs(&existing[existing_range]);
                    maximum = maximum.max(value);
                }
            }
            let next = maximum.saturating_add(1);
            write_unsigned_le(&mut record[range], next);
        }
    }
}

fn segment_range(record: &[u8], segment: &KeySegment) -> Option<std::ops::Range<usize>> {
    let start = segment.position.checked_sub(1)? as usize;
    let end = start.checked_add(segment.length as usize)?;
    (end <= record.len()).then_some(start..end)
}

fn signed_le_abs(bytes: &[u8]) -> u64 {
    if bytes.is_empty() || bytes.len() > 8 {
        let mut value = 0u64;
        for byte in bytes.iter().rev() {
            value = value.wrapping_mul(256).wrapping_add(*byte as u64);
        }
        return value >> ((8 - bytes.len().min(8)) * 8);
    }
    let mut buffer = [0u8; 8];
    buffer[..bytes.len()].copy_from_slice(bytes);
    let value = i64::from_le_bytes(buffer);
    value.unsigned_abs()
}

fn write_unsigned_le(destination: &mut [u8], value: u64) {
    for (index, byte) in destination.iter_mut().enumerate() {
        *byte = ((value >> (index * 8)) & 0xFF) as u8;
    }
}

/// Convenience helper for tests and embedders.
pub fn call_with_key(
    engine: &mut Engine,
    op: u16,
    position: &mut PositionBlock,
    data: &mut [u8],
    data_len: &mut u16,
    key: &[u8],
    key_number: i16,
) -> Status {
    let mut key_buffer = vec![0u8; 255];
    let length = key.len().min(key_buffer.len());
    key_buffer[..length].copy_from_slice(&key[..length]);
    engine.call(op, position, data, data_len, &mut key_buffer, key_number)
}
