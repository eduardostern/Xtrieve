//! Record operations: Insert, Update, Delete

use std::path::PathBuf;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::cursor::{Cursor, CursorState, PositionBlock};
use crate::file_manager::locking::{LockType, SessionId};
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

/// Operation 2: Insert a new record
pub fn insert(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let record_data = &req.data_buffer;
    if record_data.is_empty() {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    let mut f = file.write();

    // Validate record length
    if record_data.len() > f.fcr.record_length as usize {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    // Pad record to fixed length if needed
    let mut record = record_data.to_vec();
    record.resize(f.fcr.record_length as usize, 0);

    // Check for duplicate keys (for non-duplicate keys)
    for (key_num, key_spec) in f.fcr.keys.iter().enumerate() {
        if !key_spec.allows_duplicates() {
            let key_value = key_spec.extract_key(&record);

            // TODO: Search B+ tree for existing key
            // For now, skip duplicate check (will implement with full B+ tree)
        }
    }

    // Find space for the record
    // For now, allocate a new page if needed
    // TODO: Implement proper free space management

    // Allocate record address
    let record_addr = RecordAddress::new(f.fcr.num_pages, 0);

    // Insert into all indexes
    // TODO: Implement B+ tree insert

    // Update FCR
    f.fcr.num_records += 1;
    f.update_fcr()?;

    // Build position block with new record position
    let mut cursor = Cursor::new(path.clone(), req.key_number);
    cursor.position(record_addr, Vec::new(), record);
    let position = PositionBlock::from_cursor(&cursor);

    Ok(OperationResponse::success()
        .with_position(position.data.to_vec()))
}

/// Operation 3: Update the current record
pub fn update(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Restore cursor from position block
    let position = PositionBlock::from_bytes(&req.position_block);
    let cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return Err(BtrieveError::Status(StatusCode::InvalidPositioning));
    }

    let record_addr = cursor.record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    // Check record lock
    let lock_type = LockType::from_bias(req.lock_bias);
    if engine.locks.is_record_locked(&path.to_string_lossy(), record_addr, session) {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    // Validate new record data
    let new_record = &req.data_buffer;
    if new_record.len() > f.fcr.record_length as usize {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    // Check modifiable key constraints
    let old_record = &cursor.record_data;
    for (key_num, key_spec) in f.fcr.keys.iter().enumerate() {
        if !key_spec.is_modifiable() {
            let old_key = key_spec.extract_key(old_record);
            let new_key = key_spec.extract_key(new_record);

            if old_key != new_key {
                return Err(BtrieveError::Status(StatusCode::ModifiableKeyChanged));
            }
        }
    }

    // TODO: Implement actual record update
    // 1. Update record data in data page
    // 2. Update indexes if keys changed

    Ok(OperationResponse::success()
        .with_position(req.position_block.clone()))
}

/// Operation 4: Delete the current record
pub fn delete(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Restore cursor from position block
    let position = PositionBlock::from_bytes(&req.position_block);
    let mut cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return Err(BtrieveError::Status(StatusCode::InvalidPositioning));
    }

    let record_addr = cursor.record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    // Check record lock
    if engine.locks.is_record_locked(&path.to_string_lossy(), record_addr, session) {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let mut f = file.write();

    // TODO: Implement actual record deletion
    // 1. Mark record slot as deleted
    // 2. Remove from all indexes
    // 3. Add to free space list

    // Update FCR
    f.fcr.num_records = f.fcr.num_records.saturating_sub(1);
    f.update_fcr()?;

    // Invalidate cursor
    cursor.invalidate();
    let position = PositionBlock::from_cursor(&cursor);

    Ok(OperationResponse::success()
        .with_position(position.data.to_vec()))
}
