# Xtrieve Client for Go

Go client library for Xtrieve - a Btrieve 5.1 compatible ISAM database engine.

## Installation

```bash
go get github.com/eduardostern/xtrieve-go
```

## Quick Start

```go
package main

import (
    "encoding/binary"
    "fmt"
    "log"

    xtrieve "github.com/eduardostern/xtrieve-go"
)

func main() {
    // Connect
    client, err := xtrieve.Connect("127.0.0.1", 7419)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Create a file
    spec := &xtrieve.FileSpec{
        RecordLength: 100,
        PageSize:     4096,
        Keys: []xtrieve.KeySpec{
            {Position: 0, Length: 8, Type: xtrieve.KeyTypeUnsignedBinary},
        },
    }
    client.Create("customers.dat", spec)

    // Open file
    resp, err := client.Open("customers.dat", -1)
    if err != nil {
        log.Fatal(err)
    }
    posBlock := resp.PositionBlock

    // Insert a record
    record := make([]byte, 100)
    binary.LittleEndian.PutUint64(record[0:], 1001)  // ID
    copy(record[8:], "John Doe")                      // Name

    resp, err = client.Insert(posBlock, record)
    if err != nil {
        log.Fatal(err)
    }
    posBlock = resp.PositionBlock

    // Read by key
    key := make([]byte, 8)
    binary.LittleEndian.PutUint64(key, 1001)

    resp, err = client.GetEqual(posBlock, key, 0)
    if err != nil {
        log.Fatal(err)
    }

    if resp.StatusCode == xtrieve.StatusSuccess {
        id := binary.LittleEndian.Uint64(resp.DataBuffer[0:8])
        name := string(resp.DataBuffer[8:40])
        fmt.Printf("Found: ID=%d, Name=%s\n", id, name)
    }

    // Close file
    client.CloseFile(posBlock)
}
```

## API Reference

### Connection

```go
// Connect to server
client, err := xtrieve.Connect("127.0.0.1", 7419)
if err != nil {
    log.Fatal(err)
}

// Close connection
defer client.Close()
```

### File Operations

```go
// Open file (-1=normal, -2=read-only, -3=exclusive)
resp, err := client.Open("data.dat", -1)
posBlock := resp.PositionBlock

// Close file
resp, err := client.CloseFile(posBlock)

// Create file
spec := &xtrieve.FileSpec{
    RecordLength: 100,
    PageSize:     4096,
    Keys: []xtrieve.KeySpec{
        {Position: 0, Length: 8, Type: xtrieve.KeyTypeUnsignedBinary},
        {Position: 8, Length: 32, Flags: xtrieve.KeyFlagDuplicates, Type: xtrieve.KeyTypeString},
    },
}
resp, err := client.Create("data.dat", spec)
```

### Record Operations

```go
// Insert
resp, err := client.Insert(posBlock, recordData)
posBlock = resp.PositionBlock

// Update current record
resp, err := client.Update(posBlock, newData, keyNumber)

// Delete current record
resp, err := client.Delete(posBlock, keyNumber)
```

### Key-Based Retrieval

```go
// Get by exact key match
resp, err := client.GetEqual(posBlock, keyValue, keyNumber)

// Get first record
resp, err := client.GetFirst(posBlock, keyNumber)

// Get last record
resp, err := client.GetLast(posBlock, keyNumber)

// Get next record
resp, err := client.GetNext(posBlock, keyNumber)

// Get previous record
resp, err := client.GetPrevious(posBlock, keyNumber)
```

### Transactions

```go
// Begin transaction
resp, err := client.BeginTransaction(posBlock, xtrieve.LockSingleWait)

// Commit
resp, err := client.EndTransaction(posBlock)

// Rollback
resp, err := client.AbortTransaction(posBlock)
```

### Iteration

```go
// Iterate all records
count, err := client.ForEach(posBlock, 0, func(record, key []byte) error {
    id := binary.LittleEndian.Uint64(record[0:8])
    fmt.Printf("Record ID: %d\n", id)
    return nil
})
fmt.Printf("Processed %d records\n", count)
```

