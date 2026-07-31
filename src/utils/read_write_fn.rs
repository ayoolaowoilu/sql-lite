use std::{eprint, eprintln, fs, mem::replace, path::Path, println};

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


        let schema_line = match lines.next() {
            Some(line) => line.trim(),
            None => {
                println!("  Table: {} (empty file)\n", file_name);
                continue;
            }
        };

        println!("  Table: {}", file_name);
        println!("  Schema: {}", schema_line);

        
        let schema_clean = schema_line
            .trim_start_matches('[')
            .trim_end_matches(']');
        
        for part in schema_clean.split(',') {
            let part = part.trim();
            if let Some((col_name, col_type)) = part.split_once("->") {
                println!("    Column: {} ({})", col_name.trim(), col_type.trim());
            }
        }
        println!();
    }
}

pub fn delete_line_from_db_table(db_table: &str, target_id: u64) {

    let formatted_string = db_table.replace(".", "/");
    let path = format!("{}/{}.txt", BASE_PATH, formatted_string);
    let path = Path::new(&path);

  
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return; 
        }
    };

    let mut lines: Vec<&str> = content.lines().collect();
    
    if lines.is_empty() {
        eprintln!("Empty file");
        return;
    }

   
    let schema = lines[0];
    let mut kept_rows: Vec<String> = vec![schema.to_string()];
    let mut found = false;

    for line in &lines[1..] {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

  
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json.get("id") == Some(&serde_json::json!(target_id)) {
                found = true;
                continue; 
            }
        }
        kept_rows.push(line.to_string());
    }

    if !found {
        eprintln!("Row with id {} not found", target_id);
        return;
    }

 
    let new_content = kept_rows.join("\n") + "\n";
    if let Err(e) = fs::write(path, new_content) {
        eprintln!("Failed to write file: {}", e);
        return;
    }

    println!("Deleted row with id {}", target_id);
}

