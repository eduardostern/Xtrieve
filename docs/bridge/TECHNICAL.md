# Technical Reference

## Component Specifications

### BTRSERL.EXE (DOS TSR)

| Property | Value |
|----------|-------|
| **Size** | 7,262 bytes |
| **Resident Size** | ~2KB |
| **Compiler** | Turbo C 2.0 |
| **Memory Model** | Small (-ms) |
| **Baud Rate** | 115200 |
| **Serial Port** | COM1 (0x3F8) |
| **Interrupt** | 7Bh |

### serial-bridge (Rust)

| Property | Value |
|----------|-------|
| **Language** | Rust 2021 |
| **Dependencies** | None (pure std) |
| **Listen Port** | 7418 |
| **Target Port** | 7419 (xtrieved) |
| **Sync Marker** | 0xBB 0xBB |

### xtrieved (Rust)

| Property | Value |
|----------|-------|
| **Language** | Rust 2021 |
| **Compatibility** | Btrieve 5.10 |
| **File Format** | Native Btrieve .DAT |
| **Page Sizes** | 512, 1024, 2048, 4096 bytes |
| **Key Types** | String, Integer, Float, etc. |

## TSR Implementation

The DOS TSR hooks INT 7Bh to intercept Btrieve calls:

```c
/* BTRSERL.C - Core interrupt handler */

void interrupt new_int7b(
    unsigned bp, unsigned di,
    unsigned si, unsigned ds,
    unsigned es, unsigned dx,
    unsigned cx, unsigned bx,
    unsigned ax, unsigned ip,
    unsigned cs, unsigned flags)
{
    BTR_PARMS far *parms;
    parms = MK_FP(ds, dx);

    /* Check Btrieve interface ID */
    if (parms->iface_id != 0x6176)
        (*old_int7b)();  /* Chain to original handler */

    /* Process the call via serial */
    status = do_call(parms);
    *(parms->stat_ptr) = status;
}
```

### Building the TSR

```bash
# Using Turbo C 2.0
TCC -ms BTRSERL.C
```

### TSR Memory Layout

```
┌─────────────────────────────────┐
│  PSP (Program Segment Prefix)   │  256 bytes
├─────────────────────────────────┤
│  Code Segment                   │  ~1.5KB
│  - Interrupt handler            │
│  - Serial I/O routines          │
│  - Protocol serialization       │
├─────────────────────────────────┤
│  Data Segment                   │  ~512 bytes
│  - TX/RX buffers                │
│  - Position block cache         │
│  - Old INT 7B vector            │
└─────────────────────────────────┘
```

## Serial Communication

### Initialization Sequence

1. Set baud rate divisor for 115200 bps
2. Configure 8N1 (8 data bits, no parity, 1 stop bit)
3. Enable FIFO if 16550 UART detected
4. Set DTR and RTS

### COM1 Port Registers

| Port | Register | Usage |
|------|----------|-------|
| 0x3F8 | THR/RBR | Transmit/Receive Buffer |
| 0x3F9 | IER | Interrupt Enable |
| 0x3FA | IIR/FCR | Interrupt ID / FIFO Control |
| 0x3FB | LCR | Line Control |
| 0x3FC | MCR | Modem Control |
| 0x3FD | LSR | Line Status |

## DOSBox-X Configuration

### Required Settings

```ini
[serial]
serial1 = nullmodem server:127.0.0.1 port:7418

[cpu]
cycles = max
```

### Nullmodem Parameters

The DOSBox-X nullmodem emulates a direct serial connection over TCP:

- **server:** Connects to specified host:port
- **client:** Listens on specified port
- Automatic flow control handling
- No modem AT commands needed

## Debugging

### Enable Debug Output

```bash
# serial-bridge with verbose logging
RUST_LOG=debug cargo run --release
```

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| No connection | DOSBox-X not running | Start DOSBox-X first |
| Timeout errors | Wrong baud rate | Verify 115200 bps |
| Sync failures | Garbage on line | Bridge auto-recovers |
| Status 12 | File not found | Check data directory path |

## Performance Considerations

- Serial communication adds ~1-5ms latency per operation
- Batch operations when possible
- Keep files in server's data directory for best performance
- The 115200 baud rate handles typical ISAM workloads well
