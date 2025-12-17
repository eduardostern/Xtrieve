//! Xtrieve Daemon - Btrieve 5.1 compatible database server
//!
//! This daemon provides TCP access to Btrieve file operations using a
//! simple binary protocol similar to original Btrieve.

use std::io::{BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use anyhow::Result;
use clap::Parser;
use tracing::{info, warn, error, debug, Level};
use tracing_subscriber::FmtSubscriber;

use xtrieve_engine::operations::{Engine, OperationCode, OperationRequest};
use xtrieve_engine::file_manager::cursor::PositionBlock;
use xtrieve_engine::protocol::{Request, Response};

mod server;

/// Xtrieve daemon - Btrieve 5.1 compatible database server
#[derive(Parser, Debug)]
#[command(name = "xtrieved")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Address to listen on
    #[arg(short, long, default_value = "127.0.0.1:7419")]
    listen: String,

    /// Page cache size (number of pages)
    #[arg(short, long, default_value_t = 10000)]
    cache_size: usize,

    /// Data directory for relative paths
    #[arg(short, long, default_value = "./data")]
    data_dir: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

/// Session ID counter
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

fn resolve_path(data_dir: &PathBuf, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        data_dir.join(path)
    }
}

fn handle_client(
    stream: TcpStream,
    engine: Arc<Engine>,
    data_dir: PathBuf,
) {
    let peer = stream.peer_addr().ok();
    debug!("Client connected: {:?}", peer);

    let session_id = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);

    let mut reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
    let mut writer = BufWriter::new(stream);

    loop {
        // Read request
        let req = match Request::from_reader(&mut reader) {
            Ok(r) => r,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    debug!("Client disconnected: {:?}", peer);
                } else {
                    warn!("Error reading request: {}", e);
                }
                break;
            }
        };

        debug!("Op {} from session {}", req.operation_code, session_id);

        // Extract session from position block if available
        let pos_block = PositionBlock::from_bytes(&req.position_block);
        let stored_session = pos_block.get_session_id();
        let effective_session = if stored_session > 0 {
            stored_session
        } else {
            session_id
        };

        // Convert to engine request
        let engine_req = OperationRequest {
            operation: OperationCode::from_raw(req.operation_code as u32),
            file_path: if req.file_path.is_empty() {
                None
            } else {
                Some(resolve_path(&data_dir, &req.file_path).to_string_lossy().to_string())
            },
            position_block: req.position_block,
            data_buffer: req.data_buffer,
            key_buffer: req.key_buffer,
            key_number: req.key_number as i32,
            data_length: 0,
            key_length: 0,
            open_mode: 0,
            lock_bias: req.lock_bias as i32,
        };

        // Execute
        let result = engine.execute(effective_session, engine_req);

        // Store session in position block
        let mut result_pos_block = PositionBlock::from_bytes(&result.position_block);
        result_pos_block.set_session_id(effective_session);

        // Build response
        let response = Response {
            status_code: result.status.as_raw() as u16,
            position_block: result_pos_block.data.to_vec(),
            data_buffer: result.data_buffer,
            key_buffer: result.key_buffer,
        };

        // Send response
        if let Err(e) = writer.write_all(&response.to_bytes()) {
            warn!("Error writing response: {}", e);
            break;
        }
        if let Err(e) = writer.flush() {
            warn!("Error flushing response: {}", e);
            break;
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Set up logging
    let log_level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .with_thread_ids(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // Create data directory if needed
    std::fs::create_dir_all(&args.data_dir)?;

    // Parse listen address
    let addr: SocketAddr = args.listen.parse()?;

    // Create engine
    let engine = Arc::new(Engine::new(args.cache_size));

    // Classic Btrieve-style startup banner
    println!();
    println!("Xtrieve Record Manager Version {}", env!("CARGO_PKG_VERSION"));
    println!("Btrieve 5.10 Compatible ISAM Database Engine");
    println!();

    info!("Listening on {}", addr);
    info!("Data directory: {}", args.data_dir.display());
    info!("Cache size: {} pages", args.cache_size);

    // Bind TCP listener
    let listener = TcpListener::bind(addr)?;

    // Accept connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let engine = engine.clone();
                let data_dir = args.data_dir.clone();
                thread::spawn(move || {
                    handle_client(stream, engine, data_dir);
                });
            }
            Err(e) => {
                error!("Accept failed: {}", e);
            }
        }
    }

    Ok(())
}
