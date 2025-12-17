/**
 * Xtrieve Client for Node.js
 *
 * Btrieve 5.1 compatible ISAM database client using binary TCP protocol.
 */

import * as net from 'net';

// Constants
export const POSITION_BLOCK_SIZE = 128;
export const DEFAULT_PORT = 7419;

// Operation codes
export const Operations = {
    OPEN: 0,
    CLOSE: 1,
    INSERT: 2,
    UPDATE: 3,
    DELETE: 4,
    GET_EQUAL: 5,
    GET_NEXT: 6,
    GET_PREVIOUS: 7,
    GET_GREATER: 8,
    GET_GREATER_OR_EQUAL: 9,
    GET_LESS: 10,
    GET_LESS_OR_EQUAL: 11,
    GET_FIRST: 12,
    GET_LAST: 13,
    CREATE: 14,
    STAT: 15,
    BEGIN_TRANSACTION: 19,
    END_TRANSACTION: 20,
    ABORT_TRANSACTION: 21,
    STEP_NEXT: 24,
    UNLOCK: 27,
    STEP_FIRST: 33,
    STEP_LAST: 34,
    STEP_PREVIOUS: 35,
} as const;

// Status codes
export const StatusCodes = {
    SUCCESS: 0,
    INVALID_OPERATION: 1,
    IO_ERROR: 2,
    FILE_NOT_OPEN: 3,
    KEY_NOT_FOUND: 4,
    DUPLICATE_KEY: 5,
    INVALID_KEY_NUMBER: 6,
    DIFFERENT_KEY_NUMBER: 7,
    INVALID_POSITIONING: 8,
    END_OF_FILE: 9,
    FILE_NOT_FOUND: 12,
    DISK_FULL: 18,
    DATA_BUFFER_TOO_SHORT: 22,
    RECORD_LOCKED: 84,
    FILE_LOCKED: 85,
} as const;

// Lock biases
export const LockBias = {
    NO_LOCK: 0,
    SINGLE_WAIT: 100,
    SINGLE_NO_WAIT: 200,
    MULTI_WAIT: 300,
    MULTI_NO_WAIT: 400,
} as const;

// Key types for Create operation
export const KeyTypes = {
    STRING: 0,
    INTEGER: 1,
    FLOAT: 2,
    DATE: 3,
    TIME: 4,
    DECIMAL: 5,
    MONEY: 6,
    LOGICAL: 7,
    NUMERIC: 8,
    BFLOAT: 9,
    LSTRING: 10,
    ZSTRING: 11,
    UNSIGNED_BINARY: 14,
    AUTOINCREMENT: 15,
} as const;

// Key flags for Create operation
export const KeyFlags = {
    DUPLICATES: 0x0001,
    MODIFIABLE: 0x0002,
    BINARY: 0x0004,
    NULL_KEY: 0x0008,
    SEGMENTED: 0x0010,
    DESCENDING: 0x0020,
    SUPPLEMENTAL: 0x0040,
    EXTENDED_TYPE: 0x0080,
} as const;

/**
 * Request structure for Xtrieve operations
 */
export interface BtrieveRequest {
    operation: number;
    positionBlock?: Buffer;
    dataBuffer?: Buffer;
    keyBuffer?: Buffer;
    keyNumber?: number;
    filePath?: string;
    lockBias?: number;
}

/**
 * Response structure from Xtrieve operations
 */
export interface BtrieveResponse {
    statusCode: number;
    positionBlock: Buffer;
    dataBuffer: Buffer;
    keyBuffer: Buffer;
}

/**
 * Key specification for file creation
 */
export interface KeySpec {
    position: number;
    length: number;
    flags: number;
    type: number;
    nullValue?: number;
}

/**
 * File specification for creation
 */
export interface FileSpec {
    recordLength: number;
    pageSize: number;
    keys: KeySpec[];
}

/**
 * Xtrieve client for Node.js
 */
export class XtrieveClient {
    private socket: net.Socket | null = null;
    private connected = false;
    private receiveBuffer = Buffer.alloc(0);
    private pendingResolve: ((response: BtrieveResponse) => void) | null = null;
    private pendingReject: ((error: Error) => void) | null = null;

