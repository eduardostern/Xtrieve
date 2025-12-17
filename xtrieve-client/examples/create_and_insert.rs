//! Integration test: Create a file and insert records

use xtrieve_client::proto::xtrieve_client::XtrieveClient;
use xtrieve_client::proto::{BtrieveRequest, FileSpec, KeySpec};

const OP_OPEN: u32 = 0;
const OP_CLOSE: u32 = 1;
const OP_INSERT: u32 = 2;
const OP_CREATE: u32 = 14;
const OP_STAT: u32 = 15;
const OP_GET_FIRST: u32 = 12;
const OP_GET_NEXT: u32 = 6;
const OP_GET_EQUAL: u32 = 5;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Xtrieve Create and Insert Test ===\n");

    let mut client = XtrieveClient::connect("http://127.0.0.1:7419").await?;
    println!("Connected to xtrieved");

    let test_file = "/Users/eduardo/xtrieve/data/test_crud.dat";

    // Delete existing test file if present
    let _ = std::fs::remove_file(test_file);

    // 1. Create a new file
    println!("\n1. Creating file: {}", test_file);

    let file_spec = FileSpec {
        record_length: 100,
        page_size: 4096,
        num_keys: 1,
        file_flags: 0,
        pre_allocation: 0,
        keys: vec![KeySpec {
            position: 0,
            length: 20,
            flags: 0, // No duplicates
            key_type: 0, // KEY_TYPE_STRING
            null_value: 0,
            manual_key_number: 0,
            acs_number: 0,
        }],
    };

    // Build data buffer for Create (file spec format)
    // Format: record_length(2) + page_size(2) + num_keys(2) + reserved(4) = 10 bytes header
    // Then: key_specs (16 bytes each)
    let mut data_buffer = Vec::new();
    data_buffer.extend_from_slice(&100u16.to_le_bytes()); // record_length (0-1)
    data_buffer.extend_from_slice(&4096u16.to_le_bytes()); // page_size (2-3)
    data_buffer.extend_from_slice(&1u16.to_le_bytes()); // num_keys (4-5)
    data_buffer.extend_from_slice(&0u32.to_le_bytes()); // reserved (6-9)

    // Key spec at offset 10 (16 bytes per key)
    // Format: position(2) + length(2) + flags(2) + key_type(1) + null_value(1) + reserved(8)
    data_buffer.extend_from_slice(&0u16.to_le_bytes()); // position
    data_buffer.extend_from_slice(&20u16.to_le_bytes()); // length
    data_buffer.extend_from_slice(&0u16.to_le_bytes()); // flags (0 = no duplicates, modifiable)
    data_buffer.push(0); // key_type (0 = STRING)
    data_buffer.push(0); // null_value
    data_buffer.extend_from_slice(&[0u8; 8]); // reserved padding

    let create_req = BtrieveRequest {
        operation_code: OP_CREATE,
        file_path: test_file.to_string(),
        data_buffer,
        data_buffer_length: 26,
        ..Default::default()
    };

    let response = client
        .execute(tonic::Request::new(create_req))
        .await?
        .into_inner();

    if response.status_code == 0 {
        println!("   File created successfully!");
    } else {
        println!("   Create failed with status: {}", response.status_code);
        return Ok(());
    }

    // 2. Open the file
    println!("\n2. Opening file...");

    let open_req = BtrieveRequest {
        operation_code: OP_OPEN,
        file_path: test_file.to_string(),
        open_mode: 0, // Read-write
        ..Default::default()
    };

    let response = client
        .execute(tonic::Request::new(open_req))
        .await?
        .into_inner();

    if response.status_code != 0 {
        println!("   Open failed with status: {}", response.status_code);
        return Ok(());
    }

    let position_block = response.position_block.clone();
    println!("   File opened successfully!");

    // 3. Get file statistics
    println!("\n3. Getting file stats...");

    let stat_req = BtrieveRequest {
        operation_code: OP_STAT,
        position_block: position_block.clone(),
        ..Default::default()
    };

    let response = client
        .execute(tonic::Request::new(stat_req))
        .await?
        .into_inner();

    if response.status_code == 0 && response.data_buffer.len() >= 10 {
        let record_len = u16::from_le_bytes([response.data_buffer[0], response.data_buffer[1]]);
        let page_size = u16::from_le_bytes([response.data_buffer[2], response.data_buffer[3]]);
        let num_keys = u16::from_le_bytes([response.data_buffer[4], response.data_buffer[5]]);
        let num_records = u32::from_le_bytes([
            response.data_buffer[6],
            response.data_buffer[7],
            response.data_buffer[8],
            response.data_buffer[9],
        ]);
        println!("   Record length: {}", record_len);
        println!("   Page size: {}", page_size);
        println!("   Number of keys: {}", num_keys);
        println!("   Number of records: {}", num_records);
    }

    // 4. Insert records
    println!("\n4. Inserting 10 test records...");

    let mut current_position = position_block.clone();

    for i in 0..10 {
        // Create a 100-byte record
        let mut record = vec![0u8; 100];

        // Key (first 20 bytes): "Record_XX           "
        let key = format!("Record_{:02}           ", i);
        record[..20].copy_from_slice(&key.as_bytes()[..20]);

        // Data (remaining 80 bytes)
        let data = format!("This is test record number {}. Some padding here.", i);
        let data_bytes = data.as_bytes();
        let copy_len = data_bytes.len().min(80);
        record[20..20 + copy_len].copy_from_slice(&data_bytes[..copy_len]);

        let insert_req = BtrieveRequest {
            operation_code: OP_INSERT,
            position_block: current_position.clone(),
            data_buffer: record,
            data_buffer_length: 100,
            key_number: 0,
            ..Default::default()
        };

        let response = client
            .execute(tonic::Request::new(insert_req))
            .await?
            .into_inner();

        if response.status_code == 0 {
            println!("   Inserted Record_{:02}", i);
            current_position = response.position_block;
        } else {
            println!(
                "   Insert failed for record {} with status: {}",
                i, response.status_code
            );
        }
    }

    // 5. Get stats again to verify inserts
    println!("\n5. Verifying inserts (getting stats)...");

    let stat_req = BtrieveRequest {
        operation_code: OP_STAT,
        position_block: current_position.clone(),
        ..Default::default()
    };

    let response = client
        .execute(tonic::Request::new(stat_req))
        .await?
        .into_inner();

    if response.status_code == 0 && response.data_buffer.len() >= 10 {
        let num_records = u32::from_le_bytes([
            response.data_buffer[6],
            response.data_buffer[7],
            response.data_buffer[8],
            response.data_buffer[9],
        ]);
        println!("   Number of records after insert: {}", num_records);
    }

    // 6. Read records using GetFirst/GetNext
    println!("\n6. Reading all records with GetFirst/GetNext...");

    // GetFirst
    let get_first_req = BtrieveRequest {
        operation_code: OP_GET_FIRST,
        position_block: current_position.clone(),
        key_number: 0,
        data_buffer_length: 100,
        ..Default::default()
    };

    let response = client
        .execute(tonic::Request::new(get_first_req))
        .await?
        .into_inner();

    let mut count = 0;
    if response.status_code == 0 {
        let key = String::from_utf8_lossy(&response.data_buffer[..20]);
        println!("   First: {}", key.trim());
        count += 1;
        current_position = response.position_block;

        // GetNext loop
        loop {
            let get_next_req = BtrieveRequest {
                operation_code: OP_GET_NEXT,
                position_block: current_position.clone(),
                key_number: 0,
                data_buffer_length: 100,
                ..Default::default()
            };

            let response = client
                .execute(tonic::Request::new(get_next_req))
                .await?
                .into_inner();

            if response.status_code == 9 {
                // End of file
                break;
            } else if response.status_code == 0 {
                let key = String::from_utf8_lossy(&response.data_buffer[..20]);
                println!("   Next:  {}", key.trim());
                count += 1;
                current_position = response.position_block;
            } else {
                println!("   GetNext error: {}", response.status_code);
                break;
            }
        }
    } else {
        println!("   GetFirst failed with status: {}", response.status_code);
    }

    println!("\n   Total records read: {}", count);

    // 7. Search for specific record
    println!("\n7. Searching for 'Record_05'...");

    let mut search_key = vec![0u8; 20];
    let key_str = "Record_05           ";
    search_key.copy_from_slice(key_str.as_bytes());

    let get_equal_req = BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: current_position.clone(),
        key_buffer: search_key,
        key_buffer_length: 20,
        key_number: 0,
        data_buffer_length: 100,
        ..Default::default()
    };

    let response = client
        .execute(tonic::Request::new(get_equal_req))
        .await?
        .into_inner();

    if response.status_code == 0 {
        let key = String::from_utf8_lossy(&response.data_buffer[..20]);
        let data = String::from_utf8_lossy(&response.data_buffer[20..70]);
        println!("   Found: {}", key.trim());
        println!("   Data:  {}", data.trim());
        current_position = response.position_block;
    } else {
        println!("   Search failed with status: {}", response.status_code);
    }

    // 8. Close file
    println!("\n8. Closing file...");

    let close_req = BtrieveRequest {
        operation_code: OP_CLOSE,
        position_block: current_position,
        ..Default::default()
    };

    let response = client
        .execute(tonic::Request::new(close_req))
        .await?
        .into_inner();

    if response.status_code == 0 {
        println!("   File closed successfully!");
    } else {
        println!("   Close failed with status: {}", response.status_code);
    }

    println!("\n=== Test Complete ===");
    Ok(())
}
