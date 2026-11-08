//! The caller-visible 128-byte position block.
//!
//! The DOS engine copies a 64-byte cursor workspace into the first half of the
//! caller's block on every call and copies it back on return.  The clone keeps
//! the same 64-byte layout; bytes 0x40..0x7F are left untouched.  The file
//! number at `0x34` identifies the open handle and doubles as the in-memory
//! cursor key.

pub const POSITION_BLOCK_SIZE: usize = 128;
pub type PositionBlock = [u8; POSITION_BLOCK_SIZE];

pub const OPEN_MARKER: u8 = 0xCB;
const OPCODE_OFFSET: usize = 0x01;
pub const FILE_NUMBER_OFFSET: usize = 0x34;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionRef {
    pub handle: u32,
    pub token: u32,
}

/// Apply the writes Open performs on the caller's cursor workspace.  Every
/// other byte is preserved from the caller's input.
pub fn encode_open(position: &mut PositionBlock, handle: u32, aux: u16) {
    position[0] = OPEN_MARKER;
    position[OPCODE_OFFSET] = 0;
    position[0x02] = 0;
    position[0x03] = 0xFF;
    position[0x04] = 0xFF;
    position[0x07] = 0xFF;
    position[0x08] = 0xFF;
    position[0x0B] = 0xFF;
    position[0x0C] = 0xFF;
    position[0x33] = 0;
    position[FILE_NUMBER_OFFSET] = handle as u8;
    position[0x35] = 0;
    position[0x36..0x38].copy_from_slice(&aux.to_le_bytes());
    position[0x38..0x3C].fill(0);
}

pub fn decode(position: &PositionBlock) -> Option<PositionRef> {
    if position[0] != OPEN_MARKER {
        return None;
    }
    let handle = position[FILE_NUMBER_OFFSET] as u32;
    if handle == 0 {
        return None;
    }
    Some(PositionRef {
        handle,
        token: handle,
    })
}

/// Close writes only the operation word: the rest of the cursor workspace is
/// preserved exactly as the engine left it.
pub fn close(position: &mut PositionBlock) {
    position[0] = 0;
    position[OPCODE_OFFSET] = 0x01;
    position[0x02] = 0;
}

/// Write a record address in the engine's position format: high 16-bit word
/// first, then the low word, each little-endian.
pub fn write_address(buffer: &mut [u8], address: u32) -> bool {
    if buffer.len() < 4 {
        return false;
    }
    buffer[..2].copy_from_slice(&((address >> 16) as u16).to_le_bytes());
    buffer[2..4].copy_from_slice(&(address as u16).to_le_bytes());
    true
}

pub fn read_address(buffer: &[u8]) -> Option<u32> {
    if buffer.len() < 4 {
        return None;
    }
    let high = u16::from_le_bytes([buffer[0], buffer[1]]) as u32;
    let low = u16::from_le_bytes([buffer[2], buffer[3]]) as u32;
    Some((high << 16) | low)
}
