//! B+ Tree implementation for Btrieve 5.1 indexes
//!
//! Each key in a Btrieve file has an associated B+ tree index.
//! The tree stores key values and record addresses (file offsets).
//!
//! Btrieve 5.1 index page format:
//! - Header (16 bytes):
//!   - bytes 0-1: page type (00 00)
//!   - bytes 2-3: page number (u16 LE)
//!   - bytes 4-5: total entries capacity
//!   - bytes 6-7: entry count (u16 LE)
//!   - bytes 8-11: prev sibling page (u32 LE, 0xFFFFFFFF = none)
//!   - bytes 12-15: next sibling page (u32 LE, 0xFFFFFFFF = none)
//! - Entries (16 bytes each):
//!   - bytes 0-3: key value (4 bytes for our test file)
//!   - bytes 4-5: unused
//!   - bytes 6-7: record offset low (u16 LE)
//!   - bytes 8-9: unused
//!   - bytes 10-11: duplicate record offset (u16 LE)
//!   - bytes 12-15: link pointer

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::cmp::Ordering;
use std::io::{self, Cursor};

use super::key::KeySpec;
use super::record::RecordAddress;

/// B+ tree node types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    /// Internal node (contains keys and child pointers)
    Internal,
    /// Leaf node (contains keys and record addresses)
    Leaf,
}

/// Entry in an internal B+ tree node
#[derive(Debug, Clone)]
pub struct InternalEntry {
    /// Key value (separator)
    pub key: Vec<u8>,
    /// Page number of child node
    pub child_page: u32,
}

/// Entry in a leaf B+ tree node
#[derive(Debug, Clone)]
pub struct LeafEntry {
    /// Key value
    pub key: Vec<u8>,
    /// Address of the record
    pub record_address: RecordAddress,
    /// Duplicate sequence number (for duplicate keys)
    pub dup_sequence: u32,
}

/// B+ tree index node
#[derive(Debug, Clone)]
pub struct IndexNode {
    /// Page number of this node
    pub page_number: u32,
    /// Node type (internal or leaf)
    pub node_type: NodeType,
    /// Key specification for this index
    pub key_spec: KeySpec,
    /// Number of entries in the node
    pub entry_count: u16,
    /// Pointer to leftmost child (internal nodes only)
    pub leftmost_child: u32,
    /// Pointer to previous sibling (leaf nodes only)
    pub prev_sibling: u32,
    /// Pointer to next sibling (leaf nodes only)
    pub next_sibling: u32,
    /// Internal entries (if internal node)
    pub internal_entries: Vec<InternalEntry>,
    /// Leaf entries (if leaf node)
    pub leaf_entries: Vec<LeafEntry>,
}

impl IndexNode {
    /// Header size for Btrieve 5.1 index nodes
    pub const HEADER_SIZE: usize = 16;

    /// Entry size in Btrieve 5.1 index pages (16 bytes per entry)
    pub const ENTRY_SIZE: usize = 16;

