//! Comprehensive test for all Btrieve operations
//!
//! Tests all implemented Btrieve operation codes:
//! - File ops: Open(0), Close(1), Create(14), Stat(15)
//! - Record ops: Insert(2), Update(3), Delete(4)
//! - Key retrieval: GetEqual(5), GetNext(6), GetPrev(7), GetGreater(8),
//!                  GetGE(9), GetLess(10), GetLE(11), GetFirst(12), GetLast(13)
//! - Position: GetPosition(22), GetDirect(23)
//! - Step ops: StepNext(24), StepFirst(33), StepLast(34), StepPrev(35)
//! - Transactions: Begin(19), End(20), Abort(21)
//! - Other: Version(26), Reset(28)

use xtrieve_client::proto::xtrieve_client::XtrieveClient;
use xtrieve_client::proto::BtrieveRequest;

const TEST_FILE: &str = "/Users/eduardo/xtrieve/data/test_all_ops.dat";

// Operation codes
const OP_OPEN: u32 = 0;
const OP_CLOSE: u32 = 1;
const OP_INSERT: u32 = 2;
const OP_UPDATE: u32 = 3;
const OP_DELETE: u32 = 4;
const OP_GET_EQUAL: u32 = 5;
const OP_GET_NEXT: u32 = 6;
const OP_GET_PREV: u32 = 7;
const OP_GET_GREATER: u32 = 8;
const OP_GET_GE: u32 = 9;
const OP_GET_LESS: u32 = 10;
const OP_GET_LE: u32 = 11;
const OP_GET_FIRST: u32 = 12;
const OP_GET_LAST: u32 = 13;
const OP_CREATE: u32 = 14;
const OP_STAT: u32 = 15;
const OP_BEGIN_TRANS: u32 = 19;
const OP_END_TRANS: u32 = 20;
const OP_ABORT_TRANS: u32 = 21;
const OP_GET_POSITION: u32 = 22;
const OP_GET_DIRECT: u32 = 23;
const OP_STEP_NEXT: u32 = 24;
const OP_VERSION: u32 = 26;
const OP_RESET: u32 = 28;
const OP_STEP_FIRST: u32 = 33;
const OP_STEP_LAST: u32 = 34;
const OP_STEP_PREV: u32 = 35;

struct TestResult {
    name: String,
    passed: bool,
    message: String,
}

impl TestResult {
    fn pass(name: &str) -> Self {
        TestResult {
            name: name.to_string(),
            passed: true,
            message: "OK".to_string(),
        }
    }

    fn fail(name: &str, msg: &str) -> Self {
        TestResult {
            name: name.to_string(),
            passed: false,
            message: msg.to_string(),
        }
    }
}

