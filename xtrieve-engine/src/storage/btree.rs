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
    /// Header size for index nodes
    pub const HEADER_SIZE: usize = 20;

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
