# Xtrieve Operations Reference

Complete reference for all Btrieve-compatible operations.

## Table of Contents

- [File Operations](#file-operations)
  - [Open (0)](#open-0)
  - [Close (1)](#close-1)
  - [Create (14)](#create-14)
  - [Stat (15)](#stat-15)
- [Record Operations](#record-operations)
  - [Insert (2)](#insert-2)
  - [Update (3)](#update-3)
  - [Delete (4)](#delete-4)
- [Key-Based Retrieval](#key-based-retrieval)
  - [GetEqual (5)](#getequal-5)
  - [GetNext (6)](#getnext-6)
  - [GetPrevious (7)](#getprevious-7)
  - [GetGreater (8)](#getgreater-8)
  - [GetGreaterOrEqual (9)](#getgreaterorequal-9)
  - [GetLess (10)](#getless-10)
  - [GetLessOrEqual (11)](#getlessorequal-11)
  - [GetFirst (12)](#getfirst-12)
  - [GetLast (13)](#getlast-13)
- [Physical Access](#physical-access)
  - [StepNext (24)](#stepnext-24)
  - [StepFirst (33)](#stepfirst-33)
  - [StepLast (34)](#steplast-34)
  - [StepPrevious (35)](#stepprevious-35)
- [Transaction Operations](#transaction-operations)
  - [BeginTransaction (19)](#begintransaction-19)
  - [EndTransaction (20)](#endtransaction-20)
  - [AbortTransaction (21)](#aborttransaction-21)
- [Locking Operations](#locking-operations)
  - [Unlock (27)](#unlock-27)

---

## File Operations

### Open (0)

Opens an existing Btrieve file for access.

**Request:**
| Field | Value |
|-------|-------|
| operation | 0 |
| file_path | Path to the .dat file |
| key_number | Open mode (-1 = normal, -2 = read-only, -3 = exclusive) |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |
| position_block | File handle for subsequent operations |

**Example (Rust):**
```rust
let resp = client.execute(BtrieveRequest {
    operation_code: 0,
    file_path: "customers.dat".to_string(),
    key_number: -1,  // Normal mode
    ..Default::default()
})?;
let pos_block = resp.position_block;  // Save this!
```

**Example (JavaScript):**
```javascript
const resp = await client.execute({
    operation: 0,
    filePath: 'customers.dat',
    keyNumber: -1
});
const posBlock = resp.positionBlock;
```

**Possible Errors:**
- 12: File not found
- 88: File already open in incompatible mode

---

### Close (1)

Closes an open file and releases all locks.

**Request:**
| Field | Value |
|-------|-------|
| operation | 1 |
| position_block | Handle from Open |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |

**Example:**
```rust
client.execute(BtrieveRequest {
    operation_code: 1,
    position_block: pos_block,
    ..Default::default()
})?;
```

**Notes:**
- Always close files when done
- Closing releases all record locks held by this session
- Closing a file in a transaction does NOT abort the transaction

---

### Create (14)

Creates a new Btrieve file with specified structure.

**Request:**
| Field | Value |
|-------|-------|
| operation | 14 |
| file_path | Path for new file |
| data_buffer | File specification (see below) |

**File Specification Format:**
```
Offset  Size  Description
0       2     Record length (bytes)
2       2     Page size (512, 1024, 2048, or 4096)
4       2     Number of keys
6       4     Reserved (set to 0)
10      16*N  Key specifications (N = number of keys)
```

**Key Specification Format (16 bytes each):**
```
Offset  Size  Description
0       2     Key position (byte offset in record)
2       2     Key length
4       2     Key flags (see below)
6       1     Key type (see below)
7       1     Null value
8       8     Reserved
```

**Key Flags:**
| Value | Description |
|-------|-------------|
| 0x0001 | Duplicates allowed |
| 0x0002 | Modifiable |
| 0x0004 | Binary key (not sorted as string) |
| 0x0008 | Null key (all nulls = no index entry) |
| 0x0010 | Segmented key (continues in next spec) |
| 0x0020 | Descending order |
| 0x0040 | Supplemental key |
| 0x0080 | Extended type |

**Key Types:**
| Value | Type |
|-------|------|
| 0 | String (null-terminated) |
| 1 | Integer (signed) |
| 2 | Float |
| 3 | Date |
| 4 | Time |
| 5 | Decimal |
| 6 | Money |
| 7 | Logical |
| 8 | Numeric |
| 9 | Bfloat |
| 10 | Lstring (length-prefixed) |
| 11 | Zstring (null-terminated) |
| 14 | Unsigned binary |
| 15 | Autoincrement |

**Example:**
```rust
fn build_file_spec(record_len: u16, page_size: u16, keys: &[(u16, u16, u16, u8)]) -> Vec<u8> {
    let mut buf = Vec::new();

    // Header
    buf.extend_from_slice(&record_len.to_le_bytes());
    buf.extend_from_slice(&page_size.to_le_bytes());
    buf.extend_from_slice(&(keys.len() as u16).to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]);  // Reserved

    // Key specs
    for (pos, len, flags, key_type) in keys {
        buf.extend_from_slice(&pos.to_le_bytes());
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&flags.to_le_bytes());
        buf.push(*key_type);
        buf.push(0);  // Null value
        buf.extend_from_slice(&[0u8; 8]);  // Reserved
    }

    buf
}

// Create file with 100-byte records, 4KB pages, one key
let spec = build_file_spec(100, 4096, &[
    (0, 8, 0x0001, 14),  // Offset 0, 8 bytes, duplicates OK, unsigned binary
]);

client.execute(BtrieveRequest {
    operation_code: 14,
    file_path: "newfile.dat".to_string(),
    data_buffer: spec,
    ..Default::default()
})?;
```

---

### Stat (15)

Retrieves file statistics and structure information.

**Request:**
| Field | Value |
|-------|-------|
| operation | 15 |
| position_block | Handle from Open |
| data_buffer | Empty buffer (will receive stats) |

**Response:**
| Field | Description |
|-------|-------------|
| data_buffer | File statistics (same format as Create spec, plus record count) |

**Extended Stat Format:**
```
Offset  Size  Description
0       2     Record length
2       2     Page size
4       2     Number of keys
6       4     Record count
10      2     Unused pages
12      4     Flags
16+     16*N  Key specifications
```

---

## Record Operations

### Insert (2)

Inserts a new record into the file.

**Request:**
| Field | Value |
|-------|-------|
| operation | 2 |
| position_block | Handle from Open |
| data_buffer | Complete record data |
| data_buffer_length | Record length |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |
| position_block | Updated (cursor points to inserted record) |

**Example:**
```rust
let record = build_customer_record("John Doe", "john@example.com");
let resp = client.execute(BtrieveRequest {
    operation_code: 2,
    position_block: pos_block,
    data_buffer: record,
    data_buffer_length: 100,
    ..Default::default()
})?;
pos_block = resp.position_block;  // Update position block
```

**Possible Errors:**
- 5: Duplicate key value (if duplicates not allowed)
- 18: Disk full
- 22: Data buffer too short

---

### Update (3)

Updates the current record (must be positioned on a record first).

**Request:**
| Field | Value |
|-------|-------|
| operation | 3 |
| position_block | Handle positioned on record |
| data_buffer | New record data |
| data_buffer_length | Record length |
| key_number | Key that was used to position |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |

**Example:**
```rust
// First, get the record
let resp = client.execute(BtrieveRequest {
    operation_code: 5,  // GetEqual
    position_block: pos_block,
    key_buffer: customer_id.to_le_bytes().to_vec(),
    key_number: 0,
    ..Default::default()
})?;

// Modify it
let mut record = resp.data_buffer;
record[50..60].copy_from_slice(b"NewValue  ");

// Update
client.execute(BtrieveRequest {
    operation_code: 3,
    position_block: resp.position_block,
    data_buffer: record,
    key_number: 0,
    ..Default::default()
})?;
```

**Possible Errors:**
- 5: Duplicate key (if changing a unique key to existing value)
- 8: Invalid positioning (no current record)

---

### Delete (4)

Deletes the current record.

**Request:**
| Field | Value |
|-------|-------|
| operation | 4 |
| position_block | Handle positioned on record |
| key_number | Key that was used to position |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |

**Example:**
```rust
// Position on record first
let resp = client.execute(BtrieveRequest {
    operation_code: 5,  // GetEqual
    position_block: pos_block,
    key_buffer: customer_id.to_le_bytes().to_vec(),
    key_number: 0,
    ..Default::default()
})?;

// Delete it
client.execute(BtrieveRequest {
    operation_code: 4,
    position_block: resp.position_block,
    key_number: 0,
    ..Default::default()
})?;
```

**Possible Errors:**
- 8: Invalid positioning

---

## Key-Based Retrieval

All key-based operations use the B+ tree index for efficient access.

### GetEqual (5)

Retrieves the first record matching the exact key value.

**Request:**
| Field | Value |
|-------|-------|
| operation | 5 |
| position_block | Handle from Open |
| key_buffer | Key value to search for |
| key_number | Index to use (0-based) |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 4 if not found |
| data_buffer | Record data |
| key_buffer | Actual key value |
| position_block | Updated cursor position |

**Example:**
```rust
let resp = client.execute(BtrieveRequest {
    operation_code: 5,
    position_block: pos_block,
    key_buffer: b"CUST001".to_vec(),
    key_number: 0,
    ..Default::default()
})?;

if resp.status_code == 0 {
    println!("Found: {:?}", resp.data_buffer);
}
```

---

### GetNext (6)

Retrieves the next record in key order.

**Request:**
| Field | Value |
|-------|-------|
| operation | 6 |
| position_block | Handle positioned in file |
| key_number | Index to traverse |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 9 if end of file |
| data_buffer | Record data |
| key_buffer | Key value |

**Example - Iterate all records:**
```rust
// Get first
let mut resp = client.execute(BtrieveRequest {
    operation_code: 12,  // GetFirst
    position_block: pos_block,
    key_number: 0,
    ..Default::default()
})?;

while resp.status_code == 0 {
    process_record(&resp.data_buffer);

    resp = client.execute(BtrieveRequest {
        operation_code: 6,  // GetNext
        position_block: resp.position_block,
        key_number: 0,
        ..Default::default()
    })?;
}
```

---

### GetPrevious (7)

Retrieves the previous record in key order.

**Request:**
| Field | Value |
|-------|-------|
| operation | 7 |
| position_block | Handle positioned in file |
| key_number | Index to traverse |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 9 if beginning of file |
| data_buffer | Record data |
| key_buffer | Key value |

---

### GetGreater (8)

Retrieves the first record with key greater than specified value.

**Request:**
| Field | Value |
|-------|-------|
| operation | 8 |
| position_block | Handle from Open |
| key_buffer | Key value (exclusive lower bound) |
| key_number | Index to use |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 9 if none found |
| data_buffer | Record data |
| key_buffer | Actual key value |

---

### GetGreaterOrEqual (9)

Retrieves the first record with key greater than or equal to specified value.

**Request:**
| Field | Value |
|-------|-------|
| operation | 9 |
| position_block | Handle from Open |
| key_buffer | Key value (inclusive lower bound) |
| key_number | Index to use |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 9 if none found |
| data_buffer | Record data |
| key_buffer | Actual key value |

**Use Case - Range Query:**
```rust
// Find all customers with ID >= 1000
let mut resp = client.execute(BtrieveRequest {
    operation_code: 9,  // GetGreaterOrEqual
    position_block: pos_block,
    key_buffer: 1000i64.to_le_bytes().to_vec(),
    key_number: 0,
    ..Default::default()
})?;

while resp.status_code == 0 {
    let id = i64::from_le_bytes(resp.key_buffer[0..8].try_into().unwrap());
    if id >= 2000 { break; }  // End of range

    process_record(&resp.data_buffer);

    resp = client.execute(BtrieveRequest {
        operation_code: 6,  // GetNext
        position_block: resp.position_block,
        key_number: 0,
        ..Default::default()
    })?;
}
```

---

### GetLess (10)

Retrieves the first record with key less than specified value.

**Request:**
| Field | Value |
|-------|-------|
| operation | 10 |
| position_block | Handle from Open |
| key_buffer | Key value (exclusive upper bound) |
| key_number | Index to use |

---

### GetLessOrEqual (11)

Retrieves the first record with key less than or equal to specified value.

**Request:**
| Field | Value |
|-------|-------|
| operation | 11 |
| position_block | Handle from Open |
| key_buffer | Key value (inclusive upper bound) |
| key_number | Index to use |

---

### GetFirst (12)

Retrieves the first record in key order.

**Request:**
| Field | Value |
|-------|-------|
| operation | 12 |
| position_block | Handle from Open |
| key_number | Index to use |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 9 if file is empty |
| data_buffer | First record |
| key_buffer | Key value |

---

### GetLast (13)

Retrieves the last record in key order.

**Request:**
| Field | Value |
|-------|-------|
| operation | 13 |
| position_block | Handle from Open |
| key_number | Index to use |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 9 if file is empty |
| data_buffer | Last record |
| key_buffer | Key value |

---

## Physical Access

Physical access operations traverse records in physical storage order, ignoring indexes.

### StepNext (24)

Steps to the next physical record.

**Request:**
| Field | Value |
|-------|-------|
| operation | 24 |
| position_block | Handle positioned in file |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 9 if end of file |
| data_buffer | Record data |

---

### StepFirst (33)

Steps to the first physical record.

**Request:**
| Field | Value |
|-------|-------|
| operation | 33 |
| position_block | Handle from Open |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success, 9 if file is empty |
| data_buffer | First physical record |

**Use Case - Full Table Scan:**
```rust
let mut resp = client.execute(BtrieveRequest {
    operation_code: 33,  // StepFirst
    position_block: pos_block,
    ..Default::default()
})?;

let mut count = 0;
while resp.status_code == 0 {
    count += 1;
    resp = client.execute(BtrieveRequest {
        operation_code: 24,  // StepNext
        position_block: resp.position_block,
        ..Default::default()
    })?;
}
println!("Total records: {}", count);
```

---

### StepLast (34)

Steps to the last physical record.

**Request:**
| Field | Value |
|-------|-------|
| operation | 34 |
| position_block | Handle from Open |

---

### StepPrevious (35)

Steps to the previous physical record.

**Request:**
| Field | Value |
|-------|-------|
| operation | 35 |
| position_block | Handle positioned in file |

---

## Transaction Operations

Transactions provide ACID guarantees for multiple operations.

### BeginTransaction (19)

Starts a new transaction.

**Request:**
| Field | Value |
|-------|-------|
| operation | 19 |
| position_block | Any open file handle |
| lock_bias | Lock mode (100=exclusive, 200=shared) |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |

**Example:**
```rust
// Begin exclusive transaction
client.execute(BtrieveRequest {
    operation_code: 19,
    position_block: pos_block,
    lock_bias: 100,  // Exclusive
    ..Default::default()
})?;

// Perform operations...
// Changes are isolated until commit
```

**Notes:**
- All operations after Begin are part of the transaction
- Changes are not visible to other sessions until EndTransaction
- Locks are held until transaction ends

---

### EndTransaction (20)

Commits the current transaction.

**Request:**
| Field | Value |
|-------|-------|
| operation | 20 |
| position_block | Any open file handle |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |

**Example:**
```rust
// Commit transaction
client.execute(BtrieveRequest {
    operation_code: 20,
    position_block: pos_block,
    ..Default::default()
})?;
// All changes are now permanent and visible
```

---

### AbortTransaction (21)

Rolls back the current transaction.

**Request:**
| Field | Value |
|-------|-------|
| operation | 21 |
| position_block | Any open file handle |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |

**Example:**
```rust
// Rollback on error
if let Err(e) = do_complex_operation() {
    client.execute(BtrieveRequest {
        operation_code: 21,  // Abort
        position_block: pos_block,
        ..Default::default()
    })?;
    return Err(e);
}
```

---

## Locking Operations

### Unlock (27)

Releases locks on the current record or all records.

**Request:**
| Field | Value |
|-------|-------|
| operation | 27 |
| position_block | Handle from Open |
| lock_bias | -1 to unlock all, -2 to unlock current |

**Response:**
| Field | Description |
|-------|-------------|
| status_code | 0 on success |

**Example:**
```rust
// Read with lock
let resp = client.execute(BtrieveRequest {
    operation_code: 5,  // GetEqual
    position_block: pos_block,
    key_buffer: key,
    key_number: 0,
    lock_bias: 100,  // Single wait lock
    ..Default::default()
})?;

// Process...

// Release lock
client.execute(BtrieveRequest {
    operation_code: 27,  // Unlock
    position_block: resp.position_block,
    lock_bias: -2,  // Current record
    ..Default::default()
})?;
```
