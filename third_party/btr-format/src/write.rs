//! Writer for Btrieve/N 5.10a fixed-length files.
//!
//! The writer reproduces the engine's Create and Insert layout for the
//! fixture corpus: sequential page allocation, 6-byte data-page headers,
//! one-level B-tree splits, duplicate-key chains, and the FCR/page-header
//! counters observed in `docs/btrieve-5.10a-file-format.md`.
//!
//! Supported today: fixed-length records, one or more keys, unique and
//! duplicate keys, one-level leaf split (capacity + 1 entries).  Variable-
//! length inserts, compression, key-only files, preallocation, and deeper
//! B-tree splits return `Unsupported`.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::error::{FormatError, Result};
use crate::file::BtrieveFile;
use crate::key::{compare_composite, Acs, KeySegment};
use crate::page::{PageSize, DATA_PAGE_FLAG, KEY_DEFS_OFFSET, KEY_DEF_SIZE, PAGE_HEADER_SIZE};

/// One logical key definition.  A key may consist of several segments; the
/// convenience fields mirror the first segment and the aggregate length.
#[derive(Debug, Clone, Serialize)]
pub struct KeySpec {
    /// 1-based byte position of the first segment within the record.
    pub position: u16,
    /// Total key length (sum of the segment lengths).
    pub length: u16,
    /// Flags of the first segment.
    pub flags: u16,
    pub key_type: u8,
    pub null_value: u8,
    /// Every segment in key order.
    pub segments: Vec<KeySegment>,
    /// Entry size override for key-only files, where an index entry stores
    /// the full record instead of a record address.
    pub entry_size_override: Option<u16>,
}

impl KeySpec {
    pub fn new(position: u16, length: u16, flags: u16, key_type: u8, null_value: u8) -> Self {
        Self {
            position,
            length,
            flags,
            key_type,
            null_value,
            segments: vec![KeySegment::new(
                position, length, flags, key_type, null_value,
            )],
            entry_size_override: None,
        }
    }

    /// Build a segmented key.  The segmented flag (0x0010) is set on every
    /// segment except the last, matching the Create specification blocks.
    pub fn segmented(mut segments: Vec<KeySegment>) -> Result<Self> {
        if segments.is_empty() {
            return Err(FormatError::InvalidSpec("empty key segment list"));
        }
        let last = segments.len() - 1;
        for (index, segment) in segments.iter_mut().enumerate() {
            if index < last {
                segment.flags |= 0x0010;
            } else {
                segment.flags &= !0x0010;
            }
        }
        Ok(Self::from_segments(segments))
    }

    /// Construct from already-normalized segment blocks (used when rebuilding
    /// a file from its on-disk key definitions).
    pub fn from_segments(segments: Vec<KeySegment>) -> Self {
        let first = &segments[0];
        let length = segments
            .iter()
            .map(|segment| segment.length as u32)
            .sum::<u32>()
            .min(u16::MAX as u32) as u16;
        Self {
            position: first.position,
            length,
            flags: first.flags,
            key_type: first.key_type,
            null_value: first.null_value,
            segments,
            entry_size_override: None,
        }
    }

    pub fn has_duplicates(&self) -> bool {
        self.flags & 0x0001 != 0
    }

    pub fn is_extended(&self) -> bool {
        self.segments.iter().any(KeySegment::is_extended_type)
    }

    pub fn has_acs(&self) -> bool {
        self.segments.iter().any(KeySegment::has_acs)
    }

    pub const fn is_supplemental(&self) -> bool {
        self.flags & 0x0080 != 0
    }

    /// Supplemental duplicate indexes store one entry per record but do not
    /// reserve a duplicate address field or record tail.
    pub fn uses_duplicate_tail(&self) -> bool {
        self.has_duplicates() && !self.is_supplemental()
    }

    pub fn has_null(&self) -> bool {
        self.segments.iter().any(KeySegment::has_null)
    }

    pub fn is_manual(&self) -> bool {
        self.segments.iter().any(KeySegment::is_manual)
    }

    pub fn is_segmented(&self) -> bool {
        self.segments.len() > 1
    }

    pub fn entry_size(&self) -> usize {
        if let Some(size) = self.entry_size_override {
            return size as usize;
        }
        self.length as usize + if self.has_duplicates() { 12 } else { 8 }
    }

    /// Extract the composite key bytes (segments concatenated) from a record.
    pub fn build_key(&self, record: &[u8]) -> Result<Vec<u8>> {
        let mut key = Vec::with_capacity(self.length as usize);
        for segment in &self.segments {
            let bytes = segment.bytes(record).ok_or(FormatError::KeyOutOfRange {
                index: 0,
                position: segment.position,
                length: segment.length,
                record_length: record.len(),
            })?;
            key.extend_from_slice(bytes);
        }
        Ok(key)
    }

    /// Null/manual records are stored but omitted from the index.
    pub fn is_excluded_from_index(&self, record: &[u8]) -> bool {
        let manual = self.is_manual();
        let null = self.has_null();
        if !manual && !null {
            return false;
        }
        let states = self
            .segments
            .iter()
            .map(|segment| segment.all_null(record))
            .collect::<Vec<_>>();
        if manual {
            states.iter().any(|&state| state)
        } else {
            states.iter().all(|&state| state)
        }
    }

    pub fn compare(&self, a: &[u8], b: &[u8], acs: Option<&Acs>) -> Ordering {
        compare_composite(a, b, &self.segments, acs)
    }
}

/// Owner-name hash and access/encryption mode.  Use [`OwnerInfo::from_name`]
/// to derive the engine's nine-byte hash, or preserve the hash read from an
/// existing file.
#[derive(Debug, Clone, Serialize)]
pub struct OwnerInfo {
    pub hash: [u8; 9],
    pub mode: u8,
}

