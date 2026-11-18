use std::{
    io,
    sync::atomic::{AtomicU32, Ordering},
};

use rand::RngCore;
use rusqlite::{types::ValueRef, Connection};
use sha1::{Digest, Sha1};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::{log_query, AppState};

const CAPABILITIES: u32 = 0x0008_a20d;
const CLIENT_CONNECT_WITH_DB: u32 = 0x0000_0008;
const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
const CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA: u32 = 0x0020_0000;
const STATUS_AUTOCOMMIT: u16 = 0x0002;
const MAX_PACKET_SIZE: usize = 0x00ff_ffff;

static NEXT_CONNECTION_ID: AtomicU32 = AtomicU32::new(1);

struct Column {
    name: String,
    mysql_type: u8,
}

struct QueryResult {
    columns: Vec<Column>,
    rows: Vec<Vec<Option<Vec<u8>>>>,
    affected_rows: u64,
    last_insert_id: u64,
}

impl QueryResult {
    fn ok(affected_rows: u64, last_insert_id: u64) -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            affected_rows,
            last_insert_id,
        }
    }

    fn single(name: &str, value: Option<String>) -> Self {
        Self {
            columns: vec![Column {
                name: name.to_string(),
                mysql_type: 0xfd,
            }],
            rows: vec![vec![value.map(String::into_bytes)]],
            affected_rows: 0,
            last_insert_id: 0,
        }
    }
}

pub async fn serve(listener: TcpListener, state: AppState, password: String) -> io::Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        let state = state.clone();
        let password = password.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_client(stream, state, password).await {
                eprintln!("MySQL client {} disconnected: {}", peer, error);
            }
        });
    }
}

async fn handle_client(mut stream: TcpStream, state: AppState, password: String) -> io::Result<()> {
    let mut seed = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut seed);
    write_packet(&mut stream, &handshake(&seed), 0).await?;

    let (_, login) = read_packet(&mut stream).await?;
    let (capabilities, username, auth_response, database) = match parse_login(&login) {
        Some(login) => login,
        None => {
            write_error(
                &mut stream,
                1043,
                "08S01",
                "Malformed handshake response",
                2,
            )
            .await?;
            return Ok(());
        }
    };

    if capabilities & CLIENT_PROTOCOL_41 == 0
        || username != "root"
        || !verify_password(&password, &seed, &auth_response)
    {
        write_error(
            &mut stream,
            1045,
            "28000",
            "Access denied for user 'root'",
            2,
        )
        .await?;
        return Ok(());
    }

    let mut selected_database = None;
    if let Some(database) = database {
        if !valid_database_name(&database) {
            write_error(&mut stream, 1049, "42000", "Invalid database name", 2).await?;
            return Ok(());
        }
        if state.get_connection(&database).is_err() {
            write_error(
                &mut stream,
                1049,
                "42000",
                &format!("Unknown database '{}'", database),
                2,
            )
            .await?;
            return Ok(());
        }
        selected_database = Some(database);
    }
    write_ok(&mut stream, 0, 0, 2).await?;

    loop {
        let (sequence, packet) = match read_packet(&mut stream).await {
            Ok(packet) => packet,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        };
        let Some((&command, payload)) = packet.split_first() else {
            write_error(&mut stream, 1043, "08S01", "Empty command packet", 1).await?;
            continue;
        };
        let response_sequence = sequence.wrapping_add(1);

        match command {
            0x01 => return Ok(()),
            0x02 => {
                let database = String::from_utf8_lossy(payload)
                    .trim_end_matches('\0')
                    .to_string();
                if valid_database_name(&database) && state.get_connection(&database).is_ok() {
                    selected_database = Some(database);
                    write_ok(&mut stream, 0, 0, response_sequence).await?;
                } else {
                    write_error(
                        &mut stream,
                        1049,
                        "42000",
                        &format!("Unknown database '{}'", database),
                        response_sequence,
                    )
                    .await?;
                }
            }
            0x03 => {
                let sql = String::from_utf8_lossy(payload).trim().to_string();
                match execute_query(&state, &mut selected_database, &sql) {
                    Ok(result) if result.columns.is_empty() => {
                        write_ok(
                            &mut stream,
                            result.affected_rows,
                            result.last_insert_id,
                            response_sequence,
                        )
                        .await?;
                    }
                    Ok(result) => write_result_set(&mut stream, result, response_sequence).await?,
                    Err((code, sql_state, message)) => {
                        write_error(&mut stream, code, sql_state, &message, response_sequence)
                            .await?;
                    }
                }
            }
            0x0e | 0x1b => write_ok(&mut stream, 0, 0, response_sequence).await?,
            _ => {
                write_error(
                    &mut stream,
                    1047,
                    "08S01",
                    "Unsupported MySQL command",
                    response_sequence,
                )
                .await?;
            }
        }
    }
}

