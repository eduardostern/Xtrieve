use serde::Serialize;

use crate::error::{FormatError, Result};
use crate::fcr::Fcr;
use crate::page::{PageHeader, PageKind, PageSize, PAGE_HEADER_SIZE};

#[derive(Debug, Clone, Serialize)]
pub struct Record {
    /// Byte offset of the record within the file.
    pub address: u32,
    pub page: u32,
    /// Offset within the page where the record starts.
    pub offset: u16,
    /// Logical record bytes (the fixed-length portion in this build).
    pub data: Vec<u8>,
    /// Bytes between the logical record and the next physical record slot
    /// (duplicate-key metadata, variable-length data, or zero padding).
    pub trailing: Vec<u8>,
}

/// Extract a fixed-layout record at a byte address.
pub fn extract_record(
    bytes: &[u8],
    page_size: PageSize,
    page_count: u32,
    fcr: &Fcr,
    address: u32,
) -> Result<Record> {
    if fcr.file_flags.key_only() {
        return Err(FormatError::Unsupported("key-only file records"));
    }
    let page_bytes = page_size.bytes() as u32;
    if address < PAGE_HEADER_SIZE as u32 {
        return Err(FormatError::RecordInHeader(address));
    }
    let page_index = address / page_bytes;
    let offset = (address % page_bytes) as usize;
    if page_index >= page_count {
        return Err(FormatError::InvalidRecordAddress(address));
    }

    let start = page_index as usize * page_bytes as usize;
    let page = &bytes[start..start + page_bytes as usize];
    let header = PageHeader::parse(page)?;
    let kind = header.kind(page_index);
    if kind != PageKind::Data {
        return Err(FormatError::NotDataPage {
            address,
            page: page_index,
            kind,
        });
    }
    if offset < PAGE_HEADER_SIZE {
        return Err(FormatError::RecordInHeader(address));
    }

    let physical = fcr.physical_record_length as usize;
    let logical = fcr.logical_record_length as usize;
    if physical == 0 || (!fcr.file_flags.compression() && logical == 0) {
        return Err(FormatError::Unsupported("zero record length"));
    }
    if offset + physical > page.len() {
        return Err(FormatError::InvalidRecordAddress(address));
    }

    let data = if fcr.file_flags.compression() {
        let tail = &page[offset..offset + physical];
        let pointer = variable_pointer(tail)?;
        let image = if pointer == 0xFFFF_FFFF {
            Vec::new()
        } else {
            read_variable_portion(bytes, page_size, page_count, pointer)?
        };
        decompress_record(&image)?
    } else {
        let mut data = page[offset..offset + logical].to_vec();
        if fcr.file_flags.variable_length() {
            let tail = &page[offset + logical..offset + physical];
            let pointer = variable_pointer(tail)?;
            let truncated = if fcr.file_flags.blank_truncation() {
                tail.get(4..6)
                    .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                    .unwrap_or(0)
            } else {
                0
            };
            if pointer != 0xFFFF_FFFF {
                let variable = read_variable_portion(bytes, page_size, page_count, pointer)?;
                data.extend_from_slice(&variable);
            }
            if truncated > 0 {
                data.extend(std::iter::repeat(b' ').take(truncated as usize));
            }
        }
        data
    };
    let trailing = page[offset + logical..offset + physical].to_vec();
    Ok(Record {
        address,
        page: page_index,
        offset: offset as u16,
        data,
        trailing,
    })
}

/// Expand a Btrieve compressed record image.  Blocks alternate between a
/// literal block (16-bit length followed by bytes) and a run block (16-bit
/// count followed by the repeated byte).
fn decompress_record(image: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut position = 0usize;
    while position < image.len() {
        if position + 2 > image.len() {
            return Err(FormatError::Unsupported(
                "truncated compression literal length",
            ));
        }
        let literal = u16::from_le_bytes([image[position], image[position + 1]]) as usize;
        position += 2;
        if position + literal > image.len() {
            return Err(FormatError::Unsupported("truncated compression literal"));
        }
        out.extend_from_slice(&image[position..position + literal]);
        position += literal;
        if position >= image.len() {
            break;
        }
        if position + 2 > image.len() {
            return Err(FormatError::Unsupported("truncated compression run length"));
        }
        let run = u16::from_le_bytes([image[position], image[position + 1]]) as usize;
        position += 2;
        if position >= image.len() {
            return Err(FormatError::Unsupported("truncated compression run byte"));
        }
        let byte = image[position];
        position += 1;
        out.extend(std::iter::repeat(byte).take(run));
        if out.len() > 0xFFFF {
            return Err(FormatError::Unsupported("decompressed record is too long"));
        }
    }
    Ok(out)
}

