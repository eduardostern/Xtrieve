//! Parse the Create operation's data-buffer specification.

use btr_format::{Acs, FileSpec, KeySegment, KeySpec};

use crate::status::{
    from_format, Status, DATA_BUFFER_LENGTH, INVALID_KEY_LENGTH, INVALID_KEY_POSITION,
    INVALID_RECORD_LENGTH, NUMBER_OF_KEYS, PAGE_SIZE_ERROR,
};

const FILE_SPEC_SIZE: usize = 16;
const KEY_SPEC_SIZE: usize = 16;
const ACS_SIZE: usize = 265;

pub fn parse_create_spec(data: &[u8]) -> Result<FileSpec, Status> {
    if data.len() < FILE_SPEC_SIZE {
        return Err(DATA_BUFFER_LENGTH);
    }
    let record_length = u16::from_le_bytes([data[0], data[1]]);
    let page_size = u16::from_le_bytes([data[2], data[3]]);
    let key_count = u16::from_le_bytes([data[4], data[5]]) as usize;
    let file_flags = u16::from_le_bytes([data[10], data[11]]);
    let allocation = u16::from_le_bytes([data[14], data[15]]);

    if record_length == 0 {
        return Err(INVALID_RECORD_LENGTH);
    }
    if !matches!(page_size, 512 | 1024 | 2048 | 4096) {
        return Err(PAGE_SIZE_ERROR);
    }
    if key_count > 24 {
        return Err(NUMBER_OF_KEYS);
    }

    let mut cursor = FILE_SPEC_SIZE;
    let mut keys = Vec::with_capacity(key_count);
    let mut has_acs = false;
    for _ in 0..key_count {
        let mut segments: Vec<KeySegment> = Vec::new();
        loop {
            if cursor + KEY_SPEC_SIZE > data.len() {
                return Err(DATA_BUFFER_LENGTH);
            }
            let block = &data[cursor..cursor + KEY_SPEC_SIZE];
            cursor += KEY_SPEC_SIZE;
            let position = u16::from_le_bytes([block[0], block[1]]);
            let length = u16::from_le_bytes([block[2], block[3]]);
            let flags = u16::from_le_bytes([block[4], block[5]]);
            let key_type = block[10];
            let null_value = block[11];
            if position == 0 || length == 0 {
                return Err(INVALID_KEY_LENGTH);
            }
            if u32::from(position) + u32::from(length) - 1 > u32::from(record_length) {
                return Err(INVALID_KEY_POSITION);
            }
            has_acs |= flags & 0x0020 != 0;
            segments.push(KeySegment::new(
                position, length, flags, key_type, null_value,
            ));
            if segments.len() > 24 {
                return Err(NUMBER_OF_KEYS);
            }
            if flags & 0x0010 == 0 {
                break;
            }
        }
        let key = if segments.len() == 1 {
            let segment = &segments[0];
            KeySpec::new(
                segment.position,
                segment.length,
                segment.flags,
                segment.key_type,
                segment.null_value,
            )
        } else {
            KeySpec::segmented(segments).map_err(|error| from_format(&error))?
        };
        keys.push(key);
    }

    let acs = if has_acs {
        if cursor + ACS_SIZE > data.len() {
            return Err(DATA_BUFFER_LENGTH);
        }
        Some(Acs::from_bytes(&data[cursor..cursor + ACS_SIZE]).ok_or(DATA_BUFFER_LENGTH)?)
    } else {
        None
    };

    FileSpec::new_with_acs(record_length, page_size, file_flags, allocation, keys, acs)
        .map_err(|error| from_format(&error))
}
