use std::fs;

use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower_http::services::ServeDir;
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod utils;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    history: Arc<Mutex<Vec<QueryRecord>>>,
    logs: Arc<Mutex<Vec<LogEntry>>>,
}

#[derive(Clone, Serialize)]
struct QueryRecord {
    sql: String,
    timestamp: String,
    duration_ms: u128,
    row_count: usize,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct LogEntry {
    time: String,
    level: String,
    message: String,
}

#[derive(Deserialize)]
struct QueryRequest {
    sql: String,
}

#[derive(Serialize)]
struct QueryResult {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    duration_ms: u128,
    row_count: usize,
    error: Option<String>,
}

#[derive(Serialize)]
struct SchemaResponse {
    databases: Vec<DatabaseInfo>,
}

#[derive(Serialize)]
struct DatabaseInfo {
    name: String,
    tables: Vec<TableInfo>,
}

#[derive(Serialize)]
struct TableInfo {
    name: String,
    columns: Vec<String>,
}

#[derive(Serialize)]
struct HistoryResponse {
    queries: Vec<QueryRecord>,
}

#[derive(Serialize)]
struct LogsResponse {
    logs: Vec<LogEntry>,
}

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            username TEXT NOT NULL,
            email TEXT,
            created_at TEXT,
            active INTEGER DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS products (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            price REAL,
            stock INTEGER DEFAULT 0,
            category TEXT
        );
        CREATE TABLE IF NOT EXISTS orders (
            id INTEGER PRIMARY KEY,
            user_id INTEGER,
            product_id INTEGER,
            quantity INTEGER,
            total REAL,
            order_date TEXT,
            status TEXT DEFAULT 'pending'
        );
        CREATE TABLE IF NOT EXISTS logs (
            id INTEGER PRIMARY KEY,
            level TEXT,
            message TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );"
    )?;

    let users = [
        (1, "alice", "alice@example.com", "2024-01-15", 1),
        (2, "bob", "bob@example.com", "2024-02-20", 1),
        (3, "charlie", "charlie@example.com", "2024-03-10", 0),
        (4, "diana", "diana@example.com", "2024-04-05", 1),
        (5, "eve", "eve@example.com", "2024-05-12", 1),
    ];
    for (id, name, email, date, active) in users {
        conn.execute(
            "INSERT OR IGNORE INTO users (id, username, email, created_at, active) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, name, email, date, active],
        )?;
    }

    let products = [
        (1, "Laptop Pro", 1299.99, 45, "Electronics"),
        (2, "Wireless Mouse", 29.99, 200, "Electronics"),
        (3, "USB-C Hub", 79.99, 150, "Electronics"),
        (4, "Standing Desk", 499.50, 12, "Furniture"),
        (5, "Ergonomic Chair", 349.00, 8, "Furniture"),
        (6, "Notebook Pack", 14.99, 500, "Stationery"),
        (7, "Monitor 4K", 399.99, 30, "Electronics"),
    ];
    for (id, name, price, stock, cat) in products {
        conn.execute(
            "INSERT OR IGNORE INTO products (id, name, price, stock, category) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, name, price, stock, cat],
        )?;
    }

    let orders = [
        (1, 1, 1, 1, 1299.99, "2024-06-01", "completed"),
        (2, 1, 2, 2, 59.98, "2024-06-02", "completed"),
        (3, 2, 4, 1, 499.50, "2024-06-03", "shipped"),
        (4, 3, 6, 5, 74.95, "2024-06-04", "pending"),
        (5, 4, 7, 2, 799.98, "2024-06-05", "processing"),
        (6, 5, 3, 1, 79.99, "2024-06-06", "pending"),
    ];
    for (id, uid, pid, qty, total, date, status) in orders {
        conn.execute(
            "INSERT OR IGNORE INTO orders (id, user_id, product_id, quantity, total, order_date, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, uid, pid, qty, total, date, status],
        )?;
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    

    let conn = Connection::open_in_memory().expect("Failed to open DB");
    init_db(&conn).expect("Failed to init DB");

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        history: Arc::new(Mutex::new(Vec::new())),
        logs: Arc::new(Mutex::new(Vec::new())),
    };

    let router = Router::new()
          
        .route("/api/query", post(run_query))
        .route("/api/history", get(get_history))
        .route("/api/logs", get(get_logs))
         .fallback_service(ServeDir::new("static"))   
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://localhost:3000");

    axum::serve(listener, router).await.unwrap();
}

