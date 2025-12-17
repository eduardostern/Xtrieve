/**
 * Xtrieve Client for C
 *
 * Btrieve 5.1 compatible ISAM database client using binary TCP protocol.
 *
 * Usage:
 *   xtrieve_client_t *client = xtrieve_connect("127.0.0.1", 7419);
 *   if (!client) { handle_error(); }
 *
 *   xtrieve_request_t req = {0};
 *   xtrieve_response_t resp = {0};
 *
 *   req.operation = XTRIEVE_OP_OPEN;
 *   req.file_path = "data.dat";
 *   xtrieve_execute(client, &req, &resp);
 *
 *   xtrieve_disconnect(client);
 */

#ifndef XTRIEVE_H
#define XTRIEVE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Constants
 * ============================================================================ */

#define XTRIEVE_POSITION_BLOCK_SIZE 128
#define XTRIEVE_DEFAULT_PORT 7419
#define XTRIEVE_MAX_RECORD_SIZE 65535
#define XTRIEVE_MAX_KEY_SIZE 255
#define XTRIEVE_MAX_PATH_SIZE 260

/* Operation codes */
#define XTRIEVE_OP_OPEN                  0
#define XTRIEVE_OP_CLOSE                 1
#define XTRIEVE_OP_INSERT                2
#define XTRIEVE_OP_UPDATE                3
#define XTRIEVE_OP_DELETE                4
#define XTRIEVE_OP_GET_EQUAL             5
#define XTRIEVE_OP_GET_NEXT              6
#define XTRIEVE_OP_GET_PREVIOUS          7
#define XTRIEVE_OP_GET_GREATER           8
#define XTRIEVE_OP_GET_GREATER_OR_EQUAL  9
#define XTRIEVE_OP_GET_LESS              10
#define XTRIEVE_OP_GET_LESS_OR_EQUAL     11
#define XTRIEVE_OP_GET_FIRST             12
#define XTRIEVE_OP_GET_LAST              13
#define XTRIEVE_OP_CREATE                14
#define XTRIEVE_OP_STAT                  15
#define XTRIEVE_OP_BEGIN_TRANSACTION     19
#define XTRIEVE_OP_END_TRANSACTION       20
#define XTRIEVE_OP_ABORT_TRANSACTION     21
#define XTRIEVE_OP_STEP_NEXT             24
#define XTRIEVE_OP_UNLOCK                27
#define XTRIEVE_OP_STEP_FIRST            33
#define XTRIEVE_OP_STEP_LAST             34
#define XTRIEVE_OP_STEP_PREVIOUS         35

/* Status codes */
#define XTRIEVE_SUCCESS                  0
#define XTRIEVE_ERR_INVALID_OPERATION    1
#define XTRIEVE_ERR_IO_ERROR             2
#define XTRIEVE_ERR_FILE_NOT_OPEN        3
#define XTRIEVE_ERR_KEY_NOT_FOUND        4
#define XTRIEVE_ERR_DUPLICATE_KEY        5
#define XTRIEVE_ERR_INVALID_KEY_NUMBER   6
#define XTRIEVE_ERR_DIFFERENT_KEY_NUMBER 7
#define XTRIEVE_ERR_INVALID_POSITIONING  8
#define XTRIEVE_ERR_END_OF_FILE          9
#define XTRIEVE_ERR_FILE_NOT_FOUND       12
#define XTRIEVE_ERR_DISK_FULL            18
#define XTRIEVE_ERR_DATA_BUFFER_SHORT    22
#define XTRIEVE_ERR_RECORD_LOCKED        84
#define XTRIEVE_ERR_FILE_LOCKED          85

/* Lock bias values */
#define XTRIEVE_LOCK_NONE                0
#define XTRIEVE_LOCK_SINGLE_WAIT         100
#define XTRIEVE_LOCK_SINGLE_NO_WAIT      200
#define XTRIEVE_LOCK_MULTI_WAIT          300
#define XTRIEVE_LOCK_MULTI_NO_WAIT       400

