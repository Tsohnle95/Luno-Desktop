#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use luno_desktop_lib::contracts::{CommandEnvelope, ContractError};
use luno_desktop_lib::migration::first_run::{migrate, MigrationOptions, MigrationPaths};
use luno_desktop_lib::migration::keychain::MacOsKeychain;
use luno_desktop_lib::migration::legacy_readers::{has_legacy_data, validated_music_root};
use luno_desktop_lib::state::{AppState, MigrationStatus};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

const BACKEND_EVENT: &str = "backend-event";
const GATE5_EVENT: &str = "gate5-smoke-event";

#[tauri::command(rename_all = "snake_case")]
fn gate5_smoke(app: tauri::AppHandle, root: String, preview_root: String, video_root: String) -> Result<Value, String> {
    let result = luno_desktop_lib::runtime::gate5_smoke::run(PathBuf::from(root), PathBuf::from(preview_root), PathBuf::from(video_root))?;
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    app.emit(GATE5_EVENT, value.clone()).map_err(|error| error.to_string())?;
    Ok(value)
}

fn drain_pending_events(app: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    state
        .drain_events(|event| {
            app.emit(BACKEND_EVENT, event)
                .map_err(|error| error.to_string())
        })
        .map(|_| ())
}

#[tauri::command]
fn backend_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    payload: Value,
) -> Result<Value, String> {
    let envelope: CommandEnvelope = serde_json::from_value(payload.clone()).map_err(|error| {
        let code = payload
            .get("version")
            .and_then(Value::as_u64)
            .filter(|version| *version != 1)
            .map(|_| luno_desktop_lib::contracts::types::ErrorCode::UnsupportedVersion)
            .unwrap_or(luno_desktop_lib::contracts::types::ErrorCode::InvalidRequest);
        error_json(ContractError {
            code,
            message: format!("invalid command envelope: {error}"),
            retryable: false,
        })
    })?;
    let result = state.dispatch(envelope);
    drain_pending_events(&app, &state).map_err(|error| {
        error_json(ContractError {
            code: luno_desktop_lib::contracts::types::ErrorCode::Internal,
            message: error,
            retryable: true,
        })
    })?;
    result.map_err(error_json)
}

fn dispatch_target(app: &tauri::AppHandle, state: &AppState, command: luno_desktop_lib::contracts::events::Command) -> Result<Value, String> {
    let envelope = CommandEnvelope { version: 1, request_id: format!("target-{}", std::process::id()), command };
    let result = state.dispatch(envelope).map_err(error_json);
    drain_pending_events(app, state).map_err(|error| error_json(ContractError { code: luno_desktop_lib::contracts::types::ErrorCode::Internal, message: error, retryable: true }))?;
    result
}

#[tauri::command]
fn downloader_search(app: tauri::AppHandle, state: tauri::State<'_, AppState>, query: String, try_soundcloud: bool) -> Result<Value, String> {
    dispatch_target(&app, &state, luno_desktop_lib::contracts::events::Command::SearchDownloader { query, try_soundcloud })
}

#[tauri::command]
fn settings_load(app: tauri::AppHandle, state: tauri::State<'_, AppState>, home: String) -> Result<Value, String> {
    dispatch_target(&app, &state, luno_desktop_lib::contracts::events::Command::LoadSettings { home })
}

#[tauri::command]
fn transfer_preview(app: tauri::AppHandle, state: tauri::State<'_, AppState>, manifest: String, destination: String) -> Result<Value, String> {
    dispatch_target(&app, &state, luno_desktop_lib::contracts::events::Command::TransferPreview { manifest, destination })
}

fn error_json(error: ContractError) -> String {
    serde_json::to_string(&error).unwrap_or_else(|_| {
        "{\"code\":\"internal\",\"message\":\"internal error\",\"retryable\":false}".into()
    })
}

#[tauri::command]
fn migration_status(state: tauri::State<'_, AppState>) -> Result<MigrationStatus, String> {
    state
        .migration_status
        .lock()
        .map(|status| status.clone())
        .map_err(|_| "migration status unavailable".into())
}

#[cfg(target_os = "macos")]
fn migration_failed(app: &tauri::AppHandle, reason: String) -> Result<(), String> {
    *app.state::<AppState>()
        .migration_status
        .lock()
        .map_err(|_| "migration status unavailable".to_string())? =
        MigrationStatus::Failed { reason };
    Ok(())
}

