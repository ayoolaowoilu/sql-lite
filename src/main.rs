use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use mysql::{prelude::Queryable, Conn, Opts, OptsBuilder, SslOpts, Value as MysqlValue};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const PORT: u16 = 6141;
const MYSQL_PORT: u16 = 9995;

mod mysql_server;
mod password_store;

#[derive(Clone)]
struct AppState {
    data_dir: PathBuf,
    connections: Arc<Mutex<HashMap<String, Arc<Mutex<Connection>>>>>,
    active_db: Arc<Mutex<Option<String>>>,
    mysql_password: Arc<Mutex<Option<String>>>,
    active_connection: Arc<Mutex<Option<ActiveConnection>>>,
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

#[derive(Deserialize)]
struct MysqlPasswordRequest {
    password: String,
    confirmation: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct ConnectionProfile {
    id: String,
    name: String,
    kind: String,
    host: String,
    port: u16,
    user: String,
}

#[derive(Deserialize)]
struct ConnectRequest {
    profile: ConnectionProfile,
    password: String,
}

#[derive(Clone)]
enum ActiveConnection {
    Local,
    External {
        profile: ConnectionProfile,
        password: String,
    },
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
        self.connections
            .lock()
            .unwrap()
            .insert(name.to_string(), arc);
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
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[tokio::main]
async fn main() {
    let data_dir = PathBuf::from("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let mysql_password = match password_store::load_password() {
        Ok(password) => password,
        Err(error) => {
            eprintln!("Could not access the secure password store: {}", error);
            std::process::exit(1);
        }
    };

    let state = AppState {
        data_dir,
        connections: Arc::new(Mutex::new(HashMap::new())),
        active_db: Arc::new(Mutex::new(None)),
        mysql_password: Arc::new(Mutex::new(mysql_password)),
        active_connection: Arc::new(Mutex::new(None)),
        history: Arc::new(Mutex::new(Vec::new())),
        logs: Arc::new(Mutex::new(Vec::new())),
    };

    let mysql_state = state.clone();
    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{}", MYSQL_PORT);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                println!("MySQL protocol listener on {}", addr);
                if let Err(error) = mysql_server::serve(listener, mysql_state).await {
                    eprintln!("MySQL listener stopped: {}", error);
                }
            }
            Err(error) => eprintln!("Could not start MySQL listener on {}: {}", addr, error),
        }
    });

    let app = Router::new()
        .route("/api/mysql/status", get(get_mysql_status))
        .route("/api/mysql/password", post(set_mysql_password))
        .route("/api/connections/connect", post(connect_profile))
        .route("/api/connections/disconnect", post(disconnect_profile))
        .route("/api/databases", get(list_databases))
        .route("/api/databases", post(create_database_endpoint))
        .route("/api/databases/:name", delete(drop_database_endpoint))
        .route("/api/schema", get(get_schema))
        .route("/api/query", post(run_query))
        .route("/api/history", get(get_history))
        .route("/api/logs", get(get_logs))
        .fallback_service(tower_http::services::ServeDir::new("static"))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", PORT);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    let url = format!("http://127.0.0.1:{}", PORT);
    println!("MySQLite running on {}", url);

    // Open browser after a short delay so the server is ready
    let url_clone = url.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
        open_browser(&url_clone);
    });

    axum::serve(listener, app).await.unwrap();
}

async fn get_mysql_status(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "configured": state.mysql_password.lock().unwrap().is_some()
    }))
}

async fn set_mysql_password(
    State(state): State<AppState>,
    Json(req): Json<MysqlPasswordRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if let Err(error) = validate_mysql_password(&req.password, &req.confirmation) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": error }))));
    }

    let mut password = state.mysql_password.lock().unwrap();
    if password.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "The MySQL password has already been set." })),
        ));
    }
    password_store::save_password(&req.password).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Could not save password securely: {}", error) })),
        )
    })?;
    *password = Some(req.password);
    Ok(Json(json!({
        "ok": true,
        "message": "MySQL root password saved securely on this computer."
    })))
}

fn validate_mysql_password(password: &str, confirmation: &str) -> Result<(), String> {
    if password.chars().count() < 8 {
        return Err("Use a password with at least 8 characters.".to_string());
    }
    if password.chars().count() > 128 {
        return Err("Use a password with no more than 128 characters.".to_string());
    }
    if password != confirmation {
        return Err("The passwords do not match.".to_string());
    }
    Ok(())
}

