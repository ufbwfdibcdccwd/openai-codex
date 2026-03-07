// Tauri commands exposed to the frontend via invoke().
// Replaces the WebSocket send logic and utility functions from bridge.js/serve.js.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use tauri::{AppHandle, State};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::terminal;
use crate::AppState;

fn hidden_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

// --- App Context (replaces query string params from launcher.ps1) ---

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContext {
    pub cwd: String,
    pub home: String,
    pub host_id: String,
    pub session_id: String,
}

#[tauri::command]
pub fn get_app_context() -> AppContext {
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let session_id = uuid::Uuid::new_v4().to_string();

    // CWD strategy: use HOMEDIR as default so the app doesn't open
    // with the build directory as the project. The user can pick
    // a workspace root from the UI.
    let cwd = home.clone();

    AppContext {
        cwd,
        home,
        host_id: "local".to_string(),
        session_id,
    }
}

// --- Codex Status (solves race condition — invoke is request-response) ---

#[tauri::command]
pub fn get_codex_status(state: State<'_, AppState>) -> String {
    state
        .codex_status
        .lock()
        .map(|s| s.clone())
        .unwrap_or_else(|_| "disconnected".to_string())
}

// --- Send to Codex (replaces WebSocket.send in bridge.js) ---

#[tauri::command]
pub async fn send_to_codex(
    state: State<'_, AppState>,
    message: Value,
) -> Result<(), String> {
    let tx = state
        .codex_ws_tx
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?
        .clone();

    match tx {
        Some(sender) => {
            let json_str = serde_json::to_string(&message)
                .map_err(|e| format!("Serialize error: {}", e))?;
            sender
                .send(json_str)
                .map_err(|e| format!("Send error: {}", e))
        }
        None => Err("Not connected to codex.exe".to_string()),
    }
}

// --- Diagnostics (replaces POST /diag in serve.js) ---

#[tauri::command]
pub fn log_diag(entries: Vec<Value>) {
    for entry in &entries {
        tracing::debug!(target: "bridge", "{}", entry);
    }
}

// --- Native File Dialogs (rfd — Rusty File Dialog) ---
// Use the window handle to ensure dialogs appear in front of the main window.

#[tauri::command]
pub async fn pick_folder(window: tauri::Window) -> Option<String> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Select folder")
        .set_parent(&window);
    let handle = dialog.pick_folder().await;
    handle.map(|h| h.path().to_string_lossy().to_string())
}

#[tauri::command]
pub async fn pick_file(window: tauri::Window) -> Option<String> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Select file")
        .set_parent(&window);
    let handle = dialog.pick_file().await;
    handle.map(|h| h.path().to_string_lossy().to_string())
}

// --- External URL opener (native Windows shell API) ---

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err("URL is empty".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        hidden_command("rundll32")
            .args(["url.dll,FileProtocolHandler", &url])
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("open_external_url is only implemented for Windows".to_string())
}

// --- Terminal Commands (replaces terminal WebSocket in serve.js) ---

#[tauri::command]
pub fn create_terminal(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    cwd: Option<String>,
    shell: Option<String>,
) -> Result<(), String> {
    terminal::create_session(&app, &state, &session_id, cwd, shell)
}

#[tauri::command]
pub fn attach_terminal(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    cwd: Option<String>,
    shell: Option<String>,
) -> Result<(), String> {
    terminal::attach_session(&app, &state, &session_id, cwd, shell)
}

#[tauri::command]
pub fn write_terminal(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    terminal::write_session(&state, &session_id, &data)
}

#[tauri::command]
pub fn resize_terminal(
    _state: State<'_, AppState>,
    _session_id: String,
    _cols: u16,
    _rows: u16,
) -> Result<(), String> {
    // Resize requires PTY support (e.g., conpty or node-pty).
    // For now, this is a no-op — standard process pipes don't support resize.
    Ok(())
}

#[tauri::command]
pub fn detach_terminal(
    _state: State<'_, AppState>,
    _session_id: String,
) -> Result<(), String> {
    // Detach is a client-side concept (stop receiving events).
    // In Tauri, the frontend simply stops listening. No server-side action needed.
    Ok(())
}

