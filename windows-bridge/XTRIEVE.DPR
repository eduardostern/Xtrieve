{
  XTRIEVE.DPR - COM to TCP Bridge for Windows 98SE

  Bridges BTRSERL.EXE (DOS TSR) to xtrieved server via TCP/IP.
  Uses com0com virtual COM port pair.

  Architecture:
    DOS App -> BTRSERL.EXE -> COM1 -> com0com -> COM2 -> XTRIEVE.EXE -> TCP -> xtrieved

  Compile with Delphi 3/5/7:
    DCC32 XTRIEVE.DPR

  Or with Free Pascal:
    FPC -Mdelphi XTRIEVE.DPR

  Copyright (c) 2025 Eduardo Stern
  MIT License
}

program Xtrieve;

{$APPTYPE CONSOLE}

uses
  Windows,
  WinSock,
  SysUtils;

const
  POS_BLOCK_SIZE = 128;
  MAX_BUFFER = 8192;
  DEFAULT_COM_PORT = 'COM2';
  DEFAULT_SERVER = '127.0.0.1';
  DEFAULT_PORT = 7419;

var
  hComPort: THandle = INVALID_HANDLE_VALUE;
  XtrieveSocket: TSocket = INVALID_SOCKET;

  { Configuration }
  gComPort: string = DEFAULT_COM_PORT;
  gServer: string = DEFAULT_SERVER;
  gPort: Integer = DEFAULT_PORT;

  { Statistics }
  gRequestCount: DWORD = 0;

{---------------------------------------------------------------------------
  Serial port functions
---------------------------------------------------------------------------}

function SerialInit(const PortName: string): Boolean;
var
  DCB: TDCB;
  Timeouts: TCommTimeouts;
  FullName: string;
begin
  Result := False;

  { Build full port name (\\.\COM2) }
  FullName := '\\.\' + PortName;

  hComPort := CreateFile(PChar(FullName),
                         GENERIC_READ or GENERIC_WRITE,
                         0,
                         nil,
                         OPEN_EXISTING,
                         0,
                         0);

  if hComPort = INVALID_HANDLE_VALUE then
  begin
    WriteLn('Error: Cannot open ', PortName, ' (error ', GetLastError, ')');
    Exit;
  end;

  { Configure port: 115200 8N1 }
  FillChar(DCB, SizeOf(DCB), 0);
  DCB.DCBlength := SizeOf(DCB);

  if not GetCommState(hComPort, DCB) then
  begin
    WriteLn('Error: GetCommState failed');
    CloseHandle(hComPort);
    Exit;
  end;

  DCB.BaudRate := 115200;
  DCB.ByteSize := 8;
  DCB.Parity := NOPARITY;
  DCB.StopBits := ONESTOPBIT;
  DCB.Flags := 1; { fBinary }

  if not SetCommState(hComPort, DCB) then
  begin
    WriteLn('Error: SetCommState failed');
    CloseHandle(hComPort);
    Exit;
  end;

  { Set timeouts }
  Timeouts.ReadIntervalTimeout := 50;
  Timeouts.ReadTotalTimeoutMultiplier := 10;
  Timeouts.ReadTotalTimeoutConstant := 1000;
  Timeouts.WriteTotalTimeoutMultiplier := 10;
  Timeouts.WriteTotalTimeoutConstant := 1000;
  SetCommTimeouts(hComPort, Timeouts);

  WriteLn('[*] Opened ', PortName, ' at 115200 baud');
  Result := True;
end;

function SerialReadByte: Integer;
var
  B: Byte;
  BytesRead: DWORD;
begin
  if not ReadFile(hComPort, B, 1, BytesRead, nil) or (BytesRead <> 1) then
    Result := -1
  else
    Result := B;
end;

function SerialReadBytes(Buffer: PByte; Count: DWORD): Boolean;
var
  TotalRead, BytesRead: DWORD;
begin
  TotalRead := 0;
  while TotalRead < Count do
  begin
    if not ReadFile(hComPort, Buffer[TotalRead], Count - TotalRead, BytesRead, nil) then
    begin
      Result := False;
      Exit;
    end;
    if BytesRead = 0 then
    begin
      Result := False;  { Timeout }
      Exit;
    end;
    Inc(TotalRead, BytesRead);
  end;
  Result := True;
end;

function SerialWriteBytes(Buffer: PByte; Count: DWORD): Boolean;
var
  BytesWritten: DWORD;
begin
  Result := WriteFile(hComPort, Buffer^, Count, BytesWritten, nil) and (BytesWritten = Count);
end;