fn mysql_options(profile: &ConnectionProfile, password: &str, database: Option<&str>) -> Opts {
    let builder = OptsBuilder::default()
        .ip_or_hostname(Some(profile.host.clone()))
        .tcp_port(profile.port)
        .user(Some(profile.user.clone()))
        .pass(Some(password.to_string()))
        .db_name(database.map(str::to_string))
        .ssl_opts(Some(SslOpts::default()));
    Opts::from(builder)
}

async fn connect_profile(
    State(state): State<AppState>,
    Json(req): Json<ConnectRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let profile = req.profile;
    if profile.kind == "local" {
        let expected = state.mysql_password.lock().unwrap().clone();
        if expected.as_deref() != Some(req.password.as_str()) {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Incorrect MySQL password." })),
            ));
        }
        *state.active_connection.lock().unwrap() = Some(ActiveConnection::Local);
    } else if profile.kind == "mysql" {
        if profile.host.trim().is_empty() || profile.user.trim().is_empty() || profile.port == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Enter a valid host, port, and username." })),
            ));
        }
        let options = mysql_options(&profile, &req.password, None);
        tokio::task::spawn_blocking(move || Conn::new(options))
            .await
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Connection check failed: {}", error) })),
                )
            })?
            .map_err(|error| {
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": format!("Could not connect: {}", error) })),
                )
            })?;
        *state.active_connection.lock().unwrap() = Some(ActiveConnection::External {
            profile,
            password: req.password,
        });
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Unsupported connection type." })),
        ));
    }

    *state.active_db.lock().unwrap() = None;
    Ok(Json(json!({ "ok": true })))
}

async fn disconnect_profile(State(state): State<AppState>) -> Json<Value> {
    *state.active_connection.lock().unwrap() = None;
    *state.active_db.lock().unwrap() = None;
    Json(json!({ "ok": true }))
}

async fn list_databases(
    State(state): State<AppState>,
) -> Result<Json<DatabasesResponse>, (StatusCode, Json<Value>)> {
    let connection = state.active_connection.lock().unwrap().clone();
    let dbs = match connection {
        Some(ActiveConnection::Local) => state.list_databases(),
        Some(ActiveConnection::External { profile, password }) => {
            tokio::task::spawn_blocking(move || {
                let mut conn = Conn::new(mysql_options(&profile, &password, None))
                    .map_err(|error| error.to_string())?;
                conn.query_map("SHOW DATABASES", |name: String| name)
                    .map_err(|error| error.to_string())
            })
            .await
            .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error))?
        }
        None => {
            return Err(api_error(
                StatusCode::UNAUTHORIZED,
                "Connect to a server first.",
            ))
        }
    };
    Ok(Json(DatabasesResponse {
        databases: dbs.into_iter().map(|n| DbInfo { name: n }).collect(),
        active: state.active_db.lock().unwrap().clone(),
    }))
}

fn api_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message.into() })))
}

fn mysql_value_to_json(value: MysqlValue) -> Value {
    match value {
        MysqlValue::NULL => Value::Null,
        MysqlValue::Bytes(bytes) => Value::String(String::from_utf8_lossy(&bytes).into_owned()),
        MysqlValue::Int(value) => Value::Number(value.into()),
        MysqlValue::UInt(value) => Value::Number(value.into()),
        MysqlValue::Float(value) => serde_json::Number::from_f64(value as f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        MysqlValue::Double(value) => serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        MysqlValue::Date(year, month, day, hour, minute, second, micros) => Value::String(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
            year, month, day, hour, minute, second, micros
        )),
        MysqlValue::Time(negative, days, hours, minutes, seconds, micros) => {
            Value::String(format!(
                "{}{} {:02}:{:02}:{:02}.{:06}",
                if negative { "-" } else { "" },
                days,
                hours,
                minutes,
                seconds,
                micros
            ))
        }
    }
}

fn run_external_query(
    profile: &ConnectionProfile,
    password: &str,
    database: Option<&str>,
    sql: &str,
) -> Result<QueryResult, String> {
    let mut connection =
        Conn::new(mysql_options(profile, password, database)).map_err(|error| error.to_string())?;
    let mut query = connection
        .query_iter(sql)
        .map_err(|error| error.to_string())?;
    let columns = query
        .columns()
        .as_ref()
        .iter()
        .map(|column| column.name_str().into_owned())
        .collect::<Vec<_>>();
    let has_columns = !columns.is_empty();
    let mut output_rows = Vec::new();
    if let Some(mut rows) = query.iter() {
        for row in &mut rows {
            let values = row.map_err(|error| error.to_string())?.unwrap();
            output_rows.push(values.into_iter().map(mysql_value_to_json).collect());
        }
    }
    let affected_rows = if has_columns {
        output_rows.len()
    } else {
        query.affected_rows() as usize
    };
    Ok(QueryResult {
        columns,
        rows: output_rows,
        duration_ms: 0,
        row_count: affected_rows,
        error: None,
        message: if !has_columns {
            Some(format!("Query OK, {} rows affected", affected_rows))
        } else {
            None
        },
    })
}

