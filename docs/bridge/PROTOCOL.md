# Xtrieve Serial Protocol Specification

The Xtrieve DOS Bridge uses a compact binary protocol over serial communication. All multi-byte values are little-endian.

## Request Format (DOS → Xtrieve)

```
┌──────┬──────┬────────────┬──────────┬──────┬──────┬──────┬──────┐
│ SYNC │  OP  │  POS_BLK   │   DATA   │  KEY │K_NUM │ PATH │ LOCK │
│ 0xBB │  2   │    128     │  4+N     │ 2+N  │  2   │ 2+N  │  2   │
│ 0xBB │bytes │   bytes    │  bytes   │bytes │bytes │bytes │bytes │
└──────┴──────┴────────────┴──────────┴──────┴──────┴──────┴──────┘
```

| Field | Size | Description |
|-------|------|-------------|
| SYNC | 2 bytes | Sync marker: `0xBB 0xBB` |
| OP | 2 bytes | Operation code (u16) |
| POS_BLK | 128 bytes | Position block (file handle + cursor state) |
| DATA | 4 + N bytes | Data length (u32) + data bytes |
| KEY | 2 + N bytes | Key length (u16) + key bytes |
| K_NUM | 2 bytes | Key number (u16) |
| PATH | 2 + N bytes | Path length (u16) + path string |
| LOCK | 2 bytes | Lock bias (u16) |

## Response Format (Xtrieve → DOS)

```
┌──────────┬──────────────┬────────────┬──────────────────────────┐
│  STATUS  │   POS_BLK    │    DATA    │           KEY            │
│    2     │     128      │    4+N     │           2+N            │
│  bytes   │    bytes     │   bytes    │          bytes           │
└──────────┴──────────────┴────────────┴──────────────────────────┘
```

| Field | Size | Description |
|-------|------|-------------|
| STATUS | 2 bytes | Btrieve status code (u16) |
| POS_BLK | 128 bytes | Updated position block |
| DATA | 4 + N bytes | Data length (u32) + record data |
| KEY | 2 + N bytes | Key length (u16) + key value |

## Sync Marker

DOSBox-X sends garbage bytes when establishing serial connections. The bridge uses a sync marker (`0xBB 0xBB`) to detect valid request boundaries:

```
░░░░░░ → 0xBB → 0xBB → [VALID DATA]
garbage   sync   sync   request begins
```

This allows recovery from any desync condition - the bridge simply discards bytes until it sees the sync pattern.

## Status Codes

| Code | Name | Description |
|------|------|-------------|
| 0 | OK | Operation successful |
| 4 | KEY_NOT_FOUND | Key value not found |
| 5 | DUPLICATE_KEY | Duplicate key value |
| 9 | END_OF_FILE | No more records |
| 12 | FILE_NOT_FOUND | File does not exist |
| 22 | DATA_BUFFER_TOO_SHORT | Buffer too small for record |

## Position Block

The 128-byte position block contains:

| Offset | Size | Description |
|--------|------|-------------|
| 0 | 4 | File handle/identifier |
| 4 | 60 | Reserved |
| 64 | 64 | File path (null-terminated) |

## Example Transaction

**Open File Request:**
```
BB BB          # Sync marker
00 00          # Operation: OPEN (0)
[128 bytes]    # Position block (zeros)
04 00 00 00    # Data length: 4
00 00 00 00    # Data: zeros
00 00          # Key length: 0
00 00          # Key number: 0
08 00          # Path length: 8
54 45 53 54    # Path: "TEST"
2E 44 41 54    # Path: ".DAT"
00 00          # Lock bias: 0
```

**Open File Response:**
```
00 00          # Status: OK (0)
[128 bytes]    # Position block (with file handle)
04 00 00 00    # Data length: 4
00 00 00 00    # Data
04 00          # Key length: 4
00 00 00 00    # Key value
```
