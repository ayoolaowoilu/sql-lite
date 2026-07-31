
use std::{fs, path::Path};
use std::collections::HashMap;
use serde_json::Value;

const BASE_PATH: &str = r"./src/db_prop";

pub struct DeleteProp {
    pub db_table: String,
    pub target_id: u64,
}

#[derive(PartialEq)]
pub enum UpdateType {
    Value,
    Type,
}

pub struct UpdateProp {
    pub db_table: String,
    pub updates: HashMap<String, Value>,
    pub update_type: UpdateType,
    /// Name of the column to match on (e.g. "id", "email", "name")
    pub target_column: String,
    /// Value to match in that column
    pub target_value: Option<Value>,
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

pub fn delete_line_from_db_table(props: DeleteProp) {
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

        if let Ok(json) = serde_json::from_str::<Value>(line) {
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
        if part.is_empty() {
            continue;
        }
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
        "int" => value.is_u64() || value.is_i64(),
        "float" => value.is_f64(),
        "bool" => value.is_boolean(),
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
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<Value>(line) {
            if let Some(id) = json.get("id").and_then(|v| v.as_u64()) {
                if id > max_id {
                    max_id = id;
                }
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
    match props.update_type {
        UpdateType::Value => update_row_values(props),
        UpdateType::Type => update_schema_types(props),
    }
}

fn update_row_values(props: UpdateProp) {
    let path_str = props.db_table.replace('.', "/");
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
        eprintln!("Empty file");
        return;
    }

    let schema = match parse_schema(lines[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Schema parse error: {}", e);
            return;
        }
    };

    // Validate update values against schema
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

        // Match: compare the column value directly against target_value
        let matches = row.get(&props.target_column) == props.target_value.as_ref();

        if matches {
            found = true;
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
        eprintln!(
            "Row where '{}' = {:?} not found",
            props.target_column, props.target_value
        );
        return;
    }

    let new_content = kept_rows.join("\n") + "\n";
    if let Err(e) = fs::write(&path, new_content) {
        eprintln!("Failed to write file: {}", e);
        return;
    }

    println!(
        "Updated row where {} = {:?} in {}",
        props.target_column, props.target_value, props.db_table
    );
}

fn update_schema_types(props: UpdateProp) {
    let path_str = props.db_table.replace('.', "/");
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
        eprintln!("Empty file");
        return;
    }

    let mut schema = match parse_schema(lines[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Schema parse error: {}", e);
            return;
        }
    };

    // Validate and apply type changes
    // props.updates: key = column name, value = new type (as Value::String)
    for (col_name, new_type_val) in &props.updates {
        let Some(new_type) = new_type_val.as_str() else {
            eprintln!("Type change for '{}' must be a string", col_name);
            return;
        };

        if !schema.contains_key(col_name) {
            eprintln!("Schema update failed: unknown column '{}'", col_name);
            return;
        }

        match new_type {
            "string" | "int" | "float" | "bool" => {}
            _ => {
                eprintln!("Schema update failed: unknown type '{}'", new_type);
                return;
            }
        }

        schema.insert(col_name.clone(), new_type.to_string());
    }

    // Validate existing rows against new schema
    for line in &lines[1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(json) = serde_json::from_str::<Value>(trimmed) else { continue };
        let Some(obj) = json.as_object() else { continue };

        for (col, expected_type) in &schema {
            if let Some(val) = obj.get(col) {
                if !type_matches(val, expected_type) {
                    eprintln!(
                        "Schema change rejected: existing row has {:?} for '{}', expected type '{}'",
                        val, col, expected_type
                    );
                    return;
                }
            }
        }
    }

    // Commit new schema
    let schema_parts: Vec<String> = schema
        .iter()
        .map(|(k, v)| format!("{}->{}", k, v))
        .collect();
    let new_schema_line = format!("[{}]", schema_parts.join(", "));

    let mut new_lines: Vec<String> = vec![new_schema_line];
    new_lines.extend(lines[1..].iter().map(|s| s.to_string()));

    let new_content = new_lines.join("\n") + "\n";
    if let Err(e) = fs::write(&path, new_content) {
        eprintln!("Failed to write file: {}", e);
        return;
    }

    println!("Updated schema for {}", props.db_table);
    for (col, new_type_val) in &props.updates {
        if let Some(t) = new_type_val.as_str() {
            println!("  {} -> {}", col, t);
        }
    }
}