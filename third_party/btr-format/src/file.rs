use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::data::{extract_record, Record};
use crate::error::{FormatError, Result};
use crate::fcr::{Fcr, FileFlags, KeyDef};
use crate::index::IndexPage;
use crate::key::{Acs, KeySegment};
use crate::page::{Page, PageHeader, PageId, PageKind, PageSize, PAGE_HEADER_SIZE};

#[derive(Debug, Clone, Serialize)]
pub struct PageInfo {
    pub id: u32,
    pub previous: u16,
    pub number: u16,
    pub flags: u16,
    pub kind: PageKind,
    pub aux: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyStat {
    pub key_number: u16,
    pub position: u16,
    pub length: u16,
    pub flags: u16,
    /// Flags as the Stat operation reports them (bit 0x0100 set).
    pub stat_flags: u16,
    pub key_type: u8,
    pub null_value: u8,
    pub has_duplicates: bool,
    pub segment_count: usize,
    pub segments: Vec<KeySegment>,
    pub entry_count: usize,
    pub unique_key_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stat {
    pub record_length: u16,
    pub page_size: u16,
    pub key_count: u16,
    pub record_count: u32,
    pub file_flags: u16,
    pub keys: Vec<KeyStat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BtrieveFile {
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub bytes: Vec<u8>,
    pub file_size: usize,
    pub page_size: PageSize,
    pub page_count: u32,
    pub fcr: Fcr,
    /// Alternate collating sequence definition, when any key segment uses one.
    pub acs: Option<Acs>,
    pub pages: Vec<PageInfo>,
    pub index_pages: Vec<IndexPage>,
    /// Unique data-record addresses collected from index entries, ascending.
    pub record_addresses: Vec<u32>,
    /// Records stored inline in the index pages of a key-only file.
    #[serde(skip)]
    pub inline_records: Vec<Record>,
}

impl BtrieveFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        Self::from_bytes(path, bytes)
    }

    pub fn from_bytes(path: &Path, bytes: Vec<u8>) -> Result<Self> {
        let mut bytes = bytes;
        if bytes.len() < 512 {
            return Err(FormatError::TooSmall);
        }
        let page_size_raw = u16::from_le_bytes([bytes[8], bytes[9]]);
        let page_size = PageSize::new(page_size_raw)?;
        let page_size_bytes = page_size.bytes();
        if bytes.len() % page_size_bytes != 0 {
            return Err(FormatError::InvalidFileSize {
                size: bytes.len(),
                page_size: page_size_bytes,
            });
        }
        let page_count = (bytes.len() / page_size_bytes) as u32;
        if page_count == 0 {
            return Err(FormatError::TooSmall);
        }

        let page0 = Page {
            id: PageId(0),
            data: &bytes[..page_size_bytes],
        };
        let fcr = Fcr::parse(&page0)?;
        if fcr.page_size != page_size {
            return Err(FormatError::InvalidPageSize(fcr.page_size.as_u16()));
        }
        if fcr.owner_present && fcr.owner_mode & 0x02 != 0 {
            for page in 1..page_count as usize {
                let start = page * page_size_bytes;
                for offset in 0..page_size_bytes {
                    bytes[start + offset] ^=
                        crate::fcr::owner_mask_byte(&fcr.owner_hash, offset, page_size_bytes);
                }
            }
        }

        let has_acs = fcr.key_defs.iter().any(KeyDef::has_acs);
        let acs = if has_acs && page_count > 1 {
            let start = page_size_bytes + PAGE_HEADER_SIZE;
            bytes
                .get(start..start + Acs::SIZE)
                .and_then(Acs::from_bytes)
        } else {
            None
        };

        let mut pages = Vec::with_capacity(page_count as usize);
        for index in 0..page_count {
            let start = index as usize * page_size_bytes;
            let page = Page {
                id: PageId(index),
                data: &bytes[start..start + page_size_bytes],
            };
            let header = page.header()?;
            pages.push(PageInfo {
                id: index,
                previous: header.previous,
                number: header.number,
                flags: header.flags,
                kind: header.kind(index),
                aux: header.aux(),
            });
        }
        if acs.is_some() {
            pages[1].kind = PageKind::Acs;
        }

        let physical = fcr.physical_record_length;
        let key_only = fcr.file_flags.key_only();
        let mut index_pages = Vec::new();
        let mut addresses: BTreeSet<u32> = BTreeSet::new();

        // Follow each key's root page and its child links.  Index pages do
        // not carry a key number, so reachability from the key definition is
        // the only unambiguous assignment (especially for supplemental
        // indexes).
        for (key_index, key) in fcr.key_defs.iter().enumerate() {
            if key.length == 0 || key.index_page == 0 {
                continue;
            }
            let mut queue = VecDeque::new();
            let mut visited: BTreeSet<u32> = BTreeSet::new();
            queue.push_back(key.index_page as u32);
            while let Some(page_number) = queue.pop_front() {
                if !visited.insert(page_number) || page_number == 0 || page_number >= page_count {
                    continue;
                }
                if pages[page_number as usize].kind != PageKind::Index {
                    continue;
                }
                let start = page_number as usize * page_size_bytes;
                let page = Page {
                    id: PageId(page_number),
                    data: &bytes[start..start + page_size_bytes],
                };
                let parsed = match IndexPage::parse(
                    &page,
                    key,
                    key_index,
                    key_only,
                    fcr.logical_record_length as usize,
                ) {
                    Ok(parsed) => parsed,
                    Err(_) => continue,
                };
                let mut children = Vec::new();
                for link in [parsed.link_a, parsed.link_b] {
                    if let Some(child) = child_page(link) {
                        children.push(child);
                    }
                }
                for entry in &parsed.entries {
                    if let Some(child) = child_page(entry.next_link) {
                        children.push(child);
                    }
                }
                if !key_only {
                    for entry in &parsed.entries {
                        if address_is_data_record(
                            &bytes,
                            page_size,
                            page_count,
                            physical,
                            entry.record_address,
                        ) {
                            addresses.insert(entry.record_address);
                        }
                    }
                }
                index_pages.push(parsed);
                for child in children {
                    if !visited.contains(&child) {
                        queue.push_back(child);
                    }
                }
            }
        }

        // Index entries omit null/manual-key records, so collect records from
        // the data pages themselves.  Every data page except the last is full;
        // the last page's fill is recorded in the FCR free-byte count.
        let mut freed: BTreeSet<u32> = BTreeSet::new();
        let mut free = ((fcr.free_links[2] as u32) << 16) | fcr.free_links[3] as u32;
        let mut guard = 0usize;
        while free != 0 && free != 0xFFFF_FFFF && guard < 1_000_000 {
            guard += 1;
            if !freed.insert(free) {
                break;
            }
            let page_index = free / page_size_bytes as u32;
            let offset = (free % page_size_bytes as u32) as usize;
            if page_index == 0 || page_index >= page_count {
                break;
            }
            let slot = page_index as usize * page_size_bytes + offset;
            if slot + 4 > bytes.len() {
                break;
            }
            free = ((u16::from_le_bytes([bytes[slot], bytes[slot + 1]]) as u32) << 16)
                | u16::from_le_bytes([bytes[slot + 2], bytes[slot + 3]]) as u32;
        }

        let mut data_addresses = Vec::new();
        if physical > 0 {
            let capacity = (page_size_bytes - PAGE_HEADER_SIZE) / physical as usize;
            let last_data = pages.iter().rposition(|info| info.kind == PageKind::Data);
            for (index, info) in pages.iter().enumerate().skip(1) {
                if info.kind != PageKind::Data {
                    continue;
                }
                let mut count = capacity;
                if Some(index) == last_data {
                    let used = (page_size_bytes - PAGE_HEADER_SIZE)
                        .saturating_sub(fcr.free_bytes_last_data_page as usize);
                    count = (used / physical as usize).min(capacity);
                }
                for slot in 0..count {
                    let offset = PAGE_HEADER_SIZE + slot * physical as usize;
                    let address = index as u32 * page_size_bytes as u32 + offset as u32;
                    if !freed.contains(&address) {
                        data_addresses.push(address);
                    }
                }
            }
        }
        let mut record_addresses: BTreeSet<u32> = data_addresses.into_iter().collect();
        if record_addresses.len() < fcr.record_count as usize {
            // Fall back to index-derived addresses if the data-page walk could
            // not account for every record (for example a partially filled
            // non-final page).
            record_addresses.extend(addresses);
        }

        let mut inline_records = Vec::new();
        if fcr.file_flags.key_only() {
            for page in &index_pages {
                for entry in &page.entries {
                    if let Some(record) = &entry.record {
                        inline_records.push(Record {
                            address: entry.record_address,
                            page: 0,
                            offset: 0,
                            data: record.clone(),
                            trailing: Vec::new(),
                        });
                    }
                }
            }
            inline_records.sort_by_key(|record| record.address);
        }
        let record_addresses = if fcr.file_flags.key_only() {
            inline_records.iter().map(|record| record.address).collect()
        } else {
            record_addresses.into_iter().collect()
        };

        Ok(Self {
            path: path.to_path_buf(),
            bytes,
            file_size: pages.len() * page_size_bytes,
            page_size,
            page_count,
            fcr,
            acs,
            pages,
            index_pages,
            record_addresses,
            inline_records,
        })
    }

    pub fn page(&self, id: u32) -> Result<Page<'_>> {
        if id >= self.page_count {
            return Err(FormatError::InvalidRecordAddress(id));
        }
        let size = self.page_size.bytes();
        let start = id as usize * size;
        Ok(Page {
            id: PageId(id),
            data: &self.bytes[start..start + size],
        })
    }

