//! TCP server and per-connection session lifecycle.

use std::io::{BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing::{debug, info, warn};
use xtrieve_engine::status_codes;
use xtrieve_engine::{Engine, OperationRequest, Request, Response};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn resolve_path(data_dir: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        data_dir.join(path)
    }
}

pub fn serve(listener: TcpListener, engine: Arc<Engine>, data_dir: PathBuf) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let engine = Arc::clone(&engine);
                let data_dir = data_dir.clone();
                std::thread::spawn(move || {
                    let session = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);
                    handle_client(stream, engine, data_dir, session);
                });
            }
            Err(error) => warn!("Accept failed: {error}"),
        }
    }
}

pub fn handle_client(stream: TcpStream, engine: Arc<Engine>, data_dir: PathBuf, session: u64) {
    let peer = stream.peer_addr().ok();
    debug!("Client connected: {peer:?} session {session}");

    let reader_stream = match stream.try_clone() {
        Ok(stream) => stream,
        Err(error) => {
            warn!("Failed to clone connection: {error}");
            return;
        }
    };
    let mut reader = BufReader::new(reader_stream);
    let mut writer = BufWriter::new(stream);

    loop {
        let request = match Request::from_reader(&mut reader) {
            Ok(request) => request,
            Err(error) => {
                if error.kind() != std::io::ErrorKind::UnexpectedEof {
                    warn!("Error reading request: {error}");
                }
                break;
            }
        };

        let engine_request = OperationRequest {
            operation: request.operation_code,
            lock_bias: request.lock_bias,
            position_block: request.position_block,
            data_buffer: request.data_buffer,
            key_buffer: request.key_buffer,
            key_number: request.key_number,
            path: (!request.file_path.is_empty())
                .then(|| resolve_path(&data_dir, &request.file_path)),
        };

        let result = catch_unwind(AssertUnwindSafe(|| engine.execute(session, engine_request)))
            .unwrap_or_else(|_| {
                warn!("Operation panicked for session {session}");
                xtrieve_engine::OperationResponse::error(status_codes::IO_ERROR)
            });

        let response = Response {
            status_code: result.status,
            position_block: result.position_block,
            data_buffer: result.data_buffer,
            key_buffer: result.key_buffer,
        };

        if let Err(error) = writer.write_all(&response.to_bytes()) {
            warn!("Error writing response: {error}");
            break;
        }
        if let Err(error) = writer.flush() {
            warn!("Error flushing response: {error}");
            break;
        }
    }

    engine.end_session(session);
    info!("Session {session} closed");
}