impl OwnerInfo {
    /// Derive the engine's owner-name hash: the first byte is the sum of the
    /// eight space-padded name bytes; the remaining eight bytes are four
    /// 16-bit words transformed by rotates/negation chosen by the sum parity.
    pub fn from_name(name: &[u8], mode: u8) -> Self {
        let mut padded = [b' '; 8];
        for (index, byte) in name.iter().take(8).enumerate() {
            if *byte == 0 {
                break;
            }
            padded[index] = *byte;
        }
        let sum = padded
            .iter()
            .fold(0u8, |accumulator, byte| accumulator.wrapping_add(*byte));
        let word = |index: usize| -> u16 { u16::from_le_bytes([padded[index], padded[index + 1]]) };
        let mut hash = [0u8; 9];
        hash[0] = sum;
        let (w0, w1, w2, w3) = (word(0), word(2), word(4), word(6));
        if sum & 1 == 0 {
            hash[1..3].copy_from_slice(&w1.wrapping_neg().to_le_bytes());
            hash[3..5].copy_from_slice(&w3.rotate_right(1).to_le_bytes());
            hash[5..7].copy_from_slice(&w2.rotate_left(5).to_le_bytes());
            hash[7..9].copy_from_slice(&w0.rotate_right(3).to_le_bytes());
        } else {
            hash[1..3].copy_from_slice(&w2.wrapping_neg().to_le_bytes());
            hash[3..5].copy_from_slice(&w0.rotate_right(1).to_le_bytes());
            hash[5..7].copy_from_slice(&w1.rotate_left(1).to_le_bytes());
            hash[7..9].copy_from_slice(&w3.rotate_left(3).to_le_bytes());
        }
        Self { hash, mode }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FileSpec {
    pub record_length: u16,
    pub page_size: u16,
    pub file_flags: u16,
    pub preallocate: u16,
    pub keys: Vec<KeySpec>,
    /// Alternate collating sequence, required when any key segment sets the
    /// ACS flag.
    pub acs: Option<Acs>,
    /// Owner hash/mode when the file requires an owner name.
    pub owner: Option<OwnerInfo>,
    /// Extension file name stored at FCR +0x98 after an Extend operation.
    pub extension: Option<String>,
}

impl FileSpec {
    pub fn new(
        record_length: u16,
        page_size: u16,
        file_flags: u16,
        preallocate: u16,
        keys: Vec<KeySpec>,
    ) -> Result<Self> {
        Self::new_with_acs(
            record_length,
            page_size,
            file_flags,
            preallocate,
            keys,
            None,
        )
    }

    /// Construct a specification with an optional alternate collating
    /// sequence.  Use this (rather than [`FileSpec::new`]) when any key
    /// segment sets the ACS flag.
    pub fn new_with_acs(
        record_length: u16,
        page_size: u16,
        file_flags: u16,
        preallocate: u16,
        keys: Vec<KeySpec>,
        acs: Option<Acs>,
    ) -> Result<Self> {
        let mut spec = Self {
            record_length,
            page_size,
            file_flags,
            preallocate,
            keys,
            acs,
            owner: None,
            extension: None,
        };
        if spec.is_key_only() {
            for key in &mut spec.keys {
                key.entry_size_override = Some(record_length.saturating_add(8));
            }
        }
        spec.validate_spec()?;
        Ok(spec)
    }

    pub fn from_file(file: &BtrieveFile) -> Result<Self> {
        let keys = file
            .fcr
            .key_defs
            .iter()
            .map(|key| KeySpec::from_segments(key.segments.clone()))
            .collect();
        let record_length = if file.fcr.file_flags.compression() {
            file.fcr.compressed_record_length
        } else {
            file.fcr.logical_record_length
        };
        let mut spec = Self::new_with_acs(
            record_length,
            file.page_size.as_u16(),
            file.fcr.file_flags.raw(),
            if file.fcr.file_flags.preallocation() {
                file.fcr.preallocated_pages
            } else {
                0
            },
            keys,
            file.acs.clone(),
        )?;
        if file.fcr.owner_present {
            spec.owner = Some(OwnerInfo {
                hash: file.fcr.owner_hash,
                mode: file.fcr.owner_mode,
            });
        }
        spec.extension = file.fcr.extension_name.clone();
        Ok(spec)
    }

    pub fn has_acs(&self) -> bool {
        self.keys.iter().any(KeySpec::has_acs)
    }

    /// Set an owner name (mode 0-3; modes 2/3 also encrypt data pages).
    pub fn set_owner_name(&mut self, name: &[u8], mode: u8) {
        self.owner = Some(OwnerInfo::from_name(name, mode));
    }

    pub fn total_segments(&self) -> usize {
        self.keys.iter().map(|key| key.segments.len()).sum()
    }

    pub fn validate_spec(&self) -> Result<()> {
        if self.record_length == 0 {
            return Err(FormatError::InvalidSpec("zero record length"));
        }
        if !matches!(self.page_size, 512 | 1024 | 2048 | 4096) {
            return Err(FormatError::InvalidPageSize(self.page_size));
        }
        if self.is_key_only() && self.keys.len() != 1 {
            return Err(FormatError::InvalidSpec(
                "key-only files require exactly one key",
            ));
        }
        let key_table_end = KEY_DEFS_OFFSET + self.total_segments() * KEY_DEF_SIZE;
        if key_table_end > self.page_size as usize {
            return Err(FormatError::InvalidSpec("key table does not fit the FCR"));
        }
        for key in &self.keys {
            if key.segments.is_empty() {
                return Err(FormatError::InvalidSpec("key has no segments"));
            }
            if key.length == 0 || key.length > 255 {
                return Err(FormatError::InvalidSpec("invalid total key length"));
            }
            for segment in &key.segments {
                if segment.position == 0 || segment.length == 0 {
                    return Err(FormatError::InvalidSpec("zero key position or length"));
                }
                if segment.position as usize - 1 + segment.length as usize
                    > self.record_length as usize
                {
                    return Err(FormatError::InvalidSpec("key segment exceeds the record"));
                }
            }
            let segment_sum = key
                .segments
                .iter()
                .map(|segment| segment.length as usize)
                .sum::<usize>();
            if segment_sum != key.length as usize {
                return Err(FormatError::InvalidSpec(
                    "key length does not match its segments",
                ));
            }
            if key.entry_size() + 12 > self.page_size as usize {
                return Err(FormatError::InvalidSpec("index entry does not fit a page"));
            }
            if key.entry_size() >= self.page_size as usize {
                return Err(FormatError::InvalidSpec("invalid key entry size"));
            }
        }
        if self.has_acs() {
            if self.acs.is_none() {
                return Err(FormatError::InvalidSpec(
                    "alternate collating sequence is missing",
                ));
            }
            if PAGE_HEADER_SIZE + Acs::SIZE > self.page_size as usize {
                return Err(FormatError::InvalidSpec(
                    "alternate collating sequence does not fit a page",
                ));
            }
        }
        let physical = self.physical_record_length() as usize;
        if physical == 0 || physical > self.page_size as usize - 6 {
            return Err(FormatError::InvalidSpec("record does not fit a page"));
        }
        Ok(())
    }

    pub fn page_size_bytes(&self) -> usize {
        self.page_size as usize
    }

    pub fn has_variable_length(&self) -> bool {
        self.file_flags & 0x0001 != 0
    }

    pub fn has_blank_truncation(&self) -> bool {
        self.file_flags & 0x0002 != 0
    }

    pub fn duplicate_key_count(&self) -> usize {
        self.keys
            .iter()
            .filter(|key| key.uses_duplicate_tail())
            .count()
    }

    pub fn is_compressed(&self) -> bool {
        self.file_flags & 0x0008 != 0
    }

    pub fn is_key_only(&self) -> bool {
        self.file_flags & 0x0010 != 0
    }

    /// Compressed files always use the variable-page layout, even when the
    /// variable-length flag is clear.
    pub fn uses_variable_pages(&self) -> bool {
        self.is_compressed() || self.has_variable_length()
    }

    pub fn physical_record_length(&self) -> u16 {
        if self.is_key_only() {
            return self
                .keys
                .first()
                .map(|key| key.entry_size() as u16)
                .unwrap_or(self.record_length.saturating_add(8));
        }
        let length = if self.is_compressed() {
            // 4-byte variable pointer plus the compressed-record marker.
            5
        } else {
            let mut length = self.record_length;
            if self.has_variable_length() {
                length = length.saturating_add(4);
                if self.has_blank_truncation() {
                    length = length.saturating_add(2);
                }
            }
            length
        };
        length.saturating_add(8 * self.duplicate_key_count() as u16)
    }

    pub fn format_word(&self) -> u16 {
        if self.owner.is_some() {
            return 0xFD00;
        }
        if self.is_compressed() || self.is_key_only() {
            return 0x0500;
        }
        let extended = self
            .keys
            .iter()
            .any(|key| key.is_extended() || key.flags & 0x0080 != 0);
        if extended {
            0x0400
        } else {
            0x0300
        }
    }

    pub fn variable_meta(&self) -> [u16; 2] {
        if self.has_variable_length() {
            if self.has_blank_truncation() {
                [0xFFFD, 0xFFFF]
            } else {
                [0xFFFF, 0xFFFF]
            }
        } else {
            [0, 0]
        }
    }

    pub fn variable_aux(&self) -> u16 {
        if self.has_variable_length() {
            0x0019
        } else {
            0
        }
    }

    pub fn create_empty(&self) -> Result<Vec<u8>> {
        Writer::new(self.clone())?.finish()
    }
}

#[derive(Debug, Clone)]
struct EntryState {
    key: Vec<u8>,
    address: u32,
    duplicate: Option<u32>,
    /// Interior pages use this as the child pointer to the right of this key;
    /// leaf entries leave it zero.
    link: u32,
    /// Key-only files store the full record in the index entry.
    record: Vec<u8>,
    /// Key-only record serial number (stored high-word first).
    serial: u32,
}

/// One B-tree page.  Level 0 pages are leaves whose entries carry record
/// addresses; higher levels are interior nodes whose entry links and
/// left/right pointers name child pages.
#[derive(Debug, Clone)]
struct NodeState {
    page: u32,
    /// Page-header previous word (0xFF00 on supplemental index pages).
    previous: u16,
    level: u8,
    entries: Vec<EntryState>,
    /// Leftmost child (interior nodes only).
    left: u32,
    /// Rightmost child (interior nodes only).
    right: u32,
    aux: u16,
}

#[derive(Debug, Clone)]
struct TreeState {
    key: KeySpec,
    capacity: usize,
    nodes: Vec<NodeState>,
    /// Index of the root node in `nodes`.
    root: usize,
    acs: Option<Acs>,
    /// Largest absolute autoincrement key value inserted so far.
    autoinc_max_abs: i128,
}

impl TreeState {
    fn find_entry(&self, key: &[u8]) -> Option<(usize, usize)> {
        for (node_index, node) in self.nodes.iter().enumerate() {
            if let Some(entry_index) = node.entries.iter().position(|entry| {
                self.key
                    .compare(entry.key.as_slice(), key, self.acs.as_ref())
                    == Ordering::Equal
            }) {
                return Some((node_index, entry_index));
            }
        }
        None
    }

    fn find_any(&self, key: &[u8]) -> bool {
        self.find_entry(key).is_some()
    }

    /// Walk from the root to the leaf that should contain `key`.
    fn target_leaf(&self, key: &[u8]) -> Result<(usize, Vec<usize>)> {
        let mut node_index = self.root;
        let mut path = Vec::new();
        loop {
            let node = &self.nodes[node_index];
            if node.level == 0 {
                return Ok((node_index, path));
            }
            let position = node.entries.partition_point(|entry| {
                self.key
                    .compare(entry.key.as_slice(), key, self.acs.as_ref())
                    == Ordering::Less
            });
            if position < node.entries.len()
                && self.key.compare(
                    node.entries[position].key.as_slice(),
                    key,
                    self.acs.as_ref(),
                ) == Ordering::Equal
            {
                return Err(FormatError::DuplicateKey);
            }
            let target = if position == 0 {
                node.left
            } else if position == node.entries.len() {
                node.right
            } else {
                node.entries[position - 1].link
            };
            path.push(node_index);
            node_index = self
                .nodes
                .iter()
                .position(|candidate| candidate.page == target)
                .ok_or(FormatError::InvalidSpec("tree child page is missing"))?;
        }
    }

    fn index_page(&self) -> u32 {
        self.nodes[self.root].page
    }

    fn entry_count(&self) -> usize {
        self.nodes
            .iter()
            .map(|node| node.entries.len())
            .sum::<usize>()
    }
}
#[derive(Debug, Clone)]
struct DataSlot {
    address: u32,
    /// Fixed-length portion of the record.
    data: Vec<u8>,
    /// Big-endian (page, slot) pair of the variable portion, or `0xFFFFFFFF`.
    variable_pointer: u32,
    /// Trailing blanks removed by blank truncation.
    truncated_blanks: u16,
    /// Compressed-record marker byte (1 for compressed files).
    compression_marker: u8,
    /// Full logical record, used to build supplemental indexes later.
    full_record: Vec<u8>,
    /// One [previous, next] pair per duplicate-enabled key, in key order.
    tails: Vec<[u32; 2]>,
}

#[derive(Debug, Clone)]
struct DataPageState {
    page: u32,
    slots: Vec<DataSlot>,
    aux: u16,
}

#[derive(Debug, Clone)]
struct VariableSlot {
    /// Variable bytes stored in this chunk (a continuation chunk stores the
    /// bytes that precede the chunk named by `pointer`).
    data: Vec<u8>,
    /// Continuation chunks start with a big-endian (page, slot) pointer to
    /// the next chunk.
    continuation: bool,
    pointer: u32,
}

impl VariableSlot {
    fn length(&self) -> usize {
        self.data.len() + if self.continuation { 4 } else { 0 }
    }
}

#[derive(Debug, Clone)]
struct VariablePageState {
    page: u32,
    slots: Vec<VariableSlot>,
    data_end: usize,
    aux: u16,
}

impl VariablePageState {
    fn new(page: u32) -> Self {
        Self {
            page,
            slots: Vec::new(),
            data_end: PAGE_HEADER_SIZE + 6,
            aux: 0,
        }
    }

    /// Bytes available for a new slot, whose boundary table entry grows the
    /// page-end table by one 16-bit value.
    fn capacity(&self, page_size: usize) -> usize {
        let slot_count = self.slots.len() + 1;
        let table_size = 2 * (slot_count + 1);
        page_size
            .saturating_sub(table_size)
            .saturating_sub(self.data_end)
    }

    fn push(&mut self, slot: VariableSlot) {
        self.data_end += slot.length();
        self.slots.push(slot);
        let count = self.slots.len();
        self.aux = if count > 1 { (2 * count - 1) as u16 } else { 0 };
    }

    fn is_full(&self, page_size: usize) -> bool {
        let table_size = 2 * (self.slots.len() + 1);
        self.data_end >= page_size - table_size
    }
}

/// Builder for a new file.  Insert records with [`Writer::insert`], then call
/// [`Writer::finish`] to obtain the file bytes.
pub struct Writer {
    spec: FileSpec,
    page_size: PageSize,
    next_page: u32,
    record_count: u32,
    /// Successful insert/update/supplemental/drop operations, used for the
    /// engine's aux counters.
    operation_count: u32,
    /// Freed index pages, newest head first via each page's link A.
    free_pages: Vec<u32>,
    data_pages: Vec<DataPageState>,
    variable_pages: Vec<VariablePageState>,
    trees: Vec<TreeState>,
    duplicate_slots: Vec<Option<usize>>,
}

impl Writer {
    pub fn new(spec: FileSpec) -> Result<Self> {
        spec.validate_spec()?;
        let page_size = PageSize::new(spec.page_size)?;
        let key_count = spec.keys.len();
        let acs_page = spec.has_acs().then_some(1u32);
        let index_base = if acs_page.is_some() { 2u32 } else { 1u32 };
        let mut trees = Vec::with_capacity(key_count);
        let mut duplicate_slots = vec![None; key_count];
        let mut duplicate_slot = 0usize;
        for (index, key) in spec.keys.iter().enumerate() {
            if key.uses_duplicate_tail() && !spec.is_key_only() {
                duplicate_slots[index] = Some(duplicate_slot);
                duplicate_slot += 1;
            }
            let capacity = (spec.page_size as usize - 12) / key.entry_size();
            trees.push(TreeState {
                key: key.clone(),
                capacity,
                nodes: vec![NodeState {
                    page: index_base + index as u32,
                    previous: 0,
                    level: 0,
                    entries: Vec::new(),
                    left: 0,
                    right: 0,
                    aux: 0,
                }],
                root: 0,
                acs: spec.acs.clone(),
                autoinc_max_abs: 0,
            });
        }
        let data_page = index_base + key_count as u32;
        let next_page = data_page + 1;
        let data_pages = if spec.is_key_only() {
            Vec::new()
        } else {
            vec![DataPageState {
                page: data_page,
                slots: Vec::new(),
                aux: 0,
            }]
        };
        let next_page = if spec.is_key_only() {
            index_base + key_count as u32
        } else {
            next_page
        };
        Ok(Self {
            spec,
            page_size,
            next_page,
            record_count: 0,
            operation_count: 0,
            free_pages: Vec::new(),
            data_pages,
            variable_pages: Vec::new(),
            trees,
            duplicate_slots,
        })
    }

    /// Build a writer from a specification and a sequence of existing records.
    /// Records are re-inserted in order; this is the basis for inserting into,
    /// updating, and deleting from an existing file.
    pub fn from_records(spec: FileSpec, records: &[Vec<u8>]) -> Result<Self> {
        let mut writer = Self::new(spec)?;
        for record in records {
            writer.insert(record)?;
        }
        Ok(writer)
    }

    /// Rebuild a writer from an existing file's specification and records.
    pub fn from_file(file: &BtrieveFile) -> Result<Self> {
        let spec = FileSpec::from_file(file)?;
        let records = file
            .records()?
            .into_iter()
            .map(|record| record.data)
            .collect::<Vec<_>>();
        Self::from_records(spec, &records)
    }

    /// Insert one fixed-length record.  Keys are extracted from the record.
    /// For a duplicate-enabled key, an existing key value updates the
    /// duplicate chain instead of adding a second index entry.
    pub fn insert(&mut self, record: &[u8]) -> Result<()> {
        let logical_length = self.spec.record_length as usize;
        if self.spec.is_compressed() || self.spec.has_variable_length() {
            if record.len() < logical_length {
                return Err(FormatError::RecordLengthMismatch {
                    expected: self.spec.record_length,
                    actual: record.len(),
                });
            }
        } else if record.len() != logical_length {
            return Err(FormatError::RecordLengthMismatch {
                expected: self.spec.record_length,
                actual: record.len(),
            });
        }
        let keys = self.spec.keys.clone();
        let mut record = record.to_vec();

        // A zero-valued autoincrement key is assigned the next absolute value
        // and written back into the record before any key is extracted.
        for (index, key) in keys.iter().enumerate() {
            if key.key_type == 15 && key.segments.len() == 1 {
                let segment = &key.segments[0];
                let start = segment.position as usize - 1;
                let end = start + segment.length as usize;
                let slice = record.get(start..end).ok_or(FormatError::KeyOutOfRange {
                    index,
                    position: segment.position,
                    length: segment.length,
                    record_length: record.len(),
                })?;
                if slice.iter().all(|&byte| byte == 0) {
                    let next = self.trees[index].autoinc_max_abs.saturating_add(1);
                    let record_length = record.len();
                    let destination =
                        record
                            .get_mut(start..end)
                            .ok_or(FormatError::KeyOutOfRange {
                                index,
                                position: segment.position,
                                length: segment.length,
                                record_length,
                            })?;
                    write_signed_le(destination, next)?;
                }
            }
        }

        if self.spec.is_compressed() && !self.spec.has_variable_length() {
            record.truncate(logical_length);
        }

        let mut key_values = Vec::with_capacity(keys.len());
        let mut excluded = Vec::with_capacity(keys.len());
        for key in &keys {
            let value = key.build_key(&record)?;
            excluded.push(key.is_excluded_from_index(&record));
            key_values.push(value);
        }

        let mut existing: Vec<Option<(usize, usize)>> = vec![None; keys.len()];
        for (index, key) in keys.iter().enumerate() {
            if excluded[index] {
                continue;
            }
            if key.has_duplicates() {
                existing[index] = self.trees[index].find_entry(&key_values[index]);
            } else if self.trees[index].find_any(&key_values[index]) {
                return Err(FormatError::DuplicateKey);
            }
        }

        if self.spec.is_key_only() {
            if keys.len() != 1 {
                return Err(FormatError::InvalidSpec(
                    "key-only files require exactly one key",
                ));
            }
            if excluded[0] {
                return Err(FormatError::Unsupported("null key in a key-only file"));
            }
            let serial = self.record_count;
            self.record_count += 1;
            self.operation_count += 1;
            let aux = (self.operation_count.saturating_mul(2) - 1) as u16;
            insert_into_tree(
                &mut self.trees[0],
                &mut self.next_page,
                EntryState {
                    key: key_values[0].clone(),
                    address: serial,
                    duplicate: None,
                    link: 0,
                    record,
                    serial,
                },
                aux,
            )?;
            if keys[0].key_type == 15 && keys[0].segments.len() == 1 {
                let absolute = signed_le_abs(&key_values[0]);
                self.trees[0].autoinc_max_abs = self.trees[0].autoinc_max_abs.max(absolute);
            }
            return Ok(());
        }

        let full_record = record.clone();
        self.operation_count += 1;
        let physical = self.spec.physical_record_length() as usize;
        let page_capacity = (self.page_size.bytes() - PAGE_HEADER_SIZE) / physical;
        if self
            .data_pages
            .last()
            .map(|page| page.slots.len() >= page_capacity)
            .unwrap_or(true)
        {
            let page = self.next_page;
            self.next_page += 1;
            self.data_pages.push(DataPageState {
                page,
                slots: Vec::new(),
                aux: 0,
            });
        }
        let page_index = self.data_pages.len() - 1;
        let offset = PAGE_HEADER_SIZE + self.data_pages[page_index].slots.len() * physical;
        let address =
            self.data_pages[page_index].page * self.page_size.bytes() as u32 + offset as u32;

        let (fixed_data, variable_pointer, truncated_blanks, compression_marker) =
            if self.spec.is_compressed() {
                let image = compress_record(&record);
                let pointer = if image.is_empty() {
                    0xFFFF_FFFF
                } else {
                    self.allocate_variable(&image)?
                };
                (Vec::new(), pointer, 0, 1)
            } else {
                let mut variable = if self.spec.has_variable_length() {
                    record.split_off(logical_length)
                } else {
                    Vec::new()
                };
                let mut truncated_blanks = 0u16;
                if self.spec.has_blank_truncation() {
                    while variable.last() == Some(&b' ') {
                        variable.pop();
                        truncated_blanks = truncated_blanks.saturating_add(1);
                    }
                }
                let pointer = if variable.is_empty() {
                    0xFFFF_FFFF
                } else {
                    self.allocate_variable(&variable)?
                };
                (record, pointer, truncated_blanks, 0)
            };

        let duplicate_count = self.spec.duplicate_key_count();
        // Null/manual records carry an all-zero duplicate tail rather than a
        // link, matching the engine.
        let mut tails = vec![[0u32; 2]; duplicate_count];
        let mut duplicate_updates: Vec<(usize, u32)> = Vec::new();
        for (index, _key) in keys.iter().enumerate() {
            let Some(slot) = self.duplicate_slots[index] else {
                continue;
            };
            if excluded[index] {
                tails[slot] = [0, 0];
                continue;
            }
            tails[slot] = [u32::MAX, u32::MAX];
            let Some((leaf_index, entry_index)) = existing[index] else {
                continue;
            };
            let entry = &mut self.trees[index].nodes[leaf_index].entries[entry_index];
            let previous_last = entry.duplicate.unwrap_or(entry.address);
            entry.duplicate = Some(address);
            duplicate_updates.push((slot, previous_last));
            tails[slot] = [previous_last, u32::MAX];
        }
        for (slot, previous_last) in duplicate_updates {
            self.update_tail(previous_last, slot, address)?;
        }

        self.data_pages[page_index].slots.push(DataSlot {
            address,
            data: fixed_data,
            variable_pointer,
            truncated_blanks,
            compression_marker,
            full_record,
            tails,
        });
        self.record_count += 1;
        let aux = (self.operation_count.saturating_mul(2) - 1) as u16;
        self.data_pages[page_index].aux = aux;

        for (index, key) in keys.iter().enumerate() {
            if excluded[index] {
                continue;
            }
            if let Some((leaf_index, _)) = existing[index] {
                self.trees[index].nodes[leaf_index].aux = aux;
            } else {
                insert_into_tree(
                    &mut self.trees[index],
                    &mut self.next_page,
                    EntryState {
                        key: key_values[index].clone(),
                        address,
                        duplicate: if key.has_duplicates() {
                            Some(address)
                        } else {
                            None
                        },
                        link: 0,
                        record: Vec::new(),
                        serial: 0,
                    },
                    aux,
                )?;
            }
            if key.key_type == 15 && key.segments.len() == 1 {
                let absolute = signed_le_abs(&key_values[index]);
                self.trees[index].autoinc_max_abs = self.trees[index].autoinc_max_abs.max(absolute);
            }
        }
        Ok(())
    }

    fn new_variable_page(&mut self) -> usize {
        let page = self.next_page;
        self.next_page += 1;
        self.variable_pages.push(VariablePageState::new(page));
        self.variable_pages.len() - 1
    }

    /// Store a variable portion in chunk slots and return the big-endian
    /// (page, slot) pointer of the chunk named by the fixed record tail.
    fn allocate_variable(&mut self, data: &[u8]) -> Result<u32> {
        let page_size = self.page_size.bytes();
        let mut index = match self.variable_pages.len() {
            0 => self.new_variable_page(),
            count => count - 1,
        };
        if self.variable_pages[index].capacity(page_size) == 0 {
            index = self.new_variable_page();
        }
        let capacity = self.variable_pages[index].capacity(page_size);
        if capacity == 0 {
            return Err(FormatError::Unsupported("variable page has no room"));
        }
        let take = capacity.min(data.len());
        let payload = data[data.len() - take..].to_vec();
        let page = self.variable_pages[index].page;
        let slot_index = self.variable_pages[index].slots.len();
        let mut next_pointer = (page << 16) | slot_index as u32;
        self.variable_pages[index].push(VariableSlot {
            data: payload,
            continuation: false,
            pointer: 0,
        });

        let mut remaining = &data[..data.len() - take];
        while !remaining.is_empty() {
            let index = self.new_variable_page();
            let capacity = self.variable_pages[index].capacity(page_size);
            if capacity <= 4 {
                return Err(FormatError::Unsupported("variable chunk does not fit"));
            }
            let take = (capacity - 4).min(remaining.len());
            let payload = remaining[remaining.len() - take..].to_vec();
            let page = self.variable_pages[index].page;
            let slot_index = self.variable_pages[index].slots.len();
            let pointer = (page << 16) | slot_index as u32;
            self.variable_pages[index].push(VariableSlot {
                data: payload,
                continuation: true,
                pointer: next_pointer,
            });
            next_pointer = pointer;
            remaining = &remaining[..remaining.len() - take];
        }
        Ok(next_pointer)
    }

    /// Add a supplemental index to an existing record set.  The new index
    /// pages are allocated after the current pages, matching the engine's
    /// Create Supplemental Index operation.
    pub fn add_supplemental(&mut self, mut key: KeySpec) -> Result<usize> {
        if self.spec.is_key_only() {
            return Err(FormatError::Unsupported(
                "supplemental index in a key-only file",
            ));
        }
        self.operation_count += 1;
        key.flags |= 0x0080;
        key.entry_size_override = Some(key.length.saturating_add(8));
        if let Some(segment) = key.segments.first_mut() {
            segment.flags |= 0x0080;
        }
        let page = self.next_page;
        self.next_page += 1;
        let capacity = (self.page_size.bytes() - 12) / key.entry_size();
        let tree_index = self.trees.len();
        self.trees.push(TreeState {
            key: key.clone(),
            capacity,
            nodes: vec![NodeState {
                page,
                previous: 0xFF00,
                level: 0,
                entries: Vec::new(),
                left: 0,
                right: 0,
                aux: 0,
            }],
            root: 0,
            acs: self.spec.acs.clone(),
            autoinc_max_abs: 0,
        });

        let mut index_entries = Vec::new();
        for data_page in &self.data_pages {
            for slot in &data_page.slots {
                if key.is_excluded_from_index(&slot.full_record) {
                    continue;
                }
                index_entries.push((key.build_key(&slot.full_record)?, slot.address));
            }
        }
        for (value, address) in index_entries {
            if !key.has_duplicates() && self.trees[tree_index].find_any(&value) {
                return Err(FormatError::DuplicateKey);
            }
            insert_into_tree(
                &mut self.trees[tree_index],
                &mut self.next_page,
                EntryState {
                    key: value,
                    address,
                    duplicate: None,
                    link: 0,
                    record: Vec::new(),
                    serial: 0,
                },
                0,
            )?;
        }
        let count = self.trees[tree_index].entry_count();
        for node in &mut self.trees[tree_index].nodes {
            node.aux = 0;
        }
        self.spec.keys.push(key);
        self.duplicate_slots.push(None);
        Ok(count)
    }

    /// Drop a supplemental index, freeing its index pages on the FCR page
    /// free list.  The file size is not reduced, matching the engine.
    pub fn drop_supplemental(&mut self, key_index: usize) -> Result<()> {
        if key_index == 0 || key_index >= self.spec.keys.len() {
            return Err(FormatError::InvalidSpec("invalid supplemental key number"));
        }
        if self.spec.keys[key_index].flags & 0x0080 == 0 {
            return Err(FormatError::InvalidSpec("key is not supplemental"));
        }
        let tree = self.trees.remove(key_index);
        self.duplicate_slots.remove(key_index);
        self.spec.keys.remove(key_index);
        self.operation_count += 1;

        let mut pages = tree.nodes.iter().map(|node| node.page).collect::<Vec<_>>();
        pages.sort_unstable_by(|left, right| right.cmp(left));
        let old = std::mem::take(&mut self.free_pages);
        self.free_pages = pages.into_iter().chain(old).collect();
        Ok(())
    }

    fn update_tail(&mut self, address: u32, duplicate_slot: usize, next: u32) -> Result<()> {
        for page in &mut self.data_pages {
            for slot in &mut page.slots {
                if slot.address == address {
                    slot.tails[duplicate_slot][1] = next;
                    return Ok(());
                }
            }
        }
        Err(FormatError::InvalidRecordAddress(address))
    }

    /// Finish the file and return its bytes.
    pub fn finish(self) -> Result<Vec<u8>> {
        if self.next_page > u16::MAX as u32 {
            return Err(FormatError::Unsupported("more than 65535 pages"));
        }
        let page_size = self.page_size.bytes();
        let physical_pages = self.next_page.max(self.spec.preallocate as u32) as usize;
        let mut bytes = vec![0u8; physical_pages * page_size];
        self.write_fcr(&mut bytes);
        self.write_acs_page(&mut bytes);
        self.write_data_pages(&mut bytes);
        self.write_variable_pages(&mut bytes);
        self.write_index_pages(&mut bytes);
        self.write_free_pages(&mut bytes);
        if let Some(owner) = &self.spec.owner {
            if owner.mode & 0x02 != 0 {
                let page_size = self.page_size.bytes();
                for page in 1..self.next_page as usize {
                    let start = page * page_size;
                    for offset in 0..page_size {
                        bytes[start + offset] ^=
                            crate::fcr::owner_mask_byte(&owner.hash, offset, page_size);
                    }
                }
            }
        }
        Ok(bytes)
    }

    pub fn record_count(&self) -> u32 {
        self.record_count
    }

    pub fn next_page(&self) -> u32 {
        self.next_page
    }

    fn last_data_page(&self) -> u32 {
        self.data_pages.last().map(|page| page.page).unwrap_or(1)
    }

    /// FCR +0x38/+0x3A: the low byte distinguishes the truncation mode, the
    /// high byte is 0xFF when no variable page has been allocated, and the
    /// second word names the most recently allocated variable page.
    fn variable_meta(&self) -> [u16; 2] {
        if !self.spec.uses_variable_pages() {
            return [0, 0];
        }
        let low = if self.spec.is_compressed() && !self.spec.has_variable_length() {
            0x0000u16
        } else if self.spec.has_blank_truncation() {
            0x00FDu16
        } else {
            0x00FFu16
        };
        match self.variable_pages.last() {
            Some(page) => [low, page.page as u16],
            None => [low | 0xFF00, 0xFFFF],
        }
    }

    fn free_bytes_last_data_page(&self) -> u16 {
        let physical = self.spec.physical_record_length() as usize;
        let used = self
            .data_pages
            .last()
            .map(|page| page.slots.len() * physical)
            .unwrap_or(0);
        (self.page_size.bytes() - 6).saturating_sub(used) as u16
    }

    fn write_fcr(&self, bytes: &mut [u8]) {
        let page_size = self.page_size.bytes();
        let page = &mut bytes[..page_size];
        put_u16(page, 0x00, 0);
        put_u16(page, 0x02, 0);
        // Setting an owner name is an engine operation: it advances the FCR
        // operation counter by two but does not touch page aux values.
        let owner_bias = if self.spec.owner.is_some() { 2 } else { 0 };
        put_u16(
            page,
            0x04,
            self.operation_count
                .saturating_mul(2)
                .saturating_add(owner_bias) as u16,
        );
        put_u16(page, 0x06, self.spec.format_word());
        put_u16(page, 0x08, self.spec.page_size);
        put_u16(page, 0x0A, 0);
        if let Some(head) = self.free_pages.first() {
            put_u16(page, 0x0C, (head >> 16) as u16);
            put_u16(page, 0x0E, *head as u16);
        } else {
            put_u16(page, 0x0C, 0xFFFF);
            put_u16(page, 0x0E, 0xFFFF);
        }
        put_u16(page, 0x10, 0xFFFF);
        put_u16(page, 0x12, 0xFFFF);
        put_u16(
            page,
            0x14,
            if self.spec.is_key_only() {
                0xFFFF
            } else {
                self.spec.keys.len() as u16
            },
        );
        let logical_length = if self.spec.is_compressed() {
            0
        } else {
            self.spec.record_length
        };
        put_u16(page, 0x16, logical_length);
        put_u16(page, 0x18, self.spec.physical_record_length());
        put_u16(page, 0x1A, (self.record_count >> 16) as u16);
        put_u16(page, 0x1C, self.record_count as u16);
        put_u16(page, 0x1E, 0);
        put_u16(
            page,
            0x20,
            if self.spec.is_key_only() {
                self.record_count as u16
            } else {
                self.last_data_page() as u16
            },
        );
        put_u16(page, 0x22, 0);
        put_u16(
            page,
            0x24,
            if self.spec.extension.is_some() { 2 } else { 1 },
        );
        put_u16(page, 0x26, 0);
        put_u16(page, 0x28, self.next_page as u16);
        put_u16(
            page,
            0x2A,
            if self.spec.is_key_only() {
                (self.page_size.bytes() - PAGE_HEADER_SIZE) as u16
            } else {
                self.free_bytes_last_data_page()
            },
        );
        let variable_meta = self.variable_meta();
        put_u16(page, 0x38, variable_meta[0]);
        put_u16(page, 0x3A, variable_meta[1]);
        put_u16(page, 0x48, self.spec.preallocate);
        put_u16(page, 0x106, self.spec.file_flags);
        put_u16(page, 0x108, self.spec.variable_aux());
        if self.spec.has_acs() {
            if let Some(acs) = &self.spec.acs {
                page[0x3C..0x44].copy_from_slice(&acs.raw[1..9]);
            }
            put_u16(page, 0x10C, 1);
        }
        if self.spec.is_compressed() {
            put_u16(page, 0x10E, self.spec.record_length);
        }
        if let Some(owner) = &self.spec.owner {
            // 0x2E..0x36 is the nine-byte hash; 0x37 carries the mode.  The
            // owner's presence is expressed by the 0xFD00 format word.
            page[0x2E..0x37].copy_from_slice(&owner.hash);
            page[0x37] = owner.mode;
        }
        if let Some(extension) = &self.spec.extension {
            let bytes = extension.as_bytes();
            let length = bytes.len().min(64);
            page[0x98..0x98 + length].copy_from_slice(&bytes[..length]);
            if length < 64 {
                page[0x98 + length] = 0;
            }
        }

        let mut offset = KEY_DEFS_OFFSET;
        for (index, key) in self.spec.keys.iter().enumerate() {
            let capacity = self.trees[index].capacity as u16;
            let first = &key.segments[0];
            put_u16(
                page,
                offset,
                if key.flags & 0x0080 != 0 { 0xFF00 } else { 0 },
            );
            put_u16(page, offset + 0x02, self.trees[index].index_page() as u16);
            put_u16(page, offset + 0x04, 0);
            let entry_count =
                if key.has_duplicates() && (key.is_supplemental() || self.spec.is_key_only()) {
                    unique_entry_count(&self.trees[index])
                } else {
                    self.trees[index].entry_count()
                };
            put_u16(page, offset + 0x06, entry_count as u16);
            put_u16(
                page,
                offset + 0x08,
                if self.spec.is_key_only() {
                    first.flags | 0x4000
                } else {
                    first.flags
                },
            );
            put_u16(page, offset + 0x0A, key.length);
            put_u16(page, offset + 0x0C, key.entry_size() as u16);
            put_u16(page, offset + 0x0E, capacity);
            put_u16(page, offset + 0x10, capacity / 2);
            put_u16(
                page,
                offset + 0x12,
                if key.uses_duplicate_tail() && !self.spec.is_key_only() {
                    self.spec
                        .physical_record_length()
                        .saturating_sub(8 * self.spec.duplicate_key_count() as u16)
                } else {
                    0
                },
            );
            put_u16(page, offset + 0x14, first.position - 1);
            put_u16(page, offset + 0x16, first.length);
            put_u16(
                page,
                offset + 0x18,
                if key.flags & 0x0080 != 0 { 0x00FF } else { 0 },
            );
            put_u16(page, offset + 0x1A, 0);
            page[offset + 0x1C] = first.key_type;
            page[offset + 0x1D] = first.null_value;
            offset += KEY_DEF_SIZE;

            for segment in &key.segments[1..] {
                put_u16(
                    page,
                    offset + 0x08,
                    if self.spec.is_key_only() {
                        segment.flags | 0x4000
                    } else {
                        segment.flags
                    },
                );
                put_u16(page, offset + 0x14, segment.position - 1);
                put_u16(page, offset + 0x16, segment.length);
                page[offset + 0x1C] = segment.key_type;
                page[offset + 0x1D] = segment.null_value;
                offset += KEY_DEF_SIZE;
            }
        }
    }

    fn write_acs_page(&self, bytes: &mut [u8]) {
        if !self.spec.has_acs() {
            return;
        }
        let Some(acs) = &self.spec.acs else {
            return;
        };
        let page_size = self.page_size.bytes();
        let page = &mut bytes[page_size..2 * page_size];
        put_u16(page, 0x00, 0);
        put_u16(page, 0x02, 1);
        put_u16(page, 0x04, 0);
        page[PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + Acs::SIZE].copy_from_slice(&acs.raw[..Acs::SIZE]);
    }

    fn write_data_pages(&self, bytes: &mut [u8]) {
        let page_size = self.page_size.bytes();
        let logical = self.spec.record_length as usize;
        for data_page in &self.data_pages {
            let page = &mut bytes
                [data_page.page as usize * page_size..(data_page.page as usize + 1) * page_size];
            put_u16(page, 0x00, 0);
            put_u16(page, 0x02, data_page.page as u16);
            put_u16(page, 0x04, DATA_PAGE_FLAG | data_page.aux);
            let mut offset = PAGE_HEADER_SIZE;
            for slot in &data_page.slots {
                if self.spec.is_compressed() {
                    put_variable_pointer(page, offset, slot.variable_pointer);
                    offset += 4;
                    page[offset] = slot.compression_marker;
                    offset += 1;
                } else {
                    page[offset..offset + logical].copy_from_slice(&slot.data);
                    offset += logical;
                    if self.spec.has_variable_length() {
                        put_variable_pointer(page, offset, slot.variable_pointer);
                        offset += 4;
                        if self.spec.has_blank_truncation() {
                            put_u16(page, offset, slot.truncated_blanks);
                            offset += 2;
                        }
                    }
                }
                for tail in &slot.tails {
                    put_address(page, offset, tail[0]);
                    offset += 4;
                    put_address(page, offset, tail[1]);
                    offset += 4;
                }
            }
        }
    }

    fn write_variable_pages(&self, bytes: &mut [u8]) {
        let page_size = self.page_size.bytes();
        for variable_page in &self.variable_pages {
            let page = &mut bytes[variable_page.page as usize * page_size
                ..(variable_page.page as usize + 1) * page_size];
            put_u16(page, 0x00, 0);
            put_u16(page, 0x02, variable_page.page as u16);
            put_u16(page, 0x04, variable_page.aux);
            let full = variable_page.is_full(page_size);
            put_u16(page, 0x06, if full { 0xFFFF } else { 0x00FF });
            put_u16(page, 0x08, 0xFFFF);
            put_u16(page, 0x0A, variable_page.slots.len() as u16);

            let mut offset = PAGE_HEADER_SIZE + 6;
            let mut boundaries = Vec::with_capacity(variable_page.slots.len() + 1);
            boundaries.push(
                offset
                    | if variable_page.slots[0].continuation {
                        0x8000
                    } else {
                        0
                    },
            );
            for slot in &variable_page.slots {
                if slot.continuation {
                    put_variable_pointer(page, offset, slot.pointer);
                    offset += 4;
                }
                page[offset..offset + slot.data.len()].copy_from_slice(&slot.data);
                offset += slot.data.len();
                boundaries.push(offset);
            }
            for (index, boundary) in boundaries.iter().enumerate() {
                put_u16(page, page_size - 2 * (index + 1), *boundary as u16);
            }
        }
    }

    fn write_free_pages(&self, bytes: &mut [u8]) {
        let page_size = self.page_size.bytes();
        for (index, page_number) in self.free_pages.iter().enumerate() {
            let start = *page_number as usize * page_size;
            let page = &mut bytes[start..start + page_size];
            page.fill(0);
            put_u16(page, 0x00, 0);
            put_u16(page, 0x02, *page_number as u16);
            put_u16(page, 0x04, 0);
            put_u16(page, 0x06, 0);
            let next = self
                .free_pages
                .get(index + 1)
                .copied()
                .unwrap_or(0xFFFF_FFFF);
            if next == 0xFFFF_FFFF {
                put_u32_le(page, 0x08, 0xFFFF_FFFF);
            } else {
                put_u32_le(page, 0x08, next << 16);
            }
            put_u32_le(page, 0x0C, 0);
        }
    }

    fn write_index_pages(&self, bytes: &mut [u8]) {
        let page_size = self.page_size.bytes();
        for tree in &self.trees {
            for node in &tree.nodes {
                let page = &mut bytes
                    [node.page as usize * page_size..(node.page as usize + 1) * page_size];
                put_u16(page, 0x00, node.previous);
                put_u16(page, 0x02, node.page as u16);
                put_u16(page, 0x04, node.aux);
                put_u16(page, 0x06, node.entries.len() as u16);
                if node.level == 0 {
                    put_u32_le(page, 0x08, 0xFFFF_FFFF);
                    put_u32_le(
                        page,
                        0x0C,
                        if node.entries.is_empty() {
                            0
                        } else {
                            0xFFFF_FFFF
                        },
                    );
                } else {
                    put_u32_le(page, 0x08, node.right << 16);
                    put_u32_le(page, 0x0C, node.left << 16);
                }
                for (index, entry) in node.entries.iter().enumerate() {
                    let mut offset = 0x10 + index * tree.key.entry_size();
                    if self.spec.is_key_only() {
                        page[offset..offset + entry.record.len()].copy_from_slice(&entry.record);
                        offset += entry.record.len();
                        put_address(page, offset, entry.serial);
                        offset += 4;
                    } else {
                        page[offset..offset + tree.key.length as usize].copy_from_slice(&entry.key);
                        offset += tree.key.length as usize;
                        put_address(page, offset, entry.address);
                        offset += 4;
                        if tree.key.uses_duplicate_tail() {
                            put_address(page, offset, entry.duplicate.unwrap_or(entry.address));
                            offset += 4;
                        }
                    }
                    let link = if node.level == 0 {
                        if index + 1 == node.entries.len() {
                            0
                        } else {
                            0xFFFF_FFFF
                        }
                    } else if index + 1 < node.entries.len() {
                        entry.link << 16
                    } else {
                        0
                    };
                    put_u32_le(page, offset, link);
                }
            }
        }
    }
}

fn unique_entry_count(tree: &TreeState) -> usize {
    let mut keys = tree
        .nodes
        .iter()
        .flat_map(|node| node.entries.iter().map(|entry| entry.key.clone()))
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| tree.key.compare(left, right, tree.acs.as_ref()));
    keys.dedup_by(|left, right| {
        tree.key.compare(left, right, tree.acs.as_ref()) == Ordering::Equal
    });
    keys.len()
}

fn insert_into_tree(
    tree: &mut TreeState,
    next_page: &mut u32,
    entry: EntryState,
    aux: u16,
) -> Result<()> {
    let key = entry.key.clone();
    let (leaf_index, mut path) = tree.target_leaf(&key)?;
    let key_spec = tree.key.clone();
    let acs = tree.acs.clone();
    let position = tree.nodes[leaf_index].entries.partition_point(|existing| {
        key_spec.compare(existing.key.as_slice(), key.as_slice(), acs.as_ref()) != Ordering::Greater
    });
    tree.nodes[leaf_index].entries.insert(position, entry);

    if tree.nodes[leaf_index].entries.len() <= tree.capacity {
        tree.nodes[leaf_index].aux = aux;
        return Ok(());
    }

    // A split-created leaf keeps the current operation's aux when the new
    // entry lands in the newly allocated half; otherwise the engine leaves the
    // new page's aux at zero.  Split-created interior pages use zero.
    let (mut promoted, mut new_page, mut original_page) =
        split_node(tree, leaf_index, next_page, aux, 0)?;
    if let Some(index) = tree.nodes.iter().position(|node| node.page == new_page) {
        let inserted_in_new = tree.nodes[index].entries.iter().any(|existing| {
            key_spec.compare(existing.key.as_slice(), key.as_slice(), acs.as_ref())
                == Ordering::Equal
        });
        if inserted_in_new {
            tree.nodes[index].aux = aux;
        }
    }
    loop {
        let Some(parent_index) = path.pop() else {
            let level = tree.nodes[leaf_index_of_split(tree, original_page)].level + 1;
            let root_page = *next_page;
            *next_page += 1;
            promoted.link = 0;
            let root_index = tree.nodes.len();
            tree.nodes.push(NodeState {
                page: root_page,
                previous: 0,
                level,
                entries: vec![promoted],
                left: new_page,
                right: original_page,
                aux: 0,
            });
            tree.root = root_index;
            return Ok(());
        };
        let parent = &mut tree.nodes[parent_index];
        if parent.left == original_page {
            parent.left = new_page;
            promoted.link = original_page;
            parent.entries.insert(0, promoted.clone());
        } else if parent.right == original_page {
            if let Some(last) = parent.entries.last_mut() {
                last.link = new_page;
            }
            promoted.link = 0;
            parent.entries.push(promoted.clone());
        } else if let Some(index) = parent
            .entries
            .iter()
            .position(|entry| entry.link == original_page)
        {
            parent.entries[index].link = new_page;
            promoted.link = original_page;
            parent.entries.insert(index + 1, promoted.clone());
        } else {
            return Err(FormatError::InvalidSpec(
                "split node is not linked from its parent",
            ));
        }
        parent.aux = aux;
        if parent.entries.len() <= tree.capacity {
            return Ok(());
        }
        let (next_promoted, next_new_page, next_original_page) =
            split_node(tree, parent_index, next_page, aux, 0)?;
        promoted = next_promoted;
        new_page = next_new_page;
        original_page = next_original_page;
    }
}

fn leaf_index_of_split(tree: &TreeState, page: u32) -> usize {
    tree.nodes
        .iter()
        .position(|node| node.page == page)
        .unwrap_or(tree.root)
}

fn split_node(
    tree: &mut TreeState,
    node_index: usize,
    next_page: &mut u32,
    aux: u16,
    new_aux: u16,
) -> Result<(EntryState, u32, u32)> {
    let new_page = *next_page;
    *next_page += 1;
    let node = &mut tree.nodes[node_index];
    let original_page = node.page;
    let level = node.level;
    let mut entries = std::mem::take(&mut node.entries);
    let split_at = entries.len() / 2;
    let right_entries = entries.split_off(split_at + 1);
    let mut promoted = entries
        .pop()
        .ok_or(FormatError::InvalidSpec("empty index page"))?;
    let left_entries = entries;

    let left = if level > 0 {
        let old_left = node.left;
        let old_right = node.right;
        let left_right = left_entries
            .last()
            .map(|entry| entry.link)
            .unwrap_or(old_left);
        let right_left = promoted.link;
        node.entries = right_entries;
        node.left = right_left;
        node.right = old_right;
        node.aux = aux;
        NodeState {
            page: new_page,
            previous: 0,
            level,
            entries: left_entries,
            left: old_left,
            right: left_right,
            aux: new_aux,
        }
    } else {
        node.entries = right_entries;
        node.aux = aux;
        NodeState {
            page: new_page,
            previous: 0,
            level: 0,
            entries: left_entries,
            left: 0,
            right: 0,
            aux: new_aux,
        }
    };
    tree.nodes.push(left);
    promoted.link = 0;
    Ok((promoted, new_page, original_page))
}

fn compress_record(record: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut position = 0usize;
    while position < record.len() {
        let mut run_start = None;
        let mut index = position;
        while index + 5 <= record.len() {
            let byte = record[index];
            if record[index + 1] == byte
                && record[index + 2] == byte
                && record[index + 3] == byte
                && record[index + 4] == byte
            {
                run_start = Some(index);
                break;
            }
            index += 1;
        }
        let literal_end = run_start.unwrap_or(record.len());
        let literal = &record[position..literal_end];
        out.extend_from_slice(&(literal.len() as u16).to_le_bytes());
        out.extend_from_slice(literal);
        position = literal_end;
        if position >= record.len() {
            break;
        }
        let byte = record[position];
        let mut end = position + 1;
        while end < record.len() && record[end] == byte {
            end += 1;
        }
        out.extend_from_slice(&((end - position) as u16).to_le_bytes());
        out.push(byte);
        position = end;
    }
    out
}

fn write_signed_le(buffer: &mut [u8], value: i128) -> Result<()> {
    match buffer.len() {
        2 => {
            let value = i16::try_from(value)
                .map_err(|_| FormatError::InvalidSpec("autoincrement value out of range"))?;
            buffer.copy_from_slice(&value.to_le_bytes());
        }
        4 => {
            let value = i32::try_from(value)
                .map_err(|_| FormatError::InvalidSpec("autoincrement value out of range"))?;
            buffer.copy_from_slice(&value.to_le_bytes());
        }
        _ => return Err(FormatError::Unsupported("autoincrement key length")),
    }
    Ok(())
}

fn signed_le_abs(bytes: &[u8]) -> i128 {
    let mut value: i128 = 0;
    for (index, byte) in bytes.iter().enumerate() {
        value |= (*byte as i128) << (8 * index);
    }
    let bits = bytes.len() * 8;
    if bits > 0 && bits < 128 && value & (1i128 << (bits - 1)) != 0 {
        value -= 1i128 << bits;
    }
    value.abs()
}

/// Variable-portion pointers store the page and slot as big-endian words,
/// unlike ordinary record addresses.
fn put_variable_pointer(buffer: &mut [u8], offset: usize, pointer: u32) {
    let page = (pointer >> 16) as u16;
    let slot = pointer as u16;
    buffer[offset..offset + 2].copy_from_slice(&page.to_be_bytes());
    buffer[offset + 2..offset + 4].copy_from_slice(&slot.to_be_bytes());
}

fn put_u16(buffer: &mut [u8], offset: usize, value: u16) {
    buffer[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32_le(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Record addresses in index entries and duplicate tails store the high
/// 16-bit word first, unlike ordinary little-endian words.
fn put_address(buffer: &mut [u8], offset: usize, address: u32) {
    put_u16(buffer, offset, (address >> 16) as u16);
    put_u16(buffer, offset + 2, address as u16);
}

/// Insert records into an existing file by rebuilding it from its current
/// specification and records.  Returns the new record count.
pub fn insert_records(path: impl AsRef<Path>, records: &[Vec<u8>]) -> Result<usize> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    let mut writer = Writer::from_file(&file)?;
    for record in records {
        writer.insert(record)?;
    }
    let count = writer.record_count() as usize;
    let bytes = writer.finish()?;
    std::fs::write(path, bytes)?;
    Ok(count)
}

/// Replace the first record whose key 0 matches `key`.  Returns false if no
/// record matched.  The file is rebuilt, so changing a key value is supported.
pub fn update_record(path: impl AsRef<Path>, key: &[u8], new_record: &[u8]) -> Result<bool> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    let spec = FileSpec::from_file(&file)?;
    check_search_key(&spec, key.len())?;
    let key_zero = &spec.keys[0];
    if is_fixed_in_place_candidate(&file, &spec)
        && new_record.len() == file.fcr.logical_record_length as usize
    {
        let pages = fixed_key_pages(&file, &spec)
            .ok_or(FormatError::InvalidSpec("unexpected index layout"))?;
        let page = pages[0];
        if let Some(index) = page.entries.iter().position(|entry| {
            key_zero.compare(&entry.key, key, spec.acs.as_ref()) == Ordering::Equal
        }) {
            let address = page.entries[index].record_address;
            let mut bytes = read_working(path, &file)?;
            let outcome = update_fixed_in_place(&mut bytes, &file, address, new_record, &spec);
            if matches!(outcome, Err(FormatError::DuplicateKey)) {
                write_working(path, &file, bytes.clone())?;
            }
            outcome?;
            write_working(path, &file, bytes)?;
            return Ok(true);
        }
        return Ok(false);
    }
    let mut records = file
        .records()?
        .into_iter()
        .map(|record| record.data)
        .collect::<Vec<_>>();
    let mut found = false;
    for record in &mut records {
        let current = key_zero.build_key(record)?;
        if key_zero.compare(&current, key, spec.acs.as_ref()) == Ordering::Equal {
            *record = new_record.to_vec();
            found = true;
            break;
        }
    }
    if !found {
        return Ok(false);
    }
    let writer = Writer::from_records(spec, &records)?;
    let bytes = writer.finish()?;
    std::fs::write(path, bytes)?;
    Ok(true)
}

/// Insert one record into a simple fixed single-key file in place, reusing a
/// freed slot or appending to the last data page.  Returns the new record
/// address when the file qualifies, or `None` when the caller must fall back
/// to a rebuild.
pub fn insert_fixed_at_in_place(path: impl AsRef<Path>, record: &[u8]) -> Result<Option<u32>> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    let spec = FileSpec::from_file(&file)?;
    if !is_fixed_layout_candidate(&file, &spec)
        || record.len() != file.fcr.logical_record_length as usize
    {
        return Ok(None);
    }
    let mut values = Vec::with_capacity(spec.keys.len());
    let mut leaves = Vec::with_capacity(spec.keys.len());
    for (index, key) in spec.keys.iter().enumerate() {
        if key.is_excluded_from_index(record) {
            return Ok(None);
        }
        let value = key.build_key(record)?;
        let Some(pages) = key_tree_pages(&file, index) else {
            return Ok(None);
        };
        if tree_contains(&pages, key, &value, spec.acs.as_ref()) {
            return Err(FormatError::DuplicateKey);
        }
        let root = file.fcr.key_defs[index].index_page as u32;
        let Some(leaf) = target_leaf(&pages, root, key, &value, spec.acs.as_ref()) else {
            return Ok(None);
        };
        if leaf.count as usize >= (file.page_size.bytes() - 12) / key.entry_size() {
            return Ok(None);
        }
        values.push(value);
        leaves.push(leaf);
    }
    let pages = &leaves;

    let page_size = file.page_size.bytes();
    let physical = file.fcr.physical_record_length as usize;
    let mut bytes = read_working(path, &file)?;
    let old_free = ((file.fcr.free_links[2] as u32) << 16) | file.fcr.free_links[3] as u32;
    let address;
    let mut allocated_page = false;
    if old_free != 0xFFFF_FFFF {
        address = old_free;
        let slot = address as usize;
        if slot + physical > bytes.len() {
            return Ok(None);
        }
        let next_free = ((u16::from_le_bytes([bytes[slot], bytes[slot + 1]]) as u32) << 16)
            | u16::from_le_bytes([bytes[slot + 2], bytes[slot + 3]]) as u32;
        put_u16(&mut bytes, 0x10, (next_free >> 16) as u16);
        put_u16(&mut bytes, 0x12, next_free as u16);
    } else {
        if physical == 0 || file.fcr.last_data_page == 0 {
            return Ok(None);
        }
        let last_page = file.fcr.last_data_page as usize;
        let free_bytes = file.fcr.free_bytes_last_data_page as usize;
        let used = (page_size - PAGE_HEADER_SIZE).saturating_sub(free_bytes) / physical;
        let offset = PAGE_HEADER_SIZE + used * physical;
        if free_bytes >= physical && offset + physical <= page_size {
            address = (last_page * page_size + offset) as u32;
            put_u16(&mut bytes, 0x2A, free_bytes.saturating_sub(physical) as u16);
        } else {
            let new_page = file.fcr.total_pages as usize;
            allocated_page = true;
            let start = new_page * page_size;
            if start + page_size > bytes.len() {
                bytes.resize(start + page_size, 0);
            }
            bytes[start..start + page_size].fill(0);
            put_u16(&mut bytes, start, 0);
            put_u16(&mut bytes, start + 2, new_page as u16);
            address = (start + PAGE_HEADER_SIZE) as u32;
            put_u16(&mut bytes, 0x20, new_page as u16);
            put_u16(&mut bytes, 0x28, (new_page + 1) as u16);
            let free = (page_size - PAGE_HEADER_SIZE - physical) as u16;
            put_u16(&mut bytes, 0x2A, free);
        }
    }

    let slot = address as usize;
    let logical = file.fcr.logical_record_length as usize;
    bytes[slot..slot + logical].copy_from_slice(record);
    if physical > logical {
        bytes[slot + logical..slot + physical].fill(0);
    }

    let even = u16::from_le_bytes([bytes[4], bytes[5]]).wrapping_add(2);
    put_u16(&mut bytes, 0x04, even);
    let aux = even.wrapping_sub(1);
    let data_page = slot / page_size;
    let page_aux = if allocated_page { 0 } else { aux };
    put_u16(
        &mut bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | page_aux,
    );

    // Insert one entry into each key's leaf page, in key order.
    for (index, key) in spec.keys.iter().enumerate() {
        let page = pages[index];
        let mut entries: Vec<(Vec<u8>, u32)> = page
            .entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.record_address))
            .collect();
        entries.push((values[index].clone(), address));
        entries.sort_by(|left, right| key.compare(&left.0, &right.0, spec.acs.as_ref()));
        if entries.windows(2).any(|window| {
            key.compare(&window[0].0, &window[1].0, spec.acs.as_ref()) == Ordering::Equal
        }) {
            return Err(FormatError::DuplicateKey);
        }
        store_key_entries(
            &mut bytes,
            page,
            key,
            &entries,
            aux,
            key_def_offset(&file, index),
            file.fcr.key_defs[index].entry_count.saturating_add(1),
            false,
        );
    }
    let records = file.fcr.record_count.saturating_add(1);
    put_u16(&mut bytes, 0x1A, (records >> 16) as u16);
    put_u16(&mut bytes, 0x1C, records as u16);

    write_working(path, &file, bytes)?;
    Ok(Some(address))
}

