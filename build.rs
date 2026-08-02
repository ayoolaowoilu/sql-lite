use std::io;

#[cfg(windows)]
fn main() -> io::Result<()> {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("FileDescription", "MySQLite - Database Manager");
    res.set("ProductName", "MySQLite");
    res.set("OriginalFilename", "mysqlite.exe");
    res.compile()
}

#[cfg(not(windows))]
fn main() {}