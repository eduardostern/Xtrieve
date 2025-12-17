// Package xtrieve provides a client for Xtrieve database - a Btrieve 5.1 compatible ISAM engine.
package xtrieve

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"sync"
)

// Constants
const (
	PositionBlockSize = 128
	DefaultPort       = 7419
)

// Operation codes
const (
	OpOpen             = 0
	OpClose            = 1
	OpInsert           = 2
	OpUpdate           = 3
	OpDelete           = 4
	OpGetEqual         = 5
	OpGetNext          = 6
	OpGetPrevious      = 7
	OpGetGreater       = 8
	OpGetGreaterOrEqual = 9
	OpGetLess          = 10
	OpGetLessOrEqual   = 11
	OpGetFirst         = 12
	OpGetLast          = 13
	OpCreate           = 14
	OpStat             = 15
	OpBeginTransaction = 19
	OpEndTransaction   = 20
	OpAbortTransaction = 21
	OpStepNext         = 24
	OpUnlock           = 27
	OpStepFirst        = 33
	OpStepLast         = 34
	OpStepPrevious     = 35
)

// Status codes
const (
	StatusSuccess           = 0
	StatusInvalidOperation  = 1
	StatusIOError           = 2
	StatusFileNotOpen       = 3
	StatusKeyNotFound       = 4
	StatusDuplicateKey      = 5
	StatusInvalidKeyNumber  = 6
	StatusDifferentKeyNumber = 7
	StatusInvalidPositioning = 8
	StatusEndOfFile         = 9
	StatusFileNotFound      = 12
	StatusDiskFull          = 18
	StatusDataBufferTooShort = 22
	StatusRecordLocked      = 84
	StatusFileLocked        = 85
)

// Lock bias values
const (
	LockNone        = 0
	LockSingleWait  = 100
	LockSingleNoWait = 200
	LockMultiWait   = 300
	LockMultiNoWait = 400
)

// Key types
const (
	KeyTypeString        = 0
	KeyTypeInteger       = 1
	KeyTypeFloat         = 2
	KeyTypeDate          = 3
	KeyTypeTime          = 4
	KeyTypeDecimal       = 5
	KeyTypeMoney         = 6
	KeyTypeLogical       = 7
	KeyTypeNumeric       = 8
	KeyTypeBfloat        = 9
	KeyTypeLstring       = 10
	KeyTypeZstring       = 11
	KeyTypeUnsignedBinary = 14
	KeyTypeAutoincrement = 15
)

// Key flags
const (
	KeyFlagDuplicates   = 0x0001
	KeyFlagModifiable   = 0x0002
	KeyFlagBinary       = 0x0004
	KeyFlagNullKey      = 0x0008
	KeyFlagSegmented    = 0x0010
	KeyFlagDescending   = 0x0020
	KeyFlagSupplemental = 0x0040
	KeyFlagExtendedType = 0x0080
)

// Request represents a Btrieve request
type Request struct {
	Operation     uint16
	PositionBlock []byte
	DataBuffer    []byte
	KeyBuffer     []byte
	KeyNumber     int16
	FilePath      string
	LockBias      uint16
}

// Response represents a Btrieve response
type Response struct {
	StatusCode    uint16
	PositionBlock []byte
	DataBuffer    []byte
	KeyBuffer     []byte
}

// KeySpec represents a key specification for file creation
type KeySpec struct {
	Position  uint16
	Length    uint16
	Flags     uint16
	Type      uint8
	NullValue uint8
}

// FileSpec represents a file specification for creation
type FileSpec struct {
	RecordLength uint16
	PageSize     uint16
	Keys         []KeySpec
}

// Client represents a connection to an Xtrieve server
type Client struct {
	conn  net.Conn
	mu    sync.Mutex
}

// Connect creates a new client and connects to the server
func Connect(host string, port int) (*Client, error) {
	addr := fmt.Sprintf("%s:%d", host, port)
	conn, err := net.Dial("tcp", addr)
	if err != nil {
		return nil, fmt.Errorf("failed to connect: %w", err)
	}

	return &Client{conn: conn}, nil
}

// Close closes the connection
func (c *Client) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

