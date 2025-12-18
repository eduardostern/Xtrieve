//! Step operations: Physical record traversal (not using indexes)
//!
//! Btrieve 5.1 data page format:
//! - 6-byte header (prev_page:2, page_num:2, usage:2)
//! - Fixed-length records at consecutive offsets
//! - Deleted records marked by key=0xFFFFFFFF or first 2 bytes=0x0000

use std::path::PathBuf;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::cursor::{Cursor, PositionBlock};
use crate::file_manager::locking::SessionId;
use crate::storage::record::RecordAddress;

use super::dispatcher::{Engine, OperationRequest, OperationResponse};

/// Btrieve 5.1 data page header size
const HEADER_SIZE: usize = 6;

/// Check if a record slot is deleted in Btrieve 5.1 format
fn is_deleted(record_data: &[u8]) -> bool {
    if record_data.len() < 4 {
        return true;
    }
    // Deleted records have:
    // - First 4 bytes = 0xFFFFFFFF (end of free list)
    // - First 2 bytes = 0x0000 with next pointer in bytes 2-3 (free list link)
    let first_four = u32::from_le_bytes([record_data[0], record_data[1], record_data[2], record_data[3]]);
    let first_two = u16::from_le_bytes([record_data[0], record_data[1]]);
    first_four == 0xFFFFFFFF || first_two == 0x0000
}

/// Get record at given slot from page data
fn get_record(page_data: &[u8], record_length: u16, slot: u16) -> Option<Vec<u8>> {
    let record_len = record_length as usize;
    let offset = HEADER_SIZE + (slot as usize * record_len);
    if offset + record_len <= page_data.len() {
        let record_data = &page_data[offset..offset + record_len];
        if !is_deleted(record_data) {
            return Some(record_data.to_vec());
        }
    }
    None
}

/// Find first valid record in a page
fn first_record(page_data: &[u8], record_length: u16) -> Option<(u16, Vec<u8>)> {
    let record_len = record_length as usize;
    let mut offset = HEADER_SIZE;
    let mut slot: u16 = 0;

    while offset + record_len <= page_data.len() {
        let record_data = &page_data[offset..offset + record_len];
        if !is_deleted(record_data) {
            return Some((slot, record_data.to_vec()));
        }
        offset += record_len;
        slot += 1;
    }
    None
}

/// Find last valid record in a page
fn last_record(page_data: &[u8], record_length: u16) -> Option<(u16, Vec<u8>)> {
    let record_len = record_length as usize;
    let usable_space = page_data.len().saturating_sub(HEADER_SIZE);
    let max_slots = usable_space / record_len;

    for slot in (0..max_slots as u16).rev() {
        let offset = HEADER_SIZE + (slot as usize * record_len);
        if offset + record_len <= page_data.len() {
            let record_data = &page_data[offset..offset + record_len];
            if !is_deleted(record_data) {
                return Some((slot, record_data.to_vec()));
            }
        }
    }
    None
}

/// Find next valid record after given slot
fn next_record(page_data: &[u8], record_length: u16, after_slot: u16) -> Option<(u16, Vec<u8>)> {
    let record_len = record_length as usize;
    let usable_space = page_data.len().saturating_sub(HEADER_SIZE);
    let max_slots = usable_space / record_len;
    let mut slot = after_slot + 1;

    while (slot as usize) < max_slots {
        let offset = HEADER_SIZE + (slot as usize * record_len);
        if offset + record_len <= page_data.len() {
            let record_data = &page_data[offset..offset + record_len];
            if !is_deleted(record_data) {
                return Some((slot, record_data.to_vec()));
            }
        }
        slot += 1;
    }
    None
}

/// Find previous valid record before given slot
fn prev_record(page_data: &[u8], record_length: u16, before_slot: u16) -> Option<(u16, Vec<u8>)> {
    if before_slot == 0 {
        return None;
    }
    let record_len = record_length as usize;

    for slot in (0..before_slot).rev() {
        let offset = HEADER_SIZE + (slot as usize * record_len);
        if offset + record_len <= page_data.len() {
            let record_data = &page_data[offset..offset + record_len];
            if !is_deleted(record_data) {
                return Some((slot, record_data.to_vec()));
            }
        }
    }
    None
}

/// Extract file path from position block
fn get_file_path(position_block: &[u8]) -> Option<PathBuf> {
    if position_block.len() < 128 {
        return None;
    }
    let end = position_block[64..].iter()
        .position(|&b| b == 0)
        .unwrap_or(64);
    if end == 0 {
        return None;
    }
    let path_str = String::from_utf8_lossy(&position_block[64..64 + end]);
    Some(PathBuf::from(path_str.as_ref()))
}

/// Operation 33: Step First - get first record physically
pub fn step_first(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let record_length = f.fcr.record_length;
    let num_pages = f.fcr.num_pages;
    let first_data_page = f.fcr.first_data_page;

    if first_data_page == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Scan data pages looking for first valid record
    for page_num in first_data_page..=num_pages {
        let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), page_num) {
            cached
        } else {
            match f.read_page(page_num) {
                Ok(p) => {
                    engine.cache.put(&path.to_string_lossy(), p.clone(), false);
                    p
                }
                Err(_) => continue,
            }
        };

        if let Some((slot, record_data)) = first_record(&page.data, record_length) {
            let record_addr = RecordAddress::new(page_num, slot);
            drop(f);

            let mut cursor = Cursor::new(path, -1);
            cursor.position(record_addr, Vec::new(), record_data.clone());
            cursor.physical_position = Some(record_addr);
            let position = PositionBlock::from_cursor(&cursor);

            return Ok(OperationResponse::success()
                .with_data(record_data)
                .with_position(position.data.to_vec()));
        }
    }

    Err(BtrieveError::Status(StatusCode::EndOfFile))
}

