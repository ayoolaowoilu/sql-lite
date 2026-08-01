use std::{arch::x86_64::_mm_dpbusd_avx_epi32, collections::HashMap, fs::{self, read_to_string}, println, vec};

use axum::{Router, response::Html, routing::{get, post}};
use serde_json::{Value, json};
use tokio::net::windows::named_pipe::PipeEnd::Server;

use crate::utils::read_write_fn::{UpdateProp, UpdateType, add_line_to_db_table, create_db, create_table, read_db, update_db_table};


mod utils;

#[tokio::main]
async fn main() {
    

    let html_file = fs::read_to_string("static/index.html").unwrap();

    let router = Router::new()
        .route("/", get(|| async { Html(html_file) }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://localhost:3000");

    axum::serve(listener, router).await.unwrap();
     
      
//     read_db("Killa");

//         let mut row = HashMap::new();
//     row.insert("name".to_string(), Value::String("Dave".to_string()));
//     row.insert("email".to_string(), Value::String("dave@x.com".to_string()));
//     row.insert("id".to_string(), Value::Number(3.into()));

//     add_line_to_db_table("default.users", row);


// let prop = UpdateProp {
//     db_table: "default.users".into(),
//     updates: HashMap::from([("name".into(), json!("Alice"))]),
//     update_type: UpdateType::Value,
//     target_column: "id".into(),
//     target_value: Some(json!(1)),  
// };
// update_db_table(prop);
// let schemas = vec![
//     "name->string".to_string(),
//     "age->int".to_string(),
//     "active->bool".to_string(),
// ];

// create_table("killa.users", schemas.iter().collect());

}



