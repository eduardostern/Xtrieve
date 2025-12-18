//! Test B+ tree key operations with real Btrieve 5.1 files

use xtrieve_client::{XtrieveClient, BtrieveRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing B+ Tree Key Operations ===\n");

    let mut client = XtrieveClient::connect("127.0.0.1:7419")?;

    // Test with TESTE2.DAT (3 records, no deletions)
    println!("--- Testing TESTE2.DAT (3 records) ---");
    test_file(&mut client, "TESTE2.DAT")?;

    // Test with TESTE3.DAT (3 records, ID=2 deleted)
    println!("\n--- Testing TESTE3.DAT (3 records, ID=2 deleted) ---");
    test_file(&mut client, "TESTE3.DAT")?;

    // Test with TESTE.DAT (600 records)
    println!("\n--- Testing TESTE.DAT (600 records) ---");
    test_file(&mut client, "TESTE.DAT")?;

    Ok(())
}

fn test_file(client: &mut XtrieveClient, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Open file
    let resp = client.execute(BtrieveRequest {
        operation_code: 0, // OPEN
        file_path: filename.to_string(),
        ..Default::default()
    })?;

    println!("Open {}: status={}", filename, resp.status_code);
    if resp.status_code != 0 {
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

    if resp.status_code == 0 && resp.data_buffer.len() >= 16 {
        let data = &resp.data_buffer;
        let record_len = u16::from_le_bytes([data[0], data[1]]);
        let page_size = u16::from_le_bytes([data[2], data[3]]);
        let num_keys = u16::from_le_bytes([data[4], data[5]]);
        let num_records = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);

        println!("  Record length: {} bytes, Page size: {} bytes", record_len, page_size);
        println!("  Number of keys: {}, Number of records: {}", num_keys, num_records);
    }

    // Test GetFirst (operation 12) - first record by key 0
    println!("\n  GetFirst (key order):");
    let resp = client.execute(BtrieveRequest {
        operation_code: 12, // GET_FIRST
        position_block: pos.clone(),
        key_number: 0,
        data_buffer_length: 64,
        ..Default::default()
    })?;

    println!("    Status: {}", resp.status_code);
    if resp.status_code == 0 {
        pos = resp.position_block.clone();
        print_record(&resp.data_buffer);

        // Test GetNext (operation 6) - next records by key
        println!("  GetNext (key order):");
        let mut count = 1;
        loop {
            let resp = client.execute(BtrieveRequest {
                operation_code: 6, // GET_NEXT
                position_block: pos.clone(),
                key_number: 0,
                data_buffer_length: 64,
                ..Default::default()
            })?;

            if resp.status_code != 0 {
                println!("    Status: {} (EOF after {} records)", resp.status_code, count);
                break;
            }

            pos = resp.position_block.clone();
            print_record(&resp.data_buffer);
            count += 1;

            if count >= 10 {
                println!("    ... (stopped at 10 records)");
                break;
            }
        }
    }

    // Test GetLast (operation 13)
    println!("  GetLast (key order):");
    let resp = client.execute(BtrieveRequest {
        operation_code: 13, // GET_LAST
        position_block: pos.clone(),
        key_number: 0,
        data_buffer_length: 64,
        ..Default::default()
    })?;

    println!("    Status: {}", resp.status_code);
    if resp.status_code == 0 {
        pos = resp.position_block.clone();
        print_record(&resp.data_buffer);

        // Test GetPrevious (operation 7) - previous records by key
        println!("  GetPrevious (from last):");
        let mut count = 1;
        loop {
            let resp = client.execute(BtrieveRequest {
                operation_code: 7, // GET_PREVIOUS
                position_block: pos.clone(),
                key_number: 0,
                data_buffer_length: 64,
                ..Default::default()
            })?;

            if resp.status_code != 0 {
                println!("    Status: {} (BOF after {} records)", resp.status_code, count);
                break;
            }

            pos = resp.position_block.clone();
            print_record(&resp.data_buffer);
            count += 1;

            if count >= 10 {
                println!("    ... (stopped at 10 records)");
                break;
            }
        }
    }

    // Test GetEqual (operation 5) - search for specific key
    println!("  GetEqual (search for key=2):");
    let search_key = 2i32.to_le_bytes().to_vec();
    let resp = client.execute(BtrieveRequest {
        operation_code: 5, // GET_EQUAL
        position_block: pos.clone(),
        key_number: 0,
        key_buffer: search_key,
        data_buffer_length: 64,
        ..Default::default()
    })?;

    println!("    Status: {}", resp.status_code);
    if resp.status_code == 0 {
        print_record(&resp.data_buffer);
    } else {
        println!("    Key not found (expected if ID=2 was deleted)");
    }

    // Close
    let resp = client.execute(BtrieveRequest {
        operation_code: 1, // CLOSE
        position_block: pos,
        ..Default::default()
    })?;
    println!("  Close: status={}", resp.status_code);

    Ok(())
}

fn print_record(data: &[u8]) {
    if data.len() >= 4 {
        let id = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let text_end = data[4..].iter().position(|&b| b == 0).unwrap_or(28);
        let text = String::from_utf8_lossy(&data[4..4+text_end.min(28)]);
        println!("    ID={:4}: {}", id, text);
    }
}