// Execute executes a Btrieve operation
func (c *Client) Execute(req *Request) (*Response, error) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if c.conn == nil {
		return nil, errors.New("not connected")
	}

	// Build request
	packet := c.buildRequest(req)

	// Send request
	if _, err := c.conn.Write(packet); err != nil {
		return nil, fmt.Errorf("send failed: %w", err)
	}

	// Read response
	return c.readResponse()
}

// BuildFileSpec creates a file specification buffer for Create operation
func BuildFileSpec(spec *FileSpec) []byte {
	headerSize := 10
	keySpecSize := 16
	buf := make([]byte, headerSize+len(spec.Keys)*keySpecSize)

	// Header
	binary.LittleEndian.PutUint16(buf[0:], spec.RecordLength)
	binary.LittleEndian.PutUint16(buf[2:], spec.PageSize)
	binary.LittleEndian.PutUint16(buf[4:], uint16(len(spec.Keys)))
	// bytes 6-9 reserved (zero)

	// Key specs
	for i, key := range spec.Keys {
		offset := headerSize + i*keySpecSize
		binary.LittleEndian.PutUint16(buf[offset:], key.Position)
		binary.LittleEndian.PutUint16(buf[offset+2:], key.Length)
		binary.LittleEndian.PutUint16(buf[offset+4:], key.Flags)
		buf[offset+6] = key.Type
		buf[offset+7] = key.NullValue
		// bytes 8-15 reserved (zero)
	}

	return buf
}

// ========== Convenience Methods ==========

// Open opens a file
func (c *Client) Open(filePath string, mode int16) (*Response, error) {
	return c.Execute(&Request{
		Operation: OpOpen,
		FilePath:  filePath,
		KeyNumber: mode,
	})
}

// CloseFile closes an open file
func (c *Client) CloseFile(positionBlock []byte) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpClose,
		PositionBlock: positionBlock,
	})
}

// Create creates a new file
func (c *Client) Create(filePath string, spec *FileSpec) (*Response, error) {
	return c.Execute(&Request{
		Operation:  OpCreate,
		FilePath:   filePath,
		DataBuffer: BuildFileSpec(spec),
	})
}

// Insert inserts a record
func (c *Client) Insert(positionBlock []byte, data []byte) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpInsert,
		PositionBlock: positionBlock,
		DataBuffer:    data,
	})
}

// Update updates the current record
func (c *Client) Update(positionBlock []byte, data []byte, keyNumber int16) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpUpdate,
		PositionBlock: positionBlock,
		DataBuffer:    data,
		KeyNumber:     keyNumber,
	})
}

// Delete deletes the current record
func (c *Client) Delete(positionBlock []byte, keyNumber int16) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpDelete,
		PositionBlock: positionBlock,
		KeyNumber:     keyNumber,
	})
}

// GetEqual gets a record by exact key match
func (c *Client) GetEqual(positionBlock []byte, key []byte, keyNumber int16) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpGetEqual,
		PositionBlock: positionBlock,
		KeyBuffer:     key,
		KeyNumber:     keyNumber,
	})
}

// GetFirst gets the first record in key order
func (c *Client) GetFirst(positionBlock []byte, keyNumber int16) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpGetFirst,
		PositionBlock: positionBlock,
		KeyNumber:     keyNumber,
	})
}

// GetLast gets the last record in key order
func (c *Client) GetLast(positionBlock []byte, keyNumber int16) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpGetLast,
		PositionBlock: positionBlock,
		KeyNumber:     keyNumber,
	})
}

// GetNext gets the next record in key order
func (c *Client) GetNext(positionBlock []byte, keyNumber int16) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpGetNext,
		PositionBlock: positionBlock,
		KeyNumber:     keyNumber,
	})
}

// GetPrevious gets the previous record in key order
func (c *Client) GetPrevious(positionBlock []byte, keyNumber int16) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpGetPrevious,
		PositionBlock: positionBlock,
		KeyNumber:     keyNumber,
	})
}

// BeginTransaction begins a transaction
func (c *Client) BeginTransaction(positionBlock []byte, lockMode uint16) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpBeginTransaction,
		PositionBlock: positionBlock,
		LockBias:      lockMode,
	})
}

// EndTransaction commits a transaction
func (c *Client) EndTransaction(positionBlock []byte) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpEndTransaction,
		PositionBlock: positionBlock,
	})
}

