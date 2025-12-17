//! Test ACID Isolation - uncommitted changes should not be visible to other sessions

use xtrieve_client::{XtrieveClient, BtrieveRequest};

// Operation codes
const OP_OPEN: u32 = 0;
const OP_CLOSE: u32 = 1;
const OP_INSERT: u32 = 2;
const OP_UPDATE: u32 = 3;
const OP_GET_EQUAL: u32 = 5;
const OP_CREATE: u32 = 14;
const OP_BEGIN_TRANSACTION: u32 = 19;
const OP_END_TRANSACTION: u32 = 20;
const OP_ABORT_TRANSACTION: u32 = 21;

// User session IDs
const USER_A: u64 = 100;
const USER_B: u64 = 200;

fn build_create_buffer() -> Vec<u8> {
    let mut buf = Vec::new();
    // File spec: record_len(2), page_size(2), num_keys(2), unused(4)
    buf.extend_from_slice(&100u16.to_le_bytes());  // record length
    buf.extend_from_slice(&4096u16.to_le_bytes()); // page size
    buf.extend_from_slice(&1u16.to_le_bytes());    // number of keys
    buf.extend_from_slice(&0u32.to_le_bytes());    // unused
    // Key spec: position(2), length(2), flags(2), key_type(1), null_val(1), reserved(8)
    buf.extend_from_slice(&0u16.to_le_bytes());    // key position
    buf.extend_from_slice(&20u16.to_le_bytes());   // key length
    buf.extend_from_slice(&0u16.to_le_bytes());    // flags
    buf.push(0); // key type (string)
    buf.push(0); // null value
    buf.extend_from_slice(&[0u8; 8]);              // reserved
    buf
}