fn handshake(seed: &[u8; 20]) -> Vec<u8> {
    let connection_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
    let mut packet = vec![10];
    packet.extend_from_slice(b"8.0.36-sq-lite\0");
    packet.extend_from_slice(&connection_id.to_le_bytes());
    packet.extend_from_slice(&seed[..8]);
    packet.push(0);
    packet.extend_from_slice(&(CAPABILITIES as u16).to_le_bytes());
    packet.push(33);
    packet.extend_from_slice(&STATUS_AUTOCOMMIT.to_le_bytes());
    packet.extend_from_slice(&((CAPABILITIES >> 16) as u16).to_le_bytes());
    packet.push(21);
    packet.extend_from_slice(&[0; 10]);
    packet.extend_from_slice(&seed[8..]);
    packet.push(0);
    packet.extend_from_slice(b"mysql_native_password\0");
    packet
}

fn parse_login(packet: &[u8]) -> Option<(u32, String, Vec<u8>, Option<String>)> {
    if packet.len() < 32 {
        return None;
    }
    let capabilities = u32::from_le_bytes(packet[0..4].try_into().ok()?);
    let mut offset = 32;
    let username_end = packet[offset..].iter().position(|byte| *byte == 0)? + offset;
    let username = String::from_utf8_lossy(&packet[offset..username_end]).to_string();
    offset = username_end + 1;

    let auth_response = if capabilities & CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA != 0 {
        let (length, consumed) = decode_length(&packet[offset..])?;
        offset += consumed;
        let end = offset.checked_add(length as usize)?;
        let response = packet.get(offset..end)?.to_vec();
        offset = end;
        response
    } else if capabilities & CLIENT_SECURE_CONNECTION != 0 {
        let length = *packet.get(offset)? as usize;
        offset += 1;
        let end = offset.checked_add(length)?;
        let response = packet.get(offset..end)?.to_vec();
        offset = end;
        response
    } else {
        let end = packet[offset..].iter().position(|byte| *byte == 0)? + offset;
        let response = packet[offset..end].to_vec();
        offset = end + 1;
        response
    };

    let database = if capabilities & CLIENT_CONNECT_WITH_DB != 0 {
        let end = packet[offset..].iter().position(|byte| *byte == 0)? + offset;
        Some(String::from_utf8_lossy(&packet[offset..end]).to_string())
    } else {
        None
    };
    Some((capabilities, username, auth_response, database))
}

