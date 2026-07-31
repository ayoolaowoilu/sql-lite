use std::{fs, path::Path};

const BASE_PATH: &str = r"./src/db_prop";

fn auth_db(name: &str) -> bool {
    let Ok(entries) = Path::new(BASE_PATH).read_dir() else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        if let Some(file_name) = entry.file_name().to_str() {
            if file_name == name {
                return true;
            }
        }
    }
    false
}

pub fn read_db(db: &str) {
    if !auth_db(db) {
        eprintln!("Database '{}' not found", db);
        return;
    }

    let path = format!("{}/{}", BASE_PATH, db);
    let Ok(entries) = fs::read_dir(&path) else {
        eprintln!("Cannot read database: {}", path);
        return;
    };

    println!("Database: {}\n", db);

    for entry in entries {
        let Ok(entry) = entry else { continue };


        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }

        let file_name = match entry.file_name().into_string() {
            Ok(s) => s,
            Err(_) => continue,
        };

        
        let content = match fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  Error reading {}: {}", file_name, e);
                continue;
            }
        };

        let mut lines = content.lines();

        // --- TOP LINE: Schema ---
        let schema_line = match lines.next() {
            Some(line) => line.trim(),
            None => {
                println!("  Table: {} (empty file)\n", file_name);
                continue;
            }
        };

        println!("  Table: {}", file_name);
        println!("  Schema: {}", schema_line);

        // Parse schema into clean pairs
        let schema_clean = schema_line
            .trim_start_matches('[')
            .trim_end_matches(']');
        
        for part in schema_clean.split(',') {
            let part = part.trim();
            if let Some((col_name, col_type)) = part.split_once("->") {
                println!("    Column: {} ({})", col_name.trim(), col_type.trim());
            }
        }

  
        println!("  Rows:");
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            println!("    [{}] {}", i + 1, line);
        }
        println!();
    }
}

