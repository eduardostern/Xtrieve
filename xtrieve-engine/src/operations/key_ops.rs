//! Key-based retrieval operations: Get Equal, Get Next, Get Previous, etc.

use std::cmp::Ordering;
use std::path::PathBuf;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::cursor::{Cursor, CursorState, PositionBlock};
use crate::file_manager::locking::{LockType, SessionId};
use crate::storage::btree::{IndexNode, NodeType, SearchResult};
use crate::storage::page::PageType;
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

    // Try cache first
    let page = if let Some(cached) = engine.cache.get(&file_path.to_string_lossy(), address.page) {
        cached
    } else {
        let page = f.read_page(address.page)?;
        engine.cache.put(&file_path.to_string_lossy(), page.clone(), false);
        page
    };

    // Parse data page
    let data_page = DataPage::from_bytes(address.page, page.data)?;

    // Get record from slot
    data_page.get_record(address.slot)
        .map(|r| r.to_vec())
        .ok_or(BtrieveError::Status(StatusCode::InvalidRecordAddress))
}

/// Search the B+ tree for a key
fn search_btree(
    engine: &Engine,
    file_path: &PathBuf,
    key_number: usize,
    search_key: &[u8],
) -> BtrieveResult<SearchResult> {
    let file = engine.files.get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    if key_number >= f.fcr.keys.len() {
        return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
    }

    let key_spec = &f.fcr.keys[key_number];
    let root_page = *f.fcr.index_roots.get(key_number).unwrap_or(&0);

    if root_page == 0 {
        // Empty index
        return Ok(SearchResult::not_found(0));
    }

    // Traverse tree from root to leaf
    let mut current_page = root_page;

    loop {
        // Read page
        let page = if let Some(cached) = engine.cache.get(&file_path.to_string_lossy(), current_page) {
            cached
        } else {
            let page = f.read_page(current_page)?;
            engine.cache.put(&file_path.to_string_lossy(), page.clone(), false);
            page
        };

        let node = IndexNode::from_bytes(current_page, &page.data, key_spec.clone())?;

        if node.is_leaf() {
            // Search leaf node
            if let Some(entry) = node.find_exact(search_key) {
                let index = node.find_index(search_key).unwrap_or(0);
                return Ok(SearchResult::found(current_page, index, entry.clone()));
            } else {
                return Ok(SearchResult::not_found(current_page));
            }
        } else {
            // Internal node - find child to descend into
            current_page = node.find_child(search_key);
        }
    }
}

/// Operation 5: Get Equal - find record by exact key match
pub fn get_equal(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let key_number = req.key_number as usize;
    let search_key = &req.key_buffer;

    // Search B+ tree
    let result = search_btree(engine, &path, key_number, search_key)?;

    if !result.exact_match {
        return Err(BtrieveError::Status(StatusCode::KeyNotFound));
    }

    let entry = result.entry.ok_or(BtrieveError::Status(StatusCode::KeyNotFound))?;

    // Read the record
    let record_data = read_record(engine, &path, entry.record_address)?;

    // Acquire lock if requested
    let lock_type = LockType::from_bias(req.lock_bias);
    if lock_type != LockType::None {
        engine.locks.lock_record(
            &path.to_string_lossy(),
            entry.record_address,
            session,
            lock_type,
        )?;
    }

    // Build cursor
    let mut cursor = Cursor::new(path, req.key_number);
    cursor.position_with_leaf(
        entry.record_address,
        entry.key.clone(),
        record_data.clone(),
        result.leaf_page,
        result.entry_index as usize,
    );
    let position = PositionBlock::from_cursor(&cursor);

    Ok(OperationResponse::success()
        .with_data(record_data)
        .with_key(entry.key)
        .with_position(position.data.to_vec()))
}