{---------------------------------------------------------------------------
  TCP/IP functions
---------------------------------------------------------------------------}

function TcpInit(const Server: string; Port: Integer): Boolean;
var
  WSA: TWSAData;
  Addr: TSockAddrIn;
  Host: PHostEnt;
begin
  Result := False;

  if WSAStartup($0101, WSA) <> 0 then
  begin
    WriteLn('Error: WSAStartup failed');
    Exit;
  end;

  XtrieveSocket := Socket(AF_INET, SOCK_STREAM, 0);
  if XtrieveSocket = INVALID_SOCKET then
  begin
    WriteLn('Error: socket() failed');
    Exit;
  end;

  FillChar(Addr, SizeOf(Addr), 0);
  Addr.sin_family := AF_INET;
  Addr.sin_port := htons(Port);

  { Try as IP address first }
  Addr.sin_addr.S_addr := inet_addr(PChar(Server));
  if Addr.sin_addr.S_addr = INADDR_NONE then
  begin
    { Resolve hostname }
    Host := gethostbyname(PChar(Server));
    if Host = nil then
    begin
      WriteLn('Error: Cannot resolve ', Server);
      closesocket(XtrieveSocket);
      Exit;
    end;
    Move(Host^.h_addr^, Addr.sin_addr, Host^.h_length);
  end;

  WriteLn('[*] Connecting to ', Server, ':', Port, '...');

  if connect(XtrieveSocket, Addr, SizeOf(Addr)) <> 0 then
  begin
    WriteLn('Error: connect() failed (error ', WSAGetLastError, ')');
    closesocket(XtrieveSocket);
    Exit;
  end;

  WriteLn('[+] Connected to xtrieved');
  Result := True;
end;

function TcpSend(Buffer: PByte; Len: Integer): Boolean;
begin
  Result := send(XtrieveSocket, Buffer^, Len, 0) = Len;
end;

function TcpRecvExact(Buffer: PByte; Count: Integer): Boolean;
var
  TotalRead, BytesRead: Integer;
begin
  TotalRead := 0;
  while TotalRead < Count do
  begin
    BytesRead := recv(XtrieveSocket, Buffer[TotalRead], Count - TotalRead, 0);
    if BytesRead <= 0 then
    begin
      Result := False;
      Exit;
    end;
    Inc(TotalRead, BytesRead);
  end;
  Result := True;
end;

{---------------------------------------------------------------------------
  Protocol handling
---------------------------------------------------------------------------}

{ Wait for sync marker 0xBB 0xBB }
function WaitForSync: Boolean;
var
  B: Integer;
  FoundFirst: Boolean;
begin
  FoundFirst := False;
  while True do
  begin
    B := SerialReadByte;
    if B < 0 then
      Continue;  { Keep waiting }

    if B = $BB then
    begin
      if FoundFirst then
      begin
        Result := True;  { Got 0xBB 0xBB }
        Exit;
      end;
      FoundFirst := True;
    end
    else
      FoundFirst := False;
  end;
end;

{ Read 2-byte little-endian value from COM port }
function ReadU16Serial: Word;
var
  Buf: array[0..1] of Byte;
begin
  if not SerialReadBytes(@Buf, 2) then
    Result := $FFFF
  else
    Result := Buf[0] or (Buf[1] shl 8);
end;

{ Read 4-byte little-endian value from COM port }
function ReadU32Serial: DWORD;
var
  Buf: array[0..3] of Byte;
begin
  if not SerialReadBytes(@Buf, 4) then
    Result := $FFFFFFFF
  else
    Result := Buf[0] or (Buf[1] shl 8) or (Buf[2] shl 16) or (Buf[3] shl 24);
end;

{ Read 2-byte little-endian value from TCP }
function ReadU16Tcp: Word;
var
  Buf: array[0..1] of Byte;
begin
  if not TcpRecvExact(@Buf, 2) then
    Result := $FFFF
  else
    Result := Buf[0] or (Buf[1] shl 8);
end;

{ Read 4-byte little-endian value from TCP }
function ReadU32Tcp: DWORD;
var
  Buf: array[0..3] of Byte;
begin
  if not TcpRecvExact(@Buf, 4) then
    Result := $FFFFFFFF
  else
    Result := Buf[0] or (Buf[1] shl 8) or (Buf[2] shl 16) or (Buf[3] shl 24);
end;

