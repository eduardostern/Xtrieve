//! Key-based retrieval operations: Get Equal, Get Next, Get Previous, etc.
//!
//! Btrieve 5.1 uses a hash-based index structure:
//! - Index entries are grouped by the low byte (hash) of the key
//! - Multiple index pages may exist, scattered throughout the file
//! - Index pages are identified by: prev_sibling=0xFFFFFFFF, next_sibling=0xFFFFFFFF
//! - For sorted access (GetFirst, GetNext), we must scan all index pages

use std::path::PathBuf;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::cursor::{Cursor, PositionBlock};
use crate::file_manager::locking::{LockType, SessionId};
use crate::storage::btree::{IndexNode, LeafEntry, SearchResult};
use crate::storage::key::KeySpec;
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
/// In Btrieve 5.1, address.page contains the absolute file offset to the record
/// (slot=0 indicates file offset mode)
fn read_record(
    engine: &Engine,
    file_path: &PathBuf,
    address: RecordAddress,
) -> BtrieveResult<Vec<u8>> {
    let file = engine.files.get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();

    // Btrieve 5.1: address.page contains absolute file offset to record data
    // Calculate which page contains this offset
    let file_offset = address.page as u64;
    let page_size = f.fcr.page_size as u64;
    let page_number = (file_offset / page_size) as u32;
    let offset_in_page = (file_offset % page_size) as usize;

    // Read the page containing the record
    let page = if let Some(cached) = engine.cache.get(&file_path.to_string_lossy(), page_number) {
        cached
    } else {
        let page = f.read_page(page_number)?;
        engine.cache.put(&file_path.to_string_lossy(), page.clone(), false);
        page
    };

    // Extract record data from the page at the calculated offset
    // Record format in Btrieve 5.1: record data starts at file_offset
    let record_length = f.fcr.record_length as usize;

    if offset_in_page + record_length > page.data.len() {
        return Err(BtrieveError::Status(StatusCode::InvalidRecordAddress));
    }

    let record_data = page.data[offset_in_page..offset_in_page + record_length].to_vec();
    Ok(record_data)
}

/// Check if a page is an index page (Btrieve 5.1 hash index format)
/// Index pages have: prev_sibling=0xFFFFFFFF, next_sibling=0xFFFFFFFF, entry_count > 0
fn is_index_page(page_data: &[u8]) -> bool {
    if page_data.len() < 16 {
        return false;
    }
    let entry_count = u16::from_le_bytes([page_data[6], page_data[7]]);
    let prev_sib = u32::from_le_bytes([page_data[8], page_data[9], page_data[10], page_data[11]]);
    let next_sib = u32::from_le_bytes([page_data[12], page_data[13], page_data[14], page_data[15]]);

    entry_count > 0 && entry_count < 1000 && prev_sib == 0xFFFFFFFF && next_sib == 0xFFFFFFFF
}

/// Collect all index entries from all index pages in the file
/// Returns entries sorted by key value for ordered access
fn collect_all_index_entries(
    engine: &Engine,
    file_path: &PathBuf,
    key_spec: &KeySpec,
) -> BtrieveResult<Vec<(LeafEntry, u32, usize)>> {
    let file = engine.files.get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let num_pages = f.fcr.num_pages;
    let mut all_entries: Vec<(LeafEntry, u32, usize)> = Vec::new();

    // Scan all pages to find index pages
    for page_num in 1..=num_pages {
        let page = if let Some(cached) = engine.cache.get(&file_path.to_string_lossy(), page_num) {
            cached
        } else {
            match f.read_page(page_num) {
                Ok(p) => {
                    engine.cache.put(&file_path.to_string_lossy(), p.clone(), false);
                    p
                }
                Err(_) => continue,
            }
        };

        if !is_index_page(&page.data) {
            continue;
        }

        // Parse index page and collect entries
        if let Ok(node) = IndexNode::from_bytes(page_num, &page.data, key_spec.clone()) {
            for (idx, entry) in node.leaf_entries.into_iter().enumerate() {
                all_entries.push((entry, page_num, idx));
            }
        }
    }

    // Sort entries by key value
    all_entries.sort_by(|a, b| a.0.key.cmp(&b.0.key));

    Ok(all_entries)
}