async fn create_database_endpoint(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> Json<Value> {
    let name = req.sql.trim().to_string();
    let active_connection = state.active_connection.lock().unwrap().clone();
    if active_connection.is_none() {
        return Json(json!({ "ok": false, "error": "Connect to a server first." }));
    }
    if let Some(ActiveConnection::External { profile, password }) = active_connection {
        if !valid_database_name(&name) {
            return Json(json!({ "ok": false, "error": "Invalid database name." }));
        }
        let database_name = name.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = Conn::new(mysql_options(&profile, &password, None))
                .map_err(|error| error.to_string())?;
            conn.query_drop(format!("CREATE DATABASE `{}`", database_name))
                .map_err(|error| error.to_string())
        })
        .await;
        return match result {
            Ok(Ok(())) => {
                Json(json!({ "ok": true, "message": format!("Database '{}' created", name) }))
            }
            Ok(Err(error)) => Json(json!({ "ok": false, "error": error })),
            Err(error) => Json(json!({ "ok": false, "error": error.to_string() })),
        };
    }
    match state.create_database(&name) {
        Ok(_) => Json(json!({ "ok": true, "message": format!("Database '{}' created", name) })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

use axum::extract::Path;

async fn drop_database_endpoint(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<Value> {
    let active_connection = state.active_connection.lock().unwrap().clone();
    if active_connection.is_none() {
        return Json(json!({ "ok": false, "error": "Connect to a server first." }));
    }
    if let Some(ActiveConnection::External { profile, password }) = active_connection {
        if !valid_database_name(&name) {
            return Json(json!({ "ok": false, "error": "Invalid database name." }));
        }
        let drop_name = name.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = Conn::new(mysql_options(&profile, &password, None))
                .map_err(|error| error.to_string())?;
            conn.query_drop(format!("DROP DATABASE `{}`", drop_name))
                .map_err(|error| error.to_string())
        })
        .await;
        return match result {
            Ok(Ok(())) => {
                if state.active_db.lock().unwrap().as_deref() == Some(name.as_str()) {
                    *state.active_db.lock().unwrap() = None;
                }
                Json(json!({ "ok": true, "message": format!("Database '{}' dropped", name) }))
            }
            Ok(Err(error)) => Json(json!({ "ok": false, "error": error })),
            Err(error) => Json(json!({ "ok": false, "error": error.to_string() })),
        };
    }
    match state.drop_database(&name) {
        Ok(_) => Json(json!({ "ok": true, "message": format!("Database '{}' dropped", name) })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

async fn get_schema(
    State(state): State<AppState>,
) -> Result<Json<SchemaResponse>, (StatusCode, Json<Value>)> {
    if state.active_connection.lock().unwrap().is_none() {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Connect to a server first.",
        ));
    }
    let active = state.active_db.lock().unwrap().clone();
    let active_connection = state.active_connection.lock().unwrap().clone();
    if let Some(ActiveConnection::External { profile, password }) = active_connection {
        let Some(database) = active.clone() else {
            return Ok(Json(SchemaResponse {
                tables: Vec::new(),
                active_db: None,
            }));
        };
        let tables = tokio::task::spawn_blocking(move || {
            let mut conn = Conn::new(mysql_options(&profile, &password, None))
                .map_err(|error| error.to_string())?;
            let rows: Vec<(String, String, String)> = conn
                .exec(
                    "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE FROM information_schema.columns WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
                    (database,),
                )
                .map_err(|error| error.to_string())?;
            let mut tables = Vec::<TableInfo>::new();
            for (table_name, column_name, dtype) in rows {
                if tables.last().map(|table| table.name.as_str()) != Some(table_name.as_str()) {
                    tables.push(TableInfo {
                        name: table_name,
                        columns: Vec::new(),
                    });
                }
                if let Some(table) = tables.last_mut() {
                    table.columns.push(ColInfo {
                        name: column_name,
                        dtype,
                    });
                }
            }
            Ok::<_, String>(tables)
        })
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error))?;
        return Ok(Json(SchemaResponse {
            tables,
            active_db: active,
        }));
    }
    let mut tables = Vec::new();

    if let Some(ref db_name) = active {
        if let Ok(conn_arc) = state.get_connection(db_name) {
            let conn = conn_arc.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let names: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();

            for name in names {
                let mut col_stmt = conn
                    .prepare(&format!("PRAGMA table_info({})", name))
                    .unwrap();
                let cols = col_stmt
                    .query_map([], |row| {
                        Ok(ColInfo {
                            name: row.get::<_, String>(1)?,
                            dtype: row.get::<_, String>(2)?,
                        })
                    })
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                tables.push(TableInfo {
                    name,
                    columns: cols,
                });
            }
        }
    }

    Ok(Json(SchemaResponse {
        tables,
        active_db: active,
    }))
}

