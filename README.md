# Xtrieve

A Btrieve/N 5.10a compatible ISAM database engine for modern systems.

Xtrieve now uses the clean-room byte-exact Btrieve/N 5.10a engine in
`third_party/btr-engine`. Files are genuine single-file `.BTR` databases with
native FCR, multi-level B-trees, variable/compressed records, owner
encryption, native 128-byte position blocks, and real `.PRE`/`.TRN`
transaction artifacts.

**Breaking change:** Xtrieve's old custom `.DAT`/`.IX#` storage is no longer
readable. There is no migration tool; create new `.BTR` files with the Create
operation. The old files are left untouched.

## Features

- **Btrieve/N 5.10a engine** - genuine file format, opcodes, statuses, and
  position blocks
- **Full ISAM operations** - Insert, Update, Delete, keyed Gets, Step,
  Get Position/Direct, Stat, Extend, owner operations, supplemental indexes,
  and extended get/step/insert
- **Real transactions** - per-session Begin/End/Abort, `.PRE` images,
  `BTRIEVE.TRN`, and crash recovery on Open
- **Record locking** - in-process single/multiple record locks (wait biases
  do not block; they return status 84)
- **Multi-session server** - one session per TCP connection; disconnect
  aborts the session's transaction and closes its handles
- **Sync & async clients** - blocking and tokio-based client APIs
- **DOS bridge** - original DOS Btrieve applications via the serial bridge

## DOS Bridge

Run **original, unmodified DOS applications** from the 1990s against Xtrieve. No recompilation required.

```
DOS Application (Turbo Pascal, Clipper, C...)
         │
         │ INT 7Bh
         ▼
    BTRSERL.EXE (TSR) ──── COM1 @ 115200 ────┐
                                              │
                   ┌──────────────────────────┘
                   ▼
            serial-bridge (Rust) ──── TCP:7419 ──── xtrieved
```

The bridge consists of:
- **BTRSERL.EXE** - DOS TSR (~7KB) that hooks INT 7Bh and redirects to serial
- **serial-bridge** - Rust program that translates serial to TCP protocol
- **xtrieved** - The Xtrieve server

See [docs/bridge/](docs/bridge/) for complete documentation.

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
│  - Session-aware request adapter                        │
│  - Mutex<btr-engine::Engine>                            │
│  - third_party/btr-format + btr-engine                  │
└─────────────────────────────────────────────────────────┘
```

## Quick Start

### Build

```bash
# Build all crates (optimized release build)
cargo build --release
```

### Run Server

```bash
# Start the daemon
./target/release/xtrieved --data-dir ./data --listen 127.0.0.1:7419
```

### Client Usage

```rust
use xtrieve_client::btrieve::{create_file, BtrieveFile, KeyDefinition};
use xtrieve_client::XtrieveClient;

create_file(
    XtrieveClient::connect("127.0.0.1:7419")?,
    "mydata.btr",
    64,   // record length
    1024, // page size
    vec![KeyDefinition::string(1, 4, false, true)],
)?;

let mut file = BtrieveFile::open(
    XtrieveClient::connect("127.0.0.1:7419")?,
    "mydata.btr",
    -1, // normal mode
)?;
file.insert(&my_record_bytes)?; // 64 bytes
let record = file.get_equal(&key_bytes)?;
file.close()?;
```

Each TCP connection is a session. Open a separate connection for each
independent transaction or cursor namespace.

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
| 8    | GetGreater    | First key greater than value         |
| 9    | GetGE         | First key greater than or equal      |
| 10   | GetLess       | First key less than value            |
| 11   | GetLE         | First key less than or equal         |
| 12   | GetFirst      | Get first record in key order        |
| 13   | GetLast       | Get last record in key order         |
| 14   | Create        | Create a new file                    |
| 15   | Stat          | Get file statistics                  |
| 16   | Extend        | Record extension file                |
| 17   | SetDirectory  | Set this session's directory         |
| 18   | GetDirectory  | Get this session's directory         |
| 19   | BeginTrans    | Begin transaction                    |
| 20   | EndTrans      | Commit transaction                   |
| 21   | AbortTrans    | Rollback transaction                 |
| 22   | GetPosition   | Save native cursor position          |
| 23   | GetDirect     | Get record by address                |
| 24   | StepNext      | Step to next physical record         |
| 25   | Stop          | Close this session's resources       |
| 26   | Version       | Return 5.10a version                 |
| 27   | Unlock        | Release a record lock                |
| 28   | Reset         | Close files and abort this session   |
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

## File Format

Genuine Btrieve/N 5.10a `.BTR` files use:
- Page sizes: 512, 1024, 2048, or 4096 bytes
- Page 0: FCR (File Control Record)
- Interleaved data, index, variable, and PAT pages
- Multi-level B-trees, duplicate chains, optional 265-byte ACS
- Little-endian byte order throughout

Older Xtrieve `.DAT`/`.IX#` files are **not** readable by this backend and are
not migrated.

## Crate Structure

- **xtrieve-engine** - Session-aware adapter around `third_party/btr-engine`
- **xtrieved** - TCP daemon with one session per connection
- **xtrieve-client** - Client library (sync + async)
- **serial-bridge** - DOS serial-to-TCP bridge

## Performance

The byte-exact engine is optimized for compatibility, not concurrency. It
reads, parses, and writes the whole file per operation, and the server
serializes operations through one mutex. This is acceptable for DOS-era ISAM
files; large-file throughput and caching are a separate follow-up.

## Documentation

- [DOS Bridge Guide](docs/bridge/) - Run legacy DOS apps with Xtrieve
- [Protocol Specification](docs/bridge/PROTOCOL.md) - Wire protocol details
- [Technical Reference](docs/bridge/TECHNICAL.md) - TSR internals

## The Story

This project bridges 30+ years of database history - from BBS systems running Btrieve in 1991 to modern Rust servers in 2025.

- [docs/STORY.md](docs/STORY.md) - The journey from BBS era to 2025
- [docs/FRIENDSHIP.md](docs/FRIENDSHIP.md) - On partnership, Kirk & Spock, and where "no one" has gone before

---

```
╔════════════════════════════════════════════════════════════════════════════╗
║                                                                            ║
║                        X T R I E V E   v 0 . 1 . 0                        ║
║                                                                            ║
║                   ░▒▓█ S H A R E W A R E   E D I T I O N █▓▒░              ║
║                                                                            ║
╠════════════════════════════════════════════════════════════════════════════╣
║                                                                            ║
║   This software is distributed under the MIT License.                      ║
║   It is FREE for personal and commercial use.                              ║
║                                                                            ║
║   If you find this software useful, please consider:                       ║
║                                                                            ║
║     ★  Starring the repository on GitHub                                   ║
║     ★  Contributing code, documentation, or bug reports                    ║
║     ★  Telling others about this project                                   ║
║     ★  Sponsoring the development                                          ║
║                                                                            ║
║   Registration is not required, but your support is appreciated!           ║
║                                                                            ║
║   "If you like it, share it. If you love it, contribute."                  ║
║                                                                            ║
║                                             - The spirit of Shareware      ║
║                                                                            ║
╚════════════════════════════════════════════════════════════════════════════╝
```

## License

MIT

## Acknowledgments

Btrieve is a trademark of Actian Corporation. This project is an independent implementation for compatibility purposes.