{
  Process one request from BTRSERL.EXE:

  Serial format (from BTRSERL):
    [sync:2][op:2][pos:128][dlen:4][data:N][klen:2][key:N][knum:2][plen:2][path:N][lock:2]

  TCP format (to xtrieved) - same but without sync:
    [op:2][pos:128][dlen:4][data:N][klen:2][key:N][knum:2][plen:2][path:N][lock:2]

  Response (from xtrieved and to BTRSERL):
    [status:2][pos:128][dlen:4][data:N][klen:2][key:N]
}
function ProcessRequest: Boolean;
var
  Request, Response: array[0..MAX_BUFFER-1] of Byte;
  ReqLen, RespLen: Integer;

  Op, KeyLen, KeyNum, PathLen, Lock: Word;
  DataLen: DWORD;
  PosBlock: array[0..POS_BLOCK_SIZE-1] of Byte;

  RespStatus, RespKeyLen: Word;
  RespDataLen: DWORD;
begin
  Result := False;
  ReqLen := 0;
  RespLen := 0;

  { Wait for sync marker }
  if not WaitForSync then
  begin
    WriteLn('[-] Lost sync');
    Exit;
  end;

  { Read operation code }
  Op := ReadU16Serial;
  if Op = $FFFF then Exit;
  WriteLn('[>] Request #', gRequestCount + 1, ': op=', Op);

  { Build request buffer for TCP (without sync marker) }
  Request[ReqLen] := Op and $FF; Inc(ReqLen);
  Request[ReqLen] := (Op shr 8) and $FF; Inc(ReqLen);

  { Read and forward position block (128 bytes) }
  if not SerialReadBytes(@PosBlock, POS_BLOCK_SIZE) then Exit;
  Move(PosBlock, Request[ReqLen], POS_BLOCK_SIZE);
  Inc(ReqLen, POS_BLOCK_SIZE);

  { Read data length and data }
  DataLen := ReadU32Serial;
  if DataLen = $FFFFFFFF then Exit;
  Request[ReqLen] := DataLen and $FF; Inc(ReqLen);
  Request[ReqLen] := (DataLen shr 8) and $FF; Inc(ReqLen);
  Request[ReqLen] := (DataLen shr 16) and $FF; Inc(ReqLen);
  Request[ReqLen] := (DataLen shr 24) and $FF; Inc(ReqLen);

  if DataLen > 0 then
  begin
    if not SerialReadBytes(@Request[ReqLen], DataLen) then Exit;
    Inc(ReqLen, DataLen);
  end;

  { Read key length and key }
  KeyLen := ReadU16Serial;
  if KeyLen = $FFFF then Exit;
  Request[ReqLen] := KeyLen and $FF; Inc(ReqLen);
  Request[ReqLen] := (KeyLen shr 8) and $FF; Inc(ReqLen);

  if KeyLen > 0 then
  begin
    if not SerialReadBytes(@Request[ReqLen], KeyLen) then Exit;
    Inc(ReqLen, KeyLen);
  end;

  { Read key number }
  KeyNum := ReadU16Serial;
  if KeyNum = $FFFF then Exit;
  Request[ReqLen] := KeyNum and $FF; Inc(ReqLen);
  Request[ReqLen] := (KeyNum shr 8) and $FF; Inc(ReqLen);

  { Read path length and path }
  PathLen := ReadU16Serial;
  if PathLen = $FFFF then Exit;
  Request[ReqLen] := PathLen and $FF; Inc(ReqLen);
  Request[ReqLen] := (PathLen shr 8) and $FF; Inc(ReqLen);

  if PathLen > 0 then
  begin
    if not SerialReadBytes(@Request[ReqLen], PathLen) then Exit;
    Inc(ReqLen, PathLen);
  end;

  { Read lock bias }
  Lock := ReadU16Serial;
  if Lock = $FFFF then Exit;
  Request[ReqLen] := Lock and $FF; Inc(ReqLen);
  Request[ReqLen] := (Lock shr 8) and $FF; Inc(ReqLen);

  WriteLn('    data_len=', DataLen, ' key_len=', KeyLen, ' path_len=', PathLen);

  { Send to xtrieved }
  if not TcpSend(@Request, ReqLen) then
  begin
    WriteLn('[-] TCP send failed');
    Exit;
  end;

  { Read response from xtrieved }
  { [status:2][pos:128][dlen:4][data:N][klen:2][key:N] }

  { Status }
  RespStatus := ReadU16Tcp;
  if RespStatus = $FFFF then Exit;
  Response[RespLen] := RespStatus and $FF; Inc(RespLen);
  Response[RespLen] := (RespStatus shr 8) and $FF; Inc(RespLen);

  { Position block }
  if not TcpRecvExact(@Response[RespLen], POS_BLOCK_SIZE) then Exit;
  Inc(RespLen, POS_BLOCK_SIZE);

  { Data length and data }
  RespDataLen := ReadU32Tcp;
  if RespDataLen = $FFFFFFFF then Exit;
  Response[RespLen] := RespDataLen and $FF; Inc(RespLen);
  Response[RespLen] := (RespDataLen shr 8) and $FF; Inc(RespLen);
  Response[RespLen] := (RespDataLen shr 16) and $FF; Inc(RespLen);
  Response[RespLen] := (RespDataLen shr 24) and $FF; Inc(RespLen);

  if RespDataLen > 0 then
  begin
    if not TcpRecvExact(@Response[RespLen], RespDataLen) then Exit;
    Inc(RespLen, RespDataLen);
  end;

  { Key length and key }
  RespKeyLen := ReadU16Tcp;
  if RespKeyLen = $FFFF then Exit;
  Response[RespLen] := RespKeyLen and $FF; Inc(RespLen);
  Response[RespLen] := (RespKeyLen shr 8) and $FF; Inc(RespLen);

  if RespKeyLen > 0 then
  begin
    if not TcpRecvExact(@Response[RespLen], RespKeyLen) then Exit;
    Inc(RespLen, RespKeyLen);
  end;

  WriteLn('[<] Response: status=', RespStatus, ' data_len=', RespDataLen);

  { Send response back to BTRSERL via COM port }
  if not SerialWriteBytes(@Response, RespLen) then
  begin
    WriteLn('[-] Serial write failed');
    Exit;
  end;

  Inc(gRequestCount);
  Result := True;