/// Operation 34: Step Last - get last record physically
pub fn step_last(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let record_length = f.fcr.record_length;
    let num_pages = f.fcr.num_pages;
    let first_data_page = f.fcr.first_data_page;

    // Scan data pages from last to first looking for last valid record
    for page_num in (first_data_page..=num_pages).rev() {
        let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), page_num) {
            cached
        } else {
            match f.read_page(page_num) {
                Ok(p) => {
                    engine.cache.put(&path.to_string_lossy(), p.clone(), false);
                    p
                }
                Err(_) => continue,
            }
        };

        if let Some((slot, record_data)) = last_record(&page.data, record_length) {
            let record_addr = RecordAddress::new(page_num, slot);
            drop(f);

            let mut cursor = Cursor::new(path, -1);
            cursor.position(record_addr, Vec::new(), record_data.clone());
            cursor.physical_position = Some(record_addr);
            let position = PositionBlock::from_cursor(&cursor);

            return Ok(OperationResponse::success()
                .with_data(record_data)
                .with_position(position.data.to_vec()));
        }
    }

    Err(BtrieveError::Status(StatusCode::EndOfFile))
}

/// Operation 24: Step Next - get next record physically
pub fn step_next(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Restore cursor
    let position = PositionBlock::from_bytes(&req.position_block);
    let cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return step_first(engine, _session, req);
    }

    let current_addr = cursor.physical_position
        .or(cursor.record_address)
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let record_length = f.fcr.record_length;
    let num_pages = f.fcr.num_pages;

    // Try next slot in current page
    let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), current_addr.page) {
        cached
    } else {
        let page = f.read_page(current_addr.page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    if let Some((next_slot, record_data)) = next_record(&page.data, record_length, current_addr.slot) {
        let record_addr = RecordAddress::new(current_addr.page, next_slot);
        drop(f);

        let mut new_cursor = Cursor::new(path, -1);
        new_cursor.position(record_addr, Vec::new(), record_data.clone());
        new_cursor.physical_position = Some(record_addr);
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(new_position.data.to_vec()));
    }

    // Try subsequent pages
    for page_num in (current_addr.page + 1)..=num_pages {
        let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), page_num) {
            cached
        } else {
            match f.read_page(page_num) {
                Ok(p) => {
                    engine.cache.put(&path.to_string_lossy(), p.clone(), false);
                    p
                }
                Err(_) => continue,
            }
        };

        if let Some((slot, record_data)) = first_record(&page.data, record_length) {
            let record_addr = RecordAddress::new(page_num, slot);
            drop(f);

            let mut new_cursor = Cursor::new(path, -1);
            new_cursor.position(record_addr, Vec::new(), record_data.clone());
            new_cursor.physical_position = Some(record_addr);
            let new_position = PositionBlock::from_cursor(&new_cursor);

            return Ok(OperationResponse::success()
                .with_data(record_data)
                .with_position(new_position.data.to_vec()));
        }
    }

    Err(BtrieveError::Status(StatusCode::EndOfFile))
}

/// Operation 35: Step Previous - get previous record physically
pub fn step_previous(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Restore cursor
    let position = PositionBlock::from_bytes(&req.position_block);
    let cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return step_last(engine, _session, req);
    }

    let current_addr = cursor.physical_position
        .or(cursor.record_address)
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let record_length = f.fcr.record_length;
    let first_data_page = f.fcr.first_data_page;

    // Try previous slot in current page
    let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), current_addr.page) {
        cached
    } else {
        let page = f.read_page(current_addr.page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    if let Some((prev_slot, record_data)) = prev_record(&page.data, record_length, current_addr.slot) {
        let record_addr = RecordAddress::new(current_addr.page, prev_slot);
        drop(f);

        let mut new_cursor = Cursor::new(path, -1);
        new_cursor.position(record_addr, Vec::new(), record_data.clone());
        new_cursor.physical_position = Some(record_addr);
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(new_position.data.to_vec()));
    }

    // Try previous pages
    if current_addr.page > first_data_page {
        for page_num in (first_data_page..current_addr.page).rev() {
            let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), page_num) {
                cached
            } else {
                match f.read_page(page_num) {
                    Ok(p) => {
                        engine.cache.put(&path.to_string_lossy(), p.clone(), false);
                        p
                    }
                    Err(_) => continue,
                }
            };

            if let Some((slot, record_data)) = last_record(&page.data, record_length) {
                let record_addr = RecordAddress::new(page_num, slot);
                drop(f);

                let mut new_cursor = Cursor::new(path, -1);
                new_cursor.position(record_addr, Vec::new(), record_data.clone());
                new_cursor.physical_position = Some(record_addr);
                let new_position = PositionBlock::from_cursor(&new_cursor);

                return Ok(OperationResponse::success()
                    .with_data(record_data)
                    .with_position(new_position.data.to_vec()));
            }
        }
    }

    Err(BtrieveError::Status(StatusCode::EndOfFile))
}
