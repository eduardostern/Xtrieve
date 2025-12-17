//! Read a real Btrieve 5.1 file created by DOS Btrieve

use xtrieve_client::{XtrieveClient, BtrieveRequest};

const OP_OPEN: u32 = 0;
const OP_CLOSE: u32 = 1;
const OP_GET_FIRST: u32 = 12;
const OP_GET_NEXT: u32 = 6;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Reading Btrieve 5.1 TEST.DAT file");
    println!("=================================\n");

    let mut client = XtrieveClient::connect("127.0.0.1:7419")?;
    println!("Connected to Xtrieve server\n");

    // Open TEST.DAT
    let resp = client.execute(BtrieveRequest {
        operation_code: OP_OPEN,
        file_path: "TEST.DAT".to_string(),
        ..Default::default()
    })?;

    if resp.status_code != 0 {
        println!("Open failed: status {}", resp.status_code);
        return Ok(());
    }
    println!("Opened TEST.DAT\n");

    let pos_block = resp.position_block;

    // Read first record
    let mut resp = client.execute(BtrieveRequest {
        operation_code: OP_GET_FIRST,
        position_block: pos_block.clone(),
        key_number: 0,
        ..Default::default()
    })?;

    println!("Records from Btrieve 5.1 file:");
    println!("{:<8} {}", "ID", "Name");
    println!("{}", "-".repeat(40));

    let mut count = 0;
    while resp.status_code == 0 {
        let data = &resp.data_buffer;

        // Parse record: ID at offset 0 (4 bytes), Name at offset 4
        if data.len() >= 8 {
            let id = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);

            // Find end of name string
            let name_end = data[4..].iter().position(|&b| b == 0).unwrap_or(30);
            let name = String::from_utf8_lossy(&data[4..4+name_end]);

            println!("{:<8} {}", id, name);
            count += 1;
        }

        resp = client.execute(BtrieveRequest {
            operation_code: OP_GET_NEXT,
            position_block: resp.position_block,
            key_number: 0,
            ..Default::default()
        })?;
    }

    println!("{}", "-".repeat(40));
    println!("Total: {} records (status {} = end of file)\n", count, resp.status_code);

    // Close
    client.execute(BtrieveRequest {
        operation_code: OP_CLOSE,
        position_block: pos_block,
        ..Default::default()
    })?;

    println!("Success! Xtrieve read a real Btrieve 5.1 file.");
    Ok(())
}
