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

#[cfg(debug_assertions)]
const GATE5_EVENT: &str = "gate5-smoke-event";

#[cfg(debug_assertions)]
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
async fn backend_command(
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
    // Dispatch MUST leave the main thread: commands may block on network
    // (Last.fm discovery) or process spawns (yt-dlp search), and a sync
    // command would freeze the whole window for their duration.
    let app_state = state.inner().clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app_state.dispatch(envelope)))
            .unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("dispatch panicked");
                // PoisonError is an internal detail — surface a sanitized internal error
                // so the UI does not show `PoisonError { .. }` and a restart can recover
                // via lock poisoning recovery.
                let sanitized = if message.contains("PoisonError") {
                    "internal dispatch error (poison recovered)".to_string()
                } else {
                    format!("dispatch panicked: {message}")
                };
                Err(ContractError {
                    code: luno_desktop_lib::contracts::types::ErrorCode::Internal,
                    message: sanitized,
                    retryable: true,
                })
            });
        if let Err(error) = drain_pending_events(&handle, &app_state) {
            return Err(error_json(ContractError {
                code: luno_desktop_lib::contracts::types::ErrorCode::Internal,
                message: error,
                retryable: true,
            }));
        }
        result.map_err(error_json)
    })
    .await
    .map_err(|error| format!("command worker failed: {error}"))?
}

fn dispatch_target(app: &tauri::AppHandle, state: &AppState, command: luno_desktop_lib::contracts::events::Command) -> Result<Value, String> {
    let envelope = CommandEnvelope { version: 1, request_id: format!("target-{}", std::process::id()), command };
    let result = state.dispatch(envelope).map_err(error_json);
    drain_pending_events(app, state).map_err(|error| error_json(ContractError { code: luno_desktop_lib::contracts::types::ErrorCode::Internal, message: error, retryable: true }))?;
    result
}

/// Target dispatch that keeps the (possibly blocking) command work OFF the
/// main thread — yt-dlp searches and settings IO must never stall the window.
async fn dispatch_target_async(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    command: luno_desktop_lib::contracts::events::Command,
) -> Result<Value, String> {
    let inner = state.inner().clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch_target(&handle, &inner, command)))
            .unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("dispatch panicked");
                let sanitized = if message.contains("PoisonError") {
                    "internal dispatch error (poison recovered)".to_string()
                } else {
                    format!("dispatch panicked: {message}")
                };
                Err(sanitized)
            })
    })
    .await
    .map_err(|error| format!("command worker failed: {error}"))?
}

#[tauri::command]
async fn downloader_search(app: tauri::AppHandle, state: tauri::State<'_, AppState>, query: String, try_soundcloud: bool, limit: Option<usize>) -> Result<Value, String> {
    dispatch_target_async(app, state, luno_desktop_lib::contracts::events::Command::SearchDownloader {
        query,
        try_soundcloud,
        limit: limit.unwrap_or(10),
    })
    .await
}

#[tauri::command]
async fn settings_load(app: tauri::AppHandle, state: tauri::State<'_, AppState>, home: String) -> Result<Value, String> {
    dispatch_target_async(app, state, luno_desktop_lib::contracts::events::Command::LoadSettings { home }).await
}

#[tauri::command]
async fn transfer_preview(app: tauri::AppHandle, state: tauri::State<'_, AppState>, manifest: String, destination: String) -> Result<Value, String> {
    dispatch_target_async(app, state, luno_desktop_lib::contracts::events::Command::TransferPreview { manifest, destination }).await
}

fn error_json(error: ContractError) -> String {
    serde_json::to_string(&error).unwrap_or_else(|_| {
        "{\"code\":\"internal\",\"message\":\"internal error\",\"retryable\":false}".into()
    })
}

