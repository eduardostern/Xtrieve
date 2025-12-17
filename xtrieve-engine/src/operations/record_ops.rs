//! Record operations: Insert, Update, Delete

use std::path::PathBuf;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::cursor::{Cursor, PositionBlock};
use crate::file_manager::locking::{LockType, SessionId};
use crate::storage::btree::{IndexNode, InternalEntry, LeafEntry};
use crate::storage::page::Page;
use crate::storage::record::{DataPage, RecordAddress};

use super::dispatcher::{Engine, OperationRequest, OperationResponse};

/// Extract file path from position block
fn get_file_path(position_block: &[u8]) -> Option<PathBuf> {
    if position_block.len() < 128 {
        return None;
    }
    let end = position_block[64..]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(64);
    if end == 0 {
        return None;
    }
    let path_str = String::from_utf8_lossy(&position_block[64..64 + end]);
    Some(PathBuf::from(path_str.as_ref()))
}

/// Insert a key into the B+ tree, handling splits as needed
fn btree_insert(
    engine: &Engine,
    file_path: &PathBuf,
    key_number: usize,
    key_value: Vec<u8>,
    record_address: RecordAddress,
    allow_duplicates: bool,
    page_size: u16,
    session: SessionId,
) -> BtrieveResult<()> {
    let file = engine
        .files
        .get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Read root_page and key_spec with a short-lived read lock to avoid deadlock
    let (root_page, key_spec) = {
        let f = file.read();
        (f.fcr.index_roots[key_number], f.fcr.keys[key_number].clone())
    };

    // If no root exists, create initial leaf node (needs write lock)
    if root_page == 0 {
        let mut f = file.write();

        // Re-check in case another thread created it
        if f.fcr.index_roots[key_number] != 0 {
            drop(f);
            // Retry with the now-existing root
            return btree_insert(engine, file_path, key_number, key_value, record_address, allow_duplicates, page_size, session);
        }

        let new_page_num = f.fcr.num_pages;
        let mut leaf = IndexNode::new_leaf(new_page_num, key_spec.clone(), page_size);

        // Get next dup sequence if duplicates allowed
        let dup_seq = if allow_duplicates {
            key_spec.unique_count
        } else {
            0
        };

        leaf.insert_leaf_entry(
            LeafEntry {
                key: key_value.clone(),
                record_address,
                dup_sequence: dup_seq,
            },
            allow_duplicates,
        );

        // Write the new leaf page
        let leaf_data = leaf.to_bytes(page_size);
        let page = Page::from_data(new_page_num, leaf_data);
        f.fcr.num_pages += 1;
        f.fcr.index_roots[key_number] = new_page_num;

        // Update unique count if needed
        if allow_duplicates {
            f.fcr.keys[key_number].unique_count += 1;
        }

        f.update_fcr()?;
        f.write_page_for_session(&page, session)?;

        // Update cache with new leaf page
        let path_str = file_path.to_string_lossy();
        engine.cache.put(&path_str, page, false);

        return Ok(());
    }

    // Traverse tree to find insertion point (no locks held here)
    // For simplicity, we'll do a recursive descent with split propagation
    let result = btree_insert_recursive(
        engine,
        file_path,
        root_page,
        &key_spec,
        key_value.clone(),
        record_address,
        allow_duplicates,
        page_size,
        session,
    )?;

    // If root split occurred, create new root
    if let Some((separator, right_page)) = result {
        let file = engine.files.get(file_path).unwrap();
        let mut f = file.write();

        let new_root_num = f.fcr.num_pages;
        let mut new_root = IndexNode::new_internal(new_root_num, key_spec.clone(), root_page);
        new_root.insert_internal_entry(InternalEntry {
            key: separator,
            child_page: right_page,
        });

        let root_data = new_root.to_bytes(page_size);
        let page = Page::from_data(new_root_num, root_data);

        f.fcr.num_pages += 1;
        f.fcr.index_roots[key_number] = new_root_num;
        f.update_fcr()?;
        f.write_page_for_session(&page, session)?;

        // Update cache with new root page
        engine.cache.put(&file_path.to_string_lossy(), page, false);
    }

    Ok(())
}