/// Update the fixed-length record stored at `address` in place when the file
/// is a simple fixed single-key file.  Returns `false` when the file does not
/// qualify or has no record at that address, in which case the caller should
/// fall back to a rebuild.  This is the engine's byte-identical update path.
pub fn update_fixed_at_in_place(
    path: impl AsRef<Path>,
    address: u32,
    new_record: &[u8],
) -> Result<bool> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    let spec = FileSpec::from_file(&file)?;
    let logical = file.fcr.logical_record_length as usize;
    if !file.fcr.file_flags.key_only()
        && file.fcr.file_flags.variable_length()
        && !file.fcr.file_flags.compression()
        && new_record.len() >= logical
    {
        if let Some(old) = file
            .records()?
            .into_iter()
            .find(|record| record.address == address)
        {
            let blank_truncation = file.fcr.file_flags.blank_truncation();
            // Blank truncation removes trailing blanks from the variable
            // portion, so a record whose supplied tail is entirely blanks is
            // stored as a fixed record with a truncated-blank count.
            let mut effective = new_record.len();
            if blank_truncation {
                while effective > logical && new_record[effective - 1] == b' ' {
                    effective -= 1;
                }
            }
            if effective == logical {
                let keys_changed = spec.keys.iter().any(|key| {
                    match (key.build_key(&old.data), key.build_key(new_record)) {
                        (Ok(old), Ok(new)) => {
                            key.compare(&old, &new, spec.acs.as_ref()) != Ordering::Equal
                        }
                        _ => true,
                    }
                });
                if !keys_changed {
                    let page_size = file.page_size.bytes();
                    let aux = file.fcr.aux0.wrapping_add(2);
                    let mut bytes = read_working(path, &file)?;
                    free_record_variable(
                        &mut bytes,
                        &file,
                        address,
                        page_size,
                        aux.wrapping_sub(1),
                    )?;
                    let slot = address as usize;
                    bytes[slot..slot + logical].copy_from_slice(&new_record[..logical]);
                    let pointer = slot + logical;
                    if pointer + 4 <= bytes.len() {
                        bytes[pointer..pointer + 4].fill(0xFF);
                    }
                    // A blank-truncated record stores the number of stripped
                    // trailing blanks in the two bytes after the pointer.
                    if blank_truncation && pointer + 6 <= bytes.len() {
                        put_u16(&mut bytes, pointer + 4, (new_record.len() - logical) as u16);
                    }
                    put_u16(&mut bytes, 0x04, aux);
                    let data_page = slot / page_size;
                    put_u16(
                        &mut bytes,
                        data_page * page_size + 0x04,
                        DATA_PAGE_FLAG | aux.wrapping_sub(1),
                    );
                    write_working(path, &file, bytes)?;
                    return Ok(true);
                }
            }
            if !file.fcr.file_flags.blank_truncation() && new_record.len() > old.data.len() {
                let keys_changed = spec.keys.iter().any(|key| {
                    match (key.build_key(&old.data), key.build_key(new_record)) {
                        (Ok(old), Ok(new)) => {
                            key.compare(&old, &new, spec.acs.as_ref()) != Ordering::Equal
                        }
                        _ => true,
                    }
                });
                if !keys_changed {
                    let aux = file.fcr.aux0.wrapping_add(2);
                    let mut bytes = read_working(path, &file)?;
                    if extend_variable_record_in_place(
                        &mut bytes,
                        &file,
                        address,
                        new_record,
                        old.data.len().saturating_sub(logical),
                        aux,
                    )? {
                        write_working(path, &file, bytes)?;
                        return Ok(true);
                    }
                }
            }
            if !file.fcr.file_flags.blank_truncation()
                && new_record.len() > logical
                && new_record.len() < old.data.len()
            {
                let keys_changed = spec.keys.iter().any(|key| {
                    match (key.build_key(&old.data), key.build_key(new_record)) {
                        (Ok(old), Ok(new)) => {
                            key.compare(&old, &new, spec.acs.as_ref()) != Ordering::Equal
                        }
                        _ => true,
                    }
                });
                if !keys_changed {
                    let aux = file.fcr.aux0.wrapping_add(2);
                    let mut bytes = read_working(path, &file)?;
                    if shrink_variable_record_in_place(
                        &mut bytes,
                        &file,
                        address,
                        new_record,
                        old.data.len().saturating_sub(logical),
                        aux,
                    )? {
                        write_working(path, &file, bytes)?;
                        return Ok(true);
                    }
                }
            }
            if !file.fcr.file_flags.blank_truncation() && new_record.len() == old.data.len() {
                let keys_changed = spec.keys.iter().any(|key| {
                    match (key.build_key(&old.data), key.build_key(new_record)) {
                        (Ok(old), Ok(new)) => {
                            key.compare(&old, &new, spec.acs.as_ref()) != Ordering::Equal
                        }
                        _ => true,
                    }
                });
                if !keys_changed {
                    let aux = file.fcr.aux0.wrapping_add(2);
                    let mut bytes = read_working(path, &file)?;
                    if update_variable_record_in_place(&mut bytes, &file, address, new_record, aux)?
                    {
                        write_working(path, &file, bytes)?;
                        return Ok(true);
                    }
                }
            }
        }
    }
    if !is_fixed_update_candidate(&file, &spec)
        || new_record.len() != file.fcr.logical_record_length as usize
    {
        return Ok(false);
    }
    let exists = if spec.keys.is_empty() {
        file.records()?
            .iter()
            .any(|record| record.address == address)
    } else {
        file.index_pages.iter().any(|page| {
            page.entries
                .iter()
                .any(|entry| entry.record_address == address)
        })
    };
    if !exists {
        return Ok(false);
    }
    let slot = address as usize;
    if slot + logical > file.bytes.len() {
        return Ok(false);
    }
    let old_record = file.bytes[slot..slot + logical].to_vec();
    let keys_changed =
        spec.keys.iter().any(
            |key| match (key.build_key(&old_record), key.build_key(new_record)) {
                (Ok(old), Ok(new)) => key.compare(&old, &new, spec.acs.as_ref()) != Ordering::Equal,
                _ => true,
            },
        );
    let mut bytes = read_working(path, &file)?;
    if !keys_changed {
        update_fixed_data_only(&mut bytes, &file, address, new_record)?;
        write_working(path, &file, bytes)?;
        return Ok(true);
    }
    if fixed_key_pages(&file, &spec).is_none() {
        return Ok(false);
    }
    let outcome = update_fixed_in_place(&mut bytes, &file, address, new_record, &spec);
    if matches!(outcome, Err(FormatError::DuplicateKey)) {
        write_working(path, &file, bytes.clone())?;
    }
    outcome?;
    write_working(path, &file, bytes)?;
    Ok(true)
}

