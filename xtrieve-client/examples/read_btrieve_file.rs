//! Test reading a real Btrieve 5.1 file created in DOSBox

use xtrieve_client::{XtrieveClient, BtrieveRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Reading Real Btrieve 5.1 File ===\n");

    let mut client = XtrieveClient::connect("127.0.0.1:7419")?;

    // Open TESTE.DAT (created by original Btrieve in DOSBox)
    let resp = client.execute(BtrieveRequest {
        operation_code: 0, // OPEN
        file_path: "TESTE.DAT".to_string(),
        ..Default::default()
    })?;

    println!("Open status: {}", resp.status_code);
    if resp.status_code != 0 {
        println!("Failed to open file!");
        return Ok(());
    }

    let mut pos = resp.position_block;

    // Get STAT to see file info
    let resp = client.execute(BtrieveRequest {
        operation_code: 15, // STAT
        position_block: pos.clone(),
        data_buffer_length: 256,
        ..Default::default()
    })?;

    println!("Stat status: {}", resp.status_code);
    if resp.status_code == 0 && resp.data_buffer.len() >= 16 {
        let data = &resp.data_buffer;
        let record_len = u16::from_le_bytes([data[0], data[1]]);
        let page_size = u16::from_le_bytes([data[2], data[3]]);
        let num_keys = u16::from_le_bytes([data[4], data[5]]);
        let num_records = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);

        println!("\nFile Statistics:");
        println!("  Record length: {} bytes", record_len);
        println!("  Page size: {} bytes", page_size);
        println!("  Number of keys: {}", num_keys);
        println!("  Number of records: {}", num_records);
    }

    // Get first record using STEP (physical order)
    println!("\n--- Getting First 10 Records (Physical Order) ---");
    let resp = client.execute(BtrieveRequest {
        operation_code: 33, // STEP_FIRST
        position_block: pos.clone(),
        data_buffer_length: 64,
        ..Default::default()
    })?;

    println!("StepFirst status: {}", resp.status_code);

    if resp.status_code == 0 {
        pos = resp.position_block.clone();
        print_record(&resp.data_buffer);

        // Get next 9 records
        for i in 1..10 {
            let resp = client.execute(BtrieveRequest {
                operation_code: 24, // STEP_NEXT
                position_block: pos.clone(),
                data_buffer_length: 64,
                ..Default::default()
            })?;

            if resp.status_code != 0 {
                println!("StepNext {} status: {} (end of file?)", i, resp.status_code);
                break;
            }

            pos = resp.position_block.clone();
            print_record(&resp.data_buffer);
        }
    }

    // Get last record
    println!("\n--- Getting Last Record (Physical Order) ---");
    let resp = client.execute(BtrieveRequest {
        operation_code: 34, // STEP_LAST
        position_block: pos.clone(),
        data_buffer_length: 64,
        ..Default::default()
    })?;

    println!("StepLast status: {}", resp.status_code);
    if resp.status_code == 0 {
        print_record(&resp.data_buffer);
    }

    // Close
    let resp = client.execute(BtrieveRequest {
        operation_code: 1, // CLOSE
        position_block: pos,
        ..Default::default()
    })?;
    println!("\nClose status: {}", resp.status_code);

    Ok(())
}

fn print_record(data: &[u8]) {
    if data.len() >= 4 {
        let id = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let text_end = data[4..].iter().position(|&b| b == 0).unwrap_or(28);
        let text = String::from_utf8_lossy(&data[4..4+text_end.min(28)]);
        println!("  ID={:4}: {}", id, text);
    }
}
