//! Input buffers for the extended-filter operations 0x24-0x28.
//!
//! All five operations share the filter/extraction structure documented for
//! Get Next Extended.  The 5.10a engine accepts only the `EG` start mode; the
//! later `UC` mode returns status 62 on this build.

use std::cmp::Ordering;

use btr_format::{compare_segment, Acs, KeySegment};

use crate::status::{Status, DATA_BUFFER_LENGTH, INCORRECT_DESCRIPTOR, INCORRECT_FIELD_OFFSET};

#[derive(Debug, Clone)]
pub enum Operand {
    Constant(Vec<u8>),
    Field(u16),
}

#[derive(Debug, Clone)]
pub struct FilterTerm {
    pub data_type: u8,
    pub length: u16,
    pub offset: u16,
    /// Comparison code including its bias bits.
    pub comparison: u8,
    pub connector: u8,
    pub operand: Operand,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldDescriptor {
    pub length: u16,
    pub offset: u16,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub max_reject: u16,
    pub terms: Vec<FilterTerm>,
    pub record_limit: u16,
    pub fields: Vec<FieldDescriptor>,
}

impl Request {
    /// Parse the input structure.  `capacity` is the caller's data-buffer
    /// length; the structure's own length is at offset 0.
    pub fn parse(data: &[u8], capacity: usize) -> Result<Self, Status> {
        if capacity < 2 || data.len() < 2 {
            return Err(DATA_BUFFER_LENGTH);
        }
        let input_length = u16::from_le_bytes([data[0], data[1]]) as usize;
        if input_length > capacity || input_length > data.len() {
            return Err(DATA_BUFFER_LENGTH);
        }
        if input_length < 12 {
            return Err(INCORRECT_DESCRIPTOR);
        }
        if &data[2..4] != b"EG" {
            return Err(INCORRECT_DESCRIPTOR);
        }
        let max_reject = u16::from_le_bytes([data[4], data[5]]);
        let term_count = u16::from_le_bytes([data[6], data[7]]) as usize;
        let mut cursor = 8usize;
        let mut terms = Vec::with_capacity(term_count);
        for index in 0..term_count {
            if cursor + 7 > input_length {
                return Err(INCORRECT_DESCRIPTOR);
            }
            let data_type = data[cursor];
            let length = u16::from_le_bytes([data[cursor + 1], data[cursor + 2]]);
            let offset = u16::from_le_bytes([data[cursor + 3], data[cursor + 4]]);
            let comparison = data[cursor + 5];
            let connector = data[cursor + 6];
            cursor += 7;
            if length == 0 || connector > 2 || !(1..=6).contains(&(comparison & 0x07)) {
                return Err(INCORRECT_DESCRIPTOR);
            }
            let operand = if comparison & 0x40 != 0 {
                if cursor + 2 > input_length {
                    return Err(INCORRECT_DESCRIPTOR);
                }
                let field_offset = u16::from_le_bytes([data[cursor], data[cursor + 1]]);
                cursor += 2;
                Operand::Field(field_offset)
            } else {
                let size = length as usize;
                if cursor + size > input_length {
                    return Err(INCORRECT_DESCRIPTOR);
                }
                let constant = data[cursor..cursor + size].to_vec();
                cursor += size;
                Operand::Constant(constant)
            };
            if index + 1 < term_count && connector == 0 {
                return Err(INCORRECT_DESCRIPTOR);
            }
            terms.push(FilterTerm {
                data_type,
                length,
                offset,
                comparison,
                connector,
                operand,
            });
        }
        if terms.last().is_some_and(|term| term.connector != 0) {
            return Err(INCORRECT_DESCRIPTOR);
        }
        if cursor + 4 > input_length {
            return Err(INCORRECT_DESCRIPTOR);
        }
        let record_limit = u16::from_le_bytes([data[cursor], data[cursor + 1]]);
        let field_count = u16::from_le_bytes([data[cursor + 2], data[cursor + 3]]) as usize;
        cursor += 4;
        if record_limit == 0 || cursor + field_count * 4 > input_length {
            return Err(INCORRECT_DESCRIPTOR);
        }
        let mut fields = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            let length = u16::from_le_bytes([data[cursor], data[cursor + 1]]);
            let offset = u16::from_le_bytes([data[cursor + 2], data[cursor + 3]]);
            cursor += 4;
            if length == 0 {
                return Err(INCORRECT_DESCRIPTOR);
            }
            fields.push(FieldDescriptor { length, offset });
        }
        if cursor != input_length {
            return Err(INCORRECT_DESCRIPTOR);
        }
        Ok(Self {
            max_reject,
            terms,
            record_limit,
            fields,
        })
    }

