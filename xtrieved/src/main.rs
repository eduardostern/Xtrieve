//! Xtrieve daemon - Btrieve/N 5.10a compatible database server.

use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use xtrieve_engine::Engine;
use xtrieved::server;

#[derive(Parser, Debug)]
#[command(name = "xtrieved")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Address to listen on
    #[arg(short, long, default_value = "127.0.0.1:7419")]
    listen: String,

    /// Data directory for relative paths
    #[arg(short, long, default_value = "./data")]
    data_dir: PathBuf,

    /// Transaction log path (defaults to BTRIEVE.TRN next to the data file)
    #[arg(long)]
    transaction_log: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

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

    std::fs::create_dir_all(&args.data_dir)?;

    let addr: SocketAddr = args.listen.parse()?;
    let engine = Arc::new(Engine::new(args.data_dir.clone()));
    if let Some(log) = &args.transaction_log {
        engine.set_transaction_log(log);
    }

    println!();
    println!(
        "Xtrieve Record Manager Version {}",
        env!("CARGO_PKG_VERSION")
    );
    println!("Btrieve/N 5.10a compatible ISAM database engine");
    println!();

    info!("Listening on {}", addr);
    info!("Data directory: {}", args.data_dir.display());

    let listener = TcpListener::bind(addr)?;
    server::serve(listener, engine, args.data_dir);
    Ok(())
}
