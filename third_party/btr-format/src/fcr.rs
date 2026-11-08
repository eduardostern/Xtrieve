use std::cmp::Ordering;

use serde::Serialize;

use crate::error::{FormatError, Result};
use crate::key::{compare_composite, Acs, KeySegment};
use crate::page::{Page, PageSize, KEY_DEFS_OFFSET, KEY_DEF_SIZE};

/// File flags stored in the FCR at offset 0x106 (verified by differential
/// probes of the Create operation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct FileFlags(pub u16);

impl FileFlags {
    pub const VARIABLE_LENGTH: u16 = 0x0001;
    pub const BLANK_TRUNCATION: u16 = 0x0002;
    pub const PREALLOCATION: u16 = 0x0004;
    pub const COMPRESSION: u16 = 0x0008;
    pub const KEY_ONLY: u16 = 0x0010;

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn variable_length(self) -> bool {
        self.0 & Self::VARIABLE_LENGTH != 0
    }

    pub const fn blank_truncation(self) -> bool {
        self.0 & Self::BLANK_TRUNCATION != 0
    }

    pub const fn preallocation(self) -> bool {
        self.0 & Self::PREALLOCATION != 0
    }

    pub const fn compression(self) -> bool {
        self.0 & Self::COMPRESSION != 0
    }

    pub const fn key_only(self) -> bool {
        self.0 & Self::KEY_ONLY != 0
    }

    /// Bits 6-7 encode the variable-page free-space threshold.
    pub const fn free_space_threshold_percent(self) -> u8 {
        match (self.0 >> 6) & 0x03 {
            0 => 0,
            1 => 10,
            2 => 20,
            _ => 30,
        }
    }
}

/// Owner-encryption mask byte for a page-relative `offset`.
///
/// Engine-created encrypted files reveal two schemes, selected by the parity
/// of the ninth hash byte:
///
/// * odd: the page is divided into four equal quarters; quarter `q` XORs with
///   hash bytes `2q` and `2q+1` (a different pair per quarter).
/// * even: the eight hash bytes 0-7 repeat every eight bytes.
///
/// Verified for 512- and 1024-byte pages with both parities.
pub const fn owner_mask_byte(hash: &[u8; 9], offset: usize, page_size: usize) -> u8 {
    if hash[8] & 1 != 0 {
        let quarter = page_size / 4;
        let pair = (offset / quarter) & 3;
        hash[pair * 2 + (offset & 1)]
    } else {
        hash[offset & 7]
    }
}

/// Key definition stored in the FCR key table at 0x110.  The table contains
/// one 30-byte block **per key segment**: a logical key whose first block has
/// the segmented flag (0x0010) set continues with the next block.  The first
/// block carries the aggregate fields (total length, entry size, root page);
/// every block carries its own position, length, type, and null value.
#[derive(Debug, Clone, Serialize)]
pub struct KeyDef {
    /// Zero-based logical key/index number.
    pub index: u16,
    /// Page number of this key's root index page (field at FCR+0x02).
    pub index_page: u16,
    /// Number of unique index entries stored for this key (+0x06).
    pub entry_count: u16,
    /// Raw flags of the first segment (the Stat operation additionally sets
    /// bit 0x0100 for extended key types).
    pub flags: u16,
    /// Total key length, the sum of all segment lengths.
    pub length: u16,
    /// Size of one index entry: key length + 8 (unique) or + 12 (duplicates).
    pub index_entry_length: u16,
    pub keys_per_page: u16,
    pub min_keys_per_page: u16,
    /// Field at FCR key-table +0x12. Zero for unique keys; equal to the
    /// logical record length for duplicate keys.
    pub duplicate_aux: u16,
    /// 1-based position of the first segment within the record.
    pub position: u16,
    /// Length of the first segment (field +0x16).
    pub length_copy: u16,
    /// Extended key type code of the first segment.
    pub key_type: u8,
    pub null_value: u8,
    /// Every segment of this logical key, in key order.
    pub segments: Vec<KeySegment>,
}