    /// Parse an index node from page data (Btrieve 5.1 format)
    pub fn from_bytes(
        page_number: u32,
        data: &[u8],
        key_spec: KeySpec,
    ) -> io::Result<Self> {
        if data.len() < Self::HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Index node too short",
            ));
        }

        // Parse Btrieve 5.1 index page header
        // Offset 0-1: page type (usually 00 00)
        // Offset 2-3: page number
        // Offset 4-5: capacity or some other count
        // Offset 6-7: entry count
        // Offset 8-11: prev sibling (0xFFFFFFFF = none)
        // Offset 12-15: next sibling (0xFFFFFFFF = none)
        let entry_count = u16::from_le_bytes([data[6], data[7]]);
        let prev_sibling = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let next_sibling = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);

        // For Btrieve 5.1, assume leaf node (combined index+data pages)
        let node_type = NodeType::Leaf;

        let key_length = key_spec.length as usize;
        let mut leaf_entries = Vec::with_capacity(entry_count as usize);

        // Parse Btrieve 5.1 index entries (16 bytes each, starting at offset 16)
        for i in 0..entry_count as usize {
            let entry_offset = Self::HEADER_SIZE + (i * Self::ENTRY_SIZE);
            if entry_offset + Self::ENTRY_SIZE > data.len() {
                break;
            }

            // Extract key (first 4 bytes for u32 key, or key_length bytes)
            let key_end = entry_offset + key_length.min(4);
            let key = data[entry_offset..key_end].to_vec();

            // Extract record offset (at entry_offset + 6, 2 bytes)
            // This is the absolute file offset to the record data
            let record_offset = u16::from_le_bytes([
                data[entry_offset + 6],
                data[entry_offset + 7],
            ]) as u32;

            // Convert file offset to page/slot address
            // For now, store the raw file offset in the record address
            let record_address = RecordAddress {
                page: 0, // We'll use file_offset instead
                slot: record_offset as u16,
            };

            leaf_entries.push(LeafEntry {
                key,
                record_address,
                dup_sequence: 0,
            });
        }

        Ok(IndexNode {
            page_number,
            node_type,
            key_spec,
            entry_count,
            leftmost_child: 0,
            prev_sibling: if prev_sibling == 0xFFFFFFFF { 0 } else { prev_sibling },
            next_sibling: if next_sibling == 0xFFFFFFFF { 0 } else { next_sibling },
            internal_entries: Vec::new(),
            leaf_entries,
        })
    }

    /// Check if this is a leaf node
    pub fn is_leaf(&self) -> bool {
        self.node_type == NodeType::Leaf
    }

    /// Find the child page for a given key in an internal node
    pub fn find_child(&self, key: &[u8]) -> u32 {
        if self.internal_entries.is_empty() {
            return self.leftmost_child;
        }

        for entry in &self.internal_entries {
            match self.key_spec.compare(key, &entry.key) {
                Ordering::Less => return self.leftmost_child,
                Ordering::Equal | Ordering::Greater => continue,
            }
        }

        // Key is greater than all entries, return rightmost child
        self.internal_entries
            .last()
            .map(|e| e.child_page)
            .unwrap_or(self.leftmost_child)
    }

    /// Search for exact key match in leaf node
    pub fn find_exact(&self, key: &[u8]) -> Option<&LeafEntry> {
        self.leaf_entries
            .iter()
            .find(|e| self.key_spec.compare(&e.key, key) == Ordering::Equal)
    }

    /// Find first entry >= key in leaf node
    pub fn find_ge(&self, key: &[u8]) -> Option<&LeafEntry> {
        self.leaf_entries.iter().find(|e| {
            matches!(
                self.key_spec.compare(&e.key, key),
                Ordering::Equal | Ordering::Greater
            )
        })
    }

    /// Find first entry > key in leaf node
    pub fn find_gt(&self, key: &[u8]) -> Option<&LeafEntry> {
        self.leaf_entries
            .iter()
            .find(|e| self.key_spec.compare(&e.key, key) == Ordering::Greater)
    }

    /// Find last entry <= key in leaf node
    pub fn find_le(&self, key: &[u8]) -> Option<&LeafEntry> {
        self.leaf_entries.iter().rev().find(|e| {
            matches!(
                self.key_spec.compare(&e.key, key),
                Ordering::Equal | Ordering::Less
            )
        })
    }

    /// Find last entry < key in leaf node
    pub fn find_lt(&self, key: &[u8]) -> Option<&LeafEntry> {
        self.leaf_entries
            .iter()
            .rev()
            .find(|e| self.key_spec.compare(&e.key, key) == Ordering::Less)
    }

    /// Get first entry in leaf node
    pub fn first_entry(&self) -> Option<&LeafEntry> {
        self.leaf_entries.first()
    }

    /// Get last entry in leaf node
    pub fn last_entry(&self) -> Option<&LeafEntry> {
        self.leaf_entries.last()
    }

    /// Get entry by index
    pub fn get_entry(&self, index: usize) -> Option<&LeafEntry> {
        self.leaf_entries.get(index)
    }

    /// Find index of entry with matching key
    pub fn find_index(&self, key: &[u8]) -> Option<usize> {
        self.leaf_entries
            .iter()
            .position(|e| self.key_spec.compare(&e.key, key) == Ordering::Equal)
    }

    /// Create a new empty leaf node
    pub fn new_leaf(page_number: u32, key_spec: KeySpec, _page_size: u16) -> Self {
        IndexNode {
            page_number,
            node_type: NodeType::Leaf,
            key_spec,
            entry_count: 0,
            leftmost_child: 0,
            prev_sibling: 0,
            next_sibling: 0,
            internal_entries: Vec::new(),
            leaf_entries: Vec::new(),
        }
    }

    /// Create a new empty internal node
    pub fn new_internal(page_number: u32, key_spec: KeySpec, leftmost_child: u32) -> Self {
        IndexNode {
            page_number,
            node_type: NodeType::Internal,
            key_spec,
            entry_count: 0,
            leftmost_child,
            prev_sibling: 0,
            next_sibling: 0,
            internal_entries: Vec::new(),
            leaf_entries: Vec::new(),
        }
    }

    /// Calculate the size of an entry in bytes
    pub fn entry_size(&self) -> usize {
        Self::ENTRY_SIZE
    }

    /// Calculate how many entries can fit in a page
    pub fn max_entries(&self, page_size: u16) -> usize {
        let available = page_size as usize - Self::HEADER_SIZE;
        available / self.entry_size()
    }

    /// Check if node is full (needs split)
    pub fn is_full(&self, page_size: u16) -> bool {
        let current = match self.node_type {
            NodeType::Internal => self.internal_entries.len(),
            NodeType::Leaf => self.leaf_entries.len(),
        };
        current >= self.max_entries(page_size)
    }

    /// Insert a leaf entry in sorted order
    pub fn insert_leaf_entry(&mut self, entry: LeafEntry, allow_duplicates: bool) -> bool {
        let pos = self.leaf_entries.iter()
            .position(|e| {
                let cmp = self.key_spec.compare(&entry.key, &e.key);
                cmp == Ordering::Less || (cmp == Ordering::Equal && entry.dup_sequence < e.dup_sequence)
            })
            .unwrap_or(self.leaf_entries.len());

        if !allow_duplicates {
            if let Some(existing) = self.leaf_entries.get(pos) {
                if self.key_spec.compare(&entry.key, &existing.key) == Ordering::Equal {
                    return false;
                }
            }
            if pos > 0 {
                if let Some(prev) = self.leaf_entries.get(pos - 1) {
                    if self.key_spec.compare(&entry.key, &prev.key) == Ordering::Equal {
                        return false;
                    }
                }
            }
        }

        self.leaf_entries.insert(pos, entry);
        self.entry_count = self.leaf_entries.len() as u16;
        true
    }

    /// Insert an internal entry in sorted order
    pub fn insert_internal_entry(&mut self, entry: InternalEntry) {
        let pos = self.internal_entries.iter()
            .position(|e| self.key_spec.compare(&entry.key, &e.key) == Ordering::Less)
            .unwrap_or(self.internal_entries.len());

        self.internal_entries.insert(pos, entry);
        self.entry_count = self.internal_entries.len() as u16;
    }

    /// Split a leaf node, returning the new right node and the separator key
    pub fn split_leaf(&mut self, new_page_number: u32) -> (IndexNode, Vec<u8>) {
        let mid = self.leaf_entries.len() / 2;
        let right_entries: Vec<_> = self.leaf_entries.drain(mid..).collect();
        let separator = right_entries.first().unwrap().key.clone();

        let mut right = IndexNode::new_leaf(new_page_number, self.key_spec.clone(), 0);
        right.leaf_entries = right_entries;
        right.entry_count = right.leaf_entries.len() as u16;

        right.prev_sibling = self.page_number;
        right.next_sibling = self.next_sibling;
        self.next_sibling = new_page_number;

        self.entry_count = self.leaf_entries.len() as u16;

        (right, separator)
    }

    /// Split an internal node, returning the new right node and the promoted key
    pub fn split_internal(&mut self, new_page_number: u32) -> (IndexNode, Vec<u8>, u32) {
        let mid = self.internal_entries.len() / 2;

        let promoted = self.internal_entries.remove(mid);
        let right_entries: Vec<_> = self.internal_entries.drain(mid..).collect();

        let mut right = IndexNode::new_internal(new_page_number, self.key_spec.clone(), promoted.child_page);
        right.internal_entries = right_entries;
        right.entry_count = right.internal_entries.len() as u16;

        self.entry_count = self.internal_entries.len() as u16;

        (right, promoted.key, promoted.child_page)
    }

    /// Serialize node to bytes for writing to page (Btrieve 5.1 format)
    pub fn to_bytes(&self, page_size: u16) -> Vec<u8> {
        let mut data = vec![0u8; page_size as usize];

        // Page header (Btrieve 5.1 format)
        data[0] = 0x00; // Page type
        data[1] = 0x00;
        data[2..4].copy_from_slice(&(self.page_number as u16).to_le_bytes());
        data[4..6].copy_from_slice(&0u16.to_le_bytes()); // Capacity
        data[6..8].copy_from_slice(&self.entry_count.to_le_bytes());

        let prev = if self.prev_sibling == 0 { 0xFFFFFFFFu32 } else { self.prev_sibling };
        let next = if self.next_sibling == 0 { 0xFFFFFFFFu32 } else { self.next_sibling };
        data[8..12].copy_from_slice(&prev.to_le_bytes());
        data[12..16].copy_from_slice(&next.to_le_bytes());

        // Entries
        let mut offset = Self::HEADER_SIZE;

        for entry in &self.leaf_entries {
            // Write key (4 bytes)
            let key_len = entry.key.len().min(4);
            data[offset..offset + key_len].copy_from_slice(&entry.key[..key_len]);
            offset += 4;

            // Padding
            data[offset..offset + 2].copy_from_slice(&[0, 0]);
            offset += 2;

            // Record offset (2 bytes)
            data[offset..offset + 2].copy_from_slice(&entry.record_address.slot.to_le_bytes());
            offset += 2;

            // More padding/duplicate pointer
            data[offset..offset + 4].copy_from_slice(&[0, 0, 0, 0]);
            offset += 4;

            // Link pointer
            data[offset..offset + 4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            offset += 4;
        }

        data
    }

    /// Remove a leaf entry by key and record address
    pub fn remove_leaf_entry(&mut self, key: &[u8], record_address: RecordAddress) -> bool {
        if let Some(pos) = self.leaf_entries.iter().position(|e| {
            self.key_spec.compare(&e.key, key) == Ordering::Equal
                && e.record_address == record_address
        }) {
            self.leaf_entries.remove(pos);
            self.entry_count = self.leaf_entries.len() as u16;
            return true;
        }
        false
    }
}