/// Delete the fixed-length record stored at `address` in place when the file
/// is a simple fixed single-key file.  Returns `false` when the file does not
/// qualify or has no record at that address.
pub fn delete_fixed_at_in_place(path: impl AsRef<Path>, address: u32) -> Result<bool> {
    Ok(delete_fixed_at_in_place_with_journal(path, address)?.0)
}

/// Delete in place and return the ordered index-page images the engine
/// journals for this operation.
pub fn delete_fixed_at_in_place_with_journal(
    path: impl AsRef<Path>,
    address: u32,
) -> Result<(bool, Vec<Vec<u8>>)> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    let spec = FileSpec::from_file(&file)?;
    if spec.keys.iter().any(|key| key.has_duplicates()) {
        let mut bytes = read_working(path, &file)?;
        if delete_duplicate_in_place(&mut bytes, &file, address, &spec)? {
            write_working(path, &file, bytes)?;
            return Ok((true, Vec::new()));
        }
    }
    if !spec.keys.is_empty()
        && !file.index_pages.iter().any(|page| {
            page.entries
                .iter()
                .any(|entry| entry.record_address == address)
        })
    {
        if delete_unindexed_in_place(path, &file, address, &spec)? {
            return Ok((true, Vec::new()));
        }
        return Ok((false, Vec::new()));
    }
    if !is_in_place_delete_candidate(&file, &spec) {
        return Ok((false, Vec::new()));
    }
    let mut bytes = read_working(path, &file)?;
    if let Some(journal) = delete_tree_in_place(&mut bytes, &file, address, &spec)? {
        write_working(path, &file, bytes)?;
        return Ok((true, journal));
    }
    let mut bytes = read_working(path, &file)?;
    if !delete_fixed_in_place(&mut bytes, &file, address, &spec)? {
        return Ok((false, Vec::new()));
    }
    write_working(path, &file, bytes)?;
    Ok((true, Vec::new()))
}

fn is_fixed_layout_candidate(file: &BtrieveFile, spec: &FileSpec) -> bool {
    !file.fcr.file_flags.key_only()
        && !file.fcr.file_flags.compression()
        && !file.fcr.file_flags.variable_length()
        && file.fcr.physical_record_length == file.fcr.logical_record_length
        && spec
            .keys
            .iter()
            .all(|key| !key.has_duplicates() && !key.has_null() && !key.is_manual())
}

/// Fixed-slot files: strictly fixed records plus variable-length files whose
/// records use only the four-byte null variable pointer (no chunk pages).
fn owner_encrypted(file: &BtrieveFile) -> bool {
    file.fcr.owner_present && file.fcr.owner_mode & 0x02 != 0
}

/// In-place mutations work on plaintext pages.  Encrypted files are read
/// from the reader's decrypted image and re-encrypted on write.
fn read_working(path: &Path, file: &BtrieveFile) -> Result<Vec<u8>> {
    if owner_encrypted(file) {
        Ok(file.bytes.clone())
    } else {
        Ok(std::fs::read(path)?)
    }
}

fn write_working(path: &Path, file: &BtrieveFile, mut bytes: Vec<u8>) -> Result<()> {
    if owner_encrypted(file) {
        let page_size = file.page_size.bytes();
        for page in 1..bytes.len() / page_size {
            let start = page * page_size;
            for offset in 0..page_size {
                bytes[start + offset] ^=
                    crate::fcr::owner_mask_byte(&file.fcr.owner_hash, offset, page_size);
            }
        }
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn delete_unindexed_in_place(
    path: &Path,
    file: &BtrieveFile,
    address: u32,
    spec: &FileSpec,
) -> Result<bool> {
    let page_size = file.page_size.bytes();
    let aux = file.fcr.aux0.wrapping_add(2);
    let page_aux = aux.wrapping_sub(1);
    let mut bytes = read_working(path, file)?;
    free_record_variable(&mut bytes, file, address, page_size, page_aux)?;
    let data_page = address as usize / page_size;
    put_u16(
        &mut bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | page_aux,
    );
    let old_head = ((file.fcr.free_links[2] as u32) << 16) | file.fcr.free_links[3] as u32;
    let slot = address as usize;
    let physical = file.fcr.physical_record_length as usize;
    put_address(&mut bytes, slot, old_head);
    bytes[slot + 4..slot + physical].fill(0);
    put_u16(&mut bytes, 0x10, (address >> 16) as u16);
    put_u16(&mut bytes, 0x12, address as u16);
    put_u16(&mut bytes, 0x04, aux);
    let count = file.fcr.record_count.saturating_sub(1);
    put_u16(&mut bytes, 0x1A, (count >> 16) as u16);
    put_u16(&mut bytes, 0x1C, count as u16);
    let _ = spec;
    write_working(path, file, bytes)?;
    Ok(true)
}

fn is_in_place_delete_candidate(file: &BtrieveFile, _spec: &FileSpec) -> bool {
    !file.fcr.file_flags.key_only()
}

fn is_fixed_update_candidate(file: &BtrieveFile, spec: &FileSpec) -> bool {
    if file.fcr.file_flags.key_only()
        || file.fcr.file_flags.compression()
        || spec
            .keys
            .iter()
            .any(|key| key.has_duplicates() || key.has_null() || key.is_manual())
    {
        return false;
    }
    let logical = file.fcr.logical_record_length as usize;
    let physical = file.fcr.physical_record_length as usize;
    if !file.fcr.file_flags.variable_length() {
        return physical == logical;
    }
    if physical != logical + 4 {
        return false;
    }
    file.records()
        .map(|records| {
            records.iter().all(|record| {
                record.trailing.len() == 4 && record.trailing.iter().all(|byte| *byte == 0xFF)
            })
        })
        .unwrap_or(false)
}

fn is_fixed_in_place_candidate(file: &BtrieveFile, spec: &FileSpec) -> bool {
    is_fixed_update_candidate(file, spec) && fixed_key_pages(file, spec).is_some()
}

/// Every page belonging to one key's index tree, in key order.
fn key_tree_pages(file: &BtrieveFile, index: usize) -> Option<Vec<&crate::index::IndexPage>> {
    let pages: Vec<_> = file
        .index_pages
        .iter()
        .filter(|page| page.key_index == index)
        .collect();
    (!pages.is_empty()).then_some(pages)
}

fn tree_page<'a>(
    pages: &[&'a crate::index::IndexPage],
    number: u32,
) -> Option<&'a crate::index::IndexPage> {
    pages
        .iter()
        .copied()
        .find(|page| page.page_number == number)
}

/// Descend to the leaf that would hold `value`.  Interior pages store the
/// right child at +0x08 and the left child at +0x0C, each as `page << 16`.
fn target_leaf<'a>(
    pages: &[&'a crate::index::IndexPage],
    root: u32,
    key: &KeySpec,
    value: &[u8],
    acs: Option<&Acs>,
) -> Option<&'a crate::index::IndexPage> {
    let mut page = tree_page(pages, root)?;
    for _ in 0..64 {
        if page.link_a == 0xFFFF_FFFF {
            return Some(page);
        }
        let position = page
            .entries
            .partition_point(|entry| key.compare(&entry.key, value, acs) == Ordering::Less);
        let child = if position == 0 {
            page.link_b >> 16
        } else if position == page.entries.len() {
            page.link_a >> 16
        } else {
            page.entries[position - 1].next_link >> 16
        };
        page = tree_page(pages, child)?;
    }
    None
}

fn tree_contains(
    pages: &[&crate::index::IndexPage],
    key: &KeySpec,
    value: &[u8],
    acs: Option<&Acs>,
) -> bool {
    pages.iter().any(|page| {
        page.entries
            .iter()
            .any(|entry| key.compare(&entry.key, value, acs) == Ordering::Equal)
    })
}

/// One leaf index page per key, in key order.  Multi-page trees and missing
/// pages disqualify the in-place fast paths.
fn fixed_key_pages<'a>(
    file: &'a BtrieveFile,
    spec: &FileSpec,
) -> Option<Vec<&'a crate::index::IndexPage>> {
    if spec.keys.len() != file.fcr.key_defs.len() {
        return None;
    }
    let mut pages = Vec::with_capacity(spec.keys.len());
    for index in 0..spec.keys.len() {
        let mut matching = file
            .index_pages
            .iter()
            .filter(|page| page.key_index == index);
        let first = matching.next()?;
        if matching.next().is_some() {
            return None;
        }
        pages.push(first);
    }
    Some(pages)
}

fn key_def_offset(file: &BtrieveFile, index: usize) -> usize {
    let mut offset = KEY_DEFS_OFFSET;
    for key in file.fcr.key_defs.iter().take(index) {
        offset += key.segments.len() * KEY_DEF_SIZE;
    }
    offset
}

/// Write a key's unique leaf entries in order and update the leaf aux and the
/// FCR key-table entry count.  `clear_tail` zeroes entry slots removed by a
/// delete, as the DOS engine does.
#[allow(clippy::too_many_arguments)]
fn store_key_entries(
    bytes: &mut [u8],
    page: &crate::index::IndexPage,
    key: &KeySpec,
    entries: &[(Vec<u8>, u32)],
    aux: u16,
    key_def: usize,
    key_def_entries: u16,
    clear_tail: bool,
) {
    let page_size = u16::from_le_bytes([bytes[0x08], bytes[0x09]]) as usize;
    let page_start = page.page_number as usize * page_size;
    let entry_size = key.entry_size();
    let old_count = page.count as usize;
    for (position, (value, address)) in entries.iter().enumerate() {
        let offset = page_start + 0x10 + position * entry_size;
        bytes[offset..offset + key.length as usize].copy_from_slice(value);
        let tail = offset + key.length as usize;
        put_address(bytes, tail, *address);
        put_u32_le(
            bytes,
            tail + 4,
            if position + 1 == entries.len() {
                0
            } else {
                0xFFFF_FFFF
            },
        );
    }
    if clear_tail && entries.len() < old_count {
        for position in entries.len()..old_count {
            let offset = page_start + 0x10 + position * entry_size;
            bytes[offset..offset + entry_size].fill(0);
        }
    }
    put_u16(bytes, page_start + 0x04, aux);
    put_u16(bytes, page_start + 0x06, entries.len() as u16);
    put_u32_le(
        bytes,
        page_start + 0x0C,
        if entries.is_empty() { 0 } else { 0xFFFF_FFFF },
    );
    put_u16(bytes, key_def + 0x06, key_def_entries);
}

/// Delete all records whose key 0 matches `key`.  Returns false if nothing
/// matched.  The file is rebuilt.
pub fn delete_record(path: impl AsRef<Path>, key: &[u8]) -> Result<bool> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    let spec = FileSpec::from_file(&file)?;
    check_search_key(&spec, key.len())?;
    let key_zero = &spec.keys[0];
    if is_in_place_delete_candidate(&file, &spec) {
        if let Some(found) = file.records()?.into_iter().find(|record| {
            key_zero
                .build_key(&record.data)
                .map(|value| key_zero.compare(&value, key, spec.acs.as_ref()) == Ordering::Equal)
                .unwrap_or(false)
        }) {
            if delete_fixed_at_in_place(path, found.address)? {
                return Ok(true);
            }
        } else {
            return Ok(false);
        }
    }
    let records = file
        .records()?
        .into_iter()
        .map(|record| record.data)
        .collect::<Vec<_>>();
    let mut kept = Vec::with_capacity(records.len());
    let mut removed = false;
    for record in records {
        let current = key_zero.build_key(&record)?;
        if key_zero.compare(&current, key, spec.acs.as_ref()) == Ordering::Equal {
            removed = true;
        } else {
            kept.push(record);
        }
    }
    if !removed {
        return Ok(false);
    }
    let writer = Writer::from_records(spec, &kept)?;
    let bytes = writer.finish()?;
    std::fs::write(path, bytes)?;
    Ok(true)
}