// AbortTransaction rolls back a transaction
func (c *Client) AbortTransaction(positionBlock []byte) (*Response, error) {
	return c.Execute(&Request{
		Operation:     OpAbortTransaction,
		PositionBlock: positionBlock,
	})
}

// ForEach iterates all records
func (c *Client) ForEach(positionBlock []byte, keyNumber int16, fn func(record, key []byte) error) (int, error) {
	resp, err := c.GetFirst(positionBlock, keyNumber)
	if err != nil {
		return 0, err
	}

	count := 0
	for resp.StatusCode == StatusSuccess {
		if err := fn(resp.DataBuffer, resp.KeyBuffer); err != nil {
			return count, err
		}
		count++

		resp, err = c.GetNext(resp.PositionBlock, keyNumber)
		if err != nil {
			return count, err
		}
	}

	return count, nil
}

// ========== Private Methods ==========

func (c *Client) buildRequest(req *Request) []byte {
	posBlock := make([]byte, PositionBlockSize)
	if len(req.PositionBlock) > 0 {
		copy(posBlock, req.PositionBlock)
	}

	filePathBytes := []byte(req.FilePath)

	// Calculate total size
	totalSize := 2 + PositionBlockSize + 4 + len(req.DataBuffer) +
		2 + len(req.KeyBuffer) + 2 + 2 + len(filePathBytes) + 2

	buf := make([]byte, totalSize)
	offset := 0

	// Operation (2 bytes)
	binary.LittleEndian.PutUint16(buf[offset:], req.Operation)
	offset += 2

	// Position block (128 bytes)
	copy(buf[offset:], posBlock)
	offset += PositionBlockSize

	// Data buffer length + data
	binary.LittleEndian.PutUint32(buf[offset:], uint32(len(req.DataBuffer)))
	offset += 4
	copy(buf[offset:], req.DataBuffer)
	offset += len(req.DataBuffer)

	// Key buffer length + key
	binary.LittleEndian.PutUint16(buf[offset:], uint16(len(req.KeyBuffer)))
	offset += 2
	copy(buf[offset:], req.KeyBuffer)
	offset += len(req.KeyBuffer)

	// Key number (2 bytes, signed)
	binary.LittleEndian.PutUint16(buf[offset:], uint16(req.KeyNumber))
	offset += 2

	// File path length + path
	binary.LittleEndian.PutUint16(buf[offset:], uint16(len(filePathBytes)))
	offset += 2
	copy(buf[offset:], filePathBytes)
	offset += len(filePathBytes)

	// Lock bias
	binary.LittleEndian.PutUint16(buf[offset:], req.LockBias)

	return buf
}

func (c *Client) readResponse() (*Response, error) {
	resp := &Response{
		PositionBlock: make([]byte, PositionBlockSize),
	}

	// Read header: status(2) + position_block(128) + data_len(4)
	header := make([]byte, 2+PositionBlockSize+4)
	if _, err := io.ReadFull(c.conn, header); err != nil {
		return nil, fmt.Errorf("read header failed: %w", err)
	}

	resp.StatusCode = binary.LittleEndian.Uint16(header[0:])
	copy(resp.PositionBlock, header[2:2+PositionBlockSize])
	dataLen := binary.LittleEndian.Uint32(header[2+PositionBlockSize:])

	// Read data buffer
	if dataLen > 0 {
		resp.DataBuffer = make([]byte, dataLen)
		if _, err := io.ReadFull(c.conn, resp.DataBuffer); err != nil {
			return nil, fmt.Errorf("read data failed: %w", err)
		}
	}

	// Read key length
	keyLenBuf := make([]byte, 2)
	if _, err := io.ReadFull(c.conn, keyLenBuf); err != nil {
		return nil, fmt.Errorf("read key length failed: %w", err)
	}
	keyLen := binary.LittleEndian.Uint16(keyLenBuf)

	// Read key buffer
	if keyLen > 0 {
		resp.KeyBuffer = make([]byte, keyLen)
		if _, err := io.ReadFull(c.conn, resp.KeyBuffer); err != nil {
			return nil, fmt.Errorf("read key failed: %w", err)
		}
	}

	return resp, nil
}
