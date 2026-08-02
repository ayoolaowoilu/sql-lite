# ◈ MySQLite

A lightweight, browser-based SQLite database manager built with **Rust** and **Axum**. Create databases, run queries, manage tables — all through a clean web interface. Data persists to real `.db` files on disk.

![License](https://img.shields.io/badge/license-MIT-black)
![Rust](https://img.shields.io/badge/rust-1.75%2B-black?logo=rust)

<p align="center">
  <img src="assets/icon.png" width="120" alt="MySQLite Logo">
</p>

## Features

- **Multi-database support** — Create, switch between, and drop multiple `.db` files
- **MySQL-like commands** — Use `CREATE DATABASE`, `USE`, `DROP DATABASE` just like MySQL
- **Full SQL support** — `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `CREATE TABLE`, `ALTER`, `DROP`, `PRAGMA`
- **Resizable UI** — Drag to resize sidebar, query editor, results panel, and logs
- **Query history** — Every query is logged and clickable to restore
- **Live logs** — Auto-refreshing execution log at the bottom
- **Table browser** — Click any table in the sidebar to auto-generate a `SELECT` query
- **Light theme** — Clean, professional light UI with black accents
- **Auto-opens browser** — Double-click the `.exe` and the site opens automatically
- **Zero config** — No installation, no dependencies, just run and go

## Quick Start

### Pre-built (Windows)

1. Download the latest release
2. Extract the folder
3. Double-click `mysqlite.exe`
4. Your browser opens at `http://localhost:6141`

### Build from source

```bash
# Clone
git clone https://github.com/yourname/mysqlite.git
cd mysqlite

# Build release
cargo build --release

# Run
cargo run --release
```

The server starts on **port 6141** and automatically opens your default browser.

## Usage

```sql
-- Create a new database file
CREATE DATABASE shop;

-- Switch to it
USE shop;

-- Create tables
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    email TEXT
);

-- Insert data
INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com');
INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com');

-- Query
SELECT * FROM users;

-- Drop when done
DROP DATABASE shop;
```

All databases are stored as real SQLite `.db` files in the `data/` folder.

## File Structure

```
mysqlite/
├── Cargo.toml          # Rust dependencies
├── build.rs            # Windows icon embedding
├── src/
│   └── main.rs         # Axum server + SQLite engine
├── static/             # Frontend files
│   ├── index.html
│   ├── style.css
│   └── app.js
├── assets/             # App icons
│   ├── icon.svg
│   ├── icon.png
│   └── icon.ico
└── data/               # Your .db files (auto-created)
```

## Building a Windows .exe with Icon

### Requirements

- Windows with [Rust](https://rustup.rs/) installed
- Visual Studio Build Tools (or MinGW)

### Steps

```bash
# 1. Install Windows resource compiler dependency
cargo install cargo-wix  # optional: for MSI installer

# 2. Build release (icon auto-embedded from assets/icon.ico)
cargo build --release

# 3. Output
# target/release/mysqlite.exe
```

The `.exe` will have the database cylinder icon in Explorer, Taskbar, and Alt-Tab.

### Cross-compile from Linux/Mac

```bash
# Install target
rustup target add x86_64-pc-windows-gnu

# Linux: install mingw
sudo apt-get install mingw-w64

# Build
cargo build --release --target x86_64-pc-windows-gnu
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl + Enter` | Run query |
| `Enter` (in DB name input) | Create database |

## Tech Stack

| Layer | Tech |
|-------|------|
| Backend | Rust, Axum, Tokio, rusqlite |
| Frontend | Vanilla JS, CSS Grid/Flexbox, SVG icons |
| Database | SQLite (file-based) |

## License

MIT — do whatever you want.
