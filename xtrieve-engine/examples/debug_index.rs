//! Debug tool to examine B+ tree index pages

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: debug_index <btrieve_file>");
        return Ok(());
    }

    let filename = &args[1];
    let mut file = File::open(filename)?;
    let file_size = file.metadata()?.len();

    // Read FCR (page 0)
    let mut fcr_buf = vec![0u8; 512];
    file.read_exact(&mut fcr_buf)?;

    let page_size = u16::from_le_bytes([fcr_buf[0x08], fcr_buf[0x09]]) as usize;
    let num_keys = u16::from_le_bytes([fcr_buf[0x14], fcr_buf[0x15]]);
    let record_length = u16::from_le_bytes([fcr_buf[0x16], fcr_buf[0x17]]);
    let num_records = u32::from_le_bytes([fcr_buf[0x1C], fcr_buf[0x1D], fcr_buf[0x1E], fcr_buf[0x1F]]);
    let num_pages = u32::from_le_bytes([fcr_buf[0x20], fcr_buf[0x21], fcr_buf[0x22], fcr_buf[0x23]]);

    println!("=== {} ===", filename);
    println!("File size: {} bytes", file_size);
    println!("Page size: {} bytes", page_size);
    println!("Record length: {} bytes", record_length);
    println!("Number of keys: {}", num_keys);
    println!("Number of records: {}", num_records);
    println!("Number of pages: {}", num_pages);

    // Read index page (page 1)
    println!("\n=== Index Page (page 1) ===");
    file.seek(SeekFrom::Start(page_size as u64))?;
    let mut index_buf = vec![0u8; page_size];
    file.read_exact(&mut index_buf)?;

    // Parse index header
    let page_type = u16::from_le_bytes([index_buf[0], index_buf[1]]);
    let page_num = u16::from_le_bytes([index_buf[2], index_buf[3]]);
    let capacity = u16::from_le_bytes([index_buf[4], index_buf[5]]);
    let entry_count = u16::from_le_bytes([index_buf[6], index_buf[7]]);
    let prev_sibling = u32::from_le_bytes([index_buf[8], index_buf[9], index_buf[10], index_buf[11]]);
    let next_sibling = u32::from_le_bytes([index_buf[12], index_buf[13], index_buf[14], index_buf[15]]);

    println!("Header:");
    println!("  Page type/flags: 0x{:04X}", page_type);
    println!("  Page number: {}", page_num);
    println!("  Capacity: {}", capacity);
    println!("  Entry count: {}", entry_count);
    println!("  Prev sibling: 0x{:08X}", prev_sibling);
    println!("  Next sibling: 0x{:08X}", next_sibling);

    println!("\nEntries (12 bytes each):");
    for i in 0..entry_count.min(20) as usize {
        let entry_offset = 16 + (i * 12);
        if entry_offset + 12 > page_size {
            break;
        }

        let key = u32::from_le_bytes([
            index_buf[entry_offset],
            index_buf[entry_offset + 1],
            index_buf[entry_offset + 2],
            index_buf[entry_offset + 3],
        ]);
        let offset_high = u16::from_le_bytes([
            index_buf[entry_offset + 4],
            index_buf[entry_offset + 5],
        ]);
        let offset_low = u16::from_le_bytes([
            index_buf[entry_offset + 6],
            index_buf[entry_offset + 7],
        ]);
        let dup_ptr = u32::from_le_bytes([
            index_buf[entry_offset + 8],
            index_buf[entry_offset + 9],
            index_buf[entry_offset + 10],
            index_buf[entry_offset + 11],
        ]);

        let file_offset = ((offset_high as u32) << 16) | (offset_low as u32);

        println!("  Entry {}: key={}, file_offset=0x{:08X} (high={}, low=0x{:04X}), dup_ptr=0x{:08X}",
            i, key, file_offset, offset_high, offset_low, dup_ptr);

        // Try to read record at that offset
        if file_offset < file_size as u32 {
            let mut record_buf = vec![0u8; record_length.min(32) as usize];
            file.seek(SeekFrom::Start(file_offset as u64))?;
            file.read_exact(&mut record_buf)?;

            let record_key = i32::from_le_bytes([record_buf[0], record_buf[1], record_buf[2], record_buf[3]]);
            let text_end = record_buf[4..].iter().position(|&b| b == 0).unwrap_or(20);
            let text = String::from_utf8_lossy(&record_buf[4..4+text_end.min(20)]);
            println!("    -> Record at 0x{:08X}: ID={}, text=\"{}\"", file_offset, record_key, text);
        }
    }

    if entry_count > 20 {
        println!("  ... ({} more entries)", entry_count - 20);
    }

    Ok(())
}
