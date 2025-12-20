/* XTRIEVE.C - COM to TCP Bridge for Windows 98SE
 *
 * Bridges BTRSERL.EXE (DOS TSR) to xtrieved server via TCP/IP.
 * Uses com0com virtual COM port pair.
 *
 * Architecture:
 *   DOS App -> BTRSERL.EXE -> COM1 -> com0com -> COM2 -> XTRIEVE.EXE -> TCP -> xtrieved
 *
 * Compile with Borland C++ 5.5:
 *   BCC32 -W -O2 XTRIEVE.C WSOCK32.LIB
 *
 * Copyright (c) 2025 Eduardo Stern
 * MIT License
 */

#include <windows.h>
#include <winsock.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define POS_BLOCK_SIZE 128
#define MAX_BUFFER 8192
#define DEFAULT_COM_PORT "COM2"
#define DEFAULT_SERVER "127.0.0.1"
#define DEFAULT_PORT 7419

/* Global handles */
HANDLE hComPort = INVALID_HANDLE_VALUE;
SOCKET xtrieve_sock = INVALID_SOCKET;

/* Configuration */
char g_com_port[32] = DEFAULT_COM_PORT;
char g_server[256] = DEFAULT_SERVER;
int g_port = DEFAULT_PORT;

/* Statistics */
DWORD g_request_count = 0;

/*---------------------------------------------------------------------------
 * Serial port functions
 *---------------------------------------------------------------------------*/

BOOL serial_init(const char *port_name)
{
    DCB dcb;
    COMMTIMEOUTS timeouts;
    char full_name[32];

    /* Build full port name (\\.\COM2) */
    sprintf(full_name, "\\\\.\\%s", port_name);

    hComPort = CreateFile(full_name,
                          GENERIC_READ | GENERIC_WRITE,
                          0,
                          NULL,
                          OPEN_EXISTING,
                          0,
                          NULL);

    if (hComPort == INVALID_HANDLE_VALUE) {
        printf("Error: Cannot open %s (error %lu)\n", port_name, GetLastError());
        return FALSE;
    }

    /* Configure port: 115200 8N1 */
    memset(&dcb, 0, sizeof(dcb));
    dcb.DCBlength = sizeof(dcb);

    if (!GetCommState(hComPort, &dcb)) {
        printf("Error: GetCommState failed\n");
        CloseHandle(hComPort);
        return FALSE;
    }

    dcb.BaudRate = 115200;
    dcb.ByteSize = 8;
    dcb.Parity = NOPARITY;
    dcb.StopBits = ONESTOPBIT;
    dcb.fBinary = TRUE;
    dcb.fParity = FALSE;
    dcb.fOutxCtsFlow = FALSE;
    dcb.fOutxDsrFlow = FALSE;
    dcb.fDtrControl = DTR_CONTROL_ENABLE;
    dcb.fRtsControl = RTS_CONTROL_ENABLE;
    dcb.fOutX = FALSE;
    dcb.fInX = FALSE;

    if (!SetCommState(hComPort, &dcb)) {
        printf("Error: SetCommState failed\n");
        CloseHandle(hComPort);
        return FALSE;
    }

    /* Set timeouts */
    timeouts.ReadIntervalTimeout = 50;
    timeouts.ReadTotalTimeoutMultiplier = 10;
    timeouts.ReadTotalTimeoutConstant = 1000;
    timeouts.WriteTotalTimeoutMultiplier = 10;
    timeouts.WriteTotalTimeoutConstant = 1000;
    SetCommTimeouts(hComPort, &timeouts);

    printf("[*] Opened %s at 115200 baud\n", port_name);
    return TRUE;
}

int serial_read_byte(void)
{
    BYTE b;
    DWORD bytesRead;

    if (!ReadFile(hComPort, &b, 1, &bytesRead, NULL) || bytesRead != 1) {
        return -1;
    }
    return b;
}

BOOL serial_read_bytes(BYTE *buffer, DWORD count)
{
    DWORD totalRead = 0;
    DWORD bytesRead;

    while (totalRead < count) {
        if (!ReadFile(hComPort, buffer + totalRead, count - totalRead, &bytesRead, NULL)) {
            return FALSE;
        }
        if (bytesRead == 0) {
            return FALSE;  /* Timeout */
        }
        totalRead += bytesRead;
    }
    return TRUE;
}