    pub fn page_info(&self, id: u32) -> Option<&PageInfo> {
        self.pages.get(id as usize)
    }

    pub fn record_at(&self, address: u32) -> Result<Record> {
        if self.fcr.file_flags.key_only() {
            return self
                .inline_records
                .iter()
                .find(|record| record.address == address)
                .cloned()
                .ok_or(FormatError::InvalidRecordAddress(address));
        }
        extract_record(
            &self.bytes,
            self.page_size,
            self.page_count,
            &self.fcr,
            address,
        )
    }

    pub fn records(&self) -> Result<Vec<Record>> {
        if self.fcr.file_flags.key_only() {
            return Ok(self.inline_records.clone());
        }
        self.record_addresses
            .iter()
            .map(|&address| self.record_at(address))
            .collect()
    }

    pub fn stat(&self) -> Stat {
        let mut keys = Vec::with_capacity(self.fcr.key_defs.len());
        for (key_index, key) in self.fcr.key_defs.iter().enumerate() {
            let mut unique: BTreeSet<Vec<u8>> = BTreeSet::new();
            let mut entry_count = 0usize;
            for page in self
                .index_pages
                .iter()
                .filter(|page| page.key_index == key_index)
            {
                entry_count += page.entries.len();
                for entry in &page.entries {
                    unique.insert(entry.key.clone());
                }
            }
            let reported = if key.has_duplicates()
                && (key.is_supplemental() || self.fcr.file_flags.key_only())
            {
                unique.len()
            } else {
                entry_count
            };
            keys.push(key_stat(key, reported, unique.len()));
        }
        Stat {
            record_length: self.fcr.logical_record_length,
            page_size: self.page_size.as_u16(),
            key_count: self.fcr.key_count,
            record_count: self.fcr.record_count,
            file_flags: self.fcr.file_flags.raw(),
            keys,
        }
    }