#[tauri::command]
pub fn close_terminal(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    terminal::close_session(&state, &session_id)
}

// --- Git Commands (provide real git data to the UI) ---

#[tauri::command]
pub fn git_status(cwd: String) -> Value {
    let path = Path::new(&cwd);
    if !path.exists() {
        return json!({ "isRepo": false });
    }

    // Check if this is a git repository
    let is_repo = hidden_command("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(&cwd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_repo {
        return json!({ "isRepo": false });
    }

    // Get current branch
    let branch = hidden_command("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&cwd)
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
        } else { None });

    // Get uncommitted changes count
    let status_output = hidden_command("git")
        .args(["status", "--porcelain"])
        .current_dir(&cwd)
        .output()
        .ok();

    let (staged, unstaged) = status_output.map(|o| {
        if !o.status.success() { return (0i32, 0i32); }
        let text = String::from_utf8_lossy(&o.stdout);
        let mut s = 0i32;
        let mut u = 0i32;
        for line in text.lines() {
            if line.len() < 2 { continue; }
            let bytes = line.as_bytes();
            if bytes[0] != b' ' && bytes[0] != b'?' { s += 1; }
            if bytes[1] != b' ' { u += 1; }
        }
        (s, u)
    }).unwrap_or((0, 0));

    // Get remote URL
    let remote = hidden_command("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(&cwd)
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
        } else { None });

    json!({
        "isRepo": true,
        "branch": branch,
        "staged": staged,
        "unstaged": unstaged,
        "remoteUrl": remote,
    })
}

#[tauri::command]
pub fn git_origins(cwd: String) -> Value {
    let path = Path::new(&cwd);
    if !path.exists() {
        return json!({ "origins": [], "homeDir": "" });
    }

    // Get the git repo root (--show-toplevel)
    let root = hidden_command("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&cwd)
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            String::from_utf8(o.stdout).ok().map(|s| s.trim().replace('/', "\\"))
        } else { None });

    // Not a git repo
    if root.is_none() {
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        return json!({ "origins": [], "homeDir": home });
    }
    let root = root.unwrap();

    // Get common dir (for worktrees)
    let common_dir = hidden_command("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(&cwd)
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            let raw = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // Resolve relative paths
            if Path::new(&raw).is_absolute() {
                Some(raw.replace('/', "\\"))
            } else {
                Some(Path::new(&cwd).join(&raw)
                    .canonicalize()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| Path::new(&cwd).join(&raw).to_string_lossy().to_string()))
            }
        } else { None })
        .unwrap_or_else(|| format!("{}\\.git", &root));

    // Get origin URL
    let origin_url = hidden_command("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(&cwd)
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
        } else { None });

    let home = std::env::var("USERPROFILE").unwrap_or_default();

    // Build origins in the format the React bundle expects
    let mut origins = Vec::new();
    if let Some(url) = &origin_url {
        origins.push(json!({
            "dir": &cwd,
            "root": &root,
            "originUrl": url,
            "commonDir": &common_dir,
        }));
    }

    // Also include remote names/urls for compatibility
    let remotes_output = hidden_command("git")
        .args(["remote", "-v"])
        .current_dir(&cwd)
        .output()
        .ok();

    let remotes: Vec<Value> = remotes_output.map(|o| {
        if !o.status.success() { return vec![]; }
        let text = String::from_utf8_lossy(&o.stdout);
        let mut seen = std::collections::HashSet::new();
        text.lines().filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && seen.insert(parts[0].to_string()) {
                Some(json!({
                    "name": parts[0],
                    "url": parts[1],
                }))
            } else {
                None
            }
        }).collect()
    }).unwrap_or_default();

    json!({
        "origins": origins,
        "remotes": remotes,
        "root": &root,
        "commonDir": &common_dir,
        "homeDir": home,
    })
}

// --- File system commands ---