/// Replace the record stored at `address` (the 32-bit page-relative record
/// address reported by the reader).  This is the update path for data-only
/// files, which have no key to search on; keyed files may use it too.  The
/// file is rebuilt, so changing bytes that participate in any key is
/// supported.
pub fn update_record_at(path: impl AsRef<Path>, address: u32, new_record: &[u8]) -> Result<bool> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    let spec = FileSpec::from_file(&file)?;
    let mut records = file
        .records()?
        .into_iter()
        .map(|record| (record.address, record.data))
        .collect::<Vec<_>>();
    let Some(index) = records.iter().position(|(at, _)| *at == address) else {
        return Ok(false);
    };
    records[index].1 = new_record.to_vec();
    let records = records
        .into_iter()
        .map(|(_, record)| record)
        .collect::<Vec<_>>();
    let writer = Writer::from_records(spec, &records)?;
    let bytes = writer.finish()?;
    std::fs::write(path, bytes)?;
    Ok(true)
}

/// Delete the record stored at `address` (see [`update_record_at`]).  Returns
/// false when no record has that address.  The file is rebuilt.
pub fn delete_record_at(path: impl AsRef<Path>, address: u32) -> Result<bool> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    let spec = FileSpec::from_file(&file)?;
    let records = file
        .records()?
        .into_iter()
        .map(|record| (record.address, record.data))
        .collect::<Vec<_>>();
    let Some(index) = records.iter().position(|(at, _)| *at == address) else {
        return Ok(false);
    };
    let kept = records
        .into_iter()
        .enumerate()
        .filter(|(position, _)| *position != index)
        .map(|(_, (_, record))| record)
        .collect::<Vec<_>>();
    let writer = Writer::from_records(spec, &kept)?;
    let bytes = writer.finish()?;
    std::fs::write(path, bytes)?;
    Ok(true)
}

/// Delete a fixed record in place: remove its entry from every key, put the
/// data slot on the FCR free list, and advance the operation counters the same
/// way the engine does.
type PageWrite = (usize, u32, Vec<(Vec<u8>, u32)>, bool);

fn read_pointer(bytes: &[u8], offset: usize) -> Option<u32> {
    // Variable/compressed pointers store page and slot as big-endian words.
    let high = u16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]);
    let low = u16::from_be_bytes([*bytes.get(offset + 2)?, *bytes.get(offset + 3)?]);
    Some(((high as u32) << 16) | low as u32)
}

fn boundary(bytes: &[u8], page_start: usize, page_size: usize, index: usize) -> Option<u16> {
    let offset = page_start.checked_add(page_size.checked_sub(2 * (index + 1))?)?;
    Some(u16::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
    ]))
}

fn set_boundary(
    bytes: &mut [u8],
    page_start: usize,
    page_size: usize,
    index: usize,
    value: u16,
) -> Option<()> {
    let offset = page_start.checked_add(page_size.checked_sub(2 * (index + 1))?)?;
    put_u16(bytes, offset, value);
    Some(())
}

/// Release one variable/compressed chunk slot and compact the page exactly as
/// the DOS engine does: free boundaries become FFFF, later boundaries and data
/// shift left by the chunk size, and a freed final slot shrinks the page.
fn free_variable_slot(
    bytes: &mut [u8],
    page: usize,
    slot: usize,
    page_size: usize,
    aux: u16,
) -> Result<()> {
    let page_start = page * page_size;
    if page_start + page_size > bytes.len() {
        return Err(FormatError::InvalidRecordAddress((page * page_size) as u32));
    }
    let count = u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
    let was_full =
        u16::from_le_bytes([bytes[page_start + 0x06], bytes[page_start + 0x07]]) == 0xFFFF;
    if slot >= count {
        return Ok(());
    }
    let start = boundary(bytes, page_start, page_size, slot)
        .ok_or(FormatError::InvalidRecordAddress(page as u32))?
        & 0x7FFF;
    let mut end_value = None;
    for index in slot + 1..=count {
        let value = boundary(bytes, page_start, page_size, index)
            .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
        if value != 0xFFFF {
            end_value = Some(value & 0x7FFF);
            break;
        }
    }
    let end = end_value.ok_or(FormatError::InvalidRecordAddress(page as u32))?;
    if end < start || end as usize > page_size {
        return Err(FormatError::InvalidRecordAddress(page as u32));
    }
    let chunk = (end - start) as usize;
    if slot + 1 == count {
        let old_end = boundary(bytes, page_start, page_size, count)
            .ok_or(FormatError::InvalidRecordAddress(page as u32))?
            & 0x7FFF;
        set_boundary(bytes, page_start, page_size, slot, start)
            .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
        set_boundary(bytes, page_start, page_size, slot + 1, 0)
            .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
        let mut new_count = count - 1;
        // A trailing free slot left behind by an earlier delete is discarded
        // when the final live slot is released: the freed slot's start becomes
        // the chunk end of the preceding live entry.
        while new_count > 0 {
            let previous = boundary(bytes, page_start, page_size, new_count - 1)
                .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
            if previous != 0xFFFF {
                break;
            }
            let end = boundary(bytes, page_start, page_size, new_count)
                .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
            set_boundary(bytes, page_start, page_size, new_count - 1, end)
                .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
            set_boundary(bytes, page_start, page_size, new_count, 0)
                .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
            new_count -= 1;
        }
        put_u16(bytes, page_start + 0x0A, new_count as u16);
        bytes[page_start + start as usize..page_start + end as usize].fill(0);
        if new_count > 0 {
            let remaining = new_count;
            let live = (0..remaining).any(|index| {
                boundary(bytes, page_start, page_size, index)
                    .map(|value| value != 0xFFFF)
                    .unwrap_or(false)
            });
            if !live {
                // Collapse a page whose remaining boundaries are all holes.
                put_u16(bytes, page_start + 0x0A, 0);
                set_boundary(bytes, page_start, page_size, 0, 0x0C)
                    .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
                for index in 1..=count {
                    set_boundary(bytes, page_start, page_size, index, 0)
                        .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
                }
                bytes[page_start + 0x0C..page_start + old_end as usize].fill(0);
            }
        }
    } else {
        let old_end = boundary(bytes, page_start, page_size, count)
            .ok_or(FormatError::InvalidRecordAddress(page as u32))?
            & 0x7FFF;
        set_boundary(bytes, page_start, page_size, slot, 0xFFFF)
            .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
        for index in slot + 1..=count {
            let Some(value) = boundary(bytes, page_start, page_size, index) else {
                return Err(FormatError::InvalidRecordAddress(page as u32));
            };
            if value != 0xFFFF {
                set_boundary(
                    bytes,
                    page_start,
                    page_size,
                    index,
                    value.wrapping_sub(chunk as u16),
                );
            }
        }
        if end < old_end {
            bytes.copy_within(
                page_start + end as usize..page_start + old_end as usize,
                page_start + start as usize,
            );
        }
        if old_end as usize >= chunk {
            bytes[page_start + old_end as usize - chunk..page_start + old_end as usize].fill(0);
        }
        let live = (0..count).any(|index| {
            boundary(bytes, page_start, page_size, index)
                .map(|value| value != 0xFFFF)
                .unwrap_or(false)
        });
        if !live {
            put_u16(bytes, page_start + 0x0A, 0);
            set_boundary(bytes, page_start, page_size, 0, 0x0C)
                .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
            for index in 1..=count {
                set_boundary(bytes, page_start, page_size, index, 0)
                    .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
            }
            bytes[page_start + 0x0C..page_start + old_end as usize].fill(0);
        }
    }
    if was_full {
        put_u16(bytes, page_start + 0x06, 0);
        put_u16(bytes, page_start + 0x08, 4);
    }
    put_u16(bytes, page_start + 0x04, aux);
    Ok(())
}

/// Walk the record's variable-portion chain and free every chunk.  The FCR
/// variable-page metadata is updated to the last freed page.
fn free_record_variable(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    page_size: usize,
    aux: u16,
) -> Result<()> {
    if !file.fcr.file_flags.variable_length() && !file.fcr.file_flags.compression() {
        return Ok(());
    }
    let slot = address as usize;
    let logical = file.fcr.logical_record_length as usize;
    let pointer_offset = if file.fcr.file_flags.compression() {
        slot
    } else {
        slot + logical
    };
    let Some(mut pointer) = read_pointer(bytes, pointer_offset) else {
        return Ok(());
    };
    let mut chain = Vec::new();
    for _ in 0..4096 {
        if pointer == 0xFFFF_FFFF {
            break;
        }
        let page = (pointer >> 16) as usize;
        let chunk_slot = (pointer & 0xFFFF) as usize;
        let page_start = page * page_size;
        if page == 0 || page_start + page_size > bytes.len() {
            return Ok(());
        }
        let count =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        if chunk_slot >= count {
            return Ok(());
        }
        chain.push((page, chunk_slot));
        let Some(start) = boundary(bytes, page_start, page_size, chunk_slot) else {
            return Ok(());
        };
        pointer = if start & 0x8000 != 0 {
            read_pointer(bytes, page_start + (start & 0x7FFF) as usize).unwrap_or(0xFFFF_FFFF)
        } else {
            0xFFFF_FFFF
        };
    }
    for (page, chunk_slot) in chain {
        free_variable_slot(bytes, page, chunk_slot, page_size, aux)?;
        put_u16(bytes, 0x3A, page as u16);
    }
    Ok(())
}

/// One page of a key's B-tree during a delete plan.  Interior pages carry a
/// real record in every entry plus `count + 1` child page links: the first in
/// the page header, then one after each entry except the last.
#[derive(Clone)]
struct DeleteNode {
    page: u32,
    leaf: bool,
    entries: Vec<(Vec<u8>, u32)>,
    children: Vec<u32>,
    old_count: usize,
}

enum DeleteJournalEvent {
    Old(u32),
    Node(u32, Box<DeleteNode>),
}

struct DeleteTree {
    root: u32,
    nodes: HashMap<u32, DeleteNode>,
    parents: HashMap<u32, (u32, usize)>,
    dirty: HashSet<u32>,
    freed: Vec<u32>,
    root_pointer: Option<u32>,
    /// Ordered page images the engine journals while removing the entry.
    journal: Vec<DeleteJournalEvent>,
}

impl DeleteTree {
    fn load(file: &BtrieveFile, pages: &[&crate::index::IndexPage], index: usize) -> Option<Self> {
        let root = file.fcr.key_defs.get(index)?.index_page as u32;
        let mut nodes = HashMap::new();
        let mut parents = HashMap::new();
        let mut queue = vec![root];
        while let Some(number) = queue.pop() {
            if nodes.contains_key(&number) {
                return None;
            }
            let page = pages
                .iter()
                .copied()
                .find(|page| page.page_number == number)?;
            let leaf = page.link_a == 0xFFFF_FFFF;
            let entries: Vec<(Vec<u8>, u32)> = page
                .entries
                .iter()
                .map(|entry| (entry.key.clone(), entry.record_address))
                .collect();
            let mut children = Vec::new();
            if !leaf {
                let count = entries.len();
                let first = page.link_b >> 16;
                if first == 0 {
                    return None;
                }
                children.push(first);
                for entry in page.entries.iter().take(count.saturating_sub(1)) {
                    if entry.next_link == 0xFFFF_FFFF || entry.next_link >> 16 == 0 {
                        return None;
                    }
                    children.push(entry.next_link >> 16);
                }
                let last = page.link_a >> 16;
                if last == 0 {
                    return None;
                }
                children.push(last);
                if children.len() != count + 1 {
                    return None;
                }
            }
            for (position, child) in children.iter().enumerate() {
                if let Some(previous) = parents.insert(*child, (number, position)) {
                    if previous.0 != number || previous.1 != position {
                        return None;
                    }
                }
                queue.push(*child);
            }
            nodes.insert(
                number,
                DeleteNode {
                    page: number,
                    leaf,
                    entries,
                    children,
                    old_count: page.count as usize,
                },
            );
        }
        Some(Self {
            root,
            nodes,
            parents,
            dirty: HashSet::new(),
            freed: Vec::new(),
            root_pointer: None,
            journal: Vec::new(),
        })
    }

    fn mark_free(&mut self, page: u32) {
        self.dirty.remove(&page);
        self.freed.push(page);
    }

    fn rotate_right(&mut self, left: u32, right: u32, parent: u32, separator: usize) {
        let moved_up = self.nodes[&parent].entries[separator].clone();
        let moved_down = {
            let node = self.nodes.get_mut(&right).expect("right child");
            let entry = node.entries.remove(0);
            let child = if node.leaf {
                None
            } else {
                Some(node.children.remove(0))
            };
            (entry, child)
        };
        {
            let node = self.nodes.get_mut(&left).expect("left child");
            node.entries.push(moved_up);
            if let Some(child) = moved_down.1 {
                node.children.push(child);
            }
        }
        self.nodes.get_mut(&parent).expect("parent").entries[separator] = moved_down.0;
        self.dirty.insert(left);
        self.dirty.insert(right);
        self.dirty.insert(parent);
    }

    fn rotate_left(&mut self, left: u32, right: u32, parent: u32, separator: usize) {
        let moved_down = self.nodes[&parent].entries[separator].clone();
        let moved_up = {
            let node = self.nodes.get_mut(&left).expect("left sibling");
            let entry = node.entries.pop().expect("non-empty sibling");
            let child = if node.leaf {
                None
            } else {
                Some(node.children.pop().expect("child link"))
            };
            (entry, child)
        };
        {
            let node = self.nodes.get_mut(&right).expect("right child");
            node.entries.insert(0, moved_down);
            if let Some(child) = moved_up.1 {
                node.children.insert(0, child);
            }
        }
        self.nodes.get_mut(&parent).expect("parent").entries[separator] = moved_up.0;
        self.dirty.insert(left);
        self.dirty.insert(right);
        self.dirty.insert(parent);
    }

    fn merge_right(&mut self, left: u32, right: u32, parent: u32, separator: usize) {
        let middle = self
            .nodes
            .get_mut(&parent)
            .expect("parent")
            .entries
            .remove(separator);
        let mut entries = self.nodes[&left].entries.clone();
        entries.push(middle);
        entries.extend(self.nodes[&right].entries.iter().cloned());
        let mut children = self.nodes[&left].children.clone();
        children.extend(self.nodes[&right].children.iter().copied());
        {
            let node = self.nodes.get_mut(&right).expect("right child");
            node.entries = entries;
            node.children = children;
        }
        {
            let node = self.nodes.get_mut(&parent).expect("parent");
            node.children[separator] = right;
            node.children.remove(separator + 1);
        }
        self.dirty.insert(right);
        self.dirty.insert(parent);
        self.mark_free(left);
    }

    fn merge_left(&mut self, left: u32, right: u32, parent: u32, separator: usize) {
        let middle = self
            .nodes
            .get_mut(&parent)
            .expect("parent")
            .entries
            .remove(separator);
        let mut entries = self.nodes[&left].entries.clone();
        entries.push(middle);
        entries.extend(self.nodes[&right].entries.iter().cloned());
        let mut children = self.nodes[&left].children.clone();
        children.extend(self.nodes[&right].children.iter().copied());
        {
            let node = self.nodes.get_mut(&left).expect("left sibling");
            node.entries = entries;
            node.children = children;
        }
        {
            let node = self.nodes.get_mut(&parent).expect("parent");
            node.children[separator] = left;
            node.children.remove(separator + 1);
        }
        self.dirty.insert(left);
        self.dirty.insert(parent);
        self.mark_free(right);
    }

    fn rebalance(&mut self, mut page: u32, minimum: usize) {
        loop {
            let Some(node) = self.nodes.get(&page) else {
                return;
            };
            if node.entries.len() >= minimum {
                break;
            }
            let Some(&(parent, index)) = self.parents.get(&page) else {
                break;
            };
            let children = self.nodes[&parent].children.clone();
            if index + 1 < children.len() {
                // The engine always looks to the right sibling first.
                let right = children[index + 1];
                self.journal.push(DeleteJournalEvent::Old(parent));
                self.journal.push(DeleteJournalEvent::Old(right));
                if self.nodes[&right].entries.len() > minimum {
                    self.rotate_right(page, right, parent, index);
                    break;
                }
                self.merge_right(page, right, parent, index);
                page = parent;
            } else {
                if index == 0 {
                    break;
                }
                let left = children[index - 1];
                let separator = index - 1;
                self.journal.push(DeleteJournalEvent::Old(parent));
                self.journal.push(DeleteJournalEvent::Old(left));
                if self.nodes[&left].entries.len() > minimum {
                    self.rotate_left(left, page, parent, separator);
                    break;
                }
                self.merge_left(left, page, parent, separator);
                page = parent;
            }
        }
        if page == self.root {
            let collapse = {
                let node = &self.nodes[&page];
                !node.leaf && node.entries.is_empty() && node.children.len() == 1
            };
            if collapse {
                let child = self.nodes[&page].children[0];
                self.root_pointer = Some(child);
                self.mark_free(page);
            }
        }
    }

    /// Remove the entry for `address`, replacing an interior entry with its
    /// predecessor exactly as the DOS engine does.  Returns false when the
    /// tree does not contain the record.
    fn remove(&mut self, address: u32, minimum: usize) -> bool {
        let Some((page, position)) = self.nodes.iter().find_map(|(page, node)| {
            node.entries
                .iter()
                .position(|(_, entry_address)| *entry_address == address)
                .map(|position| (*page, position))
        }) else {
            return false;
        };
        let leaf = {
            let node = &self.nodes[&page];
            if node.leaf {
                page
            } else {
                let mut child = node.children[position];
                loop {
                    let Some(node) = self.nodes.get(&child) else {
                        return false;
                    };
                    if node.leaf {
                        break;
                    }
                    let Some(next) = node.children.last().copied() else {
                        return false;
                    };
                    child = next;
                }
                child
            }
        };
        if leaf != page {
            self.journal.push(DeleteJournalEvent::Old(page));
            let mut intermediate = self.nodes[&page].clone();
            intermediate.entries.remove(position);
            if !intermediate.leaf && position < intermediate.children.len() {
                intermediate.children.remove(position);
            }
            self.journal
                .push(DeleteJournalEvent::Node(page, Box::new(intermediate)));
            let Some(predecessor) = self.nodes[&leaf].entries.last().cloned() else {
                return false;
            };
            self.nodes.get_mut(&page).expect("interior page").entries[position] = predecessor;
            self.dirty.insert(page);
            self.nodes.get_mut(&leaf).expect("leaf page").entries.pop();
            self.journal.push(DeleteJournalEvent::Old(leaf));
            if self.parents.contains_key(&leaf) && self.nodes[&leaf].entries.len() < minimum {
                let node = self.nodes[&leaf].clone();
                self.journal
                    .push(DeleteJournalEvent::Node(leaf, Box::new(node)));
            }
        } else {
            self.journal.push(DeleteJournalEvent::Old(page));
            self.nodes
                .get_mut(&page)
                .expect("leaf page")
                .entries
                .remove(position);
            if self.parents.contains_key(&page) && self.nodes[&page].entries.len() < minimum {
                let node = self.nodes[&page].clone();
                self.journal
                    .push(DeleteJournalEvent::Node(page, Box::new(node)));
            }
        }
        self.dirty.insert(leaf);
        self.rebalance(leaf, minimum);
        true
    }
}

fn store_delete_node(
    bytes: &mut [u8],
    key: &KeySpec,
    node: &DeleteNode,
    aux: u16,
    key_def: usize,
    key_def_entries: u16,
) {
    let page_size = u16::from_le_bytes([bytes[0x08], bytes[0x09]]) as usize;
    let page_start = node.page as usize * page_size;
    if page_start + page_size > bytes.len() {
        return;
    }
    let entry_size = key.entry_size();
    let count = node.entries.len();
    for (position, (value, address)) in node.entries.iter().enumerate() {
        let offset = page_start + 0x10 + position * entry_size;
        if offset + entry_size > page_start + page_size {
            return;
        }
        bytes[offset..offset + key.length as usize].copy_from_slice(value);
        let tail = offset + key.length as usize;
        put_address(bytes, tail, *address);
        let link = if node.leaf {
            if position + 1 == count {
                0
            } else {
                0xFFFF_FFFF
            }
        } else if position + 1 == count {
            0
        } else {
            node.children[position + 1] << 16
        };
        put_u32_le(bytes, tail + 4, link);
    }
    if count < node.old_count {
        for position in count..node.old_count {
            let offset = page_start + 0x10 + position * entry_size;
            bytes[offset..offset + entry_size].fill(0);
        }
    }
    put_u16(bytes, page_start + 0x04, aux);
    put_u16(bytes, page_start + 0x06, count as u16);
    let (link_a, link_b) = if node.leaf {
        (0xFFFF_FFFF, if count == 0 { 0 } else { 0xFFFF_FFFF })
    } else {
        let last = node.children.last().copied().unwrap_or(0);
        let first = node.children.first().copied().unwrap_or(0);
        (last << 16, first << 16)
    };
    put_u32_le(bytes, page_start + 0x08, link_a);
    put_u32_le(bytes, page_start + 0x0C, link_b);
    put_u16(bytes, key_def + 0x06, key_def_entries);
}