/// Operation 6: Get Next - get next record in key order
pub fn get_next(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Restore cursor
    let position = PositionBlock::from_bytes(&req.position_block);
    let cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return Err(BtrieveError::Status(StatusCode::InvalidPositioning));
    }

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let key_number = cursor.key_number as usize;

    if key_number >= f.fcr.keys.len() {
        return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
    }

    let key_spec = &f.fcr.keys[key_number];

    // Read current leaf node
    let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), cursor.leaf_page) {
        cached
    } else {
        let page = f.read_page(cursor.leaf_page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let node = IndexNode::from_bytes(cursor.leaf_page, &page.data, key_spec.clone())?;

    // Try next entry in current node
    let next_index = cursor.leaf_index + 1;
    if let Some(entry) = node.get_entry(next_index) {
        drop(f);
        let record_data = read_record(engine, &path, entry.record_address)?;

        let mut new_cursor = Cursor::new(path, cursor.key_number);
        new_cursor.position_with_leaf(
            entry.record_address,
            entry.key.clone(),
            record_data.clone(),
            cursor.leaf_page,
            next_index,
        );
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_key(entry.key.clone())
            .with_position(new_position.data.to_vec()));
    }

    // Move to next sibling leaf
    if node.next_sibling == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    let next_page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), node.next_sibling) {
        cached
    } else {
        let page = f.read_page(node.next_sibling)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let next_node = IndexNode::from_bytes(node.next_sibling, &next_page.data, key_spec.clone())?;

    if let Some(entry) = next_node.first_entry() {
        drop(f);
        let record_data = read_record(engine, &path, entry.record_address)?;

        let mut new_cursor = Cursor::new(path, cursor.key_number);
        new_cursor.position_with_leaf(
            entry.record_address,
            entry.key.clone(),
            record_data.clone(),
            node.next_sibling,
            0,
        );
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_key(entry.key.clone())
            .with_position(new_position.data.to_vec()));
    }

    Err(BtrieveError::Status(StatusCode::EndOfFile))
}

/// Operation 7: Get Previous - get previous record in key order
pub fn get_previous(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let position = PositionBlock::from_bytes(&req.position_block);
    let cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return Err(BtrieveError::Status(StatusCode::InvalidPositioning));
    }

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let key_number = cursor.key_number as usize;
    let key_spec = &f.fcr.keys[key_number];

    let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), cursor.leaf_page) {
        cached
    } else {
        let page = f.read_page(cursor.leaf_page)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let node = IndexNode::from_bytes(cursor.leaf_page, &page.data, key_spec.clone())?;

    // Try previous entry in current node
    if cursor.leaf_index > 0 {
        let prev_index = cursor.leaf_index - 1;
        if let Some(entry) = node.get_entry(prev_index) {
            drop(f);
            let record_data = read_record(engine, &path, entry.record_address)?;

            let mut new_cursor = Cursor::new(path, cursor.key_number);
            new_cursor.position_with_leaf(
                entry.record_address,
                entry.key.clone(),
                record_data.clone(),
                cursor.leaf_page,
                prev_index,
            );
            let new_position = PositionBlock::from_cursor(&new_cursor);

            return Ok(OperationResponse::success()
                .with_data(record_data)
                .with_key(entry.key.clone())
                .with_position(new_position.data.to_vec()));
        }
    }

    // Move to previous sibling leaf
    if node.prev_sibling == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    let prev_page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), node.prev_sibling) {
        cached
    } else {
        let page = f.read_page(node.prev_sibling)?;
        engine.cache.put(&path.to_string_lossy(), page.clone(), false);
        page
    };

    let prev_node = IndexNode::from_bytes(node.prev_sibling, &prev_page.data, key_spec.clone())?;

    if let Some(entry) = prev_node.last_entry() {
        let last_index = prev_node.leaf_entries.len() - 1;
        drop(f);
        let record_data = read_record(engine, &path, entry.record_address)?;

        let mut new_cursor = Cursor::new(path, cursor.key_number);
        new_cursor.position_with_leaf(
            entry.record_address,
            entry.key.clone(),
            record_data.clone(),
            node.prev_sibling,
            last_index,
        );
        let new_position = PositionBlock::from_cursor(&new_cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_key(entry.key.clone())
            .with_position(new_position.data.to_vec()));
    }

    Err(BtrieveError::Status(StatusCode::EndOfFile))
}

/// Operation 8: Get Greater - get first record with key > search key
pub fn get_greater(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let key_number = req.key_number as usize;
    let search_key = &req.key_buffer;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    if key_number >= f.fcr.keys.len() {
        return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
    }

    let key_spec = &f.fcr.keys[key_number];
    let root_page = *f.fcr.index_roots.get(key_number).unwrap_or(&0);

    if root_page == 0 {
        return Err(BtrieveError::Status(StatusCode::KeyNotFound));
    }

    // Navigate to leaf and find first entry > search_key
    // For now, use get_equal and then get_next approach
    // TODO: Optimize with direct traversal

    // First try exact match, then get next
    let result = search_btree(engine, &path, key_number, search_key);
    drop(f);

    // Simulate by using get_equal then get_next
    // This is a simplified implementation
    Err(BtrieveError::Status(StatusCode::KeyNotFound))
}

