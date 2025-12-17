# Xtrieve Client for PHP

PHP client library for Xtrieve - a Btrieve 5.1 compatible ISAM database engine.

## Requirements

- PHP 8.0+
- ext-sockets

## Installation

```bash
composer require xtrieve/client
```

Or add to composer.json:

```json
{
    "require": {
        "xtrieve/client": "^1.0"
    }
}
```

## Quick Start

```php
<?php
require 'vendor/autoload.php';

use Xtrieve\XtrieveClient;
use Xtrieve\FileSpec;
use Xtrieve\KeySpec;
use Xtrieve\KeyTypes;
use Xtrieve\StatusCodes;

// Connect
$client = new XtrieveClient();
$client->connect('127.0.0.1', 7419);

// Create a file
$spec = new FileSpec();
$spec->recordLength = 100;
$spec->pageSize = 4096;

$key = new KeySpec();
$key->position = 0;
$key->length = 8;
$key->type = KeyTypes::UNSIGNED_BINARY;
$spec->keys = [$key];

$client->create('customers.dat', $spec);

// Open file
$resp = $client->open('customers.dat');
$posBlock = $resp->positionBlock;

// Insert a record
$record = pack('P', 1001);  // ID (8 bytes, little-endian)
$record .= str_pad('John Doe', 32, "\0");  // Name
$record .= str_pad('john@example.com', 60, "\0");  // Email

$resp = $client->insert($posBlock, $record);
$posBlock = $resp->positionBlock;

// Read by key
$key = pack('P', 1001);
$resp = $client->getEqual($posBlock, $key, 0);

if ($resp->statusCode === StatusCodes::SUCCESS) {
    $id = unpack('P', substr($resp->dataBuffer, 0, 8))[1];
    $name = rtrim(substr($resp->dataBuffer, 8, 32), "\0");
    echo "Found: ID=$id, Name=$name\n";
}

// Close file
$client->closeFile($posBlock);

// Disconnect
$client->close();
```

## API Reference

### Connection

```php
// Connect to server
$client = new XtrieveClient();
$client->connect('127.0.0.1', 7419);

// Check connection
$client->isConnected();

// Disconnect
$client->close();
```

### File Operations

```php
// Open file
$resp = $client->open('data.dat', -1);  // -1=normal, -2=read-only, -3=exclusive
$posBlock = $resp->positionBlock;

// Close file
$client->closeFile($posBlock);

// Create file
$spec = new FileSpec();
$spec->recordLength = 100;
$spec->pageSize = 4096;
$spec->keys = [/* KeySpec objects */];
$client->create('data.dat', $spec);
```

### Record Operations

```php
// Insert
$resp = $client->insert($posBlock, $recordData);
$posBlock = $resp->positionBlock;

// Update current record
$resp = $client->update($posBlock, $newData, $keyNumber);

// Delete current record
$resp = $client->delete($posBlock, $keyNumber);
```

### Key-Based Retrieval

```php
// Get by exact key match
$resp = $client->getEqual($posBlock, $keyValue, $keyNumber);

// Get first record
$resp = $client->getFirst($posBlock, $keyNumber);

// Get last record
$resp = $client->getLast($posBlock, $keyNumber);

// Get next record
$resp = $client->getNext($posBlock, $keyNumber);

// Get previous record
$resp = $client->getPrevious($posBlock, $keyNumber);
```

### Transactions

```php
use Xtrieve\LockBias;

// Begin transaction
$client->beginTransaction($posBlock, LockBias::SINGLE_WAIT);

// Commit
$client->endTransaction($posBlock);

// Rollback
$client->abortTransaction($posBlock);
```

### Iteration

```php
// Iterate all records
$count = $client->forEach($posBlock, 0, function($record, $key) {
    $id = unpack('P', substr($record, 0, 8))[1];
    echo "Record ID: $id\n";
});
echo "Processed $count records\n";
```

## Constants

### Operations

```php
use Xtrieve\Operations;

Operations::OPEN              // 0
Operations::CLOSE             // 1
Operations::INSERT            // 2
Operations::UPDATE            // 3
Operations::DELETE            // 4
Operations::GET_EQUAL         // 5
Operations::GET_NEXT          // 6
Operations::GET_PREVIOUS      // 7
Operations::GET_FIRST         // 12
Operations::GET_LAST          // 13
Operations::CREATE            // 14
Operations::BEGIN_TRANSACTION // 19
Operations::END_TRANSACTION   // 20
Operations::ABORT_TRANSACTION // 21
```

### Status Codes

```php
use Xtrieve\StatusCodes;

StatusCodes::SUCCESS            // 0
StatusCodes::KEY_NOT_FOUND      // 4
StatusCodes::DUPLICATE_KEY      // 5
StatusCodes::INVALID_POSITIONING // 8
StatusCodes::END_OF_FILE        // 9
StatusCodes::FILE_NOT_FOUND     // 12
StatusCodes::RECORD_LOCKED      // 84
```

### Key Types

```php
use Xtrieve\KeyTypes;

KeyTypes::STRING          // 0
KeyTypes::INTEGER         // 1
KeyTypes::FLOAT           // 2
KeyTypes::UNSIGNED_BINARY // 14
KeyTypes::AUTOINCREMENT   // 15
```

### Key Flags

```php
use Xtrieve\KeyFlags;

KeyFlags::DUPLICATES  // 0x0001
KeyFlags::MODIFIABLE  // 0x0002
KeyFlags::BINARY      // 0x0004
KeyFlags::NULL_KEY    // 0x0008
KeyFlags::DESCENDING  // 0x0020
```

## Error Handling

```php
use Xtrieve\XtrieveException;
use Xtrieve\StatusCodes;

try {
    $resp = $client->getEqual($posBlock, $key, 0);

    switch ($resp->statusCode) {
        case StatusCodes::SUCCESS:
            // Process record
            break;
        case StatusCodes::KEY_NOT_FOUND:
            echo "Record not found\n";
            break;
        case StatusCodes::RECORD_LOCKED:
            // Retry
            break;
        default:
            throw new Exception("Btrieve error: " . $resp->statusCode);
    }
} catch (XtrieveException $e) {
    echo "Connection error: " . $e->getMessage() . "\n";
}
```

## Binary Data Helpers

```php
// Pack 64-bit integer (little-endian)
$key = pack('P', 12345);

// Unpack 64-bit integer
$id = unpack('P', substr($data, 0, 8))[1];

// Pack 32-bit integer
$val = pack('V', 12345);

// Unpack 32-bit integer
$val = unpack('V', substr($data, 0, 4))[1];

// Pack float
$f = pack('e', 3.14);  // Little-endian float

// Pack double
$d = pack('E', 3.14);  // Little-endian double
```

## License

MIT