fn verify_password(password: &str, seed: &[u8; 20], response: &[u8]) -> bool {
    let expected = if password.is_empty() {
        Vec::new()
    } else {
        let stage1 = Sha1::digest(password.as_bytes());
        let stage2 = Sha1::digest(stage1);
        let mut digest = Sha1::new();
        digest.update(seed);
        digest.update(stage2);
        let stage3 = digest.finalize();
        stage1
            .iter()
            .zip(stage3.iter())
            .map(|(left, right)| left ^ right)
            .collect()
    };

    if expected.len() != response.len() {
        return false;
    }
    expected
        .iter()
        .zip(response)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

fn execute_query(
    state: &AppState,
    selected_database: &mut Option<String>,
    sql: &str,
) -> Result<QueryResult, (u16, &'static str, String)> {
    let start = std::time::Instant::now();
    let upper = sql.trim().to_ascii_uppercase();
    let result = if upper.starts_with("CREATE DATABASE") {
        let database = database_name(sql, 2);
        if !valid_database_name(&database) {
            Err((1064, "42000", "CREATE DATABASE requires a name".to_string()))
        } else if upper.contains("IF NOT EXISTS") && state.list_databases().contains(&database) {
            Ok(QueryResult::ok(0, 0))
        } else {
            state
                .create_database(&database)
                .map(|_| QueryResult::ok(1, 0))
                .map_err(|error| (1007, "HY000", error))
        }
    } else if upper.starts_with("DROP DATABASE") {
        let database = database_name(sql, 2);
        if !valid_database_name(&database) {
            Err((1064, "42000", "DROP DATABASE requires a name".to_string()))
        } else if upper.contains("IF EXISTS") && !state.list_databases().contains(&database) {
            Ok(QueryResult::ok(0, 0))
        } else {
            state
                .drop_database(&database)
                .map(|_| {
                    if selected_database.as_deref() == Some(database.as_str()) {
                        *selected_database = None;
                    }
                    QueryResult::ok(1, 0)
                })
                .map_err(|error| (1008, "HY000", error))
        }
    } else if upper.starts_with("USE ") {
        let database = database_name(sql, 1);
        if valid_database_name(&database) && state.get_connection(&database).is_ok() {
            *selected_database = Some(database);
            Ok(QueryResult::ok(0, 0))
        } else {
            Err((1049, "42000", format!("Unknown database '{}'", database)))
        }
    } else if upper.starts_with("SET ") || upper.starts_with("SET\t") {
        Ok(QueryResult::ok(0, 0))
    } else if upper.starts_with("SHOW DATABASES") {
        let databases = state.list_databases();
        Ok(QueryResult {
            columns: vec![Column {
                name: "Database".to_string(),
                mysql_type: 0xfd,
            }],
            rows: databases
                .into_iter()
                .map(|database| vec![Some(database.into_bytes())])
                .collect(),
            affected_rows: 0,
            last_insert_id: 0,
        })
    } else if upper.starts_with("SHOW TABLES") {
        let database = selected_database.clone().ok_or((
            1046,
            "3D000",
            "No database selected. Connect with a database or run USE.".to_string(),
        ))?;
        let connection = state
            .get_connection(&database)
            .map_err(|error| (1049, "42000", error))?;
        let connection = ArcConnection::from(connection);
        let result = execute_sql(
            &connection,
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        Ok(QueryResult {
            columns: vec![Column {
                name: format!("Tables_in_{}", database),
                mysql_type: 0xfd,
            }],
            ..result
        })
    } else if upper.starts_with("SELECT VERSION()") || upper.starts_with("SELECT @@VERSION") {
        Ok(QueryResult::single(
            "VERSION()",
            Some("8.0.36-sq-lite".to_string()),
        ))
    } else if upper.starts_with("SELECT DATABASE()") || upper.starts_with("SELECT SCHEMA()") {
        Ok(QueryResult::single("DATABASE()", selected_database.clone()))
    } else {
        let database = selected_database.clone();
        let connection = match database {
            Some(database) => ArcConnection::from(
                state
                    .get_connection(&database)
                    .map_err(|error| (1049, "42000", error))?,
            ),
            None if upper.starts_with("SELECT ") => {
                ArcConnection::memory().map_err(|error| (1105, "HY000", error))?
            }
            None => {
                return Err((
                    1046,
                    "3D000",
                    "No database selected. Connect with a database or run USE.".to_string(),
                ));
            }
        };
        execute_sql(&connection, sql)
    };

    let duration = start.elapsed().as_millis();
    match &result {
        Ok(output) => log_query(
            state,
            sql,
            duration,
            output.rows.len().max(output.affected_rows as usize),
            None,
        ),
        Err((_, _, error)) => log_query(state, sql, duration, 0, Some(error.clone())),
    }
    result
}

use std::sync::Arc;

enum ArcConnection {
    Shared(Arc<std::sync::Mutex<Connection>>),
    Memory(Connection),
}

impl ArcConnection {
    fn memory() -> Result<Self, String> {
        Connection::open_in_memory()
            .map(Self::Memory)
            .map_err(|error| error.to_string())
    }

    fn with<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        match self {
            Self::Shared(connection) => {
                let connection = connection.lock().map_err(|error| error.to_string())?;
                operation(&connection)
            }
            Self::Memory(connection) => operation(connection),
        }
    }
}

impl From<Arc<std::sync::Mutex<Connection>>> for ArcConnection {
    fn from(connection: Arc<std::sync::Mutex<Connection>>) -> Self {
        Self::Shared(connection)
    }
}

fn execute_sql(
    connection: &ArcConnection,
    sql: &str,
) -> Result<QueryResult, (u16, &'static str, String)> {
    connection
        .with(|connection| {
            let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
            let column_count = statement.column_count();
            if column_count > 0 {
                let names = statement
                    .column_names()
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                let mut rows = statement.query([]).map_err(|error| error.to_string())?;
                let mut output_rows = Vec::new();
                let mut types = vec![0xfd; column_count];
                while let Some(row) = rows.next().map_err(|error| error.to_string())? {
                    let mut values = Vec::with_capacity(column_count);
                    for (index, mysql_type) in types.iter_mut().enumerate() {
                        let value = match row.get_ref(index).map_err(|error| error.to_string())? {
                            ValueRef::Null => None,
                            ValueRef::Integer(value) => {
                                *mysql_type = 0x08;
                                Some(value.to_string().into_bytes())
                            }
                            ValueRef::Real(value) => {
                                *mysql_type = 0x05;
                                Some(value.to_string().into_bytes())
                            }
                            ValueRef::Text(value) => {
                                *mysql_type = 0xfd;
                                Some(value.to_vec())
                            }
                            ValueRef::Blob(value) => {
                                *mysql_type = 0xfc;
                                Some(value.to_vec())
                            }
                        };
                        values.push(value);
                    }
                    output_rows.push(values);
                }
                Ok(QueryResult {
                    columns: names
                        .into_iter()
                        .zip(types)
                        .map(|(name, mysql_type)| Column { name, mysql_type })
                        .collect(),
                    rows: output_rows,
                    affected_rows: 0,
                    last_insert_id: 0,
                })
            } else {
                drop(statement);
                let affected_rows = connection
                    .execute(sql, [])
                    .map_err(|error| error.to_string())?;
                let normalized = sql.trim_start().to_ascii_uppercase();
                let last_insert_id =
                    if normalized.starts_with("INSERT") || normalized.starts_with("REPLACE") {
                        connection.last_insert_rowid() as u64
                    } else {
                        0
                    };
                Ok(QueryResult::ok(affected_rows as u64, last_insert_id))
            }
        })
        .map_err(|error| (1105, "HY000", error))
}

fn database_name(sql: &str, index: usize) -> String {
    let words = sql.split_whitespace().collect::<Vec<_>>();
    let mut name_index = index;
    if words
        .get(name_index)
        .is_some_and(|word| word.eq_ignore_ascii_case("IF"))
    {
        name_index += if words
            .get(name_index + 1)
            .is_some_and(|word| word.eq_ignore_ascii_case("NOT"))
        {
            3
        } else {
            2
        };
    }
    words
        .get(name_index)
        .copied()
        .unwrap_or_default()
        .trim_end_matches(';')
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn valid_database_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn decode_length(packet: &[u8]) -> Option<(u64, usize)> {
    let first = *packet.first()?;
    match first {
        0..=250 => Some((u64::from(first), 1)),
        0xfc if packet.len() >= 3 => {
            Some((u64::from(u16::from_le_bytes([packet[1], packet[2]])), 3))
        }
        0xfd if packet.len() >= 4 => Some((
            u64::from(packet[1]) | (u64::from(packet[2]) << 8) | (u64::from(packet[3]) << 16),
            4,
        )),
        0xfe if packet.len() >= 9 => Some((u64::from_le_bytes(packet[1..9].try_into().ok()?), 9)),
        _ => None,
    }
}

async fn write_result_set(
    stream: &mut TcpStream,
    result: QueryResult,
    mut sequence: u8,
) -> io::Result<()> {
    let mut header = Vec::new();
    encode_length(result.columns.len() as u64, &mut header);
    write_packet(stream, &header, sequence).await?;
    sequence = sequence.wrapping_add(1);

    for column in &result.columns {
        let mut definition = Vec::new();
        for value in ["def", "", "", "", column.name.as_str(), ""] {
            encode_string(value.as_bytes(), &mut definition);
        }
        definition.push(0x0c);
        definition.extend_from_slice(&33u16.to_le_bytes());
        definition.extend_from_slice(&1024u32.to_le_bytes());
        definition.push(column.mysql_type);
        definition.extend_from_slice(&0u16.to_le_bytes());
        definition.push(0);
        definition.extend_from_slice(&[0, 0]);
        write_packet(stream, &definition, sequence).await?;
        sequence = sequence.wrapping_add(1);
    }

    write_packet(stream, &[0xfe, 0, 0, 2, 0], sequence).await?;
    sequence = sequence.wrapping_add(1);
    for row in result.rows {
        let mut packet = Vec::new();
        for value in row {
            match value {
                Some(value) => encode_string(&value, &mut packet),
                None => packet.push(0xfb),
            }
        }
        write_packet(stream, &packet, sequence).await?;
        sequence = sequence.wrapping_add(1);
    }
    write_packet(stream, &[0xfe, 0, 0, 2, 0], sequence).await
}

async fn write_ok(
    stream: &mut TcpStream,
    affected_rows: u64,
    last_insert_id: u64,
    sequence: u8,
) -> io::Result<()> {
    let mut packet = vec![0];
    encode_length(affected_rows, &mut packet);
    encode_length(last_insert_id, &mut packet);
    packet.extend_from_slice(&STATUS_AUTOCOMMIT.to_le_bytes());
    packet.extend_from_slice(&0u16.to_le_bytes());
    write_packet(stream, &packet, sequence).await
}

async fn write_error(
    stream: &mut TcpStream,
    code: u16,
    state: &str,
    message: &str,
    sequence: u8,
) -> io::Result<()> {
    let mut packet = vec![0xff];
    packet.extend_from_slice(&code.to_le_bytes());
    packet.push(b'#');
    packet.extend_from_slice(state.as_bytes());
    packet.extend_from_slice(message.as_bytes());
    write_packet(stream, &packet, sequence).await
}

async fn read_packet(stream: &mut TcpStream) -> io::Result<(u8, Vec<u8>)> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let length =
        usize::from(header[0]) | (usize::from(header[1]) << 8) | (usize::from(header[2]) << 16);
    let mut payload = vec![0; length];
    stream.read_exact(&mut payload).await?;
    Ok((header[3], payload))
}

async fn write_packet(stream: &mut TcpStream, payload: &[u8], sequence: u8) -> io::Result<()> {
    if payload.len() > MAX_PACKET_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MySQL packet exceeds the protocol maximum",
        ));
    }
    let length = payload.len();
    stream
        .write_all(&[
            length as u8,
            (length >> 8) as u8,
            (length >> 16) as u8,
            sequence,
        ])
        .await?;
    stream.write_all(payload).await?;
    stream.flush().await
}

