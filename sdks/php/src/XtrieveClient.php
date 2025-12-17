<?php
/**
 * Xtrieve Client for PHP
 *
 * Btrieve 5.1 compatible ISAM database client using binary TCP protocol.
 *
 * @package Xtrieve
 */

namespace Xtrieve;

/**
 * Operation codes
 */
class Operations
{
    public const OPEN = 0;
    public const CLOSE = 1;
    public const INSERT = 2;
    public const UPDATE = 3;
    public const DELETE = 4;
    public const GET_EQUAL = 5;
    public const GET_NEXT = 6;
    public const GET_PREVIOUS = 7;
    public const GET_GREATER = 8;
    public const GET_GREATER_OR_EQUAL = 9;
    public const GET_LESS = 10;
    public const GET_LESS_OR_EQUAL = 11;
    public const GET_FIRST = 12;
    public const GET_LAST = 13;
    public const CREATE = 14;
    public const STAT = 15;
    public const BEGIN_TRANSACTION = 19;
    public const END_TRANSACTION = 20;
    public const ABORT_TRANSACTION = 21;
    public const STEP_NEXT = 24;
    public const UNLOCK = 27;
    public const STEP_FIRST = 33;
    public const STEP_LAST = 34;
    public const STEP_PREVIOUS = 35;
}

/**
 * Status codes
 */
class StatusCodes
{
    public const SUCCESS = 0;
    public const INVALID_OPERATION = 1;
    public const IO_ERROR = 2;
    public const FILE_NOT_OPEN = 3;
    public const KEY_NOT_FOUND = 4;
    public const DUPLICATE_KEY = 5;
    public const INVALID_KEY_NUMBER = 6;
    public const DIFFERENT_KEY_NUMBER = 7;
    public const INVALID_POSITIONING = 8;
    public const END_OF_FILE = 9;
    public const FILE_NOT_FOUND = 12;
    public const DISK_FULL = 18;
    public const DATA_BUFFER_TOO_SHORT = 22;
    public const RECORD_LOCKED = 84;
    public const FILE_LOCKED = 85;
}

/**
 * Lock bias values
 */
class LockBias
{
    public const NONE = 0;
    public const SINGLE_WAIT = 100;
    public const SINGLE_NO_WAIT = 200;
    public const MULTI_WAIT = 300;
    public const MULTI_NO_WAIT = 400;
}

/**
 * Key types for file creation
 */
class KeyTypes
{
    public const STRING = 0;
    public const INTEGER = 1;
    public const FLOAT = 2;
    public const DATE = 3;
    public const TIME = 4;
    public const DECIMAL = 5;
    public const MONEY = 6;
    public const LOGICAL = 7;
    public const NUMERIC = 8;
    public const BFLOAT = 9;
    public const LSTRING = 10;
    public const ZSTRING = 11;
    public const UNSIGNED_BINARY = 14;
    public const AUTOINCREMENT = 15;
}

/**
 * Key flags for file creation
 */
class KeyFlags
{
    public const DUPLICATES = 0x0001;
    public const MODIFIABLE = 0x0002;
    public const BINARY = 0x0004;
    public const NULL_KEY = 0x0008;
    public const SEGMENTED = 0x0010;
    public const DESCENDING = 0x0020;
    public const SUPPLEMENTAL = 0x0040;
    public const EXTENDED_TYPE = 0x0080;
}

/**
 * Request structure
 */
class BtrieveRequest
{
    public int $operation = 0;
    public string $positionBlock = '';
    public string $dataBuffer = '';
    public string $keyBuffer = '';
    public int $keyNumber = 0;
    public string $filePath = '';
    public int $lockBias = 0;
}

/**
 * Response structure
 */
class BtrieveResponse
{
    public int $statusCode = 0;
    public string $positionBlock = '';
    public string $dataBuffer = '';
    public string $keyBuffer = '';
}

/**
 * Key specification for file creation
 */
class KeySpec
{
    public int $position = 0;
    public int $length = 0;
    public int $flags = 0;
    public int $type = 0;
    public int $nullValue = 0;
}

/**
 * File specification for file creation
 */
class FileSpec
{
    public int $recordLength = 0;
    public int $pageSize = 4096;
    /** @var KeySpec[] */
    public array $keys = [];
}

/**
 * Xtrieve exception
 */
class XtrieveException extends \Exception
{
}

/**
 * Xtrieve Client
 */
class XtrieveClient
{
    private const POSITION_BLOCK_SIZE = 128;
    private const DEFAULT_PORT = 7419;

    private $socket = null;
    private bool $connected = false;

    /**
     * Connect to Xtrieve server
     *
     * @param string $host Server hostname
     * @param int $port Server port
     * @throws XtrieveException
     */
    public function connect(string $host = '127.0.0.1', int $port = self::DEFAULT_PORT): void
    {
        $this->socket = @socket_create(AF_INET, SOCK_STREAM, SOL_TCP);
        if ($this->socket === false) {
            throw new XtrieveException('Failed to create socket: ' . socket_strerror(socket_last_error()));
        }

        if (@socket_connect($this->socket, $host, $port) === false) {
            $error = socket_strerror(socket_last_error($this->socket));
            socket_close($this->socket);
            $this->socket = null;
            throw new XtrieveException("Failed to connect to $host:$port - $error");
        }

        $this->connected = true;
    }

