# Xtrieve DOS Client (BTRSERL)

A TSR (Terminate and Stay Resident) program that intercepts Btrieve INT 7B calls and forwards them to the Xtrieve server over serial port, enabling original DOS Btrieve applications to use Xtrieve as a backend.

## Architecture

```
┌────────────────────────────────────────┐
│  DOS Btrieve App (original, unmodified)│
│              ↓ INT 7B                  │
│  BTRSERL.EXE (TSR ~7KB)               │
│              ↓ COM1 Serial @ 115200    │
│  DOSBox-X nullmodem                    │
│              ↓ TCP/IP                  │
│  serial-bridge (host)                  │
│              ↓ TCP/IP                  │
│  xtrieved (Xtrieve Server)            │
└────────────────────────────────────────┘
```

## Requirements

### DOS Side (DOSBox-X)
- DOSBox-X with serial port support
- Turbo C 2.0 (to compile from source)
- Or use pre-compiled BTRSERL.EXE

### Host Side
- Rust toolchain
- xtrieved (Xtrieve server)
- serial-bridge

## Quick Start

### 1. Configure DOSBox-X

Add to your `dosbox-x.conf`:

```ini
[serial]
serial1 = nullmodem server:127.0.0.1 port:7418
```

### 2. Start Host Services

Terminal 1 - Start Xtrieve server:
```bash
cd /path/to/xtrieve
cargo run -p xtrieved
```

Terminal 2 - Start serial bridge:
```bash
cd /path/to/xtrieve/serial-bridge
cargo run --release
```

### 3. Start DOSBox-X

The serial-bridge should show:
```
[+] DOS client connected: ...
[+] Connected to Xtrieve at 127.0.0.1:7419
```

### 4. Load TSR in DOS

```
C:\> BTRSERL
BTRSERL v1.0 - Btrieve Serial Redirector

Initializing COM1 (115200 baud)...
Installing INT 7B handler...
Going resident.
```

### 5. Run Your Btrieve Application

Any DOS application that uses Btrieve via INT 7B will now transparently use Xtrieve!

## Building from Source

In DOSBox with Turbo C 2.0:

```
C:\TC> TCC -ms BTRSERL.C
```

The `-ms` flag selects the small memory model, required for proper far pointer handling.

## How It Works

1. **BTRSERL** hooks INT 7B (the Btrieve interrupt)
2. When a Btrieve call is made, BTRSERL:
   - Reads the BTR_PARMS structure from DS:DX
   - Serializes it to Xtrieve protocol format
   - Sends a sync marker (0xBB 0xBB) followed by the request
   - Waits for response over serial
   - Deserializes response back to caller's buffers
3. **DOSBox-X nullmodem** forwards serial data to TCP port 7418
4. **serial-bridge** receives data, waits for sync marker, parses protocol, forwards to Xtrieve
5. **xtrieved** processes the Btrieve operation and returns result

## Protocol

### Request Format (DOS → Xtrieve)
```
[sync:2][op:2][pos_block:128][data_len:4][data:N][key_len:2][key:N][key_num:2][path_len:2][path:N][lock:2]
```

### Response Format (Xtrieve → DOS)
```
[status:2][pos_block:128][data_len:4][data:N][key_len:2][key:N]
```

## Supported Operations

All standard Btrieve 5.x operations are supported:

| Op | Name | Description |
|----|------|-------------|
| 0 | OPEN | Open a file |
| 1 | CLOSE | Close a file |
| 2 | INSERT | Insert a record |
| 3 | UPDATE | Update current record |
| 4 | DELETE | Delete current record |
| 5 | GET_EQUAL | Find by key value |
| 6 | GET_NEXT | Get next record |
| 7 | GET_PREV | Get previous record |
| 12 | GET_FIRST | Get first record |
| 13 | GET_LAST | Get last record |
| 14 | CREATE | Create a new file |
| ... | ... | And more |

## Troubleshooting

### "File not open" errors (status 3)
- Position block may have been corrupted during transmission
- Try reducing operation frequency or adding delays

### No connection from DOSBox
- Verify DOSBox-X config has correct serial1 line
- Ensure serial-bridge is running BEFORE starting DOSBox-X
- Check port 7418 is not in use

### Garbage data / desync
- The sync marker (0xBB 0xBB) helps recover from garbage
- DOSBox-X sends some bytes on connection; bridge skips until sync

## Files

- `BTRSERL.C` - TSR source code (Turbo C 2.0)
- `BTRSERL.EXE` - Pre-compiled TSR executable
- `README.md` - This file

## License

Part of the Xtrieve project.
