/* BTRSERL.C - Minimal Btrieve-to-Serial TSR */
/* Turbo C 2.0: TCC -ms BTRSERL.C */
/* Pure C - no TASM required */

#include <dos.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define COM1_BASE 0x3F8
#define COM1_DATA (COM1_BASE + 0)
#define COM1_LSR  (COM1_BASE + 5)
#define LSR_DATA_READY 0x01
#define LSR_TX_EMPTY   0x20

#define POS_BLOCK_SIZE 128
#define TIMEOUT 30000U

/* Old INT 7B handler */
void interrupt (*old_int7b)(void);

/* ===== Serial I/O ===== */

void serial_init(void)
{
    outportb(COM1_BASE + 1, 0x00);
    outportb(COM1_BASE + 3, 0x80);
    outportb(COM1_BASE + 0, 0x01);
    outportb(COM1_BASE + 1, 0x00);
    outportb(COM1_BASE + 3, 0x03);
    outportb(COM1_BASE + 2, 0xC7);
    outportb(COM1_BASE + 4, 0x0B);
}

void ser_putc(unsigned char c)
{
    unsigned int t = TIMEOUT;
    while (!(inportb(COM1_LSR) & LSR_TX_EMPTY) && t--) ;
    outportb(COM1_DATA, c);
}

int ser_getc(void)
{
    unsigned int t = TIMEOUT;
    while (!(inportb(COM1_LSR) & LSR_DATA_READY) && t--) ;
    if (inportb(COM1_LSR) & LSR_DATA_READY)
        return inportb(COM1_DATA);
    return -1;
}

void send_u16(unsigned int v)
{
    ser_putc(v & 0xFF);
    ser_putc((v >> 8) & 0xFF);
}

void send_u32(unsigned long v)
{
    ser_putc(v & 0xFF);
    ser_putc((v >> 8) & 0xFF);
    ser_putc((v >> 16) & 0xFF);
    ser_putc((v >> 24) & 0xFF);
}

unsigned int recv_u16(void)
{
    int lo = ser_getc();
    int hi = ser_getc();
    if (lo < 0 || hi < 0) return 0xFFFF;
    return (lo & 0xFF) | ((hi & 0xFF) << 8);
}

unsigned long recv_u32(void)
{
    unsigned long v = 0;
    int i, c;
    for (i = 0; i < 4; i++) {
        c = ser_getc();
        if (c < 0) return 0xFFFFFFFFUL;
        v |= ((unsigned long)(c & 0xFF) << (i * 8));
    }
    return v;
}

/* ===== Btrieve Parameter Block ===== */
typedef struct {
    void far *data_buf;
    unsigned int data_len;
    void far *pos_blk;
    void far *fcb;
    unsigned int operation;
    void far *key_buf;
    unsigned char key_len;
    char key_num;
    int far *stat_ptr;
    unsigned int iface_id;
} BTR_PARMS;

/* ===== Process one Btrieve call ===== */
int do_call(BTR_PARMS far *p)
{
    unsigned int i, dlen, klen, plen;
    unsigned long resp_dlen;
    unsigned int resp_klen;
    unsigned int status;
    unsigned char far *src;
    unsigned char far *dst;
    char path[80];

    plen = 0;
    if ((p->operation == 0 || p->operation == 14) && p->key_buf != NULL) {
        src = (unsigned char far *)p->key_buf;
        while (src[plen] && plen < 79) {
            path[plen] = src[plen];
            plen++;
        }
        path[plen] = 0;
    }

    dlen = p->data_len;
    klen = 80;

    /* SEND REQUEST */
    /* Sync marker: 0xBB 0xBB (easily identifiable) */
    ser_putc(0xBB);
    ser_putc(0xBB);
    send_u16(p->operation);

    src = (unsigned char far *)p->pos_blk;
    for (i = 0; i < POS_BLOCK_SIZE; i++)
        ser_putc(src ? src[i] : 0);

    send_u32((unsigned long)dlen);
    src = (unsigned char far *)p->data_buf;
    for (i = 0; i < dlen; i++)
        ser_putc(src ? src[i] : 0);

    send_u16(klen);
    src = (unsigned char far *)p->key_buf;
    for (i = 0; i < klen; i++)
        ser_putc(src ? src[i] : 0);

    send_u16((unsigned int)(p->key_num & 0xFF));

    send_u16(plen);
    for (i = 0; i < plen; i++)
        ser_putc(path[i]);

    send_u16(0);

    /* RECEIVE RESPONSE */
    status = recv_u16();
    if (status == 0xFFFF) return 20;

    dst = (unsigned char far *)p->pos_blk;
    for (i = 0; i < POS_BLOCK_SIZE; i++) {
        int c = ser_getc();
        if (c < 0) return 20;
        if (dst) dst[i] = (unsigned char)c;
    }

    resp_dlen = recv_u32();
    if (resp_dlen == 0xFFFFFFFFUL) return 20;
    dst = (unsigned char far *)p->data_buf;
    for (i = 0; i < (unsigned int)resp_dlen; i++) {
        int c = ser_getc();
        if (c < 0) return 20;
        if (dst && i < p->data_len) dst[i] = (unsigned char)c;
    }
    p->data_len = (unsigned int)resp_dlen;

    resp_klen = recv_u16();
    if (resp_klen == 0xFFFF) return 20;
    dst = (unsigned char far *)p->key_buf;
    for (i = 0; i < resp_klen; i++) {
        int c = ser_getc();
        if (c < 0) return 20;
        if (dst && i < 80) dst[i] = (unsigned char)c;
    }

    return status;
}

/* ===== INT 7B Handler ===== */
/* Turbo C register order: BP, DI, SI, DS, ES, DX, CX, BX, AX, IP, CS, FLAGS */
void interrupt new_int7b(
    unsigned bp, unsigned di, unsigned si,
    unsigned ds, unsigned es, unsigned dx,
    unsigned cx, unsigned bx, unsigned ax,
    unsigned ip, unsigned cs, unsigned flags)
{
    BTR_PARMS far *parms;
    int status;

    /* DS:DX contains pointer to parameter block */
    parms = (BTR_PARMS far *)MK_FP(ds, dx);

    /* Check for Btrieve interface ID */
    if (parms->iface_id != 0x6176) {
        /* Chain to old handler */
        (*old_int7b)();
        return;
    }

    /* Process the call */
    status = do_call(parms);

    /* Store status */
    if (parms->stat_ptr != NULL) {
        *(parms->stat_ptr) = status;
    }
}

/* End of resident portion marker */
void end_of_resident(void) {}

/* ===== Main ===== */
int main(int argc, char *argv[])
{
    unsigned int para;

    printf("BTRSERL v1.0 - Btrieve Serial Redirector\n\n");

    if (argc > 1 && strcmp(argv[1], "/?") == 0) {
        printf("Hooks INT 7B, sends Btrieve calls to COM1\n\n");
        printf("DOSBox-X config:\n");
        printf("  serial1=nullmodem server:127.0.0.1 port:7418\n");
        return 0;
    }

    old_int7b = getvect(0x7B);

    printf("Initializing COM1 (115200 baud)...\n");
    serial_init();

    printf("Installing INT 7B handler...\n");
    setvect(0x7B, new_int7b);

    printf("Going resident.\n");

    /* Resident size: use _SS and _SP to calculate end of program */
    para = (_SS + ((_SP + 100) >> 4)) - _psp;

    keep(0, para);

    return 0;
}
