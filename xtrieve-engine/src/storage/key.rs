//! Key specification and key type handling for Btrieve indexes
//!
//! Btrieve supports up to 24 keys (indexes) per file, each with specific
//! type information and flags. Keys can be simple or segmented (compound).

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::cmp::Ordering;
use std::io::{self, Cursor};

/// Key data types supported by Btrieve 5.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyType {
    /// ASCII string, case-sensitive binary comparison
    String = 0,
    /// Signed integer (1, 2, 4, or 8 bytes)
    Integer = 1,
    /// IEEE floating point (4 or 8 bytes)
    Float = 2,
    /// Date (4 bytes: YYYYMMDD)
    Date = 3,
    /// Time (4 bytes: HHMMSSCC)
    Time = 4,
    /// Packed decimal (BCD)
    Decimal = 5,
    /// Currency/money (8 bytes)
    Money = 6,
    /// Logical/boolean (1 or 2 bytes)
    Logical = 7,
    /// ASCII numeric string
    Numeric = 8,
    /// Btrieve float format
    BFloat = 9,
    /// Length-prefixed string (first byte is length)
    LString = 10,
    /// Null-terminated string
    ZString = 11,
    /// Unsigned binary integer
    UnsignedBinary = 14,
    /// Auto-incrementing integer
    AutoIncrement = 15,
}

impl KeyType {
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => KeyType::String,
            1 => KeyType::Integer,
            2 => KeyType::Float,
            3 => KeyType::Date,
            4 => KeyType::Time,
            5 => KeyType::Decimal,
            6 => KeyType::Money,
            7 => KeyType::Logical,
            8 => KeyType::Numeric,
            9 => KeyType::BFloat,
            10 => KeyType::LString,
            11 => KeyType::ZString,
            14 => KeyType::UnsignedBinary,
            15 => KeyType::AutoIncrement,
            _ => KeyType::String, // Default to string for unknown types
        }
    }
}

bitflags::bitflags! {
    /// Key flags that modify key behavior
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct KeyFlags: u16 {
        /// Allow duplicate key values
        const DUPLICATES = 0x0001;
        /// Key value can be modified with Update operation
        const MODIFIABLE = 0x0002;
        /// Binary key (obsolete in 5.1, use key types instead)
        const BINARY = 0x0004;
        /// Null key values allowed (all-null segments)
        const NULL = 0x0008;
        /// This is a segment of a compound key
        const SEGMENTED = 0x0010;
        /// Use alternate collating sequence
        const ALT_SEQUENCE = 0x0020;
        /// Descending sort order
        const DESCENDING = 0x0040;
        /// Supplemental index (added after file creation)
        const SUPPLEMENTAL = 0x0080;
        /// Extended key type flags present
        const EXTENDED_TYPE = 0x0100;
        /// Manual key number assignment
        const MANUAL = 0x0200;
    }
}

/// Key specification from FCR
#[derive(Debug, Clone)]
pub struct KeySpec {
    /// Position in record (0-based byte offset)
    pub position: u16,
    /// Key length in bytes
    pub length: u16,
    /// Key flags
    pub flags: KeyFlags,
    /// Key type
    pub key_type: KeyType,
    /// Null value (byte value indicating null)
    pub null_value: u8,
    /// ACS (Alternate Collating Sequence) number
    pub acs_number: u8,
    /// Number of unique values (statistics)
    pub unique_count: u32,
}

impl KeySpec {
    /// Size of a key specification in the FCR (bytes)
    pub const SIZE: usize = 16;

