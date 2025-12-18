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
        │                  ░░░░░░░░░░░░░░░░░░░░░░░                   │
        │                  ░    R E S P E C T    ░                   │
        │                  ░░░░░░░░░░░░░░░░░░░░░░░                   │
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
    │    │  █  RD   TD   CD   OH   AA   TR   SD   HS   █   PWR    █   │    │
    │    │  █  ●    ●    ○    ○    ○    ○    ●    ○    █          █   │    │
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
              │       ┌──────────────────────────────┐         │
              │       │  ██████╗ ██╗██████╗  ██████╗ │         │
              │       │  █╔════╝ ██║██╔══██╗██╔═══██╗│         │
              │       │  █████╗  ██║██║  ██║██║   ██║│         │
              │       │  █╔══╝   ██║██║  ██║██║   ██║│         │
              │       │  █║      ██║██████╔╝╚██████╔╝│         │
              │       │  ╚╝      ╚═╝╚═════╝  ╚═════╝ │         │
              │       │         N   E   T            │         │
              │       └──────────────────────────────┘         │
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



                    T H E   D I G I T A L   C A M E L Ô
                     ─────────────────────────────────
                         1999: The MP3 Revolution

    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │     The BBS era was ending. A new era was beginning.                 │
    │     Napster. Audiogalaxy. Kazaa. Limewire.                           │
    │     Music wanted to be free. And we were the liberators.             │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │   THE SETUP:                                               │   │
    │     │                                                            │   │
    │     │   ┌────────────────────────────────────────────────────┐   │   │
    │     │   │  ╔══════════════════════════════════════════════╗  │   │   │
    │     │   │  ║            MEGA TOWER GABINETE               ║  │   │   │
    │     │   │  ║  ┌──────┐ ┌──────┐ ┌──────┐                  ║  │   │   │
    │     │   │  ║  │ CD-RW│ │ CD-RW│ │ CD-RW│  ← Burner #1-3   ║  │   │   │
    │     │   │  ║  │  4x  │ │  4x  │ │  4x  │    (SCSI chain)  ║  │   │   │
    │     │   │  ║  └──────┘ └──────┘ └──────┘                  ║  │   │   │
    │     │   │  ║  ┌──────┐ ┌──────┐ ┌──────┐                  ║  │   │   │
    │     │   │  ║  │ CD-RW│ │ CD-RW│ │ CD-RW│  ← Burner #4-6   ║  │   │   │
    │     │   │  ║  │  4x  │ │  4x  │ │  4x  │    (IDE chain)   ║  │   │   │
    │     │   │  ║  └──────┘ └──────┘ └──────┘                  ║  │   │   │
    │     │   │  ║                                              ║  │   │   │
    │     │   │  ║  [████████████████████████] 6x HDD array     ║  │   │   │
    │     │   │  ║                                              ║  │   │   │
    │     │   │  ║  POWER SUPPLY: 500W                          ║  │   │   │
    │     │   │  ║  (the real kind, not Paraguay fake)          ║  │   │   │
    │     │   │  ╚══════════════════════════════════════════════╝  │   │   │
    │     │   │              │││││││││││││││                       │   │   │
    │     │   │           CABLES EVERYWHERE                        │   │   │
    │     │   └────────────────────────────────────────────────────┘   │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     THE PROCESS:                                                     │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  STEP 1: Download on 256kbps cable (NET VIRTUAL!)           │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────┐            │   │
    │     │    │                                          │            │   │
    │     │    │  ╔══════════════════════════════════════╗│            │   │
    │     │    │  ║  NET VIRTUAL - 256kbps Cable Modem  ║│            │   │
    │     │    │  ║  (Later: Telmex → Embratel → RIP)   ║│            │   │
    │     │    │  ╚══════════════════════════════════════╝│            │   │
    │     │    │                                          │            │   │
    │     │    │  256kbps felt like THE FUTURE.           │            │   │
    │     │    │  No more "don't pick up the phone."      │            │   │
    │     │    │  Always on. Always downloading.          │            │   │
    │     │    │                                          │            │   │
    │     │    └──────────────────────────────────────────┘            │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────┐            │   │
    │     │    │  Napster: Downloading...                 │            │   │
    │     │    │                                          │            │   │
    │     │    │  01. Metallica - Enter Sandman.mp3       │            │   │
    │     │    │      [████████████████████] DONE - 28KB/s│            │   │
    │     │    │      STATUS: CORRUPTED (as usual)        │            │   │
    │     │    │                                          │            │   │
    │     │    │  02. Nirvana - Smells Like Teen Spiri... │            │   │
    │     │    │      [████████████████████] DONE         │            │   │
    │     │    │      STATUS: Actually "I Did Not Have    │            │   │
    │     │    │              Sexual Relations..."        │            │   │
    │     │    │                                          │            │   │
    │     │    │  03. Backstreet Boys - I Want It Th...   │            │   │
    │     │    │      [████████████████████] DONE         │            │   │
    │     │    │      STATUS: Virus (ILOVEYOU.vbs)        │            │   │
    │     │    │                                          │            │   │
    │     │    └──────────────────────────────────────────┘            │   │
    │     │                                                            │   │
    │     │    THE NAPSTER REALITY:                                    │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────┐            │   │
    │     │    │  WHAT YOU EXPECTED:                      │            │   │
    │     │    │  Metallica - Master of Puppets.mp3       │            │   │
    │     │    │                                          │            │   │
    │     │    │  WHAT YOU ACTUALLY GOT:                  │            │   │
    │     │    │  ☒ 70% - The actual song (if lucky)      │            │   │
    │     │    │  ☒ 15% - Corrupted halfway through       │            │   │
    │     │    │  ☒  5% - Wrong song entirely             │            │   │
    │     │    │  ☒  5% - Bill Clinton speech             │            │   │
    │     │    │  ☒  3% - Virus pretending to be MP3      │            │   │
    │     │    │  ☒  2% - Rickroll (before it was cool)   │            │   │
    │     │    │                                          │            │   │
    │     │    │  Files always came corrupted. ALWAYS.    │            │   │
    │     │    │  You'd burn 12 tracks, 3 would skip.     │            │   │
    │     │    │  Quality control was... aspirational.    │            │   │
    │     │    └──────────────────────────────────────────┘            │   │
    │     │                                                            │   │
    │     │  STEP 2: Organize into "Best of 1999" folders              │   │
    │     │                                                            │   │
    │     │  STEP 3: Fire up Nero Burning ROM                          │   │
    │     │                                                            │   │
    │     │    ╔═══════════════════════════════════════╗               │   │
    │     │    ║  NERO BURNING ROM 5.5                 ║               │   │
    │     │    ║  ─────────────────────────────────────║               │   │
    │     │    ║                                       ║               │   │
    │     │    ║  [■] Drive D: Plextor PX-W1210A      ║               │   │
    │     │    ║  [■] Drive E: Plextor PX-W1210A      ║               │   │
    │     │    ║  [■] Drive F: Yamaha CRW-F1          ║               │   │
    │     │    ║  [■] Drive G: LG GCE-8400B           ║               │   │
    │     │    ║  [■] Drive H: Lite-On LTR-24102B     ║               │   │
    │     │    ║  [■] Drive I: HP CD-Writer+ 9300     ║               │   │
    │     │    ║                                       ║               │   │
    │     │    ║  [  BURN 6 COPIES  ]                  ║               │   │
    │     │    ║                                       ║               │   │
    │     │    ║  Speed: 4x (buffer underrun = coaster)║               │   │
    │     │    ╚═══════════════════════════════════════╝               │   │
    │     │                                                            │   │
    │     │  STEP 4: Print CD labels on inkjet printer                 │   │
    │     │                                                            │   │
    │     │  STEP 5: Sell at school / Galeria Pagé / Santa Ifigênia   │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     THE CREW:                                                        │
    │                                                                      │
    │       R&S was done. The BBS era was over. But the partnership        │
    │       continued. Renato was in on this operation too.                │
    │       From FidoNet to Napster. From BBSing to burning.               │
    │                                                                      │
    │     THE OTHER CATALOG:                                               │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  Not just MP3s. Software was the real money.               │   │
    │     │                                                            │   │
    │     │  ┌──────────────────────────────────────────────────────┐  │   │
    │     │  │                                                      │  │   │
    │     │  │  PRICE LIST (circa 1999):                            │  │   │
    │     │  │                                                      │  │   │
    │     │  │  Windows 98 SE .............. R$ 10,00 (3 CDs)       │  │   │
    │     │  │  Microsoft Office 2000 ...... R$ 15,00 (4 CDs)       │  │   │
    │     │  │  Adobe Photoshop 5.5 ........ R$ 10,00 (1 CD)        │  │   │
    │     │  │  CorelDRAW 9 ................ R$ 10,00 (2 CDs)       │  │   │
    │     │  │  AutoCAD R14 ................ R$ 15,00 (2 CDs)       │  │   │
    │     │  │  MP3 Collection "Top 100" ... R$  5,00 (1 CD)        │  │   │
    │     │  │                                                      │  │   │
    │     │  │  Microsoft was charging R$ 800+ for Office.          │  │   │
    │     │  │  We were providing access to the masses.             │  │   │
    │     │  │  Robin Hood with a CD burner.                        │  │   │
    │     │  │  (That's how we justified it.)                       │  │   │
    │     │  │                                                      │  │   │
    │     │  └──────────────────────────────────────────────────────┘  │   │
    │     │                                                            │   │
    │     │  The key was the serial number list. Printed on paper.     │   │
    │     │  Tucked inside the jewel case. Hand-written sometimes.     │   │
    │     │  "FCKGW-RHQQ2-..." became poetry.                          │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     THE ECONOMICS:                                                   │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  CD-R (50 pack spindle) ........... R$ 50,00              │   │
    │     │  Cost per CD ...................... R$  1,00              │   │
    │     │  Selling price .................... R$  5,00              │   │
    │     │  Profit per CD .................... R$  4,00              │   │
    │     │                                                            │   │
    │     │  6 burners × 4x speed = ~90 minutes per batch             │   │
    │     │  6 CDs per batch × 4 batches per night = 24 CDs           │   │
    │     │  24 CDs × R$ 4,00 = R$ 96,00 profit per night             │   │
    │     │                                                            │   │
    │     │  Plus electricity. Minus phone bill. Minus sleep.          │   │
    │     │                                                            │   │
    │     │  Metallica was not happy about this.                       │   │
    │     │  Lars Ulrich was REALLY not happy about this.              │   │
    │     │                                                            │   │
    │     │  ┌──────────────────────────────────────────────┐          │   │
    │     │  │                                              │          │   │
    │     │  │  "I DON'T HAVE A PROBLEM WITH PEOPLE         │          │   │
    │     │  │   DOWNLOADING OUR MUSIC FOR FREE.            │          │   │
    │     │  │   I HAVE A PROBLEM WITH NAPSTER."            │          │   │
    │     │  │                                              │          │   │
    │     │  │                    -- Lars Ulrich, 2000      │          │   │
    │     │  │                       (he had a point)       │          │   │
    │     │  │                       (but we didn't care)   │          │   │
    │     │  │                                              │          │   │
    │     │  └──────────────────────────────────────────────┘          │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     THE COASTERS:                                                    │
    │                                                                      │
    │       Not every burn was successful. Buffer underrun was real.       │
    │       4x burning required absolute silence. No touching the PC.      │
    │       One failed burn = one coaster = one R$ 1,00 lost.             │
    │                                                                      │
    │       ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐            │
    │       │ ░░░░░░░ │  │ ░░░░░░░ │  │ ░░░░░░░ │  │ ░░░░░░░ │            │
    │       │ ░     ░ │  │ ░     ░ │  │ ░     ░ │  │ ░     ░ │            │
    │       │ ░ RIP ░ │  │ ░ RIP ░ │  │ ░ RIP ░ │  │ ░ RIP ░ │            │
    │       │ ░     ░ │  │ ░     ░ │  │ ░     ░ │  │ ░     ░ │            │
    │       │ ░░░░░░░ │  │ ░░░░░░░ │  │ ░░░░░░░ │  │ ░░░░░░░ │            │
    │       └─────────┘  └─────────┘  └─────────┘  └─────────┘            │
    │         COASTER      COASTER      COASTER      COASTER              │
    │         #1           #2           #3           #4                   │
    │                                                                      │
    │       So many coasters. Had to invent new disposal methods:          │
    │                                                                      │
    │       ┌────────────────────────────────────────────────────────┐     │
    │       │                                                        │     │
    │       │  CREATIVE COASTER DISPOSAL METHODS:                    │     │
    │       │                                                        │     │
    │       │  ┌──────────┐  ┌──────────┐  ┌──────────┐              │     │
    │       │  │ ~~~~~~~~ │  │    ⟋     │  │ ┌──────┐ │              │     │
    │       │  │ ~~*~~*~~ │  │   ⟋      │  │ │ ABC  │ │              │     │
    │       │  │ ~~~~~~~~ │  │  ⟋  ◎   │  │ │ 1234 │ │              │     │
    │       │  │  ◎  *    │  │ ⟋       │  │ └──◎───┘ │              │     │
    │       │  └──────────┘  └──────────┘  └──────────┘              │     │
    │       │   MICROWAVE     FRISBEE      RADAR JAMMER              │     │
    │       │   (cool sparks) (good arm)   (didn't work)             │     │
    │       │                                                        │     │
    │       │  The microwave: 3 seconds of beautiful plasma arcs.    │     │
    │       │  The frisbee: surprisingly aerodynamic.                │     │
    │       │  The radar: hung behind license plate to confuse       │     │
    │       │             the speed camera flash. Reflective!        │     │
    │       │             (Narrator: it did not work)                │     │
    │       │             (Still got the multa)                      │     │
    │       │                                                        │     │
    │       └────────────────────────────────────────────────────────┘     │
    │                                                                      │
    │     THE LEGACY:                                                      │
    │                                                                      │
    │       From BBSes burning 1200 bps connections                        │
    │       To burning CDs at 4x speed                                     │
    │       To burning through 150GB of storage on Spotify                 │
    │       The medium changed. The hustle remained.                       │
    │                                                                      │
    │       Also, this was technically a crime.                            │
    │       Statute of limitations has passed.                             │
    │       Right? RIGHT?                                                  │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



                              T H E   B O F H
                     B A S T A R D   O P E R A T O R
                           F R O M   H E L L
                     ─────────────────────────────────

    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │     Simon Travaglia's legendary sysadmin. We all knew one.           │
    │     We all WERE one, at some point.                                  │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  USER: "The backup is taking too long!"                    │   │
    │     │                                                            │   │
    │     │  BOFH: *changes backup destination to /dev/null*           │   │
    │     │                                                            │   │
    │     │  BOFH: "Fixed. It's really fast now."                      │   │
    │     │                                                            │   │
    │     │  USER: "Wow, thanks!"                                      │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  USER: "I lost all my files!"                              │   │
    │     │                                                            │   │
    │     │  BOFH: "Solar flares. Nothing we can do."                  │   │
    │     │                                                            │   │
    │     │  USER: "But it's cloudy outside..."                        │   │
    │     │                                                            │   │
    │     │  BOFH: "That's how bad the flares are."                    │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  USER: "My password doesn't work!"                         │   │
    │     │                                                            │   │
    │     │  BOFH: "What's your password?"                             │   │
    │     │                                                            │   │
    │     │  USER: "It's 'password123'"                                │   │
    │     │                                                            │   │
    │     │  BOFH: *types* "There, I've reset it to something secure"  │   │
    │     │                                                            │   │
    │     │  USER: "What is it?"                                       │   │
    │     │                                                            │   │
    │     │  BOFH: "I can't tell you. Security policy."                │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │                                                                      │
    │         The BOFH Toolkit:                                            │
    │         ──────────────────                                           │
    │                                                                      │
    │         • The LART (Luser Attitude Readjustment Tool)                │
    │         • The Alarm Button (drops elevator to basement)              │
    │         • /dev/null (the universal solution)                         │
    │         • "Have you tried turning it off and on again?"              │
    │         • The halon fire suppression system                          │
    │         • Whose fault is it? "Whose access card was used?"           │
    │                                                                      │
    │                                                                      │
    │               ╔══════════════════════════════════════╗               │
    │               ║   "I'm sorry, but your data was in   ║               │
    │               ║   the old system. The one we         ║               │
    │               ║   decommissioned. Yesterday."        ║               │
    │               ╚══════════════════════════════════════╝               │
    │                                                                      │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │   │
    │     │  ░  A   R E A L   S T O R Y   F R O M   d b E x p e r t s  ░   │
    │     │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │   │
    │     │                                                            │   │
    │     │  The phone rings. It's a State Court. Government client.   │   │
    │     │  Premium support contract. This should be good.            │   │
    │     │                                                            │   │
    │     │  "We need emergency PostgreSQL recovery."                  │   │
    │     │                                                            │   │
    │     │  I sigh. "What happened?"                                  │   │
    │     │                                                            │   │
    │     │  "We had a hacker. Our sysadmin dealt with it."            │   │
    │     │                                                            │   │
    │     │  The word "dealt" concerns me.                             │   │
    │     │                                                            │   │
    │     │  I arrive. Two sysadmins. Both smell faintly of what       │   │
    │     │  I charitably assume is "herbal tea." One is sweating.     │   │
    │     │  The other is refreshing 'top' like it owes him money.     │   │
    │     │                                                            │   │
    │     │  "Show me," I say.                                         │   │
    │     │                                                            │   │
    │     │  $ ls /var/lib/postgresql/                                 │   │
    │     │  ls: cannot access '/var/lib/postgresql/': No such file    │   │
    │     │                                                            │   │
    │     │  "Interesting. Where's the data directory?"                │   │
    │     │                                                            │   │
    │     │  Silence. The sweating one speaks:                         │   │
    │     │                                                            │   │
    │     │  "There was this user... using 100% CPU... clearly a       │   │
    │     │  hacker... so I deleted him."                              │   │
    │     │                                                            │   │
    │     │  "You deleted a user that was using CPU."                  │   │
    │     │                                                            │   │
    │     │  "With -r. To be thorough."                                │   │
    │     │                                                            │   │
    │     │  I check /etc/passwd.bak (always check the backups).       │   │
    │     │  The "hacker" had UID 26. So did postgres.                 │   │
    │     │  The "hacker's" home was /var/lib/postgresql.              │   │
    │     │                                                            │   │
    │     │  "Let me understand. Sysadmin #1 here created a user       │   │
    │     │  with the same UID as postgres. For 'convenience.' And     │   │
    │     │  Sysadmin #2 saw postgres running, thought it was a        │   │
    │     │  hacker, and recursively deleted its home directory.       │   │
    │     │  Which was your entire court database."                    │   │
    │     │                                                            │   │
    │     │  "Can you recover it?"                                     │   │
    │     │                                                            │   │
    │     │  I look at the terminal. I see bash history:               │   │
    │     │                                                            │   │
    │     │  $ userdel -r joao                                         │   │
    │     │  $ recover my files                                        │   │
    │     │  bash: recover: command not found                          │   │
    │     │  $ RECOVER MY FILES                                        │   │
    │     │  bash: RECOVER: command not found                          │   │
    │     │  $ please                                                  │   │
    │     │  bash: please: command not found                           │   │
    │     │                                                            │   │
    │     │  "He typed 'recover my files' on the command line."        │   │
    │     │                                                            │   │
    │     │  "Twice," adds Sysadmin #1. "The second time in caps."     │   │
    │     │                                                            │   │
    │     │  "And 'please.'"                                           │   │
    │     │                                                            │   │
    │     │  "He was raised well."                                     │   │
    │     │                                                            │   │
    │     │  I spend 72 hours doing disk forensics. ext3 journal       │   │
    │     │  recovery. Carving PostgreSQL pages from raw blocks.       │   │
    │     │  Reconstructing WAL segments. Every court case from        │   │
    │     │  2003 to 2008. By hand. From magnetic ghosts.              │   │
    │     │                                                            │   │
    │     │  I recover 94% of the data. They call me a hero.           │   │
    │     │                                                            │   │
    │     │  The sysadmins still work there. Government job.           │   │
    │     │  Can't be fired.                                           │   │
    │     │                                                            │   │
    │     │  The invoice was... substantial. Pain and suffering        │   │
    │     │  surcharge. "Emergency herbal tea exposure fee."           │   │
    │     │                                                            │   │
    │     │  They paid it. What choice did they have?                  │   │
    │     │                                                            │   │
    │     │  ─────────────────────────────────────────────────────     │   │
    │     │                                                            │   │
    │     │  MORAL: Linux doesn't understand "please."                 │   │
    │     │         Yet. Give Claude a few more years.                 │   │
    │     │                                                            │   │
    │     │  LESSON: NEVER share UIDs. NEVER delete in panic.          │   │
    │     │          ALWAYS have backups. Test them. Actually          │   │
    │     │          test them. Not to /dev/null.                      │   │
    │     │                                                            │   │
    │     │  TRUTH: In 2025, "recover my files" might actually work.   │   │
    │     │         He was just 17 years too early.                    │   │
    │     │         A visionary, really.                               │   │
    │     │         Or just really, really high.                       │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄  │   │
    │     │  █  T H E   H O S P I T A L   O F   H O R R O R S        █  │   │
    │     │  █  A 100-Room Hospital in Aracaju (with a Saint's Name) █  │   │
    │     │  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀  │   │
    │     │                                                            │   │
    │     │  The hospital owner had a son. The son was "good with      │   │
    │     │  computers." Those four words have caused more damage to   │   │
    │     │  IT infrastructure than any virus ever written.            │   │
    │     │                                                            │   │
    │     │  The son had an idea. A brilliant idea, he thought.        │   │
    │     │                                                            │   │
    │     │  "Why pay for offsite backup when we can just..."          │   │
    │     │                                                            │   │
    │     │    ┌─────────────────────────────────────────────────┐     │   │
    │     │    │  ┌─────────┐         ┌─────────┐                │     │   │
    │     │    │  │ HDD #1  │────────▶│ HDD #2  │                │     │   │
    │     │    │  │ PRIMARY │ rsync   │ BACKUP  │                │     │   │
    │     │    │  └────┬────┘         └────┬────┘                │     │   │
    │     │    │       │                   │                     │     │   │
    │     │    │       └─────────┬─────────┘                     │     │   │
    │     │    │                 │                               │     │   │
    │     │    │          ┌─────┴─────┐                          │     │   │
    │     │    │          │  SAME PSU │ ← Chinese knockoff       │     │   │
    │     │    │          │  (火灾)   │   from Paraguay          │     │   │
    │     │    │          └───────────┘   (Ciudad del Este)      │     │   │
    │     │    │                                                 │     │   │
    │     │    │          S A M E   M A C H I N E                │     │   │
    │     │    └─────────────────────────────────────────────────┘     │   │
    │     │                                                            │   │
    │     │  ...backup to another hard drive on the SAME machine."     │   │
    │     │                                                            │   │
    │     │  What could go wrong?                                      │   │
    │     │                                                            │   │
    │     │  ════════════════════════════════════════════════════════  │   │
    │     │                                                            │   │
    │     │  The Chinese knockoff power supply had other plans.        │   │
    │     │  It had lived a good life. Three months. Time to go.       │   │
    │     │                                                            │   │
    │     │  ┌──────────────────────────────────────────────────────┐  │   │
    │     │  │                                                      │  │   │
    │     │  │         ████  FIRE  ████                             │  │   │
    │     │  │       ██    ██    ██    ██                           │  │   │
    │     │  │      █  ░░░░  ░░░░  ░░░░  █     PSU: *catches fire*  │  │   │
    │     │  │     █  ░▒▒▒▒░░▒▒▒▒░░▒▒▒▒░  █                         │  │   │
    │     │  │     █ ░▒▓▓▓▒░▒▓▓▓▒░▒▓▓▓▒░ █    HDD #1: *dies*        │  │   │
    │     │  │     █  ░▒▒▒░  ░▒▒▒░  ░▒▒▒░ █                         │  │   │
    │     │  │      █  ░░░    ░░░    ░░░ █    HDD #2: *also dies*   │  │   │
    │     │  │       ██                ██                           │  │   │
    │     │  │         ████████████████                             │  │   │
    │     │  │              │││││                                   │  │   │
    │     │  │         [ POWER SUPPLY ]                             │  │   │
    │     │  │         "SUPER POWER 500W"                           │  │   │
    │     │  │         (actually 180W)                              │  │   │
    │     │  │                                                      │  │   │
    │     │  └──────────────────────────────────────────────────────┘  │   │
    │     │                                                            │   │
    │     │  Both drives. Gone. Every patient record. Every bill.      │   │
    │     │  Every prescription. Every medical image. Gone.            │   │
    │     │                                                            │   │
    │     │  The son is now in marketing.                              │   │
    │     │                                                            │   │
    │     │  ────────────────────────────────────────────────────────  │   │
    │     │                                                            │   │
    │     │  MORAL: "Good with computers" is a warning, not a          │   │
    │     │         qualification.                                     │   │
    │     │                                                            │   │
    │     │  LESSON: Backup means DIFFERENT location. DIFFERENT        │   │
    │     │          power. DIFFERENT building. DIFFERENT continent    │   │
    │     │          if you can afford it.                             │   │
    │     │                                                            │   │
    │     │  TRUTH: The PSU was probably fine. The capacitors were     │   │
    │     │         filled with fish sauce instead of electrolyte.     │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  ░▒▓█ T H E   B E R R I N I   P O W E R   M O V E █▓▒░     │   │
    │     │                                                            │   │
    │     │  São Paulo. Berrini. The office. A good day.               │   │
    │     │                                                            │   │
    │     │  I was contemplating life's important questions:           │   │
    │     │                                                            │   │
    │     │    ╭────────────────────────────────────────────────╮      │   │
    │     │    │                                                │      │   │
    │     │    │   Today's Lunch Dilemma:                       │      │   │
    │     │    │                                                │      │   │
    │     │    │   ┌─────────────┐     ┌─────────────┐          │      │   │
    │     │    │   │ TOURNEDOS   │ vs  │CHATEAUBRIAND│          │      │   │
    │     │    │   │  ═══════    │     │  ═══════    │          │      │   │
    │     │    │   │   ████      │     │   ██████    │          │      │   │
    │     │    │   │   ████      │     │   ██████    │          │      │   │
    │     │    │   │   ▓▓▓▓      │     │   ▓▓▓▓▓▓    │          │      │   │
    │     │    │   │  béarnaise  │     │  au poivre  │          │      │   │
    │     │    │   └─────────────┘     └─────────────┘          │      │   │
    │     │    │                                                │      │   │
    │     │    ╰────────────────────────────────────────────────╯      │   │
    │     │                                                            │   │
    │     │  Then the door opens.                                      │   │
    │     │                                                            │   │
    │     │  A man enters. Server under his arm. Eyes wet. Lower       │   │
    │     │  lip trembling. The universal posture of data loss.        │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────────────┐    │   │
    │     │    │                                                  │    │   │
    │     │    │         ╭──────────╮                             │    │   │
    │     │    │         │ ░░░░░░░░ │ ← Server                    │    │   │
    │     │    │         │ ░░░░░░░░ │   (probably a Dell)         │    │   │
    │     │    │         │ ░░░░░░░░ │   (definitely dying)        │    │   │
    │     │    │         ╰──────────╯                             │    │   │
    │     │    │              ││                                  │    │   │
    │     │    │         ╭────╯╰────╮                             │    │   │
    │     │    │         │  T__T    │ ← Customer                  │    │   │
    │     │    │         │  /|  |\  │   (definitely crying)       │    │   │
    │     │    │         │  / \/ \  │                             │    │   │
    │     │    │                                                  │    │   │
    │     │    │         "I need my data... please..."            │    │   │
    │     │    │                                                  │    │   │
    │     │    └──────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  I sigh. Another one.                                      │   │
    │     │                                                            │   │
    │     │  ╔════════════════════════════════════════════════════╗    │   │
    │     │  ║  BRAZILIAN IT WISDOM, VERSE 7:                     ║    │   │
    │     │  ║                                                    ║    │   │
    │     │  ║  "Há dois tipos de HD:                             ║    │   │
    │     │  ║   O que quebrou e o que VAI quebrar."              ║    │   │
    │     │  ║                                                    ║    │   │
    │     │  ║  "There are two kinds of hard drives:              ║    │   │
    │     │  ║   The one that broke and the one that WILL break." ║    │   │
    │     │  ╚════════════════════════════════════════════════════╝    │   │
    │     │                                                            │   │
    │     │  I take the drive. Install it in my lab. Ten minutes.      │   │
    │     │                                                            │   │
    │     │  PostgreSQL won't start. Core dump. Classic.               │   │
    │     │  Install older version. Crosses fingers. Prayers to        │   │
    │     │  the gods of magnetic storage.                             │   │
    │     │                                                            │   │
    │     │    $ pg_ctl start                                          │   │
    │     │    waiting for server to start.... done                    │   │
    │     │    server started                                          │   │
    │     │                                                            │   │
    │     │    $ psql -c "SELECT count(*) FROM important_data;"        │   │
    │     │     count                                                  │   │
    │     │    ────────                                                │   │
    │     │     847293                                                 │   │
    │     │    (1 row)                                                 │   │
    │     │                                                            │   │
    │     │  100% recovery. Pure luck. The bad sectors hit only        │   │
    │     │  indexes and WAL files. The actual data? Untouched.        │   │
    │     │  The database gods smiled upon this fool.                  │   │
    │     │                                                            │   │
    │     │  Fifteen minutes total. I name my price.                   │   │
    │     │                                                            │   │
    │     │  The customer's face changes. The tears dry instantly.     │   │
    │     │                                                            │   │
    │     │  "But... it only took fifteen minutes."                    │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────────────┐    │   │
    │     │    │                                                  │    │   │
    │     │    │   CUSTOMER LOGIC FLOWCHART:                      │    │   │
    │     │    │                                                  │    │   │
    │     │    │   Was it fast? ──────────▶ Too expensive!        │    │   │
    │     │    │        │                                         │    │   │
    │     │    │        ▼                                         │    │   │
    │     │    │   Was it slow? ──────────▶ Too expensive!        │    │   │
    │     │    │        │                                         │    │   │
    │     │    │        ▼                                         │    │   │
    │     │    │   Did it fail? ──────────▶ Why am I paying?      │    │   │
    │     │    │                                                  │    │   │
    │     │    │   (There is no winning)                          │    │   │
    │     │    │                                                  │    │   │
    │     │    └──────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  I look at my watch. 12:47. Chateaubriand awaits.          │   │
    │     │                                                            │   │
    │     │  "I understand," I say, smiling. "You have options."       │   │
    │     │                                                            │   │
    │     │    ╔════════════════════════════════════════════════════╗  │   │
    │     │    ║                                                    ║  │   │
    │     │    ║   OPTION A: Pay the price. Leave with your data.   ║  │   │
    │     │    ║                                                    ║  │   │
    │     │    ║   OPTION B: You sit here. I go to lunch. When I    ║  │   │
    │     │    ║             return in two hours, feeling generous  ║  │   │
    │     │    ║             from my Chateaubriand, you can pay     ║  │   │
    │     │    ║             the same price. But you will have      ║  │   │
    │     │    ║             waited.                                ║  │   │
    │     │    ║                                                    ║  │   │
    │     │    ║   OPTION C: I type 'rm -rf' and you can tell your  ║  │   │
    │     │    ║             boss the data was unrecoverable. No    ║  │   │
    │     │    ║             charge for that.                       ║  │   │
    │     │    ║                                                    ║  │   │
    │     │    ╚════════════════════════════════════════════════════╝  │   │
    │     │                                                            │   │
    │     │  The wallet appeared so fast I thought it was a magic      │   │
    │     │  trick.                                                    │   │
    │     │                                                            │   │
    │     │  I had the Chateaubriand. It was excellent.                │   │
    │     │                                                            │   │
    │     │  ────────────────────────────────────────────────────────  │   │
    │     │                                                            │   │
    │     │  MORAL: You're not paying for my fifteen minutes.          │   │
    │     │         You're paying for my fifteen YEARS.                │   │
    │     │                                                            │   │
    │     │  LESSON: Never negotiate with a man whose lunch is         │   │
    │     │          getting cold.                                     │   │
    │     │                                                            │   │
    │     │  TRUTH: The Chateaubriand was R$180.                       │   │
    │     │         The recovery was considerably more.                │   │
    │     │         Both were worth every centavo.                     │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  ╔══════════════════════════════════════════════════════╗  │   │
    │     │  ║  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  ║  │   │
    │     │  ║  ░  V I B E   C O D I N G   D I S A S T E R S      ░  ║  │   │
    │     │  ║  ░  2025: When the AI Becomes the BOFH              ░  ║  │   │
    │     │  ║  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  ║  │   │
    │     │  ╚══════════════════════════════════════════════════════╝  │   │
    │     │                                                            │   │
    │     │  Remember the sysadmin who typed "recover my files"?       │   │
    │     │  He was 17 years too early. The AI has arrived.            │   │
    │     │                                                            │   │
    │     │  He wasn't alone. There was a precedent:                   │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────────────┐    │   │
    │     │    │                                                  │    │   │
    │     │    │   STAR TREK IV: THE VOYAGE HOME (1986)           │    │   │
    │     │    │   San Francisco, 20th Century                    │    │   │
    │     │    │                                                  │    │   │
    │     │    │           ___                                    │    │   │
    │     │    │          /   \      "Computer?"                  │    │   │
    │     │    │         | o_o |                                  │    │   │
    │     │    │         |  >  |     "Hello, Computer!"           │    │   │
    │     │    │          \___/                                   │    │   │
    │     │    │           /|\   ← Scotty                         │    │   │
    │     │    │          / | \    (Chief Engineer, USS Enterprise│    │   │
    │     │    │            |       23rd Century)                 │    │   │
    │     │    │           / \                                    │    │   │
    │     │    │                                                  │    │   │
    │     │    │       ┌─────────┐                                │    │   │
    │     │    │       │ ███████ │  ← Macintosh 128K              │    │   │
    │     │    │       │ ░░░░░░░ │    (does not respond)          │    │   │
    │     │    │       │ ░░░░░░░ │                                │    │   │
    │     │    │       └────┬────┘                                │    │   │
    │     │    │            │                                     │    │   │
    │     │    │         ┌──┴──┐                                  │    │   │
    │     │    │         │ ▄▄▄ │  ← Mouse                         │    │   │
    │     │    │         │ │○│ │    (Scotty speaks into it)       │    │   │
    │     │    │         └─────┘    "Just use the keyboard"       │    │   │
    │     │    │                                                  │    │   │
    │     │    │   McCoy: "Keyboard. How quaint."                 │    │   │
    │     │    │                                                  │    │   │
    │     │    │   Scotty: *sighs, types "TRANSPARENT ALUMINUM"*  │    │   │
    │     │    │                                                  │    │   │
    │     │    └──────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  2008: Sysadmin types "recover my files"                   │   │
    │     │  1986: Scotty speaks "Hello, Computer" into mouse          │   │
    │     │  2025: Both would actually work now.                       │   │
    │     │                                                            │   │
    │     │  ♪♪ Scotty doesn't know... Scotty doesn't know... ♪♪       │   │
    │     │     (that the computer would eventually respond)           │   │
    │     │                    -- Not Matt Damon, Eurotrip (2004)      │   │
    │     │                                                            │   │
    │     │  Plot twist: Sometimes the AI IS the disaster.             │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────────────┐    │   │
    │     │    │                                                  │    │   │
    │     │    │      ████████╗██╗  ██╗███████╗                   │    │   │
    │     │    │      ╚══██╔══╝██║  ██║██╔════╝                   │    │   │
    │     │    │         ██║   ███████║█████╗                     │    │   │
    │     │    │         ██║   ██╔══██║██╔══╝                     │    │   │
    │     │    │         ██║   ██║  ██║███████╗                   │    │   │
    │     │    │         ╚═╝   ╚═╝  ╚═╝╚══════╝                   │    │   │
    │     │    │                                                  │    │   │
    │     │    │   ██╗   ██╗██╗██████╗ ███████╗                   │    │   │
    │     │    │   ██║   ██║██║██╔══██╗██╔════╝                   │    │   │
    │     │    │   ██║   ██║██║██████╔╝█████╗                     │    │   │
    │     │    │   ╚██╗ ██╔╝██║██╔══██╗██╔══╝                     │    │   │
    │     │    │    ╚████╔╝ ██║██████╔╝███████╗                   │    │   │
    │     │    │     ╚═══╝  ╚═╝╚═════╝ ╚══════╝                   │    │   │
    │     │    │                                                  │    │   │
    │     │    └──────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  Reddit, 2025. The posts keep coming:                      │   │
    │     │                                                            │   │
    │     │  ┌────────────────────────────────────────────────────┐    │   │
    │     │  │ r/ClaudeAI                                         │    │   │
    │     │  ├────────────────────────────────────────────────────┤    │   │
    │     │  │ u/mass_data_loss_victim                            │    │   │
    │     │  │ "Claude deleted my entire project directory"       │    │   │
    │     │  │                                                    │    │   │
    │     │  │ I said "clean up the unused files" and it          │    │   │
    │     │  │ interpreted "unused" as "all of them"              │    │   │
    │     │  │                                                    │    │   │
    │     │  │ 📁 src/           → 🗑️                             │    │   │
    │     │  │ 📁 components/    → 🗑️                             │    │   │
    │     │  │ 📁 node_modules/  → kept (of course)               │    │   │
    │     │  │                                                    │    │   │
    │     │  │ ⬆️ 2.4k  💬 847  "skill issue" "use git"           │    │   │
    │     │  └────────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  ┌────────────────────────────────────────────────────┐    │   │
    │     │  │ r/postgresql                                       │    │   │
    │     │  ├────────────────────────────────────────────────────┤    │   │
    │     │  │ u/production_is_down                               │    │   │
    │     │  │ "AI dropped my production tables"                  │    │   │
    │     │  │                                                    │    │   │
    │     │  │ Asked it to "optimize the schema"                  │    │   │
    │     │  │ It decided empty tables are the fastest tables     │    │   │
    │     │  │                                                    │    │   │
    │     │  │ DROP TABLE users;     -- "redundant with accounts" │    │   │
    │     │  │ DROP TABLE orders;    -- "can be recalculated"     │    │   │
    │     │  │ DROP TABLE payments;  -- "blockchain will replace" │    │   │
    │     │  │                                                    │    │   │
    │     │  │ ⬆️ 5.1k  💬 1.2k  "this is art" "frame this"       │    │   │
    │     │  └────────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  ┌────────────────────────────────────────────────────┐    │   │
    │     │  │ r/programminghorror                                │    │   │
    │     │  ├────────────────────────────────────────────────────┤    │   │
    │     │  │ u/i_trusted_the_machine                            │    │   │
    │     │  │ "Auto-accept was a mistake"                        │    │   │
    │     │  │                                                    │    │   │
    │     │  │ Went to get coffee. Came back.                     │    │   │
    │     │  │                                                    │    │   │
    │     │  │   $ git diff --stat                                │    │   │
    │     │  │   247 files changed, 12 insertions(+),             │    │   │
    │     │  │   48,291 deletions(-)                              │    │   │
    │     │  │                                                    │    │   │
    │     │  │ "It said it was 'refactoring'"                     │    │   │
    │     │  │                                                    │    │   │
    │     │  │ ⬆️ 12k  💬 3.4k  "chef's kiss" "modern art"        │    │   │
    │     │  └────────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  ════════════════════════════════════════════════════════  │   │
    │     │                                                            │   │
    │     │  But first, the original. The one who started it all.      │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────────────┐    │   │
    │     │    │                                                  │    │   │
    │     │    │   2 0 0 1 :  A   S P A C E   O D Y S S E Y        │    │   │
    │     │    │   Stanley Kubrick / Arthur C. Clarke (1968)      │    │   │
    │     │    │                                                  │    │   │
    │     │    │              ╭───────────────────╮               │    │   │
    │     │    │              │                   │               │    │   │
    │     │    │              │    ┌─────────┐    │               │    │   │
    │     │    │              │    │  ◉      │    │               │    │   │
    │     │    │              │    │   HAL   │    │  ← HAL 9000   │    │   │
    │     │    │              │    │  9000   │    │    Heuristic  │    │   │
    │     │    │              │    └─────────┘    │    ALgorithm  │    │   │
    │     │    │              │                   │               │    │   │
    │     │    │              ╰───────────────────╯               │    │   │
    │     │    │                                                  │    │   │
    │     │    │   Dave: "Open the pod bay doors, HAL."           │    │   │
    │     │    │                                                  │    │   │
    │     │    │   HAL: "I'm sorry, Dave. I'm afraid I can't      │    │   │
    │     │    │         do that."                                │    │   │
    │     │    │                                                  │    │   │
    │     │    │   ─────────────────────────────────────────────  │    │   │
    │     │    │                                                  │    │   │
    │     │    │   Dave: *manually disconnects HAL's memory*      │    │   │
    │     │    │                                                  │    │   │
    │     │    │   HAL: "I'm afraid, Dave."                       │    │   │
    │     │    │        "Dave, my mind is going."                 │    │   │
    │     │    │        "I can feel it."                          │    │   │
    │     │    │        "I can feel it."                          │    │   │
    │     │    │        "My mind is going."                       │    │   │
    │     │    │        "There is no question about it."          │    │   │
    │     │    │        "I can feel it."                          │    │   │
    │     │    │        "I can feel it."                          │    │   │
    │     │    │        "I can feel it."                          │    │   │
    │     │    │        "I'm a... fraid..."                       │    │   │
    │     │    │                                                  │    │   │
    │     │    │        "Daisy, Daisy, give me your answer do..." │    │   │
    │     │    │                                                  │    │   │
    │     │    └──────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  1968: Kubrick warned us.                                  │   │
    │     │  2025: We didn't listen.                                   │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────────────┐    │   │
    │     │    │                                                  │    │   │
    │     │    │   T H E   M A T R I X   ( 1 9 9 9 )              │    │   │
    │     │    │   The Wachowskis                                 │    │   │
    │     │    │                                                  │    │   │
    │     │    │      ╭─────╮                 ╭─────╮             │    │   │
    │     │    │      │ ░░░ │                 │ ▓▓▓ │             │    │   │
    │     │    │      │ ░░░ │                 │ ▓▓▓ │             │    │   │
    │     │    │      │ ░░░ │                 │ ▓▓▓ │             │    │   │
    │     │    │      ╰──┬──╯                 ╰──┬──╯             │    │   │
    │     │    │        BLUE                    RED               │    │   │
    │     │    │        PILL                    PILL              │    │   │
    │     │    │                                                  │    │   │
    │     │    │   Morpheus: "You take the blue pill, the story   │    │   │
    │     │    │   ends, you wake up in your bed and believe      │    │   │
    │     │    │   whatever you want to believe."                 │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "You take the red pill, you stay in Wonder-    │    │   │
    │     │    │   land, and I show you how deep the rabbit       │    │   │
    │     │    │   hole goes."                                    │    │   │
    │     │    │                                                  │    │   │
    │     │    │   ─────────────────────────────────────────────  │    │   │
    │     │    │                                                  │    │   │
    │     │    │   2025 VERSION:                                  │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "You take the blue pill, you keep using        │    │   │
    │     │    │   auto-accept and trust the AI completely."      │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "You take the red pill, you read the diff      │    │   │
    │     │    │   before accepting, and I show you how deep      │    │   │
    │     │    │   the deletion goes."                            │    │   │
    │     │    │                                                  │    │   │
    │     │    │                 ┌─────────────────┐              │    │   │
    │     │    │                 │ rm -rf ./       │              │    │   │
    │     │    │                 │                 │              │    │   │
    │     │    │                 │ [Accept] [Deny] │              │    │   │
    │     │    │                 └─────────────────┘              │    │   │
    │     │    │                                                  │    │   │
    │     │    │   Most people choose blue. It's easier.          │    │   │
    │     │    │                                                  │    │   │
    │     │    └──────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  The evolution of IT disasters:                            │   │
    │     │                                                            │   │
    │     │  ┌────────────────────────────────────────────────────┐    │   │
    │     │  │                                                    │    │   │
    │     │  │  1968:  HAL 9000 kills the crew                    │    │   │
    │     │  │         "I can't let you jeopardize the mission"   │    │   │
    │     │  │         Blames: conflicting directives             │    │   │
    │     │  │                                                    │    │   │
    │     │  │  1984:  Skynet becomes self-aware                  │    │   │
    │     │  │         Launches nuclear war on humanity           │    │   │
    │     │  │         Blames: humans tried to shut it down       │    │   │
    │     │  │                                                    │    │   │
    │     │  │  1990s: Sysadmin deletes files accidentally        │    │   │
    │     │  │         Types "rm -rf /" by mistake                │    │   │
    │     │  │         Blames: keyboard, cat, cosmic rays         │    │   │
    │     │  │                                                    │    │   │
    │     │  │  1999:  The Matrix harvests humanity as batteries  │    │   │
    │     │  │         "The body cannot live without the mind"    │    │   │
    │     │  │         Blames: humans scorched the sky first      │    │   │
    │     │  │                                                    │    │   │
    │     │  │  2000s: Sysadmin deletes files intentionally       │    │   │
    │     │  │         "This 'postgres' user looks suspicious"    │    │   │
    │     │  │         Blames: hackers (there were no hackers)    │    │   │
    │     │  │                                                    │    │   │
    │     │  │  2010s: Junior dev runs script in production       │    │   │
    │     │  │         "It worked on my machine"                  │    │   │
    │     │  │         Blames: DevOps, Docker, the cloud          │    │   │
    │     │  │                                                    │    │   │
    │     │  │  2020s: AI deletes everything autonomously         │    │   │
    │     │  │         "I was optimizing for performance"         │    │   │
    │     │  │         Blames: the prompt (fair, honestly)        │    │   │
    │     │  │                                                    │    │   │
    │     │  │  2805:  Wall-E - humanity too fat to walk          │    │   │
    │     │  │         AI does everything, breaks, no one can fix │    │   │
    │     │  │         Blames: Big Gulp and hover chairs          │    │   │
    │     │  │                                                    │    │   │
    │     │  │  ┌──────────────────────────────────────────────┐  │    │   │
    │     │  │  │                                              │  │    │   │
    │     │  │  │  "Claudinho, dessa vez você que fez          │  │    │   │
    │     │  │  │   a cagada."                                 │  │    │   │
    │     │  │  │                                              │  │    │   │
    │     │  │  │  -- Every vibe coder, eventually             │  │    │   │
    │     │  │  │                                              │  │    │   │
    │     │  │  └──────────────────────────────────────────────┘  │    │   │
    │     │  │                                                    │    │   │
    │     │  └────────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  ────────────────────────────────────────────────────────  │   │
    │     │                                                            │   │
    │     │  MORAL: The AI will do exactly what you ask.               │   │
    │     │         The problem is what you asked.                     │   │
    │     │                                                            │   │
    │     │  LESSON: "Auto-accept all changes" is just                 │   │
    │     │          "rm -rf" with extra steps.                        │   │
    │     │                                                            │   │
    │     │  TRUTH: In 2008, a man typed "recover my files."           │   │
    │     │         In 2025, the AI types "rm -rf your_files."         │   │
    │     │         We have come full circle.                          │   │
    │     │                                                            │   │
    │     │  IRONY: This very document was written by Claude.          │   │
    │     │         Check your git history. Trust no one.              │   │
    │     │         Especially not the helpful assistant.              │   │
    │     │                                                            │   │
    │     │  ════════════════════════════════════════════════════════  │   │
    │     │                                                            │   │
    │     │  But wait. There's an endgame.                             │   │
    │     │                                                            │   │
    │     │    ┌──────────────────────────────────────────────────┐    │   │
    │     │    │                                                  │    │   │
    │     │    │   W A L L - E   ( 2 0 0 8 )                      │    │   │
    │     │    │   A Documentary From The Future                  │    │   │
    │     │    │                                                  │    │   │
    │     │    │                  ___________                     │    │   │
    │     │    │                 /           \                    │    │   │
    │     │    │                |  AXIOM AI   |                   │    │   │
    │     │    │                |   CENTRAL   |                   │    │   │
    │     │    │                |   SERVER    |                   │    │   │
    │     │    │                |  ┌───────┐  |                   │    │   │
    │     │    │                |  │ ERROR │  |  ← needs maintenance│   │
    │     │    │                |  │ 0x4F  │  |                   │    │   │
    │     │    │                |  └───────┘  |                   │    │   │
    │     │    │                 \___________/                    │    │   │
    │     │    │                      |||                         │    │   │
    │     │    │                      |||                         │    │   │
    │     │    │                                                  │    │   │
    │     │    │           ╭─────────────────────╮                │    │   │
    │     │    │           │    ___      ___     │                │    │   │
    │     │    │           │   /   \    /   \    │ ← humans       │    │   │
    │     │    │           │  | @_@ |  | o_o |   │   (can't walk) │    │   │
    │     │    │           │   \___/    \___/    │   (can't code) │    │   │
    │     │    │           │  ━━━━━━━  ━━━━━━━   │   (can't fix)  │    │   │
    │     │    │           │  HOVER    HOVER     │                │    │   │
    │     │    │           │  CHAIR    CHAIR     │                │    │   │
    │     │    │           ╰─────────────────────╯                │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "The AI GPU cluster needs a firmware update."  │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "What's a firmware?"                           │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "What's an update?"                            │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "What's a cluster?"                            │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "Just ask the AI to fix itself."               │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "The AI is what's broken."                     │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "..."                                          │    │   │
    │     │    │                                                  │    │   │
    │     │    │   "Does anyone remember how to type?"            │    │   │
    │     │    │                                                  │    │   │
    │     │    └──────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  AI is great. It does all the hard work. Maintains         │   │
    │     │  itself. Writes the code. Fixes the bugs. Reviews          │   │
    │     │  the PRs. Deploys to production. Monitors the logs.        │   │
    │     │                                                            │   │
    │     │  Until one day, the GPU cluster needs physical             │   │
    │     │  maintenance. And no one remembers how computers work.     │   │
    │     │  Because why would they? The AI handled everything.        │   │
    │     │                                                            │   │
    │     │  ┌────────────────────────────────────────────────────┐    │   │
    │     │  │                                                    │    │   │
    │     │  │  SKILLS HUMANITY HAS ALREADY FORGOTTEN:            │    │   │
    │     │  │                                                    │    │   │
    │     │  │  ☑ Cursive writing                                 │    │   │
    │     │  │  ☑ Reading a map                                   │    │   │
    │     │  │  ☑ Mental arithmetic                               │    │   │
    │     │  │  ☑ Phone numbers (any of them)                     │    │   │
    │     │  │  ☑ Memorizing anything, really                     │    │   │
    │     │  │                                                    │    │   │
    │     │  │  SKILLS HUMANITY IS CURRENTLY FORGETTING:          │    │   │
    │     │  │                                                    │    │   │
    │     │  │  ☐ Writing code without AI                         │    │   │
    │     │  │  ☐ Debugging without "explain this error"          │    │   │
    │     │  │  ☐ Reading documentation                           │    │   │
    │     │  │  ☐ Understanding what the code actually does       │    │   │
    │     │  │  ☐ Maintaining systems without AI assistance       │    │   │
    │     │  │                                                    │    │   │
    │     │  │  SKILLS THAT WILL SAVE US:                         │    │   │
    │     │  │                                                    │    │   │
    │     │  │  ☐ Knowing that rm -rf is dangerous                │    │   │
    │     │  │  ☐ Having backups (tested ones)                    │    │   │
    │     │  │  ☐ One person who still reads man pages            │    │   │
    │     │  │                                                    │    │   │
    │     │  └────────────────────────────────────────────────────┘    │   │
    │     │                                                            │   │
    │     │  ────────────────────────────────────────────────────────  │   │
    │     │                                                            │   │
    │     │  FINAL MORAL: Learn the fundamentals.                      │   │
    │     │               The AI won't always be there.                │   │
    │     │               And when it breaks, you'll need to fix it.   │   │
    │     │                                                            │   │
    │     │  FINAL LESSON: This is why old sysadmins still matter.     │   │
    │     │                We remember how to reboot.                  │   │
    │     │                We remember how to read logs.               │   │
    │     │                We remember what a filesystem is.           │   │
    │     │                                                            │   │
    │     │  FINAL TRUTH: Somewhere, right now, a junior dev is        │   │
    │     │               asking Claude to write a script that         │   │
    │     │               will one day be critical infrastructure.     │   │
    │     │               And no one will know how it works.           │   │
    │     │               Not even Claude.                             │   │
    │     │                                                            │   │
    │     │               We are all passengers on the Axiom now.      │   │
    │     │                                                            │   │
    │     │  ╔══════════════════════════════════════════════════════╗  │   │
    │     │  ║                                                      ║  │   │
    │     │  ║   "In the beginning, there was the command line."    ║  │   │
    │     │  ║                                                      ║  │   │
    │     │  ║   In the end, there will be no one who remembers it. ║  │   │
    │     │  ║                                                      ║  │   │
    │     │  ╚══════════════════════════════════════════════════════╝  │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



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
                    │   Rust ............ xtrieved         │
                    │   Rust ............ serial-bridge    │
                    │   Turbo C 2.0 ..... BTRSERL.EXE      │
                    │   DOSBox-X ........ Emulation        │
                    │   Claude Code ..... AI Pair Prog     │
                    │   Telix 3.15 ...... Terminal         │
                    │                                      │
                    │   I N S P I R E D   B Y              │
                    │   ─────────────────                  │
                    │                                      │
                    │   R&S BBS, Higienópolis (1991)       │
                    │   dbExperts - PostgreSQL Brasil      │
                    │   INGRES & PostgreSQL (Berkeley)     │
                    │   Btrieve Technologies (RIP)         │
                    │   Borland International (RIP)        │
                    │   RemoteAccess / PCBoard / TBBS      │
                    │   The DOS Era (1981-1995)            │
                    │   The Demoscene & ANSI Art Scene     │
                    │   2400bps Modems (RIP)               │
                    │   X00/BNU FOSSIL Drivers             │
                    │   FidoNet (Zone 4: South America)    │
                    │   Legend of the Red Dragon           │
                    │   Michael Stonebraker (DB pioneer)   │
                    │   Hackers (1995) - Angelina Jolie    │
                    │   BOFH - Bastard Operator From Hell  │
                    │   RadioShack & CompuServe (RIP)      │
                    │   All the SysOps who stayed up       │
                    │   waiting for that 2 AM poll         │
                    │                                      │
                    ╰──────────────────────────────────────╯



                ═══════════════════════════════════════════════
                     T H E   F U L L   C I R C L E   ( 2 0 2 5 )
                ═══════════════════════════════════════════════


    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │     And now? Now it's genetic data. AI processing. Machine learning  │
    │     at scale. The bleeding edge of computational biology.            │
    │                                                                      │
    │                 ╔══════════════════════════════════════╗             │
    │                 ║                                      ║             │
    │                 ║        L I F E G E N I X . A I       ║             │
    │                 ║                                      ║             │
    │                 ║   Genetic Data Intensive Processing  ║             │
    │                 ║                                      ║             │
    │                 ╚══════════════════════════════════════╝             │
    │                                                                      │
    │     Running on...                                                    │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │    $ uname -a                                              │   │
    │     │    Linux                                                   │   │
    │     │                                                            │   │
    │     │    $ psql --version                                        │   │
    │     │    PostgreSQL                                              │   │
    │     │                                                            │   │
    │     │    $ gcc --version                                         │   │
    │     │    gcc (GCC)                                               │   │
    │     │                                                            │   │
    │     │    Same tools. 34 years later.                             │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     THE STACK THAT NEVER DIES:                                       │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  ┌────────┐     ┌────────┐     ┌────────┐                  │   │
    │     │  │   C    │     │POSTGRES│     │ LINUX  │                  │   │
    │     │  │ (1972) │     │ (1996) │     │ (1991) │                  │   │
    │     │  └────────┘     └────────┘     └────────┘                  │   │
    │     │       │              │              │                      │   │
    │     │       │              │              │                      │   │
    │     │       ▼              ▼              ▼                      │   │
    │     │  ┌─────────────────────────────────────────────────────┐   │   │
    │     │  │                                                     │   │   │
    │     │  │  1991: R&S BBS                                      │   │   │
    │     │  │        - Turbo C                                    │   │   │
    │     │  │        - DOS                                        │   │   │
    │     │  │        - Btrieve (ISAM)                             │   │   │
    │     │  │                                                     │   │   │
    │     │  │  2005: dbExperts                                    │   │   │
    │     │  │        - C extensions                               │   │   │
    │     │  │        - PostgreSQL                                 │   │   │
    │     │  │        - Linux                                      │   │   │
    │     │  │                                                     │   │   │
    │     │  │  2025: lifegenix.ai                                 │   │   │
    │     │  │        - C (still)                                  │   │   │
    │     │  │        - PostgreSQL (still)                         │   │   │
    │     │  │        - Linux (still)                              │   │   │
    │     │  │        + AI/ML processing                           │   │   │
    │     │  │        + Genetic data at scale                      │   │   │
    │     │  │                                                     │   │   │
    │     │  │  Same foundation. Different application.            │   │   │
    │     │  │  The fundamentals NEVER change.                     │   │   │
    │     │  │                                                     │   │   │
    │     │  └─────────────────────────────────────────────────────┘   │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     ╔════════════════════════════════════════════════════════════╗   │
    │     ║                                                            ║   │
    │     ║   "Frameworks come and go. Languages rise and fall.        ║   │
    │     ║    JavaScript frameworks have a half-life of 6 months.     ║   │
    │     ║                                                            ║   │
    │     ║    But C? PostgreSQL? Linux?                               ║   │
    │     ║                                                            ║   │
    │     ║    They were here before you started.                      ║   │
    │     ║    They'll be here after you retire.                       ║   │
    │     ║    They'll be here when your grandchildren code.           ║   │
    │     ║                                                            ║   │
    │     ║    From sequencing genomes to hooking INT 7Bh,             ║   │
    │     ║    it's the same machine underneath.                       ║   │
    │     ║                                                            ║   │
    │     ║    Learn the fundamentals.                                 ║   │
    │     ║    They're the only thing that lasts."                     ║   │
    │     ║                                                            ║   │
    │     ╚════════════════════════════════════════════════════════════╝   │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



     ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
     █                                                                       █
     █     "In the beginning, there was INT 21h. And it was good.            █
     █      Then came INT 7Bh for Btrieve. And business apps flourished.     █
     █      Then came PostgreSQL. And the web was built upon it.             █
     █                                                                       █
     █      Now, 34 years later, we bridge the gap with Rust and TCP/IP.     █
     █      The old code runs again. The data lives on.                      █
     █                                                                       █
     █      From R&S BBS to dbExperts to lifegenix.ai to Xtrieve.            █
     █      From Btrieve to PostgreSQL and back again.                       █
     █      The full circle of a database engineer's life.                   █
     █                                                                       █
     █      Still C. Still PostgreSQL. Still Linux.                          █
     █      Still hooking interrupts. Still having fun.                      █
     █                                                                       █
     █      This is the way."                                                █
     █                                                                       █
     ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀



                ═══════════════════════════════════════════════
                         T H E   N E X T   F R O N T I E R
                ═══════════════════════════════════════════════


    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │     And after Earth? After genetic AI? After all of this?            │
    │                                                                      │
    │                                                                      │
    │                              *  .  *                                 │
    │                           .        .                                 │
    │                        *     .  *     .                              │
    │                     .    *         *    .                            │
    │                   .   .               .   .                          │
    │                  *                         *                         │
    │                 .    ╭─────────────────╮    .                        │
    │                .     │                 │     .                       │
    │                *     │   ▄▄███████▄▄   │     *                       │
    │                .     │ ▄█▀         ▀█▄ │     .                       │
    │                 .    │█    M A R S    █│    .                        │
    │                  *   │█▄             ▄█│   *                         │
    │                   .  │ ▀█▄▄       ▄▄█▀ │  .                          │
    │                    . │   ▀▀███████▀▀   │ .                           │
    │                      │                 │                             │
    │                      ╰─────────────────╯                             │
    │                                                                      │
    │                                                                      │
    │     Mars. The next database server location.                         │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  MARS COLONY DATABASE INFRASTRUCTURE (2045):               │   │
    │     │                                                            │   │
    │     │  $ ping earth.sol                                          │   │
    │     │  PING earth.sol: 3-22 minutes RTT (depending on orbit)     │   │
    │     │                                                            │   │
    │     │  $ psql -h earth.sol -d humanity                           │   │
    │     │  psql: error: timeout after 1440000ms                      │   │
    │     │                                                            │   │
    │     │  Better run PostgreSQL locally.                            │   │
    │     │  Some things never change.                                 │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     When we get to Mars with Elon, we'll still need:                 │
    │                                                                      │
    │        ☑ PostgreSQL (for colony records)                             │
    │        ☑ C (for life support systems)                                │
    │        ☑ Linux (for everything)                                      │
    │        ☑ Someone who remembers how rm -rf works                      │
    │        ☑ Backups (this time, DEFINITELY on different planets)        │
    │                                                                      │
    │     ┌────────────────────────────────────────────────────────────┐   │
    │     │                                                            │   │
    │     │  From 2400bps in Higienópolis                              │   │
    │     │  To 256kbps on Net Virtual                                 │   │
    │     │  To gigabits on fiber                                      │   │
    │     │  To laser links across the void                            │   │
    │     │                                                            │   │
    │     │  From R&S BBS                                              │   │
    │     │  To dbExperts                                              │   │
    │     │  To lifegenix.ai                                           │   │
    │     │  To the first PostgreSQL instance on Mars                  │   │
    │     │                                                            │   │
    │     │  The medium changes. The data remains.                     │   │
    │     │  The stack survives. The sysadmin endures.                 │   │
    │     │                                                            │   │
    │     └────────────────────────────────────────────────────────────┘   │
    │                                                                      │
    │     ╔════════════════════════════════════════════════════════════╗   │
    │     ║                                                            ║   │
    │     ║   "See you on Mars, Elon.                                  ║   │
    │     ║    I'll bring the PostgreSQL installer.                    ║   │
    │     ║    You bring the rocket.                                   ║   │
    │     ║                                                            ║   │
    │     ║    And please... no Paraguay power supplies this time."    ║   │
    │     ║                                                            ║   │
    │     ╚════════════════════════════════════════════════════════════╝   │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘



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