BOOL serial_write_bytes(const BYTE *buffer, DWORD count)
{
    DWORD bytesWritten;
    return WriteFile(hComPort, buffer, count, &bytesWritten, NULL) && bytesWritten == count;
}

/*---------------------------------------------------------------------------
 * TCP/IP functions
 *---------------------------------------------------------------------------*/

BOOL tcp_init(const char *server, int port)
{
    WSADATA wsa;
    struct sockaddr_in addr;
    struct hostent *host;

    if (WSAStartup(MAKEWORD(1, 1), &wsa) != 0) {
        printf("Error: WSAStartup failed\n");
        return FALSE;
    }

    xtrieve_sock = socket(AF_INET, SOCK_STREAM, 0);
    if (xtrieve_sock == INVALID_SOCKET) {
        printf("Error: socket() failed\n");
        return FALSE;
    }

    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons((u_short)port);

    /* Try as IP address first */
    addr.sin_addr.s_addr = inet_addr(server);
    if (addr.sin_addr.s_addr == INADDR_NONE) {
        /* Resolve hostname */
        host = gethostbyname(server);
        if (host == NULL) {
            printf("Error: Cannot resolve %s\n", server);
            closesocket(xtrieve_sock);
            return FALSE;
        }
        memcpy(&addr.sin_addr, host->h_addr, host->h_length);
    }

    printf("[*] Connecting to %s:%d...\n", server, port);

    if (connect(xtrieve_sock, (struct sockaddr *)&addr, sizeof(addr)) != 0) {
        printf("Error: connect() failed (error %d)\n", WSAGetLastError());
        closesocket(xtrieve_sock);
        return FALSE;
    }

    printf("[+] Connected to xtrieved\n");
    return TRUE;
}

BOOL tcp_send(const BYTE *buffer, int len)
{
    int sent = send(xtrieve_sock, (const char *)buffer, len, 0);
    return sent == len;
}

int tcp_recv(BYTE *buffer, int max_len)
{
    return recv(xtrieve_sock, (char *)buffer, max_len, 0);
}

BOOL tcp_recv_exact(BYTE *buffer, int count)
{
    int totalRead = 0;
    int bytesRead;

    while (totalRead < count) {
        bytesRead = recv(xtrieve_sock, (char *)(buffer + totalRead), count - totalRead, 0);
        if (bytesRead <= 0) {
            return FALSE;
        }
        totalRead += bytesRead;
    }
    return TRUE;
}

/*---------------------------------------------------------------------------
 * Protocol handling
 *---------------------------------------------------------------------------*/

/* Wait for sync marker 0xBB 0xBB */
BOOL wait_for_sync(void)
{
    int b;
    BOOL found_first = FALSE;

    while (1) {
        b = serial_read_byte();
        if (b < 0) {
            continue;  /* Keep waiting */
        }

        if (b == 0xBB) {
            if (found_first) {
                return TRUE;  /* Got 0xBB 0xBB */
            }
            found_first = TRUE;
        } else {
            found_first = FALSE;
        }
    }
}

/* Read 2-byte little-endian value from COM port */
WORD read_u16_serial(void)
{
    BYTE buf[2];
    if (!serial_read_bytes(buf, 2)) return 0xFFFF;
    return buf[0] | (buf[1] << 8);
}

/* Read 4-byte little-endian value from COM port */
DWORD read_u32_serial(void)
{
    BYTE buf[4];
    if (!serial_read_bytes(buf, 4)) return 0xFFFFFFFF;
    return buf[0] | (buf[1] << 8) | (buf[2] << 16) | (buf[3] << 24);
}

/* Read 2-byte little-endian value from TCP */
WORD read_u16_tcp(void)
{
    BYTE buf[2];
    if (!tcp_recv_exact(buf, 2)) return 0xFFFF;
    return buf[0] | (buf[1] << 8);
}

/* Read 4-byte little-endian value from TCP */
DWORD read_u32_tcp(void)
{
    BYTE buf[4];
    if (!tcp_recv_exact(buf, 4)) return 0xFFFFFFFF;
    return buf[0] | (buf[1] << 8) | (buf[2] << 16) | (buf[3] << 24);
}

