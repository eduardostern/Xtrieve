use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use btr_format::BtrieveFile as FormatFile;
use tempfile::TempDir;
use xtrieve_client::btrieve::{create_file, BtrieveFile, KeyDefinition};
use xtrieve_client::XtrieveClient;
use xtrieve_engine::status_codes;
use xtrieve_engine::Engine;
use xtrieved::server;

fn start() -> (TempDir, std::net::SocketAddr) {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = Arc::new(Engine::new(dir.path()));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let data_dir = dir.path().to_path_buf();
    thread::spawn(move || server::serve(listener, engine, data_dir));
    (dir, addr)
}

fn client(addr: std::net::SocketAddr) -> XtrieveClient {
    XtrieveClient::connect(&addr.to_string()).expect("connect")
}

fn record(key: u32) -> Vec<u8> {
    let mut record = vec![b'A'; 32];
    record[..4].copy_from_slice(&key.to_le_bytes());
    record
}

#[test]
fn create_insert_get_close_round_trip() {
    let (dir, addr) = start();
    create_file(
        client(addr),
        "test.btr",
        32,
        512,
        vec![KeyDefinition::string(1, 4, false, false)],
    )
    .expect("create");

    let mut file = BtrieveFile::open(client(addr), "test.btr", -1).expect("open");
    file.insert(&record(1)).expect("insert");
    let got = file.get_equal(&1u32.to_le_bytes()).expect("get");
    assert_eq!(got.data, record(1));
    file.close().expect("close");

    let parsed = FormatFile::open(dir.path().join("test.btr")).expect("parse .btr");
    assert_eq!(parsed.records().expect("records").len(), 1);
    assert!(!dir.path().join("test.PRE").exists());
}

#[test]
fn transaction_is_visible_only_to_its_session() {
    let (_dir, addr) = start();
    create_file(
        client(addr),
        "shared.btr",
        32,
        512,
        vec![KeyDefinition::string(1, 4, false, false)],
    )
    .expect("create");

    let mut first = BtrieveFile::open(client(addr), "shared.btr", -1).expect("open 1");
    let mut second = BtrieveFile::open(client(addr), "shared.btr", -1).expect("open 2");
    first.begin_transaction().expect("begin");
    first.insert(&record(7)).expect("insert");
    assert_eq!(
        second
            .get_equal(&7u32.to_le_bytes())
            .unwrap_err()
            .status_code(),
        status_codes::FILE_IN_USE
    );
    first.end_transaction().expect("commit");
    assert_eq!(
        second.get_equal(&7u32.to_le_bytes()).expect("get").data,
        record(7)
    );
    first.close().expect("close 1");
    second.close().expect("close 2");
}

#[test]
fn dos_style_open_mode_is_sign_extended() {
    let (_dir, addr) = start();
    create_file(
        client(addr),
        "dos.btr",
        32,
        512,
        vec![KeyDefinition::string(1, 4, false, false)],
    )
    .expect("create");

    let mut connection = client(addr);
    let response = connection
        .execute(xtrieve_client::BtrieveRequest {
            operation_code: 0x00,
            file_path: "dos.btr".to_string(),
            key_number: 255, // DOS bridge sent -1 as an unsigned byte
            position_block: vec![0x20; 128],
            ..Default::default()
        })
        .expect("open");
    assert_eq!(response.status_code, status_codes::SUCCESS);
    let close = connection
        .execute(xtrieve_client::BtrieveRequest {
            operation_code: 0x01,
            position_block: response.position_block,
            ..Default::default()
        })
        .expect("close");
    assert_eq!(close.status_code, status_codes::SUCCESS);
}

#[test]
fn disconnect_aborts_the_session_transaction() {
    let (_dir, addr) = start();
    create_file(
        client(addr),
        "drop.btr",
        32,
        512,
        vec![KeyDefinition::string(1, 4, false, false)],
    )
    .expect("create");

    {
        let mut file = BtrieveFile::open(client(addr), "drop.btr", -1).expect("open");
        file.begin_transaction().expect("begin");
        file.insert(&record(9)).expect("insert");
        // Dropping the client closes the TCP connection without Close/Abort.
    }
    thread::sleep(Duration::from_millis(200));

    let mut file = BtrieveFile::open(client(addr), "drop.btr", -1).expect("reopen");
    assert_eq!(
        file.get_equal(&9u32.to_le_bytes())
            .unwrap_err()
            .status_code(),
        status_codes::KEY_NOT_FOUND
    );
    file.close().expect("close");
}