impl KeyDef {
    /// Parse one logical key starting at `offset`; returns the definition and
    /// the number of 30-byte blocks consumed.
    pub fn parse(page: &Page<'_>, offset: usize, index: u16) -> Result<(Self, usize)> {
        let first = parse_segment(page, offset)?;
        let mut segments = vec![first];
        let mut blocks = 1usize;
        let mut cursor = offset;
        while segments
            .last()
            .map(|segment| segment.is_segmented())
            .unwrap_or(false)
        {
            if blocks >= 24 {
                return Err(FormatError::Unsupported("more than 24 key segments"));
            }
            cursor += KEY_DEF_SIZE;
            if cursor + KEY_DEF_SIZE > page.data.len() {
                return Err(FormatError::OutOfBounds {
                    offset: cursor,
                    size: KEY_DEF_SIZE,
                    page_size: page.data.len(),
                });
            }
            segments.push(parse_segment(page, cursor)?);
            blocks += 1;
        }
        let first = &segments[0];
        Ok((
            Self {
                index,
                index_page: page.u16_at(offset + 0x02)?,
                entry_count: page.u16_at(offset + 0x06)?,
                flags: first.flags,
                length: page.u16_at(offset + 0x0A)?,
                index_entry_length: page.u16_at(offset + 0x0C)?,
                keys_per_page: page.u16_at(offset + 0x0E)?,
                min_keys_per_page: page.u16_at(offset + 0x10)?,
                duplicate_aux: page.u16_at(offset + 0x12)?,
                position: first.position,
                length_copy: first.length,
                key_type: first.key_type,
                null_value: first.null_value,
                segments,
            },
            blocks,
        ))
    }

    pub const fn has_duplicates(&self) -> bool {
        self.flags & 0x0001 != 0
    }

    pub const fn is_modifiable(&self) -> bool {
        self.flags & 0x0002 != 0
    }

    pub const fn is_binary(&self) -> bool {
        self.flags & 0x0004 != 0
    }

    pub fn has_null(&self) -> bool {
        self.segments.iter().any(KeySegment::has_null)
    }

    pub fn is_segmented(&self) -> bool {
        self.segments.len() > 1 || self.flags & 0x0010 != 0
    }

    pub fn has_acs(&self) -> bool {
        self.segments.iter().any(KeySegment::has_acs)
    }

    pub fn is_descending(&self) -> bool {
        self.segments.iter().any(KeySegment::is_descending)
    }

    pub const fn is_supplemental(&self) -> bool {
        self.flags & 0x0080 != 0
    }

    pub fn is_extended_type(&self) -> bool {
        self.segments.iter().any(KeySegment::is_extended_type)
    }

    pub fn is_manual(&self) -> bool {
        self.segments.iter().any(KeySegment::is_manual)
    }

    /// The Stat operation reports extended key types with bit 0x0100 set even
    /// when the file's key flags omit it (observed for string keys).
    pub const fn stat_flags(&self) -> u16 {
        self.flags | 0x0100
    }

