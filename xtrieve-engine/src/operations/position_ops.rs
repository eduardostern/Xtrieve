//! Position operations: Get Position, Get Direct, Get By Percentage

use std::path::PathBuf;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::cursor::{Cursor, PositionBlock};
use crate::file_manager::locking::SessionId;
use crate::storage::record::RecordAddress;

use super::dispatcher::{Engine, OperationRequest, OperationResponse};

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

/// Helper to read a record given its address
/// In Btrieve 5.1 format, address.slot contains the absolute file offset
fn read_record(
    engine: &Engine,
    file_path: &PathBuf,
    address: RecordAddress,
) -> BtrieveResult<Vec<u8>> {
    let file = engine.files.get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    // Btrieve 5.1: address.slot contains absolute file offset to record data
    let file_offset = address.slot as u64;
    let page_size = f.fcr.page_size as u64;
    let page_number = (file_offset / page_size) as u32;
    let offset_in_page = (file_offset % page_size) as usize;

    let page = if let Some(cached) = engine.cache.get(&file_path.to_string_lossy(), page_number) {
        cached
    } else {
        let page = f.read_page(page_number)?;
        engine.cache.put(&file_path.to_string_lossy(), page.clone(), false);
        page
    };

    let record_length = f.fcr.record_length as usize;

    if offset_in_page + record_length > page.data.len() {
        return Err(BtrieveError::Status(StatusCode::InvalidRecordAddress));
    }

    let record_data = page.data[offset_in_page..offset_in_page + record_length].to_vec();
    Ok(record_data)
}

/// Operation 22: Get Position - get physical address of current record
pub fn get_position(
    _engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Restore cursor
    let position_block = PositionBlock::from_bytes(&req.position_block);
    let cursor = position_block.to_cursor(path);

    if !cursor.is_positioned() {
        return Err(BtrieveError::Status(StatusCode::InvalidPositioning));
    }

    let record_addr = cursor.record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    // Convert to 4-byte position (Btrieve format)
    let position_value = record_addr.to_position(0); // page_size not needed for basic conversion

    // Return position in data buffer (4 bytes)
    let mut data = vec![0u8; 4];
    data.copy_from_slice(&position_value.to_le_bytes());

    Ok(OperationResponse::success()
        .with_data(data)
        .with_position(req.position_block.clone()))
}

/// Operation 23: Get Direct - get record by physical position
pub fn get_direct(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Position is passed in data buffer (4 bytes)
    if req.data_buffer.len() < 4 {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    let position_value = u32::from_le_bytes([
        req.data_buffer[0],
        req.data_buffer[1],
        req.data_buffer[2],
        req.data_buffer[3],
    ]);

    // Convert position to record address
    let record_addr = RecordAddress::from_position(position_value);

    // Validate address
    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    {
        let f = file.read();
        if record_addr.page >= f.fcr.num_pages {
            return Err(BtrieveError::Status(StatusCode::InvalidRecordAddress));
        }
    }

    // Read the record
    let record_data = read_record(engine, &path, record_addr)?;

    // Build cursor
    let mut cursor = Cursor::new(path, req.key_number);
    cursor.position(record_addr, Vec::new(), record_data.clone());
    let position = PositionBlock::from_cursor(&cursor);

    Ok(OperationResponse::success()
        .with_data(record_data)
        .with_position(position.data.to_vec()))
}

/// Operation 26: Get By Percentage - position to approximate location
pub fn get_by_percentage(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Percentage is passed in data buffer (4 bytes, scaled 0-10000)
    if req.data_buffer.len() < 4 {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    let percentage = u32::from_le_bytes([
        req.data_buffer[0],
        req.data_buffer[1],
        req.data_buffer[2],
        req.data_buffer[3],
    ]);

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let total_records = f.fcr.num_records;

    if total_records == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Calculate approximate record number
    let target_record = ((percentage as u64 * total_records as u64) / 10000) as u32;

    // For now, use step operations to find the record
    // TODO: Implement more efficient positioning

    drop(f);

    // Start from first and step forward
    let mut modified_req = req.clone();

    // Get first record
    let first_response = super::step_ops::step_first(engine, _session, &modified_req)?;

    if target_record == 0 {
        return Ok(first_response);
    }

    // Step through to target (inefficient, but functional)
    modified_req.position_block = first_response.position_block.clone();

    for _ in 0..target_record {
        match super::step_ops::step_next(engine, _session, &modified_req) {
            Ok(response) => {
                modified_req.position_block = response.position_block.clone();
            }
            Err(_) => break,
        }
    }

    // Re-read current record
    let position = PositionBlock::from_bytes(&modified_req.position_block);
    let cursor = position.to_cursor(path.clone());

    if let Some(addr) = cursor.record_address {
        let record_data = read_record(engine, &path, addr)?;
        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(modified_req.position_block));
    }

    Err(BtrieveError::Status(StatusCode::EndOfFile))
}

/// Operation 27: Find Percentage - get percentage position of current record
pub fn find_percentage(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let position_block = PositionBlock::from_bytes(&req.position_block);
    let cursor = position_block.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return Err(BtrieveError::Status(StatusCode::InvalidPositioning));
    }

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let total_records = f.fcr.num_records;

    if total_records == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Estimate percentage based on record address
    // This is approximate - real implementation would count records
    let record_addr = cursor.record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    // Simple estimation: assume even distribution across pages
    let page_ratio = record_addr.page as f64 / f.fcr.num_pages as f64;
    let percentage = (page_ratio * 10000.0) as u32;

    // Return percentage in data buffer (4 bytes)
    let mut data = vec![0u8; 4];
    data.copy_from_slice(&percentage.to_le_bytes());

    Ok(OperationResponse::success()
        .with_data(data)
        .with_position(req.position_block.clone()))
}
