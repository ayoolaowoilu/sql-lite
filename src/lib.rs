mod utils;

mod tests {
    use std::collections::HashMap;
    use serde_json::json;

use crate::utils::read_write_fn::{BASE_PATH, UpdateProp, UpdateType, add_line_to_db_table, create_table, update_db_table, update_schema_types};
   

    #[test]
    fn test_create_table() {
        let db_props = "testdb.testtable";
        let schemas = vec![
            "name->string".to_string(),
            "age->int".to_string(),
            "active->bool".to_string(),
        ];
    
        create_table(db_props, schemas.iter().collect());

      
        let file_path = format!("{}/{}.txt", BASE_PATH, db_props.replace('.', "/"));
        assert!(std::path::Path::new(&file_path).exists());
    }

    #[test]
    fn test_add_line_to_db_table() {
        let db_table = "testdb.testtable";
        let mut row = HashMap::new();
        row.insert("name".to_string(), json!("John Doe"));
        row.insert("age".to_string(), json!(30));
        row.insert("active".to_string(), json!(true));
        row.insert("id".to_string(), json!(1)); 

        add_line_to_db_table(db_table, row);

       
        let file_path = format!("{}/{}.txt", BASE_PATH, db_table.replace('.', "/"));
        let content = std::fs::read_to_string(file_path).expect("Failed to read file");
        assert!(content.contains("John Doe"));
    }

    #[test] 
    fn test_create_db() {
        let db_name = "testdb";
        crate::utils::read_write_fn::create_db(db_name);

        let db_path = format!("{}/{}", BASE_PATH, db_name);
        assert!(std::path::Path::new(&db_path).exists());
    }

    #[test]
    fn test_update_schema_types(){
          let mut row = HashMap::new();
        row.insert("name".to_string(), json!("John Doe"));
        row.insert("age".to_string(), json!(30));
        row.insert("active".to_string(), json!(true));
          row.insert("id".to_string(), json!(2));

         add_line_to_db_table("testdb.testable",row );
          update_schema_types(UpdateProp {
            db_table: "testdb.testtable".into(),
            updates: HashMap::from([("name".into() , json!("string"))]),
            update_type: UpdateType::Type,
            target_column: "name".into(),
            target_value: None
        });

        let content = std::fs::read_to_string(format!("{}/testdb/testtable.txt", BASE_PATH)).expect("Failed to read file");
        assert!(content.contains("name->string"));

 

        
    }
}