fn variable_pointer(tail: &[u8]) -> Result<u32> {
    if tail.len() < 4 {
        return Err(FormatError::Unsupported(
            "variable record tail is too short",
        ));
    }
    let page = u16::from_be_bytes([tail[0], tail[1]]) as u32;
    let slot = u16::from_be_bytes([tail[2], tail[3]]) as u32;
    Ok((page << 16) | slot)
}

/// Follow a variable-portion chunk chain.  A chunk whose start boundary has
/// bit 15 set begins with a big-endian (page, slot) pointer to the next
/// chunk; its remaining bytes precede that chunk's data.
fn read_variable_portion(
    bytes: &[u8],
    page_size: PageSize,
    page_count: u32,
    mut pointer: u32,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut guard = 0usize;
    while pointer != 0xFFFF_FFFF {
        guard += 1;
        if guard > 4096 {
            return Err(FormatError::Unsupported("variable chunk chain is too long"));
        }
        let page_index = pointer >> 16;
        let slot = (pointer & 0xFFFF) as usize;
        if page_index == 0 || page_index >= page_count {
            return Err(FormatError::InvalidRecordAddress(pointer));
        }
        let size = page_size.bytes();
        let start = page_index as usize * size;
        let page = &bytes[start..start + size];
        let header = PageHeader::parse(page)?;
        if header.kind(page_index) == PageKind::Data {
            return Err(FormatError::NotDataPage {
                address: pointer,
                page: page_index,
                kind: PageKind::Data,
            });
        }
        let count = u16::from_le_bytes([page[0x0A], page[0x0B]]) as usize;
        if slot >= count {
            return Err(FormatError::InvalidRecordAddress(pointer));
        }
        let boundary = |index: usize| -> Result<u16> {
            let offset = size
                .checked_sub(2 * (index + 1))
                .ok_or(FormatError::OutOfBounds {
                    offset: size,
                    size: 2,
                    page_size: size,
                })?;
            Ok(u16::from_le_bytes([page[offset], page[offset + 1]]))
        };
        let start_boundary = boundary(slot)?;
        // Freed slots leave FFFF in the boundary table; the end of a live
        // chunk is the next boundary that is not a hole.
        let mut end_boundary = None;
        for index in slot + 1..=count {
            let value = boundary(index)?;
            if value != 0xFFFF {
                end_boundary = Some(value);
                break;
            }
        }
        let end_boundary = end_boundary.ok_or(FormatError::InvalidRecordAddress(pointer))?;
        if start_boundary == 0xFFFF {
            return Err(FormatError::InvalidRecordAddress(pointer));
        }
        let chunk_start = (start_boundary & 0x7FFF) as usize;
        let chunk_end = (end_boundary & 0x7FFF) as usize;
        if chunk_start < 12 || chunk_start > chunk_end || chunk_end > size {
            return Err(FormatError::InvalidRecordAddress(pointer));
        }
        let chunk = &page[chunk_start..chunk_end];
        if start_boundary & 0x8000 != 0 {
            if chunk.len() < 4 {
                return Err(FormatError::Unsupported("variable chunk is too short"));
            }
            let next_page = u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
            let next_slot = u16::from_be_bytes([chunk[2], chunk[3]]) as u32;
            out.extend_from_slice(&chunk[4..]);
            pointer = (next_page << 16) | next_slot;
        } else {
            out.extend_from_slice(chunk);
            pointer = 0xFFFF_FFFF;
        }
        if out.len() > 0xFFFF {
            return Err(FormatError::Unsupported("variable record is too long"));
        }
    }
    Ok(out)
}
