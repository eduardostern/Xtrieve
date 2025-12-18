// Serial-to-Xtrieve Bridge (Protocol-Aware)
// Parses Xtrieve protocol to detect packet boundaries
//
// Request:  [op:2][pos:128][dlen:4][data:N][klen:2][key:N][knum:2][plen:2][path:N][lock:2]
// Response: [status:2][pos:128][dlen:4][data:N][klen:2][key:N]

use std::env;
use std::io::{Read, Write, BufReader, BufWriter};
use std::net::{TcpListener, TcpStream};
use std::thread;

const DEFAULT_LISTEN_PORT: u16 = 7418;
const DEFAULT_XTRIEVE_ADDR: &str = "127.0.0.1:7419";
const POS_BLOCK_SIZE: usize = 128;

fn read_exact<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<()> {
    let mut total = 0;
    while total < buf.len() {
        let n = reader.read(&mut buf[total..])?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed",
            ));
        }
        total += n;
    }
    Ok(())
}

fn read_u16<R: Read>(reader: &mut R) -> std::io::Result<u16> {
    let mut buf = [0u8; 2];
    read_exact(reader, &mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32<R: Read>(reader: &mut R) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    read_exact(reader, &mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

/// Wait for sync marker 0xBB 0xBB
fn wait_for_sync<R: Read>(reader: &mut R) -> std::io::Result<()> {
    let mut buf = [0u8; 1];
    let mut found_first = false;

    loop {
        read_exact(reader, &mut buf)?;
        if buf[0] == 0xBB {
            if found_first {
                // Got 0xBB 0xBB - sync found!
                return Ok(());
            }
            found_first = true;
        } else {
            if found_first {
                println!("    [sync] skipping 0x{:02X} after first 0xBB", buf[0]);
            } else if buf[0] != 0xFF && buf[0] != 0x00 {
                println!("    [sync] skipping garbage byte 0x{:02X}", buf[0]);
            }
            found_first = false;
        }
    }
}

/// Read a complete Xtrieve request from DOS
/// Returns the serialized request bytes
fn read_request<R: Read>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::with_capacity(512);

    // Wait for sync marker first
    wait_for_sync(reader)?;
    println!("    [sync] got sync marker");

    // Operation code (2 bytes)
    let op = read_u16(reader)?;
    request.extend_from_slice(&op.to_le_bytes());
    println!("    op={}", op);

    // Position block (128 bytes)
    let mut pos_block = [0u8; POS_BLOCK_SIZE];
    read_exact(reader, &mut pos_block)?;
    request.extend_from_slice(&pos_block);

    // Data length (4 bytes) + data
    let data_len = read_u32(reader)?;
    request.extend_from_slice(&data_len.to_le_bytes());
    println!("    data_len={}", data_len);

    if data_len > 0 {
        let mut data = vec![0u8; data_len as usize];
        read_exact(reader, &mut data)?;
        request.extend_from_slice(&data);
    }

    // Key length (2 bytes) + key
    let key_len = read_u16(reader)?;
    request.extend_from_slice(&key_len.to_le_bytes());
    println!("    key_len={}", key_len);

    if key_len > 0 {
        let mut key = vec![0u8; key_len as usize];
        read_exact(reader, &mut key)?;
        request.extend_from_slice(&key);
    }

    // Key number (2 bytes)
    let key_num = read_u16(reader)?;
    request.extend_from_slice(&key_num.to_le_bytes());

    // Path length (2 bytes) + path
    let path_len = read_u16(reader)?;
    request.extend_from_slice(&path_len.to_le_bytes());
    println!("    path_len={}", path_len);

    if path_len > 0 {
        let mut path = vec![0u8; path_len as usize];
        read_exact(reader, &mut path)?;
        request.extend_from_slice(&path);
        if let Ok(s) = std::str::from_utf8(&path) {
            println!("    path={}", s);
        }
    }

    // Lock bias (2 bytes)
    let lock = read_u16(reader)?;
    request.extend_from_slice(&lock.to_le_bytes());

    println!("    total request size: {} bytes", request.len());
    Ok(request)
}

/// Read a complete Xtrieve response from server
/// Returns the serialized response bytes
fn read_response<R: Read>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut response = Vec::with_capacity(512);

    // Status code (2 bytes)
    let status = read_u16(reader)?;
    response.extend_from_slice(&status.to_le_bytes());
    println!("    status={}", status);

    // Position block (128 bytes)
    let mut pos_block = [0u8; POS_BLOCK_SIZE];
    read_exact(reader, &mut pos_block)?;
    response.extend_from_slice(&pos_block);

    // Data length (4 bytes) + data
    let data_len = read_u32(reader)?;
    response.extend_from_slice(&data_len.to_le_bytes());
    println!("    resp_data_len={}", data_len);

    if data_len > 0 {
        let mut data = vec![0u8; data_len as usize];
        read_exact(reader, &mut data)?;
        response.extend_from_slice(&data);
    }

    // Key length (2 bytes) + key
    let key_len = read_u16(reader)?;
    response.extend_from_slice(&key_len.to_le_bytes());

    if key_len > 0 {
        let mut key = vec![0u8; key_len as usize];
        read_exact(reader, &mut key)?;
        response.extend_from_slice(&key);
    }

    println!("    total response size: {} bytes", response.len());
    Ok(response)
}

fn handle_client(dos_stream: TcpStream, xtrieve_addr: &str) {
    let peer = dos_stream.peer_addr().ok();
    println!("[+] DOS client connected: {:?}", peer);

    // Connect to Xtrieve server
    let xtrieve_stream = match TcpStream::connect(xtrieve_addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[-] Failed to connect to Xtrieve: {}", e);
            return;
        }
    };
    println!("[+] Connected to Xtrieve at {}", xtrieve_addr);

    let mut dos_reader = BufReader::new(&dos_stream);
    let mut dos_writer = BufWriter::new(&dos_stream);
    let mut xtrieve_reader = BufReader::new(&xtrieve_stream);
    let mut xtrieve_writer = BufWriter::new(&xtrieve_stream);

    let mut request_count = 0u64;

    loop {
        // Read complete request from DOS
        println!("\n[>] Reading request #{}...", request_count + 1);
        let request = match read_request(&mut dos_reader) {
            Ok(r) => r,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    println!("[*] DOS client disconnected");
                } else {
                    eprintln!("[-] Error reading request: {}", e);
                }
                break;
            }
        };

        // Forward to Xtrieve
        println!("[>] Forwarding {} bytes to Xtrieve", request.len());
        if let Err(e) = xtrieve_writer.write_all(&request) {
            eprintln!("[-] Error writing to Xtrieve: {}", e);
            break;
        }
        if let Err(e) = xtrieve_writer.flush() {
            eprintln!("[-] Error flushing to Xtrieve: {}", e);
            break;
        }

        // Read complete response from Xtrieve
        println!("[<] Reading response from Xtrieve...");
        let response = match read_response(&mut xtrieve_reader) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[-] Error reading response: {}", e);
                break;
            }
        };

        // Forward to DOS
        println!("[<] Forwarding {} bytes to DOS", response.len());
        if let Err(e) = dos_writer.write_all(&response) {
            eprintln!("[-] Error writing to DOS: {}", e);
            break;
        }
        if let Err(e) = dos_writer.flush() {
            eprintln!("[-] Error flushing to DOS: {}", e);
            break;
        }

        request_count += 1;
        println!("[*] Request #{} complete", request_count);
    }

    println!("[-] Session ended: {} requests processed", request_count);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let listen_port: u16 = args.get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LISTEN_PORT);

    let xtrieve_addr = args.get(2)
        .map(|s| s.as_str())
        .unwrap_or(DEFAULT_XTRIEVE_ADDR);

    println!("===========================================");
    println!("  Xtrieve Serial Bridge (Protocol-Aware)");
    println!("===========================================");
    println!("Listening on port {} for DOSBox-X", listen_port);
    println!("Forwarding to Xtrieve at {}", xtrieve_addr);
    println!();
    println!("Protocol:");
    println!("  Request:  [op:2][pos:128][dlen:4][data][klen:2][key][knum:2][plen:2][path][lock:2]");
    println!("  Response: [status:2][pos:128][dlen:4][data][klen:2][key]");
    println!();
    println!("DOSBox-X config:");
    println!("  serial1=nullmodem server:127.0.0.1 port:{}", listen_port);
    println!();

    let listener = TcpListener::bind(format!("0.0.0.0:{}", listen_port))
        .expect("Failed to bind listener");

    println!("[*] Waiting for DOS connections...\n");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let addr = xtrieve_addr.to_string();
                thread::spawn(move || {
                    handle_client(s, &addr);
                });
            }
            Err(e) => {
                eprintln!("[-] Accept error: {}", e);
            }
        }
    }
}