/// B+ tree structure for an index
#[derive(Debug)]
pub struct BTree {
    /// Root page number
    pub root_page: u32,
    /// Key specification
    pub key_spec: KeySpec,
    /// Key number (index number)
    pub key_number: u8,
}

impl BTree {
    /// Create a new B+ tree reference
    pub fn new(root_page: u32, key_spec: KeySpec, key_number: u8) -> Self {
        BTree {
            root_page,
            key_spec,
            key_number,
        }
    }

    /// Check if tree is empty (no root)
    pub fn is_empty(&self) -> bool {
        self.root_page == 0
    }
}

/// Search result from B+ tree traversal
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The leaf node containing (or near) the key
    pub leaf_page: u32,
    /// Index within the leaf node (-1 if not found)
    pub entry_index: i32,
    /// The entry if found
    pub entry: Option<LeafEntry>,
    /// Whether exact match was found
    pub exact_match: bool,
}

impl SearchResult {
    /// Create a not-found result
    pub fn not_found(leaf_page: u32) -> Self {
        SearchResult {
            leaf_page,
            entry_index: -1,
            entry: None,
            exact_match: false,
        }
    }

    /// Create a found result
    pub fn found(leaf_page: u32, entry_index: usize, entry: LeafEntry) -> Self {
        SearchResult {
            leaf_page,
            entry_index: entry_index as i32,
            entry: Some(entry),
            exact_match: true,
        }
    }