/// Range-aware streaming of staged videos over the `luno-video://localhost`
/// scheme (`video::stream`). The stock asset protocol buffers whole files
/// when the first media request carries no `Range` header, and wry delivers
/// the entire body as ONE NSData blob — for 100 MB+ staged MP4s that froze
/// the window during song switches. Here EVERY response is a small ≤2 MB
/// window, so the webview stays in cheap ranged mode. All decision logic and
/// file IO live in `video::stream::response_for` (unit-tested there); this
/// wiring only resolves the store root and runs it off the UI thread.

#[tauri::command]
fn migration_status(state: tauri::State<'_, AppState>) -> Result<MigrationStatus, String> {
    Ok(state
        .migration_status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}

#[tauri::command]
async fn track_artwork(
    state: tauri::State<'_, AppState>,
    track_id: String,
) -> Result<Option<String>, String> {
    // Extraction reads audio files (ID3/symphonia probe); keep it off the
    // main thread so scrolling rows requesting artwork never jank the UI.
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app.track_artwork(&track_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
fn discovery_key_status(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    Ok(serde_json::json!({ "configured": state.has_discovery_key() }))
}

/// Stores or clears the Last.fm API key in the app-data key file (0600).
/// Never touches the macOS Keychain — that item is legacy read-only seed
/// material and must not generate prompts.
#[tauri::command]
fn save_discovery_key(app: tauri::AppHandle, state: tauri::State<'_, AppState>, key: String) -> Result<Value, String> {
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        if let Some(path) = discovery_key_path(&app) {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("could not clear discovery key: {error}")),
            }
        }
        state.set_discovery_key(None);
    } else {
        let path = discovery_key_path(&app).ok_or_else(|| "app data directory is unavailable".to_string())?;
        write_key_file(&path, trimmed.as_bytes()).map_err(|error| format!("could not store discovery key: {error}"))?;
        state.set_discovery_key(Some(trimmed));
    }
    Ok(serde_json::json!({ "configured": state.has_discovery_key() }))
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

fn resolve_bundle_helper(app: &tauri::AppHandle) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = app.path().resource_dir().ok() {
        candidates.push(dir.join("bin").join("luno-download-helper"));
    }
    if let Some(dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(dir).join("resources").join("bin").join("luno-download-helper"));
    }
    let resolved = candidates.iter().find(|path| path.is_file()).cloned();
    eprintln!("bundle helper candidates: {candidates:?} -> {resolved:?}");
    resolved
}

fn dirs_fallback_home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not available".into())
}

fn discovery_key_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|root| root.join("lastfm-key"))
}

