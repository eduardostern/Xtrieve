//! Page I/O and page structure definitions for Btrieve files
//!
//! Btrieve files are organized into fixed-size pages. The page size is
//! set at file creation and stored in the FCR (page 0).

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

/// Valid page sizes in Btrieve 5.1
pub const PAGE_SIZES: [u16; 4] = [512, 1024, 2048, 4096];

/// Minimum page size
pub const MIN_PAGE_SIZE: u16 = 512;

/// Maximum page size
pub const MAX_PAGE_SIZE: u16 = 4096;

/// Page type identifiers (stored in page header)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PageType {
    /// File Control Record - always page 0
    Fcr = 0x00,
    /// PAT (Page Allocation Table) - free page tracking
    Pat = 0x01,
    /// Data page - contains records
    Data = 0x02,
    /// Index page - B+ tree node
    Index = 0x03,
    /// Variable page - overflow for variable-length records
    Variable = 0x04,
    /// Unknown page type
    Unknown = 0xFF,
}

impl From<u8> for PageType {
    fn from(value: u8) -> Self {
        match value {
            0x00 => PageType::Fcr,
            0x01 => PageType::Pat,
            0x02 => PageType::Data,
            0x03 => PageType::Index,
            0x04 => PageType::Variable,
            _ => PageType::Unknown,
        }
    }
}

/// Common page header structure (first bytes of each page)
#[derive(Debug, Clone)]
pub struct PageHeader {
    /// Page type
    pub page_type: PageType,
    /// Usage count or flags
    pub usage: u16,
    /// Next page in chain (for data pages)
    pub next_page: u32,
    /// Previous page in chain (for data pages)
    pub prev_page: u32,
}

impl PageHeader {
    /// Size of the page header in bytes
    pub const SIZE: usize = 12;

    /// Read a page header from bytes
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < Self::SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Page header too short",
            ));
        }

        let mut cursor = Cursor::new(data);
        let page_type = PageType::from(cursor.read_u8()?);
        let _reserved = cursor.read_u8()?;
        let usage = cursor.read_u16::<LittleEndian>()?;
        let next_page = cursor.read_u32::<LittleEndian>()?;
        let prev_page = cursor.read_u32::<LittleEndian>()?;

        Ok(PageHeader {
            page_type,
            usage,
            next_page,
            prev_page,
        })
    }

    /// Write page header to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::SIZE);
        buf.write_u8(self.page_type as u8).unwrap();
        buf.write_u8(0).unwrap(); // reserved
        buf.write_u16::<LittleEndian>(self.usage).unwrap();
        buf.write_u32::<LittleEndian>(self.next_page).unwrap();
        buf.write_u32::<LittleEndian>(self.prev_page).unwrap();
        buf
    }
}

/// A page buffer containing raw page data
#[derive(Clone)]
pub struct Page {
    /// Page number (0-based)
    pub page_number: u32,
    /// Page size
    pub page_size: u16,
    /// Raw page data
    pub data: Vec<u8>,
    /// Whether the page has been modified
    pub dirty: bool,
}

impl Page {
    /// Create a new empty page
    pub fn new(page_number: u32, page_size: u16) -> Self {
        Page {
            page_number,
            page_size,
            data: vec![0; page_size as usize],
            dirty: false,
        }
    }

    /// Create a page from raw data
    pub fn from_data(page_number: u32, data: Vec<u8>) -> Self {
        let page_size = data.len() as u16;
        Page {
            page_number,
            page_size,
            data,
            dirty: false,
        }
    }

    /// Get the page header
    pub fn header(&self) -> io::Result<PageHeader> {
        PageHeader::from_bytes(&self.data)
    }

    /// Get the page type
    pub fn page_type(&self) -> PageType {
        if self.data.is_empty() {
            PageType::Unknown
        } else {
            PageType::from(self.data[0])
        }
    }

    /// Get page content after header
    pub fn content(&self) -> &[u8] {
        if self.data.len() > PageHeader::SIZE {
            &self.data[PageHeader::SIZE..]
        } else {
            &[]
        }
    }

    /// Get mutable page content after header
    pub fn content_mut(&mut self) -> &mut [u8] {
        if self.data.len() > PageHeader::SIZE {
            &mut self.data[PageHeader::SIZE..]
        } else {
            &mut []
        }
    }

    /// Mark page as dirty (modified)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("page_number", &self.page_number)
            .field("page_size", &self.page_size)
            .field("page_type", &self.page_type())
            .field("dirty", &self.dirty)
            .finish()
    }
}

/// Page I/O operations on a file
pub struct PageIO<F> {
    file: F,
    page_size: u16,
}

impl<F: Read + Write + Seek> PageIO<F> {
    /// Create a new PageIO wrapper
    pub fn new(file: F, page_size: u16) -> Self {
        PageIO { file, page_size }
    }

    /// Get the page size
    pub fn page_size(&self) -> u16 {
        self.page_size
    }

    /// Read a page from the file
    pub fn read_page(&mut self, page_number: u32) -> io::Result<Page> {
        let offset = (page_number as u64) * (self.page_size as u64);
        self.file.seek(SeekFrom::Start(offset))?;

        let mut data = vec![0u8; self.page_size as usize];
        self.file.read_exact(&mut data)?;

        Ok(Page::from_data(page_number, data))
    }

    /// Write a page to the file
    pub fn write_page(&mut self, page: &Page) -> io::Result<()> {
        let offset = (page.page_number as u64) * (self.page_size as u64);
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&page.data)?;
        Ok(())
    }

    /// Get the total number of pages in the file
    pub fn page_count(&mut self) -> io::Result<u32> {
        let end = self.file.seek(SeekFrom::End(0))?;
        Ok((end / self.page_size as u64) as u32)
    }

    /// Allocate a new page at the end of the file
    pub fn allocate_page(&mut self) -> io::Result<Page> {
        let page_number = self.page_count()?;
        let page = Page::new(page_number, self.page_size);
        self.write_page(&page)?;
        Ok(page)
    }

    /// Get mutable reference to underlying file
    pub fn file_mut(&mut self) -> &mut F {
        &mut self.file
    }

    /// Sync file to disk
    pub fn sync(&mut self) -> io::Result<()>
    where
        F: std::os::unix::io::AsRawFd,
    {
        // On Unix, we can use fsync via the raw fd
        // For now, just flush
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_page_header_roundtrip() {
        let header = PageHeader {
            page_type: PageType::Data,
            usage: 42,
            next_page: 100,
            prev_page: 50,
        };

        let bytes = header.to_bytes();
        let parsed = PageHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.page_type, PageType::Data);
        assert_eq!(parsed.usage, 42);
        assert_eq!(parsed.next_page, 100);
        assert_eq!(parsed.prev_page, 50);
    }

    #[test]
    fn test_page_io() {
        let mut buffer = vec![0u8; 4096];
        let cursor = Cursor::new(&mut buffer);
        let mut page_io = PageIO::new(cursor, 512);

        // Write a page
        let mut page = Page::new(0, 512);
        page.data[0] = PageType::Data as u8;
        page.data[100] = 0x42;
        page_io.write_page(&page).unwrap();

        // Read it back
        let read_page = page_io.read_page(0).unwrap();
        assert_eq!(read_page.page_type(), PageType::Data);
        assert_eq!(read_page.data[100], 0x42);
    }
}