    pub const fn entry_size(&self) -> usize {
        self.index_entry_length as usize
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Build the composite key bytes (segment values concatenated in key
    /// order) from a record.
    pub fn build_key(&self, record: &[u8]) -> Result<Vec<u8>> {
        let mut key = Vec::with_capacity(self.length as usize);
        for segment in &self.segments {
            let bytes = segment.bytes(record).ok_or(FormatError::KeyOutOfRange {
                index: self.index as usize,
                position: segment.position,
                length: segment.length,
                record_length: record.len(),
            })?;
            key.extend_from_slice(bytes);
        }
        Ok(key)
    }

    /// A null key is excluded when every byte of every segment is the
    /// segment's null value.  A manual key is excluded when any one segment
    /// is all null; the manual attribute overrides null.
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

    /// Compare two composite key byte strings using this key's segment
    /// definitions and the optional alternate collating sequence.
    pub fn compare(&self, a: &[u8], b: &[u8], acs: Option<&Acs>) -> Ordering {
        compare_composite(a, b, &self.segments, acs)
    }

    pub const fn key_type_name(&self) -> &'static str {
        match self.key_type {
            0 => "string",
            1 => "integer",
            2 => "float",
            3 => "date",
            4 => "time",
            5 => "decimal",
            6 => "money",
            7 => "logical",
            8 => "numeric",
            9 => "bfloat",
            10 => "lstring",
            11 => "zstring",
            14 => "unsigned binary",
            15 => "autoincrement",
            _ => "unknown",
        }
    }
}

const EXTENSION_NAME_OFFSET: usize = 0x98;
const EXTENSION_NAME_END: usize = 0xD8;

fn parse_extension_name(page: &Page<'_>) -> Result<Option<String>> {
    let bytes = page.bytes(
        EXTENSION_NAME_OFFSET,
        EXTENSION_NAME_END - EXTENSION_NAME_OFFSET,
    )?;
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    if end == 0 {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    Ok((!text.is_empty()).then_some(text))
}

fn parse_segment(page: &Page<'_>, offset: usize) -> Result<KeySegment> {
    Ok(KeySegment {
        position: page.u16_at(offset + 0x14)?.saturating_add(1),
        length: page.u16_at(offset + 0x16)?,
        flags: page.u16_at(offset + 0x08)?,
        key_type: page.u8_at(offset + 0x1C)?,
        null_value: page.u8_at(offset + 0x1D)?,
    })
}

/// File Control Record (page 0).  Verified fields are documented in
/// `docs/btrieve-5.10a-file-format.md`; fields marked "aux" are preserved raw
/// until their meaning is confirmed.
#[derive(Debug, Clone, Serialize)]
pub struct Fcr {
    pub page_size: PageSize,
    pub key_count: u16,
    pub logical_record_length: u16,
    pub physical_record_length: u16,
    /// 32-bit count assembled from the high word at 0x1A and low word at 0x1C.
    pub record_count: u32,
    /// Data-page hint stored at 0x20 (the last/current data page in fixtures).
    pub last_data_page: u16,
    pub data_page_aux: u16,
    pub total_pages: u16,
    pub free_bytes_last_data_page: u16,
    pub file_flags: FileFlags,
    /// Minimum/fixed record length for compressed files (FCR +0x10E).
    pub compressed_record_length: u16,
    /// Nine-byte owner-name hash at FCR +0x2E..+0x36.
    pub owner_hash: [u8; 9],
    /// An owner name is set: the format word is 0xFD00.  (There is no
    /// separate presence byte; +0x36 is the last hash byte.)
    pub owner_present: bool,
    /// Owner access/encryption mode (FCR +0x37; bit 1 = encrypted).
    pub owner_mode: u8,
    /// Preallocated page count (FCR +0x48).
    pub preallocated_pages: u16,
    /// Extension file name at FCR +0x98 (DOS drive path, NUL terminated).
    pub extension_name: Option<String>,
    pub key_defs: Vec<KeyDef>,
    /// Unclassified FCR words kept for future analysis.
    pub aux0: u16,
    pub format_word: u16,
    pub free_links: [u16; 4],
    pub variable_meta: [u16; 2],
}

impl Fcr {
    pub fn parse(page: &Page<'_>) -> Result<Self> {
        let page_size = PageSize::new(page.u16_at(0x08)?)?;
        let file_flags_raw = page.u16_at(0x106)?;
        let raw_key_count = page.u16_at(0x14)?;
        // Key-only files store 0xFFFF here and have exactly one key.
        let key_count = if file_flags_raw & 0x0010 != 0 && raw_key_count == 0xFFFF {
            1
        } else {
            raw_key_count
        };
        if KEY_DEFS_OFFSET + key_count as usize * KEY_DEF_SIZE > page_size.bytes() {
            return Err(crate::error::FormatError::OutOfBounds {
                offset: KEY_DEFS_OFFSET,
                size: key_count as usize * KEY_DEF_SIZE,
                page_size: page_size.bytes(),
            });
        }

        // The table stores one block per segment; key_count is the number of
        // logical indexes.  Walk blocks until each logical key is complete.
        let mut key_defs = Vec::with_capacity(key_count as usize);
        let mut block_offset = KEY_DEFS_OFFSET;
        for index in 0..key_count as usize {
            if block_offset + KEY_DEF_SIZE > page_size.bytes() {
                return Err(crate::error::FormatError::OutOfBounds {
                    offset: block_offset,
                    size: KEY_DEF_SIZE,
                    page_size: page_size.bytes(),
                });
            }
            let (definition, blocks) = KeyDef::parse(page, block_offset, index as u16)?;
            block_offset += blocks * KEY_DEF_SIZE;
            key_defs.push(definition);
        }

        Ok(Self {
            page_size,
            key_count,
            logical_record_length: page.u16_at(0x16)?,
            physical_record_length: page.u16_at(0x18)?,
            record_count: ((page.u16_at(0x1A)? as u32) << 16) | page.u16_at(0x1C)? as u32,
            last_data_page: page.u16_at(0x20)?,
            data_page_aux: page.u16_at(0x24)?,
            total_pages: page.u16_at(0x28)?,
            free_bytes_last_data_page: page.u16_at(0x2A)?,
            file_flags: FileFlags(page.u16_at(0x106)?),
            compressed_record_length: page.u16_at(0x10E)?,
            owner_hash: page.bytes(0x2E, 9)?.try_into().expect("nine bytes"),
            owner_present: page.u16_at(0x06)? == 0xFD00,
            owner_mode: page.u8_at(0x37)?,
            preallocated_pages: page.u16_at(0x48)?,
            extension_name: parse_extension_name(page)?,
            key_defs,
            aux0: page.u16_at(0x04)?,
            format_word: page.u16_at(0x06)?,
            free_links: [
                page.u16_at(0x0C)?,
                page.u16_at(0x0E)?,
                page.u16_at(0x10)?,
                page.u16_at(0x12)?,
            ],
            variable_meta: [page.u16_at(0x38)?, page.u16_at(0x3A)?],
        })
    }

    pub fn duplicate_key_count(&self) -> usize {
        self.key_defs
            .iter()
            .filter(|key| key.has_duplicates())
            .count()
    }
}
