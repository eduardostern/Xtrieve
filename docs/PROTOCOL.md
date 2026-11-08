# Xtrieve Binary Protocol Specification

Version 1.0

## Overview

Xtrieve uses a compact binary protocol over TCP for client-server communication. All multi-byte integers are little-endian. The default port is **7419**.

## Connection

Clients establish a TCP connection to the server. Each connection maintains its own session state including:
- Open file handles (position blocks)
- Current cursor positions
- Active locks
- Transaction state

## Request Format

```
┌──────────────┬────────────────┬──────────────┬─────────────┬──────────────┬─────────────┬──────────────┬─────────────┬──────────────┬─────────────┐
│ operation    │ position_block │ data_length  │ data_buffer │ key_length   │ key_buffer  │ key_number   │ path_length │ file_path    │ lock_bias   │
│ (2 bytes)    │ (128 bytes)    │ (4 bytes)    │ (variable)  │ (2 bytes)    │ (variable)  │ (2 bytes)    │ (2 bytes)   │ (variable)   │ (2 bytes)   │
└──────────────┴────────────────┴──────────────┴─────────────┴──────────────┴─────────────┴──────────────┴─────────────┴──────────────┴─────────────┘
```

| Field | Size | Description |
|-------|------|-------------|
| operation | 2 bytes | Btrieve operation code (u16) |
| position_block | 128 bytes | Native Btrieve caller/FCB + cursor state |
| data_length | 4 bytes | For writes, record length; for reads, buffer capacity (u32) |
| data_buffer | variable | Record data for writes, or a zero-filled capacity buffer for reads |
| key_length | 2 bytes | Length of key_buffer (u16) |
| key_buffer | variable | Key value or filename |
| key_number | 2 bytes | Key index; Open mode (-1 normal, -2 read-only, -3 verify, -4 exclusive); Create -1 |
| path_length | 2 bytes | Length of file_path (u16) |
| file_path | variable | Absolute/relative path for Open/Create; bypasses DOS filename parsing |
| lock_bias | 2 bytes | 0/100/200/300/400; added to the base opcode |

## Response Format

```
┌──────────────┬────────────────┬──────────────┬─────────────┬──────────────┬─────────────┐
│ status_code  │ position_block │ data_length  │ data_buffer │ key_length   │ key_buffer  │
│ (2 bytes)    │ (128 bytes)    │ (4 bytes)    │ (variable)  │ (2 bytes)    │ (variable)  │
└──────────────┴────────────────┴──────────────┴─────────────┴──────────────┴─────────────┘
```

| Field | Size | Description |
|-------|------|-------------|
| status_code | 2 bytes | Result status (0 = success, see error codes) |
| position_block | 128 bytes | Updated file handle and cursor state |
| data_length | 4 bytes | Length of returned data_buffer (u32) |
| data_buffer | variable | Retrieved record data |
| key_length | 2 bytes | Length of returned key_buffer (u16) |
| key_buffer | variable | Retrieved key value |

## Position Block

The position block is the native 128-byte Btrieve/N 5.10a caller/FCB state.
`btr-engine` writes bytes `0x00..0x3F` and preserves `0x40..0x7F`. Xtrieve does
not store a session ID or path in it; the TCP connection is the session.

**Important:** Always use the position_block from the previous response for
subsequent operations on the same file.

## Lock Bias

Add these values to the operation code OR pass them in the lock_bias field:

| Value | Constant | Description |
|-------|----------|-------------|
| 0 | NO_LOCK | No locking |
| 100 | SINGLE_WAIT_LOCK | Single record lock (currently non-blocking) |
| 200 | SINGLE_NO_WAIT_LOCK | Single record lock, error if locked |
| 300 | MULTI_WAIT_LOCK | Multi-record lock (currently non-blocking) |
| 400 | MULTI_NO_WAIT_LOCK | Multi-record lock, error if locked |

Wait-lock biases do not block; a conflicting record returns status 84.

## Example: Reading a Record

**Request (hex):**
```
05 00                   # Operation: GetEqual (5)
[128 bytes pos_block]   # Position block from Open
64 00 00 00             # Data capacity: 100 bytes
[100 zero bytes]         # Caller-supplied output capacity
04 00                   # Key length: 4
41 42 43 44             # Key: "ABCD"
00 00                   # Key number: 0
00 00                   # Path length: 0
                        # (no path)
00 00                   # Lock bias: 0
```

**Response (hex):**
```
00 00                   # Status: Success (0)
[128 bytes pos_block]   # Updated position block
64 00 00 00             # Data length: 100
[100 bytes record]      # Record data
04 00                   # Key length: 4
41 42 43 44             # Key: "ABCD"
```