    /**
     * Connect to Xtrieve server
     */
    async connect(host: string = '127.0.0.1', port: number = DEFAULT_PORT): Promise<void> {
        return new Promise((resolve, reject) => {
            this.socket = new net.Socket();

            this.socket.on('connect', () => {
                this.connected = true;
                resolve();
            });

            this.socket.on('error', (err) => {
                if (this.pendingReject) {
                    this.pendingReject(err);
                    this.pendingReject = null;
                    this.pendingResolve = null;
                }
                reject(err);
            });

            this.socket.on('close', () => {
                this.connected = false;
                if (this.pendingReject) {
                    this.pendingReject(new Error('Connection closed'));
                    this.pendingReject = null;
                    this.pendingResolve = null;
                }
            });

            this.socket.on('data', (data) => {
                this.receiveBuffer = Buffer.concat([this.receiveBuffer, data]);
                this.tryParseResponse();
            });

            this.socket.connect(port, host);
        });
    }

    /**
     * Close connection
     */
    close(): void {
        if (this.socket) {
            this.socket.destroy();
            this.socket = null;
            this.connected = false;
        }
    }

    /**
     * Check if connected
     */
    isConnected(): boolean {
        return this.connected;
    }

    /**
     * Execute a Btrieve operation
     */
    async execute(request: BtrieveRequest): Promise<BtrieveResponse> {
        if (!this.socket || !this.connected) {
            throw new Error('Not connected');
        }

        const packet = this.buildRequest(request);

        return new Promise((resolve, reject) => {
            this.pendingResolve = resolve;
            this.pendingReject = reject;
            this.socket!.write(packet);
        });
    }

    /**
     * Build file specification buffer for Create operation
     */
    static buildFileSpec(spec: FileSpec): Buffer {
        const headerSize = 10;
        const keySpecSize = 16;
        const totalSize = headerSize + spec.keys.length * keySpecSize;
        const buf = Buffer.alloc(totalSize);

        // Header
        buf.writeUInt16LE(spec.recordLength, 0);
        buf.writeUInt16LE(spec.pageSize, 2);
        buf.writeUInt16LE(spec.keys.length, 4);
        buf.writeUInt32LE(0, 6);  // Reserved

        // Key specs
        let offset = headerSize;
        for (const key of spec.keys) {
            buf.writeUInt16LE(key.position, offset);
            buf.writeUInt16LE(key.length, offset + 2);
            buf.writeUInt16LE(key.flags, offset + 4);
            buf.writeUInt8(key.type, offset + 6);
            buf.writeUInt8(key.nullValue ?? 0, offset + 7);
            // Reserved 8 bytes are already zero
            offset += keySpecSize;
        }

        return buf;
    }

    // ========== Convenience Methods ==========