/// Recursive B+ tree insertion, returns Some((separator, right_page)) if split occurred
fn btree_insert_recursive(
    engine: &Engine,
    file_path: &PathBuf,
    page_num: u32,
    key_spec: &crate::storage::key::KeySpec,
    key_value: Vec<u8>,
    record_address: RecordAddress,
    allow_duplicates: bool,
    page_size: u16,
    session: SessionId,
) -> BtrieveResult<Option<(Vec<u8>, u32)>> {
    let file = engine
        .files
        .get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Read the current node
    let page = {
        let f = file.read();
        f.read_page(page_num)?
    };

    let mut node = IndexNode::from_bytes(page_num, &page.data, key_spec.clone())?;

    if node.is_leaf() {
        // Insert into leaf
        let dup_seq = if allow_duplicates {
            // Count existing entries with same key
            node.leaf_entries
                .iter()
                .filter(|e| key_spec.compare(&e.key, &key_value) == std::cmp::Ordering::Equal)
                .count() as u32
        } else {
            0
        };

        let entry = LeafEntry {
            key: key_value.clone(),
            record_address,
            dup_sequence: dup_seq,
        };

        if !node.insert_leaf_entry(entry, allow_duplicates) {
            return Err(BtrieveError::Status(StatusCode::DuplicateKey));
        }

        // Check if split needed
        if node.is_full(page_size) {
            // Allocate new page for split
            let file = engine.files.get(file_path).unwrap();
            let mut f = file.write();
            let new_page_num = f.fcr.num_pages;
            f.fcr.num_pages += 1;
            f.update_fcr()?;
            drop(f);

            let (right_node, separator) = node.split_leaf(new_page_num);

            // Write both nodes
            let f = file.read();
            let left_data = node.to_bytes(page_size);
            let right_data = right_node.to_bytes(page_size);

            let left_page = Page::from_data(page_num, left_data);
            let right_page = Page::from_data(new_page_num, right_data);

            f.write_page(&left_page)?;
            f.write_page(&right_page)?;

            // Update cache with both pages
            let path_str = file_path.to_string_lossy();
            engine.cache.put(&path_str, left_page, false);
            engine.cache.put(&path_str, right_page, false);

            return Ok(Some((separator, new_page_num)));
        } else {
            // Write updated node
            let f = file.read();
            let node_data = node.to_bytes(page_size);
            let page = Page::from_data(page_num, node_data);
            f.write_page(&page)?;

            // Update cache
            engine.cache.put(&file_path.to_string_lossy(), page, false);

            return Ok(None);
        }
    } else {
        // Internal node - find child and recurse
        let child_page = node.find_child(&key_value);

        let result = btree_insert_recursive(
            engine,
            file_path,
            child_page,
            key_spec,
            key_value,
            record_address,
            allow_duplicates,
            page_size,
            session,
        )?;

        // If child split, insert separator into this node
        if let Some((separator, right_child)) = result {
            node.insert_internal_entry(InternalEntry {
                key: separator.clone(),
                child_page: right_child,
            });

            // Check if this node needs to split
            if node.is_full(page_size) {
                let file = engine.files.get(file_path).unwrap();
                let mut f = file.write();
                let new_page_num = f.fcr.num_pages;
                f.fcr.num_pages += 1;
                f.update_fcr()?;
                drop(f);

                let (right_node, promoted_key, _) = node.split_internal(new_page_num);

                let f = file.read();
                let left_data = node.to_bytes(page_size);
                let right_data = right_node.to_bytes(page_size);

                let left_page = Page::from_data(page_num, left_data);
                let right_page = Page::from_data(new_page_num, right_data);

                f.write_page(&left_page)?;
                f.write_page(&right_page)?;

                // Update cache with both pages
                let path_str = file_path.to_string_lossy();
                engine.cache.put(&path_str, left_page, false);
                engine.cache.put(&path_str, right_page, false);

                return Ok(Some((promoted_key, new_page_num)));
            } else {
                let f = file.read();
                let node_data = node.to_bytes(page_size);
                let page = Page::from_data(page_num, node_data);
                f.write_page(&page)?;

                // Update cache
                engine.cache.put(&file_path.to_string_lossy(), page, false);

                return Ok(None);
            }
        }

        Ok(None)
    }
}