    pub fn file_flags(&self) -> FileFlags {
        self.fcr.file_flags
    }

    pub fn keys(&self) -> &[KeyDef] {
        &self.fcr.key_defs
    }

    pub fn acs(&self) -> Option<&Acs> {
        self.acs.as_ref()
    }
}

fn key_stat(key: &KeyDef, entry_count: usize, unique_key_count: usize) -> KeyStat {
    KeyStat {
        key_number: key.index,
        position: key.position,
        length: key.length,
        flags: key.flags,
        stat_flags: key.stat_flags(),
        key_type: key.key_type,
        null_value: key.null_value,
        has_duplicates: key.has_duplicates(),
        segment_count: key.segment_count(),
        segments: key.segments.clone(),
        entry_count,
        unique_key_count,
    }
}

fn child_page(link: u32) -> Option<u32> {
    if link == 0 || link == 0xFFFF_FFFF {
        return None;
    }
    let page = link >> 16;
    (page != 0 && page != 0xFFFF).then_some(page)
}

fn address_is_data_record(
    bytes: &[u8],
    page_size: PageSize,
    page_count: u32,
    physical_record_length: u16,
    address: u32,
) -> bool {
    let page_size_u32 = page_size.bytes() as u32;
    if address < PAGE_HEADER_SIZE as u32 || physical_record_length == 0 {
        return false;
    }
    let page_index = address / page_size_u32;
    let offset = address % page_size_u32;
    if page_index == 0 || page_index >= page_count {
        return false;
    }
    if offset < PAGE_HEADER_SIZE as u32 {
        return false;
    }
    if offset + physical_record_length as u32 > page_size_u32 {
        return false;
    }
    let start = page_index as usize * page_size.bytes();
    let header = match PageHeader::parse(&bytes[start..start + page_size.bytes()]) {
        Ok(header) => header,
        Err(_) => return false,
    };
    header.kind(page_index) == PageKind::Data
}