end;

{---------------------------------------------------------------------------
  Configuration
---------------------------------------------------------------------------}

procedure LoadConfig;
var
  IniPath: string;
  Buffer: array[0..255] of Char;
begin
  { Get path to INI file (same directory as EXE) }
  SetLength(IniPath, MAX_PATH);
  GetModuleFileName(0, PChar(IniPath), MAX_PATH);
  IniPath := ExtractFilePath(IniPath) + 'XTRIEVE.INI';

  { Read settings }
  GetPrivateProfileString('Server', 'Address', PChar(DEFAULT_SERVER), Buffer, SizeOf(Buffer), PChar(IniPath));
  gServer := Buffer;

  gPort := GetPrivateProfileInt('Server', 'Port', DEFAULT_PORT, PChar(IniPath));

  GetPrivateProfileString('COM', 'Port', PChar(DEFAULT_COM_PORT), Buffer, SizeOf(Buffer), PChar(IniPath));
  gComPort := Buffer;

  WriteLn('[*] Config: ', gComPort, ' -> ', gServer, ':', gPort);
end;

{---------------------------------------------------------------------------
  Main
---------------------------------------------------------------------------}

var
  Colon: Integer;
begin
  WriteLn('===========================================');
  WriteLn('  Xtrieve COM-to-TCP Bridge v1.0 (Delphi)');
  WriteLn('  For Windows 98SE');
  WriteLn('===========================================');
  WriteLn;

  { Load configuration }
  LoadConfig;

  { Allow command-line override }
  if ParamCount >= 1 then
    gComPort := ParamStr(1);

  if ParamCount >= 2 then
  begin
    Colon := Pos(':', ParamStr(2));
    if Colon > 0 then
    begin
      gServer := Copy(ParamStr(2), 1, Colon - 1);
      gPort := StrToIntDef(Copy(ParamStr(2), Colon + 1, 10), DEFAULT_PORT);
    end
    else
      gServer := ParamStr(2);
  end;

  { Initialize COM port }
  if not SerialInit(gComPort) then
  begin
    ExitCode := 1;
    Exit;
  end;

  { Connect to xtrieved }
  if not TcpInit(gServer, gPort) then
  begin
    CloseHandle(hComPort);
    ExitCode := 1;
    Exit;
  end;

  WriteLn;
  WriteLn('[*] Bridge ready - waiting for requests...');
  WriteLn;

  { Main loop }
  while True do
  begin
    if not ProcessRequest then
    begin
      WriteLn('[-] Request failed, reconnecting...');

      { Try to reconnect to xtrieved }
      closesocket(XtrieveSocket);
      if not TcpInit(gServer, gPort) then
      begin
        WriteLn('[-] Reconnect failed, exiting');
        Break;
      end;
    end;
  end;

  { Cleanup }
  closesocket(XtrieveSocket);
  WSACleanup;
  CloseHandle(hComPort);

  WriteLn;
  WriteLn('[*] Bridge stopped. ', gRequestCount, ' requests processed.');
end.
