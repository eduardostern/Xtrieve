# CLAUDE.md

## Project Overview

Xtrieve is a Btrieve/N 5.10a compatible ISAM service. The runtime storage
engine is the clean-room implementation in `third_party/btr-engine`; Xtrieve
contributes the TCP daemon, binary protocol, clients, SDKs, and DOS serial
bridge.

The old custom `.DAT`/`.IX#` storage implementation has been removed. There
is no migration tool. New files are genuine `.BTR` files.

## Architecture

```
client / SDK / DOS bridge
  -> TCP :7419 (op, 128-byte position block, data, key, path, lock)
  -> xtrieved: one thread and SessionId per connection
  -> xtrieve_engine::Engine (Mutex<btr_engine::Engine>)
  -> btr-format / btr-engine: .BTR, .PRE, BTRIEVE.TRN, recovery
```

- Session identity is the TCP connection; it is never stored in the position
  block.
- `btr-engine` owns handles, cursors, file sharing, record locks,
  per-session directories, and per-session transactions.
- Disconnect calls `Engine::end_session`, which aborts the transaction and
  closes the session's handles.

## Crate Structure

- **xtrieve-engine**: adapter and request/response types
- **xtrieved**: TCP daemon and server library
- **xtrieve-client**: sync/async client and high-level helpers
- **third_party/btr-format**, **third_party/btr-engine**: vendored 5.10a engine
- **serial-bridge**: standalone DOS serial-to-TCP bridge

## Commands

```bash
cargo build --release
cargo test --workspace
cargo test -p xtrieved --test tcp
cargo test -p xtrieve-engine --test backend
cargo run -p xtrieved -- --listen 127.0.0.1:7419 --data-dir /tmp/xtrieve-data
```

## Key Files

- `xtrieve-engine/src/operations/dispatcher.rs` - request adapter
- `xtrieved/src/server.rs` - TCP sessions and cleanup
- `xtrieved/src/main.rs` - daemon entry point
- `xtrieve-client/src/btrieve.rs` - high-level API and Create spec
- `third_party/btr-engine/src/lib.rs` - session-aware engine

## Genuine 5.10a contract

- Statuses are raw `u16`; do not reintroduce an enum with different numbers.
- Create uses a 16-byte file header and 16-byte key blocks; key type is at
  key-block `+10`, null value at `+11`, file flags at header `+10`, and
  preallocation at `+14`.
- Position blocks are 128 opaque bytes. `btr-engine` owns `0x00..0x3F` and
  preserves `0x40..0x7F`.
- Genuine operation codes: Open 0x00, Close 0x01, Insert 0x02, Update 0x03,
  Delete 0x04, Gets 0x05..0x0D, Create 0x0E, Stat 0x0F, Extend 0x10,
  Set/Get Directory 0x11/0x12, transactions 0x13..0x15, Get Position 0x16,
  Get Direct 0x17, Step Next 0x18, Stop 0x19, Version 0x1A, Unlock 0x1B,
  Reset 0x1C, owner 0x1D/0x1E, supplemental 0x1F/0x20, steps 0x21..0x23,
  extended 0x24..0x28. Key-only bias is `0x37..0x3F`; lock biases are
  `+0x64/+0xC8/+0x12C/+0x190`.

## Testing

Tests generate their own files with the engine; do not commit proprietary
`.BTR` fixtures. The vendored engine's byte-exact oracle lives in the upstream
`B` project, not in this repository.
