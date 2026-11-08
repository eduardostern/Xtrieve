//! On-disk transaction artifacts: the FCR marker, `.PRE` images, and the
//! fixed 1536-byte `/T:` transaction log.

use std::path::{Path, PathBuf};

use btr_format::BtrieveFile;

pub const MARKER_OFFSET: usize = 0x2C;
pub const OPEN_MARKER_OFFSET: usize = 0x70;

pub fn preimage_path(path: &Path) -> PathBuf {
    path.with_extension("PRE")
}

/// Set or clear the transaction-state word in a data-file image.
pub fn set_marker(bytes: &mut [u8], active: bool) {
    if bytes.len() >= MARKER_OFFSET + 2 {
        let value: u16 = if active { 4 } else { 0 };
        bytes[MARKER_OFFSET..MARKER_OFFSET + 2].copy_from_slice(&value.to_le_bytes());
    }
}

/// The engine mirrors the transaction marker at +0x2C and +0x70.  Its value
/// is the number of pages in the preimage file.
pub fn set_transaction_markers(bytes: &mut [u8], value: u16) {
    if bytes.len() >= MARKER_OFFSET + 2 {
        bytes[MARKER_OFFSET..MARKER_OFFSET + 2].copy_from_slice(&value.to_le_bytes());
    }
    if bytes.len() >= OPEN_MARKER_OFFSET + 2 {
        bytes[OPEN_MARKER_OFFSET..OPEN_MARKER_OFFSET + 2].copy_from_slice(&value.to_le_bytes());
    }
}

/// End/Abort clear the active transaction word at +0x2C.  The journal page
/// count at +0x70 survives until the last handle is closed.
pub fn clear_transaction_marker(bytes: &mut [u8]) {
    if bytes.len() >= MARKER_OFFSET + 2 {
        bytes[MARKER_OFFSET..MARKER_OFFSET + 2].fill(0);
    }
}

/// Set a page's aux word, preserving the data-page flag.  `index` is the
/// zero-based file page number (page 0 is the FCR and is never touched).
pub fn set_page_aux(bytes: &mut [u8], index: usize, page_size: usize, aux: u16) {
    let offset = index * page_size + 4;
    if offset + 2 > bytes.len() {
        return;
    }
    let flags = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
    let value = if flags & 0x8000 != 0 {
        0x8000 | aux
    } else {
        aux
    };
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub fn page_is_data(bytes: &[u8], index: usize, page_size: usize) -> bool {
    let offset = index * page_size + 4;
    offset + 2 <= bytes.len()
        && u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) & 0x8000 != 0
}

pub fn changed_pages(old: &[u8], new: &[u8], page_size: usize) -> Vec<usize> {
    let pages = old.len().max(new.len()).div_ceil(page_size);
    (1..pages)
        .filter(|index| page_image(old, *index, page_size) != page_image(new, *index, page_size))
        .collect()
}

/// Changed pages ignoring the header aux word.  A transaction pins every
/// touched page's aux to the same value, so a rebuilt page whose contents are
/// unchanged must not be treated as an operation image.
fn changed_content_pages(old: &[u8], new: &[u8], page_size: usize) -> Vec<usize> {
    let pages = old.len().max(new.len()).div_ceil(page_size);
    (1..pages)
        .filter(|index| {
            let mut left = page_image(old, *index, page_size);
            let mut right = page_image(new, *index, page_size);
            if left.len() >= 6 && right.len() >= 6 {
                left[4..6].fill(0);
                right[4..6].fill(0);
            }
            left != right
        })
        .collect()
}

fn page_image(bytes: &[u8], index: usize, page_size: usize) -> Vec<u8> {
    let start = index * page_size;
    let mut image = vec![0u8; page_size];
    if start < bytes.len() {
        let end = (start + page_size).min(bytes.len());
        image[..end - start].copy_from_slice(&bytes[start..end]);
    }
    image
}

/// Which per-operation images the DOS engine appends to the preimage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    Insert,
    Update,
    Delete,
}

/// Build the operation-specific page-image group.  All images carry the
/// supplied aux value (the transaction counter inside a transaction, the
/// resulting FCR aux otherwise).
#[allow(clippy::too_many_arguments)]
pub fn operation_image_group(
    old: &[u8],
    new: &[u8],
    page_size: usize,
    kind: MutationKind,
    aux: u16,
    old_file: &BtrieveFile,
    selected_key: Option<usize>,
) -> Vec<u8> {
    let changed = changed_content_pages(old, new, page_size);
    let mut data = Vec::new();
    let mut index = Vec::new();
    for page in changed {
        if page_is_data(new, page, page_size) || page_is_data(old, page, page_size) {
            data.push(page);
        } else {
            index.push(page);
        }
    }
    let mut images: Vec<Vec<u8>> = Vec::new();
    let mut push = |bytes: &[u8], page: usize| {
        let mut image = page_image(bytes, page, page_size);
        let flags = if page_is_data(bytes, page, page_size) {
            0x8000 | aux
        } else {
            aux
        };
        image[4..6].copy_from_slice(&flags.to_le_bytes());
        images.push(image);
    };
    match kind {
        MutationKind::Insert => {
            for page in &data {
                // A newly allocated data page has no pre-transaction image.
                if page * page_size < old.len() {
                    push(old, *page);
                }
            }
            let mut target_pages = Vec::new();
            for page in &index {
                let existing = old_file
                    .index_pages
                    .iter()
                    .find(|candidate| candidate.page_number == *page as u32);
                let key_index = existing.map(|candidate| candidate.key_index);
                let selected = selected_key.is_none() || key_index == selected_key;
                let start = page_size.saturating_mul(*page);
                let is_leaf = page_size != 0
                    && start + 12 <= new.len()
                    && new[start + 8..start + 12] == [0xFF, 0xFF, 0xFF, 0xFF];
                if existing.is_some() && selected && is_leaf {
                    target_pages.push(*page);
                }
            }
            // The target leaf's old image comes first, followed by the other
            // modified index pages.  Only that leaf gets a new-image entry.
            let mut ordered_index = target_pages.clone();
            for page in &index {
                if !target_pages.contains(page) {
                    ordered_index.push(*page);
                }
            }
            for page in &ordered_index {
                if page * page_size < old.len() {
                    push(old, *page);
                }
            }
            for page in &target_pages {
                push(new, *page);
            }
        }
        MutationKind::Update => {
            // A data-only update logs the old data page.  A key-changing
            // update additionally logs the old and new index pages.
            for page in &data {
                push(old, *page);
            }
            for page in &index {
                if page * page_size < old.len() {
                    push(old, *page);
                }
            }
            for page in &index {
                if page * page_size < old.len() && page * page_size < new.len() {
                    push(new, *page);
                }
            }
        }
        MutationKind::Delete => {
            for page in data.iter().chain(index.iter()) {
                if page * page_size < old.len() {
                    push(old, *page);
                }
            }
        }
    }
    let mut group = Vec::with_capacity(images.len() * page_size);
    for image in &images {
        group.extend_from_slice(image);
    }
    group
}

