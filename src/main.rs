use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use axum::{
    extract::State,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const PORT: u16 = 6141;

#[derive(Clone)]
struct AppState {
    data_dir: PathBuf,
    connections: Arc<Mutex<HashMap<String, Arc<Mutex<Connection>>>>>,
    active_db: Arc<Mutex<Option<String>>>,
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
    message: Option<String>,
}

#[derive(Serialize)]
struct DatabasesResponse {
    databases: Vec<DbInfo>,
    active: Option<String>,
}

#[derive(Serialize)]
struct DbInfo {
    name: String,
}

#[derive(Serialize)]
struct SchemaResponse {
    tables: Vec<TableInfo>,
    active_db: Option<String>,
}

#[derive(Serialize)]
struct TableInfo {
    name: String,
    columns: Vec<ColInfo>,
}

#[derive(Serialize)]
struct ColInfo {
    name: String,
    dtype: String,
}

#[derive(Serialize)]
struct HistoryResponse {
    queries: Vec<QueryRecord>,
}

#[derive(Serialize)]
struct LogsResponse {
    logs: Vec<LogEntry>,
}

impl AppState {
    fn db_path(&self, name: &str) -> PathBuf {
        self.data_dir.join(format!("{}.db", name))
    }

    fn list_databases(&self) -> Vec<String> {
        let mut dbs = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "db" {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            dbs.push(stem.to_string());
                        }
                    }
                }
            }
        }
        dbs.sort();
        dbs
    }

    fn get_connection(&self, name: &str) -> Result<Arc<Mutex<Connection>>, String> {
        let mut conns = self.connections.lock().unwrap();
        if let Some(conn) = conns.get(name) {
            return Ok(conn.clone());
        }
        let path = self.db_path(name);
        if !path.exists() {
            return Err(format!("Database '{}' does not exist", name));
        }
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        let arc = Arc::new(Mutex::new(conn));
        conns.insert(name.to_string(), arc.clone());
        Ok(arc)
    }

    fn create_database(&self, name: &str) -> Result<(), String> {
        let path = self.db_path(name);
        if path.exists() {
            return Err(format!("Database '{}' already exists", name));
        }
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        let arc = Arc::new(Mutex::new(conn));
        self.connections.lock().unwrap().insert(name.to_string(), arc);
        Ok(())
    }

    fn drop_database(&self, name: &str) -> Result<(), String> {
        let mut conns = self.connections.lock().unwrap();
        conns.remove(name);
        let path = self.db_path(name);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        let mut active = self.active_db.lock().unwrap();
        if active.as_deref() == Some(name) {
            *active = None;
        }
        Ok(())
    }

    fn set_active(&self, name: &str) -> Result<(), String> {
        let path = self.db_path(name);
        if !path.exists() {
            return Err(format!("Database '{}' does not exist", name));
        }
        drop(self.get_connection(name)?);
        *self.active_db.lock().unwrap() = Some(name.to_string());
        Ok(())
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(url)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(url)
            .spawn();
    }
}

#[tokio::main]
async fn main() {
    let data_dir = PathBuf::from("data");
    std::fs::create_dir_all(&data_dir).unwrap();

    let state = AppState {
        data_dir,
        connections: Arc::new(Mutex::new(HashMap::new())),
        active_db: Arc::new(Mutex::new(None)),
        history: Arc::new(Mutex::new(Vec::new())),
        logs: Arc::new(Mutex::new(Vec::new())),
    };

    let app = Router::new()
        .route("/api/databases", get(list_databases))
        .route("/api/databases", post(create_database_endpoint))
        .route("/api/databases/:name", delete(drop_database_endpoint))
        .route("/api/schema", get(get_schema))
        .route("/api/query", post(run_query))
        .route("/api/history", get(get_history))
        .route("/api/logs", get(get_logs))
        .fallback_service(tower_http::services::ServeDir::new("static"))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", PORT);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    let url = format!("http://localhost:{}", PORT);
    println!("MySQLite running on {}", url);

    // Open browser after a short delay so the server is ready
    let url_clone = url.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
        open_browser(&url_clone);
    });

    axum::serve(listener, app).await.unwrap();
}

async fn list_databases(State(state): State<AppState>) -> Json<DatabasesResponse> {
    let dbs = state.list_databases();
    Json(DatabasesResponse {
        databases: dbs.into_iter().map(|n| DbInfo { name: n }).collect(),
        active: state.active_db.lock().unwrap().clone(),
    })
}

