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
| position_block | 128 bytes | File handle and cursor state |
| data_length | 4 bytes | Length of data_buffer (u32) |
| data_buffer | variable | Record data (for insert/update) or buffer for retrieval |
| key_length | 2 bytes | Length of key_buffer (u16) |
| key_buffer | variable | Key value for keyed operations |
| key_number | 2 bytes | Key index to use (i16, -1 for physical access) |
| path_length | 2 bytes | Length of file_path (u16) |
| file_path | variable | File path for Open/Create operations (UTF-8) |
| lock_bias | 2 bytes | Lock type modifier (u16) |

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

The position block (128 bytes) is an opaque handle that maintains:
- File identifier
- Current record position
- Current key number
- Cursor state for sequential access

**Important:** Always use the position_block from the previous response for subsequent operations on the same file.

## Lock Bias

Add these values to the operation code OR pass in lock_bias field:

| Value | Constant | Description |
|-------|----------|-------------|
| 0 | NO_LOCK | No locking |
| 100 | SINGLE_WAIT_LOCK | Single record lock, wait if locked |
| 200 | SINGLE_NO_WAIT_LOCK | Single record lock, return error if locked |
| 300 | MULTI_WAIT_LOCK | Multi-record lock, wait if locked |
| 400 | MULTI_NO_WAIT_LOCK | Multi-record lock, return error if locked |

## Example: Reading a Record

**Request (hex):**
```
05 00                   # Operation: GetEqual (5)
[128 bytes pos_block]   # Position block from Open
04 00 00 00             # Data length: 4
00 00 00 00             # Data buffer (unused for read)
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
