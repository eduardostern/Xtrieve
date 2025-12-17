//! B+ Tree implementation for Btrieve indexes
//!
//! Each key in a Btrieve file has an associated B+ tree index.
//! The tree stores key values and record addresses.

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
    /// Header size for index nodes (page_type + flags + entry_count + leftmost + prev + next)
    pub const HEADER_SIZE: usize = 16;

    /// Parse an index node from page data
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

        let mut cursor = Cursor::new(data);

        // Page header
        let _page_type = cursor.read_u8()?;
        let flags = cursor.read_u8()?;
        let entry_count = cursor.read_u16::<LittleEndian>()?;

        let node_type = if flags & 0x01 != 0 {
            NodeType::Leaf
        } else {
            NodeType::Internal
        };

        // Node-specific header
        let leftmost_child = cursor.read_u32::<LittleEndian>()?;
        let prev_sibling = cursor.read_u32::<LittleEndian>()?;
        let next_sibling = cursor.read_u32::<LittleEndian>()?;

        let key_length = key_spec.length as usize;
        let mut internal_entries = Vec::new();
        let mut leaf_entries = Vec::new();

        match node_type {
            NodeType::Internal => {
                // Internal node: key + child pointer pairs
                let entry_size = key_length + 4; // key + u32 child pointer

                for _ in 0..entry_count {
                    let pos = cursor.position() as usize;
                    if pos + entry_size > data.len() {
                        break;
                    }

                    let key = data[pos..pos + key_length].to_vec();
                    cursor.set_position((pos + key_length) as u64);
                    let child_page = cursor.read_u32::<LittleEndian>()?;

                    internal_entries.push(InternalEntry { key, child_page });
                }
            }
            NodeType::Leaf => {
                // Leaf node: key + record address + dup sequence
                let entry_size = key_length + 6 + 4; // key + 6-byte addr + 4-byte dup

                for _ in 0..entry_count {
                    let pos = cursor.position() as usize;
                    if pos + entry_size > data.len() {
                        break;
                    }

                    let key = data[pos..pos + key_length].to_vec();
                    cursor.set_position((pos + key_length) as u64);

                    let record_address = RecordAddress::from_bytes(
                        &data[cursor.position() as usize..],
                    )?;
                    cursor.set_position(cursor.position() + 6);

                    let dup_sequence = cursor.read_u32::<LittleEndian>()?;

                    leaf_entries.push(LeafEntry {
                        key,
                        record_address,
                        dup_sequence,
                    });
                }
            }
        }

        Ok(IndexNode {
            page_number,
            node_type,
            key_spec,
            entry_count,
            leftmost_child,
            prev_sibling,
            next_sibling,
            internal_entries,
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
    pub fn new_leaf(page_number: u32, key_spec: KeySpec, page_size: u16) -> Self {
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
        let key_len = self.key_spec.length as usize;
        match self.node_type {
            NodeType::Internal => key_len + 4, // key + child pointer
            NodeType::Leaf => key_len + 6 + 4, // key + record addr + dup sequence
        }
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
    /// Returns true if inserted, false if duplicate (when not allowed)
    pub fn insert_leaf_entry(&mut self, entry: LeafEntry, allow_duplicates: bool) -> bool {
        // Find insertion position
        let pos = self.leaf_entries.iter()
            .position(|e| {
                let cmp = self.key_spec.compare(&entry.key, &e.key);
                cmp == Ordering::Less || (cmp == Ordering::Equal && entry.dup_sequence < e.dup_sequence)
            })
            .unwrap_or(self.leaf_entries.len());

        // Check for duplicates
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

        // Update sibling pointers
        right.prev_sibling = self.page_number;
        right.next_sibling = self.next_sibling;
        self.next_sibling = new_page_number;

        self.entry_count = self.leaf_entries.len() as u16;

        (right, separator)
    }

    /// Split an internal node, returning the new right node and the promoted key
    pub fn split_internal(&mut self, new_page_number: u32) -> (IndexNode, Vec<u8>, u32) {
        let mid = self.internal_entries.len() / 2;

        // The middle entry's key gets promoted, its child becomes leftmost of right node
        let promoted = self.internal_entries.remove(mid);
        let right_entries: Vec<_> = self.internal_entries.drain(mid..).collect();

        let mut right = IndexNode::new_internal(new_page_number, self.key_spec.clone(), promoted.child_page);
        right.internal_entries = right_entries;
        right.entry_count = right.internal_entries.len() as u16;

        self.entry_count = self.internal_entries.len() as u16;

        (right, promoted.key, promoted.child_page)
    }

    /// Serialize node to bytes for writing to page
    pub fn to_bytes(&self, page_size: u16) -> Vec<u8> {
        let mut data = vec![0u8; page_size as usize];

        // Page header
        data[0] = 0x03; // Index page type
        data[1] = if self.is_leaf() { 0x01 } else { 0x00 }; // Flags
        data[2..4].copy_from_slice(&self.entry_count.to_le_bytes());
        data[4..8].copy_from_slice(&self.leftmost_child.to_le_bytes());
        data[8..12].copy_from_slice(&self.prev_sibling.to_le_bytes());
        data[12..16].copy_from_slice(&self.next_sibling.to_le_bytes());

        let key_len = self.key_spec.length as usize;
        let mut offset = Self::HEADER_SIZE;

        match self.node_type {
            NodeType::Internal => {
                for entry in &self.internal_entries {
                    // Write key (padded to key_len)
                    let key_bytes = &entry.key;
                    let copy_len = key_bytes.len().min(key_len);
                    data[offset..offset + copy_len].copy_from_slice(&key_bytes[..copy_len]);
                    offset += key_len;

                    // Write child page
                    data[offset..offset + 4].copy_from_slice(&entry.child_page.to_le_bytes());
                    offset += 4;
                }
            }
            NodeType::Leaf => {
                for entry in &self.leaf_entries {
                    // Write key (padded to key_len)
                    let key_bytes = &entry.key;
                    let copy_len = key_bytes.len().min(key_len);
                    data[offset..offset + copy_len].copy_from_slice(&key_bytes[..copy_len]);
                    offset += key_len;

                    // Write record address (6 bytes)
                    let addr_bytes = entry.record_address.to_bytes();
                    data[offset..offset + 6].copy_from_slice(&addr_bytes);
                    offset += 6;

                    // Write dup sequence
                    data[offset..offset + 4].copy_from_slice(&entry.dup_sequence.to_le_bytes());
                    offset += 4;
                }
            }
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
            length: 10,
            flags: KeyFlags::empty(),
            key_type: KeyType::String,
            null_value: 0,
            acs_number: 0,
            unique_count: 0,
        }
    }

    #[test]
    fn test_leaf_entry_search() {
        let key_spec = test_key_spec();
        let node = IndexNode {
            page_number: 1,
            node_type: NodeType::Leaf,
            key_spec: key_spec.clone(),
            entry_count: 3,
            leftmost_child: 0,
            prev_sibling: 0,
            next_sibling: 0,
            internal_entries: vec![],
            leaf_entries: vec![
                LeafEntry {
                    key: b"AAA       ".to_vec(),
                    record_address: RecordAddress::new(2, 0),
                    dup_sequence: 0,
                },
                LeafEntry {
                    key: b"BBB       ".to_vec(),
                    record_address: RecordAddress::new(2, 1),
                    dup_sequence: 0,
                },
                LeafEntry {
                    key: b"CCC       ".to_vec(),
                    record_address: RecordAddress::new(2, 2),
                    dup_sequence: 0,
                },
            ],
        };

        // Test exact match
        let found = node.find_exact(b"BBB       ");
        assert!(found.is_some());
        assert_eq!(found.unwrap().record_address.slot, 1);

        // Test not found
        let not_found = node.find_exact(b"DDD       ");
        assert!(not_found.is_none());

        // Test find_ge
        let ge = node.find_ge(b"BBB       ");
        assert!(ge.is_some());
        assert_eq!(ge.unwrap().record_address.slot, 1);

        // Test find_gt
        let gt = node.find_gt(b"BBB       ");
        assert!(gt.is_some());
        assert_eq!(gt.unwrap().record_address.slot, 2);
    }
}