async fn create_database_endpoint(State(state): State<AppState>, Json(req): Json<QueryRequest>) -> Json<Value> {
    let name = req.sql.trim().to_string();
    match state.create_database(&name) {
        Ok(_) => Json(json!({ "ok": true, "message": format!("Database '{}' created", name) })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

use axum::extract::Path;

async fn drop_database_endpoint(State(state): State<AppState>, Path(name): Path<String>) -> Json<Value> {
    match state.drop_database(&name) {
        Ok(_) => Json(json!({ "ok": true, "message": format!("Database '{}' dropped", name) })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

async fn get_schema(State(state): State<AppState>) -> Json<SchemaResponse> {
    let active = state.active_db.lock().unwrap().clone();
    let mut tables = Vec::new();

    if let Some(ref db_name) = active {
        if let Ok(conn_arc) = state.get_connection(db_name) {
            let conn = conn_arc.lock().unwrap();
            let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").unwrap();
            let names: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap()
                .filter_map(|r| r.ok()).collect();

            for name in names {
                let mut col_stmt = conn.prepare(&format!("PRAGMA table_info({})", name)).unwrap();
                let cols = col_stmt.query_map([], |row| {
                    Ok(ColInfo {
                        name: row.get::<_, String>(1)?,
                        dtype: row.get::<_, String>(2)?,
                    })
                }).unwrap().filter_map(|r| r.ok()).collect();
                tables.push(TableInfo { name, columns: cols });
            }
        }
    }

    Json(SchemaResponse { tables, active_db: active })
}

async fn run_query(State(state): State<AppState>, Json(req): Json<QueryRequest>) -> Json<QueryResult> {
    let start = Instant::now();
    let sql_raw = req.sql.trim().to_string();
    let sql_upper = sql_raw.to_uppercase();

    let mut result = QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        duration_ms: 0,
        row_count: 0,
        error: None,
        message: None,
    };

    if sql_upper.starts_with("CREATE DATABASE") {
        let name = parse_db_name(&sql_raw);
        if name.is_empty() {
            result.error = Some("CREATE DATABASE requires a name".to_string());
        } else {
            match state.create_database(&name) {
                Ok(_) => {
                    result.message = Some(format!("Database '{}' created", name));
                    *state.active_db.lock().unwrap() = Some(name);
                }
                Err(e) => result.error = Some(e),
            }
        }
        result.duration_ms = start.elapsed().as_millis();
        log_query(&state, &sql_raw, result.duration_ms, 0, result.error.clone());
        return Json(result);
    }

    if sql_upper.starts_with("USE ") {
        let name = sql_raw[4..].trim().trim_end_matches(';').to_string();
        match state.set_active(&name) {
            Ok(_) => {
                result.message = Some(format!("Switched to database '{}'", name));
            }
            Err(e) => result.error = Some(e),
        }
        result.duration_ms = start.elapsed().as_millis();
        log_query(&state, &sql_raw, result.duration_ms, 0, result.error.clone());
        return Json(result);
    }

    if sql_upper.starts_with("DROP DATABASE") {
        let name = parse_db_name(&sql_raw);
        if name.is_empty() {
            result.error = Some("DROP DATABASE requires a name".to_string());
        } else {
            match state.drop_database(&name) {
                Ok(_) => result.message = Some(format!("Database '{}' dropped", name)),
                Err(e) => result.error = Some(e),
            }
        }
        result.duration_ms = start.elapsed().as_millis();
        log_query(&state, &sql_raw, result.duration_ms, 0, result.error.clone());
        return Json(result);
    }

    let db_name = match state.active_db.lock().unwrap().clone() {
        Some(n) => n,
        None => {
            result.error = Some("No database selected. Use: USE database_name;".to_string());
            result.duration_ms = start.elapsed().as_millis();
            log_query(&state, &sql_raw, result.duration_ms, 0, result.error.clone());
            return Json(result);
        }
    };

    let conn_arc = match state.get_connection(&db_name) {
        Ok(c) => c,
        Err(e) => {
            result.error = Some(e);
            result.duration_ms = start.elapsed().as_millis();
            log_query(&state, &sql_raw, result.duration_ms, 0, result.error.clone());
            return Json(result);
        }
    };

    let conn = conn_arc.lock().unwrap();
    let lower = sql_raw.to_lowercase();
    let is_select = lower.starts_with("select")
        || lower.starts_with("pragma")
        || lower.starts_with("with");

    if is_select {
        match conn.prepare(&sql_raw) {
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
        match conn.execute(&sql_raw, []) {
            Ok(affected) => {
                result.message = Some(format!("Query OK, {} rows affected", affected));
            }
            Err(e) => result.error = Some(e.to_string()),
        }
    }

    result.duration_ms = start.elapsed().as_millis();
    result.row_count = result.rows.len();

    log_query(&state, &sql_raw, result.duration_ms, result.row_count, result.error.clone());
    Json(result)
}

fn parse_db_name(sql: &str) -> String {
    let parts: Vec<&str> = sql.split_whitespace().collect();
    if parts.len() >= 3 {
        let mut idx = 2;
        if parts.len() > 3 && parts[2].to_uppercase() == "IF" {
            idx = 5;
        }
        if idx < parts.len() {
            return parts[idx].trim_end_matches(';').to_string();
        }
    }
    String::new()
}

fn log_query(state: &AppState, sql: &str, duration_ms: u128, row_count: usize, error: Option<String>) {
    let record = QueryRecord {
        sql: sql.to_string(),
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        duration_ms,
        row_count,
        error: error.clone(),
    };
    state.history.lock().unwrap().push(record);

    let log_msg = if let Some(ref e) = error {
        format!("Query failed: {} | {}", sql, e)
    } else {
        format!("Query executed: {} ({} rows, {}ms)", sql, row_count, duration_ms)
    };
    state.logs.lock().unwrap().push(LogEntry {
        time: chrono::Local::now().format("%H:%M:%S").to_string(),
        level: if error.is_some() { "ERROR".to_string() } else { "INFO".to_string() },
        message: log_msg,
    });
}

async fn get_history(State(state): State<AppState>) -> Json<HistoryResponse> {
    let history = state.history.lock().unwrap().clone();
    Json(HistoryResponse { queries: history })
}

async fn get_logs(State(state): State<AppState>) -> Json<LogsResponse> {
    let logs = state.logs.lock().unwrap().clone();
    Json(LogsResponse { logs })
}