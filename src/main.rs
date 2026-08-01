use std::{collections::HashMap, vec};

use serde_json::{Value, json};

use crate::utils::read_write_fn::{UpdateProp, UpdateType, add_line_to_db_table, create_db, create_table, read_db, update_db_table};


mod utils;


fn main() {
    println!("Hello, world!");
    read_db("Killa");

        let mut row = HashMap::new();
    row.insert("name".to_string(), Value::String("Dave".to_string()));
    row.insert("email".to_string(), Value::String("dave@x.com".to_string()));
    row.insert("id".to_string(), Value::Number(3.into()));

    add_line_to_db_table("default.users", row);


let prop = UpdateProp {
    db_table: "default.users".into(),
    updates: HashMap::from([("name".into(), json!("Alice"))]),
    update_type: UpdateType::Value,
    target_column: "id".into(),
    target_value: Some(json!(1)),  
};
update_db_table(prop);
let schemas = vec![
    "name->string".to_string(),
    "age->int".to_string(),
    "active->bool".to_string(),
];

// create_table("killa.users", schemas.iter().collect());

}
