// Codex Windows — Tauri Bridge
// Replaces: bridge.js + serve.js + launcher.ps1 + Edge --app mode
// Manages: codex.exe lifecycle, WebSocket connection, terminal sessions

// Hide the Windows console window in release builds only.
// Debug builds keep the console visible so that tracing output, panic messages,
// and the remote DevTools port announcement are readable during development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod codex_backend;
mod commands;
mod terminal;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use tauri::Manager;
use tokio::sync::mpsc;
use tracing_appender::non_blocking::WorkerGuard;

// Architecture detection at compile-time
const TARGET_TRIPLE: &str = if cfg!(target_arch = "x86_64") {
    "x86_64-pc-windows-msvc"
} else if cfg!(target_arch = "aarch64") {
    "aarch64-pc-windows-msvc"
} else {
    panic!("Unsupported architecture — only x86_64 and aarch64 supported")
};

const PLATFORM_PACKAGE: &str = if cfg!(target_arch = "x86_64") {
    "@openai/codex-win32-x64"
} else if cfg!(target_arch = "aarch64") {
    "@openai/codex-win32-arm64"
} else {
    panic!("Unsupported architecture")
};

const WS_PORT: u16 = 5557;

/// Resolves the vendor root directory. Looks inside the local `vendor` folder during
/// development, or the bundled resources during production.
fn resolve_vendor_root(app: &tauri::AppHandle) -> PathBuf {
    // Development: inside src-tauri/vendor/x86_64-pc-windows-msvc
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vendor")
        .join(TARGET_TRIPLE);
        
    if dev_path.join("codex.exe").exists() {
        return dev_path;
    }

    // Production: resources bundled alongside the executable
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("vendor").join(TARGET_TRIPLE);
        if bundled.join("codex.exe").exists() {
            return bundled;
        }
    }

    dev_path
}

