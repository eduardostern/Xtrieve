use std::path::{Path, PathBuf};

use tempfile::TempDir;
use xtrieve_engine::status_codes;
use xtrieve_engine::{Engine, OperationRequest, OperationResponse};

#[allow(clippy::too_many_arguments)]
fn call(
    engine: &Engine,
    session: u64,
    operation: u16,
    position: &mut [u8; 128],
    data: Vec<u8>,
    key: Vec<u8>,
    key_number: i16,
    path: Option<PathBuf>,
) -> OperationResponse {
    let response = engine.execute(
        session,
        OperationRequest {
            operation,
            lock_bias: 0,
            position_block: position.to_vec(),
            data_buffer: data,
            key_buffer: key,
            key_number,
            path,
        },
    );
    position.copy_from_slice(&response.position_block);
    response
}

fn create_spec(record_length: u16) -> Vec<u8> {
    let mut spec = vec![0u8; 16];
    spec[0..2].copy_from_slice(&record_length.to_le_bytes());
    spec[2..4].copy_from_slice(&512u16.to_le_bytes());
    spec[4..6].copy_from_slice(&1u16.to_le_bytes());
    let mut key = [0u8; 16];
    key[0..2].copy_from_slice(&1u16.to_le_bytes());
    key[2..4].copy_from_slice(&4u16.to_le_bytes());
    spec.extend_from_slice(&key);
    spec
}

fn setup(name: &str) -> (TempDir, Engine, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(name);
    let engine = Engine::new(dir.path());
    (dir, engine, path)
}

fn create(engine: &Engine, path: &Path, record_length: u16) {
    let mut position = [0x20u8; 128];
    let response = call(
        engine,
        1,
        0x0E,
        &mut position,
        create_spec(record_length),
        Vec::new(),
        0,
        Some(path.to_path_buf()),
    );
    assert_eq!(response.status, status_codes::SUCCESS);
}

fn open(engine: &Engine, path: &Path, mode: i16) -> ([u8; 128], u16) {
    let mut position = [0x20u8; 128];
    let response = call(
        engine,
        1,
        0x00,
        &mut position,
        Vec::new(),
        Vec::new(),
        mode,
        Some(path.to_path_buf()),
    );
    let mut block = [0u8; 128];
    block.copy_from_slice(&response.position_block);
    (block, response.status)
}

fn record(key: u32) -> Vec<u8> {
    let mut data = vec![b'X'; 32];
    data[..4].copy_from_slice(&key.to_le_bytes());
    data
}

#[test]
fn native_position_block_and_round_trip() {
    let (_dir, engine, path) = setup("round.btr");
    create(&engine, &path, 32);
    let mut position = [0x5Au8; 128];
    let response = call(
        &engine,
        1,
        0x00,
        &mut position,
        Vec::new(),
        Vec::new(),
        -1,
        Some(path.to_path_buf()),
    );
    assert_eq!(response.status, status_codes::SUCCESS);
    assert_eq!(position[0], 0xCB);
    assert_eq!(&position[0x40..0x80], &[0x5Au8; 64]);

    let response = call(
        &engine,
        1,
        0x02,
        &mut position,
        record(1),
        Vec::new(),
        0,
        None,
    );
    assert_eq!(response.status, status_codes::SUCCESS);

    let response = call(
        &engine,
        1,
        0x05,
        &mut position,
        vec![0u8; 32],
        1u32.to_le_bytes().to_vec(),
        0,
        None,
    );
    assert_eq!(response.status, status_codes::SUCCESS);
    assert_eq!(&response.data_buffer[..4], &1u32.to_le_bytes());
    assert_eq!(&position[0x40..0x80], &[0x5Au8; 64]);
}

#[test]
fn genuine_status_codes_and_buffer_lengths() {
    let (_dir, engine, path) = setup("status.btr");
    create(&engine, &path, 32);
    let (mut position, status) = open(&engine, &path, -1);
    assert_eq!(status, status_codes::SUCCESS);
    assert_eq!(
        call(
            &engine,
            1,
            0x02,
            &mut position,
            record(1),
            Vec::new(),
            0,
            None,
        )
        .status,
        status_codes::SUCCESS
    );
    assert_eq!(
        call(
            &engine,
            1,
            0x02,
            &mut position,
            record(1),
            Vec::new(),
            0,
            None,
        )
        .status,
        status_codes::DUPLICATE_KEY
    );
    assert_eq!(
        call(
            &engine,
            1,
            0x05,
            &mut position,
            vec![0u8; 32],
            9u32.to_le_bytes().to_vec(),
            0,
            None,
        )
        .status,
        status_codes::KEY_NOT_FOUND
    );
    assert_eq!(
        call(
            &engine,
            1,
            0x05,
            &mut position,
            vec![0u8; 1],
            1u32.to_le_bytes().to_vec(),
            0,
            None,
        )
        .status,
        status_codes::DATA_BUFFER_LENGTH
    );
    let mut transaction_position = [0x20u8; 128];
    assert_eq!(
        call(
            &engine,
            1,
            0x13,
            &mut transaction_position,
            Vec::new(),
            Vec::new(),
            0,
            None,
        )
        .status,
        status_codes::SUCCESS
    );
    assert_eq!(
        call(
            &engine,
            1,
            0x01,
            &mut position,
            Vec::new(),
            Vec::new(),
            0,
            None,
        )
        .status,
        status_codes::OPERATION_NOT_ALLOWED
    );
    assert_eq!(
        call(
            &engine,
            1,
            0x13,
            &mut transaction_position,
            Vec::new(),
            Vec::new(),
            0,
            None,
        )
        .status,
        status_codes::TRANSACTION_ACTIVE
    );
    assert_eq!(
        call(
            &engine,
            1,
            0x15,
            &mut transaction_position,
            Vec::new(),
            Vec::new(),
            0,
            None,
        )
        .status,
        status_codes::SUCCESS
    );
    assert_eq!(
        call(
            &engine,
            1,
            0x00,
            &mut [0x20u8; 128],
            Vec::new(),
            Vec::new(),
            0,
            Some(path.with_file_name("missing.btr")),
        )
        .status,
        status_codes::FILE_NOT_FOUND
    );
}

#[test]
fn paths_with_spaces_use_the_override() {
    let (_dir, engine, path) = setup("space file.btr");
    create(&engine, &path, 32);
    let (mut position, status) = open(&engine, &path, -1);
    assert_eq!(status, status_codes::SUCCESS);
    assert_eq!(
        call(
            &engine,
            1,
            0x02,
            &mut position,
            record(3),
            Vec::new(),
            0,
            None,
        )
        .status,
        status_codes::SUCCESS
    );
    assert_eq!(
        call(
            &engine,
            1,
            0x01,
            &mut position,
            Vec::new(),
            Vec::new(),
            0,
            None
        )
        .status,
        status_codes::SUCCESS
    );
}

#[test]
fn read_only_open_rejects_writes() {
    let (_dir, engine, path) = setup("readonly.btr");
    create(&engine, &path, 32);
    let (mut position, status) = open(&engine, &path, -2);
    assert_eq!(status, status_codes::SUCCESS);
    assert_eq!(
        call(
            &engine,
            1,
            0x02,
            &mut position,
            record(1),
            Vec::new(),
            0,
            None,
        )
        .status,
        status_codes::OPERATION_NOT_ALLOWED
    );
}
