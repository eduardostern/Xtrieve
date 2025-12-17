# Xtrieve Client for Node.js

TypeScript/JavaScript client for Xtrieve - a Btrieve 5.1 compatible ISAM database engine.

## Installation

```bash
npm install xtrieve-client
```

## Quick Start

```typescript
import { XtrieveClient, Operations, StatusCodes, KeyTypes, KeyFlags } from 'xtrieve-client';

const client = new XtrieveClient();

// Connect to server
await client.connect('127.0.0.1', 7419);

// Create a new file
await client.create('customers.dat', {
    recordLength: 100,
    pageSize: 4096,
    keys: [
        { position: 0, length: 8, flags: 0, type: KeyTypes.UNSIGNED_BINARY },
        { position: 8, length: 32, flags: KeyFlags.DUPLICATES, type: KeyTypes.STRING }
    ]
});

// Open file
let resp = await client.open('customers.dat');
let posBlock = resp.positionBlock;

// Insert a record
const record = Buffer.alloc(100);
record.writeBigInt64LE(1001n, 0);           // ID
record.write('John Doe', 8, 'utf8');         // Name
record.write('john@example.com', 40, 'utf8'); // Email

resp = await client.insert(posBlock, record);
posBlock = resp.positionBlock;

// Read by key
const key = Buffer.alloc(8);
key.writeBigInt64LE(1001n, 0);
resp = await client.getEqual(posBlock, key, 0);

if (resp.statusCode === StatusCodes.SUCCESS) {
    console.log('Found record:', resp.dataBuffer);
}

// Close file
await client.closeFile(posBlock);

// Disconnect
client.close();
```

## API Reference

### Constructor

```typescript
const client = new XtrieveClient();
```

### Connection Methods

#### `connect(host?: string, port?: number): Promise<void>`

Connect to an Xtrieve server.

```typescript
await client.connect('127.0.0.1', 7419);
```

#### `close(): void`

Close the connection.

#### `isConnected(): boolean`

Check if connected.

### File Operations

#### `open(filePath: string, mode?: number): Promise<BtrieveResponse>`

Open a file. Mode: -1 = normal, -2 = read-only, -3 = exclusive.

```typescript
const resp = await client.open('data.dat');
const posBlock = resp.positionBlock;
```

#### `closeFile(positionBlock: Buffer): Promise<BtrieveResponse>`

Close an open file.

#### `create(filePath: string, spec: FileSpec): Promise<BtrieveResponse>`

Create a new file with specified structure.

```typescript
await client.create('data.dat', {
    recordLength: 100,
    pageSize: 4096,
    keys: [
        { position: 0, length: 8, flags: 0, type: KeyTypes.UNSIGNED_BINARY }
    ]
});
```

### Record Operations

#### `insert(positionBlock: Buffer, data: Buffer): Promise<BtrieveResponse>`

Insert a new record.

#### `update(positionBlock: Buffer, data: Buffer, keyNumber: number): Promise<BtrieveResponse>`

Update the current record.

#### `delete(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse>`

Delete the current record.

### Key-Based Retrieval

#### `getEqual(positionBlock: Buffer, key: Buffer, keyNumber: number): Promise<BtrieveResponse>`

Get record by exact key match.

#### `getFirst(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse>`

Get first record in key order.

#### `getLast(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse>`

Get last record in key order.

#### `getNext(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse>`

Get next record in key order.

#### `getPrevious(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse>`

Get previous record in key order.

### Transactions

#### `beginTransaction(positionBlock: Buffer, lockMode?: number): Promise<BtrieveResponse>`

Begin a transaction.

```typescript
await client.beginTransaction(posBlock, LockBias.SINGLE_WAIT);
// ... operations ...
await client.endTransaction(posBlock);
```

#### `endTransaction(positionBlock: Buffer): Promise<BtrieveResponse>`

Commit the transaction.

#### `abortTransaction(positionBlock: Buffer): Promise<BtrieveResponse>`

Rollback the transaction.

### Iteration

#### `forEach(positionBlock: Buffer, keyNumber: number, callback): Promise<number>`

Iterate all records.

```typescript
const count = await client.forEach(posBlock, 0, (record, key) => {
    console.log('Record:', record);
});
console.log(`Processed ${count} records`);
```

### Low-Level

#### `execute(request: BtrieveRequest): Promise<BtrieveResponse>`

Execute any Btrieve operation.

```typescript
const resp = await client.execute({
    operation: Operations.GET_GREATER_OR_EQUAL,
    positionBlock: posBlock,
    keyBuffer: Buffer.from([0, 0, 0, 0, 0, 0, 0x10, 0x27]),  // 10000
    keyNumber: 0
});
```

## Constants

### Operations

```typescript
Operations.OPEN              // 0
Operations.CLOSE             // 1
Operations.INSERT            // 2
Operations.UPDATE            // 3
Operations.DELETE            // 4
Operations.GET_EQUAL         // 5
Operations.GET_NEXT          // 6
Operations.GET_PREVIOUS      // 7
Operations.GET_GREATER       // 8
Operations.GET_GREATER_OR_EQUAL // 9
Operations.GET_LESS          // 10
Operations.GET_LESS_OR_EQUAL // 11
Operations.GET_FIRST         // 12
Operations.GET_LAST          // 13
Operations.CREATE            // 14
Operations.STAT              // 15
Operations.BEGIN_TRANSACTION // 19
Operations.END_TRANSACTION   // 20
Operations.ABORT_TRANSACTION // 21
Operations.STEP_NEXT         // 24
Operations.UNLOCK            // 27
Operations.STEP_FIRST        // 33
Operations.STEP_LAST         // 34
Operations.STEP_PREVIOUS     // 35
```

### Status Codes

```typescript
StatusCodes.SUCCESS            // 0
StatusCodes.KEY_NOT_FOUND      // 4
StatusCodes.DUPLICATE_KEY      // 5
StatusCodes.INVALID_POSITIONING // 8
StatusCodes.END_OF_FILE        // 9
StatusCodes.FILE_NOT_FOUND     // 12
StatusCodes.RECORD_LOCKED      // 84
```

### Key Types

```typescript
KeyTypes.STRING          // 0
KeyTypes.INTEGER         // 1
KeyTypes.FLOAT           // 2
KeyTypes.UNSIGNED_BINARY // 14
KeyTypes.AUTOINCREMENT   // 15
```

### Key Flags

```typescript
KeyFlags.DUPLICATES  // 0x0001
KeyFlags.MODIFIABLE  // 0x0002
KeyFlags.BINARY      // 0x0004
KeyFlags.NULL_KEY    // 0x0008
KeyFlags.DESCENDING  // 0x0020
```

### Lock Bias

```typescript
LockBias.NO_LOCK        // 0
LockBias.SINGLE_WAIT    // 100
LockBias.SINGLE_NO_WAIT // 200
LockBias.MULTI_WAIT     // 300
LockBias.MULTI_NO_WAIT  // 400
```

## Error Handling

```typescript
try {
    const resp = await client.getEqual(posBlock, key, 0);

    switch (resp.statusCode) {
        case StatusCodes.SUCCESS:
            processRecord(resp.dataBuffer);
            break;
        case StatusCodes.KEY_NOT_FOUND:
            console.log('Record not found');
            break;
        case StatusCodes.RECORD_LOCKED:
            // Retry with backoff
            await sleep(100);
            break;
        default:
            throw new Error(`Btrieve error: ${resp.statusCode}`);
    }
} catch (err) {
    console.error('Connection error:', err);
}
```

## License

MIT
