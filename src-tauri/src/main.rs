#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rusqlite::{params, Connection};
use std::fs;
use std::sync::Mutex;
use tauri::{Manager, State};

/// Shared, thread-safe handle to the single SQLite connection used by the whole app.
struct DbState(Mutex<Connection>);

fn init_db(conn: &Connection) {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS kv_store (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .expect("failed to create kv_store table");
}

/// Read one value by key. Returns None if the key does not exist yet
/// (this is normal on first run, or the first time a given data type is saved).
#[tauri::command]
fn db_get(key: String, state: State<DbState>) -> Option<String> {
    let conn = state.0.lock().unwrap();
    conn.query_row(
        "SELECT value FROM kv_store WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .ok()
}

/// Write/overwrite one key's value. This is a real, durable SQLite write —
/// once this command returns Ok, the data is safely on disk.
#[tauri::command]
fn db_set(key: String, value: String, state: State<DbState>) -> Result<(), String> {
    let conn = state.0.lock().unwrap();
    conn.execute(
        "INSERT INTO kv_store (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn db_delete(key: String, state: State<DbState>) -> Result<(), String> {
    let conn = state.0.lock().unwrap();
    conn.execute("DELETE FROM kv_store WHERE key = ?1", params![key])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Returns every stored key (used for import/export/backup tooling).
#[tauri::command]
fn db_list_keys(state: State<DbState>) -> Result<Vec<String>, String> {
    let conn = state.0.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT key FROM kv_store ORDER BY key")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut keys = Vec::new();
    for r in rows {
        keys.push(r.map_err(|e| e.to_string())?);
    }
    Ok(keys)
}

/// Returns the absolute path of the .db file on disk, so the app can show it
/// to the shop owner (e.g. "your data lives at: C:\Users\...\alyabani.db").
#[tauri::command]
fn db_file_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    let dir = app_handle
        .path_resolver()
        .app_data_dir()
        .ok_or("تعذر تحديد مجلد بيانات التطبيق")?;
    Ok(dir.join("alyabani.db").to_string_lossy().to_string())
}

/// Copies the live database file to a destination the user picked
/// (e.g. a USB drive or a backups folder) — a true, file-level backup.
#[tauri::command]
fn db_backup_to(dest_path: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    let dir = app_handle
        .path_resolver()
        .app_data_dir()
        .ok_or("تعذر تحديد مجلد بيانات التطبيق")?;
    let src = dir.join("alyabani.db");
    fs::copy(&src, &dest_path).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_dir = app
                .path_resolver()
                .app_data_dir()
                .expect("لا يمكن تحديد مجلد بيانات التطبيق");
            fs::create_dir_all(&app_dir).expect("تعذر إنشاء مجلد بيانات التطبيق");

            let db_path = app_dir.join("alyabani.db");
            let conn = Connection::open(&db_path).expect("تعذر فتح ملف قاعدة البيانات");
            init_db(&conn);

            app.manage(DbState(Mutex::new(conn)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_get,
            db_set,
            db_delete,
            db_list_keys,
            db_file_path,
            db_backup_to
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Tauri application");
}