    /// Parse a key specification from bytes
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        if data.len() < Self::SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Key spec too short",
            ));
        }

        let mut cursor = Cursor::new(data);
        let position = cursor.read_u16::<LittleEndian>()?;
        let length = cursor.read_u16::<LittleEndian>()?;
        let raw_flags = cursor.read_u16::<LittleEndian>()?;
        let flags = KeyFlags::from_bits_truncate(raw_flags);
        let unique_count = cursor.read_u32::<LittleEndian>()?;
        let key_type_raw = cursor.read_u8()?;
        let key_type = KeyType::from_raw(key_type_raw);
        let null_value = cursor.read_u8()?;
        let acs_number = cursor.read_u8()?;
        let _reserved = cursor.read_u8()?;

        Ok(KeySpec {
            position,
            length,
            flags,
            key_type,
            null_value,
            acs_number,
            unique_count,
        })
    }

    /// Serialize key specification to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; Self::SIZE];
        buf[0..2].copy_from_slice(&self.position.to_le_bytes());
        buf[2..4].copy_from_slice(&self.length.to_le_bytes());
        buf[4..6].copy_from_slice(&self.flags.bits().to_le_bytes());
        buf[6..10].copy_from_slice(&self.unique_count.to_le_bytes());
        buf[10] = self.key_type as u8;
        buf[11] = self.null_value;
        buf[12] = self.acs_number;
        buf[13] = 0; // reserved
        // Bytes 14-15 are padding to reach SIZE=16
        buf
    }

    /// Check if this key allows duplicate values
    pub fn allows_duplicates(&self) -> bool {
        self.flags.contains(KeyFlags::DUPLICATES)
    }

    /// Check if this key is modifiable
    pub fn is_modifiable(&self) -> bool {
        self.flags.contains(KeyFlags::MODIFIABLE)
    }

    /// Check if this key is part of a segmented (compound) key
    pub fn is_segmented(&self) -> bool {
        self.flags.contains(KeyFlags::SEGMENTED)
    }

    /// Check if this key uses descending sort order
    pub fn is_descending(&self) -> bool {
        self.flags.contains(KeyFlags::DESCENDING)
    }

    /// Check if null values are allowed
    pub fn allows_null(&self) -> bool {
        self.flags.contains(KeyFlags::NULL)
    }

    /// Extract key value from a record
    pub fn extract_key(&self, record: &[u8]) -> Vec<u8> {
        let start = self.position as usize;
        let end = start + self.length as usize;

        if end <= record.len() {
            record[start..end].to_vec()
        } else if start < record.len() {
            // Partial key - pad with zeros
            let mut key = record[start..].to_vec();
            key.resize(self.length as usize, 0);
            key
        } else {
            // Key beyond record - return zeros
            vec![0; self.length as usize]
        }
    }

    /// Compare two key values according to key type
    pub fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        let result = match self.key_type {
            KeyType::String | KeyType::ZString => {
                // Binary comparison for strings
                a.cmp(b)
            }
            KeyType::Integer => self.compare_integer(a, b),
            KeyType::UnsignedBinary | KeyType::AutoIncrement => self.compare_unsigned(a, b),
            KeyType::Float => self.compare_float(a, b),
            KeyType::LString => {
                // First byte is length
                let len_a = a.first().copied().unwrap_or(0) as usize;
                let len_b = b.first().copied().unwrap_or(0) as usize;
                let a_data = a.get(1..=len_a).unwrap_or(&[]);
                let b_data = b.get(1..=len_b).unwrap_or(&[]);
                a_data.cmp(b_data)
            }
            _ => a.cmp(b), // Default binary comparison
        };

        // Reverse for descending keys
        if self.is_descending() {
            result.reverse()
        } else {
            result
        }
    }

    fn compare_integer(&self, a: &[u8], b: &[u8]) -> Ordering {
        match self.length {
            1 => {
                let va = a.first().map(|&x| x as i8).unwrap_or(0);
                let vb = b.first().map(|&x| x as i8).unwrap_or(0);
                va.cmp(&vb)
            }
            2 => {
                let va = Cursor::new(a).read_i16::<LittleEndian>().unwrap_or(0);
                let vb = Cursor::new(b).read_i16::<LittleEndian>().unwrap_or(0);
                va.cmp(&vb)
            }
            4 => {
                let va = Cursor::new(a).read_i32::<LittleEndian>().unwrap_or(0);
                let vb = Cursor::new(b).read_i32::<LittleEndian>().unwrap_or(0);
                va.cmp(&vb)
            }
            8 => {
                let va = Cursor::new(a).read_i64::<LittleEndian>().unwrap_or(0);
                let vb = Cursor::new(b).read_i64::<LittleEndian>().unwrap_or(0);
                va.cmp(&vb)
            }
            _ => a.cmp(b),
        }
    }

    fn compare_unsigned(&self, a: &[u8], b: &[u8]) -> Ordering {
        match self.length {
            1 => {
                let va = a.first().copied().unwrap_or(0);
                let vb = b.first().copied().unwrap_or(0);
                va.cmp(&vb)
            }
            2 => {
                let va = Cursor::new(a).read_u16::<LittleEndian>().unwrap_or(0);
                let vb = Cursor::new(b).read_u16::<LittleEndian>().unwrap_or(0);
                va.cmp(&vb)
            }
            4 => {
                let va = Cursor::new(a).read_u32::<LittleEndian>().unwrap_or(0);
                let vb = Cursor::new(b).read_u32::<LittleEndian>().unwrap_or(0);
                va.cmp(&vb)
            }
            8 => {
                let va = Cursor::new(a).read_u64::<LittleEndian>().unwrap_or(0);
                let vb = Cursor::new(b).read_u64::<LittleEndian>().unwrap_or(0);
                va.cmp(&vb)
            }
            _ => a.cmp(b),
        }
    }

    fn compare_float(&self, a: &[u8], b: &[u8]) -> Ordering {
        match self.length {
            4 => {
                let va = Cursor::new(a).read_f32::<LittleEndian>().unwrap_or(0.0);
                let vb = Cursor::new(b).read_f32::<LittleEndian>().unwrap_or(0.0);
                va.partial_cmp(&vb).unwrap_or(Ordering::Equal)
            }
            8 => {
                let va = Cursor::new(a).read_f64::<LittleEndian>().unwrap_or(0.0);
                let vb = Cursor::new(b).read_f64::<LittleEndian>().unwrap_or(0.0);
                va.partial_cmp(&vb).unwrap_or(Ordering::Equal)
            }
            _ => a.cmp(b),
        }
    }

    /// Check if a key value is null (all bytes equal to null_value)
    pub fn is_null_key(&self, key: &[u8]) -> bool {
        if !self.allows_null() {
            return false;
        }
        key.iter().all(|&b| b == self.null_value)
    }
}

