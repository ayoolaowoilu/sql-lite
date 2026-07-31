use std::collections::HashMap;

use serde_json::Value;

use crate::utils::read_write_fn::{add_line_to_db_table, read_db};


mod utils;


fn main() {
    println!("Hello, world!");
    read_db("default");

        let mut row = HashMap::new();
    row.insert("name".to_string(), Value::String("Dave".to_string()));
    row.insert("email".to_string(), Value::String("dave@x.com".to_string()));
    row.insert("id".to_string(), Value::Number(1.into()));

    add_line_to_db_table("default.users", row);

}
