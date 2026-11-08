//! Key-ordered record views used by Get and Step operations.

use std::cmp::Ordering;

use btr_format::{Acs, BtrieveFile, FileSpec};

use crate::status::{Status, INVALID_KEY_NUMBER, INVALID_KEY_POSITION, IO_ERROR};

#[derive(Debug, Clone)]
pub struct ViewEntry {
    pub address: u32,
    pub key: Vec<u8>,
    pub data: Vec<u8>,
}

pub fn build_view(
    file: &BtrieveFile,
    spec: &FileSpec,
    key_number: u16,
) -> Result<Vec<ViewEntry>, Status> {
    let key_index = key_number as usize;
    let key = spec.keys.get(key_index).ok_or(INVALID_KEY_NUMBER)?;
    if key.length == 0 {
        return Err(INVALID_KEY_NUMBER);
    }
    let records = file.records().map_err(|_| IO_ERROR)?;
    let mut view = Vec::with_capacity(records.len());
    for record in records {
        if key.is_excluded_from_index(&record.data) {
            continue;
        }
        let value = key
            .build_key(&record.data)
            .map_err(|_| INVALID_KEY_POSITION)?;
        view.push(ViewEntry {
            address: record.address,
            key: value,
            data: record.data,
        });
    }
    let acs = spec.acs.as_ref();
    view.sort_by(|left, right| {
        key.compare(&left.key, &right.key, acs)
            .then_with(|| left.address.cmp(&right.address))
    });
    Ok(view)
}

/// Physical-order view used by Step operations on data-only files.
pub fn build_physical_view(file: &BtrieveFile) -> Result<Vec<ViewEntry>, Status> {
    let mut records = file.records().map_err(|_| IO_ERROR)?;
    records.sort_by_key(|record| record.address);
    Ok(records
        .into_iter()
        .map(|record| ViewEntry {
            address: record.address,
            key: Vec::new(),
            data: record.data,
        })
        .collect())
}

/// First index whose key is >= `key`.
pub fn lower_bound(view: &[ViewEntry], key: &[u8], spec: &FileSpec, key_number: u16) -> usize {
    let definition = &spec.keys[key_number as usize];
    let acs = spec.acs.as_ref();
    view.partition_point(|entry| definition.compare(&entry.key, key, acs) == Ordering::Less)
}

/// First index whose key is > `key`.
pub fn upper_bound(view: &[ViewEntry], key: &[u8], spec: &FileSpec, key_number: u16) -> usize {
    let definition = &spec.keys[key_number as usize];
    let acs = spec.acs.as_ref();
    view.partition_point(|entry| definition.compare(&entry.key, key, acs) != Ordering::Greater)
}

fn tuple_cmp(
    entry: &ViewEntry,
    key: &[u8],
    address: u32,
    spec: &FileSpec,
    key_number: u16,
) -> Ordering {
    let definition = &spec.keys[key_number as usize];
    definition
        .compare(&entry.key, key, spec.acs.as_ref())
        .then_with(|| entry.address.cmp(&address))
}

/// First index strictly after the `(key, address)` cursor tuple.
pub fn first_after(
    view: &[ViewEntry],
    key: &[u8],
    address: u32,
    spec: &FileSpec,
    key_number: u16,
) -> usize {
    view.partition_point(|entry| {
        tuple_cmp(entry, key, address, spec, key_number) != Ordering::Greater
    })
}

/// Last index strictly before the `(key, address)` cursor tuple.
pub fn last_before(
    view: &[ViewEntry],
    key: &[u8],
    address: u32,
    spec: &FileSpec,
    key_number: u16,
) -> Option<usize> {
    let count = view.partition_point(|entry| {
        tuple_cmp(entry, key, address, spec, key_number) == Ordering::Less
    });
    count.checked_sub(1)
}

pub fn acs(spec: &FileSpec) -> Option<&Acs> {
    spec.acs.as_ref()
}