/// Free an index page the way the engine does after a merge or root collapse:
/// the page joins the FCR index free list, links to the previous head in
/// `+0x08`, keeps the transaction/page aux, and is otherwise zeroed.
fn store_free_page(bytes: &mut [u8], page: u32, aux: u16) {
    let page_size = u16::from_le_bytes([bytes[0x08], bytes[0x09]]) as usize;
    let start = page as usize * page_size;
    if start + page_size > bytes.len() {
        return;
    }
    let previous = ((u16::from_le_bytes([bytes[0x0C], bytes[0x0D]]) as u32) << 16)
        | u16::from_le_bytes([bytes[0x0E], bytes[0x0F]]) as u32;
    bytes[start..start + page_size].fill(0);
    put_u16(bytes, start, 0);
    put_u16(bytes, start + 0x02, page as u16);
    put_u16(bytes, start + 0x04, aux);
    put_u16(bytes, start + 0x06, 0);
    let next = if previous == 0xFFFF_FFFF {
        0xFFFF_FFFF
    } else {
        previous << 16
    };
    put_u32_le(bytes, start + 0x08, next);
    put_u32_le(bytes, start + 0x0C, 0);
    put_u16(bytes, 0x0C, (page >> 16) as u16);
    put_u16(bytes, 0x0E, page as u16);
}

/// General in-place delete for every form of index tree: interior entries are
/// replaced by their predecessor, underflowing pages borrow from the right
/// sibling first, merge with it when it is minimal, fall back to the left
/// sibling at the right edge, and an emptied root is freed with the key
/// definition pointing at the surviving child.
fn delete_tree_in_place(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    spec: &FileSpec,
) -> Result<Option<Vec<Vec<u8>>>> {
    let page_size = file.page_size.bytes();
    let mut plans = Vec::with_capacity(spec.keys.len());
    for (index, _key) in spec.keys.iter().enumerate() {
        let Some(pages) = key_tree_pages(file, index) else {
            return Ok(None);
        };
        let Some(mut tree) = DeleteTree::load(file, &pages, index) else {
            return Ok(None);
        };
        let minimum = file
            .fcr
            .key_defs
            .get(index)
            .map(|key| key.min_keys_per_page as usize)
            .unwrap_or(0);
        if !tree.remove(address, minimum) {
            return Ok(None);
        }
        plans.push((index, tree));
    }

    let mut journal_images = Vec::new();
    for (index, tree) in &plans {
        let key = &spec.keys[*index];
        let key_def = key_def_offset(file, *index);
        let key_def_entries = file.fcr.key_defs[*index].entry_count.saturating_sub(1);
        for event in &tree.journal {
            let (page, image) = match event {
                DeleteJournalEvent::Old(page) => {
                    let start = *page as usize * page_size;
                    if start + page_size > bytes.len() {
                        continue;
                    }
                    (*page, bytes[start..start + page_size].to_vec())
                }
                DeleteJournalEvent::Node(page, node) => {
                    let mut copy = bytes.to_vec();
                    store_delete_node(&mut copy, key, node, 0, key_def, key_def_entries);
                    let start = *page as usize * page_size;
                    if start + page_size > copy.len() {
                        continue;
                    }
                    if !node.leaf && node.entries.is_empty() {
                        put_u32_le(&mut copy, start + 0x0C, 0);
                    }
                    (*page, copy[start..start + page_size].to_vec())
                }
            };
            let _ = page;
            journal_images.push(image);
        }
    }

    let aux = file.fcr.aux0.wrapping_add(2);
    let page_aux = aux.wrapping_sub(1);
    put_u16(bytes, 0x04, aux);
    for (index, tree) in &plans {
        let key = &spec.keys[*index];
        let key_def = key_def_offset(file, *index);
        let key_def_entries = file.fcr.key_defs[*index].entry_count.saturating_sub(1);
        for page in &tree.dirty {
            if let Some(node) = tree.nodes.get(page) {
                store_delete_node(bytes, key, node, page_aux, key_def, key_def_entries);
            }
        }
        for page in &tree.freed {
            store_free_page(bytes, *page, page_aux);
        }
        if let Some(root) = tree.root_pointer {
            put_u16(bytes, key_def + 0x02, root as u16);
        }
    }

    free_record_variable(bytes, file, address, page_size, page_aux)?;

    let data_page = address as usize / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | page_aux,
    );

    let old_head = ((file.fcr.free_links[2] as u32) << 16) | file.fcr.free_links[3] as u32;
    let slot = address as usize;
    let physical = file.fcr.physical_record_length as usize;
    put_address(bytes, slot, old_head);
    bytes[slot + 4..slot + physical].fill(0);
    put_u16(bytes, 0x10, (address >> 16) as u16);
    put_u16(bytes, 0x12, address as u16);

    let new_count = file.fcr.record_count.saturating_sub(1);
    put_u16(bytes, 0x1A, (new_count >> 16) as u16);
    put_u16(bytes, 0x1C, new_count as u16);
    Ok(Some(journal_images))
}

struct DeletePlan {
    writes: Vec<PageWrite>,
    /// index, root page, emptied child page, old root entry count, new root leaf
    root_empty: Option<(usize, u32, u32, usize, u32)>,
    root_replace: Option<(usize, u32, Vec<u8>, u32)>,
}

fn duplicate_tail_offset(
    file: &BtrieveFile,
    spec: &FileSpec,
    key_index: usize,
    slot: usize,
) -> usize {
    let mut offset = slot;
    if file.fcr.file_flags.compression() {
        offset += 5;
    } else {
        offset += file.fcr.logical_record_length as usize;
        if file.fcr.file_flags.variable_length() {
            offset += 4;
            if file.fcr.file_flags.blank_truncation() {
                offset += 2;
            }
        }
    }
    let duplicate_slot = spec
        .keys
        .iter()
        .take(key_index)
        .filter(|key| key.has_duplicates())
        .count();
    offset + duplicate_slot * 8
}

fn read_key_address(bytes: &[u8], offset: usize) -> Option<u32> {
    let first = u16::from_le_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]) as u32;
    let second = u16::from_le_bytes([*bytes.get(offset + 2)?, *bytes.get(offset + 3)?]) as u32;
    Some((first << 16) | second)
}

fn write_key_address(bytes: &mut [u8], offset: usize, address: u32) {
    put_address(bytes, offset, address);
}

/// Delete one record of a duplicate-key chain in place.  The index entry is
/// updated in place (no tree rebalance), neighbouring duplicate tails are
/// relinked, the data slot joins the free list, and the record's variable or
/// compressed chunks are left allocated but stamped with the page aux.
fn delete_duplicate_in_place(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    spec: &FileSpec,
) -> Result<bool> {
    if file.fcr.file_flags.key_only() {
        return Ok(false);
    }
    let page_size = file.page_size.bytes();
    let aux = file.fcr.aux0.wrapping_add(2);
    let page_aux = aux.wrapping_sub(1);
    let mut entry_updates = Vec::new();
    for (key_index, key_spec) in spec.keys.iter().enumerate() {
        let duplicate_slot = spec
            .keys
            .iter()
            .take(key_index)
            .filter(|key| key.has_duplicates())
            .count();
        if !key_spec.has_duplicates() {
            let mut found = None;
            for page in file
                .index_pages
                .iter()
                .filter(|page| page.key_index == key_index)
            {
                if page.link_a != 0xFFFF_FFFF {
                    continue;
                }
                if let Some(position) = page
                    .entries
                    .iter()
                    .position(|entry| entry.record_address == address)
                {
                    found = Some((page, position));
                    break;
                }
            }
            if let Some((page, position)) = found {
                let page_start = page.page_number as usize * page_size;
                let old_count = page.count as usize;
                let entry_size = key_spec.entry_size();
                for index in position..old_count.saturating_sub(1) {
                    let source = page_start + 0x10 + (index + 1) * entry_size;
                    let destination = page_start + 0x10 + index * entry_size;
                    bytes.copy_within(source..source + entry_size, destination);
                }
                let tail = page_start + 0x10 + old_count.saturating_sub(1) * entry_size;
                bytes[tail..tail + entry_size].fill(0);
                put_u16(bytes, page_start + 0x04, page_aux);
                put_u16(bytes, page_start + 0x06, old_count.saturating_sub(1) as u16);
                if old_count <= 1 {
                    put_u32_le(bytes, page_start + 0x0C, 0);
                }
                put_u16(
                    bytes,
                    key_def_offset(file, key_index) + 0x06,
                    file.fcr.key_defs[key_index].entry_count.saturating_sub(1),
                );
            }
            continue;
        }
        let mut found = None;
        for page in file
            .index_pages
            .iter()
            .filter(|page| page.key_index == key_index)
        {
            for (position, entry) in page.entries.iter().enumerate() {
                let mut previous = 0xFFFF_FFFFu32;
                let mut current = entry.record_address;
                let mut count = 0usize;
                let mut predecessor = 0xFFFF_FFFFu32;
                let mut successor = 0xFFFF_FFFFu32;
                let mut contains = false;
                while current != 0xFFFF_FFFF && count < 4096 {
                    let tail = duplicate_tail_offset(file, spec, duplicate_slot, current as usize);
                    let Some(next) = read_key_address(bytes, tail + 4) else {
                        break;
                    };
                    count += 1;
                    if current == address {
                        contains = true;
                        predecessor = previous;
                        successor = next;
                    }
                    previous = current;
                    current = next;
                }
                if contains {
                    found = Some((page.page_number, position, predecessor, successor, count));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        let Some((page_number, position, predecessor, successor, count)) = found else {
            if key_spec.has_null() || key_spec.is_manual() {
                continue;
            }
            return Ok(false);
        };
        let page = file
            .index_pages
            .iter()
            .find(|page| page.key_index == key_index && page.page_number == page_number)
            .expect("index page");
        if count == 1 {
            if page.link_a != 0xFFFF_FFFF {
                return Ok(false);
            }
            let key = &spec.keys[key_index];
            let key_def = &file.fcr.key_defs[key_index];
            let old_count = page.count as usize;
            let minimum = key_def.min_keys_per_page as usize;
            let root_leaf = key_def.index_page as u32 == page_number;
            if !root_leaf && old_count.saturating_sub(1) < minimum {
                return Ok(false);
            }
            let page_start = page_number as usize * page_size;
            let entry_size = key.entry_size();
            for index in position..old_count.saturating_sub(1) {
                let source = page_start + 0x10 + (index + 1) * entry_size;
                let destination = page_start + 0x10 + index * entry_size;
                bytes.copy_within(source..source + entry_size, destination);
            }
            let tail = page_start + 0x10 + old_count.saturating_sub(1) * entry_size;
            bytes[tail..tail + entry_size].fill(0);
            put_u16(bytes, page_start + 0x04, page_aux);
            put_u16(bytes, page_start + 0x06, old_count.saturating_sub(1) as u16);
            if old_count <= 1 {
                put_u32_le(bytes, page_start + 0x0C, 0);
            }
            put_u16(
                bytes,
                key_def_offset(file, key_index) + 0x06,
                key_def.entry_count.saturating_sub(1),
            );
            continue;
        }
        let entry = &page.entries[position];
        let first = entry.record_address;
        let last = entry.duplicate_address.unwrap_or(entry.record_address);
        let mut new_first = first;
        let mut new_last = last;
        let remaining = if successor != 0xFFFF_FFFF {
            successor
        } else {
            predecessor
        };
        if count == 2 {
            let tail = duplicate_tail_offset(file, spec, duplicate_slot, remaining as usize);
            write_key_address(bytes, tail, 0xFFFF_FFFF);
            write_key_address(bytes, tail + 4, 0xFFFF_FFFF);
            new_first = remaining;
            new_last = remaining;
        } else {
            if predecessor != 0xFFFF_FFFF {
                let tail = duplicate_tail_offset(file, spec, duplicate_slot, predecessor as usize);
                write_key_address(bytes, tail + 4, successor);
            } else {
                new_first = successor;
            }
            if successor != 0xFFFF_FFFF {
                let tail = duplicate_tail_offset(file, spec, duplicate_slot, successor as usize);
                write_key_address(bytes, tail, predecessor);
            } else {
                new_last = predecessor;
            }
        }
        entry_updates.push((key_index, page_number, position, new_first, new_last));
    }

    for (key_index, page_number, position, new_first, new_last) in entry_updates {
        let page = file
            .index_pages
            .iter()
            .find(|page| page.key_index == key_index && page.page_number == page_number)
            .expect("index page");
        let key = &spec.keys[key_index];
        let page_start = page_number as usize * page_size;
        let entry_size = key.entry_size();
        let offset = page_start + 0x10 + position * entry_size + key.length as usize;
        put_address(bytes, offset, new_first);
        if page.entries[position].duplicate_address.is_some() {
            put_address(bytes, offset + 4, new_last);
        }
        put_u16(bytes, page_start + 0x04, page_aux);
    }

    free_record_variable(bytes, file, address, page_size, page_aux)?;

    put_u16(bytes, 0x04, aux);
    let data_page = address as usize / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | page_aux,
    );
    let old_head = ((file.fcr.free_links[2] as u32) << 16) | file.fcr.free_links[3] as u32;
    let slot = address as usize;
    let physical = file.fcr.physical_record_length as usize;
    put_address(bytes, slot, old_head);
    bytes[slot + 4..slot + physical].fill(0);
    put_u16(bytes, 0x10, (address >> 16) as u16);
    put_u16(bytes, 0x12, address as u16);
    let count = file.fcr.record_count.saturating_sub(1);
    put_u16(bytes, 0x1A, (count >> 16) as u16);
    put_u16(bytes, 0x1C, count as u16);
    Ok(true)
}

fn delete_fixed_in_place(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    spec: &FileSpec,
) -> Result<bool> {
    let page_size = file.page_size.bytes();
    let mut plans = Vec::with_capacity(spec.keys.len());
    for (index, key) in spec.keys.iter().enumerate() {
        let Some(pages) = key_tree_pages(file, index) else {
            return Ok(false);
        };
        let root_number = file.fcr.key_defs[index].index_page as u32;
        let Some(root) = tree_page(&pages, root_number) else {
            return Ok(false);
        };
        let root_is_leaf = root.link_a == 0xFFFF_FFFF;
        let mut plan = DeletePlan {
            writes: Vec::new(),
            root_empty: None,
            root_replace: None,
        };
        let leaf_hit = pages
            .iter()
            .filter(|page| page.link_a == 0xFFFF_FFFF)
            .find_map(|page| {
                page.entries
                    .iter()
                    .position(|entry| entry.record_address == address)
                    .map(|position| (*page, position))
            });
        if let Some((leaf, position)) = leaf_hit {
            let remaining: Vec<(Vec<u8>, u32)> = leaf
                .entries
                .iter()
                .enumerate()
                .filter(|(slot, _)| *slot != position)
                .map(|(_, entry)| (entry.key.clone(), entry.record_address))
                .collect();
            let minimum = file.fcr.key_defs[index].min_keys_per_page as usize;
            let underflow = remaining.len() < minimum;
            if root_is_leaf {
                plan.writes.push((index, leaf.page_number, remaining, true));
            } else if root.count == 1 && underflow {
                let left = root.link_b >> 16;
                let right = root.link_a >> 16;
                let sibling_is_right = if left == leaf.page_number {
                    true
                } else if right == leaf.page_number {
                    false
                } else {
                    return Ok(false);
                };
                let sibling = if sibling_is_right { right } else { left };
                let Some(sibling_page) = tree_page(&pages, sibling) else {
                    return Ok(false);
                };
                let mut sibling_entries: Vec<(Vec<u8>, u32)> = sibling_page
                    .entries
                    .iter()
                    .map(|entry| (entry.key.clone(), entry.record_address))
                    .collect();
                if sibling_page.count as usize > minimum {
                    // Rotate one separator through the root.
                    let mut leaf_entries = remaining;
                    let promoted = if sibling_is_right {
                        leaf_entries
                            .push((root.entries[0].key.clone(), root.entries[0].record_address));
                        sibling_entries.remove(0)
                    } else {
                        leaf_entries.insert(
                            0,
                            (root.entries[0].key.clone(), root.entries[0].record_address),
                        );
                        sibling_entries.pop().expect("non-empty sibling")
                    };
                    if leaf_entries.len() > (page_size - 12) / key.entry_size() {
                        return Ok(false);
                    }
                    plan.writes
                        .push((index, leaf.page_number, leaf_entries, false));
                    plan.writes.push((index, sibling, sibling_entries, true));
                    plan.root_replace = Some((index, root_number, promoted.0, promoted.1));
                } else {
                    // Merge the underflowing leaf, separator, and sibling.
                    let mut entries = remaining;
                    entries.push((root.entries[0].key.clone(), root.entries[0].record_address));
                    entries.extend(sibling_entries);
                    entries
                        .sort_by(|left, right| key.compare(&left.0, &right.0, spec.acs.as_ref()));
                    if entries.len() > (page_size - 12) / key.entry_size() {
                        return Ok(false);
                    }
                    plan.writes.push((index, sibling, entries, false));
                    plan.writes
                        .push((index, leaf.page_number, Vec::new(), true));
                    plan.root_empty = Some((
                        index,
                        root_number,
                        leaf.page_number,
                        root.entries.len(),
                        sibling,
                    ));
                }
            } else if remaining.is_empty() && root.count == 0 {
                plan.writes
                    .push((index, leaf.page_number, Vec::new(), true));
            } else if underflow {
                return Ok(false);
            } else {
                plan.writes.push((index, leaf.page_number, remaining, true));
            }
        } else {
            if root_is_leaf {
                return Ok(false);
            }
            let Some(_position) = root
                .entries
                .iter()
                .position(|entry| entry.record_address == address)
            else {
                return Ok(false);
            };
            if root.count != 1 {
                return Ok(false);
            }
            let left = root.link_b >> 16;
            if left == 0 {
                return Ok(false);
            }
            let mut cursor = match tree_page(&pages, left) {
                Some(page) => page,
                None => return Ok(false),
            };
            for _ in 0..64 {
                if cursor.link_a == 0xFFFF_FFFF {
                    break;
                }
                let child = cursor.link_a >> 16;
                cursor = match tree_page(&pages, child) {
                    Some(page) => page,
                    None => return Ok(false),
                };
            }
            if cursor.count <= 1 || cursor.entries.is_empty() {
                return Ok(false);
            }
            let predecessor = cursor.entries.last().expect("non-empty").clone();
            let entries = cursor
                .entries
                .iter()
                .take(cursor.entries.len() - 1)
                .map(|entry| (entry.key.clone(), entry.record_address))
                .collect();
            plan.writes.push((index, cursor.page_number, entries, true));
            plan.root_replace = Some((
                index,
                root_number,
                predecessor.key.clone(),
                predecessor.record_address,
            ));
        }
        plans.push(plan);
    }

    let aux = file.fcr.aux0.wrapping_add(2);
    let page_aux = aux.wrapping_sub(1);
    put_u16(bytes, 0x04, aux);
    for plan in plans {
        for (index, page_number, entries, clear_tail) in plan.writes {
            let Some(page) = file
                .index_pages
                .iter()
                .find(|page| page.key_index == index && page.page_number == page_number)
            else {
                return Ok(false);
            };
            store_key_entries(
                bytes,
                page,
                &spec.keys[index],
                &entries,
                page_aux,
                key_def_offset(file, index),
                file.fcr.key_defs[index].entry_count.saturating_sub(1),
                clear_tail,
            );
        }
        if let Some((index, root_number, emptied, old_count, sibling)) = plan.root_empty {
            let page_start = root_number as usize * page_size;
            let entry_size = spec.keys[index].entry_size();
            bytes[page_start + 0x10..page_start + 0x10 + old_count * entry_size].fill(0);
            put_u16(bytes, page_start + 0x04, page_aux);
            put_u16(bytes, page_start + 0x06, 0);
            put_u32_le(bytes, page_start + 0x08, emptied << 16);
            put_u32_le(bytes, page_start + 0x0C, 0);
            // The former root is placed on the free-page list and the key
            // definition now names the merged sibling as the root leaf.
            put_u16(bytes, 0x0C, (root_number >> 16) as u16);
            put_u16(bytes, 0x0E, root_number as u16);
            put_u16(bytes, key_def_offset(file, index) + 0x02, sibling as u16);
        }
        if let Some((_index, root_number, key, record_address)) = plan.root_replace {
            let page_start = root_number as usize * page_size;
            let offset = page_start + 0x10;
            bytes[offset..offset + key.len()].copy_from_slice(&key);
            let tail = offset + key.len();
            put_address(bytes, tail, record_address);
            put_u32_le(bytes, tail + 4, 0);
            put_u16(bytes, page_start + 0x04, page_aux);
            put_u16(bytes, page_start + 0x06, 1);
        }
    }

    free_record_variable(bytes, file, address, page_size, page_aux)?;

    let data_page = address as usize / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | page_aux,
    );

    let old_head = ((file.fcr.free_links[2] as u32) << 16) | file.fcr.free_links[3] as u32;
    let slot = address as usize;
    let physical = file.fcr.physical_record_length as usize;
    put_address(bytes, slot, old_head);
    bytes[slot + 4..slot + physical].fill(0);
    put_u16(bytes, 0x10, (address >> 16) as u16);
    put_u16(bytes, 0x12, address as u16);

    let new_count = file.fcr.record_count.saturating_sub(1);
    put_u16(bytes, 0x1A, (new_count >> 16) as u16);
    put_u16(bytes, 0x1C, new_count as u16);
    Ok(true)
}

/// Return the image of one page after freeing this record's chunks on it.
/// The DOS engine journals these intermediate images while rewriting a
/// variable or compressed record.
pub fn cleared_chunk_page(
    bytes: &[u8],
    file: &BtrieveFile,
    address: u32,
    page: usize,
    aux: u16,
) -> Option<Vec<u8>> {
    let page_size = file.page_size.bytes();
    let slot = address as usize;
    let mut copy = bytes.to_vec();
    let mut slots = Vec::new();
    if file.fcr.file_flags.compression() {
        let (chunks, _) = compressed_chunks(bytes, file, address)?;
        for (chunk_page, chunk_slot, _, _) in chunks {
            if chunk_page == page {
                slots.push(chunk_slot);
            }
        }
    } else {
        let pointer_offset = slot + file.fcr.logical_record_length as usize;
        let mut pointer = read_pointer(bytes, pointer_offset)?;
        for _ in 0..4096 {
            if pointer == 0xFFFF_FFFF {
                break;
            }
            let chunk_page = (pointer >> 16) as usize;
            let chunk_slot = (pointer & 0xFFFF) as usize;
            let page_start = chunk_page * page_size;
            if chunk_page == 0 || page_start + page_size > bytes.len() {
                return None;
            }
            let start = boundary(bytes, page_start, page_size, chunk_slot)?;
            if chunk_page == page {
                slots.push(chunk_slot);
            }
            pointer = if start & 0x8000 != 0 {
                read_pointer(bytes, page_start + (start & 0x7FFF) as usize)?
            } else {
                0xFFFF_FFFF
            };
        }
    }
    slots.sort_unstable();
    for chunk_slot in slots.into_iter().rev() {
        free_variable_slot(&mut copy, page, chunk_slot, page_size, aux).ok()?;
    }
    let start = page * page_size;
    copy.get(start..start + page_size)
        .map(|image| image.to_vec())
}

/// Number of chunk entries in a variable-length record's chain.  This is the
/// number of page images the DOS engine journals while rewriting the chain,
/// not the number of distinct pages (a page can hold several chunks).
pub fn variable_chunk_pages(bytes: &[u8], file: &BtrieveFile, address: u32) -> Option<Vec<usize>> {
    let page_size = file.page_size.bytes();
    let slot = address as usize;
    let pointer_offset = slot + file.fcr.logical_record_length as usize;
    let mut pointer = read_pointer(bytes, pointer_offset)?;
    let mut pages = Vec::new();
    for _ in 0..4096 {
        if pointer == 0xFFFF_FFFF {
            return Some(pages);
        }
        let page = (pointer >> 16) as usize;
        let chunk_slot = (pointer & 0xFFFF) as usize;
        let page_start = page * page_size;
        if page == 0 || page_start + page_size > bytes.len() {
            return None;
        }
        let boundaries =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        if chunk_slot >= boundaries {
            return None;
        }
        let start = boundary(bytes, page_start, page_size, chunk_slot)?;
        if start == 0xFFFF {
            return None;
        }
        let mut end_value = None;
        for index in chunk_slot + 1..=boundaries {
            let value = boundary(bytes, page_start, page_size, index)?;
            if value != 0xFFFF {
                end_value = Some(value & 0x7FFF);
                break;
            }
        }
        end_value?;
        pages.push(page);
        pointer = if start & 0x8000 != 0 {
            read_pointer(bytes, page_start + (start & 0x7FFF) as usize)?
        } else {
            0xFFFF_FFFF
        };
    }
    None
}

pub fn variable_chunk_count(bytes: &[u8], file: &BtrieveFile, address: u32) -> Option<usize> {
    variable_chunk_pages(bytes, file, address).map(|pages| pages.len())
}

/// Number of chunk entries in a compressed record's chain.
pub fn compressed_chunk_count(bytes: &[u8], file: &BtrieveFile, address: u32) -> Option<usize> {
    compressed_chunks(bytes, file, address).map(|(chunks, _)| chunks.len())
}

/// Ordered page numbers of a compressed record's chunk entries.
pub fn compressed_chunk_pages(
    bytes: &[u8],
    file: &BtrieveFile,
    address: u32,
) -> Option<Vec<usize>> {
    compressed_chunks(bytes, file, address)
        .map(|(chunks, _)| chunks.into_iter().map(|(page, ..)| page).collect())
}

/// (page, chunk slot, data start, data end) for one compressed chunk.
type CompressedChunk = (usize, usize, usize, usize);

/// Walk a compressed record's chunk chain and return its data segments and
/// total compressed byte count.  Compressed records store the chain pointer in
/// the first four bytes of the data slot.
fn compressed_chunks(
    bytes: &[u8],
    file: &BtrieveFile,
    address: u32,
) -> Option<(Vec<CompressedChunk>, usize)> {
    let page_size = file.page_size.bytes();
    let slot = address as usize;
    let mut pointer = read_pointer(bytes, slot)?;
    let mut chunks = Vec::new();
    let mut total = 0usize;
    for _ in 0..4096 {
        if pointer == 0xFFFF_FFFF {
            break;
        }
        let page = (pointer >> 16) as usize;
        let chunk_slot = (pointer & 0xFFFF) as usize;
        let page_start = page * page_size;
        if page == 0 || page_start + page_size > bytes.len() {
            return None;
        }
        let count =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        if chunk_slot >= count {
            return None;
        }
        let start = boundary(bytes, page_start, page_size, chunk_slot)?;
        if start == 0xFFFF {
            return None;
        }
        let mut end_value = None;
        for index in chunk_slot + 1..=count {
            let value = boundary(bytes, page_start, page_size, index)?;
            if value != 0xFFFF {
                end_value = Some(value & 0x7FFF);
                break;
            }
        }
        let end = end_value?;
        let chunk_start = (start & 0x7FFF) as usize;
        let data_start = chunk_start + if start & 0x8000 != 0 { 4 } else { 0 };
        if data_start > end as usize || end as usize > page_size {
            return None;
        }
        total += end as usize - data_start;
        chunks.push((page, chunk_slot, data_start, end as usize));
        pointer = if start & 0x8000 != 0 {
            read_pointer(bytes, page_start + chunk_start)?
        } else {
            0xFFFF_FFFF
        };
    }
    Some((chunks, total))
}

fn rewrite_compressed_chain(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    image: &[u8],
    aux: u16,
) -> Result<bool> {
    let page_size = file.page_size.bytes();
    let Some((chunks, total)) = compressed_chunks(bytes, file, address) else {
        return Ok(false);
    };
    if total != image.len() || total == 0 {
        return Ok(false);
    }
    let mut cursor = 0usize;
    for (page, _, data_start, end) in chunks {
        let page_start = page * page_size;
        let length = end - data_start;
        bytes[page_start + data_start..page_start + end]
            .copy_from_slice(&image[cursor..cursor + length]);
        cursor += length;
        put_u16(bytes, page_start + 0x04, aux.wrapping_sub(1));
    }
    put_u16(bytes, 0x04, aux);
    let data_page = address as usize / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | aux.wrapping_sub(1),
    );
    Ok(true)
}

