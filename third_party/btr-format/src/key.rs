//! Key segments, alternate collating sequences, and Btrieve key ordering.
//!
//! The comparison rules follow the 5.10a manual (Appendix G) and were
//! confirmed against the genuine engine with differential ordering probes.
//! Index entries are sorted by the engine's key order, not by raw byte
//! order, so every B-tree operation in the writer uses these comparators.

use std::cmp::Ordering;

use serde::Serialize;

/// One segment of a (possibly segmented) Btrieve key.  The FCR stores one
/// 30-byte block per segment; the first block also carries the aggregate key
/// fields and the root page.
#[derive(Debug, Clone, Serialize)]
pub struct KeySegment {
    /// 1-based byte position of the segment inside the record.
    pub position: u16,
    pub length: u16,
    pub flags: u16,
    /// Extended key type code (0 string, 1 integer, ... 15 autoincrement).
    pub key_type: u8,
    pub null_value: u8,
}

impl KeySegment {
    pub const fn new(position: u16, length: u16, flags: u16, key_type: u8, null_value: u8) -> Self {
        Self {
            position,
            length,
            flags,
            key_type,
            null_value,
        }
    }

    pub const fn has_duplicates(&self) -> bool {
        self.flags & 0x0001 != 0
    }

    pub const fn is_modifiable(&self) -> bool {
        self.flags & 0x0002 != 0
    }

    pub const fn is_binary(&self) -> bool {
        self.flags & 0x0004 != 0
    }

    pub const fn has_null(&self) -> bool {
        self.flags & 0x0008 != 0
    }

    pub const fn is_segmented(&self) -> bool {
        self.flags & 0x0010 != 0
    }

    pub const fn has_acs(&self) -> bool {
        self.flags & 0x0020 != 0
    }

    pub const fn is_descending(&self) -> bool {
        self.flags & 0x0040 != 0
    }

    pub const fn is_supplemental(&self) -> bool {
        self.flags & 0x0080 != 0
    }

    pub const fn is_extended_type(&self) -> bool {
        self.flags & 0x0100 != 0
    }

    pub const fn is_manual(&self) -> bool {
        self.flags & 0x0200 != 0
    }

    /// Bytes occupied by this segment in a record, or `None` if the segment
    /// falls outside the supplied record.
    pub fn bytes<'a>(&self, record: &'a [u8]) -> Option<&'a [u8]> {
        let start = self.position.checked_sub(1)? as usize;
        let end = start.checked_add(self.length as usize)?;
        record.get(start..end)
    }

    /// A segment is null when every byte equals its null value.
    pub fn all_null(&self, record: &[u8]) -> bool {
        self.bytes(record)
            .map(|bytes| bytes.iter().all(|&byte| byte == self.null_value))
            .unwrap_or(false)
    }
}

/// Alternate collating sequence definition stored on page 1 when any key
/// segment sets the ACS flag.
#[derive(Debug, Clone, Serialize)]
pub struct Acs {
    /// Nine header bytes followed by the 256-byte collating table.
    pub raw: Vec<u8>,
    pub name: String,
    /// `table[byte]` is the sort weight used instead of the raw byte.
    #[serde(skip)]
    pub table: [u8; 256],
}

impl Acs {
    pub const SIZE: usize = 265;

    pub fn from_bytes(raw: &[u8]) -> Option<Self> {
        if raw.len() < Self::SIZE || raw[0] != 0xAC {
            return None;
        }
        let raw = raw[..Self::SIZE].to_vec();
        let name = String::from_utf8_lossy(&raw[1..9]).trim_end().to_string();
        let mut table = [0u8; 256];
        table.copy_from_slice(&raw[9..Self::SIZE]);
        Some(Self { raw, name, table })
    }

    pub fn weight(&self, byte: u8) -> u8 {
        self.table[byte as usize]
    }
}

/// Compare two composite key byte strings for the given segment list.  The
/// input bytes are the concatenation of the segment values in segment order.
pub fn compare_composite(
    a: &[u8],
    b: &[u8],
    segments: &[KeySegment],
    acs: Option<&Acs>,
) -> Ordering {
    if segments.is_empty() {
        return a.cmp(b);
    }
    let mut offset_a = 0usize;
    let mut offset_b = 0usize;
    for segment in segments {
        let length = segment.length as usize;
        let Some(part_a) = a.get(offset_a..offset_a + length) else {
            return a.cmp(b);
        };
        let Some(part_b) = b.get(offset_b..offset_b + length) else {
            return a.cmp(b);
        };
        let ordering = compare_segment(part_a, part_b, segment, acs);
        if ordering != Ordering::Equal {
            return ordering;
        }
        offset_a += length;
        offset_b += length;
    }
    // Every segment compared equal.  Composite keys have exactly the sum of
    // the segment lengths, so no trailing bytes remain to compare; comparing
    // the raw key here would defeat ACS/typed equality.
    Ordering::Equal
}

