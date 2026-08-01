# sql lite

A lightweight, file-based database engine written in Rust. No external database server required — every database is a folder, every table is a text file.

---

## How It Works

```
src/db_prop/
├── mydb/                    ← Database (folder)
│   ├── users.txt            ← Table (text file)
│   └── orders.txt           ← Another table
└── another_db/
    └── products.txt
```

Each table file has:
- **Line 1**: Schema definition
- **Lines 2+**: One JSON object per row

---

## Schema Format

```
[id->int, name->string, age->int, active->bool]
```

| Type     | JSON Example              |
|----------|---------------------------|
| `string` | `"Alice"`                 |
| `int`    | `42`                      |
| `float`  | `3.14`                    |
| `bool`   | `true` / `false`          |

The `id` column is auto-injected if omitted. It auto-increments on every insert.

---

## Row Format

Each row is a single line of compact JSON:

```json
{"id":1,"name":"Alice","age":25,"active":true}
{"id":2,"name":"Bob","age":30,"active":false}
```

---

## API

### Create a Database

```rust
create_database("mydb");
```

Creates the folder `./src/db_prop/mydb/`.

---

### Create a Table

```rust
let schemas = vec![
    "name->string",
    "age->int",
    "active->bool",
];

create_table("mydb.users", schemas);
```

Creates `./src/db_prop/mydb/users.txt` with:
```
[id->int, name->string, age->int, active->bool]
```

---

### Insert a Row

```rust
use std::collections::HashMap;
use serde_json::json;

let mut data = HashMap::new();
data.insert("name".into(), json!("Alice"));
data.insert("age".into(), json!(25));
data.insert("active".into(), json!(true));

add_line_to_db_table("mydb.users", data);
```

Output:
```
Added row id 1 to mydb.users
```

---

### Query (SELECT)

```rust
let rows = read_table(&QueryProp {
    db_table: "mydb.users".into(),
    select: vec!["name".into(), "age".into()],   // empty = SELECT *
    where_col: Some("active".into()),             // optional filter
    where_val: Some(json!(true)),
    limit: Some(5),                               // optional limit
});

for row in rows {
    println!("{:?}", row);
}
```

---

### Update a Row

```rust
let mut updates = HashMap::new();
updates.insert("name".into(), json!("Alicia"));

update_db_table(UpdateProp {
    db_table: "mydb.users".into(),
    updates,
    update_type: UpdateType::Value,
    target_column: "id".into(),
    target_value: Some(json!(1)),
});
```

---

### Update a Column Type (Schema Change)

```rust
let mut type_changes = HashMap::new();
type_changes.insert("age".into(), json!("float"));

update_db_table(UpdateProp {
    db_table: "mydb.users".into(),
    updates: type_changes,
    update_type: UpdateType::Type,
    target_column: "".into(),
    target_value: None,
});
```

Rejects the change if existing rows violate the new type.

---

### Delete a Row

```rust
delete_line_from_db_table(DeleteProp {
    db_table: "mydb.users".into(),
    target_id: 1,
});
```

---

### Read All Tables in a Database

```rust
read_db("mydb");
```

Prints every table and its schema to stdout.

---

## Type System

| Rust `serde_json::Value` | Schema Type | Validated By |
|---------------------------|-------------|--------------|
| `Value::String`          | `string`    | `value.is_string()` |
| `Value::Number` (int)    | `int`       | `value.is_u64() \|\| value.is_i64()` |
| `Value::Number` (float)  | `float`     | `value.is_f64()` |
| `Value::Bool`            | `bool`      | `value.is_boolean()` |

All inserts and updates are validated against the schema before writing.

---

## Limitations

- **No indexes** — every `WHERE` query scans the entire table file.
- **No joins** — query one table at a time.
- **No transactions** — writes are immediate; a crash mid-write can corrupt a table.
- **Single-file per table** — large tables = large files = slower reads.
- **No concurrent access safety** — two threads writing the same file will race.

This is designed for small-scale, embedded, or prototyping use cases. For production workloads, use SQLite, PostgreSQL, or another full database engine.

---

## File Example

`./src/db_prop/shop/products.txt`:
```
[id->int, name->string, price->float, in_stock->bool]
{"id":1,"name":"Keyboard","price":49.99,"in_stock":true}
{"id":2,"name":"Mouse","price":19.99,"in_stock":false}
{"id":3,"name":"Monitor","price":199.99,"in_stock":true}
```

---

## Dependencies

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

---

## License

MIT / whatever you want — it's your project.
