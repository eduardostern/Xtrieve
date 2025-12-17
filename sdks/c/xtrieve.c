/**
 * Xtrieve Client for C - Implementation
 */

#include "xtrieve.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#ifdef _WIN32
    #include <winsock2.h>
    #include <ws2tcpip.h>
    #pragma comment(lib, "ws2_32.lib")
    typedef SOCKET socket_t;
    #define INVALID_SOCKET_VALUE INVALID_SOCKET
    #define CLOSE_SOCKET closesocket
#else
    #include <sys/socket.h>
    #include <netinet/in.h>
    #include <arpa/inet.h>
    #include <netdb.h>
    #include <unistd.h>
    #include <errno.h>
    typedef int socket_t;
    #define INVALID_SOCKET_VALUE -1
    #define CLOSE_SOCKET close
#endif

/* ============================================================================
 * Internal structures
 * ============================================================================ */

struct xtrieve_client {
    socket_t socket;
    int connected;
    char last_error[256];
};

/* ============================================================================
 * Helper functions
 * ============================================================================ */

static void set_error(xtrieve_client_t *client, const char *msg) {
    if (client) {
        strncpy(client->last_error, msg, sizeof(client->last_error) - 1);
        client->last_error[sizeof(client->last_error) - 1] = '\0';
    }
}

static int send_all(socket_t sock, const uint8_t *data, size_t len) {
    size_t sent = 0;
    while (sent < len) {
        ssize_t n = send(sock, (const char*)(data + sent), len - sent, 0);
        if (n <= 0) return -1;
        sent += n;
    }
    return 0;
}

static int recv_all(socket_t sock, uint8_t *data, size_t len) {
    size_t received = 0;
    while (received < len) {
        ssize_t n = recv(sock, (char*)(data + received), len - received, 0);
        if (n <= 0) return -1;
        received += n;
    }
    return 0;
}

static uint16_t read_u16_le(const uint8_t *buf) {
    return buf[0] | (buf[1] << 8);
}

static uint32_t read_u32_le(const uint8_t *buf) {
    return buf[0] | (buf[1] << 8) | (buf[2] << 16) | (buf[3] << 24);
}

static void write_u16_le(uint8_t *buf, uint16_t val) {
    buf[0] = val & 0xFF;
    buf[1] = (val >> 8) & 0xFF;
}

static void write_u32_le(uint8_t *buf, uint32_t val) {
    buf[0] = val & 0xFF;
    buf[1] = (val >> 8) & 0xFF;
    buf[2] = (val >> 16) & 0xFF;
    buf[3] = (val >> 24) & 0xFF;
}

static void write_i16_le(uint8_t *buf, int16_t val) {
    write_u16_le(buf, (uint16_t)val);
}

/* ============================================================================
 * Connection functions
 * ============================================================================ */

xtrieve_client_t *xtrieve_connect(const char *host, int port) {
#ifdef _WIN32
    WSADATA wsa_data;
    if (WSAStartup(MAKEWORD(2, 2), &wsa_data) != 0) {
        return NULL;
    }
#endif

    xtrieve_client_t *client = calloc(1, sizeof(xtrieve_client_t));
    if (!client) return NULL;

    client->socket = INVALID_SOCKET_VALUE;
    client->connected = 0;

    /* Resolve host */
    struct addrinfo hints = {0};
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;

    char port_str[16];
    snprintf(port_str, sizeof(port_str), "%d", port);

    struct addrinfo *result;
    if (getaddrinfo(host, port_str, &hints, &result) != 0) {
        set_error(client, "Failed to resolve host");
        free(client);
        return NULL;
    }

    /* Create socket */
    client->socket = socket(result->ai_family, result->ai_socktype, result->ai_protocol);
    if (client->socket == INVALID_SOCKET_VALUE) {
        set_error(client, "Failed to create socket");
        freeaddrinfo(result);
        free(client);
        return NULL;
    }

    /* Connect */
    if (connect(client->socket, result->ai_addr, result->ai_addrlen) != 0) {
        set_error(client, "Failed to connect");
        CLOSE_SOCKET(client->socket);
        freeaddrinfo(result);
        free(client);
        return NULL;
    }

    freeaddrinfo(result);
    client->connected = 1;
    return client;
}

void xtrieve_disconnect(xtrieve_client_t *client) {
    if (!client) return;

    if (client->socket != INVALID_SOCKET_VALUE) {
        CLOSE_SOCKET(client->socket);
    }

#ifdef _WIN32
    WSACleanup();
#endif

    free(client);
}

int xtrieve_is_connected(xtrieve_client_t *client) {
    return client && client->connected;
}

const char *xtrieve_last_error(xtrieve_client_t *client) {
    return client ? client->last_error : "No client";
}

/* ============================================================================
 * Core operations
 * ============================================================================ */

