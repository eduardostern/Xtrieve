# Xtrieve DOS Bridge

A complete bridge that allows **original, unmodified DOS Btrieve applications** from the 1990s to run against a modern Rust database server in 2025.

No recompilation. No source code changes. No emulation of Btrieve.
Just interrupt hooking, serial communication, and protocol translation.

## Overview

The Xtrieve DOS Bridge connects legacy DOS applications to the modern Xtrieve server through a chain of components:

```
┌─────────────────────────────────────────┐
│           DOS APPLICATION               │
│    (Turbo Pascal, Clipper, C, etc.)     │
└──────────────────┬──────────────────────┘
                   │ INT 7Bh (Btrieve Call)
                   ▼
┌─────────────────────────────────────────┐
│            BTRSERL.EXE (TSR)            │
│     Hooks INT 7Bh, serializes calls     │
│              COM1 @ 115200              │
└──────────────────┬──────────────────────┘
                   │ Serial (via DOSBox-X nullmodem)
                   ▼
═══════════════════════════════════════════
              TCP/IP Port 7418
═══════════════════════════════════════════
                   │
                   ▼
┌─────────────────────────────────────────┐
│          SERIAL-BRIDGE (Rust)           │
│     Sync detection, protocol parsing    │
└──────────────────┬──────────────────────┘
                   │ TCP/IP Port 7419
                   ▼
┌─────────────────────────────────────────┐
│            XTRIEVED (Rust)              │
│     Btrieve 5.x ISAM Engine             │
└──────────────────┬──────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│             *.DAT FILES                 │
│       (Native Btrieve Format)           │
└─────────────────────────────────────────┘
```

## Quick Start

### Step 1: Configure DOSBox-X

Add to your `dosbox-x.conf`:

```ini
[serial]
serial1 = nullmodem server:127.0.0.1 port:7418
```

### Step 2: Start Xtrieve Server

```bash
cd xtrieve
cargo run --release -p xtrieved -- --data-dir ./data --listen 127.0.0.1:7419
```

### Step 3: Start Serial Bridge

```bash
cd xtrieve/serial-bridge
cargo run --release
```

Output:
```
═══════════════════════════════════════════
  Xtrieve Serial Bridge (Protocol-Aware)
═══════════════════════════════════════════
Listening on port 7418 for DOSBox-X
[*] Waiting for DOS connections...
```

### Step 4: Load TSR in DOSBox-X

```
C:\> BTRSERL

BTRSERL v1.0 - Btrieve Serial Redirector

Initializing COM1 (115200 baud)...
Installing INT 7B handler...
Going resident.
```

### Step 5: Run Your DOS Application

```
C:\> MYAPP.EXE
```

Your 1990s Btrieve application now uses Xtrieve!

## Components

| Component | Language | Description |
|-----------|----------|-------------|
| **BTRSERL.EXE** | Turbo C 2.0 | DOS TSR (~7KB), hooks INT 7Bh |
| **serial-bridge** | Rust | Protocol translator, sync detection |
| **xtrieved** | Rust | Btrieve 5.x compatible ISAM engine |

## Supported Operations

The bridge is transparent - it forwards ALL operation codes to xtrieved. The following operations are fully implemented:

### File Operations
| Code | Operation | Description |
|------|-----------|-------------|
| 0 | OPEN | Open an existing file |
| 1 | CLOSE | Close an open file |
| 14 | CREATE | Create a new Btrieve file |
| 15 | STAT | Get file statistics |

### Record Operations
| Code | Operation | Description |
|------|-----------|-------------|
| 2 | INSERT | Insert a new record |
| 3 | UPDATE | Update the current record |
| 4 | DELETE | Delete the current record |

### Key Navigation
| Code | Operation | Description |
|------|-----------|-------------|
| 5 | GET_EQUAL | Find record by exact key match |
| 6 | GET_NEXT | Get next record in key order |
| 7 | GET_PREVIOUS | Get previous record in key order |
| 8 | GET_GREATER | Get first record > key |
| 9 | GET_GT_OR_EQ | Get first record >= key |
| 10 | GET_LESS | Get first record < key |
| 11 | GET_LT_OR_EQ | Get first record <= key |
| 12 | GET_FIRST | Get first record in key order |
| 13 | GET_LAST | Get last record in key order |

### Physical Navigation
| Code | Operation | Description |
|------|-----------|-------------|
| 22 | GET_POSITION | Get current physical position |
| 23 | GET_DIRECT | Get record by physical position |
| 24 | STEP_NEXT | Step to next physical record |
| 33 | STEP_FIRST | Step to first physical record |
| 34 | STEP_LAST | Step to last physical record |
| 35 | STEP_PREVIOUS | Step to previous physical record |

### Transactions
| Code | Operation | Description |
|------|-----------|-------------|
| 19 | BEGIN_TRANS | Begin transaction (ACID isolation) |
| 20 | END_TRANS | Commit transaction |
| 21 | ABORT_TRANS | Rollback transaction |

### Utility
| Code | Operation | Description |
|------|-----------|-------------|
| 26 | VERSION | Get Btrieve version info |
| 28 | RESET | Reset session state |

## Windows 98SE Native Support

For running on **real Windows 98SE** (not DOSBox-X), use the COM-to-TCP bridge:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Windows 98SE Machine                          │
├─────────────────────────────────────────────────────────────────┤
│  DOS App → BTRSERL.EXE → COM1 → com0com → COM2 → XTRIEVE.EXE    │
│                                                      │           │
│                                                      │ Winsock   │
└──────────────────────────────────────────────────────┼───────────┘
                                                       │
                                                       ▼
                                               xtrieved (remote)
```

### Requirements

1. **com0com** - Virtual COM port driver (creates COM1 ↔ COM2 pair)
2. **XTRIEVE.EXE** - Windows bridge (reads COM2, sends TCP)
3. **BTRSERL.EXE** - DOS TSR (writes to COM1)

### Setup

1. Install com0com and create a virtual pair (COM1 ↔ COM2)
2. Copy files to `C:\XTRIEVE\`:
   - `XTRIEVE.EXE` (from windows-bridge/)
   - `XTRIEVE.INI` (configure server address)
   - `BTRSERL.EXE` (from dos-client/)

3. Edit `XTRIEVE.INI`:
   ```ini
   [Server]
   Address=192.168.1.100
   Port=7419

   [COM]
   Port=COM2
   ```

4. Run:
   ```batch
   REM Start Windows bridge
   START C:\XTRIEVE\XTRIEVE.EXE

   REM Load DOS TSR
   C:\XTRIEVE\BTRSERL.EXE

   REM Run your application
   C:\MYAPP\MYAPP.EXE
   ```

### Compiling XTRIEVE.EXE

Two versions available (C and Delphi/Pascal):

```batch
REM Borland C++ 5.5
BCC32 -W -O2 XTRIEVE.C WSOCK32.LIB

REM Delphi 3/5/7
DCC32 XTRIEVE.DPR

REM Free Pascal
FPC -Mdelphi XTRIEVE.DPR
```

See `windows-bridge/README.TXT` for more details.

## Documentation

- [Protocol Specification](PROTOCOL.md) - Wire protocol details
- [Technical Reference](TECHNICAL.md) - TSR internals and specifications

## The Story

This bridge represents 30+ years of database evolution - from BBS systems running Btrieve in 1991 to modern Rust servers in 2025. For the full story behind this project, see [The Story](../STORY.md).
