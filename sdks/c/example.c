/**
 * Xtrieve C SDK Example
 *
 * Compile: make example
 * Run: ./example
 */

#include <stdio.h>
#include <string.h>
#include "xtrieve.h"

int main(void) {
    printf("Xtrieve C SDK Example\n");
    printf("=====================\n\n");

    /* Connect to server */
    printf("Connecting to 127.0.0.1:7419...\n");
    xtrieve_client_t *client = xtrieve_connect("127.0.0.1", XTRIEVE_DEFAULT_PORT);
    if (!client) {
        fprintf(stderr, "Failed to connect\n");
        return 1;
    }
    printf("Connected!\n\n");

    /* Create a test file */
    printf("Creating test file...\n");
    xtrieve_key_spec_t keys[1] = {
        { .position = 0, .length = 8, .flags = 0, .type = XTRIEVE_KEY_TYPE_UNSIGNED_BINARY }
    };
    xtrieve_file_spec_t spec = {
        .record_length = 100,
        .page_size = 4096,
        .num_keys = 1,
        .keys = keys
    };

    int status = xtrieve_create(client, "example.dat", &spec);
    if (status != XTRIEVE_SUCCESS && status != 59) {  /* 59 = file already exists */
        printf("Create failed: %d\n", status);
    } else {
        printf("File created (or exists)\n");
    }

    /* Open the file */
    printf("\nOpening file...\n");
    xtrieve_response_t resp;
    status = xtrieve_open(client, "example.dat", -1, &resp);
    if (status != XTRIEVE_SUCCESS) {
        fprintf(stderr, "Open failed: %d\n", status);
        xtrieve_disconnect(client);
        return 1;
    }
    printf("File opened\n");

    /* Save position block */
    uint8_t pos_block[XTRIEVE_POSITION_BLOCK_SIZE];
    memcpy(pos_block, resp.position_block, XTRIEVE_POSITION_BLOCK_SIZE);
    xtrieve_response_free(&resp);

    /* Insert some records */
    printf("\nInserting records...\n");
    for (int i = 1; i <= 5; i++) {
        uint8_t record[100] = {0};

        /* Write ID (8 bytes, little-endian) */
        uint64_t id = i * 1000;
        for (int j = 0; j < 8; j++) {
            record[j] = (id >> (j * 8)) & 0xFF;
        }

        /* Write name */
        char name[32];
        snprintf(name, sizeof(name), "Record %d", i);
        memcpy(record + 8, name, strlen(name));

        status = xtrieve_insert(client, pos_block, record, 100);
        if (status == XTRIEVE_SUCCESS) {
            printf("  Inserted record %d\n", i);
        } else if (status == XTRIEVE_ERR_DUPLICATE_KEY) {
            printf("  Record %d already exists\n", i);
        } else {
            printf("  Insert failed: %d\n", status);
        }
    }

    /* Read all records */
    printf("\nReading all records:\n");
    status = xtrieve_get_first(client, pos_block, 0, &resp);

    while (status == XTRIEVE_SUCCESS) {
        /* Parse record */
        uint64_t id = 0;
        for (int j = 0; j < 8; j++) {
            id |= ((uint64_t)resp.data_buffer[j]) << (j * 8);
        }
        char name[33] = {0};
        memcpy(name, resp.data_buffer + 8, 32);

        printf("  ID: %llu, Name: %s\n", (unsigned long long)id, name);

        xtrieve_response_free(&resp);
        status = xtrieve_get_next(client, pos_block, 0, &resp);
    }
    xtrieve_response_free(&resp);

    if (status == XTRIEVE_ERR_END_OF_FILE) {
        printf("  (End of file)\n");
    }

    /* Close file */
    printf("\nClosing file...\n");
    xtrieve_close(client, pos_block);

    /* Disconnect */
    printf("Disconnecting...\n");
    xtrieve_disconnect(client);

    printf("\nDone!\n");
    return 0;
}