int xtrieve_execute(xtrieve_client_t *client,
                    const xtrieve_request_t *request,
                    xtrieve_response_t *response) {
    if (!client || !client->connected) {
        set_error(client, "Not connected");
        return -1;
    }

    memset(response, 0, sizeof(*response));

    /* Calculate request size */
    size_t path_len = request->file_path ? strlen(request->file_path) : 0;
    size_t req_size = 2 + XTRIEVE_POSITION_BLOCK_SIZE + 4 + request->data_buffer_len +
                      2 + request->key_buffer_len + 2 + 2 + path_len + 2;

    uint8_t *req_buf = malloc(req_size);
    if (!req_buf) {
        set_error(client, "Out of memory");
        return -1;
    }

    /* Build request */
    size_t offset = 0;

    /* Operation (2 bytes) */
    write_u16_le(req_buf + offset, request->operation);
    offset += 2;

    /* Position block (128 bytes) */
    memcpy(req_buf + offset, request->position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    offset += XTRIEVE_POSITION_BLOCK_SIZE;

    /* Data buffer length + data */
    write_u32_le(req_buf + offset, request->data_buffer_len);
    offset += 4;
    if (request->data_buffer_len > 0 && request->data_buffer) {
        memcpy(req_buf + offset, request->data_buffer, request->data_buffer_len);
        offset += request->data_buffer_len;
    }

    /* Key buffer length + key */
    write_u16_le(req_buf + offset, request->key_buffer_len);
    offset += 2;
    if (request->key_buffer_len > 0 && request->key_buffer) {
        memcpy(req_buf + offset, request->key_buffer, request->key_buffer_len);
        offset += request->key_buffer_len;
    }

    /* Key number (2 bytes, signed) */
    write_i16_le(req_buf + offset, request->key_number);
    offset += 2;

    /* File path length + path */
    write_u16_le(req_buf + offset, (uint16_t)path_len);
    offset += 2;
    if (path_len > 0) {
        memcpy(req_buf + offset, request->file_path, path_len);
        offset += path_len;
    }

    /* Lock bias (2 bytes) */
    write_u16_le(req_buf + offset, request->lock_bias);

    /* Send request */
    if (send_all(client->socket, req_buf, req_size) != 0) {
        set_error(client, "Failed to send request");
        free(req_buf);
        client->connected = 0;
        return -1;
    }
    free(req_buf);

    /* Read response header: status(2) + position_block(128) + data_len(4) */
    uint8_t resp_header[2 + XTRIEVE_POSITION_BLOCK_SIZE + 4];
    if (recv_all(client->socket, resp_header, sizeof(resp_header)) != 0) {
        set_error(client, "Failed to receive response header");
        client->connected = 0;
        return -1;
    }

    response->status_code = read_u16_le(resp_header);
    memcpy(response->position_block, resp_header + 2, XTRIEVE_POSITION_BLOCK_SIZE);
    response->data_buffer_len = read_u32_le(resp_header + 2 + XTRIEVE_POSITION_BLOCK_SIZE);

    /* Read data buffer */
    if (response->data_buffer_len > 0) {
        response->data_buffer = malloc(response->data_buffer_len);
        if (!response->data_buffer) {
            set_error(client, "Out of memory");
            return -1;
        }
        if (recv_all(client->socket, response->data_buffer, response->data_buffer_len) != 0) {
            set_error(client, "Failed to receive data buffer");
            free(response->data_buffer);
            response->data_buffer = NULL;
            client->connected = 0;
            return -1;
        }
    }

    /* Read key length */
    uint8_t key_len_buf[2];
    if (recv_all(client->socket, key_len_buf, 2) != 0) {
        set_error(client, "Failed to receive key length");
        xtrieve_response_free(response);
        client->connected = 0;
        return -1;
    }
    response->key_buffer_len = read_u16_le(key_len_buf);

    /* Read key buffer */
    if (response->key_buffer_len > 0) {
        response->key_buffer = malloc(response->key_buffer_len);
        if (!response->key_buffer) {
            set_error(client, "Out of memory");
            xtrieve_response_free(response);
            return -1;
        }
        if (recv_all(client->socket, response->key_buffer, response->key_buffer_len) != 0) {
            set_error(client, "Failed to receive key buffer");
            xtrieve_response_free(response);
            client->connected = 0;
            return -1;
        }
    }

    return 0;
}

void xtrieve_response_free(xtrieve_response_t *response) {
    if (!response) return;
    free(response->data_buffer);
    free(response->key_buffer);
    response->data_buffer = NULL;
    response->key_buffer = NULL;
    response->data_buffer_len = 0;
    response->key_buffer_len = 0;
}

/* ============================================================================
 * Helper functions
 * ============================================================================ */

int xtrieve_build_file_spec(const xtrieve_file_spec_t *spec,
                            uint8_t *buffer,
                            size_t buffer_size) {
    size_t needed = 10 + spec->num_keys * 16;
    if (buffer_size < needed) return -1;

    memset(buffer, 0, needed);

    /* Header */
    write_u16_le(buffer + 0, spec->record_length);
    write_u16_le(buffer + 2, spec->page_size);
    write_u16_le(buffer + 4, spec->num_keys);
    /* 4 bytes reserved at offset 6 */

    /* Key specs */
    for (uint16_t i = 0; i < spec->num_keys; i++) {
        size_t key_offset = 10 + i * 16;
        const xtrieve_key_spec_t *key = &spec->keys[i];

        write_u16_le(buffer + key_offset + 0, key->position);
        write_u16_le(buffer + key_offset + 2, key->length);
        write_u16_le(buffer + key_offset + 4, key->flags);
        buffer[key_offset + 6] = key->type;
        buffer[key_offset + 7] = key->null_value;
        /* 8 bytes reserved */
    }

    return (int)needed;
}

void xtrieve_request_init(xtrieve_request_t *request) {
    memset(request, 0, sizeof(*request));
}

void xtrieve_copy_position_block(xtrieve_request_t *request,
                                  const xtrieve_response_t *response) {
    memcpy(request->position_block, response->position_block, XTRIEVE_POSITION_BLOCK_SIZE);
}

/* ============================================================================
 * Convenience functions
 * ============================================================================ */

int xtrieve_open(xtrieve_client_t *client,
                 const char *file_path,
                 int mode,
                 xtrieve_response_t *response) {
    xtrieve_request_t req;
    xtrieve_request_init(&req);
    req.operation = XTRIEVE_OP_OPEN;
    req.file_path = file_path;
    req.key_number = (int16_t)mode;

    if (xtrieve_execute(client, &req, response) != 0) {
        return -1;
    }
    return response->status_code;
}

int xtrieve_close(xtrieve_client_t *client,
                  const uint8_t *position_block) {
    xtrieve_request_t req;
    xtrieve_response_t resp;
    xtrieve_request_init(&req);
    req.operation = XTRIEVE_OP_CLOSE;
    memcpy(req.position_block, position_block, XTRIEVE_POSITION_BLOCK_SIZE);

    if (xtrieve_execute(client, &req, &resp) != 0) {
        return -1;
    }
    int status = resp.status_code;
    xtrieve_response_free(&resp);
    return status;
}

int xtrieve_create(xtrieve_client_t *client,
                   const char *file_path,
                   const xtrieve_file_spec_t *spec) {
    uint8_t spec_buf[1024];
    int spec_len = xtrieve_build_file_spec(spec, spec_buf, sizeof(spec_buf));
    if (spec_len < 0) return -1;

    xtrieve_request_t req;
    xtrieve_response_t resp;
    xtrieve_request_init(&req);
    req.operation = XTRIEVE_OP_CREATE;
    req.file_path = file_path;
    req.data_buffer = spec_buf;
    req.data_buffer_len = spec_len;

    if (xtrieve_execute(client, &req, &resp) != 0) {
        return -1;
    }
    int status = resp.status_code;
    xtrieve_response_free(&resp);
    return status;
}

int xtrieve_insert(xtrieve_client_t *client,
                   uint8_t *position_block,
                   const uint8_t *data,
                   uint32_t data_len) {
    xtrieve_request_t req;
    xtrieve_response_t resp;
    xtrieve_request_init(&req);
    req.operation = XTRIEVE_OP_INSERT;
    memcpy(req.position_block, position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    req.data_buffer = (uint8_t*)data;
    req.data_buffer_len = data_len;

    if (xtrieve_execute(client, &req, &resp) != 0) {
        return -1;
    }
    memcpy(position_block, resp.position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    int status = resp.status_code;
    xtrieve_response_free(&resp);
    return status;
}

int xtrieve_get_first(xtrieve_client_t *client,
                      uint8_t *position_block,
                      int key_number,
                      xtrieve_response_t *response) {
    xtrieve_request_t req;
    xtrieve_request_init(&req);
    req.operation = XTRIEVE_OP_GET_FIRST;
    memcpy(req.position_block, position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    req.key_number = (int16_t)key_number;

    if (xtrieve_execute(client, &req, response) != 0) {
        return -1;
    }
    memcpy(position_block, response->position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    return response->status_code;
}

int xtrieve_get_next(xtrieve_client_t *client,
                     uint8_t *position_block,
                     int key_number,
                     xtrieve_response_t *response) {
    xtrieve_request_t req;
    xtrieve_request_init(&req);
    req.operation = XTRIEVE_OP_GET_NEXT;
    memcpy(req.position_block, position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    req.key_number = (int16_t)key_number;

    if (xtrieve_execute(client, &req, response) != 0) {
        return -1;
    }
    memcpy(position_block, response->position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    return response->status_code;
}

int xtrieve_get_equal(xtrieve_client_t *client,
                      uint8_t *position_block,
                      const uint8_t *key,
                      uint16_t key_len,
                      int key_number,
                      xtrieve_response_t *response) {
    xtrieve_request_t req;
    xtrieve_request_init(&req);
    req.operation = XTRIEVE_OP_GET_EQUAL;
    memcpy(req.position_block, position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    req.key_buffer = (uint8_t*)key;
    req.key_buffer_len = key_len;
    req.key_number = (int16_t)key_number;

    if (xtrieve_execute(client, &req, response) != 0) {
        return -1;
    }
    memcpy(position_block, response->position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    return response->status_code;
}
