#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use luno_desktop_lib::contracts::{CommandEnvelope, ContractError};
use luno_desktop_lib::migration::first_run::{migrate, MigrationOptions, MigrationPaths};
use luno_desktop_lib::migration::keychain::MacOsKeychain;
use luno_desktop_lib::migration::legacy_readers::{has_legacy_data, validated_music_root};
use luno_desktop_lib::state::{AppState, MigrationStatus};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tauri::{Emitter, Manager};

const BACKEND_EVENT: &str = "backend-event";

#[tauri::command]
fn backend_command(app: tauri::AppHandle, state: tauri::State<'_, AppState>, payload: Value) -> Result<Value, String> {
    let envelope: CommandEnvelope = serde_json::from_value(payload.clone()).map_err(|error| {
        let code = payload.get("version").and_then(Value::as_u64).filter(|version| *version != 1)
            .map(|_| luno_desktop_lib::contracts::types::ErrorCode::UnsupportedVersion)
            .unwrap_or(luno_desktop_lib::contracts::types::ErrorCode::InvalidRequest);
        error_json(ContractError { code, message: format!("invalid command envelope: {error}"), retryable: false })
    })?;
    let result = state.dispatch(envelope);
    for event in state.events() {
        app.emit(BACKEND_EVENT, event).map_err(|error| error_json(ContractError { code: luno_desktop_lib::contracts::types::ErrorCode::Internal, message: error.to_string(), retryable: true }))?;
    }
    result.map_err(error_json)
}

fn error_json(error: ContractError) -> String { serde_json::to_string(&error).unwrap_or_else(|_| "{\"code\":\"internal\",\"message\":\"internal error\",\"retryable\":false}".into()) }

#[tauri::command]
fn migration_status(state: tauri::State<'_, AppState>) -> Result<MigrationStatus, String> {
    state.migration_status.lock().map(|status| status.clone()).map_err(|_| "migration status unavailable".into())
}

#[cfg(target_os = "macos")]
fn startup_migration(app: &tauri::AppHandle) -> Result<(), String> {
    let legacy_root = dirs_fallback_home()?;
    let preview_dir = legacy_root.join(".luno_previews");
    let target_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let completed = fs::read(target_root.join("migration-version.json")).ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| Some(value.get("migration")?.as_str()? == "desktop-first-run"
            && value.get("version")?.as_u64()? == 1 && value.get("completed")?.as_bool()?))
        .unwrap_or(false);
    if !completed {
        match has_legacy_data(&legacy_root, Some(&preview_dir)) {
            Ok(false) => return Ok(()),
            Err(error) => {
                app.state::<AppState>().migration_status.lock().map_err(|_| "migration status unavailable".to_string())
                    .map(|mut status| *status = MigrationStatus::Failed { reason: error.clone() })?;
                return Err(format!("unsafe legacy migration input: {error}"));
            }
            Ok(true) => {}
        }
    }
    let configured_music_root = validated_music_root(&legacy_root)?;
    let paths = MigrationPaths {
        legacy_root,
        legacy_data_root: configured_music_root.clone(),
        configured_music_root,
        target_root,
        preview_dir,
        current_preview: Vec::new(),
        promoted_assets: Vec::new(),
    };
    let mut keychain = MacOsKeychain::new();
    match migrate(&paths, &MigrationOptions::default(), &mut keychain) {
        Ok(receipt) => {
            let status = MigrationStatus::Completed { warnings: receipt.warnings, snapshot_created: receipt.snapshot.is_some(), removed_previews: receipt.removed_previews.len() };
            *app.state::<AppState>().migration_status.lock().map_err(|_| "migration status unavailable".to_string())? = status;
            Ok(())
        }
        Err(error) => {
            let status = MigrationStatus::Failed { reason: "migration failed".into() };
            *app.state::<AppState>().migration_status.lock().map_err(|_| "migration status unavailable".to_string())? = status;
            Err(format!("startup migration failed: {error:?}"))
        }
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
        .invoke_handler(tauri::generate_handler![backend_command, migration_status])
        .setup(|app| {
            let handle = app.handle().clone();
            app.state::<AppState>().set_event_sink(std::sync::Arc::new(move |event| {
                let _ = handle.emit(BACKEND_EVENT, event);
            }));
            startup_migration(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Luno");
}
