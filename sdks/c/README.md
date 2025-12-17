# Xtrieve Client for C

C client library for Xtrieve - a Btrieve 5.1 compatible ISAM database engine.

## Building

```bash
# Build static and shared libraries
make

# Build example
make example

# Install (optional)
sudo make install
```

## Quick Start

```c
#include "xtrieve.h"
#include <stdio.h>
#include <string.h>

int main(void) {
    /* Connect */
    xtrieve_client_t *client = xtrieve_connect("127.0.0.1", 7419);
    if (!client) {
        fprintf(stderr, "Connection failed\n");
        return 1;
    }

    /* Open file */
    xtrieve_response_t resp;
    int status = xtrieve_open(client, "data.dat", -1, &resp);
    if (status != XTRIEVE_SUCCESS) {
        fprintf(stderr, "Open failed: %d\n", status);
        xtrieve_disconnect(client);
        return 1;
    }

    /* Save position block */
    uint8_t pos_block[XTRIEVE_POSITION_BLOCK_SIZE];
    memcpy(pos_block, resp.position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    xtrieve_response_free(&resp);

    /* Read first record */
    status = xtrieve_get_first(client, pos_block, 0, &resp);
    if (status == XTRIEVE_SUCCESS) {
        printf("Record data: %.*s\n", (int)resp.data_buffer_len, resp.data_buffer);
        xtrieve_response_free(&resp);
    }

    /* Close and disconnect */
    xtrieve_close(client, pos_block);
    xtrieve_disconnect(client);

    return 0;
}
```

## API Reference

### Connection

```c
/* Connect to server */
xtrieve_client_t *xtrieve_connect(const char *host, int port);

/* Disconnect and free resources */
void xtrieve_disconnect(xtrieve_client_t *client);

/* Check connection status */
int xtrieve_is_connected(xtrieve_client_t *client);

/* Get last error message */
const char *xtrieve_last_error(xtrieve_client_t *client);
```

### File Operations

```c
/* Open a file */
int xtrieve_open(xtrieve_client_t *client,
                 const char *file_path,
                 int mode,  /* -1=normal, -2=read-only, -3=exclusive */
                 xtrieve_response_t *response);

/* Close a file */
int xtrieve_close(xtrieve_client_t *client,
                  const uint8_t *position_block);

/* Create a new file */
int xtrieve_create(xtrieve_client_t *client,
                   const char *file_path,
                   const xtrieve_file_spec_t *spec);
```

### Record Operations

```c
/* Insert a record */
int xtrieve_insert(xtrieve_client_t *client,
                   uint8_t *position_block,  /* Updated on success */
                   const uint8_t *data,
                   uint32_t data_len);

/* Get first record */
int xtrieve_get_first(xtrieve_client_t *client,
                      uint8_t *position_block,
                      int key_number,
                      xtrieve_response_t *response);

/* Get next record */
int xtrieve_get_next(xtrieve_client_t *client,
                     uint8_t *position_block,
                     int key_number,
                     xtrieve_response_t *response);

/* Get record by key */
int xtrieve_get_equal(xtrieve_client_t *client,
                      uint8_t *position_block,
                      const uint8_t *key,
                      uint16_t key_len,
                      int key_number,
                      xtrieve_response_t *response);
```

### Low-Level

```c
/* Execute any operation */
int xtrieve_execute(xtrieve_client_t *client,
                    const xtrieve_request_t *request,
                    xtrieve_response_t *response);

/* Free response buffers */
void xtrieve_response_free(xtrieve_response_t *response);
```

## Constants

### Operations

```c
XTRIEVE_OP_OPEN                  // 0
XTRIEVE_OP_CLOSE                 // 1
XTRIEVE_OP_INSERT                // 2
XTRIEVE_OP_UPDATE                // 3
XTRIEVE_OP_DELETE                // 4
XTRIEVE_OP_GET_EQUAL             // 5
XTRIEVE_OP_GET_NEXT              // 6
XTRIEVE_OP_GET_PREVIOUS          // 7
XTRIEVE_OP_GET_FIRST             // 12
XTRIEVE_OP_GET_LAST              // 13
XTRIEVE_OP_CREATE                // 14
XTRIEVE_OP_BEGIN_TRANSACTION     // 19
XTRIEVE_OP_END_TRANSACTION       // 20
XTRIEVE_OP_ABORT_TRANSACTION     // 21
```

### Status Codes

```c
XTRIEVE_SUCCESS                  // 0
XTRIEVE_ERR_KEY_NOT_FOUND        // 4
XTRIEVE_ERR_DUPLICATE_KEY        // 5
XTRIEVE_ERR_INVALID_POSITIONING  // 8
XTRIEVE_ERR_END_OF_FILE          // 9
XTRIEVE_ERR_FILE_NOT_FOUND       // 12
XTRIEVE_ERR_RECORD_LOCKED        // 84
```

### Key Types

```c
XTRIEVE_KEY_TYPE_STRING          // 0
XTRIEVE_KEY_TYPE_INTEGER         // 1
XTRIEVE_KEY_TYPE_UNSIGNED_BINARY // 14
XTRIEVE_KEY_TYPE_AUTOINCREMENT   // 15
```

## Creating Files

```c
/* Define keys */
xtrieve_key_spec_t keys[2] = {
    {
        .position = 0,
        .length = 8,
        .flags = 0,
        .type = XTRIEVE_KEY_TYPE_UNSIGNED_BINARY
    },
    {
        .position = 8,
        .length = 32,
        .flags = XTRIEVE_KEY_FLAG_DUPLICATES,
        .type = XTRIEVE_KEY_TYPE_STRING
    }
};

/* Define file */
xtrieve_file_spec_t spec = {
    .record_length = 100,
    .page_size = 4096,
    .num_keys = 2,
    .keys = keys
};

/* Create */
int status = xtrieve_create(client, "myfile.dat", &spec);
```

## Iterating Records

```c
xtrieve_response_t resp;
uint8_t pos_block[XTRIEVE_POSITION_BLOCK_SIZE];

/* Copy from open response */
memcpy(pos_block, open_resp.position_block, XTRIEVE_POSITION_BLOCK_SIZE);

/* Iterate */
int status = xtrieve_get_first(client, pos_block, 0, &resp);
while (status == XTRIEVE_SUCCESS) {
    /* Process resp.data_buffer */
    process_record(resp.data_buffer, resp.data_buffer_len);

    xtrieve_response_free(&resp);
    status = xtrieve_get_next(client, pos_block, 0, &resp);
}
xtrieve_response_free(&resp);
```

## Thread Safety

The client is NOT thread-safe. Use one client per thread or implement your own synchronization.

## License

MIT