#[tauri::command]
pub fn find_files(cwd: String, pattern: Option<String>) -> Value {
    let path = Path::new(&cwd);
    if !path.exists() {
        return json!({ "files": [] });
    }

    // Use git ls-files if in a git repo, otherwise basic listing
    let output = hidden_command("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(&cwd)
        .output()
        .ok();

    let files: Vec<String> = output.map(|o| {
        if !o.status.success() { return vec![]; }
        let text = String::from_utf8_lossy(&o.stdout);
        let pat = pattern.as_deref().unwrap_or("");
        text.lines()
            .filter(|l| !l.is_empty())
            .filter(|l| pat.is_empty() || l.contains(pat))
            .take(500)
            .map(|l| l.to_string())
            .collect()
    }).unwrap_or_default();

    json!({ "files": files })
}

#[tauri::command]
pub fn paths_exist(paths: Vec<String>) -> Value {
    let existing: Vec<String> = paths.into_iter()
        .filter(|p| Path::new(p).exists())
        .collect();
    json!({ "existingPaths": existing })
}

#[tauri::command]
pub fn read_file_contents(path: String) -> Value {
    match std::fs::read_to_string(&path) {
        Ok(contents) => json!({ "contents": contents }),
        Err(_) => json!({ "contents": null }),
    }
}

// --- Open-in-targets detection ---

#[tauri::command]
pub fn detect_open_targets() -> Value {
    let mut targets = Vec::new();
    let mut available_targets = Vec::new();

    // Check for VS Code
    let vscode_available = hidden_command("where.exe")
        .arg("code")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    targets.push(json!({
        "id": "vscode",
        "label": "VS Code",
        "icon": "apps/vscode.png",
        "available": vscode_available,
    }));
    if vscode_available {
        available_targets.push("vscode");
    }

    // File manager is always available on Windows
    targets.push(json!({
        "id": "fileManager",
        "label": "File Explorer",
        "icon": "apps/file-explorer.png",
        "available": true,
    }));
    available_targets.push("fileManager");

    // Terminal (Windows Terminal or cmd.exe — always available)
    // Check for Windows Terminal first (wt.exe), fallback to cmd.exe
    let wt_available = hidden_command("where.exe")
        .arg("wt")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    targets.push(json!({
        "id": "terminal",
        "label": "Terminal",
        "icon": "apps/terminal.png",
        "available": true,
    }));
    available_targets.push("terminal");

    // Check for Git Bash
    let git_bash_available = {
        // Check common Git Bash locations
        let paths_to_check = [
            "C:\\Program Files\\Git\\bin\\bash.exe",
            "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
        ];
        let found_in_path = hidden_command("where.exe")
            .arg("bash")
            .output()
            .map(|o| {
                if o.status.success() {
                    let text = String::from_utf8_lossy(&o.stdout);
                    text.to_lowercase().contains("git")
                } else {
                    false
                }
            })
            .unwrap_or(false);
        found_in_path || paths_to_check.iter().any(|p| Path::new(p).exists())
    };

    if git_bash_available {
        targets.push(json!({
            "id": "gitBash",
            "label": "Git Bash",
            "icon": "apps/terminal.png",
            "available": true,
        }));
        available_targets.push("gitBash");
    }

    // Check for WSL
    let wsl_available = hidden_command("where.exe")
        .arg("wsl")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if wsl_available {
        targets.push(json!({
            "id": "wsl",
            "label": "WSL",
            "icon": "apps/terminal.png",
            "available": true,
        }));
        available_targets.push("wsl");
    }

    // Check for Cursor
    let cursor_available = hidden_command("where.exe")
        .arg("cursor")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if cursor_available {
        targets.push(json!({
            "id": "cursor",
            "label": "Cursor",
            "icon": "apps/cursor.png",
            "available": true,
        }));
        available_targets.push("cursor");
    }

    // Check for Windsurf
    let windsurf_available = hidden_command("where.exe")
        .arg("windsurf")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if windsurf_available {
        targets.push(json!({
            "id": "windsurf",
            "label": "Windsurf",
            "icon": "apps/windsurf.png",
            "available": true,
        }));
        available_targets.push("windsurf");
    }

    let preferred = if available_targets.contains(&"vscode") {
        "vscode"
    } else if available_targets.contains(&"fileManager") {
        "fileManager"
    } else {
        ""
    };

    json!({
        "preferredTarget": preferred,
        "availableTargets": available_targets,
        "targets": targets,
    })
}

// --- GitHub CLI status detection ---

#[tauri::command]
pub fn gh_cli_status() -> Value {
    let is_installed = hidden_command("where.exe")
        .arg("gh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_installed {
        return json!({
            "isInstalled": false,
            "isAuthenticated": false,
        });
    }

    let is_authenticated = hidden_command("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    json!({
        "isInstalled": true,
        "isAuthenticated": is_authenticated,
    })
}

