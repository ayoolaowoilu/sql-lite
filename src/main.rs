use std::collections::HashMap;

use serde_json::Value;

use crate::utils::read_write_fn::{UpdateProp, add_line_to_db_table, read_db, update_db_table};


mod utils;


fn main() {
    println!("Hello, world!");
    read_db("default");

        let mut row = HashMap::new();
    row.insert("name".to_string(), Value::String("Dave".to_string()));
    row.insert("email".to_string(), Value::String("dave@x.com".to_string()));
    row.insert("id".to_string(), Value::Number(1.into()));

    add_line_to_db_table("default.users", row);

    let mut changes = HashMap::new();
changes.insert("name".to_string(), Value::String("Alice makindddddd".to_string()));
changes.insert("id".to_string(), Value::Number(3.into()));

let prop = UpdateProp {
    db_table: "default.users".to_string(),
    target_id: 1,
    updates: changes,
};

update_db_table(prop);

}