    /// Create an approximate result (for range queries)
    pub fn approximate(leaf_page: u32, entry_index: usize, entry: LeafEntry) -> Self {
        SearchResult {
            leaf_page,
            entry_index: entry_index as i32,
            entry: Some(entry),
            exact_match: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::key::{KeyFlags, KeyType};

    fn test_key_spec() -> KeySpec {
        KeySpec {
            position: 0,
            length: 4,
            flags: KeyFlags::DUPLICATES,
            key_type: KeyType::UnsignedBinary,
            null_value: 0,
            acs_number: 0,
            unique_count: 0,
        }
    }

    #[test]
    fn test_parse_btrieve51_index() {
        // Simulate a Btrieve 5.1 index page with 2 entries
        let mut data = vec![0u8; 1024];

        // Header
        data[6] = 2; // entry_count = 2
        data[8..12].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // prev = none
        data[12..16].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // next = none

        // Entry 1: key=100, offset=0x0806
        data[16..20].copy_from_slice(&100u32.to_le_bytes());
        data[22..24].copy_from_slice(&0x0806u16.to_le_bytes());

        // Entry 2: key=200, offset=0x084E
        data[32..36].copy_from_slice(&200u32.to_le_bytes());
        data[38..40].copy_from_slice(&0x084Eu16.to_le_bytes());

        let key_spec = test_key_spec();
        let node = IndexNode::from_bytes(1, &data, key_spec).unwrap();

        assert_eq!(node.entry_count, 2);
        assert_eq!(node.leaf_entries.len(), 2);
        assert_eq!(node.leaf_entries[0].record_address.slot, 0x0806);
        assert_eq!(node.leaf_entries[1].record_address.slot, 0x084E);
    }
}