// --- Open in external target ---
// Note: GUI apps (VS Code, Cursor, Windsurf) must NOT use hidden_command()
// because CREATE_NO_WINDOW suppresses the GUI window from appearing.
// Only use hidden_command() for console-only tools (git, where.exe, etc.).

fn gui_command(program: &str) -> Command {
    // GUI apps must spawn normally WITHOUT CREATE_NO_WINDOW
    Command::new(program)
}

#[tauri::command]
pub fn open_in_target(target: String, path: String) -> Result<(), String> {
    match target.as_str() {
        "vscode" => {
            // VS Code is a GUI app — do NOT use hidden_command
            gui_command("code")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Failed to open VS Code: {}", e))?;
        }
        "fileManager" => {
            gui_command("explorer.exe")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Failed to open File Explorer: {}", e))?;
        }
        "terminal" => {
            // Try Windows Terminal first, fallback to cmd.exe
            let wt_available = hidden_command("where.exe")
                .arg("wt")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if wt_available {
                gui_command("wt")
                    .args(["-d", &path])
                    .spawn()
                    .map_err(|e| format!("Failed to open Windows Terminal: {}", e))?;
            } else {
                gui_command("cmd.exe")
                    .args(["/K", &format!("cd /d \"{}\"", path)])
                    .spawn()
                    .map_err(|e| format!("Failed to open cmd.exe: {}", e))?;
            }
        }
        "gitBash" => {
            // Try to find Git Bash
            let git_bash_paths = [
                "C:\\Program Files\\Git\\bin\\bash.exe",
                "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
            ];
            let bash_path = git_bash_paths.iter()
                .find(|p| Path::new(p).exists())
                .map(|p| p.to_string());

            if let Some(bash) = bash_path {
                gui_command(&bash)
                    .args(["--login", "-i"])
                    .current_dir(&path)
                    .spawn()
                    .map_err(|e| format!("Failed to open Git Bash: {}", e))?;
            } else {
                // Fallback: try "bash" from PATH
                gui_command("bash")
                    .args(["--login", "-i"])
                    .current_dir(&path)
                    .spawn()
                    .map_err(|e| format!("Failed to open Git Bash: {}", e))?;
            }
        }
        "wsl" => {
            // Open WSL in the given path (convert Windows path to WSL path)
            gui_command("wsl")
                .args(["--cd", &path])
                .spawn()
                .map_err(|e| format!("Failed to open WSL: {}", e))?;
        }
        "cursor" => {
            gui_command("cursor")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Failed to open Cursor: {}", e))?;
        }
        "windsurf" => {
            gui_command("windsurf")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Failed to open Windsurf: {}", e))?;
        }
        _ => {
            return Err(format!("Unknown target: {}", target));
        }
    }
    Ok(())
}

// --- Native Context Menu (replaces HTML fallback) ---

#[derive(Deserialize)]
pub struct ContextMenuItem {
    pub id: Option<String>,
    pub label: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
}