/// Operation 9: Get Greater or Equal
pub fn get_greater_or_equal(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // Try exact match first
    match get_equal(engine, session, req) {
        Ok(response) => Ok(response),
        Err(_) => get_greater(engine, session, req),
    }
}

/// Operation 10: Get Less Than
pub fn get_less_than(
    _engine: &Engine,
    _session: SessionId,
    _req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // TODO: Implement
    Err(BtrieveError::Status(StatusCode::InvalidOperation))
}

/// Operation 11: Get Less or Equal
pub fn get_less_or_equal(
    _engine: &Engine,
    _session: SessionId,
    _req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // TODO: Implement
    Err(BtrieveError::Status(StatusCode::InvalidOperation))
}

/// Operation 12: Get First - get first record in key order
pub fn get_first(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let key_number = req.key_number as usize;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    if key_number >= f.fcr.keys.len() {
        return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
    }

    let key_spec = &f.fcr.keys[key_number];
    let root_page = *f.fcr.index_roots.get(key_number).unwrap_or(&0);

    if root_page == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Navigate to leftmost leaf
    let mut current_page = root_page;

    loop {
        let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), current_page) {
            cached
        } else {
            let page = f.read_page(current_page)?;
            engine.cache.put(&path.to_string_lossy(), page.clone(), false);
            page
        };

        let node = IndexNode::from_bytes(current_page, &page.data, key_spec.clone())?;

        if node.is_leaf() {
            if let Some(entry) = node.first_entry() {
                drop(f);
                let record_data = read_record(engine, &path, entry.record_address)?;

                let mut cursor = Cursor::new(path, req.key_number);
                cursor.position_with_leaf(
                    entry.record_address,
                    entry.key.clone(),
                    record_data.clone(),
                    current_page,
                    0,
                );
                let position = PositionBlock::from_cursor(&cursor);

                return Ok(OperationResponse::success()
                    .with_data(record_data)
                    .with_key(entry.key.clone())
                    .with_position(position.data.to_vec()));
            } else {
                return Err(BtrieveError::Status(StatusCode::EndOfFile));
            }
        } else {
            // Go to leftmost child
            current_page = node.leftmost_child;
        }
    }
}

/// Operation 13: Get Last - get last record in key order
pub fn get_last(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let key_number = req.key_number as usize;

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    if key_number >= f.fcr.keys.len() {
        return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
    }

    let key_spec = &f.fcr.keys[key_number];
    let root_page = *f.fcr.index_roots.get(key_number).unwrap_or(&0);

    if root_page == 0 {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Navigate to rightmost leaf
    let mut current_page = root_page;

    loop {
        let page = if let Some(cached) = engine.cache.get(&path.to_string_lossy(), current_page) {
            cached
        } else {
            let page = f.read_page(current_page)?;
            engine.cache.put(&path.to_string_lossy(), page.clone(), false);
            page
        };

        let node = IndexNode::from_bytes(current_page, &page.data, key_spec.clone())?;

        if node.is_leaf() {
            if let Some(entry) = node.last_entry() {
                let last_index = node.leaf_entries.len() - 1;
                drop(f);
                let record_data = read_record(engine, &path, entry.record_address)?;

                let mut cursor = Cursor::new(path, req.key_number);
                cursor.position_with_leaf(
                    entry.record_address,
                    entry.key.clone(),
                    record_data.clone(),
                    current_page,
                    last_index,
                );
                let position = PositionBlock::from_cursor(&cursor);

                return Ok(OperationResponse::success()
                    .with_data(record_data)
                    .with_key(entry.key.clone())
                    .with_position(position.data.to_vec()));
            } else {
                return Err(BtrieveError::Status(StatusCode::EndOfFile));
            }
        } else {
            // Go to rightmost child
            current_page = node.internal_entries
                .last()
                .map(|e| e.child_page)
                .unwrap_or(node.leftmost_child);
        }
    }
}
