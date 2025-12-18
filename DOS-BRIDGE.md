```

    ██╗  ██╗████████╗██████╗ ██╗███████╗██╗   ██╗███████╗
    ╚██╗██╔╝╚══██╔══╝██╔══██╗██║██╔════╝██║   ██║██╔════╝
     ╚███╔╝    ██║   ██████╔╝██║█████╗  ██║   ██║█████╗
     ██╔██╗    ██║   ██╔══██╗██║██╔══╝  ╚██╗ ██╔╝██╔══╝
    ██╔╝ ██╗   ██║   ██║  ██║██║███████╗ ╚████╔╝ ███████╗
    ╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝╚═╝╚══════╝  ╚═══╝  ╚══════╝

       ████████╗██╗███╗   ███╗███████╗    ██████╗ ██████╗ ██╗██████╗  ██████╗ ███████╗
       ╚══██╔══╝██║████╗ ████║██╔════╝    ██╔══██╗██╔══██╗██║██╔══██╗██╔════╝ ██╔════╝
          ██║   ██║██╔████╔██║█████╗      ██████╔╝██████╔╝██║██║  ██║██║  ███╗█████╗
          ██║   ██║██║╚██╔╝██║██╔══╝      ██╔══██╗██╔══██╗██║██║  ██║██║   ██║██╔══╝
          ██║   ██║██║ ╚═╝ ██║███████╗    ██████╔╝██║  ██║██║██████╔╝╚██████╔╝███████╗
          ╚═╝   ╚═╝╚═╝     ╚═╝╚══════╝    ╚═════╝ ╚═╝  ╚═╝╚═╝╚═════╝  ╚═════╝ ╚══════╝

                    ─═══════════════════════════════════════════─
                         B R I D G I N G   3 0   Y E A R S
                              1 9 9 4  ───────  2 0 2 5
                    ─═══════════════════════════════════════════─


     ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
     █                                                                       █
     █   "They said it couldn't be done. They said DOS was dead.            █
     █    They said Btrieve was obsolete. They were wrong."                 █
     █                                                                       █
     █                                        - Anonymous Hacker, 2025       █
     █                                                                       █
     ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀


                         ╔═══════════════════════════════╗
                         ║     W H A T   I S   T H I S   ║
                         ╚═══════════════════════════════╝

    A complete bridge that allows ORIGINAL, UNMODIFIED DOS Btrieve applications
    from the 1990s to run against a modern Rust database server in 2025.

    No recompilation. No source code changes. No emulation of Btrieve.
    Just pure interrupt hooking, serial wizardry, and protocol magic.



                     ╔═══════════════════════════════════════╗
                     ║     T H E   A R C H I T E C T U R E   ║
                     ╚═══════════════════════════════════════╝


                            ┌─────────────────────────────┐
                            │  ┌─────────────────────┐    │
                            │  │   DOS APPLICATION   │    │
                            │  │    (Turbo Pascal,   │    │
                            │  │     Clipper, C...)  │    │
                            │  └──────────┬──────────┘    │
                            │             │               │
                            │        INT 7Bh              │
                            │      (Btrieve Call)         │
                            │             │               │
                            │             ▼               │
                            │  ┌─────────────────────┐    │
                            │  │    BTRSERL.EXE      │    │
                            │  │   ┌─────────────┐   │    │
                            │  │   │  TSR ~7KB   │   │    │
                            │  │   │             │   │    │
                            │  │   │ Hooks INT7B │   │    │
                            │  │   │ Serializes  │   │    │
                            │  │   │ Protocol    │   │    │
                            │  │   └─────────────┘   │    │
                            │  └──────────┬──────────┘    │
                            │             │               │
                            │         COM1 TX/RX          │
                            │        @ 115200 baud        │
                            │             │               │
                            │  ╔══════════╧══════════╗    │
                            │  ║   DOSBOX-X SERIAL   ║    │
                            │  ║     NULL MODEM      ║    │
                            │  ╚══════════╤══════════╝    │
                            │             │               │
                            └─────────────┼───────────────┘
                                          │
                               ═══════════╪═══════════
                                   TCP/IP : 7418
                               ═══════════╪═══════════
                                          │
                            ┌─────────────┼───────────────┐
                            │             │               │
                            │  ╔══════════╧══════════╗    │
                            │  ║   SERIAL-BRIDGE     ║    │
                            │  ║   ┌─────────────┐   ║    │
                            │  ║   │    RUST     │   ║    │
                            │  ║   │             │   ║    │
                            │  ║   │ Sync Detect │   ║    │
                            │  ║   │ Protocol    │   ║    │
                            │  ║   │ Parser      │   ║    │
                            │  ║   └─────────────┘   ║    │
                            │  ╚══════════╤══════════╝    │
                            │             │               │
                            │        TCP/IP:7419          │
                            │             │               │
                            │  ╔══════════╧══════════╗    │
                            │  ║     XTRIEVED        ║    │
                            │  ║   ┌─────────────┐   ║    │
                            │  ║   │    RUST     │   ║    │
                            │  ║   │             │   ║    │
                            │  ║   │  Btrieve    │   ║    │
                            │  ║   │  5.x ISAM   │   ║    │
                            │  ║   │  Engine     │   ║    │
                            │  ║   └─────────────┘   ║    │
                            │  ╚══════════╤══════════╝    │
                            │             │               │
                            │             ▼               │
                            │  ┌─────────────────────┐    │
                            │  │   *.DAT FILES       │    │
                            │  │   (Native Btrieve   │    │
                            │  │    File Format)     │    │
                            │  └─────────────────────┘    │
                            │                             │
                            │      H O S T   S Y S T E M  │
                            │     (Linux/macOS/Windows)   │
                            └─────────────────────────────┘



                         ╔═══════════════════════════════╗
                         ║    T H E   P R O T O C O L    ║
                         ╚═══════════════════════════════╝


      ┌──────────────────────────────────────────────────────────────────┐
      │                     REQUEST  (DOS → XTRIEVE)                     │
      ├──────┬──────┬────────────┬──────────┬──────┬──────┬──────┬──────┤
      │ SYNC │  OP  │  POS_BLK   │   DATA   │  KEY │K_NUM │ PATH │ LOCK │
      │ 0xBB │  2   │    128     │  4+N     │ 2+N  │  2   │ 2+N  │  2   │
      │ 0xBB │bytes │   bytes    │  bytes   │bytes │bytes │bytes │bytes │
      └──────┴──────┴────────────┴──────────┴──────┴──────┴──────┴──────┘

      ┌──────────────────────────────────────────────────────────────────┐
      │                    RESPONSE  (XTRIEVE → DOS)                     │
      ├──────────┬──────────────┬────────────┬──────────────────────────┤
      │  STATUS  │   POS_BLK    │    DATA    │           KEY            │
      │    2     │     128      │    4+N     │           2+N            │
      │  bytes   │    bytes     │   bytes    │          bytes           │
      └──────────┴──────────────┴────────────┴──────────────────────────┘


            ┌─────────────────────────────────────────────────────┐
            │               SYNC MARKER: 0xBB 0xBB                │
            │                                                     │
            │  DOSBox-X sends garbage on serial connect.          │
            │  The bridge waits for 0xBB 0xBB before parsing.     │
            │  This allows recovery from any desync condition.    │
            │                                                     │
            │         ░░░░░░ → 0xBB → 0xBB → [VALID DATA]        │
            │         garbage   sync   sync   request begins      │
            └─────────────────────────────────────────────────────┘



                         ╔═══════════════════════════════╗
                         ║   S U P P O R T E D   O P S   ║
                         ╚═══════════════════════════════╝


              ╔════╦════════════════╦═══════════════════════════════╗
              ║ OP ║     NAME       ║         DESCRIPTION           ║
              ╠════╬════════════════╬═══════════════════════════════╣
              ║  0 ║ OPEN           ║ Open an existing file         ║
              ║  1 ║ CLOSE          ║ Close an open file            ║
              ║  2 ║ INSERT         ║ Insert a new record           ║
              ║  3 ║ UPDATE         ║ Update the current record     ║
              ║  4 ║ DELETE         ║ Delete the current record     ║
              ║  5 ║ GET_EQUAL      ║ Find record by key value      ║
              ║  6 ║ GET_NEXT       ║ Get next record in key order  ║
              ║  7 ║ GET_PREVIOUS   ║ Get previous record           ║
              ║  8 ║ GET_GREATER    ║ Get first record > key        ║
              ║  9 ║ GET_GT_OR_EQ   ║ Get first record >= key       ║
              ║ 10 ║ GET_LESS       ║ Get first record < key        ║
              ║ 11 ║ GET_LT_OR_EQ   ║ Get first record <= key       ║
              ║ 12 ║ GET_FIRST      ║ Get first record in file      ║
              ║ 13 ║ GET_LAST       ║ Get last record in file       ║
              ║ 14 ║ CREATE         ║ Create a new Btrieve file     ║
              ║ 15 ║ STAT           ║ Get file statistics           ║
              ║ 26 ║ VERSION        ║ Get Btrieve version info      ║
              ╚════╩════════════════╩═══════════════════════════════╝



                         ╔═══════════════════════════════╗
                         ║      Q U I C K   S T A R T    ║
                         ╚═══════════════════════════════╝


    ┌─────────────────────────────────────────────────────────────────────┐
    │  STEP 1: Configure DOSBox-X                                         │
    │  ─────────────────────────────                                      │
    │                                                                     │
    │    Add to dosbox-x.conf:                                           │
    │                                                                     │
    │    [serial]                                                         │
    │    serial1 = nullmodem server:127.0.0.1 port:7418                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    ┌─────────────────────────────────────────────────────────────────────┐
    │  STEP 2: Start Xtrieve Server                                       │
    │  ────────────────────────────                                       │
    │                                                                     │
    │    $ cd xtrieve                                                     │
    │    $ cargo run -p xtrieved                                         │
    │                                                                     │
    │    Xtrieve Record Manager Version 0.1.0                            │
    │    Btrieve 5.10 Compatible ISAM Database Engine                    │
    │    Listening on 127.0.0.1:7419                                     │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    ┌─────────────────────────────────────────────────────────────────────┐
    │  STEP 3: Start Serial Bridge                                        │
    │  ────────────────────────────                                       │
    │                                                                     │
    │    $ cd xtrieve/serial-bridge                                      │
    │    $ cargo run --release                                           │
    │                                                                     │
    │    ═══════════════════════════════════════════                     │
    │      Xtrieve Serial Bridge (Protocol-Aware)                        │
    │    ═══════════════════════════════════════════                     │
    │    Listening on port 7418 for DOSBox-X                             │
    │    [*] Waiting for DOS connections...                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    ┌─────────────────────────────────────────────────────────────────────┐
    │  STEP 4: Start DOSBox-X & Load TSR                                  │
    │  ─────────────────────────────────                                  │
    │                                                                     │
    │    C:\> BTRSERL                                                     │
    │                                                                     │
    │    BTRSERL v1.0 - Btrieve Serial Redirector                        │
    │                                                                     │
    │    Initializing COM1 (115200 baud)...                              │
    │    Installing INT 7B handler...                                     │
    │    Going resident.                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    ┌─────────────────────────────────────────────────────────────────────┐
    │  STEP 5: Run Your DOS Application!                                  │
    │  ───────────────────────────────────                                │
    │                                                                     │
    │    C:\> MYAPP.EXE                                                   │
    │                                                                     │
    │    Your 1990s Btrieve application now uses Xtrieve!                │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘



                         ╔═══════════════════════════════╗
                         ║    T H E   T S R   C O D E    ║
                         ╚═══════════════════════════════╝


                  ┌────────────────────────────────────────────┐
                  │                                            │
                  │   /* BTRSERL.C - The Heart of the Beast */ │
                  │                                            │
                  │   void interrupt new_int7b(                │
                  │       unsigned bp, unsigned di,            │
                  │       unsigned si, unsigned ds,            │
                  │       unsigned es, unsigned dx,            │
                  │       unsigned cx, unsigned bx,            │
                  │       unsigned ax, unsigned ip,            │
                  │       unsigned cs, unsigned flags)         │
                  │   {                                        │
                  │       BTR_PARMS far *parms;                │
                  │       parms = MK_FP(ds, dx);               │
                  │                                            │
                  │       if (parms->iface_id != 0x6176)       │
                  │           (*old_int7b)();  /* chain */     │
                  │                                            │
                  │       status = do_call(parms);             │
                  │       *(parms->stat_ptr) = status;         │
                  │   }                                        │
                  │                                            │
                  │   Compiled with Turbo C 2.0                │
                  │   TCC -ms BTRSERL.C                        │
                  │                                            │
                  └────────────────────────────────────────────┘



                         ╔═══════════════════════════════╗
                         ║     T E C H   S P E C S       ║
                         ╚═══════════════════════════════╝


                 ╔══════════════════════════════════════════════╗
                 ║  BTRSERL.EXE                                 ║
                 ╠══════════════════════════════════════════════╣
                 ║  Size:         7,262 bytes                   ║
                 ║  Resident:     ~2KB                          ║
                 ║  Compiler:     Turbo C 2.0                   ║
                 ║  Memory Model: Small (-ms)                   ║
                 ║  Baud Rate:    115200                        ║
                 ║  Serial Port:  COM1 (0x3F8)                  ║
                 ║  Interrupt:    7Bh                           ║
                 ╚══════════════════════════════════════════════╝

                 ╔══════════════════════════════════════════════╗
                 ║  serial-bridge                               ║
                 ╠══════════════════════════════════════════════╣
                 ║  Language:     Rust 2021                     ║
                 ║  Dependencies: None (pure std)               ║
                 ║  Listen Port:  7418                          ║
                 ║  Target Port:  7419 (xtrieved)               ║
                 ║  Sync Marker:  0xBB 0xBB                     ║
                 ╚══════════════════════════════════════════════╝

                 ╔══════════════════════════════════════════════╗
                 ║  xtrieved                                    ║
                 ╠══════════════════════════════════════════════╣
                 ║  Language:     Rust 2021                     ║
                 ║  Compatibility: Btrieve 5.10                 ║
                 ║  File Format:  Native Btrieve .DAT           ║
                 ║  Page Sizes:   512-4096 bytes                ║
                 ║  Key Types:    String, Integer, Float, etc.  ║
                 ╚══════════════════════════════════════════════╝



                         ╔═══════════════════════════════╗
                         ║        T I M E L I N E        ║
                         ╚═══════════════════════════════╝


    1973 ──●─────────────────────────────────────────────────────────────
           │  INGRES born at UC Berkeley (Stonebraker)
           │  └─ One of the first relational databases
           │
    1982 ──●─────────────────────────────────────────────────────────────
           │  Btrieve 1.0 released by SoftCraft
           │
    1986 ──●─────────────────────────────────────────────────────────────
           │  Btrieve 4.0 - INT 7Bh interface established
           │  Postgres development begins at Berkeley
           │  └─ "Post-INGRES" - the sequel
           │
    1990 ──●─────────────────────────────────────────────────────────────
           │  Btrieve 5.0 - The "Golden Era"
           │  └─ Millions of DOS apps built on Btrieve
           │
    1991 ──●─────────────────────────────────────────────────────────────
           │  ╔════════════════════════════════════════════════════╗
           │  ║  R&S BBS goes online in Higienópolis, São Paulo   ║
           │  ║  Retz & Stern, RemoteAccess, 2 lines, hand-coded  ║
           │  ║  ANSI, D&D sessions, and dreams of the future     ║
           │  ╚════════════════════════════════════════════════════╝
           │
    1994 ──●─────────────────────────────────────────────────────────────
           │  Btrieve 6.0 - Windows support added
           │  Postgres95 - SQL support added to Postgres
           │  └─ The birth of what becomes PostgreSQL
           │
    1996 ──●─────────────────────────────────────────────────────────────
           │  PostgreSQL gets its name
           │  └─ The open-source database revolution begins
           │
    1998 ──●─────────────────────────────────────────────────────────────
           │  Pervasive.SQL emerges from Btrieve
           │
    2000s ─●─────────────────────────────────────────────────────────────
           │  ╔════════════════════════════════════════════════════╗
           │  ║                                                    ║
           │  ║   dbExperts founded by Eduardo Stern               ║
           │  ║   Becomes THE PostgreSQL authority in Brazil       ║
           │  ║                                                    ║
           │  ║   From Btrieve on BBSes to PostgreSQL on servers   ║
           │  ║   The database journey continues                   ║
           │  ║                                                    ║
           │  ╚════════════════════════════════════════════════════╝
           │
    2003 ──●─────────────────────────────────────────────────────────────
           │  DOSBox released - DOS emulation begins
           │
    2015 ──●─────────────────────────────────────────────────────────────
           │  Rust 1.0 released
           │
    2020 ──●─────────────────────────────────────────────────────────────
           │  DOSBox-X adds enhanced serial support
           │
    2025 ──●─────────────────────────────────────────────────────────────
           │
           │  ╔════════════════════════════════════════════════════╗
           │  ║                                                    ║
           │  ║   XTRIEVE TIME BRIDGE COMPLETED                    ║
           │  ║                                                    ║
           │  ║   DOS Btrieve apps from 1990 running on            ║
           │  ║   Rust database server in 2025                     ║
           │  ║                                                    ║
           │  ║   THE FULL CIRCLE:                                 ║
           │  ║   Btrieve → PostgreSQL → Back to Btrieve           ║
           │  ║                                                    ║
           │  ║   34 YEARS OF DATABASE EXPERTISE UNIFIED           ║
           │  ║                                                    ║
           │  ╚════════════════════════════════════════════════════╝
           │



                         ╔═══════════════════════════════╗
                         ║       G R E E T I N G S       ║
                         ╚═══════════════════════════════╝


        ┌────────────────────────────────────────────────────────────┐
        │                                                            │
        │   To all the DOS programmers who came before...            │
        │   To the Turbo C hackers and Clipper developers...         │
        │   To everyone who wrote "Press any key to continue"...     │
        │   To the BBS sysops and the demoscene coders...            │
        │                                                            │
        │   Your code STILL RUNS.                                    │
        │                                                            │
        │                       ░░░░░░░░░░░░░░░░░░░░░░░░░              │
        │                       ░    R E S P E C T    ░              │
        │                       ░░░░░░░░░░░░░░░░░░░░░░░░░              │
        │                                                            │
        └────────────────────────────────────────────────────────────┘



          ╔═══════════════════════════════════════════════════════════╗
          ║                                                           ║
          ║    B A C K   I N   T H E   D A Y . . .   ( 1 9 9 1 )     ║
          ║                                                           ║
          ║         "The Upside Down Was Just a Phone Call Away"      ║
          ║                                                           ║
          ╚═══════════════════════════════════════════════════════════╝



    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │    ┌────────────────────────────────────────────────────────────┐    │
    │    │  ░░░░░░ REALISTIC 2400 MODEM ░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │    │
    │    │  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  │    │
    │    │  █  RD   TD   CD   OH   AA   TR   SD   HS   █   PWR    █  │    │
    │    │  █  ●    ●    ○    ○    ○    ○    ●    ○    █          █  │    │
    │    │  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  │    │
    │    └────────────────────────────────────────────────────────────┘    │
    │                                                                      │
    │         2400 bps. No MNP5. No V.42bis. No error correction.          │
    │         Analog phone line. Pulse dial. Click-click-click-click.      │
    │         Wait for dial tone. Hope nobody picks up the extension.      │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘


                            T H E   S O U N D S
                            ───────────────────

              ╔══════════════════════════════════════════════════╗
              ║                                                  ║
              ║   ATDT 555-1234                                  ║
              ║                                                  ║
              ║   ♪ beeeeeeep ♪                                  ║
              ║   ♪ BONG... BONG... BONG... BONG... ♪            ║
              ║                                                  ║
              ║   ... ring ... ring ...                          ║
              ║                                                  ║
              ║   ♪ EEEEEEEE-KSSSHHHHHH-BONG-KSSSHHH ♪           ║
              ║   ♪ eeee-KSSSHHH-bweeeee-KSSSHHHHHHH ♪           ║
              ║                                                  ║
              ║   CONNECT 2400                                   ║
              ║                                                  ║
              ║   The most beautiful sound in the world.         ║
              ║                                                  ║
              ╚══════════════════════════════════════════════════╝



                         T H E   B B S   E R A
                         ─────────────────────

    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │     ╔════════════════════════════════════════════════════════╗       │
    │     ║           WELCOME TO THE TWILIGHT ZONE BBS             ║       │
    │     ║          ════════════════════════════════════          ║       │
    │     ║    Running RemoteAccess 2.50 on a 386DX/40 + 4MB RAM   ║       │
    │     ║              USRobotics Sportster 14.4                 ║       │
    │     ║                 Node 1 of 1 Lines                      ║       │
    │     ╠════════════════════════════════════════════════════════╣       │
    │     ║                                                        ║       │
    │     ║   ┌─────────────────────────────────────────────┐      ║       │
    │     ║   │ [F]ile Areas      [M]essage Bases           │      ║       │
    │     ║   │ [D]oor Games      [B]ulletins               │      ║       │
    │     ║   │ [C]hat with SysOp [U]ser List               │      ║       │
    │     ║   │ [S]tatistics      [G]oodbye/Logoff          │      ║       │
    │     ║   └─────────────────────────────────────────────┘      ║       │
    │     ║                                                        ║       │
    │     ║   Time Left: 45 min    Calls Today: 23                 ║       │
    │     ║                                                        ║       │
    │     ║   Select: _                                            ║       │
    │     ║                                                        ║       │
    │     ╚════════════════════════════════════════════════════════╝       │
    │                                                                      │
    │     This wasn't a terminal. This LOOKED like DOS.                    │
    │     Hand-written ANSI ESC codes. Every color, every position.        │
    │     printf("\x1B[1;37;44m"); /* Bright white on blue */              │
    │     printf("\x1B[12;35H");   /* Row 12, Column 35 */                 │
    │     No TheDraw. No ACiDDraw. Just raw escape sequences.              │
    │                                                                      │
    │     RemoteAccess with keyboard menus. F-keys worked.                 │
    │     Callers thought they were running a local DOS program.           │
    │     That was the magic.                                              │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



                       T H E   B B S   S O F T W A R E
                       ───────────────────────────────

              ╔════════════════════════════════════════════════╗
              ║                                                ║
              ║   PCBoard .......... The Professional Choice   ║
              ║   RemoteAccess ..... The Customizer's Dream    ║
              ║   Wildcat! ......... The Friendly One          ║
              ║   TBBS ............. The Serious Business      ║
              ║   Maximus .......... The FidoNet Native        ║
              ║   Opus ............. Where FOSSIL Was Born     ║
              ║   RBBS-PC .......... The Pioneer (1983!)       ║
              ║   Searchlight ...... The Database BBS          ║
              ║   TriBBS ........... The Three-Node Wonder     ║
              ║   Renegade ......... The Underground           ║
              ║                                                ║
              ╚════════════════════════════════════════════════╝



                          T H E   D O O R S
                          ─────────────────

    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │                 ══════════════════════════════════                   │
    │                   L E G E N D   O F   T H E                          │
    │                     R E D   D R A G O N                              │
    │                 ══════════════════════════════════                   │
    │                                                                      │
    │     The Dark Cloak Tavern                                            │
    │     ─────────────────────                                            │
    │                                                                      │
    │     Seth Able the Bard sings of your adventures.                     │
    │     You have 3 forest fights remaining today.                        │
    │                                                                      │
    │     [V]iew Stats  [F]orest  [S]laughter Others  [Q]uit               │
    │                                                                      │
    │   ──────────────────────────────────────────────────────────────     │
    │                                                                      │
    │     Door games. The reason to call back tomorrow.                    │
    │                                                                      │
    │     TradeWars 2002 ....... Space Trading Empire                      │
    │     Barren Realms Elite .. Kingdom Conquest                          │
    │     Usurper .............. Fantasy Combat                            │
    │     Operation: Overkill .. Post-Apocalyptic RPG                      │
    │     Food Fight ........... Just Pure Chaos                           │
    │     Global War ........... Nuclear Diplomacy                         │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



                    F I D O N E T   -   T H E   F I R S T
                          S O C I A L   N E T W O R K
                    ─────────────────────────────────────

              ┌────────────────────────────────────────────────┐
              │                                                │
              │         ┌──────────────────────────┐           │
              │         │  ██████╗ ██╗██████╗  ██████╗         │
              │         │  █╔════╝ ██║██╔══██╗██╔═══██╗        │
              │         │  █████╗  ██║██║  ██║██║   ██║        │
              │         │  █╔══╝   ██║██║  ██║██║   ██║        │
              │         │  █║      ██║██████╔╝╚██████╔╝        │
              │         │  ╚╝      ╚═╝╚═════╝  ╚═════╝         │
              │         │      N   E   T                       │
              │         └──────────────────────────┘           │
              │                                                │
              │    Zone:Net/Node.Point                         │
              │    1:105/42.0                                  │
              │                                                │
              │    Echomail: Global discussions before         │
              │    the Internet made it easy.                  │
              │                                                │
              │    NetMail: Person to person, routed           │
              │    through nodes at 2:00 AM when               │
              │    long distance was cheap.                    │
              │                                                │
              │    The FOSSIL driver made it possible.         │
              │    X00 and BNU were the backbone.              │
              │    FrontDoor polling through the night.        │
              │                                                │
              └────────────────────────────────────────────────┘



                        T H E   A N S I   A R T
                        ───────────────────────

    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │     CONFIG.SYS:                                                      │
    │     ───────────                                                      │
    │     DEVICE=C:\DOS\ANSI.SYS                                           │
    │                                                                      │
    │     Without this line, the BBS was just text.                        │
    │     With it, the BBS was ART.                                        │
    │                                                                      │
    │   ──────────────────────────────────────────────────────────────     │
    │                                                                      │
    │     TheDraw 4.63 ....... The Standard                                │
    │     ACiDDraw ........... The Scene Tool                              │
    │     PabloDraw .......... The Modern Revival                          │
    │                                                                      │
    │     Or... you just wrote the escape codes by hand:                   │
    │                                                                      │
    │     \x1B[0m      Reset                                               │
    │     \x1B[1m      Bright/Bold                                         │
    │     \x1B[5m      Blink (the controversy!)                            │
    │     \x1B[30-37m  Foreground color                                    │
    │     \x1B[40-47m  Background color                                    │
    │     \x1B[row;colH Cursor position                                    │
    │     \x1B[2J     Clear screen                                         │
    │     \x1B[K      Clear to end of line                                 │
    │                                                                      │
    │     Every byte mattered at 2400 bps.                                 │
    │     Every color was a choice.                                        │
    │     Every screen was a canvas.                                       │
    │                                                                      │
    │     The ANSI art groups:                                             │
    │     ACiD Productions ... iCE ... DARK ... TRiBE ... FUEL             │
    │                                                                      │
    │     Monthly artpacks. .NFO files. Scene drama.                       │
    │     A whole culture transmitted at 300 chars/second.                 │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



                     T H E   D O W N L O A D   A R E A
                     ─────────────────────────────────

              ╔════════════════════════════════════════════════╗
              ║                                                ║
              ║   File Area 23: GAMES - SHAREWARE              ║
              ║   ═══════════════════════════════              ║
              ║                                                ║
              ║   DOOM.ZIP ...... 2,365,478 bytes .. 16 min    ║
              ║   WOLF3D.ZIP .... 1,234,567 bytes ..  8 min    ║
              ║   COMMANDER.ZIP .   456,789 bytes ..  3 min    ║
              ║                                                ║
              ║   Download DOOM at 2400 bps?                   ║
              ║   That's 16 minutes if nobody picks up         ║
              ║   the phone. Better use ZMODEM with resume.    ║
              ║                                                ║
              ║   Protocols:                                   ║
              ║   [X]Modem ... The Original                    ║
              ║   [Y]Modem ... The Batch                       ║
              ║   [Z]Modem ... The King (with resume!)         ║
              ║   [K]ermit ... The Academic                    ║
              ║                                                ║
              ║   "Your download will continue where           ║
              ║    it left off next time you call."            ║
              ║                                                ║
              ║   ZMODEM resume saved HOURS of our lives.      ║
              ║                                                ║
              ╚════════════════════════════════════════════════╝



                        T H E   T E R M I N A L S
                        ─────────────────────────

              ┌────────────────────────────────────────────────┐
              │                                                │
              │   TELIX 3.15 ........ The Power User's Choice  │
              │   Procomm Plus ...... The Corporate Standard   │
              │   Qmodem ............ The Feature-Rich         │
              │   Terminate ......... The Memory Master        │
              │   Telemate .......... The Windows of DOS       │
              │   BitCom ............ The Simple One           │
              │   COM.EXE ........... You're hardcore          │
              │                                                │
              │   Dialing directory. Script language.          │
              │   Capture buffer. Scroll-back.                 │
              │   The tools of the night owl.                  │
              │                                                │
              └────────────────────────────────────────────────┘



                     ═══════════════════════════════════
                          T H E   C O N N E C T I O N
                     ═══════════════════════════════════

    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │      X00.SYS loaded. Or BNU.COM. The FOSSIL drivers.                 │
    │      INT 14h hooked. Serial port abstracted.                         │
    │      FidoNet mailer ready. Polling at 2 AM.                          │
    │                                                                      │
    │      BTRSERL.EXE does the same thing.                                │
    │      INT 7Bh hooked. Btrieve abstracted.                             │
    │      Xtrieve server ready. Protocol flowing.                         │
    │                                                                      │
    │      ╔══════════════════════════════════════════════════════════╗    │
    │      ║                                                          ║    │
    │      ║   1991: X00/BNU hook INT 14h for serial abstraction      ║    │
    │      ║   2025: BTRSERL hooks INT 7Bh for database abstraction   ║    │
    │      ║                                                          ║    │
    │      ║   Same pattern. Same TSR magic. Same spirit.             ║    │
    │      ║                                                          ║    │
    │      ║   The FOSSIL philosophy lives on.                        ║    │
    │      ║                                                          ║    │
    │      ╚══════════════════════════════════════════════════════════╝    │
    │                                                                      │
    │      Now DOSBox-X nullmodem gives us 115200 baud virtual serial      │
    │      over TCP/IP. From Realistic modems to virtual serial ports.     │
    │      From pulse dial to TCP sockets.                                 │
    │                                                                      │
    │                ╔══════════════════════════════════════╗              │
    │                ║  SERIAL IS SERIAL IS SERIAL IS LIFE  ║              │
    │                ╚══════════════════════════════════════╝              │
    │                                                                      │
    │      The bits still flow. The protocols still work.                  │
    │      The TSRs still hook interrupts.                                 │
    │      Some things never change.                                       │
    │                                                                      │
    │      From BBSes to databases. From FidoNet to Xtrieve.               │
    │      The architecture endures.                                       │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



                    ╔═══════════════════════════════════════════╗
                    ║                                           ║
                    ║          D E D I C A T E D   T O          ║
                    ║                                           ║
                    ╚═══════════════════════════════════════════╝


    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │                                                                      │
    │           ██████╗        ██╗       ███████╗                          │
    │           ██╔══██╗      ██╔╝       ██╔════╝                          │
    │           ██████╔╝     ██╔╝        ███████╗                          │
    │           ██╔══██╗    ██╔╝              ██║                          │
    │           ██║  ██║   ██╔╝          ███████║                          │
    │           ╚═╝  ╚═╝   ╚═╝           ╚══════╝                          │
    │                                                                      │
    │                    ══════════════════════════                        │
    │                          B   B   S                                   │
    │                    ══════════════════════════                        │
    │                                                                      │
    │                                                                      │
    │     2 Lines. RemoteAccess. Higienópolis, São Paulo.                  │
    │                                                                      │
    │     Co-SysOps:                                                       │
    │       R - Retz   (Renato Retz de Carvalho)                           │
    │       S - Stern  (Eduardo Stern)                                     │
    │                                                                      │
    │     Running from Renato's grandmother's apartment.                   │
    │     Hand-coded ANSI menus that looked like DOS, not a terminal.      │
    │     Keyboard navigation. F-keys that worked.                         │
    │                                                                      │
    │     Made no money.                                                   │
    │     But it was fun.                                                  │
    │                                                                      │
    │     ┌──────────────────────────────────────────────────────────┐     │
    │     │                                                          │     │
    │     │   That's why we do this.                                 │     │
    │     │   Not for the money. Not for the glory.                  │     │
    │     │   For the pure joy of making things work.                │     │
    │     │                                                          │     │
    │     │   From R&S BBS in 1991 to Xtrieve in 2025.               │     │
    │     │   The spirit never died.                                 │     │
    │     │                                                          │     │
    │     └──────────────────────────────────────────────────────────┘     │
    │                                                                      │
    │                                                                      │
    │     To Renato, wherever you are.                                     │
    │     We're still hooking interrupts.                                  │
    │     And rolling d20s in our hearts.                                  │
    │                                                                      │
    │                                                                      │
    │              T H E   D U N G E O N   M A S T E R                     │
    │              ───────────────────────────────────                     │
    │                                                                      │
    │         Renato played D&D. The original nerd credential.             │
    │         Before it was cool. Before Stranger Things.                  │
    │         When being a geek meant something.                           │
    │                                                                      │
    │                                                                      │
    │                        .     .                                       │
    │                       /(     )\                                      │
    │                      (  \   /  )                                     │
    │                       \  \ /  /                                      │
    │                    ____\     /____                                   │
    │                   /    .\   /.    \                                  │
    │                  /   .` |   | `.   \                                 │
    │                 /   /   |   |   \   \                                │
    │                 |  |  __|   |__  |  |                                │
    │                 |  | /  \   /  \ |  |                                │
    │                 |  |/    \_/    \|  |                                │
    │                  \  \     ^     /  /                                 │
    │                   \  \   /|\   /  /                                  │
    │                    \  \_/ | \_/  /                                   │
    │                     \____/|\____/                                    │
    │                          |||                                         │
    │                          |||                                         │
    │                         /|||\                                        │
    │                        / ||| \                                       │
    │                       /  |||  \                                      │
    │                      /___|_|___\                                     │
    │                                                                      │
    │                    D E M O G O R G O N                               │
    │             Prince of Demons, CR 26, TPK Guaranteed                  │
    │                                                                      │
    │                                                                      │
    │    ┌────────────────────────────────────────────────────────────┐    │
    │    │                                                            │    │
    │    │      ╔══════════════════════════════════════════════╗      │    │
    │    │      ║                                              ║      │    │
    │    │      ║    "You enter the dungeon. Roll initiative." ║      │    │
    │    │      ║                                              ║      │    │
    │    │      ║    [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]   ║      │    │
    │    │      ║     d4  d6  d8 d10 d12 d20 d20 d20 d20 d%    ║      │    │
    │    │      ║                                              ║      │    │
    │    │      ║    THAC0: 12    AC: -8    HP: ███████░░░     ║      │    │
    │    │      ║                                              ║      │    │
    │    │      ╚══════════════════════════════════════════════╝      │    │
    │    │                                                            │    │
    │    │    From rolling dice in Higienópolis basements             │    │
    │    │    to rolling commits on GitHub.                           │    │
    │    │                                                            │    │
    │    │    The adventure continues.                                │    │
    │    │                                                            │    │
    │    └────────────────────────────────────────────────────────────┘    │
    │                                                                      │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



                         ╔═══════════════════════════════╗
                         ║        C R E D I T S          ║
                         ╚═══════════════════════════════╝


                    ╭──────────────────────────────────────╮
                    │                                      │
                    │   C O D E   &   D E S I G N          │
                    │   ───────────────────────            │
                    │                                      │
                    │   Claude (Anthropic)                 │
                    │   Eduardo                            │
                    │                                      │
                    │   B U I L T   W I T H                │
                    │   ─────────────────                  │
                    │                                      │
                    │   Rust ............ xtrieved        │
                    │   Rust ............ serial-bridge   │
                    │   Turbo C 2.0 ..... BTRSERL.EXE     │
                    │   DOSBox-X ........ Emulation       │
                    │   Claude Code ..... AI Pair Prog    │
                    │   Telix 3.15 ...... Terminal        │
                    │                                      │
                    │   I N S P I R E D   B Y              │
                    │   ─────────────────                  │
                    │                                      │
                    │   R&S BBS, Higienópolis (1991)      │
                    │   dbExperts - PostgreSQL Brasil     │
                    │   INGRES & PostgreSQL (Berkeley)    │
                    │   Btrieve Technologies (RIP)        │
                    │   Borland International (RIP)       │
                    │   RemoteAccess / PCBoard / TBBS     │
                    │   The DOS Era (1981-1995)           │
                    │   The Demoscene & ANSI Art Scene    │
                    │   2400bps Modems (RIP)              │
                    │   X00/BNU FOSSIL Drivers            │
                    │   FidoNet (Zone 4: South America)   │
                    │   Legend of the Red Dragon          │
                    │   Michael Stonebraker (DB pioneer)  │
                    │   All the SysOps who stayed up      │
                    │   waiting for that 2 AM poll        │
                    │                                      │
                    ╰──────────────────────────────────────╯



     ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
     █                                                                       █
     █     "In the beginning, there was INT 21h. And it was good.           █
     █      Then came INT 7Bh for Btrieve. And business apps flourished.    █
     █      Then came PostgreSQL. And the web was built upon it.            █
     █                                                                       █
     █      Now, 34 years later, we bridge the gap with Rust and TCP/IP.    █
     █      The old code runs again. The data lives on.                     █
     █                                                                       █
     █      From R&S BBS to dbExperts to Xtrieve.                           █
     █      From Btrieve to PostgreSQL and back again.                      █
     █      The full circle of a database engineer's life.                  █
     █                                                                       █
     █      This is the way."                                               █
     █                                                                       █
     ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀



                 ████████╗██╗  ██╗███████╗    ███████╗███╗   ██╗██████╗
                 ╚══██╔══╝██║  ██║██╔════╝    ██╔════╝████╗  ██║██╔══██╗
                    ██║   ███████║█████╗      █████╗  ██╔██╗ ██║██║  ██║
                    ██║   ██╔══██║██╔══╝      ██╔══╝  ██║╚██╗██║██║  ██║
                    ██║   ██║  ██║███████╗    ███████╗██║ ╚████║██████╔╝
                    ╚═╝   ╚═╝  ╚═╝╚══════╝    ╚══════╝╚═╝  ╚═══╝╚═════╝


                              December 2025

                    https://github.com/eduardostern/Xtrieve


─────────────────────────────────────────────────────────────────────────────
```