/*
 * Process one request from BTRSERL.EXE:
 *
 * Serial format (from BTRSERL):
 *   [sync:2][op:2][pos:128][dlen:4][data:N][klen:2][key:N][knum:2][plen:2][path:N][lock:2]
 *
 * TCP format (to xtrieved) - same but without sync:
 *   [op:2][pos:128][dlen:4][data:N][klen:2][key:N][knum:2][plen:2][path:N][lock:2]
 *
 * Response (from xtrieved and to BTRSERL):
 *   [status:2][pos:128][dlen:4][data:N][klen:2][key:N]
 */
BOOL process_request(void)
{
    BYTE request[MAX_BUFFER];
    BYTE response[MAX_BUFFER];
    int req_len = 0;
    int resp_len = 0;

    WORD op, key_len, key_num, path_len, lock;
    DWORD data_len;
    BYTE pos_block[POS_BLOCK_SIZE];

    WORD resp_status, resp_key_len;
    DWORD resp_data_len;

    /* Wait for sync marker */
    if (!wait_for_sync()) {
        printf("[-] Lost sync\n");
        return FALSE;
    }

    /* Read operation code */
    op = read_u16_serial();
    if (op == 0xFFFF) return FALSE;
    printf("[>] Request #%lu: op=%u\n", g_request_count + 1, op);

    /* Build request buffer for TCP (without sync marker) */
    request[req_len++] = op & 0xFF;
    request[req_len++] = (op >> 8) & 0xFF;

    /* Read and forward position block (128 bytes) */
    if (!serial_read_bytes(pos_block, POS_BLOCK_SIZE)) return FALSE;
    memcpy(request + req_len, pos_block, POS_BLOCK_SIZE);
    req_len += POS_BLOCK_SIZE;

    /* Read data length and data */
    data_len = read_u32_serial();
    if (data_len == 0xFFFFFFFF) return FALSE;
    request[req_len++] = data_len & 0xFF;
    request[req_len++] = (data_len >> 8) & 0xFF;
    request[req_len++] = (data_len >> 16) & 0xFF;
    request[req_len++] = (data_len >> 24) & 0xFF;

    if (data_len > 0) {
        if (!serial_read_bytes(request + req_len, data_len)) return FALSE;
        req_len += data_len;
    }

    /* Read key length and key */
    key_len = read_u16_serial();
    if (key_len == 0xFFFF) return FALSE;
    request[req_len++] = key_len & 0xFF;
    request[req_len++] = (key_len >> 8) & 0xFF;

    if (key_len > 0) {
        if (!serial_read_bytes(request + req_len, key_len)) return FALSE;
        req_len += key_len;
    }

    /* Read key number */
    key_num = read_u16_serial();
    if (key_num == 0xFFFF) return FALSE;
    request[req_len++] = key_num & 0xFF;
    request[req_len++] = (key_num >> 8) & 0xFF;

    /* Read path length and path */
    path_len = read_u16_serial();
    if (path_len == 0xFFFF) return FALSE;
    request[req_len++] = path_len & 0xFF;
    request[req_len++] = (path_len >> 8) & 0xFF;

    if (path_len > 0) {
        if (!serial_read_bytes(request + req_len, path_len)) return FALSE;
        req_len += path_len;
    }

    /* Read lock bias */
    lock = read_u16_serial();
    if (lock == 0xFFFF) return FALSE;
    request[req_len++] = lock & 0xFF;
    request[req_len++] = (lock >> 8) & 0xFF;

    printf("    data_len=%lu key_len=%u path_len=%u\n", data_len, key_len, path_len);

    /* Send to xtrieved */
    if (!tcp_send(request, req_len)) {
        printf("[-] TCP send failed\n");
        return FALSE;
    }

    /* Read response from xtrieved */
    /* [status:2][pos:128][dlen:4][data:N][klen:2][key:N] */

    /* Status */
    resp_status = read_u16_tcp();
    if (resp_status == 0xFFFF) return FALSE;
    response[resp_len++] = resp_status & 0xFF;
    response[resp_len++] = (resp_status >> 8) & 0xFF;

    /* Position block */
    if (!tcp_recv_exact(response + resp_len, POS_BLOCK_SIZE)) return FALSE;
    resp_len += POS_BLOCK_SIZE;

    /* Data length and data */
    resp_data_len = read_u32_tcp();
    if (resp_data_len == 0xFFFFFFFF) return FALSE;
    response[resp_len++] = resp_data_len & 0xFF;
    response[resp_len++] = (resp_data_len >> 8) & 0xFF;
    response[resp_len++] = (resp_data_len >> 16) & 0xFF;
    response[resp_len++] = (resp_data_len >> 24) & 0xFF;

    if (resp_data_len > 0) {
        if (!tcp_recv_exact(response + resp_len, resp_data_len)) return FALSE;
        resp_len += resp_data_len;
    }

    /* Key length and key */
    resp_key_len = read_u16_tcp();
    if (resp_key_len == 0xFFFF) return FALSE;
    response[resp_len++] = resp_key_len & 0xFF;
    response[resp_len++] = (resp_key_len >> 8) & 0xFF;

    if (resp_key_len > 0) {
        if (!tcp_recv_exact(response + resp_len, resp_key_len)) return FALSE;
        resp_len += resp_key_len;
    }

    printf("[<] Response: status=%u data_len=%lu\n", resp_status, resp_data_len);

    /* Send response back to BTRSERL via COM port */
    if (!serial_write_bytes(response, resp_len)) {
        printf("[-] Serial write failed\n");
        return FALSE;
    }

    g_request_count++;
    return TRUE;
}