#[cfg(target_os = "macos")]
fn startup_migration(app: &tauri::AppHandle) -> Result<(), String> {
    let legacy_root = match dirs_fallback_home() {
        Ok(root) => root,
        Err(error) => return migration_failed(app, format!("legacy HOME is unavailable: {error}")),
    };
    let target_root = match app.path().app_data_dir() {
        Ok(root) => root,
        Err(error) => {
            return migration_failed(app, format!("app data directory is unavailable: {error}"))
        }
    };
    let preview_dir = legacy_root.join(".luno_previews");
    let completed = fs::read(target_root.join("migration-version.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| {
            Some(
                value.get("migration")?.as_str()? == "desktop-first-run"
                    && value.get("version")?.as_u64()? == 1
                    && value.get("completed")?.as_bool()?,
            )
        })
        .unwrap_or(false);
    if !completed {
        match has_legacy_data(&legacy_root, Some(&preview_dir)) {
            Ok(false) => {
                *app.state::<AppState>()
                    .migration_status
                    .lock()
                    .map_err(|_| "migration status unavailable".to_string())? =
                    MigrationStatus::Skipped {
                        reason: "no legacy data found".into(),
                    };
                return Ok(());
            }
            Err(error) => {
                return migration_failed(
                    app,
                    format!("legacy migration input was unsafe: {error}"),
                )
            }
            Ok(true) => {}
        }
    }
    let configured_music_root = match validated_music_root(&legacy_root) {
        Ok(root) => root,
        Err(error) => {
            return migration_failed(app, format!("configured music root was unsafe: {error}"))
        }
    };
    app.state::<AppState>()
        .set_scan_roots(configured_music_root.iter().cloned().collect());
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
            let status = MigrationStatus::Completed {
                warnings: receipt.warnings,
                snapshot_created: receipt.snapshot.is_some(),
                removed_previews: receipt.removed_previews.len(),
            };
            *app.state::<AppState>()
                .migration_status
                .lock()
                .map_err(|_| "migration status unavailable".to_string())? = status;
            Ok(())
        }
        Err(error) => migration_failed(app, format!("startup migration failed: {error:?}")),
    }
}

#[cfg(not(target_os = "macos"))]
fn startup_migration(app: &tauri::AppHandle) -> Result<(), String> {
    *app.state::<AppState>()
        .migration_status
        .lock()
        .map_err(|_| "migration status unavailable".to_string())? = MigrationStatus::Skipped {
        reason: "platform migration is not applicable".into(),
    };
    Ok(())
}

fn dirs_fallback_home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not available".into())
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::new(32))
        .invoke_handler(tauri::generate_handler![backend_command, downloader_search, settings_load, transfer_preview, migration_status, gate5_smoke])
        .setup(|app| {
            let handle = app.handle().clone();
            app.state::<AppState>()
                .set_event_sink(std::sync::Arc::new(move |event| {
                    handle.emit(BACKEND_EVENT, event).map_err(|error| {
                        eprintln!("structured backend event delivery failure: {error}");
                        "event delivery failed".to_string()
                    })
                }));
            app.state::<AppState>().replay_latest();
            let retry_handle = app.handle().clone();
            let retry_state = app.state::<AppState>().inner().clone();
            std::thread::Builder::new()
                .name("luno-event-delivery".into())
                .spawn(move || {
                    while !retry_state.event_delivery_stopped() {
                        let _ = drain_pending_events(&retry_handle, &retry_state);
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                })
                .map_err(std::io::Error::other)?;
            startup_migration(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                let state = window.app_handle().state::<AppState>();
                state.shutdown_events(
                    luno_desktop_lib::contracts::events::ShutdownReason::UserRequested,
                    std::time::Duration::from_millis(500),
                );
                let _ = drain_pending_events(&window.app_handle(), &state);
                state.stop_event_delivery();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Luno")
        .run(|app, event| {
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                let state = app.state::<AppState>();
                state.shutdown_events(
                    luno_desktop_lib::contracts::events::ShutdownReason::System,
                    std::time::Duration::from_millis(500),
                );
                let _ = drain_pending_events(app, &state);
                state.stop_event_delivery();
            }
        });
}