fn make_record(key: &str, data: &str) -> Vec<u8> {
    let mut record = vec![0u8; 100];
    let key_bytes = key.as_bytes();
    let data_bytes = data.as_bytes();
    record[..key_bytes.len().min(20)].copy_from_slice(&key_bytes[..key_bytes.len().min(20)]);
    record[20..20 + data_bytes.len().min(80)].copy_from_slice(&data_bytes[..data_bytes.len().min(80)]);
    record
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Xtrieve ACID Isolation Test ===\n");

    let mut client = XtrieveClient::connect("127.0.0.1:7419")?;

    let test_file = "test_isolation.dat";

    // Clean up any existing file
    let _ = std::fs::remove_file(format!("./data/{}", test_file));

    // Create file (User A)
    println!("1. Creating test file...");
    let create_resp = client.execute(BtrieveRequest {
        operation_code: OP_CREATE,
        file_path: test_file.to_string(),
        client_id: USER_A,
        data_buffer: build_create_buffer(),
        ..Default::default()
    })?;

    if create_resp.status_code != 0 {
        println!("   FAIL: Create failed with status {}", create_resp.status_code);
        return Ok(());
    }
    println!("   OK: File created\n");

    // Open file as User A
    println!("2. User A opens file...");
    let open_a = client.execute(BtrieveRequest {
        operation_code: OP_OPEN,
        file_path: test_file.to_string(),
        client_id: USER_A,
        open_mode: 0,
        ..Default::default()
    })?;

    if open_a.status_code != 0 {
        println!("   FAIL: User A open failed with status {}", open_a.status_code);
        return Ok(());
    }
    let pos_block_a = open_a.position_block.clone();
    println!("   OK: User A has file open\n");

    // Open file as User B (new connection for separate session)
    println!("3. User B opens file...");
    let mut client_b = XtrieveClient::connect("127.0.0.1:7419")?;
    let open_b = client_b.execute(BtrieveRequest {
        operation_code: OP_OPEN,
        file_path: test_file.to_string(),
        client_id: USER_B,
        open_mode: 0,
        ..Default::default()
    })?;

    if open_b.status_code != 0 {
        println!("   FAIL: User B open failed with status {}", open_b.status_code);
        return Ok(());
    }
    let pos_block_b = open_b.position_block.clone();
    println!("   OK: User B has file open\n");

    // Insert initial record (no transaction - should be visible immediately)
    println!("4. User A inserts 'Apple' (no transaction)...");
    let insert_resp = client.execute(BtrieveRequest {
        operation_code: OP_INSERT,
        position_block: pos_block_a.clone(),
        data_buffer: make_record("Apple", "Initial record"),
        data_buffer_length: 100,
        client_id: USER_A,
        ..Default::default()
    })?;

    if insert_resp.status_code != 0 {
        println!("   FAIL: Insert failed with status {}", insert_resp.status_code);
        return Ok(());
    }
    println!("   OK: Apple inserted\n");

    // User B should see Apple
    println!("5. User B searches for 'Apple' (should find it)...");
    let mut key_buf = vec![0u8; 20];
    key_buf[..5].copy_from_slice(b"Apple");
    let get_b = client_b.execute(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block_b.clone(),
        key_buffer: key_buf.clone(),
        key_number: 0,
        client_id: USER_B,
        ..Default::default()
    })?;

    if get_b.status_code == 0 {
        println!("   OK: User B found 'Apple' as expected\n");
    } else {
        println!("   FAIL: User B should see 'Apple' (status {})\n", get_b.status_code);
        return Ok(());
    }

    // ========== ISOLATION TEST 1: INSERT ==========
    println!("========== TEST 1: INSERT ISOLATION ==========\n");

    // User A begins transaction
    println!("6. User A begins transaction...");
    let begin_resp = client.execute(BtrieveRequest {
        operation_code: OP_BEGIN_TRANSACTION,
        position_block: pos_block_a.clone(),
        client_id: USER_A,
        ..Default::default()
    })?;

    if begin_resp.status_code != 0 {
        println!("   FAIL: Begin transaction failed with status {}", begin_resp.status_code);
        return Ok(());
    }
    println!("   OK: Transaction started\n");

    // User A inserts 'Banana' in transaction
    println!("7. User A inserts 'Banana' (in transaction)...");
    let insert_trans = client.execute(BtrieveRequest {
        operation_code: OP_INSERT,
        position_block: pos_block_a.clone(),
        data_buffer: make_record("Banana", "Uncommitted record"),
        data_buffer_length: 100,
        client_id: USER_A,
        ..Default::default()
    })?;

    if insert_trans.status_code != 0 {
        println!("   FAIL: Insert in transaction failed with status {}", insert_trans.status_code);
        return Ok(());
    }
    println!("   OK: Banana inserted (uncommitted)\n");

    // User A should see Banana
    println!("8. User A searches for 'Banana' (should find it - own transaction)...");
    let mut key_buf = vec![0u8; 20];
    key_buf[..6].copy_from_slice(b"Banana");
    let get_a = client.execute(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block_a.clone(),
        key_buffer: key_buf.clone(),
        key_number: 0,
        client_id: USER_A,
        ..Default::default()
    })?;

    if get_a.status_code == 0 {
        println!("   OK: User A found 'Banana' (own uncommitted data)\n");
    } else {
        println!("   WARN: User A couldn't find own uncommitted 'Banana' (status {})\n", get_a.status_code);
    }

    // User B should be BLOCKED from reading Banana (Btrieve 5.1 isolation via locks)
    println!("9. User B searches for 'Banana' (should be BLOCKED - ISOLATION via locks)...");
    let get_b_banana = client_b.execute(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block_b.clone(),
        key_buffer: key_buf.clone(),
        key_number: 0,
        client_id: USER_B,
        ..Default::default()
    })?;

    if get_b_banana.status_code == 79 { // RecordInUse - locked by User A's transaction
        println!("   \x1b[32mPASS\x1b[0m: User B blocked from uncommitted 'Banana' (status 79 - Record Locked)\n");
    } else if get_b_banana.status_code == 4 { // KeyNotFound
        println!("   \x1b[33mWARN\x1b[0m: User B got KeyNotFound - isolation works but via different mechanism\n");
    } else if get_b_banana.status_code == 0 {
        println!("   \x1b[31mFAIL\x1b[0m: User B can see uncommitted 'Banana' - ISOLATION VIOLATION!\n");
    } else {
        println!("   WARN: Unexpected status {} for User B search\n", get_b_banana.status_code);
    }

    // User A commits
    println!("10. User A commits transaction...");
    let commit_resp = client.execute(BtrieveRequest {
        operation_code: OP_END_TRANSACTION,
        position_block: pos_block_a.clone(),
        client_id: USER_A,
        ..Default::default()
    })?;

    if commit_resp.status_code != 0 {
        println!("   FAIL: Commit failed with status {}", commit_resp.status_code);
        return Ok(());
    }
    println!("   OK: Transaction committed\n");

    // Now User B should see Banana
    println!("11. User B searches for 'Banana' (should find it now - committed)...");
    let get_b_after = client_b.execute(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block_b.clone(),
        key_buffer: key_buf.clone(),
        key_number: 0,
        client_id: USER_B,
        ..Default::default()
    })?;

    if get_b_after.status_code == 0 {
        println!("   \x1b[32mPASS\x1b[0m: User B now sees committed 'Banana'\n");
    } else {
        println!("   \x1b[31mFAIL\x1b[0m: User B cannot see committed 'Banana' (status {})\n", get_b_after.status_code);
    }

    // ========== ISOLATION TEST 2: UPDATE ==========
    println!("========== TEST 2: UPDATE ISOLATION ==========\n");

    // User A begins another transaction
    println!("12. User A begins new transaction...");
    let begin2 = client.execute(BtrieveRequest {
        operation_code: OP_BEGIN_TRANSACTION,
        position_block: pos_block_a.clone(),
        client_id: USER_A,
        ..Default::default()
    })?;
    if begin2.status_code != 0 {
        println!("   FAIL: Begin transaction failed");
        return Ok(());
    }
    println!("   OK: Transaction started\n");

    // User A gets Apple for update
    println!("13. User A gets 'Apple' for update...");
    let mut key_buf = vec![0u8; 20];
    key_buf[..5].copy_from_slice(b"Apple");
    let get_apple = client.execute(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block_a.clone(),
        key_buffer: key_buf.clone(),
        key_number: 0,
        client_id: USER_A,
        ..Default::default()
    })?;

    if get_apple.status_code != 0 {
        println!("   FAIL: Get Apple failed");
        return Ok(());
    }
    let apple_pos = get_apple.position_block.clone();
    println!("   OK: Got Apple\n");

    // User A updates Apple
    println!("14. User A updates 'Apple' data to 'MODIFIED IN TRANSACTION'...");
    let update_resp = client.execute(BtrieveRequest {
        operation_code: OP_UPDATE,
        position_block: apple_pos.clone(),
        data_buffer: make_record("Apple", "MODIFIED IN TRANSACTION"),
        data_buffer_length: 100,
        client_id: USER_A,
        ..Default::default()
    })?;

    if update_resp.status_code != 0 {
        println!("   FAIL: Update failed with status {}", update_resp.status_code);
        return Ok(());
    }
    println!("   OK: Apple updated (uncommitted)\n");

    // User B reads Apple - should be BLOCKED (Btrieve 5.1 isolation via locks)
    println!("15. User B reads 'Apple' (should be BLOCKED - ISOLATION via locks)...");
    let get_b_apple = client_b.execute(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block_b.clone(),
        key_buffer: key_buf.clone(),
        key_number: 0,
        client_id: USER_B,
        ..Default::default()
    })?;

    if get_b_apple.status_code == 79 { // RecordInUse - locked by User A's transaction
        println!("   \x1b[32mPASS\x1b[0m: User B blocked from modified 'Apple' (status 79 - Record Locked)\n");
    } else if get_b_apple.status_code == 0 {
        let data = String::from_utf8_lossy(&get_b_apple.data_buffer[20..]);
        let data_str = data.trim_end_matches('\0');
        if data_str.contains("MODIFIED") {
            println!("   \x1b[31mFAIL\x1b[0m: User B sees MODIFIED data - ISOLATION VIOLATION!");
            println!("         Data: {}\n", data_str);
        } else {
            println!("   \x1b[33mWARN\x1b[0m: User B sees original data - isolation via another mechanism");
            println!("         Data: {}\n", data_str);
        }
    } else {
        println!("   WARN: User B couldn't read Apple (status {})\n", get_b_apple.status_code);
    }

    // User A aborts
    println!("16. User A aborts transaction...");
    let abort_resp = client.execute(BtrieveRequest {
        operation_code: OP_ABORT_TRANSACTION,
        position_block: pos_block_a.clone(),
        client_id: USER_A,
        ..Default::default()
    })?;

    if abort_resp.status_code != 0 {
        println!("   FAIL: Abort failed with status {}", abort_resp.status_code);
        return Ok(());
    }
    println!("   OK: Transaction aborted\n");

    // Verify Apple has original data
    println!("17. Verify 'Apple' has original data after rollback...");
    let verify_apple = client.execute(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block_a.clone(),
        key_buffer: key_buf.clone(),
        key_number: 0,
        client_id: USER_A,
        ..Default::default()
    })?;

    if verify_apple.status_code == 0 {
        let data = String::from_utf8_lossy(&verify_apple.data_buffer[20..]);
        let data_str = data.trim_end_matches('\0');
        if data_str.contains("MODIFIED") {
            println!("   \x1b[31mFAIL\x1b[0m: Apple still has modified data after rollback!");
        } else {
            println!("   \x1b[32mPASS\x1b[0m: Apple correctly rolled back to original");
            println!("         Data: {}\n", data_str);
        }
    }

    // Cleanup
    println!("18. Cleaning up...");
    let _ = client.execute(BtrieveRequest {
        operation_code: OP_CLOSE,
        position_block: pos_block_a,
        client_id: USER_A,
        ..Default::default()
    });
    let _ = client_b.execute(BtrieveRequest {
        operation_code: OP_CLOSE,
        position_block: pos_block_b,
        client_id: USER_B,
        ..Default::default()
    });
    println!("   OK: Files closed\n");

    println!("========================================");
    println!("         ISOLATION TEST COMPLETE");
    println!("========================================\n");

    Ok(())
}
