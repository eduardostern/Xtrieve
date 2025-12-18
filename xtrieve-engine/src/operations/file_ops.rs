//! File operations: Open, Close, Create, Stat

use std::path::PathBuf;

use crate::error::{BtrieveError, BtrieveResult, StatusCode};
use crate::file_manager::cursor::PositionBlock;
use crate::file_manager::locking::SessionId;
use crate::file_manager::open_files::OpenMode;
use crate::storage::fcr::FileControlRecord;
use crate::storage::key::{KeySpec, KeyFlags, KeyType};

use super::dispatcher::{Engine, OperationRequest, OperationResponse};

/// Operation 0: Open a Btrieve file
pub fn open(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = req.file_path.as_ref()
        .ok_or(BtrieveError::Status(StatusCode::InvalidFileName))?;

    let mode = OpenMode::from_raw(req.open_mode);
    let path = PathBuf::from(path);

    // Open the file
    let file = engine.files.open(&path, mode)?;

    // Create position block for this file
    let mut position = PositionBlock::new();
    // Store a reference to the file path in the position block
    let path_str = path.to_string_lossy();
    let path_bytes = path_str.as_bytes();
    let len = path_bytes.len().min(64);
    position.data[64..64 + len].copy_from_slice(&path_bytes[..len]);

    // Acquire file lock
    engine.locks.lock_file(
        &path.to_string_lossy(),
        session,
        mode.exclusive,
    )?;

    Ok(OperationResponse::success()
        .with_position(position.data.to_vec()))
}

/// Operation 1: Close a Btrieve file
pub fn close(
    engine: &Engine,
    session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // Get file path from position block or request
    let path = if let Some(ref p) = req.file_path {
        PathBuf::from(p)
    } else if !req.position_block.is_empty() {
        // Extract path from position block (stored at offset 64)
        let end = req.position_block[64..].iter()
            .position(|&b| b == 0)
            .unwrap_or(64);
        let path_str = String::from_utf8_lossy(&req.position_block[64..64 + end]);
        PathBuf::from(path_str.as_ref())
    } else {
        return Err(BtrieveError::Status(StatusCode::FileNotOpen));
    };

    // Release locks
    engine.locks.unlock_all_records(&path.to_string_lossy(), session);
    engine.locks.unlock_file(&path.to_string_lossy(), session);

    // Flush and close
    if let Some(file) = engine.files.get(&path) {
        // Flush dirty pages for this file
        let dirty = engine.cache.invalidate_file(&path.to_string_lossy());
        {
            let f = file.read();
            for page in dirty {
                let _ = f.write_page(&page);
            }
        }
    }

    engine.files.close(&path)?;

    Ok(OperationResponse::success())
}

/// Operation 14: Create a new Btrieve file
pub fn create(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    let path = req.file_path.as_ref()
        .ok_or(BtrieveError::Status(StatusCode::InvalidFileName))?;

    // Parse file specification from data buffer
    // Btrieve 5.x format:
    //   0-1:   record_length
    //   2-3:   page_size
    //   4-5:   num_keys
    //   6-7:   unused
    //   8-11:  file_flags
    //   12-13: reserved
    //   14-15: preallocation
    //   16+:   key specs (16 bytes each)
    if req.data_buffer.len() < 16 {
        return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
    }

    let record_length = u16::from_le_bytes([req.data_buffer[0], req.data_buffer[1]]);
    let page_size = u16::from_le_bytes([req.data_buffer[2], req.data_buffer[3]]);
    let num_keys = u16::from_le_bytes([req.data_buffer[4], req.data_buffer[5]]);

    // Validate page size
    if !crate::storage::page::PAGE_SIZES.contains(&page_size) {
        return Err(BtrieveError::Status(StatusCode::PageSizeError));
    }

    // Validate record length
    if record_length == 0 || record_length > page_size - 20 {
        return Err(BtrieveError::Status(StatusCode::InvalidRecordLength));
    }

    // Parse key specifications (start at offset 16 in Btrieve 5.x)
    let mut keys = Vec::with_capacity(num_keys as usize);
    let mut offset = 16;

    for _ in 0..num_keys {
        if offset + 16 > req.data_buffer.len() {
            return Err(BtrieveError::Status(StatusCode::DataBufferTooShort));
        }

        let key = KeySpec::from_bytes(&req.data_buffer[offset..])?;

        // Validate key
        if key.position + key.length > record_length {
            return Err(BtrieveError::Status(StatusCode::InvalidKeyPosition));
        }
        if key.length == 0 || key.length > 255 {
            return Err(BtrieveError::Status(StatusCode::InvalidKeyLength));
        }

        keys.push(key);
        offset += 16;
    }

    // Create FCR
    let fcr = FileControlRecord::new(record_length, page_size, keys);

    // Create the file
    let path = PathBuf::from(path);
    engine.files.create(&path, fcr)?;

    Ok(OperationResponse::success())
}

/// Operation 15: Get file statistics
pub fn stat(
    engine: &Engine,
    _session: SessionId,
    req: &OperationRequest,
) -> BtrieveResult<OperationResponse> {
    // Get file from position block
    let path = if let Some(ref p) = req.file_path {
        PathBuf::from(p)
    } else if !req.position_block.is_empty() {
        let end = req.position_block[64..].iter()
            .position(|&b| b == 0)
            .unwrap_or(64);
        let path_str = String::from_utf8_lossy(&req.position_block[64..64 + end]);
        PathBuf::from(path_str.as_ref())
    } else {
        return Err(BtrieveError::Status(StatusCode::FileNotOpen));
    };

    let file = engine.files.get(&path)
        .ok_or(BtrieveError::Status(StatusCode::FileNotOpen))?;

    let f = file.read();
    let fcr = &f.fcr;

    // Build stat buffer
    // Format matches Btrieve stat return:
    // record_length (2), page_size (2), num_keys (2), num_records (4),
    // flags (2), unused_pages (2), then key specs
    let mut buffer = Vec::with_capacity(256);

    buffer.extend_from_slice(&fcr.record_length.to_le_bytes());
    buffer.extend_from_slice(&fcr.page_size.to_le_bytes());
    buffer.extend_from_slice(&fcr.num_keys.to_le_bytes());
    buffer.extend_from_slice(&fcr.num_records.to_le_bytes());
    buffer.extend_from_slice(&fcr.flags.bits().to_le_bytes());
    buffer.extend_from_slice(&fcr.unused_pages.to_le_bytes());

    // Add key specifications
    for key in &fcr.keys {
        buffer.extend_from_slice(&key.to_bytes());
    }

    Ok(OperationResponse::success().with_data(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_mode_parsing() {
        let mode = OpenMode::from_raw(0);
        assert!(!mode.read_only);
        assert!(!mode.exclusive);

        let mode = OpenMode::from_raw(-2i32 as i32);
        // Note: This test depends on exact bit patterns
    }
}
