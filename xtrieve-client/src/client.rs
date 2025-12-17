//! TCP client for connecting to xtrieved
//!
//! Provides both sync and async clients:
//! - `XtrieveClient` - Synchronous client using std::net::TcpStream
//! - `AsyncXtrieveClient` - Async client using tokio::net::TcpStream

use std::io::{BufReader, BufWriter, Write};
use std::net::TcpStream;
use xtrieve_engine::protocol::{Request, Response, POSITION_BLOCK_SIZE};
use xtrieve_engine::{BtrieveError, BtrieveResult};

// ============================================================================
// Sync Client
// ============================================================================

/// Synchronous client for connecting to xtrieved daemon
pub struct XtrieveClient {
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
}

impl XtrieveClient {
    /// Connect to xtrieved at the given address (e.g., "127.0.0.1:7419")
    pub fn connect(addr: &str) -> BtrieveResult<Self> {
        let stream = TcpStream::connect(addr)
            .map_err(|e| BtrieveError::Internal(format!("Connection failed: {}", e)))?;

        let reader = BufReader::new(stream.try_clone()
            .map_err(|e| BtrieveError::Internal(format!("Clone failed: {}", e)))?);
        let writer = BufWriter::new(stream);

        Ok(XtrieveClient { reader, writer })
    }

    /// Execute a Btrieve operation
    pub fn execute(&mut self, request: BtrieveRequest) -> BtrieveResult<BtrieveResponse> {
        // Convert to wire protocol
        let wire_req = Request {
            operation_code: request.operation_code as u16,
            position_block: request.position_block,
            data_buffer: request.data_buffer,
            key_buffer: request.key_buffer,
            key_number: request.key_number as i16,
            file_path: request.file_path,
            lock_bias: request.lock_bias as u16,
        };

        // Send request
        self.writer.write_all(&wire_req.to_bytes())
            .map_err(|e| BtrieveError::Internal(format!("Write failed: {}", e)))?;
        self.writer.flush()
            .map_err(|e| BtrieveError::Internal(format!("Flush failed: {}", e)))?;

        // Read response
        let wire_resp = Response::from_reader(&mut self.reader)
            .map_err(|e| BtrieveError::Internal(format!("Read failed: {}", e)))?;

        Ok(BtrieveResponse {
            status_code: wire_resp.status_code as u32,
            position_block: wire_resp.position_block,
            data_buffer: wire_resp.data_buffer,
            key_buffer: wire_resp.key_buffer,
        })
    }
}

// ============================================================================
// Async Client (requires tokio feature)
// ============================================================================

#[cfg(feature = "async")]
pub use async_client::AsyncXtrieveClient;

#[cfg(feature = "async")]
mod async_client {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
    use tokio::net::TcpStream;

    /// Async client for connecting to xtrieved daemon
    ///
    /// Uses tokio::net::TcpStream for non-blocking I/O.
    pub struct AsyncXtrieveClient {
        reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
        writer: BufWriter<tokio::net::tcp::OwnedWriteHalf>,
    }

    impl AsyncXtrieveClient {
        /// Connect to xtrieved at the given address (e.g., "127.0.0.1:7419")
        pub async fn connect(addr: &str) -> BtrieveResult<Self> {
            let stream = TcpStream::connect(addr).await
                .map_err(|e| BtrieveError::Internal(format!("Connection failed: {}", e)))?;

            let (read_half, write_half) = stream.into_split();
            let reader = BufReader::new(read_half);
            let writer = BufWriter::new(write_half);

            Ok(AsyncXtrieveClient { reader, writer })
        }

        /// Execute a Btrieve operation asynchronously
        pub async fn execute(&mut self, request: BtrieveRequest) -> BtrieveResult<BtrieveResponse> {
            // Convert to wire protocol
            let wire_req = Request {
                operation_code: request.operation_code as u16,
                position_block: request.position_block,
                data_buffer: request.data_buffer,
                key_buffer: request.key_buffer,
                key_number: request.key_number as i16,
                file_path: request.file_path,
                lock_bias: request.lock_bias as u16,
            };

            // Send request
            self.writer.write_all(&wire_req.to_bytes()).await
                .map_err(|e| BtrieveError::Internal(format!("Write failed: {}", e)))?;
            self.writer.flush().await
                .map_err(|e| BtrieveError::Internal(format!("Flush failed: {}", e)))?;

            // Read response
            let wire_resp = self.read_response().await?;

            Ok(BtrieveResponse {
                status_code: wire_resp.status_code as u32,
                position_block: wire_resp.position_block,
                data_buffer: wire_resp.data_buffer,
                key_buffer: wire_resp.key_buffer,
            })
        }

        /// Read response from the stream asynchronously
        async fn read_response(&mut self) -> BtrieveResult<Response> {
            let mut buf2 = [0u8; 2];
            let mut buf4 = [0u8; 4];

            // Status code
            self.reader.read_exact(&mut buf2).await
                .map_err(|e| BtrieveError::Internal(format!("Read status failed: {}", e)))?;
            let status_code = u16::from_le_bytes(buf2);

            // Position block
            let mut position_block = vec![0u8; POSITION_BLOCK_SIZE];
            self.reader.read_exact(&mut position_block).await
                .map_err(|e| BtrieveError::Internal(format!("Read pos_block failed: {}", e)))?;

            // Data buffer
            self.reader.read_exact(&mut buf4).await
                .map_err(|e| BtrieveError::Internal(format!("Read data_len failed: {}", e)))?;
            let data_len = u32::from_le_bytes(buf4) as usize;
            let mut data_buffer = vec![0u8; data_len];
            if data_len > 0 {
                self.reader.read_exact(&mut data_buffer).await
                    .map_err(|e| BtrieveError::Internal(format!("Read data failed: {}", e)))?;
            }

            // Key buffer
            self.reader.read_exact(&mut buf2).await
                .map_err(|e| BtrieveError::Internal(format!("Read key_len failed: {}", e)))?;
            let key_len = u16::from_le_bytes(buf2) as usize;
            let mut key_buffer = vec![0u8; key_len];
            if key_len > 0 {
                self.reader.read_exact(&mut key_buffer).await
                    .map_err(|e| BtrieveError::Internal(format!("Read key failed: {}", e)))?;
            }

            Ok(Response {
                status_code,
                position_block,
                data_buffer,
                key_buffer,
            })
        }
    }
}

// ============================================================================
// Request/Response types
// ============================================================================

/// Btrieve request structure
#[derive(Debug, Clone, Default)]
pub struct BtrieveRequest {
    pub operation_code: u32,
    pub position_block: Vec<u8>,
    pub data_buffer: Vec<u8>,
    pub data_buffer_length: u32,
    pub key_buffer: Vec<u8>,
    pub key_buffer_length: u32,
    pub key_number: i32,
    pub file_path: String,
    pub open_mode: i32,
    pub lock_bias: u32,
    pub client_id: u64,
}

/// Btrieve response structure
#[derive(Debug, Clone, Default)]
pub struct BtrieveResponse {
    pub status_code: u32,
    pub position_block: Vec<u8>,
    pub data_buffer: Vec<u8>,
    pub key_buffer: Vec<u8>,
}
