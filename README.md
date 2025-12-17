# Xtrieve

A Btrieve 5.1 compatible ISAM database engine for modern systems.

Xtrieve implements the binary file format and operation codes of Novell/Pervasive Btrieve 5.1, allowing legacy applications to access Btrieve data files on macOS and Linux.

## Features

- **Btrieve 5.1 Compatible** - Read/write original Btrieve 5.1 file format
- **Full ISAM Operations** - Insert, Update, Delete, GetEqual, GetNext, GetPrevious, GetFirst, GetLast
- **B+ Tree Indexes** - Multiple keys per file with duplicates support
- **Transaction Support** - Begin, End, Abort with ACID isolation
- **Record Locking** - Single record and multi-record locks
- **Lightweight Binary Protocol** - Sub-megabyte server binary (656 KB)
- **Sync & Async Clients** - Both blocking and tokio-based async clients

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│              xtrieve-client (Rust Library)              │
│         Sync (XtrieveClient) / Async (AsyncXtrieveClient)
└────────────────────────┬────────────────────────────────┘
                         │ Binary Protocol (TCP port 7419)
┌────────────────────────▼────────────────────────────────┐
│                    xtrieved (daemon)                    │
├─────────────────────────────────────────────────────────┤
│  Raw TCP Server                                         │
├─────────────────────────────────────────────────────────┤
│  xtrieve-engine                                         │
│  - Operation Dispatcher (Btrieve opcodes 0-50+)         │
│  - File Manager (page cache, record locking)            │
│  - Storage Engine (B+ tree, FCR, pages, records)        │
└─────────────────────────────────────────────────────────┘
```

## Quick Start

### Build

```bash
# Build all crates (optimized release build)
cargo build --release

# Binary size: ~656 KB (vs 3.6 MB with gRPC)
ls -lh target/release/xtrieved
```

### Run Server

```bash
# Start the daemon
./target/release/xtrieved --data-dir ./data --listen 127.0.0.1:7419
```

### Client Usage

**Sync Client:**
```rust
use xtrieve_client::{XtrieveClient, BtrieveRequest};

let mut client = XtrieveClient::connect("127.0.0.1:7419")?;

// Open a file
let resp = client.execute(BtrieveRequest {
    operation_code: 0,  // OP_OPEN
    file_path: "mydata.dat".to_string(),
    ..Default::default()
})?;

let pos_block = resp.position_block;

// Insert a record
let resp = client.execute(BtrieveRequest {
    operation_code: 2,  // OP_INSERT
    position_block: pos_block.clone(),
    data_buffer: my_record_bytes,
    ..Default::default()
})?;
```

**Async Client:**
```rust
use xtrieve_client::{AsyncXtrieveClient, BtrieveRequest};

let mut client = AsyncXtrieveClient::connect("127.0.0.1:7419").await?;

let resp = client.execute(BtrieveRequest {
    operation_code: 0,
    file_path: "mydata.dat".to_string(),
    ..Default::default()
}).await?;
```

## Btrieve Operation Codes

| Code | Operation     | Description                          |
|------|---------------|--------------------------------------|
| 0    | Open          | Open a file                          |
| 1    | Close         | Close a file                         |
| 2    | Insert        | Insert a new record                  |
| 3    | Update        | Update existing record               |
| 4    | Delete        | Delete current record                |
| 5    | GetEqual      | Get record by exact key match        |
| 6    | GetNext       | Get next record in key order         |
| 7    | GetPrevious   | Get previous record in key order     |
| 12   | GetFirst      | Get first record in key order        |
| 13   | GetLast       | Get last record in key order         |
| 14   | Create        | Create a new file                    |
| 15   | Stat          | Get file statistics                  |
| 19   | BeginTrans    | Begin transaction                    |
| 20   | EndTrans      | Commit transaction                   |
| 21   | AbortTrans    | Rollback transaction                 |
| 24   | StepNext      | Step to next physical record         |
| 33   | StepFirst     | Step to first physical record        |
| 34   | StepLast      | Step to last physical record         |
| 35   | StepPrevious  | Step to previous physical record     |

## Wire Protocol

Xtrieve uses a compact binary protocol (little-endian):

**Request Format:**
```
[op_code:2][pos_block:128][data_len:4][data:N][key_len:2][key:N][key_num:2][path_len:2][path:N][lock:2]
```

**Response Format:**
```
[status:2][pos_block:128][data_len:4][data:N][key_len:2][key:N]
```

## Examples

### Weather Telemetry Demo

Fetches real weather data from Open-Meteo API and stores in Xtrieve:

```bash
# Start the server
./target/release/xtrieved --data-dir ./data &

# Run telemetry collector (creates weather.dat, fetches data)
cargo run --example weather_telemetry --features examples

# Start web dashboard
cargo run --example weather_web --features examples
# Open http://localhost:3000
```

### Transaction Isolation Test

```bash
cargo run --example test_isolation
```

## File Format

Btrieve 5.1 files use:
- Page sizes: 512, 1024, 2048, 4096 bytes
- Page 0: FCR (File Control Record) with metadata
- B+ tree indexes with interleaved data/index pages
- Little-endian byte order throughout

## Crate Structure

- **xtrieve-engine** - Core storage engine (no I/O dependencies)
- **xtrieved** - Server daemon with TCP listener
- **xtrieve-client** - Client library (sync + async)

## Building for Size

The release profile is optimized for binary size:

```toml
[profile.release]
lto = true
codegen-units = 1
opt-level = "z"
strip = true
panic = "abort"
```

Result: 656 KB (down from 3.6 MB with gRPC)

## License

MIT

## Acknowledgments

Btrieve is a trademark of Actian Corporation. This project is an independent implementation for compatibility purposes.