/// A compound (segmented) key made of multiple KeySpecs
#[derive(Debug, Clone)]
pub struct CompoundKey {
    /// The segments that make up this key
    pub segments: Vec<KeySpec>,
}

impl CompoundKey {
    /// Create a new compound key from segments
    pub fn new(segments: Vec<KeySpec>) -> Self {
        CompoundKey { segments }
    }

    /// Total length of the compound key
    pub fn total_length(&self) -> u16 {
        self.segments.iter().map(|s| s.length).sum()
    }

    /// Extract compound key value from a record
    pub fn extract_key(&self, record: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(self.total_length() as usize);
        for segment in &self.segments {
            key.extend(segment.extract_key(record));
        }
        key
    }

    /// Compare two compound key values
    pub fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        let mut a_offset = 0usize;
        let mut b_offset = 0usize;

        for segment in &self.segments {
            let a_end = a_offset + segment.length as usize;
            let b_end = b_offset + segment.length as usize;

            let a_segment = a.get(a_offset..a_end).unwrap_or(&[]);
            let b_segment = b.get(b_offset..b_end).unwrap_or(&[]);

            match segment.compare(a_segment, b_segment) {
                Ordering::Equal => {}
                other => return other,
            }

            a_offset = a_end;
            b_offset = b_end;
        }

        Ordering::Equal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_spec_roundtrip() {
        let spec = KeySpec {
            position: 10,
            length: 20,
            flags: KeyFlags::DUPLICATES | KeyFlags::MODIFIABLE,
            key_type: KeyType::String,
            null_value: 0,
            acs_number: 0,
            unique_count: 0,
        };

        let bytes = spec.to_bytes();
        let parsed = KeySpec::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.position, 10);
        assert_eq!(parsed.length, 20);
        assert!(parsed.allows_duplicates());
        assert!(parsed.is_modifiable());
    }

    #[test]
    fn test_integer_comparison() {
        let spec = KeySpec {
            position: 0,
            length: 4,
            flags: KeyFlags::empty(),
            key_type: KeyType::Integer,
            null_value: 0,
            acs_number: 0,
            unique_count: 0,
        };

        // -1 in little-endian i32
        let neg_one: [u8; 4] = (-1i32).to_le_bytes();
        let one: [u8; 4] = 1i32.to_le_bytes();
        let zero: [u8; 4] = 0i32.to_le_bytes();

        assert_eq!(spec.compare(&neg_one, &zero), Ordering::Less);
        assert_eq!(spec.compare(&zero, &one), Ordering::Less);
        assert_eq!(spec.compare(&one, &neg_one), Ordering::Greater);
    }

    #[test]
    fn test_descending_key() {
        let spec = KeySpec {
            position: 0,
            length: 4,
            flags: KeyFlags::DESCENDING,
            key_type: KeyType::UnsignedBinary,
            null_value: 0,
            acs_number: 0,
            unique_count: 0,
        };

        let one: [u8; 4] = 1u32.to_le_bytes();
        let two: [u8; 4] = 2u32.to_le_bytes();

        // Descending: larger values come first
        assert_eq!(spec.compare(&two, &one), Ordering::Less);
        assert_eq!(spec.compare(&one, &two), Ordering::Greater);
    }

    #[test]
    fn test_extract_key() {
        let spec = KeySpec {
            position: 5,
            length: 3,
            flags: KeyFlags::empty(),
            key_type: KeyType::String,
            null_value: 0,
            acs_number: 0,
            unique_count: 0,
        };

        let record = b"HELLO WORLD";
        let key = spec.extract_key(record);
        assert_eq!(&key, b" WO");
    }
}
