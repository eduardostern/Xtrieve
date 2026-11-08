//! Crash recovery from Btrieve's preimage (`.PRE`) file.
//!
//! While a transaction is active the engine writes page images to
//! `<file>.PRE` before overwriting the data file.  The FCR word at `+0x2C` is
//! nonzero only while a transaction is active.  Recovery, which the engine
//! performs when it opens such a file, restores every image in the preimage
//! file **in reverse order** and deletes the preimage file.  The physical file
//! length is not changed, so pages allocated by the aborted transaction remain
//! allocated on disk but are unreachable because the restored FCR describes
//! the old page count.
//!
//! Recovery does not clear the "file is open" byte at FCR `+0x70`; it keeps
//! the value from the transaction-start FCR image.  A clean Close zeroes it.
//! Opening a recovered file and closing it therefore produces the fully clean
//! image; the engine itself does exactly this.
//!
//! The transaction log (`/T:` file) is not needed for rollback; the engine
//! leaves it in place.  It is a fixed 1536-byte header block recording the
//! committed file's DOS path.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{FormatError, Result};
use crate::page::{PageSize, PAGE_HEADER_SIZE};

/// FCR offset of the transaction-state word.  The engine writes a nonzero
/// value when a transaction modifies the file and restores zero on commit,
/// abort, recovery, and clean close.
pub const TRANSACTION_STATE_OFFSET: usize = 0x2C;

/// FCR offset of the file-open marker observed while a file is open.  Crash
/// recovery leaves the value from the preimage; a clean Close zeroes it.
pub const FILE_OPEN_MARKER_OFFSET: usize = 0x70;

fn crypt_owner_pages(bytes: &mut [u8], hash: &[u8; 9], page_size: usize) {
    if page_size == 0 {
        return;
    }
    for page in 1..bytes.len() / page_size {
        let start = page * page_size;
        for offset in 0..page_size {
            bytes[start + offset] ^= crate::fcr::owner_mask_byte(hash, offset, page_size);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoveryReport {
    /// Preimage path consulted, if a preimage path was supplied.
    pub preimage: Option<PathBuf>,
    /// The transaction-state word at FCR `+0x2C` was nonzero.
    pub active_transaction: bool,
    /// A preimage was applied and the data file rewritten.
    pub recovered: bool,
    /// Number of page images in the preimage file.
    pub images: usize,
    /// Number of images copied into the data file (page numbers inside it).
    pub applied_pages: usize,
    /// The preimage file was deleted after a successful recovery.
    pub preimage_removed: bool,
}

/// Recover `path` using the conventional sibling preimage file
/// (`test.btr` -> `test.PRE`, with a lowercase `.pre` fallback).
pub fn recover_file(path: impl AsRef<Path>) -> Result<RecoveryReport> {
    let path = path.as_ref();
    let preimage = default_preimage_path(path);
    recover_with_preimage(path, preimage.as_deref())
}

/// Return the conventional preimage path for a data file.  Uppercase `.PRE`
/// is preferred; an existing lowercase `.pre` is also accepted.
pub fn default_preimage_path(path: &Path) -> Option<PathBuf> {
    let upper = path.with_extension("PRE");
    if upper.exists() {
        return Some(upper);
    }
    let lower = path.with_extension("pre");
    if lower.exists() {
        return Some(lower);
    }
    Some(upper)
}

/// Recover `path` with an explicit preimage path (`None` means the
/// conventional sibling file).  When no transaction is active this is a
/// no-op even if a stale preimage exists.
pub fn recover_with_preimage(path: &Path, preimage: Option<&Path>) -> Result<RecoveryReport> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < PAGE_HEADER_SIZE.max(TRANSACTION_STATE_OFFSET + 2) {
        return Err(FormatError::TooSmall);
    }
    let page_size = PageSize::new(u16::from_le_bytes([bytes[8], bytes[9]]))?;
    let page_bytes = page_size.bytes();
    if bytes.len() < page_bytes {
        return Err(FormatError::TooSmall);
    }
    if bytes.len() % page_bytes != 0 {
        return Err(FormatError::InvalidFileSize {
            size: bytes.len(),
            page_size: page_bytes,
        });
    }

    let preimage_path = preimage.map(Path::to_path_buf);
    let active = u16::from_le_bytes([
        bytes[TRANSACTION_STATE_OFFSET],
        bytes[TRANSACTION_STATE_OFFSET + 1],
    ]) != 0;
    if !active {
        return Ok(RecoveryReport {
            preimage: preimage_path,
            active_transaction: false,
            recovered: false,
            images: 0,
            applied_pages: 0,
            preimage_removed: false,
        });
    }

    let preimage_path = preimage_path.ok_or(FormatError::MissingPreimage)?;
    if !preimage_path.exists() {
        return Err(FormatError::MissingPreimage);
    }
    let images = std::fs::read(&preimage_path)?;
    if images.is_empty() || images.len() % page_bytes != 0 {
        return Err(FormatError::InvalidPreimage(
            "size is not a positive multiple of the page size",
        ));
    }

    let mut recovered = bytes;
    // Owner-encrypted files keep plaintext page images in the preimage.  The
    // live file is ciphertext, so decrypt it before applying the images and
    // re-encrypt afterwards.  The FCR (page 0) is never encrypted.
    let encrypted = recovered.len() > 0x38
        && u16::from_le_bytes([recovered[6], recovered[7]]) == 0xFD00
        && recovered[0x37] & 0x02 != 0;
    let hash: Option<[u8; 9]> = recovered
        .get(0x2E..0x37)
        .and_then(|bytes| bytes.try_into().ok());
    if let (true, Some(hash)) = (encrypted, hash) {
        crypt_owner_pages(&mut recovered, &hash, page_bytes);
    }
    let mut applied_pages = 0usize;
    for image in images.chunks_exact(page_bytes).rev() {
        let page = u16::from_le_bytes([image[2], image[3]]) as usize;
        if page == 0xFFFF || page.saturating_mul(page_bytes) + page_bytes > recovered.len() {
            continue;
        }
        let start = page * page_bytes;
        recovered[start..start + page_bytes].copy_from_slice(image);
        applied_pages += 1;
    }
    if let (true, Some(hash)) = (encrypted, hash) {
        crypt_owner_pages(&mut recovered, &hash, page_bytes);
    }
    std::fs::write(path, &recovered)?;
    let preimage_removed = std::fs::remove_file(&preimage_path).is_ok();
    Ok(RecoveryReport {
        preimage: Some(preimage_path),
        active_transaction: true,
        recovered: true,
        images: images.len() / page_bytes,
        applied_pages,
        preimage_removed,
    })
}
