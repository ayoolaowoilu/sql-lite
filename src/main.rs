use std::collections::HashMap;

use serde_json::{Value, json};

use crate::utils::read_write_fn::{UpdateProp, UpdateType, add_line_to_db_table, read_db, update_db_table};


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
    db_table: "users".into(),
    updates: HashMap::from([("name".into(), json!("Alice"))]),
    update_type: UpdateType::Value,
    target_column: "id".into(),
    target_value: Some(json!(3)),  
};
update_db_table(prop);

}
