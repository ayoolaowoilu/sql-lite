use std::{ eprintln, fs , path::Path, println};
use std::collections::HashMap;
use serde_json::Value;

const BASE_PATH: &str = r"./src/db_prop";

pub struct DeleteProp {
   pub  db_table : String,
     pub target_id : u64
}
pub struct UpdateProp {
    pub db_table: String,
    pub target_id: u64,
    pub updates: HashMap<String, Value>, // columns to change
}

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

pub fn delete_line_from_db_table(props:DeleteProp) {

    let formatted_string = props.db_table.replace(".", "/");
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
            if json.get("id") == Some(&serde_json::json!(props.target_id)) {
                found = true;
                continue; 
            }
        }
        kept_rows.push(line.to_string());
    }

    if !found {
        eprintln!("Row with id {} not found", props.target_id);
        return;
    }

 
    let new_content = kept_rows.join("\n") + "\n";
    if let Err(e) = fs::write(path, new_content) {
        eprintln!("Failed to write file: {}", e);
        return;
    }

    println!("Deleted row with id {}", props.target_id);
}

fn parse_schema(line: &str) -> Result<HashMap<String, String>, String> {
    let inner = line.trim();
    let inner = inner.strip_prefix('[').ok_or("Missing [")?;
    let inner = inner.strip_suffix(']').ok_or("Missing ]")?;

    let mut map = HashMap::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() { continue; }
        let Some((name, ty)) = part.split_once("->") else {
            return Err(format!("Bad schema part: '{}'", part));
        };
        map.insert(name.trim().to_string(), ty.trim().to_string());
    }
    Ok(map)
}


fn type_matches(value: &Value, expected: &str) -> bool {
    match expected {
        "string" => value.is_string(),
        "int"    => value.is_u64() || value.is_i64(),
        "float"  => value.is_f64(),
        "bool"   => value.is_boolean(),
        _ => {
            eprintln!("Warning: unknown schema type '{}'", expected);
            false
        }
    }
}

pub fn add_line_to_db_table(db_table: &str, data: HashMap<String, Value>) {
    let path_str = db_table.replace('.', "/");
    let path = format!("{}/{}.txt", BASE_PATH, path_str);

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return;
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        eprintln!("Empty file — no schema");
        return;
    }


    let schema = match parse_schema(lines[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Schema parse error: {}", e);
            return;
        }
    };

 
    for key in data.keys() {
        if !schema.contains_key(key) {
            eprintln!("Validation failed: unknown column '{}'", key);
            return;
        }
    }

  
    for (col_name, col_type) in &schema {
        let Some(value) = data.get(col_name) else {
            eprintln!("Validation failed: missing column '{}'", col_name);
            return;
        };
        if !type_matches(value, col_type) {
            eprintln!(
                "Validation failed: '{}' expected '{}', got {:?}",
                col_name, col_type, value
            );
            return;
        }
    }


    let mut max_id = 0u64;
    for line in &lines[1..] {
        if line.trim().is_empty() { continue; }
        if let Ok(json) = serde_json::from_str::<Value>(line) {
            if let Some(id) = json.get("id").and_then(|v| v.as_u64()) {
                if id > max_id { max_id = id; }
            }
        }
    }
    let new_id = max_id + 1;

   
    let mut row = serde_json::Map::new();
    row.insert("id".to_string(), Value::Number(new_id.into()));
    for (k, v) in data {
        row.insert(k, v);
    }

    let new_line = match serde_json::to_string(&Value::Object(row)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Serialize error: {}", e);
            return;
        }
    };


    let mut new_content = content;
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(&new_line);
    new_content.push('\n');

    if let Err(e) = fs::write(&path, new_content) {
        eprintln!("Failed to write: {}", e);
        return;
    }

    println!("Added row id {} to {}", new_id, db_table);
}


pub fn update_db_table(props: UpdateProp) {
    let path_str = props.db_table.replace('.', "/");
    let path = format!("{}/{}.txt", BASE_PATH, path_str);

    // READ
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return;
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        eprintln!("Empty file");
        return;
    }

    // PARSE SCHEMA
    let schema = match parse_schema(lines[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Schema parse error: {}", e);
            return;
        }
    };

    // VALIDATE: every column being updated must exist and match type
    for (col_name, value) in &props.updates {
        let Some(expected_type) = schema.get(col_name) else {
            eprintln!("Validation failed: unknown column '{}'", col_name);
            return;
        };
        if !type_matches(value, expected_type) {
            eprintln!(
                "Validation failed: '{}' expected '{}', got {:?}",
                col_name, expected_type, value
            );
            return;
        }
    }

    // PROCESS ROWS
    let mut kept_rows: Vec<String> = vec![lines[0].to_string()];
    let mut found = false;

    for line in &lines[1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut row: serde_json::Map<String, Value> = match serde_json::from_str(trimmed) {
            Ok(Value::Object(map)) => map,
            Ok(_) => {
                kept_rows.push(trimmed.to_string());
                continue;
            }
            Err(_) => {
                kept_rows.push(trimmed.to_string());
                continue;
            }
        };

        // Check if this is the row to update
        let row_id = row.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        if row_id == props.target_id {
            found = true;
            // Apply updates
            for (k, v) in &props.updates {
                row.insert(k.clone(), v.clone());
            }
            let updated_json = serde_json::to_string(&Value::Object(row)).unwrap();
            kept_rows.push(updated_json);
        } else {
            kept_rows.push(trimmed.to_string());
        }
    }

    if !found {
        eprintln!("Row with id {} not found", props.target_id);
        return;
    }

    // WRITE BACK
    let new_content = kept_rows.join("\n") + "\n";
    if let Err(e) = fs::write(&path, new_content) {
        eprintln!("Failed to write file: {}", e);
        return;
    }

    println!("Updated row id {} in {}", props.target_id, props.db_table);
}