fn compressed_free_pages(bytes: &[u8], page_size: usize) -> HashSet<u32> {
    let mut pages = HashSet::new();
    let head = ((u16::from_le_bytes([bytes[0x0C], bytes[0x0D]]) as u32) << 16)
        | u16::from_le_bytes([bytes[0x0E], bytes[0x0F]]) as u32;
    let mut page = head;
    for _ in 0..4096 {
        if page == 0xFFFF || page == 0xFFFF_FFFF {
            break;
        }
        let index = page as usize;
        let start = index * page_size;
        if index == 0 || start + page_size > bytes.len() || !pages.insert(page) {
            break;
        }
        let next = u32::from_le_bytes([
            bytes[start + 0x08],
            bytes[start + 0x09],
            bytes[start + 0x0A],
            bytes[start + 0x0B],
        ]);
        if next == 0xFFFF_FFFF || next == 0 {
            break;
        }
        page = next >> 16;
    }
    pages
}

fn compressed_full_threshold(bytes: &[u8]) -> usize {
    let flags = if bytes.len() >= 0x108 {
        u16::from_le_bytes([bytes[0x106], bytes[0x107]])
    } else {
        0
    };
    match (flags >> 6) & 0x03 {
        0 => 5,
        1 => 10,
        2 => 20,
        _ => 30,
    }
}

fn compressed_variable_pages(file: &BtrieveFile, bytes: &[u8], page_size: usize) -> Vec<usize> {
    let index_pages: HashSet<u32> = file
        .index_pages
        .iter()
        .map(|page| page.page_number)
        .collect();
    let free_pages = compressed_free_pages(bytes, page_size);
    let acs_page = file.acs.as_ref().map(|_| 1u32);
    let pages = bytes.len().div_ceil(page_size);
    let mut variable = Vec::new();
    for number in 1..pages {
        let start = number * page_size;
        if start + 0x10 > bytes.len() {
            break;
        }
        let header_page = u16::from_le_bytes([bytes[start + 0x02], bytes[start + 0x03]]) as usize;
        if header_page != number
            || index_pages.contains(&(number as u32))
            || free_pages.contains(&(number as u32))
            || acs_page == Some(number as u32)
            || u16::from_le_bytes([bytes[start + 0x04], bytes[start + 0x05]]) & 0x8000 != 0
        {
            continue;
        }
        let count = u16::from_le_bytes([bytes[start + 0x0A], bytes[start + 0x0B]]) as usize;
        if count > page_size / 2 {
            continue;
        }
        variable.push(number);
    }
    variable
}

/// Capture the "page full" state before a compressed update.  The engine only
/// demotes a full page to the `0000/0004` state; it never rewrites a page that
/// was already non-full unless the update fills it.
fn compressed_full_map(file: &BtrieveFile, bytes: &[u8], page_size: usize) -> HashMap<usize, bool> {
    compressed_variable_pages(file, bytes, page_size)
        .into_iter()
        .map(|page| {
            let start = page * page_size;
            let full = u16::from_le_bytes([bytes[start + 0x06], bytes[start + 0x07]]) == 0xFFFF;
            (page, full)
        })
        .collect()
}

/// Refresh variable-page full flags and the FCR's first-free-page metadata.
/// A page is full when its free bytes are below the file's threshold
/// (5% by default).  The low byte of FCR +0x38 is preserved; its high byte is
/// FF when no variable page has free space.  FCR +0x3A names the lowest
/// non-full variable page, or FFFF when every page is full.
fn normalize_compressed_pages(
    bytes: &mut [u8],
    file: &BtrieveFile,
    page_size: usize,
    old_full: &HashMap<usize, bool>,
) {
    let percent = compressed_full_threshold(bytes);
    let mut first_free = None;
    for page in compressed_variable_pages(file, bytes, page_size) {
        let start = page * page_size;
        let count = u16::from_le_bytes([bytes[start + 0x0A], bytes[start + 0x0B]]) as usize;
        let mut end = 0x0Cusize;
        for index in 0..=count {
            let offset = start + page_size - 2 * (index + 1);
            if offset + 2 > bytes.len() {
                break;
            }
            let value = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            if value != 0xFFFF {
                end = end.max((value & 0x7FFF) as usize);
            }
        }
        let table = 2 * (count + 1);
        let free = page_size.saturating_sub(table).saturating_sub(end);
        let full = free < page_size * percent / 100;
        if full {
            put_u16(bytes, start + 0x06, 0xFFFF);
            put_u16(bytes, start + 0x08, 0xFFFF);
        } else {
            if old_full.get(&page).copied().unwrap_or(false) {
                put_u16(bytes, start + 0x06, 0);
                put_u16(bytes, start + 0x08, 4);
            }
            first_free = Some(first_free.map_or(page, |first: usize| first.min(page)));
        }
    }
    let low = if file.fcr.file_flags.blank_truncation() {
        0xFD
    } else if file.fcr.file_flags.variable_length() {
        0xFF
    } else {
        0x00
    };
    let high = if first_free.is_some() { 0x00 } else { 0xFF };
    bytes[0x38] = low;
    bytes[0x39] = high;
    put_u16(
        bytes,
        0x3A,
        first_free.map(|page| page as u16).unwrap_or(0xFFFF),
    );
}

/// Update a compressed record in place.  The engine merges a short image into
/// the existing fixed-length record for non-variable compressed files and
/// replaces the whole record for compressed variable files, then rewrites the
/// compressed chunk chain using the same grow/shrink rules as variable data.
pub fn update_compressed_at_in_place(
    path: impl AsRef<Path>,
    address: u32,
    new_record: &[u8],
) -> Result<bool> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    if !file.fcr.file_flags.compression() {
        return Ok(false);
    }
    let spec = FileSpec::from_file(&file)?;
    let Some(old) = file
        .records()?
        .into_iter()
        .find(|record| record.address == address)
    else {
        return Ok(false);
    };
    let compressed_length = file.fcr.compressed_record_length as usize;
    let record = if file.fcr.file_flags.variable_length() {
        new_record.to_vec()
    } else {
        let mut merged = old.data.clone();
        let overlap = new_record.len().min(merged.len());
        merged[..overlap].copy_from_slice(&new_record[..overlap]);
        if merged.len() < compressed_length {
            merged.resize(compressed_length, 0);
        }
        merged.truncate(compressed_length);
        merged
    };
    let keys_changed =
        spec.keys.iter().any(
            |key| match (key.build_key(&old.data), key.build_key(&record)) {
                (Ok(old), Ok(new)) => key.compare(&old, &new, spec.acs.as_ref()) != Ordering::Equal,
                _ => true,
            },
        );
    if keys_changed {
        return Ok(false);
    }
    let image = compress_record(&record);
    let aux = file.fcr.aux0.wrapping_add(2);
    let page_size = file.page_size.bytes();
    let mut bytes = read_working(path, &file)?;
    let old_full = compressed_full_map(&file, &bytes, page_size);
    let Some((_, old_length)) = compressed_chunks(&bytes, &file, address) else {
        return Ok(false);
    };
    let updated = if image.len() == old_length {
        rewrite_compressed_chain(&mut bytes, &file, address, &image, aux)?
    } else if image.len() > old_length {
        extend_variable_record_in_place(&mut bytes, &file, address, &image, old_length, aux)?
    } else {
        shrink_variable_record_in_place(&mut bytes, &file, address, &image, old_length, aux)?
    };
    if !updated {
        return Ok(false);
    }
    normalize_compressed_pages(&mut bytes, &file, page_size, &old_full);
    write_working(path, &file, bytes)?;
    Ok(true)
}

/// A rejected Update with a record image shorter than the logical length
/// advances the FCR counter and stamps the aux of every chunk page in the
/// existing record's variable chain, while leaving the record and data page
/// untouched.
pub fn touch_variable_chain_aux(path: impl AsRef<Path>, address: u32, aux: u16) -> Result<()> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    if !file.fcr.file_flags.variable_length() && !file.fcr.file_flags.compression() {
        return Ok(());
    }
    let page_size = file.page_size.bytes();
    let mut bytes = read_working(path, &file)?;
    let slot = address as usize;
    let logical = file.fcr.logical_record_length as usize;
    let pointer_offset = if file.fcr.file_flags.compression() {
        slot
    } else {
        slot + logical
    };
    let Some(mut pointer) = read_pointer(&bytes, pointer_offset) else {
        return Ok(());
    };
    for _ in 0..4096 {
        if pointer == 0xFFFF_FFFF {
            break;
        }
        let page = (pointer >> 16) as usize;
        let chunk_slot = (pointer & 0xFFFF) as usize;
        let page_start = page * page_size;
        if page == 0 || page_start + page_size > bytes.len() {
            break;
        }
        let count =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        if chunk_slot >= count {
            break;
        }
        put_u16(&mut bytes, page_start + 0x04, aux);
        let Some(start) = boundary(&bytes, page_start, page_size, chunk_slot) else {
            break;
        };
        pointer = if start & 0x8000 != 0 {
            read_pointer(&bytes, page_start + (start & 0x7FFF) as usize).unwrap_or(0xFFFF_FFFF)
        } else {
            0xFFFF_FFFF
        };
    }
    write_working(path, &file, bytes)?;
    Ok(())
}

/// Apply the DOS engine's update counters/aux to a variable-length record
/// that was produced by a rebuild: FCR counter, its data page, and every
/// variable chunk page.
pub fn touch_variable_update(
    path: impl AsRef<Path>,
    address: u32,
    counter: u16,
    old_page_count: usize,
) -> Result<()> {
    let path = path.as_ref();
    let file = BtrieveFile::open(path)?;
    if !file.fcr.file_flags.variable_length() && !file.fcr.file_flags.compression() {
        return Ok(());
    }
    let page_size = file.page_size.bytes();
    let logical = if file.fcr.file_flags.compression() {
        0
    } else {
        file.fcr.logical_record_length as usize
    };
    let mut bytes = read_working(path, &file)?;
    let slot = address as usize;
    if slot + logical > bytes.len() {
        return Ok(());
    }
    put_u16(&mut bytes, 0x04, counter);
    let aux = counter.wrapping_sub(1);
    let data_page = slot / page_size;
    put_u16(
        &mut bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | aux,
    );
    let Some(mut pointer) = read_pointer(&bytes, slot + logical) else {
        write_working(path, &file, bytes)?;
        return Ok(());
    };
    for _ in 0..4096 {
        if pointer == 0xFFFF_FFFF {
            break;
        }
        let page = (pointer >> 16) as usize;
        let chunk_slot = (pointer & 0xFFFF) as usize;
        let page_start = page * page_size;
        if page == 0 || page_start + page_size > bytes.len() {
            break;
        }
        put_u16(
            &mut bytes,
            page_start + 0x04,
            if page < old_page_count { aux } else { 0 },
        );
        let count =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        if chunk_slot >= count {
            break;
        }
        let Some(start) = boundary(&bytes, page_start, page_size, chunk_slot) else {
            break;
        };
        if start == 0xFFFF {
            break;
        }
        pointer = if start & 0x8000 != 0 {
            read_pointer(&bytes, page_start + (start & 0x7FFF) as usize).unwrap_or(0xFFFF_FFFF)
        } else {
            0xFFFF_FFFF
        };
    }
    write_working(path, &file, bytes)?;
    Ok(())
}