### Low-Level

```go
// Execute any operation
resp, err := client.Execute(&xtrieve.Request{
    Operation:     xtrieve.OpGetGreaterOrEqual,
    PositionBlock: posBlock,
    KeyBuffer:     keyValue,
    KeyNumber:     0,
})
```

## Constants

### Operations

```go
xtrieve.OpOpen              // 0
xtrieve.OpClose             // 1
xtrieve.OpInsert            // 2
xtrieve.OpUpdate            // 3
xtrieve.OpDelete            // 4
xtrieve.OpGetEqual          // 5
xtrieve.OpGetNext           // 6
xtrieve.OpGetPrevious       // 7
xtrieve.OpGetGreater        // 8
xtrieve.OpGetGreaterOrEqual // 9
xtrieve.OpGetLess           // 10
xtrieve.OpGetLessOrEqual    // 11
xtrieve.OpGetFirst          // 12
xtrieve.OpGetLast           // 13
xtrieve.OpCreate            // 14
xtrieve.OpStat              // 15
xtrieve.OpBeginTransaction  // 19
xtrieve.OpEndTransaction    // 20
xtrieve.OpAbortTransaction  // 21
xtrieve.OpStepNext          // 24
xtrieve.OpUnlock            // 27
xtrieve.OpStepFirst         // 33
xtrieve.OpStepLast          // 34
xtrieve.OpStepPrevious      // 35
```

### Status Codes

```go
xtrieve.StatusSuccess            // 0
xtrieve.StatusKeyNotFound        // 4
xtrieve.StatusDuplicateKey       // 5
xtrieve.StatusInvalidPositioning // 8
xtrieve.StatusEndOfFile          // 9
xtrieve.StatusFileNotFound       // 12
xtrieve.StatusRecordLocked       // 84
xtrieve.StatusFileLocked         // 85
```

### Key Types

```go
xtrieve.KeyTypeString        // 0
xtrieve.KeyTypeInteger       // 1
xtrieve.KeyTypeFloat         // 2
xtrieve.KeyTypeUnsignedBinary // 14
xtrieve.KeyTypeAutoincrement // 15
```

### Key Flags

```go
xtrieve.KeyFlagDuplicates  // 0x0001
xtrieve.KeyFlagModifiable  // 0x0002
xtrieve.KeyFlagBinary      // 0x0004
xtrieve.KeyFlagNullKey     // 0x0008
xtrieve.KeyFlagDescending  // 0x0020
```

### Lock Bias

```go
xtrieve.LockNone        // 0
xtrieve.LockSingleWait  // 100
xtrieve.LockSingleNoWait // 200
xtrieve.LockMultiWait   // 300
xtrieve.LockMultiNoWait // 400
```

## Error Handling

```go
resp, err := client.GetEqual(posBlock, key, 0)
if err != nil {
    // Connection error
    log.Printf("Connection error: %v", err)
    return
}

switch resp.StatusCode {
case xtrieve.StatusSuccess:
    // Process record
    processRecord(resp.DataBuffer)

case xtrieve.StatusKeyNotFound:
    fmt.Println("Record not found")

case xtrieve.StatusEndOfFile:
    fmt.Println("End of file")

case xtrieve.StatusRecordLocked:
    // Retry with backoff
    time.Sleep(100 * time.Millisecond)

default:
    log.Printf("Btrieve error: %d", resp.StatusCode)
}
```

## Thread Safety

The client uses a mutex for thread safety. Multiple goroutines can share a single client.

```go
var wg sync.WaitGroup

for i := 0; i < 10; i++ {
    wg.Add(1)
    go func(id int) {
        defer wg.Done()
        // Each goroutine can use the same client safely
        resp, _ := client.GetFirst(posBlock, 0)
        fmt.Printf("Goroutine %d: status=%d\n", id, resp.StatusCode)
    }(i)
}

wg.Wait()
```

## License

MIT