/// Operation 2: Insert a new record
pub fn insert(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Track file in transaction if active
    super::transaction_ops::add_file_to_transaction(engine, session, path.clone());

    let file = engine
        .files
        .get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let record_data = &req.data_buffer;
    if record_data.is_empty() {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    // Get file info
    let (page_size, record_length, num_keys, first_data_page, last_data_page) = {
        let f = file.read();
        (
            f.fcr.page_size,
            f.fcr.record_length,
            f.fcr.num_keys as usize,
            f.fcr.first_data_page,
            f.fcr.last_data_page,
        )
    };

    // Validate record length
    if record_data.len() > record_length as usize {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    // Pad record to fixed length
    let mut record = record_data.to_vec();
    record.resize(record_length as usize, 0);

    // Find or create a data page with space
    let record_addr: RecordAddress;

    if first_data_page == 0 {
        // No data pages yet - create first one
        let mut f = file.write();
        let new_page_num = f.fcr.num_pages;

        let mut data_page = DataPage::new(new_page_num, page_size);
        let slot = data_page
            .insert_record(&record)
            .ok_or(BtrieveError::Status(StatusCode::DiskFull))?;

        record_addr = RecordAddress::new(new_page_num, slot);

        // Write data page
        let page = Page::from_data(new_page_num, data_page.to_bytes());
        f.fcr.num_pages += 1;
        f.fcr.first_data_page = new_page_num;
        f.fcr.last_data_page = new_page_num;
        f.fcr.num_records += 1;
        f.update_fcr()?;

        drop(f);
        let f = file.read();
        f.write_page(&page)?;

        // Update cache with new data page
        engine.cache.put(&path.to_string_lossy(), page, false);
    } else {
        // Try to insert into last data page
        let f = file.read();
        let page = f.read_page(last_data_page)?;
        drop(f);

        let mut data_page = DataPage::from_bytes(last_data_page, page.data)?;

        if let Some(slot) = data_page.insert_record(&record) {
            record_addr = RecordAddress::new(last_data_page, slot);

            let f = file.read();
            let page = Page::from_data(last_data_page, data_page.to_bytes());
            f.write_page(&page)?;
            drop(f);

            // Update cache with modified data page
            engine.cache.put(&path.to_string_lossy(), page, false);

            let mut f = file.write();
            f.fcr.num_records += 1;
            f.update_fcr()?;
        } else {
            // Need to allocate new page
            let mut f = file.write();
            let new_page_num = f.fcr.num_pages;

            let mut new_data_page = DataPage::new(new_page_num, page_size);
            let slot = new_data_page
                .insert_record(&record)
                .ok_or(BtrieveError::Status(StatusCode::DiskFull))?;

            record_addr = RecordAddress::new(new_page_num, slot);

            // Link pages
            new_data_page.set_prev_page(last_data_page);

            // Update previous last page to point to new page
            drop(f);

            // Read and update old last page
            let f = file.read();
            let old_page = f.read_page(last_data_page)?;
            drop(f);

            let mut old_data_page = DataPage::from_bytes(last_data_page, old_page.data)?;
            old_data_page.set_next_page(new_page_num);

            let f = file.read();
            let old_page = Page::from_data(last_data_page, old_data_page.to_bytes());
            let new_page = Page::from_data(new_page_num, new_data_page.to_bytes());
            f.write_page(&old_page)?;
            f.write_page(&new_page)?;
            drop(f);

            // Update cache with both pages
            let path_str = path.to_string_lossy();
            engine.cache.put(&path_str, old_page, false);
            engine.cache.put(&path_str, new_page, false);

            let mut f = file.write();
            f.fcr.num_pages += 1;
            f.fcr.last_data_page = new_page_num;
            f.fcr.num_records += 1;
            f.update_fcr()?;
        }
    }

    // Insert into all indexes
    {
        let f = file.read();
        let keys = f.fcr.keys.clone();
        drop(f);

        for (key_num, key_spec) in keys.iter().enumerate() {
            let key_value = key_spec.extract_key(&record);
            let allow_dups = key_spec.allows_duplicates();

            btree_insert(
                engine,
                &path,
                key_num,
                key_value,
                record_addr,
                allow_dups,
                page_size,
                session,
            )?;
        }
    }

    // Lock record if in transaction (Btrieve 5.1 isolation via locks)
    if super::transaction_ops::has_transaction(session) {
        use crate::file_manager::locking::LockType;
        engine.locks.lock_record(
            &path.to_string_lossy(),
            record_addr,
            session,
            LockType::SingleNoWait, // Transaction lock - other sessions blocked
        )?;
    }

    // Build position block with new record position
    let mut cursor = Cursor::new(path.clone(), req.key_number);
    cursor.position(record_addr, Vec::new(), record);
    let position = PositionBlock::from_cursor(&cursor);

    Ok(OperationResponse::success().with_position(position.data.to_vec()))
}

/// Operation 3: Update the current record
pub fn update(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Track file in transaction if active
    super::transaction_ops::add_file_to_transaction(engine, session, path.clone());

    // Restore cursor from position block
    let position = PositionBlock::from_bytes(&req.position_block);
    let cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return Err(BtrieveError::Status(StatusCode::InvalidPositioning));
    }

    let record_addr = cursor
        .record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    // Check record lock
    if engine
        .locks
        .is_record_locked(&path.to_string_lossy(), record_addr, session)
    {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

    let file = engine
        .files
        .get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let page_size = f.fcr.page_size;
    let record_length = f.fcr.record_length;
    let keys = f.fcr.keys.clone();

    // Validate new record data
    let new_record = &req.data_buffer;
    if new_record.len() > record_length as usize {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    // Pad new record
    let mut padded_record = new_record.to_vec();
    padded_record.resize(record_length as usize, 0);

    // Read old record
    let page = f.read_page(record_addr.page)?;
    drop(f);

    let data_page = DataPage::from_bytes(record_addr.page, page.data.clone())?;
    let old_record = data_page
        .get_record(record_addr.slot)
        .ok_or(BtrieveError::Status(StatusCode::InvalidRecordAddress))?
        .to_vec();

    // Check modifiable key constraints and update indexes
    for (key_num, key_spec) in keys.iter().enumerate() {
        let old_key = key_spec.extract_key(&old_record);
        let new_key = key_spec.extract_key(&padded_record);

        if old_key != new_key {
            if !key_spec.is_modifiable() {
                return Err(BtrieveError::Status(StatusCode::ModifiableKeyChanged));
            }

            // Remove old key from index, add new key
            btree_remove(engine, &path, key_num, &old_key, record_addr, page_size, session)?;
            btree_insert(
                engine,
                &path,
                key_num,
                new_key,
                record_addr,
                key_spec.allows_duplicates(),
                page_size,
                session,
            )?;
        }
    }

    // Update record data
    let f = file.read();
    let page = f.read_page(record_addr.page)?;
    drop(f);

    let mut data_page = DataPage::from_bytes(record_addr.page, page.data)?;
    if !data_page.update_record(record_addr.slot, &padded_record) {
        return Err(BtrieveError::Status(StatusCode::IoError));
    }

    // Write and update cache
    let updated_page = Page::from_data(record_addr.page, data_page.to_bytes());
    let f = file.read();
    f.write_page_for_session(&updated_page, session)?;
    drop(f);

    // Update cache with new data
    engine.cache.put(&path.to_string_lossy(), updated_page, false);

    // Lock record if in transaction (Btrieve 5.1 isolation via locks)
    if super::transaction_ops::has_transaction(session) {
        use crate::file_manager::locking::LockType;
        engine.locks.lock_record(
            &path.to_string_lossy(),
            record_addr,
            session,
            LockType::SingleNoWait, // Transaction lock - other sessions blocked
        )?;
    }

    Ok(OperationResponse::success().with_position(req.position_block.clone()))
}

/// Remove a key from the B+ tree
fn btree_remove(
    engine: &Engine,
    file_path: &PathBuf,
    key_number: usize,
    key_value: &[u8],
    record_address: RecordAddress,
    page_size: u16,
    session: SessionId,
) -> BtrieveResult<()> {
    let file = engine
        .files
        .get(file_path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let key_spec = f.fcr.keys[key_number].clone();
    let root_page = f.fcr.index_roots[key_number];
    drop(f);

    if root_page == 0 {
        return Ok(()); // Empty tree
    }

    // Find leaf containing the key
    let mut current_page = root_page;
    loop {
        let f = file.read();
        let page = f.read_page(current_page)?;
        drop(f);

        let mut node = IndexNode::from_bytes(current_page, &page.data, key_spec.clone())?;

        if node.is_leaf() {
            // Remove entry
            if node.remove_leaf_entry(key_value, record_address) {
                let f = file.read();
                let page = Page::from_data(current_page, node.to_bytes(page_size));
                f.write_page_for_session(&page, session)?;

                // Update cache with modified page
                engine.cache.put(&file_path.to_string_lossy(), page, false);
            }
            break;
        } else {
            current_page = node.find_child(key_value);
        }
    }

    // Note: Full B+ tree deletion with rebalancing is complex
    // This simplified version just removes from leaf without rebalancing

    Ok(())
}

/// Operation 4: Delete the current record
pub fn delete(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = get_file_path(&req.position_block)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    // Track file in transaction if active
    super::transaction_ops::add_file_to_transaction(engine, session, path.clone());

    // Restore cursor from position block
    let position = PositionBlock::from_bytes(&req.position_block);
    let mut cursor = position.to_cursor(path.clone());

    if !cursor.is_positioned() {
        return Err(BtrieveError::Status(StatusCode::InvalidPositioning));
    }

    let record_addr = cursor
        .record_address
        .ok_or(BtrieveError::Status(StatusCode::InvalidPositioning))?;

    // Check record lock
    if engine
        .locks
        .is_record_locked(&path.to_string_lossy(), record_addr, session)
    {
        return Err(BtrieveError::Status(StatusCode::RecordInUse));
    }

    let file = engine
        .files
        .get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let page_size = f.fcr.page_size;
    let keys = f.fcr.keys.clone();

    // Read the record to get key values
    let page = f.read_page(record_addr.page)?;
    drop(f);

    let mut data_page = DataPage::from_bytes(record_addr.page, page.data)?;
    let record = data_page
        .get_record(record_addr.slot)
        .ok_or(BtrieveError::Status(StatusCode::InvalidRecordAddress))?
        .to_vec();

    // Remove from all indexes
    for (key_num, key_spec) in keys.iter().enumerate() {
        let key_value = key_spec.extract_key(&record);
        btree_remove(engine, &path, key_num, &key_value, record_addr, page_size, session)?;
    }

    // Mark record as deleted
    data_page.delete_record(record_addr.slot);

    let f = file.read();
    let page = Page::from_data(record_addr.page, data_page.to_bytes());
    f.write_page_for_session(&page, session)?;
    drop(f);

    // Update cache with modified data page
    engine.cache.put(&path.to_string_lossy(), page, false);

    // Update FCR
    let mut f = file.write();
    f.fcr.num_records = f.fcr.num_records.saturating_sub(1);
    f.update_fcr()?;

    // Invalidate cursor
    cursor.invalidate();
    let position = PositionBlock::from_cursor(&cursor);

    Ok(OperationResponse::success().with_position(position.data.to_vec()))
}