/* Key types */
#define XTRIEVE_KEY_TYPE_STRING          0
#define XTRIEVE_KEY_TYPE_INTEGER         1
#define XTRIEVE_KEY_TYPE_FLOAT           2
#define XTRIEVE_KEY_TYPE_DATE            3
#define XTRIEVE_KEY_TYPE_TIME            4
#define XTRIEVE_KEY_TYPE_DECIMAL         5
#define XTRIEVE_KEY_TYPE_MONEY           6
#define XTRIEVE_KEY_TYPE_LOGICAL         7
#define XTRIEVE_KEY_TYPE_NUMERIC         8
#define XTRIEVE_KEY_TYPE_BFLOAT          9
#define XTRIEVE_KEY_TYPE_LSTRING         10
#define XTRIEVE_KEY_TYPE_ZSTRING         11
#define XTRIEVE_KEY_TYPE_UNSIGNED_BINARY 14
#define XTRIEVE_KEY_TYPE_AUTOINCREMENT   15

/* Key flags */
#define XTRIEVE_KEY_FLAG_DUPLICATES      0x0001
#define XTRIEVE_KEY_FLAG_MODIFIABLE      0x0002
#define XTRIEVE_KEY_FLAG_BINARY          0x0004
#define XTRIEVE_KEY_FLAG_NULL_KEY        0x0008
#define XTRIEVE_KEY_FLAG_SEGMENTED       0x0010
#define XTRIEVE_KEY_FLAG_DESCENDING      0x0020
#define XTRIEVE_KEY_FLAG_SUPPLEMENTAL    0x0040
#define XTRIEVE_KEY_FLAG_EXTENDED_TYPE   0x0080

/* ============================================================================
 * Types
 * ============================================================================ */

/** Opaque client handle */
typedef struct xtrieve_client xtrieve_client_t;

/** Request structure */
typedef struct xtrieve_request {
    uint16_t operation;
    uint8_t  position_block[XTRIEVE_POSITION_BLOCK_SIZE];
    uint8_t  *data_buffer;
    uint32_t data_buffer_len;
    uint8_t  *key_buffer;
    uint16_t key_buffer_len;
    int16_t  key_number;
    const char *file_path;
    uint16_t lock_bias;
} xtrieve_request_t;

/** Response structure */
typedef struct xtrieve_response {
    uint16_t status_code;
    uint8_t  position_block[XTRIEVE_POSITION_BLOCK_SIZE];
    uint8_t  *data_buffer;
    uint32_t data_buffer_len;
    uint8_t  *key_buffer;
    uint16_t key_buffer_len;
} xtrieve_response_t;

/** Key specification for file creation */
typedef struct xtrieve_key_spec {
    uint16_t position;
    uint16_t length;
    uint16_t flags;
    uint8_t  type;
    uint8_t  null_value;
} xtrieve_key_spec_t;

/** File specification for file creation */
typedef struct xtrieve_file_spec {
    uint16_t record_length;
    uint16_t page_size;
    uint16_t num_keys;
    xtrieve_key_spec_t *keys;
} xtrieve_file_spec_t;

/* ============================================================================
 * Connection Functions
 * ============================================================================ */

/**
 * Connect to an Xtrieve server.
 *
 * @param host Server hostname or IP address
 * @param port Server port (default: 7419)
 * @return Client handle, or NULL on failure
 */
xtrieve_client_t *xtrieve_connect(const char *host, int port);

/**
 * Disconnect from the server and free resources.
 *
 * @param client Client handle
 */
void xtrieve_disconnect(xtrieve_client_t *client);

/**
 * Check if connected.
 *
 * @param client Client handle
 * @return 1 if connected, 0 otherwise
 */
int xtrieve_is_connected(xtrieve_client_t *client);

/**
 * Get last error message.
 *
 * @param client Client handle
 * @return Error message string (do not free)
 */