/// Extend a variable record in place: rewrite the existing chain with the
/// new prefix and append a continuation chunk holding the extra bytes, the
/// same layout the DOS engine produces.
fn extend_variable_record_in_place(
    bytes: &mut Vec<u8>,
    file: &BtrieveFile,
    address: u32,
    new_record: &[u8],
    old_variable_length: usize,
    aux: u16,
) -> Result<bool> {
    let page_size = file.page_size.bytes();
    let logical = file.fcr.logical_record_length as usize;
    let slot = address as usize;
    if slot + logical > bytes.len() || new_record.len() < logical + old_variable_length {
        return Ok(false);
    }
    let new_variable_length = new_record.len() - logical;
    if new_variable_length <= old_variable_length {
        return Ok(false);
    }
    let Some(mut pointer) = read_pointer(bytes, slot + logical) else {
        return Ok(false);
    };
    if pointer == 0xFFFF_FFFF && old_variable_length != 0 {
        return Ok(false);
    }
    let mut chunks = Vec::new();
    let mut total = 0usize;
    for _ in 0..4096 {
        if pointer == 0xFFFF_FFFF {
            break;
        }
        let page = (pointer >> 16) as usize;
        let chunk_slot = (pointer & 0xFFFF) as usize;
        let page_start = page * page_size;
        if page == 0 || page_start + page_size > bytes.len() {
            return Ok(false);
        }
        let count =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        if chunk_slot >= count {
            return Ok(false);
        }
        let Some(start_boundary) = boundary(bytes, page_start, page_size, chunk_slot) else {
            return Ok(false);
        };
        if start_boundary == 0xFFFF {
            return Ok(false);
        }
        let mut end_value = None;
        for index in chunk_slot + 1..=count {
            let Some(value) = boundary(bytes, page_start, page_size, index) else {
                return Ok(false);
            };
            if value != 0xFFFF {
                end_value = Some(value & 0x7FFF);
                break;
            }
        }
        let Some(end) = end_value else {
            return Ok(false);
        };
        let chunk_start = (start_boundary & 0x7FFF) as usize;
        let data_start = chunk_start + if start_boundary & 0x8000 != 0 { 4 } else { 0 };
        if data_start > end as usize || end as usize > page_size {
            return Ok(false);
        }
        total += end as usize - data_start;
        chunks.push((page, chunk_slot, data_start, end as usize, start_boundary));
        pointer = if start_boundary & 0x8000 != 0 {
            read_pointer(bytes, page_start + chunk_start).unwrap_or(0xFFFF_FFFF)
        } else {
            0xFFFF_FFFF
        };
    }
    if total != old_variable_length {
        return Ok(false);
    }
    let old_head = read_pointer(bytes, slot + logical).unwrap_or(0xFFFF_FFFF);
    let extra = new_variable_length - old_variable_length;
    bytes[slot..slot + logical].copy_from_slice(&new_record[..logical]);

    // Grow the head chunk in place when it is the final slot on its page.
    if let Some(&(page, chunk_slot, head_data_start, head_end, _)) = chunks.first() {
        let page_start = page * page_size;
        let count =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        let mut page_end = None;
        for index in (0..=count).rev() {
            let Some(value) = boundary(bytes, page_start, page_size, index) else {
                return Ok(false);
            };
            if value != 0xFFFF {
                page_end = Some(value & 0x7FFF);
                break;
            }
        }
        let page_end = page_end.unwrap_or(head_end as u16) as usize;
        let capacity = page_size
            .saturating_sub(2 * (count + 1))
            .saturating_sub(page_end);
        let growth = capacity.min(extra);
        if growth > 0 {
            if page_end > head_end {
                bytes.copy_within(
                    page_start + head_end..page_start + page_end,
                    page_start + head_end + growth,
                );
            }
            set_boundary(
                bytes,
                page_start,
                page_size,
                chunk_slot + 1,
                (head_end + growth) as u16,
            )
            .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
            for index in chunk_slot + 2..=count {
                let Some(value) = boundary(bytes, page_start, page_size, index) else {
                    return Ok(false);
                };
                if value != 0xFFFF {
                    set_boundary(
                        bytes,
                        page_start,
                        page_size,
                        index,
                        value.wrapping_add(growth as u16),
                    )
                    .ok_or(FormatError::InvalidRecordAddress(page as u32))?;
                }
            }
            if growth < extra {
                // Fill the old chain to the page's capacity first; a fresh
                // head chunk holds the remaining prefix and points back at
                // it, exactly like the DOS engine's compressed growth.
                return extend_variable_record_in_place(
                    bytes,
                    file,
                    address,
                    new_record,
                    old_variable_length + growth,
                    aux,
                );
            }
            let mut cursor = logical;
            for (index, &(chunk_page, this_slot, chunk_start, chunk_end, _)) in
                chunks.iter().enumerate()
            {
                let mut take = chunk_end - chunk_start;
                let mut start = chunk_start;
                if index == 0 {
                    take += growth;
                } else if chunk_page == page && this_slot > chunk_slot {
                    start += growth;
                }
                let page_base = chunk_page * page_size;
                bytes[page_base + start..page_base + start + take]
                    .copy_from_slice(&new_record[cursor..cursor + take]);
                cursor += take;
                put_u16(bytes, page_base + 0x04, aux.wrapping_sub(1));
            }
            let _ = head_data_start;
            put_u16(bytes, 0x04, aux);
            let data_page = slot / page_size;
            put_u16(
                bytes,
                data_page * page_size + 0x04,
                DATA_PAGE_FLAG | aux.wrapping_sub(1),
            );
            return Ok(true);
        }
    }
    let continuation = old_variable_length != 0;
    let new_head_data = if continuation {
        extra
    } else {
        new_variable_length
    };
    let required = new_head_data + if continuation { 4 } else { 0 };
    let mut variable_page = file.fcr.variable_meta[1] as usize;
    let mut page_start = variable_page * page_size;
    let mut fresh = false;
    if variable_page <= 1
        || variable_page >= file.fcr.total_pages as usize
        || page_start + page_size > bytes.len()
    {
        variable_page = file.fcr.total_pages as usize;
        page_start = variable_page * page_size;
        if page_start + page_size > bytes.len() {
            bytes.resize(page_start + page_size, 0);
        }
        bytes[page_start..page_start + page_size].fill(0);
        put_u16(bytes, page_start, 0);
        put_u16(bytes, page_start + 2, variable_page as u16);
        put_u16(bytes, page_start + 6, 0x00FF);
        put_u16(bytes, page_start + 8, 0xFFFF);
        put_u16(bytes, 0x28, (variable_page + 1) as u16);
        put_u16(bytes, 0x38, 0x00FF);
        put_u16(bytes, 0x3A, variable_page as u16);
        fresh = true;
    }
    let mut count =
        u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
    let mut data_end = if count == 0 {
        0x0C
    } else {
        boundary(bytes, page_start, page_size, count)
            .ok_or(FormatError::InvalidRecordAddress(variable_page as u32))?
            & 0x7FFF
    } as usize;
    let mut capacity = page_size
        .saturating_sub(2 * (count + 2))
        .saturating_sub(data_end);
    if required > capacity {
        variable_page = file.fcr.total_pages as usize;
        page_start = variable_page * page_size;
        if page_start + page_size > bytes.len() {
            bytes.resize(page_start + page_size, 0);
        }
        bytes[page_start..page_start + page_size].fill(0);
        put_u16(bytes, page_start, 0);
        put_u16(bytes, page_start + 2, variable_page as u16);
        put_u16(bytes, page_start + 6, 0x00FF);
        put_u16(bytes, page_start + 8, 0xFFFF);
        put_u16(bytes, 0x28, (variable_page + 1) as u16);
        put_u16(bytes, 0x38, 0x00FF);
        put_u16(bytes, 0x3A, variable_page as u16);
        fresh = true;
        count = 0;
        data_end = 0x0C;
        capacity = page_size.saturating_sub(4).saturating_sub(data_end);
    }
    if required > capacity {
        return Ok(false);
    }
    let start = data_end;
    let end = start + required;
    let data_start = if continuation {
        put_variable_pointer(bytes, page_start + start, old_head);
        start + 4
    } else {
        start
    };
    bytes[page_start + data_start..page_start + end]
        .copy_from_slice(&new_record[logical..logical + new_head_data]);
    set_boundary(
        bytes,
        page_start,
        page_size,
        count,
        if continuation {
            0x8000 | start as u16
        } else {
            start as u16
        },
    )
    .ok_or(FormatError::InvalidRecordAddress(variable_page as u32))?;
    set_boundary(bytes, page_start, page_size, count + 1, end as u16)
        .ok_or(FormatError::InvalidRecordAddress(variable_page as u32))?;
    put_u16(bytes, page_start + 0x0A, (count + 1) as u16);
    if !fresh {
        put_u16(bytes, page_start + 0x04, aux.wrapping_sub(1));
    }
    put_variable_pointer(
        bytes,
        slot + logical,
        ((variable_page as u32) << 16) | count as u32,
    );
    if continuation {
        let mut cursor = logical + new_head_data;
        for (page, _, chunk_start, chunk_end, _) in chunks {
            let take = chunk_end - chunk_start;
            let page_base = page * page_size;
            bytes[page_base + chunk_start..page_base + chunk_start + take]
                .copy_from_slice(&new_record[cursor..cursor + take]);
            cursor += take;
            put_u16(bytes, page_base + 0x04, aux.wrapping_sub(1));
        }
    }
    put_u16(bytes, 0x04, aux);
    let data_page = slot / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | aux.wrapping_sub(1),
    );
    Ok(true)
}

/// Rewrite a variable-length record whose total length is unchanged.  The
/// existing chunk chain and pointer stay in place; only the fixed bytes, each
/// chunk's payload, the data-page aux, and the variable-page aux change.
/// Shrink a variable record by keeping its final chunk and freeing every
/// earlier chunk.  This is the layout the DOS engine produces when the new
/// variable portion still fits in the last chunk.
fn shrink_variable_record_in_place(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    new_record: &[u8],
    old_variable_length: usize,
    aux: u16,
) -> Result<bool> {
    let page_size = file.page_size.bytes();
    let logical = file.fcr.logical_record_length as usize;
    let slot = address as usize;
    if slot + logical > bytes.len() || new_record.len() < logical {
        return Ok(false);
    }
    let new_variable_length = new_record.len() - logical;
    if new_variable_length == 0 || new_variable_length >= old_variable_length {
        return Ok(false);
    }
    let Some(mut pointer) = read_pointer(bytes, slot + logical) else {
        return Ok(false);
    };
    let mut chunks = Vec::new();
    let mut total = 0usize;
    for _ in 0..4096 {
        if pointer == 0xFFFF_FFFF {
            break;
        }
        let page = (pointer >> 16) as usize;
        let chunk_slot = (pointer & 0xFFFF) as usize;
        let page_start = page * page_size;
        if page == 0 || page_start + page_size > bytes.len() {
            return Ok(false);
        }
        let count =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        if chunk_slot >= count {
            return Ok(false);
        }
        let Some(start_boundary) = boundary(bytes, page_start, page_size, chunk_slot) else {
            return Ok(false);
        };
        if start_boundary == 0xFFFF {
            return Ok(false);
        }
        let mut end_value = None;
        for index in chunk_slot + 1..=count {
            let Some(value) = boundary(bytes, page_start, page_size, index) else {
                return Ok(false);
            };
            if value != 0xFFFF {
                end_value = Some(value & 0x7FFF);
                break;
            }
        }
        let Some(end) = end_value else {
            return Ok(false);
        };
        let chunk_start = (start_boundary & 0x7FFF) as usize;
        let data_start = chunk_start + if start_boundary & 0x8000 != 0 { 4 } else { 0 };
        if data_start > end as usize || end as usize > page_size {
            return Ok(false);
        }
        total += end as usize - data_start;
        chunks.push((page, chunk_slot, data_start, end as usize));
        pointer = if start_boundary & 0x8000 != 0 {
            read_pointer(bytes, page_start + chunk_start).unwrap_or(0xFFFF_FFFF)
        } else {
            0xFFFF_FFFF
        };
    }
    if total != old_variable_length || chunks.is_empty() {
        return Ok(false);
    }
    let (tail_page, tail_slot, _, _) = *chunks.last().expect("non-empty");
    let earlier = chunks[..chunks.len() - 1].to_vec();
    for (page, chunk_slot, _, _) in earlier {
        free_variable_slot(bytes, page, chunk_slot, page_size, aux.wrapping_sub(1))?;
    }
    let tail_page_start = tail_page * page_size;
    let tail_count =
        u16::from_le_bytes([bytes[tail_page_start + 0x0A], bytes[tail_page_start + 0x0B]]) as usize;
    if tail_slot >= tail_count {
        return Ok(false);
    }
    let Some(start_boundary) = boundary(bytes, tail_page_start, page_size, tail_slot) else {
        return Ok(false);
    };
    if start_boundary == 0xFFFF || start_boundary & 0x8000 != 0 {
        return Ok(false);
    }
    let mut end_value = None;
    for index in tail_slot + 1..=tail_count {
        let Some(value) = boundary(bytes, tail_page_start, page_size, index) else {
            return Ok(false);
        };
        if value != 0xFFFF {
            end_value = Some(value & 0x7FFF);
            break;
        }
    }
    let Some(tail_end) = end_value else {
        return Ok(false);
    };
    let tail_start = (start_boundary & 0x7FFF) as usize;
    let tail_end = tail_end as usize;
    let new_end = tail_start + new_variable_length;
    if new_end > tail_end {
        return Ok(false);
    }
    let mut page_end = None;
    for index in (0..=tail_count).rev() {
        let Some(value) = boundary(bytes, tail_page_start, page_size, index) else {
            return Ok(false);
        };
        if value != 0xFFFF {
            page_end = Some(value & 0x7FFF);
            break;
        }
    }
    let page_end = page_end.unwrap_or(tail_end as u16) as usize;
    let removed = tail_end - new_end;
    bytes[slot..slot + logical].copy_from_slice(&new_record[..logical]);
    bytes[tail_page_start + tail_start..tail_page_start + new_end]
        .copy_from_slice(&new_record[logical..]);
    if page_end > tail_end {
        bytes.copy_within(
            tail_page_start + tail_end..tail_page_start + page_end,
            tail_page_start + new_end,
        );
    }
    if page_end >= removed {
        bytes[tail_page_start + page_end - removed..tail_page_start + page_end].fill(0);
    }
    set_boundary(
        bytes,
        tail_page_start,
        page_size,
        tail_slot + 1,
        new_end as u16,
    )
    .ok_or(FormatError::InvalidRecordAddress(tail_page as u32))?;
    for index in tail_slot + 2..=tail_count {
        let Some(value) = boundary(bytes, tail_page_start, page_size, index) else {
            return Ok(false);
        };
        if value != 0xFFFF {
            set_boundary(
                bytes,
                tail_page_start,
                page_size,
                index,
                value.wrapping_sub(removed as u16),
            )
            .ok_or(FormatError::InvalidRecordAddress(tail_page as u32))?;
        }
    }
    let was_full =
        u16::from_le_bytes([bytes[tail_page_start + 0x06], bytes[tail_page_start + 0x07]])
            == 0xFFFF;
    put_u16(bytes, tail_page_start + 0x04, aux.wrapping_sub(1));
    if was_full {
        put_u16(bytes, tail_page_start + 0x06, 0);
        put_u16(bytes, tail_page_start + 0x08, 4);
    }
    put_variable_pointer(
        bytes,
        slot + logical,
        ((tail_page as u32) << 16) | tail_slot as u32,
    );
    put_u16(bytes, 0x3A, tail_page as u16);
    put_u16(bytes, 0x04, aux);
    let data_page = slot / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | aux.wrapping_sub(1),
    );
    Ok(true)
}

fn update_variable_record_in_place(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    new_record: &[u8],
    aux: u16,
) -> Result<bool> {
    let page_size = file.page_size.bytes();
    let logical = file.fcr.logical_record_length as usize;
    let slot = address as usize;
    if slot + logical > bytes.len() || new_record.len() < logical {
        return Ok(false);
    }
    let variable_length = new_record.len() - logical;
    let Some(mut pointer) = read_pointer(bytes, slot + logical) else {
        return Ok(false);
    };
    if pointer == 0xFFFF_FFFF {
        if variable_length != 0 {
            return Ok(false);
        }
        bytes[slot..slot + logical].copy_from_slice(&new_record[..logical]);
        put_u16(bytes, 0x04, aux);
        let data_page = slot / page_size;
        put_u16(
            bytes,
            data_page * page_size + 0x04,
            DATA_PAGE_FLAG | aux.wrapping_sub(1),
        );
        return Ok(true);
    }
    let mut chunks = Vec::new();
    let mut total = 0usize;
    for _ in 0..4096 {
        if pointer == 0xFFFF_FFFF {
            break;
        }
        let page = (pointer >> 16) as usize;
        let chunk_slot = (pointer & 0xFFFF) as usize;
        let page_start = page * page_size;
        if page == 0 || page_start + page_size > bytes.len() {
            return Ok(false);
        }
        let count =
            u16::from_le_bytes([bytes[page_start + 0x0A], bytes[page_start + 0x0B]]) as usize;
        if chunk_slot >= count {
            return Ok(false);
        }
        let Some(start_boundary) = boundary(bytes, page_start, page_size, chunk_slot) else {
            return Ok(false);
        };
        if start_boundary == 0xFFFF {
            return Ok(false);
        }
        let mut end_value = None;
        for index in chunk_slot + 1..=count {
            let Some(value) = boundary(bytes, page_start, page_size, index) else {
                return Ok(false);
            };
            if value != 0xFFFF {
                end_value = Some(value & 0x7FFF);
                break;
            }
        }
        let Some(end) = end_value else {
            return Ok(false);
        };
        let chunk_start = (start_boundary & 0x7FFF) as usize;
        let data_start = chunk_start + if start_boundary & 0x8000 != 0 { 4 } else { 0 };
        if data_start > end as usize || end as usize > page_size {
            return Ok(false);
        }
        total += end as usize - data_start;
        chunks.push((page, data_start, end as usize));
        pointer = if start_boundary & 0x8000 != 0 {
            read_pointer(bytes, page_start + chunk_start).unwrap_or(0xFFFF_FFFF)
        } else {
            0xFFFF_FFFF
        };
    }
    if total != variable_length {
        return Ok(false);
    }
    bytes[slot..slot + logical].copy_from_slice(&new_record[..logical]);
    let mut cursor = logical;
    for (page, start, end) in chunks {
        let take = end - start;
        bytes[page * page_size + start..page * page_size + end]
            .copy_from_slice(&new_record[cursor..cursor + take]);
        cursor += take;
        put_u16(bytes, page * page_size + 0x04, aux.wrapping_sub(1));
    }
    put_u16(bytes, 0x04, aux);
    let data_page = slot / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | aux.wrapping_sub(1),
    );
    Ok(true)
}

/// Rewrite one fixed record and bump the operation counter without touching
/// any index (all key values are unchanged).
fn update_fixed_data_only(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    new_record: &[u8],
) -> Result<()> {
    let page_size = file.page_size.bytes();
    let slot = address as usize;
    let logical = file.fcr.logical_record_length as usize;
    if slot + logical > bytes.len() {
        return Err(FormatError::InvalidRecordAddress(address));
    }
    // Leave any variable-length pointer (or duplicate tail) untouched.
    bytes[slot..slot + logical].copy_from_slice(new_record);
    let aux = file.fcr.aux0.wrapping_add(2);
    put_u16(bytes, 0x04, aux);
    let data_page = slot / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | aux.wrapping_sub(1),
    );
    Ok(())
}

fn update_fixed_in_place(
    bytes: &mut [u8],
    file: &BtrieveFile,
    address: u32,
    new_record: &[u8],
    spec: &FileSpec,
) -> Result<()> {
    let pages =
        fixed_key_pages(file, spec).ok_or(FormatError::InvalidSpec("unexpected index layout"))?;
    let page_size = file.page_size.bytes();
    let slot = address as usize;
    let logical = file.fcr.logical_record_length as usize;
    if slot + logical > bytes.len() {
        return Err(FormatError::InvalidRecordAddress(address));
    }
    let old_record = bytes[slot..slot + logical].to_vec();
    let mut updates = Vec::new();
    let mut changed_indices = Vec::new();
    for (index, key) in spec.keys.iter().enumerate() {
        let old_value = key.build_key(&old_record)?;
        let new_value = key.build_key(new_record)?;
        if key.compare(&old_value, &new_value, spec.acs.as_ref()) == Ordering::Equal {
            continue;
        }
        if !key.segments.iter().any(KeySegment::is_modifiable) {
            return Err(FormatError::InvalidSpec("key is not modifiable"));
        }
        changed_indices.push(index);
        let page = pages[index];
        let Some(position) = page
            .entries
            .iter()
            .position(|entry| entry.record_address == address)
        else {
            return Err(FormatError::InvalidRecordAddress(address));
        };
        let mut entries: Vec<(Vec<u8>, u32)> = page
            .entries
            .iter()
            .enumerate()
            .map(|(slot, entry)| {
                (
                    if slot == position {
                        new_value.clone()
                    } else {
                        entry.key.clone()
                    },
                    entry.record_address,
                )
            })
            .collect();
        entries.sort_by(|left, right| key.compare(&left.0, &right.0, spec.acs.as_ref()));
        if entries.windows(2).any(|window| {
            key.compare(&window[0].0, &window[1].0, spec.acs.as_ref()) == Ordering::Equal
        }) {
            // The engine advances the FCR counter once and stamps the aux of
            // every index page it had already considered for the update; no
            // record or entry bytes change.
            let failed_aux = file.fcr.aux0.wrapping_add(1);
            put_u16(bytes, 0x04, failed_aux);
            for changed in &changed_indices {
                let page_start = pages[*changed].page_number as usize * page_size;
                put_u16(bytes, page_start + 0x04, failed_aux);
            }
            return Err(FormatError::DuplicateKey);
        }
        updates.push((index, entries));
    }

    bytes[slot..slot + logical].copy_from_slice(new_record);
    let aux = file.fcr.aux0.wrapping_add(2);
    put_u16(bytes, 0x04, aux);
    let data_page = slot / page_size;
    put_u16(
        bytes,
        data_page * page_size + 0x04,
        DATA_PAGE_FLAG | aux.wrapping_sub(1),
    );
    for (index, entries) in updates {
        store_key_entries(
            bytes,
            pages[index],
            &spec.keys[index],
            &entries,
            aux.wrapping_sub(1),
            key_def_offset(file, index),
            file.fcr.key_defs[index].entry_count,
            false,
        );
    }
    Ok(())
}

/// Image of an index page after removing the entry for `address` from one
/// key tree.  The DOS engine journals this intermediate image while moving a
/// modifiable key.
pub fn removed_entry_page_image(
    bytes: &[u8],
    file: &BtrieveFile,
    key_index: usize,
    address: u32,
    aux: u16,
) -> Option<Vec<u8>> {
    let page_size = file.page_size.bytes();
    let page = file
        .index_pages
        .iter()
        .find(|page| page.key_index == key_index)?;
    let position = page
        .entries
        .iter()
        .position(|entry| entry.record_address == address)?;
    let entries: Vec<(Vec<u8>, u32)> = page
        .entries
        .iter()
        .enumerate()
        .filter(|(slot, _)| *slot != position)
        .map(|(_, entry)| (entry.key.clone(), entry.record_address))
        .collect();
    let key = FileSpec::from_file(file).ok()?.keys.get(key_index)?.clone();
    let mut copy = bytes.to_vec();
    let entries_count = file
        .fcr
        .key_defs
        .get(key_index)
        .map(|definition| definition.entry_count.saturating_sub(1))
        .unwrap_or(0);
    store_key_entries(
        &mut copy,
        page,
        &key,
        &entries,
        aux,
        key_def_offset(file, key_index),
        entries_count,
        true,
    );
    let start = page.page_number as usize * page_size;
    copy.get(start..start + page_size)
        .map(|image| image.to_vec())
}

fn check_search_key(spec: &FileSpec, key_length: usize) -> Result<()> {
    let key = spec
        .keys
        .first()
        .ok_or(FormatError::InvalidSpec("no key definitions"))?;
    if key.length as usize != key_length {
        return Err(FormatError::InvalidSpec(
            "search key length does not match key 0",
        ));
    }
    Ok(())
}