/// Write the discovery key with owner-only permissions (0600).
fn write_key_file(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    fn apply_mode(options: &mut std::fs::OpenOptions) {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    fn apply_mode(_options: &mut std::fs::OpenOptions) {}
    let mut options = std::fs::OpenOptions::new();
    apply_mode(&mut options);
    let mut file = options.write(true).create(true).truncate(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// The Last.fm API key lives in a 0600 file under app data. The macOS Keychain
/// is consulted AT MOST ONCE per install — silently, via kSecUseAuthenticationUI
/// =Deny, which structurally cannot raise a prompt — purely to seed that file.
/// Startup therefore never triggers a Keychain dialog again.
fn load_discovery_key(app: &tauri::AppHandle) -> Option<String> {
    let path = discovery_key_path(app)?;
    if let Ok(existing) = fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let key = MacOsKeychain::new().try_read_lastfm_key_silent()?;
    let key = String::from_utf8_lossy(&key).trim().to_string();
    if key.is_empty() {
        return None;
    }
    if let Err(error) = write_key_file(&path, key.as_bytes()) {
        eprintln!("[luno-discovery] could not persist lastfm-key file: {error}");
    } else {
        eprintln!("[luno-discovery] seeded lastfm-key file from keychain (one-time)");
    }
    Some(key)
}

fn main() {
    // Single-instance guard: all installs share ~/.luno_config.json and the
    // app-data store, so a second concurrent instance last-writer-wins the
    // config (this once flipped video_mode back off mid-download). Claim a
    // localhost port; if it is taken, a Luno is already running — exit.
    // Isolated QA fixtures set LUNO_INSTANCE_PORT to run side-by-side.
    let guard_port: u16 = std::env::var("LUNO_INSTANCE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(48713);
    match std::net::TcpListener::bind(("127.0.0.1", guard_port)) {
        Ok(listener) => std::mem::forget(listener),
        Err(_) => {
            eprintln!("[luno] another Luno instance is already running; exiting this duplicate.");
            return;
        }
    }
    let builder = tauri::Builder::default()
        .manage(AppState::new(32))
        .register_asynchronous_uri_scheme_protocol(
            luno_desktop_lib::video::stream::STREAM_SCHEME,
            |ctx, request, responder| {
                // Scheme callbacks arrive on the UI thread; the (bounded)
                // file read runs on the async runtime so the window never
                // waits on disk for a media request.
                let app = ctx.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let response = match app.state::<AppState>().video_root_dir() {
                        Some(root) => luno_desktop_lib::video::stream::response_for(
                            &root,
                            request.method().as_str(),
                            request.uri().path(),
                            request.headers().get("range").and_then(|value| value.to_str().ok()),
                        ),
                        None => tauri::http::Response::builder().status(404).body(Vec::new()).unwrap(),
                    };
                    responder.respond(response);
                });
            },
        );
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![backend_command, downloader_search, settings_load, transfer_preview, migration_status, track_artwork, discovery_key_status, save_discovery_key, gate5_smoke]);
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![backend_command, downloader_search, settings_load, transfer_preview, migration_status, track_artwork, discovery_key_status, save_discovery_key]);
    builder
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
            // Hardware media keys (play/pause/next/previous): claim them via
            // MPRemoteCommandCenter so they control Luno instead of falling
            // through to Apple Music. Honors Settings ▸ Playback live.
            #[cfg(target_os = "macos")]
            {
                let key_state = app.state::<AppState>().inner().clone();
                let keys = luno_desktop_lib::media_keys::MediaKeys::install(
                    std::sync::Arc::new(move |key| key_state.handle_media_key(key)),
                );
                // Remote-command targets must outlive this closure; the app
                // process is their lifetime anyway.
                std::mem::forget(keys);
            }
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
            app.state::<AppState>()
                .set_bundle_helper(resolve_bundle_helper(app.handle()));
            let discovery_key = load_discovery_key(app.handle());
            app.state::<AppState>().set_discovery_key(discovery_key);
            let preview_helper = resolve_bundle_helper(app.handle());
            let managed_bin = preview_helper
                .as_ref()
                .and_then(|path| path.parent())
                .map(|parent| parent.to_path_buf());
            let preview_ffmpeg = luno_desktop_lib::process::executables::discover(
                luno_desktop_lib::process::executables::ExecutableKind::Ffmpeg,
                managed_bin.as_deref(),
            )
            .ok();
            app.state::<AppState>().set_preview_toolchain(preview_helper, preview_ffmpeg);
            // Video staging: a disposable store OUTSIDE the music library plus
            // the discovered yt-dlp/ffmpeg pair. The asset-protocol scope is
            // widened to the store so <video> can stream staged files.
            let video_root = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("luno-desktop"))
                .join("videos");
            fs::create_dir_all(&video_root).map_err(std::io::Error::other)?;
            let ffmpeg_dir = managed_bin.as_deref().map(PathBuf::from);
            let video_ytdlp = luno_desktop_lib::process::executables::discover(
                luno_desktop_lib::process::executables::ExecutableKind::YtDlp,
                managed_bin.as_deref(),
            )
            .ok();
            app.state::<AppState>().set_video_runtime(video_root.clone(), video_ytdlp, ffmpeg_dir);
            {
                let scope = app.asset_protocol_scope();
                scope.allow_directory(&video_root, false).map_err(std::io::Error::other)?;
            }
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
