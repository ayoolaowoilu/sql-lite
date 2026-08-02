fn main() {
    #[cfg(windows)]
    {
        let mut res = tauri_winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "MySQLite - Database Manager");
        res.set("ProductName", "MySQLite");
        res.set("OriginalFilename", "sq-lite.exe");
        res.compile().expect("Failed to compile Windows resources");
    }
}