/// Spawns the codex.exe process with the correct PATH (includes rg.exe).
/// Uses the flat layout (codex.exe and rg.exe side-by-side).
fn spawn_codex(vendor_root: &PathBuf) -> Result<Child, String> {
    let codex_exe = vendor_root.join("codex.exe");
    if !codex_exe.exists() {
        return Err(format!("codex.exe not found at: {}", codex_exe.display()));
    }

    // PATH for rg.exe (which sits right next to codex.exe)
    let mut path_env = vendor_root.to_string_lossy().to_string();
    path_env.push(';');
    path_env.push_str(&std::env::var("PATH").unwrap_or_default());

    let listen_addr = format!("ws://127.0.0.1:{}", WS_PORT);

    // --- CODEX_HOME isolation ---
    // Use a dedicated CODEX_HOME for our Tauri instance to avoid SQLite WAL
    // lock conflicts and session cross-contamination with other Codex instances
    // (VS Code extension, official Codex Desktop, etc.).
    // Auth and config are shared via symlinks created in ensure_codex_home_isolation().
    let codex_home = ensure_codex_home_isolation();

    let mut cmd = Command::new(&codex_exe);
    cmd.args(["app-server", "--listen", &listen_addr])
        .env("PATH", &path_env)
        .env("CODEX_HOME", &codex_home)
        .env(
            "OPENAI_API_KEY",
            std::env::var("OPENAI_API_KEY").unwrap_or_default(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    // Hide the console window for codex.exe on Windows
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn()
        .map_err(|e| format!("Failed to spawn codex.exe: {}", e))
}

/// Returns the standard CODEX_HOME path to perfectly mirror the original app.
/// We remove isolation because the user expects full continuity with their previous sessions
/// and worktrees from the official application.
fn ensure_codex_home_isolation() -> String {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".to_string());
    let shared_codex = PathBuf::from(&home).join(".codex");
    
    // Create it if it doesn't exist just in case
    let _ = std::fs::create_dir_all(&shared_codex);

    shared_codex.to_string_lossy().to_string()
}

/// Application state shared across all Tauri commands.
pub struct AppState {
    pub codex_status: Mutex<String>,
    pub codex_ws_tx: Mutex<Option<mpsc::UnboundedSender<String>>>,
    pub terminal_sessions: Mutex<HashMap<String, terminal::TerminalSession>>,
    pub codex_process: Mutex<Option<Child>>,
    _log_guard: WorkerGuard,
}

/// Fix WebView2 user-data folder permissions so the app works correctly
/// whether launched as Admin or as a normal user.
///
/// Problem: When first run as Admin, WebView2 creates its user-data cache
/// (under `%LOCALAPPDATA%\com.codex.windows`) with Admin-only ACLs.
/// A subsequent launch as a normal user cannot access those files → blank screen.
///
/// Solution: Use `icacls` to grant the "Users" group full control over the
/// data directory. This is safe because it's the app's own cache, not system data.
fn fix_webview2_permissions() {
    // Tauri v2 stores WebView2 data at %LOCALAPPDATA%\<identifier>
    // Our identifier is "com.codex.windows" from tauri.conf.json
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    if local_app_data.is_empty() {
        return;
    }

    let wv2_dir = PathBuf::from(&local_app_data).join("com.codex.windows");

    // Only fix if the directory already exists (was previously created by an Admin run)
    if wv2_dir.exists() {
        let mut cmd = Command::new("icacls");
        cmd.args([
            &wv2_dir.to_string_lossy().to_string(),
            "/grant",
            "Users:(OI)(CI)F",
            "/T",
            "/Q",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let _ = cmd.status();
    }
}

fn main() {
    // --- Fix WebView2 user-data permissions (Admin vs non-Admin) --------
    // When the app is first launched as Admin, the WebView2 runtime creates
    // its user-data folder with Admin-only ACLs. Any subsequent launch as a
    // normal user results in a blank/black window because WebView2 cannot
    // access its own cache. We proactively grant the current user full
    // control over the data directory so both scenarios work.
    fix_webview2_permissions();

    tauri::Builder::default()
        .setup(|app| {
            // --- Logging setup (tracing-appender) ---
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            std::fs::create_dir_all(&log_dir).ok();

            let file_appender = tracing_appender::rolling::daily(&log_dir, "codex.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

            tracing_subscriber::fmt()
                .with_target(true)
                .with_writer(non_blocking)
                .init();

            tracing::info!("Codex Windows starting (Tauri bridge)");

            // --- Spawn codex.exe ---
            let vendor_root = resolve_vendor_root(app.handle());
            tracing::info!("Vendor root: {}", vendor_root.display());

            let codex_child = match spawn_codex(&vendor_root) {
                Ok(mut child) => {
                    tracing::info!("codex.exe spawned (pid: {})", child.id());

                    // Stream codex.exe stderr to tracing so errors are visible
                    if let Some(stderr) = child.stderr.take() {
                        std::thread::spawn(move || {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(stderr);
                            for line in reader.lines() {
                                match line {
                                    Ok(l) => tracing::warn!(target: "codex.exe", "{}", l),
                                    Err(_) => break,
                                }
                            }
                        });
                    }

                    Some(child)
                }
                Err(e) => {
                    tracing::error!("Failed to spawn codex.exe: {}", e);
                    eprintln!("[ERROR] {}", e);
                    None
                }
            };

            // --- Create managed state ---
            let state = AppState {
                codex_status: Mutex::new("connecting".to_string()),
                codex_ws_tx: Mutex::new(None),
                terminal_sessions: Mutex::new(HashMap::new()),
                codex_process: Mutex::new(codex_child),
                _log_guard: guard,
            };
            app.manage(state);

            // --- Start WebSocket connection to codex.exe (async task) ---
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                codex_backend::run_codex_connection(app_handle, WS_PORT).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_context,
            commands::get_codex_status,
            commands::send_to_codex,
            commands::log_diag,
            commands::open_external_url,
            commands::create_terminal,
            commands::attach_terminal,
            commands::write_terminal,
            commands::resize_terminal,
            commands::detach_terminal,
            commands::close_terminal,
            commands::pick_folder,
            commands::pick_file,
            commands::pick_files,
            commands::git_status,
            commands::git_origins,
            commands::git_push,
            commands::git_create_branch,
            commands::git_checkout_branch,
            commands::git_apply_patch,
            commands::git_merge_base,
            commands::gh_pr_create,
            commands::gh_pr_status,
            commands::find_files,
            commands::paths_exist,
            commands::read_file_contents,
            commands::read_file_binary,
            commands::read_git_file_binary,
            commands::detect_open_targets,
            commands::gh_cli_status,
            commands::open_in_target,
            commands::show_native_context_menu,
        ])
        .on_window_event(|window, event| {
            // Kill codex.exe when the main window is destroyed
            if let tauri::WindowEvent::Destroyed = event {
                let app = window.app_handle();
                if let Some(state) = app.try_state::<AppState>() {
                    // Kill codex.exe
                    if let Ok(mut proc) = state.codex_process.lock() {
                        if let Some(ref mut child) = *proc {
                            tracing::info!("Killing codex.exe on window close");
                            let _ = child.kill();
                        }
                    }
                    // Kill all terminal sessions
                    if let Ok(mut sessions) = state.terminal_sessions.lock() {
                        for (id, session) in sessions.iter_mut() {
                            tracing::info!("Killing terminal session: {}", id);
                            session.kill();
                        }
                        sessions.clear();
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Codex Windows");
}