    /**
     * Disconnect from server
     */
    public function close(): void
    {
        if ($this->socket !== null) {
            socket_close($this->socket);
            $this->socket = null;
        }
        $this->connected = false;
    }

    /**
     * Check if connected
     */
    public function isConnected(): bool
    {
        return $this->connected;
    }

    /**
     * Execute a Btrieve operation
     *
     * @param BtrieveRequest $request
     * @return BtrieveResponse
     * @throws XtrieveException
     */
    public function execute(BtrieveRequest $request): BtrieveResponse
    {
        if (!$this->connected) {
            throw new XtrieveException('Not connected');
        }

        $packet = $this->buildRequest($request);
        $this->sendAll($packet);

        return $this->readResponse();
    }

    /**
     * Build file specification buffer for Create operation
     *
     * @param FileSpec $spec
     * @return string
     */
    public static function buildFileSpec(FileSpec $spec): string
    {
        $buf = '';

        // Header
        $buf .= pack('v', $spec->recordLength);  // record length
        $buf .= pack('v', $spec->pageSize);       // page size
        $buf .= pack('v', count($spec->keys));    // num keys
        $buf .= pack('V', 0);                      // reserved

        // Key specs
        foreach ($spec->keys as $key) {
            $buf .= pack('v', $key->position);
            $buf .= pack('v', $key->length);
            $buf .= pack('v', $key->flags);
            $buf .= chr($key->type);
            $buf .= chr($key->nullValue);
            $buf .= str_repeat("\0", 8);  // reserved
        }

        return $buf;
    }

    // ========== Convenience Methods ==========

