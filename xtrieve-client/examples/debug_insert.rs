//! Debug test for insert issue

use xtrieve_client::proto::xtrieve_client::XtrieveClient;
use xtrieve_client::proto::BtrieveRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = XtrieveClient::connect("http://127.0.0.1:7419").await?;
    let test_file = "/Users/eduardo/xtrieve/data/test_debug.dat";
    let _ = std::fs::remove_file(test_file);

    // Create
    let mut data_buffer = Vec::new();
    data_buffer.extend_from_slice(&100u16.to_le_bytes());
    data_buffer.extend_from_slice(&4096u16.to_le_bytes());
    data_buffer.extend_from_slice(&1u16.to_le_bytes());
    data_buffer.extend_from_slice(&0u32.to_le_bytes());
    data_buffer.extend_from_slice(&0u16.to_le_bytes()); // position
    data_buffer.extend_from_slice(&20u16.to_le_bytes()); // length
    data_buffer.extend_from_slice(&0u16.to_le_bytes()); // flags
    data_buffer.push(0);
    data_buffer.push(0);
    data_buffer.extend_from_slice(&[0u8; 8]);

    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: 14, // CREATE
        file_path: test_file.to_string(),
        data_buffer,
        ..Default::default()
    })).await?.into_inner();
    println!("Create status: {}", resp.status_code);

    // Open
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: 0, // OPEN
        file_path: test_file.to_string(),
        ..Default::default()
    })).await?.into_inner();
    println!("Open status: {}", resp.status_code);
    println!("Position block length: {}", resp.position_block.len());
    
    // Print bytes 64-128 (file path area)
    if resp.position_block.len() >= 128 {
        let path_bytes = &resp.position_block[64..];
        let end = path_bytes.iter().position(|&b| b == 0).unwrap_or(64);
        println!("Path from Open: '{}'", String::from_utf8_lossy(&path_bytes[..end]));
    }
    
    let pos1 = resp.position_block;
    
    // First insert
    let mut record = vec![0u8; 100];
    record[..9].copy_from_slice(b"Record_01");
    
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: 2, // INSERT
        position_block: pos1,
        data_buffer: record.clone(),
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    println!("\nInsert 1 status: {}", resp.status_code);
    println!("Position block length after insert 1: {}", resp.position_block.len());
    
    if resp.position_block.len() >= 128 {
        let path_bytes = &resp.position_block[64..];
        let end = path_bytes.iter().position(|&b| b == 0).unwrap_or(64);
        println!("Path after Insert 1: '{}'", String::from_utf8_lossy(&path_bytes[..end]));
    }
    
    let pos2 = resp.position_block;

    // Second insert
    record[..9].copy_from_slice(b"Record_02");
    
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: 2, // INSERT
        position_block: pos2,
        data_buffer: record,
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    println!("\nInsert 2 status: {}", resp.status_code);
    
    Ok(())
}
