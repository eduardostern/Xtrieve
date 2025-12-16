//! Step operations: Physical record traversal (not using indexes)

use std::path::PathBuf;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::cursor::{Cursor, CursorState, PositionBlock};
use crate::file_manager::locking::SessionId;
use crate::storage::record::{DataPage, RecordAddress};

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
fn read_record(
    engine: &Engine,
    file_path: &PathBuf,
    address: RecordAddress,
) -> BtrieveResult<Vec<u8>> {
    let file = engine.files.get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    let page = if let Some(cached) = engine.cache.get(&file_path.to_string_lossy(), address.page) {
        cached
    } else {
        let page = f.read_page(address.page)?;
        engine.cache.put(&file_path.to_string_lossy(), page.clone(), false);
        page
    };

    let data_page = DataPage::from_bytes(address.page, page.data)?;

    data_page.get_record(address.slot)
        .map(|r| r.to_vec())
        .ok_or(BtrieveError::Status(StatusCode::InvalidRecordAddress))
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

    // Find first data page
    let first_data_page = f.fcr.first_data_page;
    if first_data_page == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Read first data page
    let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), first_data_page) {
        cached
    } else {
        let page = f.read_page(first_data_page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let data_page = DataPage::from_bytes(first_data_page, page.data)?;

    // Find first valid slot
    if let Some(slot) = data_page.first_slot() {
        let record_addr = RecordAddress::new(first_data_page, slot);
        drop(f);
        let record_data = read_record(engine, &path, record_addr)?;

        let mut cursor = Cursor::new(path, -1); // -1 indicates physical positioning
        cursor.position(record_addr, Vec::new(), record_data.clone());
        cursor.physical_position = Some(record_addr);
        let position = PositionBlock::from_cursor(&cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(position.data.to_vec()));
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

    // Find last data page
    let last_data_page = f.fcr.last_data_page;
    if last_data_page == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Read last data page
    let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), last_data_page) {
        cached
    } else {
        let page = f.read_page(last_data_page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let data_page = DataPage::from_bytes(last_data_page, page.data)?;

    // Find last valid slot
    if let Some(slot) = data_page.last_slot() {
        let record_addr = RecordAddress::new(last_data_page, slot);
        drop(f);
        let record_data = read_record(engine, &path, record_addr)?;

        let mut cursor = Cursor::new(path, -1);
        cursor.position(record_addr, Vec::new(), record_data.clone());
        cursor.physical_position = Some(record_addr);
        let position = PositionBlock::from_cursor(&cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(position.data.to_vec()));
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
        // If not positioned, do step first
        return step_first(engine, _session, req);
    }

    let current_addr = cursor.record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    // Read current page
    let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), current_addr.page) {
        cached
    } else {
        let page = f.read_page(current_addr.page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let data_page = DataPage::from_bytes(current_addr.page, page.data)?;

    // Try next slot in current page
    if let Some(next_slot) = data_page.next_slot(current_addr.slot) {
        let record_addr = RecordAddress::new(current_addr.page, next_slot);
        drop(f);
        let record_data = read_record(engine, &path, record_addr)?;

        let mut new_cursor = Cursor::new(path, -1);
        new_cursor.position(record_addr, Vec::new(), record_data.clone());
        new_cursor.physical_position = Some(record_addr);
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(new_position.data.to_vec()));
    }

    // Move to next page
    if data_page.next_page == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    let next_page_data = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), data_page.next_page) {
        cached
    } else {
        let page = f.read_page(data_page.next_page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let next_data_page = DataPage::from_bytes(data_page.next_page, next_page_data.data)?;

    if let Some(slot) = next_data_page.first_slot() {
        let record_addr = RecordAddress::new(data_page.next_page, slot);
        drop(f);
        let record_data = read_record(engine, &path, record_addr)?;

        let mut new_cursor = Cursor::new(path, -1);
        new_cursor.position(record_addr, Vec::new(), record_data.clone());
        new_cursor.physical_position = Some(record_addr);
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(new_position.data.to_vec()));
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

    let position = PositionBlock::from_bytes(&req.position_block);
    let cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        // If not positioned, do step last
        return step_last(engine, _session, req);
    }

    let current_addr = cursor.record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), current_addr.page) {
        cached
    } else {
        let page = f.read_page(current_addr.page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let data_page = DataPage::from_bytes(current_addr.page, page.data)?;

    // Try previous slot in current page
    if let Some(prev_slot) = data_page.prev_slot(current_addr.slot) {
        let record_addr = RecordAddress::new(current_addr.page, prev_slot);
        drop(f);
        let record_data = read_record(engine, &path, record_addr)?;

        let mut new_cursor = Cursor::new(path, -1);
        new_cursor.position(record_addr, Vec::new(), record_data.clone());
        new_cursor.physical_position = Some(record_addr);
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(new_position.data.to_vec()));
    }

    // Move to previous page
    if data_page.prev_page == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    let prev_page_data = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), data_page.prev_page) {
        cached
    } else {
        let page = f.read_page(data_page.prev_page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let prev_data_page = DataPage::from_bytes(data_page.prev_page, prev_page_data.data)?;

    if let Some(slot) = prev_data_page.last_slot() {
        let record_addr = RecordAddress::new(data_page.prev_page, slot);
        drop(f);
        let record_data = read_record(engine, &path, record_addr)?;

        let mut new_cursor = Cursor::new(path, -1);
        new_cursor.position(record_addr, Vec::new(), record_data.clone());
        new_cursor.physical_position = Some(record_addr);
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_position(new_position.data.to_vec()));
    }

    Err(BtrieveError::Status(StatusCode::EndOfFile))
}
