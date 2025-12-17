//! Binary wire protocol for Xtrieve
//!
//! Simple binary protocol inspired by original Btrieve's wire format.
//!
//! Request format:
//!   [op:2][pos_block:128][data_len:4][data:N][key_len:2][key:N][key_num:2][path_len:2][path:N][lock:2]
//!
//! Response format:
//!   [status:2][pos_block:128][data_len:4][data:N][key_len:2][key:N]

use std::io::{self, Read, Write};

pub const POSITION_BLOCK_SIZE: usize = 128;
pub const DEFAULT_PORT: u16 = 7419;

/// Request from client to server
#[derive(Debug, Clone)]
pub struct Request {
    pub operation_code: u16,
    pub position_block: Vec<u8>,
    pub data_buffer: Vec<u8>,
    pub key_buffer: Vec<u8>,
    pub key_number: i16,
    pub file_path: String,
    pub lock_bias: u16,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            operation_code: 0,
            position_block: vec![0u8; POSITION_BLOCK_SIZE],
            data_buffer: Vec::new(),
            key_buffer: Vec::new(),
            key_number: 0,
            file_path: String::new(),
            lock_bias: 0,
        }
    }
}

impl Request {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Operation code (2 bytes)
        buf.extend_from_slice(&self.operation_code.to_le_bytes());

        // Position block (128 bytes, padded)
        let mut pos_block = [0u8; POSITION_BLOCK_SIZE];
        let copy_len = self.position_block.len().min(POSITION_BLOCK_SIZE);
        pos_block[..copy_len].copy_from_slice(&self.position_block[..copy_len]);
        buf.extend_from_slice(&pos_block);

        // Data buffer (4 byte length + data)
        buf.extend_from_slice(&(self.data_buffer.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.data_buffer);

        // Key buffer (2 byte length + data)
        buf.extend_from_slice(&(self.key_buffer.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.key_buffer);

        // Key number (2 bytes)
        buf.extend_from_slice(&self.key_number.to_le_bytes());

        // File path (2 byte length + data)
        let path_bytes = self.file_path.as_bytes();
        buf.extend_from_slice(&(path_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(path_bytes);

        // Lock bias (2 bytes)
        buf.extend_from_slice(&self.lock_bias.to_le_bytes());

        buf
    }

    pub fn from_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut buf2 = [0u8; 2];
        let mut buf4 = [0u8; 4];

        // Operation code
        reader.read_exact(&mut buf2)?;
        let operation_code = u16::from_le_bytes(buf2);

        // Position block
        let mut position_block = vec![0u8; POSITION_BLOCK_SIZE];
        reader.read_exact(&mut position_block)?;

        // Data buffer
        reader.read_exact(&mut buf4)?;
        let data_len = u32::from_le_bytes(buf4) as usize;
        let mut data_buffer = vec![0u8; data_len];
        if data_len > 0 {
            reader.read_exact(&mut data_buffer)?;
        }

        // Key buffer
        reader.read_exact(&mut buf2)?;
        let key_len = u16::from_le_bytes(buf2) as usize;
        let mut key_buffer = vec![0u8; key_len];
        if key_len > 0 {
            reader.read_exact(&mut key_buffer)?;
        }

        // Key number
        reader.read_exact(&mut buf2)?;
        let key_number = i16::from_le_bytes(buf2);

        // File path
        reader.read_exact(&mut buf2)?;
        let path_len = u16::from_le_bytes(buf2) as usize;
        let mut path_buf = vec![0u8; path_len];
        if path_len > 0 {
            reader.read_exact(&mut path_buf)?;
        }
        let file_path = String::from_utf8_lossy(&path_buf).to_string();

        // Lock bias
        reader.read_exact(&mut buf2)?;
        let lock_bias = u16::from_le_bytes(buf2);

        Ok(Request {
            operation_code,
            position_block,
            data_buffer,
            key_buffer,
            key_number,
            file_path,
            lock_bias,
        })
    }
}

/// Response from server to client
#[derive(Debug, Clone)]
pub struct Response {
    pub status_code: u16,
    pub position_block: Vec<u8>,
    pub data_buffer: Vec<u8>,
    pub key_buffer: Vec<u8>,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            status_code: 0,
            position_block: vec![0u8; POSITION_BLOCK_SIZE],
            data_buffer: Vec::new(),
            key_buffer: Vec::new(),
        }
    }
}

impl Response {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Status code (2 bytes)
        buf.extend_from_slice(&self.status_code.to_le_bytes());

        // Position block (128 bytes, padded)
        let mut pos_block = [0u8; POSITION_BLOCK_SIZE];
        let copy_len = self.position_block.len().min(POSITION_BLOCK_SIZE);
        pos_block[..copy_len].copy_from_slice(&self.position_block[..copy_len]);
        buf.extend_from_slice(&pos_block);

        // Data buffer (4 byte length + data)
        buf.extend_from_slice(&(self.data_buffer.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.data_buffer);

        // Key buffer (2 byte length + data)
        buf.extend_from_slice(&(self.key_buffer.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.key_buffer);

        buf
    }

    pub fn from_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut buf2 = [0u8; 2];
        let mut buf4 = [0u8; 4];

        // Status code
        reader.read_exact(&mut buf2)?;
        let status_code = u16::from_le_bytes(buf2);

        // Position block
        let mut position_block = vec![0u8; POSITION_BLOCK_SIZE];
        reader.read_exact(&mut position_block)?;

        // Data buffer
        reader.read_exact(&mut buf4)?;
        let data_len = u32::from_le_bytes(buf4) as usize;
        let mut data_buffer = vec![0u8; data_len];
        if data_len > 0 {
            reader.read_exact(&mut data_buffer)?;
        }

        // Key buffer
        reader.read_exact(&mut buf2)?;
        let key_len = u16::from_le_bytes(buf2) as usize;
        let mut key_buffer = vec![0u8; key_len];
        if key_len > 0 {
            reader.read_exact(&mut key_buffer)?;
        }

        Ok(Response {
            status_code,
            position_block,
            data_buffer,
            key_buffer,
        })
    }

    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.to_bytes())
    }
}