/*---------------------------------------------------------------------------
 * Configuration
 *---------------------------------------------------------------------------*/

void load_config(void)
{
    char ini_path[MAX_PATH];
    char buffer[256];

    /* Get path to INI file (same directory as EXE) */
    GetModuleFileName(NULL, ini_path, MAX_PATH);
    strcpy(strrchr(ini_path, '\\') + 1, "XTRIEVE.INI");

    /* Read settings */
    GetPrivateProfileString("Server", "Address", DEFAULT_SERVER, g_server, sizeof(g_server), ini_path);
    g_port = GetPrivateProfileInt("Server", "Port", DEFAULT_PORT, ini_path);
    GetPrivateProfileString("COM", "Port", DEFAULT_COM_PORT, g_com_port, sizeof(g_com_port), ini_path);

    printf("[*] Config: %s -> %s:%d\n", g_com_port, g_server, g_port);
}

/*---------------------------------------------------------------------------
 * Main
 *---------------------------------------------------------------------------*/

int main(int argc, char *argv[])
{
    printf("===========================================\n");
    printf("  Xtrieve COM-to-TCP Bridge v1.0\n");
    printf("  For Windows 98SE\n");
    printf("===========================================\n\n");

    /* Load configuration */
    load_config();

    /* Allow command-line override */
    if (argc >= 2) {
        strcpy(g_com_port, argv[1]);
    }
    if (argc >= 3) {
        char *colon = strchr(argv[2], ':');
        if (colon) {
            *colon = '\0';
            strcpy(g_server, argv[2]);
            g_port = atoi(colon + 1);
        } else {
            strcpy(g_server, argv[2]);
        }
    }

    /* Initialize COM port */
    if (!serial_init(g_com_port)) {
        return 1;
    }

    /* Connect to xtrieved */
    if (!tcp_init(g_server, g_port)) {
        CloseHandle(hComPort);
        return 1;
    }

    printf("\n[*] Bridge ready - waiting for requests...\n\n");

    /* Main loop */
    while (1) {
        if (!process_request()) {
            printf("[-] Request failed, reconnecting...\n");

            /* Try to reconnect to xtrieved */
            closesocket(xtrieve_sock);
            if (!tcp_init(g_server, g_port)) {
                printf("[-] Reconnect failed, exiting\n");
                break;
            }
        }
    }

    /* Cleanup */
    closesocket(xtrieve_sock);
    WSACleanup();
    CloseHandle(hComPort);

    printf("\n[*] Bridge stopped. %lu requests processed.\n", g_request_count);
    return 0;
}