    /**
     * Open a file
     *
     * @param string $filePath
     * @param int $mode -1=normal, -2=read-only, -3=exclusive
     * @return BtrieveResponse
     */
    public function open(string $filePath, int $mode = -1): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::OPEN;
        $req->filePath = $filePath;
        $req->keyNumber = $mode;
        return $this->execute($req);
    }

    /**
     * Close a file
     *
     * @param string $positionBlock
     * @return BtrieveResponse
     */
    public function closeFile(string $positionBlock): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::CLOSE;
        $req->positionBlock = $positionBlock;
        return $this->execute($req);
    }

    /**
     * Create a new file
     *
     * @param string $filePath
     * @param FileSpec $spec
     * @return BtrieveResponse
     */
    public function create(string $filePath, FileSpec $spec): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::CREATE;
        $req->filePath = $filePath;
        $req->dataBuffer = self::buildFileSpec($spec);
        return $this->execute($req);
    }

    /**
     * Insert a record
     *
     * @param string $positionBlock
     * @param string $data
     * @return BtrieveResponse
     */
    public function insert(string $positionBlock, string $data): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::INSERT;
        $req->positionBlock = $positionBlock;
        $req->dataBuffer = $data;
        return $this->execute($req);
    }

    /**
     * Update current record
     *
     * @param string $positionBlock
     * @param string $data
     * @param int $keyNumber
     * @return BtrieveResponse
     */
    public function update(string $positionBlock, string $data, int $keyNumber): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::UPDATE;
        $req->positionBlock = $positionBlock;
        $req->dataBuffer = $data;
        $req->keyNumber = $keyNumber;
        return $this->execute($req);
    }

    /**
     * Delete current record
     *
     * @param string $positionBlock
     * @param int $keyNumber
     * @return BtrieveResponse
     */
    public function delete(string $positionBlock, int $keyNumber): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::DELETE;
        $req->positionBlock = $positionBlock;
        $req->keyNumber = $keyNumber;
        return $this->execute($req);
    }

    /**
     * Get record by exact key match
     *
     * @param string $positionBlock
     * @param string $key
     * @param int $keyNumber
     * @return BtrieveResponse
     */
    public function getEqual(string $positionBlock, string $key, int $keyNumber): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::GET_EQUAL;
        $req->positionBlock = $positionBlock;
        $req->keyBuffer = $key;
        $req->keyNumber = $keyNumber;
        return $this->execute($req);
    }

    /**
     * Get first record in key order
     *
     * @param string $positionBlock
     * @param int $keyNumber
     * @return BtrieveResponse
     */
    public function getFirst(string $positionBlock, int $keyNumber): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::GET_FIRST;
        $req->positionBlock = $positionBlock;
        $req->keyNumber = $keyNumber;
        return $this->execute($req);
    }

    /**
     * Get last record in key order
     *
     * @param string $positionBlock
     * @param int $keyNumber
     * @return BtrieveResponse
     */
    public function getLast(string $positionBlock, int $keyNumber): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::GET_LAST;
        $req->positionBlock = $positionBlock;
        $req->keyNumber = $keyNumber;
        return $this->execute($req);
    }

    /**
     * Get next record in key order
     *
     * @param string $positionBlock
     * @param int $keyNumber
     * @return BtrieveResponse
     */
    public function getNext(string $positionBlock, int $keyNumber): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::GET_NEXT;
        $req->positionBlock = $positionBlock;
        $req->keyNumber = $keyNumber;
        return $this->execute($req);
    }

    /**
     * Get previous record in key order
     *
     * @param string $positionBlock
     * @param int $keyNumber
     * @return BtrieveResponse
     */
    public function getPrevious(string $positionBlock, int $keyNumber): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::GET_PREVIOUS;
        $req->positionBlock = $positionBlock;
        $req->keyNumber = $keyNumber;
        return $this->execute($req);
    }

    /**
     * Begin a transaction
     *
     * @param string $positionBlock
     * @param int $lockMode
     * @return BtrieveResponse
     */
    public function beginTransaction(string $positionBlock, int $lockMode = LockBias::SINGLE_WAIT): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::BEGIN_TRANSACTION;
        $req->positionBlock = $positionBlock;
        $req->lockBias = $lockMode;
        return $this->execute($req);
    }

    /**
     * Commit a transaction
     *
     * @param string $positionBlock
     * @return BtrieveResponse
     */
    public function endTransaction(string $positionBlock): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::END_TRANSACTION;
        $req->positionBlock = $positionBlock;
        return $this->execute($req);
    }

    /**
     * Abort a transaction
     *
     * @param string $positionBlock
     * @return BtrieveResponse
     */
    public function abortTransaction(string $positionBlock): BtrieveResponse
    {
        $req = new BtrieveRequest();
        $req->operation = Operations::ABORT_TRANSACTION;
        $req->positionBlock = $positionBlock;
        return $this->execute($req);
    }

    /**
     * Iterate all records
     *
     * @param string $positionBlock
     * @param int $keyNumber
     * @param callable $callback function(string $record, string $key): void
     * @return int Number of records processed
     */
    public function forEach(string $positionBlock, int $keyNumber, callable $callback): int
    {
        $resp = $this->getFirst($positionBlock, $keyNumber);
        $count = 0;

        while ($resp->statusCode === StatusCodes::SUCCESS) {
            $callback($resp->dataBuffer, $resp->keyBuffer);
            $count++;
            $resp = $this->getNext($resp->positionBlock, $keyNumber);
        }

        return $count;
    }

    // ========== Private Methods ==========

    private function buildRequest(BtrieveRequest $request): string
    {
        $posBlock = str_pad($request->positionBlock, self::POSITION_BLOCK_SIZE, "\0");
        $posBlock = substr($posBlock, 0, self::POSITION_BLOCK_SIZE);

        $buf = '';

        // Operation (2 bytes)
        $buf .= pack('v', $request->operation);

        // Position block (128 bytes)
        $buf .= $posBlock;

        // Data buffer length + data
        $buf .= pack('V', strlen($request->dataBuffer));
        $buf .= $request->dataBuffer;

        // Key buffer length + key
        $buf .= pack('v', strlen($request->keyBuffer));
        $buf .= $request->keyBuffer;

        // Key number (2 bytes, signed)
        $buf .= pack('v', $request->keyNumber & 0xFFFF);

        // File path length + path
        $buf .= pack('v', strlen($request->filePath));
        $buf .= $request->filePath;

        // Lock bias
        $buf .= pack('v', $request->lockBias);

        return $buf;
    }

    private function sendAll(string $data): void
    {
        $len = strlen($data);
        $sent = 0;

        while ($sent < $len) {
            $result = @socket_write($this->socket, substr($data, $sent));
            if ($result === false) {
                $this->connected = false;
                throw new XtrieveException('Send failed: ' . socket_strerror(socket_last_error($this->socket)));
            }
            $sent += $result;
        }
    }

    private function recvAll(int $length): string
    {
        $data = '';
        $received = 0;

        while ($received < $length) {
            $chunk = @socket_read($this->socket, $length - $received);
            if ($chunk === false || $chunk === '') {
                $this->connected = false;
                throw new XtrieveException('Receive failed');
            }
            $data .= $chunk;
            $received = strlen($data);
        }

        return $data;
    }

    private function readResponse(): BtrieveResponse
    {
        $response = new BtrieveResponse();

        // Read header: status(2) + position_block(128) + data_len(4)
        $header = $this->recvAll(2 + self::POSITION_BLOCK_SIZE + 4);

        $response->statusCode = unpack('v', substr($header, 0, 2))[1];
        $response->positionBlock = substr($header, 2, self::POSITION_BLOCK_SIZE);
        $dataLen = unpack('V', substr($header, 2 + self::POSITION_BLOCK_SIZE, 4))[1];

        // Read data buffer
        if ($dataLen > 0) {
            $response->dataBuffer = $this->recvAll($dataLen);
        }

        // Read key length
        $keyLenBuf = $this->recvAll(2);
        $keyLen = unpack('v', $keyLenBuf)[1];

        // Read key buffer
        if ($keyLen > 0) {
            $response->keyBuffer = $this->recvAll($keyLen);
        }

        return $response;
    }

    public function __destruct()
    {
        $this->close();
    }
}
