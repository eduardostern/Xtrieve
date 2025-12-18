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


    1982 ──●─────────────────────────────────────────────────────────────
           │  Btrieve 1.0 released by SoftCraft
           │
    1986 ──●─────────────────────────────────────────────────────────────
           │  Btrieve 4.0 - INT 7Bh interface established
           │
    1990 ──●─────────────────────────────────────────────────────────────
           │  Btrieve 5.0 - The "Golden Era"
           │  └─ Millions of DOS apps built on Btrieve
           │
    1994 ──●─────────────────────────────────────────────────────────────
           │  Btrieve 6.0 - Windows support added
           │  └─ DOS version still widely used
           │
    1998 ──●─────────────────────────────────────────────────────────────
           │  Pervasive.SQL emerges from Btrieve
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
           │  ║   30 YEARS OF TECHNOLOGY UNIFIED                   ║
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
        │                         ░░░░░░░░░░░░░░░░░░░░░░             │
        │                         ░   R E S P E C T   ░             │
        │                         ░░░░░░░░░░░░░░░░░░░░░░             │
        │                                                            │
        └────────────────────────────────────────────────────────────┘



                     ╔═══════════════════════════════════════╗
                     ║   B A C K   I N   T H E   D A Y . . . ║
                     ╚═══════════════════════════════════════╝


        ┌────────────────────────────────────────────────────────────┐
        │                                                            │
        │      ┌──────────────────────────────────────────────┐      │
        │      │  ░░░ REALISTIC 2400 MODEM ░░░░░░░░░░░░░░░░░  │      │
        │      │  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  │      │
        │      │  █ RD  TD  CD  OH  AA  TR  SD  HS  █  PWR █  │      │
        │      │  █ ●   ●   ○   ○   ○   ○   ●   ○   █      █  │      │
        │      │  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  │      │
        │      └──────────────────────────────────────────────┘      │
        │                                                            │
        │      2400 bps. No MNP5. No error correction.               │
        │      Analog phone line. Pulse dial. Click-click-click.     │
        │                                                            │
        │      X00.SYS loaded. Or BNU.COM. FOSSIL drivers.           │
        │      INT 14h hooked. FidoNet ready. Mailer polling.        │
        │                                                            │
        │      TELIX loaded. ANSI.SYS loaded. AT&F. ATZ. ATDT.       │
        │                                                            │
        │      Connecting to BBSes at 300 characters per second.     │
        │      Watching ASCII art scroll down line by line.          │
        │      Downloading a 50KB file took 3 minutes.               │
        │      And we were GRATEFUL.                                 │
        │                                                            │
        │      Now DOSBox-X nullmodem gives us 115200 baud virtual   │
        │      serial over TCP/IP. But the spirit is the same:       │
        │                                                            │
        │           ╔══════════════════════════════════════╗         │
        │           ║  SERIAL IS SERIAL IS SERIAL IS LIFE  ║         │
        │           ╚══════════════════════════════════════╝         │
        │                                                            │
        │      From Realistic Walkie-Talkies to DOSBox-X nullmodem.  │
        │      The bits still flow. The protocol still works.        │
        │      Some things never change.                             │
        │                                                            │
        └────────────────────────────────────────────────────────────┘



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
                    │   Btrieve Technologies (RIP)        │
                    │   Borland International (RIP)       │
                    │   The DOS Era (1981-1995)           │
                    │   The Demoscene                     │
                    │   2400bps Modems (RIP)              │
                    │   X00/BNU FOSSIL Drivers            │
                    │   FidoNet (1:2:3)                   │
                    │                                      │
                    ╰──────────────────────────────────────╯



     ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
     █                                                                       █
     █                                                                       █
     █     "In the beginning, there was INT 21h. And it was good.           █
     █      Then came INT 7Bh for Btrieve. And business apps flourished.    █
     █      Now, 30 years later, we bridge the gap with Rust and TCP/IP.    █
     █      The old code runs again. The data lives on.                     █
     █      This is the way."                                               █
     █                                                                       █
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