/// Find index entry by exact key match using hash bucket optimization
fn find_entry_by_key(
    engine: &Engine,
    file_path: &PathBuf,
    key_spec: &KeySpec,
    search_key: &[u8],
) -> BtrieveResult<Option<(LeafEntry, u32, usize)>> {
    let file = engine.files.get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let num_pages = f.fcr.num_pages;

    // Scan all index pages looking for exact match
    for page_num in 1..=num_pages {
        let page = if let Some(cached) = engine.cache.get(&file_path.to_string_lossy(), page_num) {
            cached
        } else {
            match f.read_page(page_num) {
                Ok(p) => {
                    engine.cache.put(&file_path.to_string_lossy(), p.clone(), false);
                    p
                }
                Err(_) => continue,
            }
        };

        if !is_index_page(&page.data) {
            continue;
        }

        if let Ok(node) = IndexNode::from_bytes(page_num, &page.data, key_spec.clone()) {
            for (idx, entry) in node.leaf_entries.iter().enumerate() {
                if entry.key == search_key {
                    return Ok(Some((entry.clone(), page_num, idx)));
                }
            }
        }
    }

    Ok(None)
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

    // Btrieve 5.1: Check if record is locked by another session's transaction
    // This provides isolation - uncommitted changes are invisible because we can't read them
    if engine.locks.is_record_locked(&path.to_string_lossy(), entry.record_address, session) {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

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
/// Btrieve 5.1: Finds the next larger key by scanning all index pages
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

    let key_spec = {
        let f = file.read();
        let key_number = cursor.key_number as usize;
        if key_number >= f.fcr.keys.len() {
            return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
        }
        f.fcr.keys[key_number].clone()
    };

    // Collect all index entries sorted by key
    let entries = collect_all_index_entries(engine, &path, &key_spec)?;

    if entries.is_empty() {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Find current position in sorted entries
    // Match by both key and record address for uniqueness
    let current_key = &cursor.key_value;
    let current_addr = cursor.record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    let current_idx = entries.iter().position(|(e, _, _)| {
        e.key == *current_key && e.record_address == current_addr
    });

    let next_idx = match current_idx {
        Some(idx) => idx + 1,
        None => {
            // Current key not found - find first key greater than current
            entries.iter().position(|(e, _, _)| e.key > *current_key)
                .ok_or(BtrieveError::Status(StatusCode::EndOfFile))?
        }
    };

    if next_idx >= entries.len() {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    let (entry, leaf_page, leaf_index) = &entries[next_idx];

    // Check if record is locked
    if engine.locks.is_record_locked(&path.to_string_lossy(), entry.record_address, session) {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

    let record_data = read_record(engine, &path, entry.record_address)?;

    let mut new_cursor = Cursor::new(path, cursor.key_number);
    new_cursor.position_with_leaf(
        entry.record_address,
        entry.key.clone(),
        record_data.clone(),
        *leaf_page,
        *leaf_index,
    );
    let new_position = PositionBlock::from_cursor(&new_cursor);

    Ok(OperationResponse::success()
        .with_data(record_data)
        .with_key(entry.key.clone())
        .with_position(new_position.data.to_vec()))
}

/// Operation 7: Get Previous - get previous record in key order
/// Btrieve 5.1: Finds the previous smaller key by scanning all index pages
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

    let key_spec = {
        let f = file.read();
        let key_number = cursor.key_number as usize;
        if key_number >= f.fcr.keys.len() {
            return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
        }
        f.fcr.keys[key_number].clone()
    };

    // Collect all index entries sorted by key
    let entries = collect_all_index_entries(engine, &path, &key_spec)?;

    if entries.is_empty() {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Find current position in sorted entries
    // Match by both key and record address for uniqueness
    let current_key = &cursor.key_value;
    let current_addr = cursor.record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    let current_idx = entries.iter().position(|(e, _, _)| {
        e.key == *current_key && e.record_address == current_addr
    });

    let prev_idx = match current_idx {
        Some(0) => return Err(BtrieveError::Status(StatusCode::EndOfFile)),
        Some(idx) => idx - 1,
        None => {
            // Current key not found - find last key smaller than current
            entries.iter().rposition(|(e, _, _)| e.key < *current_key)
                .ok_or(BtrieveError::Status(StatusCode::EndOfFile))?
        }
    };

    let (entry, leaf_page, leaf_index) = &entries[prev_idx];

    // Check if record is locked
    if engine.locks.is_record_locked(&path.to_string_lossy(), entry.record_address, session) {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

    let record_data = read_record(engine, &path, entry.record_address)?;

    let mut new_cursor = Cursor::new(path, cursor.key_number);
    new_cursor.position_with_leaf(
        entry.record_address,
        entry.key.clone(),
        record_data.clone(),
        *leaf_page,
        *leaf_index,
    );
    let new_position = PositionBlock::from_cursor(&new_cursor);

    Ok(OperationResponse::success()
        .with_data(record_data)
        .with_key(entry.key.clone())
        .with_position(new_position.data.to_vec()))
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
            // Find first entry > search_key
            for (idx, entry) in node.leaf_entries.iter().enumerate() {
                if entry.key.as_slice() > search_key.as_slice() {
                    // Btrieve 5.1: Check if record is locked by another session's transaction
                    if engine.locks.is_record_locked(&path.to_string_lossy(), entry.record_address, session) {
                        return Err(BtrieveError::Status(StatusCode::RecordInUse));
                    }

                    drop(f);
                    let record_data = read_record(engine, &path, entry.record_address)?;

                    let mut cursor = Cursor::new(path, req.key_number);
                    cursor.position_with_leaf(
                        entry.record_address,
                        entry.key.clone(),
                        record_data.clone(),
                        current_page,
                        idx,
                    );
                    let position = PositionBlock::from_cursor(&cursor);

                    return Ok(OperationResponse::success()
                        .with_data(record_data)
                        .with_key(entry.key.clone())
                        .with_position(position.data.to_vec()));
                }
            }
            // No entry found in this leaf, try next sibling
            if node.next_sibling == 0 {
                return Err(BtrieveError::Status(StatusCode::KeyNotFound));
            }
            current_page = node.next_sibling;
        } else {
            // Internal node - find child to descend into
            current_page = node.find_child(search_key);
        }
    }
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

/// Operation 10: Get Less Than - get last record with key < search key
pub fn get_less_than(
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

    // Navigate to leaf and find last entry < search_key
    let mut current_page = root_page;
    let mut best_entry: Option<(crate::storage::btree::LeafEntry, u32, usize)> = None;

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
            // Find last entry < search_key
            for (idx, entry) in node.leaf_entries.iter().enumerate().rev() {
                if entry.key.as_slice() < search_key.as_slice() {
                    best_entry = Some((entry.clone(), current_page, idx));
                    break;
                }
            }

            // If we found an entry, use it; otherwise try previous sibling
            if best_entry.is_some() {
                break;
            }

            if node.prev_sibling == 0 {
                return Err(BtrieveError::Status(StatusCode::KeyNotFound));
            }
            current_page = node.prev_sibling;
        } else {
            // Internal node - find child to descend into
            current_page = node.find_child(search_key);
        }
    }

    if let Some((entry, leaf_page, idx)) = best_entry {
        // Btrieve 5.1: Check if record is locked by another session's transaction
        if engine.locks.is_record_locked(&path.to_string_lossy(), entry.record_address, session) {
            return Err(BtrieveError::Status(StatusCode::RecordInUse));
        }

        drop(f);
        let record_data = read_record(engine, &path, entry.record_address)?;

        let mut cursor = Cursor::new(path, req.key_number);
        cursor.position_with_leaf(
            entry.record_address,
            entry.key.clone(),
            record_data.clone(),
            leaf_page,
            idx,
        );
        let position = PositionBlock::from_cursor(&cursor);

        return Ok(OperationResponse::success()
            .with_data(record_data)
            .with_key(entry.key.clone())
            .with_position(position.data.to_vec()));
    }

    Err(BtrieveError::Status(StatusCode::KeyNotFound))
}

/// Operation 11: Get Less or Equal - get last record with key <= search key
pub fn get_less_or_equal(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // Try exact match first
    match get_equal(engine, session, req) {
        Ok(response) => Ok(response),
        Err(_) => get_less_than(engine, session, req),
    }
}

/// Operation 12: Get First - get first record in key order
/// Btrieve 5.1: Scans all index pages to find the minimum key
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

    let key_spec = {
        let f = file.read();
        if key_number >= f.fcr.keys.len() {
            return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
        }
        f.fcr.keys[key_number].clone()
    };

    // Collect all index entries sorted by key
    let entries = collect_all_index_entries(engine, &path, &key_spec)?;

    if entries.is_empty() {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // First entry (minimum key) is at index 0 after sorting
    let (entry, leaf_page, leaf_index) = &entries[0];

    // Check if record is locked
    if engine.locks.is_record_locked(&path.to_string_lossy(), entry.record_address, session) {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

    let record_data = read_record(engine, &path, entry.record_address)?;

    let mut cursor = Cursor::new(path, req.key_number);
    cursor.position_with_leaf(
        entry.record_address,
        entry.key.clone(),
        record_data.clone(),
        *leaf_page,
        *leaf_index,
    );
    let position = PositionBlock::from_cursor(&cursor);

    Ok(OperationResponse::success()
        .with_data(record_data)
        .with_key(entry.key.clone())
        .with_position(position.data.to_vec()))
}

/// Operation 13: Get Last - get last record in key order
/// Btrieve 5.1: Scans all index pages to find the maximum key
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

    let key_spec = {
        let f = file.read();
        if key_number >= f.fcr.keys.len() {
            return Err(BtrieveError::Status(StatusCode::InvalidKeyNumber));
        }
        f.fcr.keys[key_number].clone()
    };

    // Collect all index entries sorted by key
    let entries = collect_all_index_entries(engine, &path, &key_spec)?;

    if entries.is_empty() {
        return Err(BtrieveError::Status(StatusCode::EndOfFile));
    }

    // Last entry (maximum key) is at the end after sorting
    let (entry, leaf_page, leaf_index) = &entries[entries.len() - 1];

    // Check if record is locked
    if engine.locks.is_record_locked(&path.to_string_lossy(), entry.record_address, session) {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

    let record_data = read_record(engine, &path, entry.record_address)?;

    let mut cursor = Cursor::new(path, req.key_number);
    cursor.position_with_leaf(
        entry.record_address,
        entry.key.clone(),
        record_data.clone(),
        *leaf_page,
        *leaf_index,
    );
    let position = PositionBlock::from_cursor(&cursor);

    Ok(OperationResponse::success()
        .with_data(record_data)
        .with_key(entry.key.clone())
        .with_position(position.data.to_vec()))
}