#[tauri::command]
pub async fn show_native_context_menu(
    window: tauri::Window,
    items: Vec<ContextMenuItem>,
    _x: Option<f64>,
    _y: Option<f64>,
) -> Option<String> {
    use std::sync::{Arc, Mutex as StdMutex};
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tokio::sync::oneshot;

    if items.is_empty() {
        return None;
    }

    let (tx, rx) = oneshot::channel::<Option<String>>();
    let tx = Arc::new(StdMutex::new(Some(tx)));

    let mut menu = MenuBuilder::new(&window);

    for item in &items {
        if item.item_type.as_deref() == Some("separator") {
            menu = menu.separator();
        } else if let Some(label) = &item.label {
            let id = item.id.clone().unwrap_or_default();
            match MenuItemBuilder::new(label)
                .id(id.as_str())
                .build(&window)
            {
                Ok(menu_item) => {
                    menu = menu.item(&menu_item);
                }
                Err(e) => {
                    tracing::warn!("Failed to build menu item: {}", e);
                }
            }
        }
    }

    let built_menu = match menu.build() {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("Failed to build context menu: {}", e);
            return None;
        }
    };

    // Register click handler (fires once via oneshot)
    let tx_clone = tx.clone();
    window.on_menu_event(move |_window, event| {
        let id_str = event.id().0.to_string();
        if let Ok(mut guard) = tx_clone.lock() {
            if let Some(sender) = guard.take() {
                let _ = sender.send(Some(id_str));
            }
        }
    });

    // popup_menu shows at the current cursor position on Windows
    if let Err(e) = window.popup_menu(&built_menu) {
        tracing::warn!("Failed to show popup menu: {}", e);
        return None;
    }

    // Wait for selection with timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        rx,
    ).await;

    match result {
        Ok(Ok(selected)) => selected,
        _ => None,
    }
}

// --- Git Operations (real CLI commands) ---

#[tauri::command]
pub fn git_push(cwd: String, force: Option<bool>) -> Value {
    let path = Path::new(&cwd);
    if !path.exists() {
        return json!({ "success": false, "error": "Directory does not exist" });
    }

    let mut args = vec!["push"];
    if force.unwrap_or(false) {
        args.push("--force-with-lease");
    }

    match hidden_command("git").args(&args).current_dir(&cwd).output() {
        Ok(output) => {
            if output.status.success() {
                json!({ "success": true })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                json!({ "success": false, "error": stderr })
            }
        }
        Err(e) => json!({ "success": false, "error": format!("Failed to run git push: {}", e) }),
    }
}

#[tauri::command]
pub fn git_create_branch(cwd: String, branch: String) -> Value {
    let path = Path::new(&cwd);
    if !path.exists() {
        return json!({ "success": false, "error": "Directory does not exist" });
    }

    match hidden_command("git")
        .args(["checkout", "-b", &branch])
        .current_dir(&cwd)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                json!({ "success": true })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                json!({ "success": false, "error": stderr })
            }
        }
        Err(e) => json!({ "success": false, "error": format!("Failed: {}", e) }),
    }
}

#[tauri::command]
pub fn git_checkout_branch(cwd: String, branch: String) -> Value {
    let path = Path::new(&cwd);
    if !path.exists() {
        return json!({ "success": false, "error": "Directory does not exist" });
    }

    match hidden_command("git")
        .args(["checkout", &branch])
        .current_dir(&cwd)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                json!({ "success": true })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                json!({ "success": false, "error": stderr })
            }
        }
        Err(e) => json!({ "success": false, "error": format!("Failed: {}", e) }),
    }
}

#[tauri::command]
pub fn git_apply_patch(cwd: String, patch: String) -> Value {
    let path = Path::new(&cwd);
    if !path.exists() {
        return json!({ "patchApplied": false, "error": "Directory does not exist" });
    }

    // Write patch to a temp file, then apply
    let tmp_path = std::env::temp_dir().join(format!("codex_patch_{}.patch", uuid::Uuid::new_v4()));
    if let Err(e) = std::fs::write(&tmp_path, &patch) {
        return json!({ "patchApplied": false, "error": format!("Failed to write patch file: {}", e) });
    }

    let result = hidden_command("git")
        .args(["apply", &tmp_path.to_string_lossy()])
        .current_dir(&cwd)
        .output();

    let _ = std::fs::remove_file(&tmp_path);

    match result {
        Ok(output) => {
            if output.status.success() {
                json!({ "patchApplied": true })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                json!({ "patchApplied": false, "error": stderr })
            }
        }
        Err(e) => json!({ "patchApplied": false, "error": format!("Failed: {}", e) }),
    }
}

