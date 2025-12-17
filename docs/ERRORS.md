# Xtrieve Error Codes

Complete reference for all status codes returned by Xtrieve operations.

## Success

| Code | Name | Description |
|------|------|-------------|
| 0 | Success | Operation completed successfully |

## Positioning Errors

| Code | Name | Description |
|------|------|-------------|
| 4 | KeyNotFound | No record matches the specified key value |
| 8 | InvalidPositioning | No current record (must read before update/delete) |
| 9 | EndOfFile | No more records in the specified direction |

## Key/Index Errors

| Code | Name | Description |
|------|------|-------------|
| 5 | DuplicateKey | Attempted to insert duplicate value in unique key |
| 6 | InvalidKeyNumber | Key number does not exist in file |
| 7 | DifferentKeyNumber | Key number changed during operation sequence |

## File Errors

| Code | Name | Description |
|------|------|-------------|
| 2 | IOError | Disk I/O error occurred |
| 3 | FileNotOpen | Attempted operation on closed file |
| 11 | InvalidFileName | File path is invalid or malformed |
| 12 | FileNotFound | Specified file does not exist |
| 13 | FileExtensionError | Invalid file extension |
| 18 | DiskFull | No space left on device |
| 30 | NotABtrieveFile | File is not a valid Btrieve file |
| 88 | FileAlreadyOpen | File is already open in incompatible mode |

## Record Errors

| Code | Name | Description |
|------|------|-------------|
| 22 | DataBufferTooShort | Provided buffer is smaller than record length |

## Lock Errors

| Code | Name | Description |
|------|------|-------------|
| 84 | RecordLocked | Record is locked by another session |
| 85 | FileLocked | File is locked by another session |
| 78 | DeadlockDetected | Transaction deadlock detected |

## Transaction Errors

| Code | Name | Description |
|------|------|-------------|
| 36 | TransactionError | General transaction error |
| 37 | TransactionEnded | Transaction has already ended |
| 38 | TransactionMaxFiles | Too many files in transaction |
| 39 | TransactionWriteConflict | Write conflict with another transaction |

## Access Errors

| Code | Name | Description |
|------|------|-------------|
| 46 | AccessDenied | Permission denied |
| 94 | PermissionError | Insufficient permissions |

## Internal Errors

| Code | Name | Description |
|------|------|-------------|
| 1 | InvalidOperation | Unknown or unsupported operation code |
| 20 | InternalError | Internal engine error |

## Handling Errors

### Rust

```rust
use xtrieve_client::StatusCode;

let resp = client.execute(request)?;

match resp.status_code {
    0 => {
        // Success - process data
        println!("Record: {:?}", resp.data_buffer);
    }
    4 => {
        // Key not found - expected in some cases
        println!("Record not found");
    }
    9 => {
        // End of file - normal when iterating
        break;
    }
    84 => {
        // Locked - retry or wait
        std::thread::sleep(Duration::from_millis(100));
        continue;
    }
    code => {
        // Unexpected error
        return Err(format!("Btrieve error: {}", code).into());
    }
}
```

### JavaScript

```javascript
const resp = await client.execute(request);

switch (resp.statusCode) {
    case 0:
        console.log('Success:', resp.dataBuffer);
        break;
    case 4:
        console.log('Not found');
        break;
    case 9:
        console.log('End of file');
        break;
    case 84:
        throw new Error('Record locked');
    default:
        throw new Error(`Btrieve error: ${resp.statusCode}`);
}
```

### Go

```go
resp, err := client.Execute(request)
if err != nil {
    return err
}

switch resp.StatusCode {
case 0:
    fmt.Printf("Success: %v\n", resp.DataBuffer)
case 4:
    fmt.Println("Not found")
case 9:
    fmt.Println("End of file")
case 84:
    return fmt.Errorf("record locked")
default:
    return fmt.Errorf("btrieve error: %d", resp.StatusCode)
}
```

## Retry Strategies

### Lock Contention

When receiving error 84 (RecordLocked), implement exponential backoff:

```rust
let mut delay = Duration::from_millis(10);
let max_retries = 5;

for attempt in 0..max_retries {
    let resp = client.execute(request.clone())?;

    if resp.status_code != 84 {
        return Ok(resp);
    }

    std::thread::sleep(delay);
    delay *= 2;  // Exponential backoff
}

Err("Max retries exceeded".into())
```

### Deadlock Recovery

When receiving error 78 (DeadlockDetected), abort and retry the entire transaction:

```rust
loop {
    // Begin transaction
    client.execute(begin_request())?;

    match perform_transaction(&mut client) {
        Ok(_) => {
            client.execute(commit_request())?;
            break;
        }
        Err(e) if e.code == 78 => {
            // Deadlock - abort and retry
            client.execute(abort_request())?;
            std::thread::sleep(Duration::from_millis(rand::random::<u64>() % 100));
            continue;
        }
        Err(e) => {
            client.execute(abort_request())?;
            return Err(e);
        }
    }
}
```
