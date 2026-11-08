# Xtrieve Status Codes

Statuses are the genuine Btrieve/N 5.10a `u16` values. They are returned
verbatim by the engine; there is no second enum with different numbers.

## Common statuses

| Code | Name | Meaning |
|------|------|---------|
| 0 | SUCCESS | Operation completed |
| 1 | INVALID_OPERATION | Unknown/unsupported opcode |
| 2 | IO_ERROR | I/O error |
| 3 | FILE_NOT_OPEN | Invalid handle or handle belongs to another session |
| 4 | KEY_NOT_FOUND | No matching key |
| 5 | DUPLICATE_KEY | Duplicate value for a unique key |
| 6 | INVALID_KEY_NUMBER | Key number does not exist |
| 7 | DIFFERENT_KEY_NUMBER | Key changed between related calls |
| 8 | INVALID_POSITIONING | No current record |
| 9 | END_OF_FILE | No more records |
| 10 | MODIFIABLE_KEY_ERROR | Non-modifiable key changed |
| 11 | INVALID_FILE_NAME | Invalid filename |
| 12 | FILE_NOT_FOUND | File does not exist |
| 13 | EXTENDED_FILE_ERROR | Extension file problem |
| 14 | PREIMAGE_OPEN_ERROR | Preimage missing/could not open |
| 15 | PREIMAGE_IO_ERROR | Preimage I/O/format error |
| 17 | CLOSE_ERROR | Close failed |
| 21 | KEY_BUFFER_TOO_SHORT | Key buffer too small |
| 22 | DATA_BUFFER_LENGTH | Data buffer too small |
| 23 | POSITION_BLOCK_LENGTH | Position block must be 128 bytes |
| 24 | PAGE_SIZE_ERROR | Invalid page size |
| 25 | CREATE_IO_ERROR | Create failed |
| 26 | NUMBER_OF_KEYS | Too many keys |
| 27 | INVALID_KEY_POSITION | Key outside record |
| 28 | INVALID_RECORD_LENGTH | Invalid record length |
| 29 | INVALID_KEY_LENGTH | Invalid key length |
| 30 | NOT_A_BTRIEVE_FILE | Not a genuine `.BTR` file |
| 32 | EXTEND_IO_ERROR | Extend failed |
| 34 | INVALID_EXTENSION_NAME | Invalid extension path |
| 35 | DIRECTORY_ERROR | Directory operation failed |
| 36 | TRANSACTION_ERROR | General transaction error |
| 37 | TRANSACTION_ACTIVE | Transaction already active |
| 38 | TRANSACTION_FILE_IO_ERROR | Transaction/log I/O error |
| 39 | END_ABORT_ERROR | End/Abort without a transaction |
| 41 | OPERATION_NOT_ALLOWED | Operation not allowed in this state |
| 43 | INVALID_RECORD_ADDRESS | Bad record address |
| 46 | ACCESS_DENIED | Access denied |
| 50 | OWNER_ALREADY_SET | Owner already set |
| 51 | INVALID_OWNER | Invalid owner name/hash |
| 59 | FILE_ALREADY_EXISTS | Create with mode -1 on existing file |
| 60 | REJECT_COUNT_REACHED | Extended filter reject limit |
| 61 | WORK_SPACE_TOO_SMALL | Extended workspace too small |
| 62 | INCORRECT_DESCRIPTOR | Bad extended descriptor |
| 64 | FILTER_LIMIT_REACHED | Filter optimization limit |
| 65 | INCORRECT_FIELD_OFFSET | Invalid extractor field |
| 82 | LOST_POSITION | Cursor position lost |
| 84 | RECORD_IN_USE | Record locked by another handle |
| 85 | FILE_IN_USE | File exclusive or owned by another transaction |
| 86 | FILE_TABLE_FULL | Too many open files |
| 87 | HANDLE_TABLE_FULL | Too many handles |
| 88 | INCOMPATIBLE_MODE | Incompatible open modes |

## Session behavior

- Each TCP connection is a session.
- A session can use only its own handles. Reusing another session's position
  block returns 3.
- While a session's transaction owns a file, other sessions receive 85.
- Disconnect aborts the session's transaction and closes its handles.
- Wait-lock biases do not block; a conflicting record returns 84.

## Client handling

Rust:

```rust
match response.status_code {
    0 => {}
    4 => { /* not found */ }
    9 => { /* end of file */ }
    status => return Err(format!("Btrieve status {status}").into()),
}
```