fn encode_string(value: &[u8], packet: &mut Vec<u8>) {
    encode_length(value.len() as u64, packet);
    packet.extend_from_slice(value);
}

fn encode_length(value: u64, packet: &mut Vec<u8>) {
    match value {
        0..=250 => packet.push(value as u8),
        251..=65_535 => {
            packet.push(0xfc);
            packet.extend_from_slice(&(value as u16).to_le_bytes());
        }
        65_536..=16_777_215 => {
            packet.push(0xfd);
            packet.extend_from_slice(&[value as u8, (value >> 8) as u8, (value >> 16) as u8]);
        }
        _ => {
            packet.push(0xfe);
            packet.extend_from_slice(&value.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_parser_reads_root_and_database() {
        let mut packet = vec![0; 32];
        packet[0..4].copy_from_slice(
            &(CLIENT_PROTOCOL_41 | CLIENT_SECURE_CONNECTION | CLIENT_CONNECT_WITH_DB).to_le_bytes(),
        );
        packet.extend_from_slice(b"root\0");
        packet.push(1);
        packet.push(42);
        packet.extend_from_slice(b"shop\0");

        let (capabilities, username, auth, database) = parse_login(&packet).unwrap();
        assert_eq!(capabilities & CLIENT_PROTOCOL_41, CLIENT_PROTOCOL_41);
        assert_eq!(username, "root");
        assert_eq!(auth, [42]);
        assert_eq!(database.as_deref(), Some("shop"));
    }

    #[test]
    fn password_verification_accepts_only_matching_native_token() {
        let seed = [7u8; 20];
        let password = "secret";
        let stage1 = Sha1::digest(password.as_bytes());
        let stage2 = Sha1::digest(stage1);
        let mut digest = Sha1::new();
        digest.update(seed);
        digest.update(stage2);
        let token = digest
            .finalize()
            .iter()
            .zip(stage1.iter())
            .map(|(left, right)| left ^ right)
            .collect::<Vec<_>>();

        assert!(verify_password(password, &seed, &token));
        assert!(!verify_password("wrong", &seed, &token));
    }

    #[test]
    fn database_names_cannot_escape_the_data_directory() {
        assert!(valid_database_name("shop_2"));
        assert!(!valid_database_name("../outside"));
        assert!(!valid_database_name("shop.db"));
    }
}
