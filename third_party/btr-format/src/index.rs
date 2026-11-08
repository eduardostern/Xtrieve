use serde::Serialize;

use crate::error::{FormatError, Result};
use crate::fcr::KeyDef;
use crate::page::Page;

/// One entry in an index page.
///
/// Layout on disk (5.10a):
///   unique:     key[length], address(4), link(4)
///   duplicates: key[length], address(4), duplicate_address(4), link(4)
///
/// Addresses store the high 16-bit word before the low word.
#[derive(Debug, Clone, Serialize)]
pub struct IndexEntry {
    pub key: Vec<u8>,
    pub record_address: u32,
    pub duplicate_address: Option<u32>,
    pub next_link: u32,
    /// Key-only files store the full record inline in the index entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexPage {
    pub page_number: u32,
    pub count: u16,
    pub link_a: u32,
    pub link_b: u32,
    /// Zero-based index into the FCR key-definition table.
    pub key_index: usize,
    pub key_number: u16,
    pub entries: Vec<IndexEntry>,
}

impl IndexPage {
    pub fn parse(
        page: &Page<'_>,
        key: &KeyDef,
        key_index: usize,
        key_only: bool,
        record_length: usize,
    ) -> Result<Self> {
        if key.length == 0 {
            return Err(FormatError::Unsupported("zero-length key"));
        }
        let count = page.u16_at(0x06)?;
        let entry_size = key.entry_size();
        let end = 0x10usize
            .checked_add(count as usize * entry_size)
            .ok_or(FormatError::Unsupported("index page size overflow"))?;
        if end > page.data.len() {
            return Err(FormatError::OutOfBounds {
                offset: 0x10,
                size: count as usize * entry_size,
                page_size: page.data.len(),
            });
        }

        let mut entries = Vec::with_capacity(count as usize);
        for index in 0..count as usize {
            let offset = 0x10 + index * entry_size;
            if key_only {
                let record = page.bytes(offset, record_length)?.to_vec();
                let start = key.position as usize - 1;
                let key_bytes = record
                    .get(start..start + key.length as usize)
                    .ok_or(FormatError::KeyOutOfRange {
                        index: key_index,
                        position: key.position,
                        length: key.length,
                        record_length,
                    })?
                    .to_vec();
                let address_offset = offset + record_length;
                let serial = page.record_address_at(address_offset)?;
                let next_link = page.u32_at(address_offset + 4)?;
                entries.push(IndexEntry {
                    key: key_bytes,
                    record_address: serial,
                    duplicate_address: None,
                    next_link,
                    record: Some(record),
                });
                continue;
            }
            let key_bytes = page.bytes(offset, key.length as usize)?.to_vec();
            let address_offset = offset + key.length as usize;
            let record_address = page.record_address_at(address_offset)?;
            let has_duplicate_field = key.index_entry_length as usize == key.length as usize + 12;
            let (duplicate_address, next_link) = if has_duplicate_field {
                (
                    Some(page.record_address_at(address_offset + 4)?),
                    page.u32_at(address_offset + 8)?,
                )
            } else {
                (None, page.u32_at(address_offset + 4)?)
            };
            entries.push(IndexEntry {
                key: key_bytes,
                record_address,
                duplicate_address,
                next_link,
                record: None,
            });
        }

        Ok(Self {
            page_number: page.id.0,
            count,
            link_a: page.u32_at(0x08)?,
            link_b: page.u32_at(0x0C)?,
            key_index,
            key_number: key.index,
            entries,
        })
    }

    pub fn addresses(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries.iter().map(|entry| entry.record_address)
    }
}
