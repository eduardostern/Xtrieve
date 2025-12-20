XTRIEVE.EXE - COM-to-TCP Bridge for Windows 98SE
================================================

This bridge allows DOS applications using BTRSERL.EXE to connect
to a remote xtrieved server via TCP/IP on Windows 98/ME.


REQUIREMENTS
------------

1. Windows 98SE, ME, or later
2. com0com virtual COM port driver (https://sourceforge.net/projects/com0com/)
3. TCP/IP networking configured
4. xtrieved server running on a remote machine


INSTALLATION
------------

1. Install com0com and create a virtual COM port pair:
   - Open com0com Setup
   - Create pair: COM1 <-> COM2
   - (COM1 will be used by BTRSERL, COM2 by this bridge)

2. Copy files to C:\XTRIEVE\:
   - XTRIEVE.EXE
   - XTRIEVE.INI
   - BTRSERL.EXE (from dos-client)

3. Edit XTRIEVE.INI with your server address:
   [Server]
   Address=192.168.1.100
   Port=7419

   [COM]
   Port=COM2


USAGE
-----

1. Start the bridge (in a Windows command prompt or as a background app):
   C:\XTRIEVE\XTRIEVE.EXE

2. Open a DOS prompt and load the TSR:
   C:\XTRIEVE\BTRSERL.EXE

3. Run your DOS application:
   C:\MYAPP\MYAPP.EXE

Command line options:
   XTRIEVE.EXE [com_port] [server:port]

   Examples:
   XTRIEVE.EXE COM2 192.168.1.100:7419
   XTRIEVE.EXE COM3


ARCHITECTURE
------------

   DOS Application
        |
        | INT 7Bh (Btrieve call)
        v
   BTRSERL.EXE (TSR)
        |
        | Serial I/O to COM1
        v
   com0com (virtual COM port pair)
        |
        | COM2
        v
   XTRIEVE.EXE (this bridge)
        |
        | TCP/IP (Winsock)
        v
   xtrieved server (port 7419)


COMPILING
---------

Two versions are available: C and Delphi/Pascal.

With Borland C++ 5.5 (C version):
   BCC32 -W -O2 XTRIEVE.C WSOCK32.LIB

With Visual C++ 6.0 (C version):
   CL /O2 XTRIEVE.C WSOCK32.LIB USER32.LIB

With Delphi 3/5/7 (Pascal version):
   DCC32 XTRIEVE.DPR

With Free Pascal (Pascal version):
   FPC -Mdelphi XTRIEVE.DPR

Using MAKEFILE:
   make -f MAKEFILE          (C version)
   make -f MAKEFILE delphi   (Delphi version)
   make -f MAKEFILE fpc      (Free Pascal version)


TROUBLESHOOTING
---------------

Q: Bridge says "Cannot open COM2"
A: Make sure com0com is installed and COM2 exists.
   Check Device Manager for the virtual COM ports.

Q: Bridge says "connect() failed"
A: Check that xtrieved is running on the remote server.
   Verify the IP address and port in XTRIEVE.INI.
   Check Windows firewall settings.

Q: DOS app hangs
A: Make sure BTRSERL.EXE is loaded before running the app.
   Check that the bridge is running and connected.


LICENSE
-------

MIT License
Copyright (c) 2025 Eduardo Stern