async fn run_query(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> Json<QueryResult> {
    let active_connection = state.active_connection.lock().unwrap().clone();
    match active_connection {
        Some(ActiveConnection::External { profile, password }) => {
            return run_external_query_request(state, profile, password, req.sql).await;
        }
        Some(ActiveConnection::Local) => {}
        None => {
            return Json(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                duration_ms: 0,
                row_count: 0,
                error: Some("Connect to a server first.".to_string()),
                message: None,
            });
        }
    }

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
        log_query(
            &state,
            &sql_raw,
            result.duration_ms,
            0,
            result.error.clone(),
        );
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
        log_query(
            &state,
            &sql_raw,
            result.duration_ms,
            0,
            result.error.clone(),
        );
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
        log_query(
            &state,
            &sql_raw,
            result.duration_ms,
            0,
            result.error.clone(),
        );
        return Json(result);
    }

    let db_name = match state.active_db.lock().unwrap().clone() {
        Some(n) => n,
        None => {
            result.error = Some("No database selected. Use: USE database_name;".to_string());
            result.duration_ms = start.elapsed().as_millis();
            log_query(
                &state,
                &sql_raw,
                result.duration_ms,
                0,
                result.error.clone(),
            );
            return Json(result);
        }
    };

    let conn_arc = match state.get_connection(&db_name) {
        Ok(c) => c,
        Err(e) => {
            result.error = Some(e);
            result.duration_ms = start.elapsed().as_millis();
            log_query(
                &state,
                &sql_raw,
                result.duration_ms,
                0,
                result.error.clone(),
            );
            return Json(result);
        }
    };

    let conn = conn_arc.lock().unwrap();
    let lower = sql_raw.to_lowercase();
    let is_select =
        lower.starts_with("select") || lower.starts_with("pragma") || lower.starts_with("with");

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
                                serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into()),
                            ),
                            rusqlite::types::Value::Text(s) => Value::String(s),
                            rusqlite::types::Value::Blob(b) => {
                                Value::String(format!("<BLOB {} bytes>", b.len()))
                            }
                        };
                        vals.push(json_val);
                    }
                    Ok(vals)
                });
                match rows {
                    Ok(iter) => {
                        for row in iter {
                            if let Ok(r) = row {
                                result.rows.push(r);
                            }
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

    log_query(
        &state,
        &sql_raw,
        result.duration_ms,
        result.row_count,
        result.error.clone(),
    );
    Json(result)
}

async fn run_external_query_request(
    state: AppState,
    profile: ConnectionProfile,
    password: String,
    sql: String,
) -> Json<QueryResult> {
    let start = Instant::now();
    let sql = sql.trim().to_string();
    let upper = sql.to_ascii_uppercase();
    let mut result = QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        duration_ms: 0,
        row_count: 0,
        error: None,
        message: None,
    };

    if upper.starts_with("CREATE DATABASE") {
        let name = parse_db_name(&sql);
        if !valid_database_name(&name) {
            result.error = Some("CREATE DATABASE requires a valid name".to_string());
        } else {
            let profile_for_query = profile.clone();
            let password_for_query = password.clone();
            let sql_for_query = sql.clone();
            let query_result = tokio::task::spawn_blocking(move || {
                let mut connection =
                    Conn::new(mysql_options(&profile_for_query, &password_for_query, None))
                        .map_err(|error| error.to_string())?;
                connection
                    .query_drop(sql_for_query)
                    .map_err(|error| error.to_string())
            })
            .await;
            match query_result {
                Ok(Ok(())) => {
                    *state.active_db.lock().unwrap() = Some(name.clone());
                    result.message = Some(format!("Database '{}' created", name));
                }
                Ok(Err(error)) => result.error = Some(error),
                Err(error) => result.error = Some(error.to_string()),
            }
        }
    } else if upper.starts_with("DROP DATABASE") {
        let name = parse_db_name(&sql);
        if !valid_database_name(&name) {
            result.error = Some("DROP DATABASE requires a valid name".to_string());
        } else {
            let profile_for_query = profile.clone();
            let password_for_query = password.clone();
            let sql_for_query = sql.clone();
            let query_result = tokio::task::spawn_blocking(move || {
                let mut connection =
                    Conn::new(mysql_options(&profile_for_query, &password_for_query, None))
                        .map_err(|error| error.to_string())?;
                connection
                    .query_drop(sql_for_query)
                    .map_err(|error| error.to_string())
            })
            .await;
            match query_result {
                Ok(Ok(())) => {
                    if state.active_db.lock().unwrap().as_deref() == Some(name.as_str()) {
                        *state.active_db.lock().unwrap() = None;
                    }
                    result.message = Some(format!("Database '{}' dropped", name));
                }
                Ok(Err(error)) => result.error = Some(error),
                Err(error) => result.error = Some(error.to_string()),
            }
        }
    } else if upper.starts_with("USE ") {
        let name = sql[4..]
            .trim()
            .trim_end_matches(';')
            .trim_matches('`')
            .to_string();
        if !valid_database_name(&name) {
            result.error = Some("USE requires a valid database name".to_string());
        } else {
            let profile_for_query = profile.clone();
            let password_for_query = password.clone();
            let database = name.clone();
            let query_result = tokio::task::spawn_blocking(move || {
                let mut connection =
                    Conn::new(mysql_options(&profile_for_query, &password_for_query, None))
                        .map_err(|error| error.to_string())?;
                connection
                    .query_drop(format!("USE `{}`", database))
                    .map_err(|error| error.to_string())
            })
            .await;
            match query_result {
                Ok(Ok(())) => {
                    *state.active_db.lock().unwrap() = Some(name.clone());
                    result.message = Some(format!("Switched to database '{}'", name));
                }
                Ok(Err(error)) => result.error = Some(error),
                Err(error) => result.error = Some(error.to_string()),
            }
        }
    } else {
        let database = state.active_db.lock().unwrap().clone();
        let query_sql = sql.clone();
        let query_result = tokio::task::spawn_blocking(move || {
            run_external_query(&profile, &password, database.as_deref(), &query_sql)
        })
        .await;
        match query_result {
            Ok(Ok(query_result)) => result = query_result,
            Ok(Err(error)) => result.error = Some(error),
            Err(error) => result.error = Some(error.to_string()),
        }
    }

    result.duration_ms = start.elapsed().as_millis();
    log_query(
        &state,
        &sql,
        result.duration_ms,
        result.row_count,
        result.error.clone(),
    );
    Json(result)
}

fn valid_database_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn parse_db_name(sql: &str) -> String {
    let parts: Vec<&str> = sql.split_whitespace().collect();
    if parts.len() >= 3 {
        let mut idx = 2;
        if parts
            .get(idx)
            .is_some_and(|part| part.eq_ignore_ascii_case("IF"))
        {
            idx += if parts
                .get(idx + 1)
                .is_some_and(|part| part.eq_ignore_ascii_case("NOT"))
            {
                3
            } else {
                2
            };
        }
        if idx < parts.len() {
            return parts[idx]
                .trim_end_matches(';')
                .trim_matches('`')
                .trim_matches('"')
                .to_string();
        }
    }
    String::new()
}

fn log_query(
    state: &AppState,
    sql: &str,
    duration_ms: u128,
    row_count: usize,
    error: Option<String>,
) {
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
        format!(
            "Query executed: {} ({} rows, {}ms)",
            sql, row_count, duration_ms
        )
    };
    state.logs.lock().unwrap().push(LogEntry {
        time: chrono::Local::now().format("%H:%M:%S").to_string(),
        level: if error.is_some() {
            "ERROR".to_string()
        } else {
            "INFO".to_string()
        },
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

#[cfg(test)]
mod tests {
    use super::validate_mysql_password;

    #[test]
    fn password_setup_requires_matching_passwords_of_valid_length() {
        assert!(validate_mysql_password("correct horse", "correct horse").is_ok());
        assert!(validate_mysql_password("short", "short").is_err());
        assert!(validate_mysql_password("long enough", "different").is_err());
        assert!(validate_mysql_password(&"a".repeat(129), &"a".repeat(129)).is_err());
    }
}
