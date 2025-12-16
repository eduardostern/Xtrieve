//! Quick integration test for Xtrieve connection

use xtrieve_client::proto::xtrieve_client::XtrieveClient;
use xtrieve_client::proto::StatusRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Connecting to xtrieved at 127.0.0.1:7419...");

    let mut client = XtrieveClient::connect("http://127.0.0.1:7419").await?;
    println!("Connected!");

    // Request server status
    println!("Requesting server status...");
    let request = tonic::Request::new(StatusRequest {
        include_open_files: true,
        include_statistics: true,
    });
    let response = client.get_status(request).await?;
    let status = response.into_inner();

    println!("\nServer Status:");
    println!("  Version: {}", status.version);
    println!("  Uptime: {} seconds", status.uptime_seconds);
    println!("  Open files: {}", status.open_files);
    println!("  Active transactions: {}", status.active_transactions);

    if let Some(stats) = status.statistics {
        println!("\nStatistics:");
        println!("  Total operations: {}", stats.total_operations);
        println!("  Total reads: {}", stats.total_reads);
        println!("  Total writes: {}", stats.total_writes);
        println!("  Cache hits: {}", stats.cache_hits);
        println!("  Cache misses: {}", stats.cache_misses);
    }

    println!("\nXtrieve is working correctly!");
    Ok(())
}