/// Compare one segment according to its extended key type, then apply the
/// per-segment descending flag.
pub fn compare_segment(a: &[u8], b: &[u8], segment: &KeySegment, acs: Option<&Acs>) -> Ordering {
    let ordering = match segment.key_type {
        0 => compare_mapped(a, b, acs.filter(|_| segment.has_acs())),
        1 => compare_signed_le(a, b),
        2 => compare_float(a, b),
        3 => compare_date(a, b),
        4 => compare_time(a, b),
        5 | 6 => compare_packed_decimal(a, b),
        7 => a.cmp(b),
        8 => compare_numeric(a, b),
        9 => compare_bfloat(a, b),
        10 => compare_lstring(a, b, acs.filter(|_| segment.has_acs())),
        11 => compare_zstring(a, b, acs.filter(|_| segment.has_acs())),
        14 => compare_unsigned_le(a, b),
        15 => compare_autoincrement(a, b),
        _ => a.cmp(b),
    };
    if segment.is_descending() {
        ordering.reverse()
    } else {
        ordering
    }
}

fn compare_mapped(a: &[u8], b: &[u8], acs: Option<&Acs>) -> Ordering {
    match acs {
        Some(acs) => {
            for (left, right) in a.iter().zip(b.iter()) {
                let ordering = acs.weight(*left).cmp(&acs.weight(*right));
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            a.len().cmp(&b.len())
        }
        None => a.cmp(b),
    }
}

fn compare_signed_le(a: &[u8], b: &[u8]) -> Ordering {
    let sign_a = a.last().is_some_and(|byte| byte & 0x80 != 0);
    let sign_b = b.last().is_some_and(|byte| byte & 0x80 != 0);
    if sign_a != sign_b {
        return if sign_a {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let fill = if sign_a { 0xFF } else { 0x00 };
    compare_le_bytes(a, b, fill)
}

fn compare_unsigned_le(a: &[u8], b: &[u8]) -> Ordering {
    compare_le_bytes(a, b, 0x00)
}

/// Compare little-endian integer byte strings from most significant byte to
/// least significant, sign-extending with `fill`.
fn compare_le_bytes(a: &[u8], b: &[u8], fill: u8) -> Ordering {
    let length = a.len().max(b.len());
    for index in (0..length).rev() {
        let left = a.get(index).copied().unwrap_or(fill);
        let right = b.get(index).copied().unwrap_or(fill);
        let ordering = left.cmp(&right);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn signed_le_value(bytes: &[u8]) -> i128 {
    let mut value: i128 = 0;
    for (index, byte) in bytes.iter().enumerate() {
        value |= (*byte as i128) << (8 * index);
    }
    let bits = bytes.len() * 8;
    if bits < 128 && value & (1i128 << (bits - 1)) != 0 {
        value -= 1i128 << bits;
    }
    value
}

fn compare_float(a: &[u8], b: &[u8]) -> Ordering {
    match (a.len(), b.len()) {
        (4, 4) => {
            let left = f32::from_le_bytes([a[0], a[1], a[2], a[3]]);
            let right = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            left.partial_cmp(&right).unwrap_or(Ordering::Equal)
        }
        (8, 8) => {
            let left = f64::from_le_bytes(a.try_into().expect("8-byte float"));
            let right = f64::from_le_bytes(b.try_into().expect("8-byte float"));
            left.partial_cmp(&right).unwrap_or(Ordering::Equal)
        }
        _ => a.cmp(b),
    }
}

fn compare_date(a: &[u8], b: &[u8]) -> Ordering {
    match (a.len(), b.len()) {
        (4, 4) => {
            let left = (u16::from_le_bytes([a[2], a[3]]), a[1], a[0]);
            let right = (u16::from_le_bytes([b[2], b[3]]), b[1], b[0]);
            left.cmp(&right)
        }
        _ => a.cmp(b),
    }
}

fn compare_time(a: &[u8], b: &[u8]) -> Ordering {
    match (a.len(), b.len()) {
        (4, 4) => (a[0], a[1], a[2], a[3]).cmp(&(b[0], b[1], b[2], b[3])),
        _ => a.cmp(b),
    }
}

/// Decode a packed decimal (COMP-3) value into a sign and its digit values.
/// The low nibble of the final byte is the sign: `0xC`/`0xF` positive,
/// `0xD` negative.
fn packed_decimal_parts(bytes: &[u8]) -> Option<(bool, Vec<u8>)> {
    let last = *bytes.last()?;
    let sign = last & 0x0F;
    let negative = match sign {
        0x0C | 0x0F => false,
        0x0D => true,
        _ => return None,
    };
    let mut digits = Vec::with_capacity(bytes.len() * 2 - 1);
    for (index, byte) in bytes.iter().enumerate() {
        let high = byte >> 4;
        if high > 9 {
            return None;
        }
        digits.push(high);
        if index + 1 < bytes.len() {
            let low = byte & 0x0F;
            if low > 9 {
                return None;
            }
            digits.push(low);
        }
    }
    Some((negative, digits))
}

fn compare_packed_decimal(a: &[u8], b: &[u8]) -> Ordering {
    let (Some(left), Some(right)) = (packed_decimal_parts(a), packed_decimal_parts(b)) else {
        return a.cmp(b);
    };
    compare_sign_and_digits(left, right)
}

/// Decode a numeric key: ASCII digits with an embedded sign in the final
/// byte.  Positive digits may also be stored as `A`..`I`/`{`; negative
/// digits use `J`..`R`/`}`.
fn numeric_parts(bytes: &[u8]) -> Option<(bool, Vec<u8>)> {
    let last_index = bytes.len().checked_sub(1)?;
    let mut negative = false;
    let mut digits = Vec::with_capacity(bytes.len());
    for (index, byte) in bytes.iter().enumerate() {
        if index == last_index {
            let (is_negative, digit) = match *byte {
                b'0'..=b'9' => (false, byte - b'0'),
                b'A'..=b'I' => (false, byte - b'A' + 1),
                b'{' => (false, 0),
                b'J'..=b'R' => (true, byte - b'J' + 1),
                b'}' => (true, 0),
                _ => return None,
            };
            negative = is_negative;
            digits.push(digit);
        } else if byte.is_ascii_digit() {
            digits.push(byte - b'0');
        } else {
            return None;
        }
    }
    Some((negative, digits))
}

fn compare_numeric(a: &[u8], b: &[u8]) -> Ordering {
    let (Some(left), Some(right)) = (numeric_parts(a), numeric_parts(b)) else {
        return a.cmp(b);
    };
    compare_sign_and_digits(left, right)
}

fn compare_sign_and_digits(left: (bool, Vec<u8>), right: (bool, Vec<u8>)) -> Ordering {
    if left.0 != right.0 {
        return if left.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let ordering = compare_digits(&left.1, &right.1);
    if left.0 {
        ordering.reverse()
    } else {
        ordering
    }
}

/// Compare decimal digit strings numerically, ignoring leading zeros.
fn compare_digits(a: &[u8], b: &[u8]) -> Ordering {
    let trim = |digits: &[u8]| {
        let start = digits
            .iter()
            .position(|&digit| digit != 0)
            .unwrap_or(digits.len());
        digits[start..].to_vec()
    };
    let left = trim(a);
    let right = trim(b);
    left.len().cmp(&right.len()).then_with(|| left.cmp(&right))
}

/// Decode a Microsoft BASIC single/double precision float.
fn mbf_value(bytes: &[u8]) -> Option<f64> {
    match bytes.len() {
        4 => {
            let mantissa =
                (bytes[0] as u64) | ((bytes[1] as u64) << 8) | (((bytes[2] & 0x7F) as u64) << 16);
            if bytes[3] == 0 {
                return Some(0.0);
            }
            let negative = bytes[2] & 0x80 != 0;
            // MBF has an implicit leading 1 and an exponent biased by 129.
            let fraction = 1.0 + (mantissa as f64) / (1u64 << 23) as f64;
            let value = fraction * 2f64.powi(bytes[3] as i32 - 129);
            Some(if negative { -value } else { value })
        }
        8 => {
            let mut mantissa: u64 = 0;
            for (index, byte) in bytes[..7].iter().enumerate() {
                let byte = if index == 6 { byte & 0x7F } else { *byte };
                mantissa |= (byte as u64) << (8 * index);
            }
            if bytes[7] == 0 {
                return Some(0.0);
            }
            let negative = bytes[6] & 0x80 != 0;
            let fraction = 1.0 + (mantissa as f64) / (1u64 << 55) as f64;
            let value = fraction * 2f64.powi(bytes[7] as i32 - 129);
            Some(if negative { -value } else { value })
        }
        _ => None,
    }
}

fn compare_bfloat(a: &[u8], b: &[u8]) -> Ordering {
    match (mbf_value(a), mbf_value(b)) {
        (Some(left), Some(right)) => left.partial_cmp(&right).unwrap_or(Ordering::Equal),
        _ => a.cmp(b),
    }
}

fn compare_autoincrement(a: &[u8], b: &[u8]) -> Ordering {
    signed_le_value(a).abs().cmp(&signed_le_value(b).abs())
}

fn lstring_bytes(bytes: &[u8]) -> &[u8] {
    if bytes.is_empty() {
        return bytes;
    }
    let length = (bytes[0] as usize).min(bytes.len() - 1);
    &bytes[1..1 + length]
}

fn zstring_bytes(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    &bytes[..end]
}

fn compare_lstring(a: &[u8], b: &[u8], acs: Option<&Acs>) -> Ordering {
    compare_prefix(lstring_bytes(a), lstring_bytes(b), acs)
}

fn compare_zstring(a: &[u8], b: &[u8], acs: Option<&Acs>) -> Ordering {
    compare_prefix(zstring_bytes(a), zstring_bytes(b), acs)
}

/// Lexicographic comparison where a shorter prefix sorts first.
fn compare_prefix(a: &[u8], b: &[u8], acs: Option<&Acs>) -> Ordering {
    match acs {
        Some(acs) => {
            for (left, right) in a.iter().zip(b.iter()) {
                let ordering = acs.weight(*left).cmp(&acs.weight(*right));
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            a.len().cmp(&b.len())
        }
        None => a.cmp(b),
    }
}