async fn get_schema(State(state): State<AppState>) -> Json<SchemaResponse> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").unwrap();
    let table_names: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap()
        .filter_map(|r| r.ok()).collect();

    let mut tables = Vec::new();
    for name in table_names {
        let mut col_stmt = conn.prepare(&format!("PRAGMA table_info({})", name)).unwrap();
        let columns: Vec<String> = col_stmt.query_map([], |row| row.get::<_, String>(1)).unwrap()
            .filter_map(|r| r.ok()).collect();
        tables.push(TableInfo { name, columns });
    }

    Json(SchemaResponse {
        databases: vec![DatabaseInfo {
            name: "main".to_string(),
            tables,
        }],
    })
}

async fn run_query(State(state): State<AppState>, Json(req): Json<QueryRequest>) -> Json<QueryResult> {
    let start = Instant::now();
    let sql = req.sql.trim().to_string();

    let mut result = QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        duration_ms: 0,
        row_count: 0,
        error: None,
    };

    let conn = state.db.lock().unwrap();
    let lower = sql.to_lowercase();
    let is_select = lower.starts_with("select") || lower.starts_with("pragma") || lower.starts_with("with");

    if is_select {
        match conn.prepare(&sql) {
            Ok(mut stmt) => {
                let cols: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
                result.columns = cols.clone();
                let rows = stmt.query_map([], |row| {
                    let mut vals = Vec::new();
                    for i in 0..cols.len() {
                        let val: rusqlite::types::Value = row.get(i)?;
                        let json_val = match val {
                            rusqlite::types::Value::Null => Value::Null,
                            rusqlite::types::Value::Integer(i) => Value::Number(i.into()),
                            rusqlite::types::Value::Real(f) => Value::Number(
                                serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into())
                            ),
                            rusqlite::types::Value::Text(s) => Value::String(s),
                            rusqlite::types::Value::Blob(b) => Value::String(format!("<BLOB {} bytes>", b.len())),
                        };
                        vals.push(json_val);
                    }
                    Ok(vals)
                });
                match rows {
                    Ok(iter) => {
                        for row in iter {
                            if let Ok(r) = row { result.rows.push(r); }
                        }
                    }
                    Err(e) => result.error = Some(e.to_string()),
                }
            }
            Err(e) => result.error = Some(e.to_string()),
        }
    } else {
        match conn.execute(&sql, []) {
            Ok(affected) => {
                result.columns = vec!["result".to_string()];
                result.rows = vec![vec![Value::String(format!("Query OK, {} rows affected", affected))]];
            }
            Err(e) => result.error = Some(e.to_string()),
        }
    }

    result.duration_ms = start.elapsed().as_millis();
    result.row_count = result.rows.len();

    let record = QueryRecord {
        sql: sql.clone(),
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        duration_ms: result.duration_ms,
        row_count: result.row_count,
        error: result.error.clone(),
    };
    state.history.lock().unwrap().push(record);

    let log_msg = if result.error.is_some() {
        format!("Query failed: {}", sql)
    } else {
        format!("Query executed: {} ({} rows, {}ms)", sql, result.row_count, result.duration_ms)
    };
    state.logs.lock().unwrap().push(LogEntry {
        time: chrono::Local::now().format("%H:%M:%S").to_string(),
        level: if result.error.is_some() { "ERROR".to_string() } else { "INFO".to_string() },
        message: log_msg,
    });

    Json(result)
}

async fn get_history(State(state): State<AppState>) -> Json<HistoryResponse> {
    let history = state.history.lock().unwrap().clone();
    Json(HistoryResponse { queries: history })
}

async fn get_logs(State(state): State<AppState>) -> Json<LogsResponse> {
    let logs = state.logs.lock().unwrap().clone();
    Json(LogsResponse { logs })
}