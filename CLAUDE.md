# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Xtrieve is a Btrieve 5.1 compatible ISAM database engine for macOS/Linux. It implements the binary file format and operation codes of Novell/Pervasive Btrieve 5.1, allowing legacy applications to access Btrieve data files.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                 xtrieve-client (Rust/C FFI)             │
│    Translates Btrieve API calls → gRPC requests         │
└────────────────────────┬────────────────────────────────┘
                         │ gRPC (TCP port 7419)
┌────────────────────────▼────────────────────────────────┐
│                    xtrieved (daemon)                    │
├─────────────────────────────────────────────────────────┤
│  gRPC Server Layer (tonic)                              │
├─────────────────────────────────────────────────────────┤
│  Operation Dispatcher                                   │
│  - Maps Btrieve opcodes (0-50+) to handlers             │
├─────────────────────────────────────────────────────────┤
│  xtrieve-engine                                         │
│  - File Manager (open files, page cache, locking)       │
│  - Storage Engine (B+ tree, pages, records)             │
└─────────────────────────────────────────────────────────┘
```

## Crate Structure

- **xtrieve-engine**: Core storage engine with no I/O dependencies
- **xtrieved**: Daemon binary with gRPC server
- **xtrieve-client**: Client library with Btrieve-compatible API

## Build Commands

```bash
# Build all crates
cargo build --release

# Build specific crate
cargo build -p xtrieved --release

# Run tests
cargo test

# Run daemon
cargo run -p xtrieved -- --listen 127.0.0.1:7419 --data-dir /path/to/data
```

## Key Files

- `proto/xtrieve.proto` - gRPC service definition
- `xtrieve-engine/src/operations/dispatcher.rs` - Operation routing
- `xtrieve-engine/src/storage/` - Btrieve file format implementation
- `xtrieve-engine/src/file_manager/` - Page cache and locking
- `xtrieved/src/main.rs` - Daemon entry point

## Btrieve Operation Codes

File operations: Open(0), Close(1), Create(14), Stat(15)
Record operations: Insert(2), Update(3), Delete(4)
Key retrieval: GetEqual(5), GetNext(6), GetPrevious(7), GetFirst(12), GetLast(13)
Physical access: StepFirst(33), StepNext(24), StepPrevious(35), StepLast(34)
Transactions: Begin(19), End(20), Abort(21)

## File Format Notes

Btrieve 5.1 files use:
- Page sizes: 512, 1024, 2048, 4096 bytes
- Page 0: FCR (File Control Record) with metadata
- B+ tree indexes with data and index pages interleaved
- Little-endian byte order throughout

## Testing

Currently no test Btrieve files exist. The project can create new files with `Create` operation. For testing with real Btrieve 5.1 files, place them in the data directory.
