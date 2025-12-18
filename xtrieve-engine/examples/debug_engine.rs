//! Debug tool to test engine operations directly

use std::path::PathBuf;
use xtrieve_engine::file_manager::open_files::{OpenFileTable, OpenMode};
use xtrieve_engine::storage::btree::IndexNode;
use xtrieve_engine::storage::key::KeySpec;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: debug_engine <btrieve_file>");
        return Ok(());
    }

    let filename = &args[1];
    let path = PathBuf::from(filename);

    println!("=== Testing Engine with {} ===\n", filename);

    // Open file through engine
    let open_files = OpenFileTable::new();
    let file = match open_files.open(&path, OpenMode::read_only()) {
        Ok(f) => f,
        Err(e) => {
            println!("Error opening file: {:?}", e);
            return Ok(());
        }
    };

    let f = file.read();
    println!("FCR:");
    println!("  Page size: {}", f.fcr.page_size);
    println!("  Record length: {}", f.fcr.record_length);
    println!("  Num keys: {}", f.fcr.num_keys);
    println!("  Num records: {}", f.fcr.num_records);
    println!("  Num pages: {}", f.fcr.num_pages);
    println!("  First data page: {}", f.fcr.first_data_page);
    println!("  Index roots: {:?}", f.fcr.index_roots);

    if f.fcr.num_keys > 0 {
        let key_spec = &f.fcr.keys[0];
        let root_page = f.fcr.index_roots.get(0).copied().unwrap_or(1);

        println!("\nKey 0 spec:");
        println!("  Position: {}", key_spec.position);
        println!("  Length: {}", key_spec.length);
        println!("  Root page: {}", root_page);

        // Read root page
        println!("\nReading index page {}...", root_page);
        match f.read_page(root_page) {
            Ok(page) => {
                println!("  Page data len: {} bytes", page.data.len());
                println!("  First 32 bytes: {:02x?}", &page.data[..32.min(page.data.len())]);

                // Parse as IndexNode
                match IndexNode::from_bytes(root_page, &page.data, key_spec.clone()) {
                    Ok(node) => {
                        println!("\nIndexNode:");
                        println!("  Entry count: {}", node.entry_count);
                        println!("  Is leaf: {}", node.is_leaf());
                        println!("  Prev sibling: {}", node.prev_sibling);
                        println!("  Next sibling: {}", node.next_sibling);
                        println!("  Leaf entries: {}", node.leaf_entries.len());

                        for (i, entry) in node.leaf_entries.iter().enumerate().take(10) {
                            println!("  Entry {}: key={:?}, addr=page:{} slot:{}",
                                i,
                                entry.key,
                                entry.record_address.page,
                                entry.record_address.slot);

                            // Try to read the record
                            let file_offset = entry.record_address.page;
                            let page_size = f.fcr.page_size as u32;
                            let data_page_num = file_offset / page_size;
                            let offset_in_page = (file_offset % page_size) as usize;

                            println!("    -> File offset: 0x{:08X}", file_offset);
                            println!("    -> Data page: {}, offset in page: {}", data_page_num, offset_in_page);

                            if let Ok(data_page) = f.read_page(data_page_num) {
                                let record_len = f.fcr.record_length as usize;
                                if offset_in_page + record_len <= data_page.data.len() {
                                    let record_data = &data_page.data[offset_in_page..offset_in_page + record_len];
                                    let id = i32::from_le_bytes([record_data[0], record_data[1], record_data[2], record_data[3]]);
                                    let text_end = record_data[4..].iter().position(|&b| b == 0).unwrap_or(20);
                                    let text = String::from_utf8_lossy(&record_data[4..4+text_end.min(20)]);
                                    println!("    -> Record: ID={}, text=\"{}\"", id, text);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("  Error parsing IndexNode: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("  Error reading page: {:?}", e);
            }
        }
    }

    drop(f);
    drop(file);
    open_files.close(&path);

    Ok(())
}