const char *xtrieve_last_error(xtrieve_client_t *client);

/* ============================================================================
 * Core Operations
 * ============================================================================ */

/**
 * Execute a Btrieve operation.
 *
 * @param client Client handle
 * @param request Request parameters
 * @param response Response (caller must free data_buffer and key_buffer)
 * @return 0 on success, -1 on communication error
 */
int xtrieve_execute(xtrieve_client_t *client,
                    const xtrieve_request_t *request,
                    xtrieve_response_t *response);

/**
 * Free response buffers.
 *
 * @param response Response to free
 */
void xtrieve_response_free(xtrieve_response_t *response);

/* ============================================================================
 * Helper Functions
 * ============================================================================ */

/**
 * Build file specification buffer for Create operation.
 *
 * @param spec File specification
 * @param buffer Output buffer (must be large enough)
 * @param buffer_size Size of output buffer
 * @return Number of bytes written, or -1 on error
 */
int xtrieve_build_file_spec(const xtrieve_file_spec_t *spec,
                            uint8_t *buffer,
                            size_t buffer_size);

/**
 * Initialize a request structure.
 *
 * @param request Request to initialize
 */
void xtrieve_request_init(xtrieve_request_t *request);

/**
 * Copy position block from response to request.
 *
 * @param request Destination request
 * @param response Source response
 */
void xtrieve_copy_position_block(xtrieve_request_t *request,
                                  const xtrieve_response_t *response);

/* ============================================================================
 * Convenience Functions
 * ============================================================================ */

/**
 * Open a file.
 *
 * @param client Client handle
 * @param file_path Path to file
 * @param mode Open mode (-1=normal, -2=read-only, -3=exclusive)
 * @param response Response with position block
 * @return 0 on success, -1 on error
 */
int xtrieve_open(xtrieve_client_t *client,
                 const char *file_path,
                 int mode,
                 xtrieve_response_t *response);

/**
 * Close a file.
 *
 * @param client Client handle
 * @param position_block Position block from Open
 * @return Status code
 */
int xtrieve_close(xtrieve_client_t *client,
                  const uint8_t *position_block);

/**
 * Create a new file.
 *
 * @param client Client handle
 * @param file_path Path for new file
 * @param spec File specification
 * @return Status code
 */
int xtrieve_create(xtrieve_client_t *client,
                   const char *file_path,
                   const xtrieve_file_spec_t *spec);

/**
 * Insert a record.
 *
 * @param client Client handle
 * @param position_block Position block (updated on success)
 * @param data Record data
 * @param data_len Record length
 * @return Status code
 */
int xtrieve_insert(xtrieve_client_t *client,
                   uint8_t *position_block,
                   const uint8_t *data,
                   uint32_t data_len);

/**
 * Get first record in key order.
 *
 * @param client Client handle
 * @param position_block Position block (updated on success)
 * @param key_number Key index
 * @param response Response with record data
 * @return Status code
 */
int xtrieve_get_first(xtrieve_client_t *client,
                      uint8_t *position_block,
                      int key_number,
                      xtrieve_response_t *response);

/**
 * Get next record in key order.
 *
 * @param client Client handle
 * @param position_block Position block (updated on success)
 * @param key_number Key index
 * @param response Response with record data
 * @return Status code
 */
int xtrieve_get_next(xtrieve_client_t *client,
                     uint8_t *position_block,
                     int key_number,
                     xtrieve_response_t *response);

/**
 * Get record by exact key match.
 *
 * @param client Client handle
 * @param position_block Position block (updated on success)
 * @param key Key value
 * @param key_len Key length
 * @param key_number Key index
 * @param response Response with record data
 * @return Status code
 */
int xtrieve_get_equal(xtrieve_client_t *client,
                      uint8_t *position_block,
                      const uint8_t *key,
                      uint16_t key_len,
                      int key_number,
                      xtrieve_response_t *response);

#ifdef __cplusplus
}
#endif

#endif /* XTRIEVE_H */