fn build_create_buffer() -> Vec<u8> {
    let mut buf = Vec::new();
    // File spec: record_len(2), page_size(2), num_keys(2), unused(4), file_flags(2)
    buf.extend_from_slice(&100u16.to_le_bytes());  // record length
    buf.extend_from_slice(&4096u16.to_le_bytes()); // page size
    buf.extend_from_slice(&1u16.to_le_bytes());    // number of keys
    buf.extend_from_slice(&0u32.to_le_bytes());    // unused
    // Key spec: position(2), length(2), flags(2), reserved(4), ext_type(1), null_val(1), reserved(2)
    buf.extend_from_slice(&0u16.to_le_bytes());    // key position
    buf.extend_from_slice(&20u16.to_le_bytes());   // key length
    buf.extend_from_slice(&0u16.to_le_bytes());    // flags (duplicates allowed, modifiable)
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

fn extract_key(record: &[u8]) -> String {
    let end = record[..20].iter().position(|&b| b == 0).unwrap_or(20);
    String::from_utf8_lossy(&record[..end]).to_string()
}

fn extract_data(record: &[u8]) -> String {
    if record.len() < 21 {
        return String::new();
    }
    let end = record[20..].iter().position(|&b| b == 0).unwrap_or(record.len() - 20);
    String::from_utf8_lossy(&record[20..20 + end]).to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Xtrieve Comprehensive Operation Test ===\n");

    let mut client = XtrieveClient::connect("http://127.0.0.1:7419").await?;
    let mut results: Vec<TestResult> = Vec::new();

    // Clean up any existing test file
    let _ = std::fs::remove_file(TEST_FILE);

    // ========================================
    // Test Version (26)
    // ========================================
    println!("Testing Version (26)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_VERSION,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("Version"));
    } else {
        results.push(TestResult::fail("Version", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test Create (14)
    // ========================================
    println!("Testing Create (14)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_CREATE,
        file_path: TEST_FILE.to_string(),
        data_buffer: build_create_buffer(),
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("Create"));
    } else {
        results.push(TestResult::fail("Create", &format!("status {}", resp.status_code)));
        println!("FATAL: Cannot continue without file creation");
        return Ok(());
    }

    // ========================================
    // Test Open (0)
    // ========================================
    println!("Testing Open (0)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_OPEN,
        file_path: TEST_FILE.to_string(),
        ..Default::default()
    })).await?.into_inner();
    let mut pos_block = resp.position_block.clone();
    if resp.status_code == 0 && pos_block.len() >= 128 {
        results.push(TestResult::pass("Open"));
    } else {
        results.push(TestResult::fail("Open", &format!("status {}", resp.status_code)));
        return Ok(());
    }

    // ========================================
    // Test Stat (15)
    // ========================================
    println!("Testing Stat (15)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_STAT,
        position_block: pos_block.clone(),
        data_buffer_length: 256,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("Stat"));
    } else {
        results.push(TestResult::fail("Stat", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test Insert (2) - Insert 10 records
    // ========================================
    println!("Testing Insert (2)...");
    let test_data = [
        ("Apple", "Red fruit"),
        ("Banana", "Yellow fruit"),
        ("Cherry", "Small red fruit"),
        ("Date", "Sweet fruit"),
        ("Elderberry", "Purple berry"),
        ("Fig", "Mediterranean fruit"),
        ("Grape", "Wine fruit"),
        ("Honeydew", "Green melon"),
        ("Imbe", "African fruit"),
        ("Jackfruit", "Large tropical fruit"),
    ];

    let mut insert_ok = true;
    for (key, data) in &test_data {
        let record = make_record(key, data);
        let resp = client.execute(tonic::Request::new(BtrieveRequest {
            operation_code: OP_INSERT,
            position_block: pos_block.clone(),
            data_buffer: record,
            data_buffer_length: 100,
            ..Default::default()
        })).await?.into_inner();
        if resp.status_code != 0 {
            insert_ok = false;
            results.push(TestResult::fail("Insert", &format!("failed on '{}': status {}", key, resp.status_code)));
            break;
        }
        pos_block = resp.position_block;
    }
    if insert_ok {
        results.push(TestResult::pass("Insert (10 records)"));
    }

    // ========================================
    // Test GetFirst (12)
    // ========================================
    println!("Testing GetFirst (12)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_FIRST,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        if key == "Apple" {
            results.push(TestResult::pass("GetFirst"));
        } else {
            results.push(TestResult::fail("GetFirst", &format!("expected 'Apple', got '{}'", key)));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("GetFirst", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetNext (6)
    // ========================================
    println!("Testing GetNext (6)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_NEXT,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        if key == "Banana" {
            results.push(TestResult::pass("GetNext"));
        } else {
            results.push(TestResult::fail("GetNext", &format!("expected 'Banana', got '{}'", key)));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("GetNext", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetLast (13)
    // ========================================
    println!("Testing GetLast (13)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_LAST,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        if key == "Jackfruit" {
            results.push(TestResult::pass("GetLast"));
        } else {
            results.push(TestResult::fail("GetLast", &format!("expected 'Jackfruit', got '{}'", key)));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("GetLast", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetPrevious (7)
    // ========================================
    println!("Testing GetPrevious (7)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_PREV,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        if key == "Imbe" {
            results.push(TestResult::pass("GetPrevious"));
        } else {
            results.push(TestResult::fail("GetPrevious", &format!("expected 'Imbe', got '{}'", key)));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("GetPrevious", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetEqual (5)
    // ========================================
    println!("Testing GetEqual (5)...");
    let mut key_buf = vec![0u8; 20];
    key_buf[..5].copy_from_slice(b"Grape");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_buffer: key_buf,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        let data = extract_data(&resp.data_buffer);
        if key == "Grape" && data.contains("Wine") {
            results.push(TestResult::pass("GetEqual"));
        } else {
            results.push(TestResult::fail("GetEqual", &format!("got key='{}' data='{}'", key, data)));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("GetEqual", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetGreater (8)
    // ========================================
    println!("Testing GetGreater (8)...");
    let mut key_buf = vec![0u8; 20];
    key_buf[..5].copy_from_slice(b"Grape");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_GREATER,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_buffer: key_buf,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        if key == "Honeydew" {
            results.push(TestResult::pass("GetGreater"));
        } else {
            results.push(TestResult::fail("GetGreater", &format!("expected 'Honeydew', got '{}'", key)));
        }
        pos_block = resp.position_block;
    } else {
        // May not be implemented yet
        results.push(TestResult::fail("GetGreater", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetGreaterOrEqual (9)
    // ========================================
    println!("Testing GetGreaterOrEqual (9)...");
    let mut key_buf = vec![0u8; 20];
    key_buf[..6].copy_from_slice(b"Cherry");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_GE,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_buffer: key_buf,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        if key == "Cherry" {
            results.push(TestResult::pass("GetGreaterOrEqual"));
        } else {
            results.push(TestResult::fail("GetGreaterOrEqual", &format!("expected 'Cherry', got '{}'", key)));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("GetGreaterOrEqual", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetLess (10)
    // ========================================
    println!("Testing GetLess (10)...");
    let mut key_buf = vec![0u8; 20];
    key_buf[..6].copy_from_slice(b"Cherry");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_LESS,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_buffer: key_buf,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        if key == "Banana" {
            results.push(TestResult::pass("GetLess"));
        } else {
            results.push(TestResult::fail("GetLess", &format!("expected 'Banana', got '{}'", key)));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("GetLess", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetLessOrEqual (11)
    // ========================================
    println!("Testing GetLessOrEqual (11)...");
    let mut key_buf = vec![0u8; 20];
    key_buf[..6].copy_from_slice(b"Cherry");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_LE,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_buffer: key_buf,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        let key = extract_key(&resp.data_buffer);
        if key == "Cherry" {
            results.push(TestResult::pass("GetLessOrEqual"));
        } else {
            results.push(TestResult::fail("GetLessOrEqual", &format!("expected 'Cherry', got '{}'", key)));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("GetLessOrEqual", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetPosition (22)
    // ========================================
    println!("Testing GetPosition (22)...");
    // First position on a record
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_FIRST,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    pos_block = resp.position_block;

    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_POSITION,
        position_block: pos_block.clone(),
        data_buffer_length: 8,
        ..Default::default()
    })).await?.into_inner();
    let saved_position: Vec<u8>;
    if resp.status_code == 0 && resp.data_buffer.len() >= 4 {
        saved_position = resp.data_buffer.clone();
        results.push(TestResult::pass("GetPosition"));
    } else {
        saved_position = Vec::new();
        results.push(TestResult::fail("GetPosition", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test GetDirect (23)
    // ========================================
    println!("Testing GetDirect (23)...");
    if !saved_position.is_empty() {
        let resp = client.execute(tonic::Request::new(BtrieveRequest {
            operation_code: OP_GET_DIRECT,
            position_block: pos_block.clone(),
            data_buffer: saved_position,
            data_buffer_length: 100,
            key_number: 0,
            ..Default::default()
        })).await?.into_inner();
        if resp.status_code == 0 {
            let key = extract_key(&resp.data_buffer);
            if key == "Apple" {
                results.push(TestResult::pass("GetDirect"));
            } else {
                results.push(TestResult::fail("GetDirect", &format!("expected 'Apple', got '{}'", key)));
            }
            pos_block = resp.position_block;
        } else {
            results.push(TestResult::fail("GetDirect", &format!("status {}", resp.status_code)));
        }
    } else {
        results.push(TestResult::fail("GetDirect", "skipped - no position"));
    }

    // ========================================
    // Test StepFirst (33)
    // ========================================
    println!("Testing StepFirst (33)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_STEP_FIRST,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("StepFirst"));
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("StepFirst", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test StepNext (24)
    // ========================================
    println!("Testing StepNext (24)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_STEP_NEXT,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("StepNext"));
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("StepNext", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test StepLast (34)
    // ========================================
    println!("Testing StepLast (34)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_STEP_LAST,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("StepLast"));
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("StepLast", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test StepPrevious (35)
    // ========================================
    println!("Testing StepPrevious (35)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_STEP_PREV,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("StepPrevious"));
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("StepPrevious", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test Update (3)
    // ========================================
    println!("Testing Update (3)...");
    // First get a record to update
    let mut key_buf = vec![0u8; 20];
    key_buf[..5].copy_from_slice(b"Apple");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_buffer: key_buf,
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    pos_block = resp.position_block;

    // Now update it
    let updated_record = make_record("Apple", "UPDATED: Green or Red fruit");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_UPDATE,
        position_block: pos_block.clone(),
        data_buffer: updated_record,
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        // Verify the update
        let mut key_buf = vec![0u8; 20];
        key_buf[..5].copy_from_slice(b"Apple");
        let verify = client.execute(tonic::Request::new(BtrieveRequest {
            operation_code: OP_GET_EQUAL,
            position_block: resp.position_block.clone(),
            data_buffer_length: 100,
            key_buffer: key_buf,
            key_number: 0,
            ..Default::default()
        })).await?.into_inner();
        let data = extract_data(&verify.data_buffer);
        if data.contains("UPDATED") {
            results.push(TestResult::pass("Update"));
        } else {
            results.push(TestResult::fail("Update", "data not updated"));
        }
        pos_block = verify.position_block;
    } else {
        results.push(TestResult::fail("Update", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test Begin Transaction (19)
    // ========================================
    println!("Testing BeginTransaction (19)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_BEGIN_TRANS,
        position_block: pos_block.clone(),
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("BeginTransaction"));
    } else {
        results.push(TestResult::fail("BeginTransaction", &format!("status {}", resp.status_code)));
    }

    // Insert a record within transaction
    let trans_record = make_record("Zucchini", "Transaction test vegetable");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_INSERT,
        position_block: pos_block.clone(),
        data_buffer: trans_record,
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    pos_block = resp.position_block;

    // ========================================
    // Test Abort Transaction (21)
    // ========================================
    println!("Testing AbortTransaction (21)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_ABORT_TRANS,
        position_block: pos_block.clone(),
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        // Verify the record was rolled back
        let mut key_buf = vec![0u8; 20];
        key_buf[..8].copy_from_slice(b"Zucchini");
        let verify = client.execute(tonic::Request::new(BtrieveRequest {
            operation_code: OP_GET_EQUAL,
            position_block: pos_block.clone(),
            data_buffer_length: 100,
            key_buffer: key_buf,
            key_number: 0,
            ..Default::default()
        })).await?.into_inner();
        if verify.status_code == 4 { // KeyNotFound
            results.push(TestResult::pass("AbortTransaction (rollback verified)"));
        } else if verify.status_code == 0 {
            results.push(TestResult::fail("AbortTransaction", "record not rolled back"));
        } else {
            results.push(TestResult::pass("AbortTransaction"));
        }
    } else {
        results.push(TestResult::fail("AbortTransaction", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test End Transaction (20)
    // ========================================
    println!("Testing EndTransaction (20)...");
    // Start a new transaction
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_BEGIN_TRANS,
        position_block: pos_block.clone(),
        ..Default::default()
    })).await?.into_inner();

    // Insert a record
    let trans_record = make_record("Yam", "Committed vegetable");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_INSERT,
        position_block: pos_block.clone(),
        data_buffer: trans_record,
        data_buffer_length: 100,
        ..Default::default()
    })).await?.into_inner();
    pos_block = resp.position_block;

    // Commit the transaction
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_END_TRANS,
        position_block: pos_block.clone(),
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        // Verify the record exists
        let mut key_buf = vec![0u8; 20];
        key_buf[..3].copy_from_slice(b"Yam");
        let verify = client.execute(tonic::Request::new(BtrieveRequest {
            operation_code: OP_GET_EQUAL,
            position_block: pos_block.clone(),
            data_buffer_length: 100,
            key_buffer: key_buf,
            key_number: 0,
            ..Default::default()
        })).await?.into_inner();
        if verify.status_code == 0 {
            results.push(TestResult::pass("EndTransaction (commit verified)"));
            pos_block = verify.position_block;
        } else {
            results.push(TestResult::fail("EndTransaction", "record not found after commit"));
        }
    } else {
        results.push(TestResult::fail("EndTransaction", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test Delete (4)
    // ========================================
    println!("Testing Delete (4)...");
    // Position on a record to delete
    let mut key_buf = vec![0u8; 20];
    key_buf[..3].copy_from_slice(b"Yam");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_GET_EQUAL,
        position_block: pos_block.clone(),
        data_buffer_length: 100,
        key_buffer: key_buf.clone(),
        key_number: 0,
        ..Default::default()
    })).await?.into_inner();
    pos_block = resp.position_block;

    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_DELETE,
        position_block: pos_block.clone(),
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        // Verify deletion
        let verify = client.execute(tonic::Request::new(BtrieveRequest {
            operation_code: OP_GET_EQUAL,
            position_block: resp.position_block.clone(),
            data_buffer_length: 100,
            key_buffer: key_buf,
            key_number: 0,
            ..Default::default()
        })).await?.into_inner();
        if verify.status_code == 4 { // KeyNotFound
            results.push(TestResult::pass("Delete (verified)"));
        } else {
            results.push(TestResult::fail("Delete", "record still found"));
        }
        pos_block = resp.position_block;
    } else {
        results.push(TestResult::fail("Delete", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Test Reset (28)
    // ========================================
    println!("Testing Reset (28)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_RESET,
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("Reset"));
    } else {
        results.push(TestResult::fail("Reset", &format!("status {}", resp.status_code)));
    }

    // Re-open for Close test
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_OPEN,
        file_path: TEST_FILE.to_string(),
        ..Default::default()
    })).await?.into_inner();
    pos_block = resp.position_block;

    // ========================================
    // Test Close (1)
    // ========================================
    println!("Testing Close (1)...");
    let resp = client.execute(tonic::Request::new(BtrieveRequest {
        operation_code: OP_CLOSE,
        position_block: pos_block.clone(),
        ..Default::default()
    })).await?.into_inner();
    if resp.status_code == 0 {
        results.push(TestResult::pass("Close"));
    } else {
        results.push(TestResult::fail("Close", &format!("status {}", resp.status_code)));
    }

    // ========================================
    // Print Results Summary
    // ========================================
    println!("\n========================================");
    println!("         TEST RESULTS SUMMARY");
    println!("========================================\n");

    let mut passed = 0;
    let mut failed = 0;

    for result in &results {
        let status = if result.passed {
            passed += 1;
            "\x1b[32mPASS\x1b[0m"
        } else {
            failed += 1;
            "\x1b[31mFAIL\x1b[0m"
        };
        println!("{:.<30} {} {}", result.name, status,
            if result.passed { "" } else { &result.message });
    }

    println!("\n----------------------------------------");
    println!("Total: {} tests, \x1b[32m{} passed\x1b[0m, \x1b[31m{} failed\x1b[0m",
        results.len(), passed, failed);
    println!("========================================\n");

    // Clean up
    let _ = std::fs::remove_file(TEST_FILE);

    Ok(())
}
