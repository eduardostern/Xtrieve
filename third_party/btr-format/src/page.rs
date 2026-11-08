use serde::Serialize;

use crate::error::{FormatError, Result};

/// Size of the page header shared by FCR, index, and data pages.
pub const PAGE_HEADER_SIZE: usize = 6;

/// Bit in the page-header flags word that marks a data page.
pub const DATA_PAGE_FLAG: u16 = 0x8000;

/// Offset of the key-definition table inside the FCR page (5.10a build).
pub const KEY_DEFS_OFFSET: usize = 0x110;

/// Size of one key-definition record in the FCR (5.10a build).
pub const KEY_DEF_SIZE: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PageSize(u16);

impl PageSize {
    pub fn new(value: u16) -> Result<Self> {
        match value {
            512 | 1024 | 2048 | 4096 => Ok(Self(value)),
            other => Err(FormatError::InvalidPageSize(other)),
        }
    }

    pub const fn as_u16(self) -> u16 {
        self.0
    }

    pub const fn bytes(self) -> usize {
        self.0 as usize
    }
}

impl std::fmt::Display for PageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct PageId(pub u32);

impl PageId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PageKind {
    Fcr,
    /// Alternate collating sequence definition page (always page 1 when an
    /// ACS is present).
    Acs,
    Index,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PageHeader {
    /// Previous page in a physical page chain, or 0.
    pub previous: u16,
    /// Page number stored in the page header.
    pub number: u16,
    /// Bit 15 marks a data page.  The remaining bits are page-specific.
    pub flags: u16,
}

impl PageHeader {
    pub fn parse(page: &[u8]) -> Result<Self> {
        if page.len() < PAGE_HEADER_SIZE {
            return Err(FormatError::TooSmall);
        }
        Ok(Self {
            previous: u16::from_le_bytes([page[0], page[1]]),
            number: u16::from_le_bytes([page[2], page[3]]),
            flags: u16::from_le_bytes([page[4], page[5]]),
        })
    }

    pub fn kind(&self, page_index: u32) -> PageKind {
        if page_index == 0 {
            PageKind::Fcr
        } else if self.flags & DATA_PAGE_FLAG != 0 {
            PageKind::Data
        } else {
            PageKind::Index
        }
    }

    /// Page-specific bits with the data-page marker removed.
    pub fn aux(&self) -> u16 {
        self.flags & !DATA_PAGE_FLAG
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Page<'a> {
    pub id: PageId,
    pub data: &'a [u8],
}

impl<'a> Page<'a> {
    pub fn header(&self) -> Result<PageHeader> {
        PageHeader::parse(self.data)
    }

    pub fn kind(&self) -> Result<PageKind> {
        Ok(self.header()?.kind(self.id.0))
    }

    fn out_of_bounds(&self, offset: usize, size: usize) -> FormatError {
        FormatError::OutOfBounds {
            offset,
            size,
            page_size: self.data.len(),
        }
    }

    pub fn u8_at(&self, offset: usize) -> Result<u8> {
        self.data
            .get(offset)
            .copied()
            .ok_or_else(|| self.out_of_bounds(offset, 1))
    }

    pub fn u16_at(&self, offset: usize) -> Result<u16> {
        let bytes = self
            .data
            .get(offset..offset + 2)
            .ok_or_else(|| self.out_of_bounds(offset, 2))?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn u32_at(&self, offset: usize) -> Result<u32> {
        let bytes = self
            .data
            .get(offset..offset + 4)
            .ok_or_else(|| self.out_of_bounds(offset, 4))?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn bytes(&self, offset: usize, size: usize) -> Result<&'a [u8]> {
        self.data
            .get(offset..offset + size)
            .ok_or_else(|| self.out_of_bounds(offset, size))
    }

    /// Read a Btrieve record address.  The on-disk format stores the high
    /// 16-bit word before the low 16-bit word, unlike ordinary little-endian
    /// integers.
    pub fn record_address_at(&self, offset: usize) -> Result<u32> {
        let high = self.u16_at(offset)? as u32;
        let low = self.u16_at(offset + 2)? as u32;
        Ok((high << 16) | low)
    }
}