/// Append an operation image group to an existing transaction preimage.
#[allow(clippy::too_many_arguments)]
pub fn append_operation_images(
    path: &Path,
    old: &[u8],
    new: &[u8],
    page_size: usize,
    kind: MutationKind,
    aux: u16,
    old_file: &BtrieveFile,
    selected_key: Option<usize>,
) -> std::io::Result<usize> {
    use std::io::Write;
    let group = operation_image_group(old, new, page_size, kind, aux, old_file, selected_key);
    if group.is_empty() {
        return Ok(0);
    }
    let images = group.len() / page_size;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(preimage_path(path))?;
    file.write_all(&group)?;
    Ok(images)
}

pub fn clear_open_marker(bytes: &mut [u8]) {
    if bytes.len() >= OPEN_MARKER_OFFSET + 2 {
        bytes[OPEN_MARKER_OFFSET..OPEN_MARKER_OFFSET + 2].fill(0);
    }
}

/// Write the transaction-start FCR image.  The engine validates this image's
/// operation counter against the live FCR, so the transaction counter is
/// patched in.
pub fn start_preimage(
    path: &Path,
    snapshot: &[u8],
    page_size: usize,
    operation_counter: u16,
) -> std::io::Result<()> {
    let mut image = vec![0u8; page_size];
    let copy = snapshot.len().min(page_size);
    image[..copy].copy_from_slice(&snapshot[..copy]);
    image[4..6].copy_from_slice(&operation_counter.to_le_bytes());
    std::fs::write(preimage_path(path), image)
}

/// Append one operation's page-image group: the old image of every changed
/// page followed by its new image.  Recovery applies the log in reverse, so
/// the oldest image of each page wins.
pub fn append_preimage_group(
    path: &Path,
    old: &[u8],
    new: &[u8],
    page_size: usize,
) -> std::io::Result<usize> {
    use std::io::Write;
    let pages = old.len().max(new.len()).div_ceil(page_size);
    let page = |bytes: &[u8], index: usize| -> Vec<u8> {
        let start = index * page_size;
        if start >= bytes.len() {
            return vec![0u8; page_size];
        }
        let end = (start + page_size).min(bytes.len());
        let mut image = vec![0u8; page_size];
        image[..end - start].copy_from_slice(&bytes[start..end]);
        image
    };
    let changed: Vec<usize> = (1..pages)
        .filter(|index| page(old, *index) != page(new, *index))
        .collect();
    if changed.is_empty() {
        return Ok(0);
    }
    let mut group = Vec::with_capacity(changed.len() * 2 * page_size);
    for index in &changed {
        group.extend_from_slice(&page(old, *index));
    }
    for index in &changed {
        group.extend_from_slice(&page(new, *index));
    }
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(preimage_path(path))?;
    file.write_all(&group)?;
    Ok(changed.len() * 2)
}

pub fn remove_preimage(path: &Path) {
    let _ = std::fs::remove_file(preimage_path(path));
}

/// Write (or clear) the transaction log.  The engine's log is a 1536-byte
/// header block regardless of the data file's page size.
pub fn write_log(
    log_path: &Path,
    data_path: &Path,
    current_dir: Option<&Path>,
    committed: bool,
) -> std::io::Result<()> {
    let mut buffer = vec![0u8; 1536];
    if committed {
        buffer[2..4].copy_from_slice(&1u16.to_le_bytes());
        let relative = current_dir
            .and_then(|base| data_path.strip_prefix(base).ok())
            .unwrap_or_else(|| data_path.file_name().map(Path::new).unwrap_or(data_path));
        let mut name = String::from("d:\\\\");
        name.push_str(&relative.to_string_lossy().replace('/', "\\"));
        let bytes = name.as_bytes();
        let length = bytes.len().min(63);
        buffer[0x0E..0x0E + length].copy_from_slice(&bytes[..length]);
        if length < 64 {
            buffer[0x0E + length] = 0;
        }
        buffer[0x4E] = 1;
    }
    if let Some(parent) = log_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(log_path, buffer)
}

/// Ensure the transaction log exists at Begin.  The TSR creates it at loader
/// time and Begin leaves an existing log untouched.
pub fn reset_log(log_path: &Path) -> std::io::Result<()> {
    if let Some(parent) = log_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    // The TSR creates the /T: log at load time; Begin only ensures it exists
    // and never truncates a log left by an earlier transaction.
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(log_path)?;
    Ok(())
}
