//! Xtrieve Daemon - Btrieve 5.1 compatible database server
//!
//! This daemon provides gRPC access to Btrieve file operations.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tokio::signal;
use tokio::sync::broadcast;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

use xtrieve_engine::operations::{Engine, OperationCode, OperationRequest, OperationResponse};
use xtrieve_engine::StatusCode;

mod server;

pub mod proto {
    tonic::include_proto!("xtrieve");
}

use proto::xtrieve_server::{Xtrieve, XtrieveServer};
use proto::{
    BtrieveRequest, BtrieveResponse, StatusRequest, StatusResponse,
    ShutdownRequest, ShutdownResponse, OpenFileInfo, ServerStatistics,
};

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
    #[arg(short, long, default_value = ".")]
    data_dir: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

/// Session ID counter
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Xtrieve gRPC service implementation
pub struct XtrieveService {
    engine: Arc<Engine>,
    data_dir: PathBuf,
    shutdown_tx: broadcast::Sender<()>,
    start_time: std::time::Instant,
}

impl XtrieveService {
    pub fn new(
        engine: Arc<Engine>,
        data_dir: PathBuf,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        XtrieveService {
            engine,
            data_dir,
            shutdown_tx,
            start_time: std::time::Instant::now(),
        }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            self.data_dir.join(path)
        }
    }
}

#[tonic::async_trait]
impl Xtrieve for XtrieveService {
    async fn execute(
        &self,
        request: Request<BtrieveRequest>,
    ) -> Result<Response<BtrieveResponse>, Status> {
        let req = request.into_inner();

        // Assign session ID (in real impl, track per connection)
        let session_id = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);

        // Convert gRPC request to engine request
        let engine_req = OperationRequest {
            operation: OperationCode::from_raw(req.operation_code),
            file_path: if req.file_path.is_empty() {
                None
            } else {
                Some(self.resolve_path(&req.file_path).to_string_lossy().to_string())
            },
            position_block: req.position_block,
            data_buffer: req.data_buffer,
            key_buffer: req.key_buffer,
            key_number: req.key_number,
            data_length: req.data_buffer_length,
            key_length: req.key_buffer_length,
            open_mode: req.open_mode,
            lock_bias: req.lock_bias,
        };

        // Execute operation
        let response = self.engine.execute(session_id, engine_req);

        // Convert response
        Ok(Response::new(BtrieveResponse {
            status_code: response.status.as_raw() as u32,
            position_block: response.position_block,
            data_buffer: response.data_buffer,
            data_length: response.data_length,
            key_buffer: response.key_buffer,
            key_length: response.key_length,
        }))
    }

    type ExecuteExtendedStream = tokio_stream::wrappers::ReceiverStream<Result<BtrieveResponse, Status>>;

    async fn execute_extended(
        &self,
        request: Request<BtrieveRequest>,
    ) -> Result<Response<Self::ExecuteExtendedStream>, Status> {
        // For now, just return single response
        let (tx, rx) = tokio::sync::mpsc::channel(1);

        let response = self.execute(request).await?;
        let _ = tx.send(Ok(response.into_inner())).await;

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn get_status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = request.into_inner();

        let cache_stats = self.engine.cache.stats();
        let open_files = self.engine.files.len() as u32;

        let mut open_file_list = Vec::new();
        if req.include_open_files {
            // TODO: Iterate open files and build list
        }

        let statistics = if req.include_statistics {
            Some(ServerStatistics {
                total_operations: cache_stats.hits + cache_stats.misses,
                total_reads: cache_stats.hits,
                total_writes: cache_stats.dirty_writes,
                cache_hits: cache_stats.hits,
                cache_misses: cache_stats.misses,
            })
        } else {
            None
        };

        Ok(Response::new(StatusResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            open_files,
            active_transactions: 0, // TODO: Track transactions
            open_file_list,
            statistics,
        }))
    }

    async fn shutdown(
        &self,
        request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        let req = request.into_inner();

        info!("Shutdown requested (graceful={})", req.graceful);

        // Signal shutdown
        let _ = self.shutdown_tx.send(());

        Ok(Response::new(ShutdownResponse {
            accepted: true,
            message: "Shutdown initiated".to_string(),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
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

    // Parse listen address
    let addr: SocketAddr = args.listen.parse()?;

    // Create engine
    let engine = Arc::new(Engine::new(args.cache_size));

    // Shutdown channel
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);

    // Create service
    let service = XtrieveService::new(
        engine.clone(),
        args.data_dir.clone(),
        shutdown_tx.clone(),
    );

    info!("Starting xtrieved v{}", env!("CARGO_PKG_VERSION"));
    info!("Listening on {}", addr);
    info!("Data directory: {}", args.data_dir.display());
    info!("Cache size: {} pages", args.cache_size);

    // Start server
    let server = Server::builder()
        .add_service(XtrieveServer::new(service))
        .serve_with_shutdown(addr, async move {
            // Wait for shutdown signal (Ctrl+C or explicit shutdown)
            tokio::select! {
                _ = signal::ctrl_c() => {
                    info!("Received Ctrl+C, shutting down...");
                }
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown request...");
                }
            }
        });

    server.await?;

    // Cleanup
    info!("Shutting down engine...");
    engine.shutdown();
    info!("Shutdown complete");

    Ok(())
}