    /**
     * Open a file
     */
    async open(filePath: string, mode: number = -1): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.OPEN,
            filePath,
            keyNumber: mode,
        });
    }

    /**
     * Close a file
     */
    async closeFile(positionBlock: Buffer): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.CLOSE,
            positionBlock,
        });
    }

    /**
     * Create a new file
     */
    async create(filePath: string, spec: FileSpec): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.CREATE,
            filePath,
            dataBuffer: XtrieveClient.buildFileSpec(spec),
        });
    }

    /**
     * Insert a record
     */
    async insert(positionBlock: Buffer, data: Buffer): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.INSERT,
            positionBlock,
            dataBuffer: data,
        });
    }

    /**
     * Update current record
     */
    async update(positionBlock: Buffer, data: Buffer, keyNumber: number): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.UPDATE,
            positionBlock,
            dataBuffer: data,
            keyNumber,
        });
    }

    /**
     * Delete current record
     */
    async delete(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.DELETE,
            positionBlock,
            keyNumber,
        });
    }

    /**
     * Get record by exact key match
     */
    async getEqual(positionBlock: Buffer, key: Buffer, keyNumber: number): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.GET_EQUAL,
            positionBlock,
            keyBuffer: key,
            keyNumber,
        });
    }

    /**
     * Get first record in key order
     */
    async getFirst(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.GET_FIRST,
            positionBlock,
            keyNumber,
        });
    }

    /**
     * Get last record in key order
     */
    async getLast(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.GET_LAST,
            positionBlock,
            keyNumber,
        });
    }

    /**
     * Get next record in key order
     */
    async getNext(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.GET_NEXT,
            positionBlock,
            keyNumber,
        });
    }

    /**
     * Get previous record in key order
     */
    async getPrevious(positionBlock: Buffer, keyNumber: number): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.GET_PREVIOUS,
            positionBlock,
            keyNumber,
        });
    }

    /**
     * Begin a transaction
     */
    async beginTransaction(positionBlock: Buffer, lockMode: number = LockBias.SINGLE_WAIT): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.BEGIN_TRANSACTION,
            positionBlock,
            lockBias: lockMode,
        });
    }

    /**
     * Commit a transaction
     */
    async endTransaction(positionBlock: Buffer): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.END_TRANSACTION,
            positionBlock,
        });
    }

    /**
     * Abort a transaction
     */
    async abortTransaction(positionBlock: Buffer): Promise<BtrieveResponse> {
        return this.execute({
            operation: Operations.ABORT_TRANSACTION,
            positionBlock,
        });
    }

    /**
     * Iterate all records using callback
     */
    async forEach(
        positionBlock: Buffer,
        keyNumber: number,
        callback: (record: Buffer, key: Buffer) => void | Promise<void>
    ): Promise<number> {
        let resp = await this.getFirst(positionBlock, keyNumber);
        let count = 0;

        while (resp.statusCode === StatusCodes.SUCCESS) {
            await callback(resp.dataBuffer, resp.keyBuffer);
            count++;
            resp = await this.getNext(resp.positionBlock, keyNumber);
        }

        return count;
    }

    // ========== Private Methods ==========

    private buildRequest(request: BtrieveRequest): Buffer {
        const positionBlock = request.positionBlock ?? Buffer.alloc(POSITION_BLOCK_SIZE);
        const dataBuffer = request.dataBuffer ?? Buffer.alloc(0);
        const keyBuffer = request.keyBuffer ?? Buffer.alloc(0);
        const filePath = request.filePath ?? '';
        const filePathBuf = Buffer.from(filePath, 'utf8');

        const totalSize = 2 + POSITION_BLOCK_SIZE + 4 + dataBuffer.length +
                         2 + keyBuffer.length + 2 + 2 + filePathBuf.length + 2;

        const buf = Buffer.alloc(totalSize);
        let offset = 0;

        // Operation code (2 bytes)
        buf.writeUInt16LE(request.operation, offset);
        offset += 2;

        // Position block (128 bytes)
        positionBlock.copy(buf, offset, 0, Math.min(positionBlock.length, POSITION_BLOCK_SIZE));
        offset += POSITION_BLOCK_SIZE;

        // Data buffer length + data
        buf.writeUInt32LE(dataBuffer.length, offset);
        offset += 4;
        dataBuffer.copy(buf, offset);
        offset += dataBuffer.length;

        // Key buffer length + key
        buf.writeUInt16LE(keyBuffer.length, offset);
        offset += 2;
        keyBuffer.copy(buf, offset);
        offset += keyBuffer.length;

        // Key number (2 bytes, signed)
        buf.writeInt16LE(request.keyNumber ?? 0, offset);
        offset += 2;

        // File path length + path
        buf.writeUInt16LE(filePathBuf.length, offset);
        offset += 2;
        filePathBuf.copy(buf, offset);
        offset += filePathBuf.length;

        // Lock bias (2 bytes)
        buf.writeUInt16LE(request.lockBias ?? 0, offset);

        return buf;
    }

    private tryParseResponse(): void {
        // Minimum response size: 2 + 128 + 4 + 2 = 136 bytes
        if (this.receiveBuffer.length < 136) {
            return;
        }

        // Read status and position block
        const statusCode = this.receiveBuffer.readUInt16LE(0);
        const positionBlock = Buffer.alloc(POSITION_BLOCK_SIZE);
        this.receiveBuffer.copy(positionBlock, 0, 2, 2 + POSITION_BLOCK_SIZE);

        // Read data length
        const dataLength = this.receiveBuffer.readUInt32LE(2 + POSITION_BLOCK_SIZE);
        const dataStart = 2 + POSITION_BLOCK_SIZE + 4;

        if (this.receiveBuffer.length < dataStart + dataLength + 2) {
            return;  // Wait for more data
        }

        // Read data buffer
        const dataBuffer = Buffer.alloc(dataLength);
        this.receiveBuffer.copy(dataBuffer, 0, dataStart, dataStart + dataLength);

        // Read key length
        const keyLengthOffset = dataStart + dataLength;
        const keyLength = this.receiveBuffer.readUInt16LE(keyLengthOffset);

        const totalSize = keyLengthOffset + 2 + keyLength;
        if (this.receiveBuffer.length < totalSize) {
            return;  // Wait for more data
        }

        // Read key buffer
        const keyBuffer = Buffer.alloc(keyLength);
        this.receiveBuffer.copy(keyBuffer, 0, keyLengthOffset + 2, keyLengthOffset + 2 + keyLength);

        // Remove parsed data from buffer
        this.receiveBuffer = this.receiveBuffer.subarray(totalSize);

        // Resolve pending promise
        if (this.pendingResolve) {
            const resolve = this.pendingResolve;
            this.pendingResolve = null;
            this.pendingReject = null;
            resolve({
                statusCode,
                positionBlock,
                dataBuffer,
                keyBuffer,
            });
        }
    }
}

export default XtrieveClient;