#[tauri::command]
pub fn git_merge_base(cwd: String, ref1: Option<String>, ref2: Option<String>) -> Value {
    let path = Path::new(&cwd);
    if !path.exists() {
        return json!({ "mergeBaseSha": null });
    }

    let r1 = ref1.unwrap_or_else(|| "HEAD".to_string());
    let r2 = ref2.unwrap_or_else(|| "origin/main".to_string());

    match hidden_command("git")
        .args(["merge-base", &r1, &r2])
        .current_dir(&cwd)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
                json!({ "mergeBaseSha": sha })
            } else {
                json!({ "mergeBaseSha": null })
            }
        }
        Err(_) => json!({ "mergeBaseSha": null }),
    }
}

#[tauri::command]
pub fn gh_pr_create(
    cwd: String,
    title: Option<String>,
    body: Option<String>,
    base: Option<String>,
) -> Value {
    let is_installed = hidden_command("where.exe")
        .arg("gh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_installed {
        return json!({ "success": false, "error": "GitHub CLI (gh) is not installed" });
    }

    let mut args = vec!["pr", "create"];
    if let Some(ref t) = title {
        args.push("--title");
        args.push(t);
    }
    if let Some(ref b) = body {
        args.push("--body");
        args.push(b);
    }
    if let Some(ref base_branch) = base {
        args.push("--base");
        args.push(base_branch);
    }

    match hidden_command("gh").args(&args).current_dir(&cwd).output() {
        Ok(output) => {
            if output.status.success() {
                let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
                json!({ "success": true, "url": url })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                json!({ "success": false, "error": stderr })
            }
        }
        Err(e) => json!({ "success": false, "error": format!("Failed: {}", e) }),
    }
}

#[tauri::command]
pub fn gh_pr_status(cwd: String) -> Value {
    let is_installed = hidden_command("where.exe")
        .arg("gh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_installed {
        return json!({ "prs": [] });
    }

    match hidden_command("gh")
        .args(["pr", "list", "--json", "number,title,state,url,headRefName"])
        .current_dir(&cwd)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                match serde_json::from_str::<Value>(&text) {
                    Ok(prs) => json!({ "prs": prs }),
                    Err(_) => json!({ "prs": [] }),
                }
            } else {
                json!({ "prs": [] })
            }
        }
        Err(_) => json!({ "prs": [] }),
    }
}

// --- File reading (binary as base64) ---

#[tauri::command]
pub fn read_file_binary(path: String) -> Value {
    use std::io::Read;
    let p = Path::new(&path);
    if !p.exists() {
        return json!({ "contentsBase64": null });
    }

    match std::fs::File::open(p) {
        Ok(mut file) => {
            let mut buf = Vec::new();
            match file.read_to_end(&mut buf) {
                Ok(_) => {
                    let encoded = base64_encode(&buf);
                    json!({ "contentsBase64": encoded })
                }
                Err(_) => json!({ "contentsBase64": null }),
            }
        }
        Err(_) => json!({ "contentsBase64": null }),
    }
}

// Simple base64 encoder (no external dependency needed)
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// --- Git read file from specific revision ---

#[tauri::command]
pub fn read_git_file_binary(cwd: String, rev: Option<String>, path: Option<String>) -> Value {
    let dir = Path::new(&cwd);
    if !dir.exists() {
        return json!({ "contentsBase64": null });
    }
    let r = rev.unwrap_or_else(|| "HEAD".to_string());
    let p = match path {
        Some(ref p) => p.as_str(),
        None => return json!({ "contentsBase64": null }),
    };
    let spec = format!("{}:{}", r, p);

    match hidden_command("git")
        .args(["show", &spec])
        .current_dir(&cwd)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let encoded = base64_encode(&output.stdout);
                json!({ "contentsBase64": encoded })
            } else {
                json!({ "contentsBase64": null })
            }
        }
        Err(_) => json!({ "contentsBase64": null }),
    }
}

// --- Pick multiple files ---

#[tauri::command]
pub async fn pick_files(window: tauri::Window) -> Value {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Select files")
        .set_parent(&window);
    let handles = dialog.pick_files().await;
    match handles {
        Some(files) => {
            let paths: Vec<String> = files.iter().map(|h| h.path().to_string_lossy().to_string()).collect();
            json!({ "files": paths })
        }
        None => json!({ "files": [] }),
    }
}
