//! Self-contained Xtrieve demo.
//!
//! Starts the daemon on an ephemeral localhost port, creates a genuine
//! Btrieve/N 5.10a `.BTR` file, inserts records, reads one back by key, and
//! exits. Temporary data is removed when the process ends.
//!
//! Run with:
//!     cargo run -p xtrieved --example demo

use std::error::Error;
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use xtrieve_client::btrieve::{create_file, BtrieveFile, KeyDefinition};
use xtrieve_client::XtrieveClient;
use xtrieve_engine::Engine;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let data_dir = dir.path().to_path_buf();

    let engine = Arc::new(Engine::new(data_dir.clone()));
    thread::spawn(move || xtrieved::server::serve(listener, engine, data_dir));

    println!("Xtrieve demo server on {address}");
    println!("Data directory: {}", dir.path().display());

    create_file(
        XtrieveClient::connect(&address.to_string())?,
        "demo.btr",
        32,
        512,
        vec![KeyDefinition::string(1, 4, false, false)],
    )?;

    let mut file = BtrieveFile::open(
        XtrieveClient::connect(&address.to_string())?,
        "demo.btr",
        -1,
    )?;

    for id in 1..=3u32 {
        let mut record = vec![b' '; 32];
        record[..4].copy_from_slice(&id.to_le_bytes());
        let name = format!("Record {id}");
        let bytes = name.as_bytes();
        record[4..4 + bytes.len()].copy_from_slice(bytes);
        file.insert(&record)?;
    }

    let found = file.get_equal(&1u32.to_le_bytes())?;
    println!(
        "Read record 1: {}",
        String::from_utf8_lossy(&found.data[4..]).trim_end()
    );

    file.close()?;
    println!(
        "Created {} and read 3 records.",
        dir.path().join("demo.btr").display()
    );
    Ok(())
}