    pub fn matches(&self, record: &[u8], acs: Option<&Acs>) -> Result<bool, Status> {
        if self.terms.is_empty() {
            return Ok(true);
        }
        let mut result = self.evaluate(&self.terms[0], record, acs)?;
        for index in 1..self.terms.len() {
            let value = self.evaluate(&self.terms[index], record, acs)?;
            match self.terms[index - 1].connector {
                1 => result = result && value,
                2 => result = result || value,
                _ => return Err(INCORRECT_DESCRIPTOR),
            }
        }
        Ok(result)
    }

    fn evaluate(
        &self,
        term: &FilterTerm,
        record: &[u8],
        acs: Option<&Acs>,
    ) -> Result<bool, Status> {
        let mut left = field_value(record, term.offset, term.length);
        let mut right = match &term.operand {
            Operand::Constant(bytes) => bytes.clone(),
            Operand::Field(offset) => field_value(record, *offset, term.length),
        };
        if term.comparison & 0x80 != 0 && matches!(term.data_type, 0 | 10 | 11) {
            left.make_ascii_lowercase();
            right.make_ascii_lowercase();
        }
        let use_acs = term.comparison & (0x08 | 0x20) != 0;
        let segment = KeySegment::new(1, term.length, 0, term.data_type, 0);
        let ordering = compare_segment(&left, &right, &segment, if use_acs { acs } else { None });
        Ok(match term.comparison & 0x07 {
            1 => ordering == Ordering::Equal,
            2 => ordering == Ordering::Greater,
            3 => ordering == Ordering::Less,
            4 => ordering != Ordering::Equal,
            5 => ordering != Ordering::Less,
            6 => ordering != Ordering::Greater,
            _ => return Err(INCORRECT_DESCRIPTOR),
        })
    }

    pub fn extract(&self, record: &[u8]) -> Result<Vec<u8>, Status> {
        if self.fields.is_empty() {
            return Ok(Vec::new());
        }
        let mut image = Vec::new();
        for (index, field) in self.fields.iter().enumerate() {
            let offset = field.offset as usize;
            if offset > record.len() {
                return Err(INCORRECT_FIELD_OFFSET);
            }
            let available = (record.len() - offset).min(field.length as usize);
            image.extend_from_slice(&record[offset..offset + available]);
            if available < field.length as usize && index + 1 < self.fields.len() {
                return Err(DATA_BUFFER_LENGTH);
            }
        }
        Ok(image)
    }
}

fn field_value(record: &[u8], offset: u16, length: u16) -> Vec<u8> {
    let offset = offset as usize;
    let mut value = vec![0u8; length as usize];
    if offset < record.len() {
        let available = (record.len() - offset).min(length as usize);
        value[..available].copy_from_slice(&record[offset..offset + available]);
    }
    value
}

/// Write the returned record list.  Returns the number of bytes written and,
/// when the caller's buffer cannot hold the complete output, status 22 with a
/// possibly partial final image.
pub fn write_result(
    data: &mut [u8],
    capacity: usize,
    blocks: &[(u32, Vec<u8>)],
) -> (usize, Option<Status>) {
    if capacity < 2 || data.len() < 2 {
        return (0, Some(DATA_BUFFER_LENGTH));
    }
    let mut position = 2usize;
    let mut count = 0u16;
    for (address, image) in blocks {
        if position + 6 > capacity || (position + 6 == capacity && !image.is_empty()) {
            write_count(data, count);
            return (position, Some(DATA_BUFFER_LENGTH));
        }
        let room = capacity - position - 6;
        let copied = room.min(image.len());
        data[position..position + 2].copy_from_slice(&(copied as u16).to_le_bytes());
        // Physical addresses are stored high 16-bit word first, each word
        // little-endian, matching Get Position.
        data[position + 2..position + 4].copy_from_slice(&((address >> 16) as u16).to_le_bytes());
        data[position + 4..position + 6].copy_from_slice(&(*address as u16).to_le_bytes());
        data[position + 6..position + 6 + copied].copy_from_slice(&image[..copied]);
        position += 6 + copied;
        count += 1;
        if copied < image.len() {
            write_count(data, count);
            return (position, Some(DATA_BUFFER_LENGTH));
        }
    }
    write_count(data, count);
    (position, None)
}

fn write_count(data: &mut [u8], count: u16) {
    data[0..2].copy_from_slice(&count.to_le_bytes());
}
