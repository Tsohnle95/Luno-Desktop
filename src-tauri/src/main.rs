#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use luno_desktop_lib::contracts::{CommandEnvelope, ContractError};
use luno_desktop_lib::migration::first_run::{migrate, MigrationOptions, MigrationPaths};
use luno_desktop_lib::migration::keychain::MacOsKeychain;
use luno_desktop_lib::migration::legacy_readers::has_legacy_data;
use luno_desktop_lib::state::AppState;
use serde_json::Value;
use std::path::PathBuf;
use tauri::Manager;

#[tauri::command]
fn backend_command(_state: tauri::State<'_, AppState>, envelope: CommandEnvelope) -> Result<Value, String> {
    let command = serde_json::to_value(&envelope.command).map_err(|error| error.to_string())?;
    let error = ContractError::invalid(format!("command is not implemented by the target shell: {command}"));
    Err(serde_json::to_string(&error).unwrap_or_else(|_| "unsupported command".into()))
}

#[cfg(target_os = "macos")]
fn startup_migration(app: &tauri::AppHandle) -> Result<(), String> {
    let legacy_root = dirs_fallback_home()?;
    let preview_dir = legacy_root.join(".luno_previews");
    if !has_legacy_data(&legacy_root, Some(&preview_dir)) { return Ok(()); }
    let target_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let paths = MigrationPaths {
        legacy_root,
        target_root,
        preview_dir,
        current_preview: Vec::new(),
        promoted_assets: Vec::new(),
    };
    let mut keychain = MacOsKeychain::new();
    match migrate(&paths, &MigrationOptions::default(), &mut keychain) {
        Ok(_) => Ok(()),
        Err(error) => Err(format!("startup migration failed: {error:?}")),
    }
}

#[cfg(not(target_os = "macos"))]
fn startup_migration(_app: &tauri::AppHandle) -> Result<(), String> { Ok(()) }

fn dirs_fallback_home() -> Result<PathBuf, String> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| "HOME is not available".into())
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::new(32))
        .invoke_handler(tauri::generate_handler![backend_command])
        .setup(|app| {
            startup_migration(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Luno");
}
