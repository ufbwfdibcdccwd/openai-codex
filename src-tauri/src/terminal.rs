// Terminal session management — replaces serve.js terminal WebSocket.
// Spawns shell processes and streams stdout/stderr to the frontend via Tauri events.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::AppState;

pub struct TerminalSession {
    pub process: Child,
    pub cwd: String,
    pub shell: String,
}

impl TerminalSession {
    pub fn kill(&mut self) {
        let _ = self.process.kill();
    }
}

fn default_shell() -> String {
    // Prefer PowerShell (pwsh.exe = PowerShell 7+, powershell.exe = Windows PowerShell 5.1)
    // Fall back to cmd.exe only if neither is available.
    let mut where_cmd = std::process::Command::new("where.exe");
    where_cmd.arg("pwsh.exe");
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        where_cmd.creation_flags(CREATE_NO_WINDOW);
    }
    if let Ok(output) = where_cmd.output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().lines().next().unwrap_or("").to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }
    // Windows PowerShell is always at a known path
    let winps = std::env::var("SystemRoot")
        .unwrap_or_else(|_| "C:\\Windows".to_string())
        + "\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
    if std::path::Path::new(&winps).exists() {
        return winps;
    }
    std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string())
}

fn normalize_cwd(cwd: Option<String>) -> String {
    if let Some(ref c) = cwd {
        if !c.is_empty() {
            let path = std::path::Path::new(c);
            if path.is_dir() {
                return c.clone();
            }
        }
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

/// Spawns a shell process and starts reading stdout/stderr in background threads.
/// Emits terminal-data events for output, and terminal-exit when both streams close.
fn spawn_and_stream(
    app: &AppHandle,
    session_id: &str,
    cwd: Option<String>,
    shell: Option<String>,
) -> Result<TerminalSession, String> {
    let shell_cmd = shell.unwrap_or_else(default_shell);
    let work_dir = normalize_cwd(cwd);

    let mut cmd = Command::new(&shell_cmd);
    cmd.current_dir(&work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Prevent an external PowerShell/CMD window from popping up.
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn shell '{}': {}", shell_cmd, e))?;

    // Track when both stdout and stderr are done (EOF)
    let stdout_done = Arc::new(AtomicBool::new(false));
    let stderr_done = Arc::new(AtomicBool::new(false));

    // Read stdout in background thread
    if let Some(stdout) = child.stdout.take() {
        let app_clone = app.clone();
        let sid = session_id.to_string();
        let done_flag = stdout_done.clone();
        let other_done = stderr_done.clone();
        let exit_app = app.clone();
        let exit_sid = session_id.to_string();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut reader = std::io::BufReader::new(stdout);
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app_clone.emit(
                            "terminal-data",
                            json!({
                                "type": "terminal-data",
                                "sessionId": sid,
                                "data": text,
                            }),
                        );
                    }
                    Err(_) => break,
                }
            }
            done_flag.store(true, Ordering::SeqCst);
            // If both streams are done, emit exit event
            if other_done.load(Ordering::SeqCst) {
                let _ = exit_app.emit(
                    "terminal-exit",
                    json!({
                        "type": "terminal-exit",
                        "sessionId": exit_sid,
                        "code": null,
                        "signal": null,
                    }),
                );
            }
        });
    } else {
        stdout_done.store(true, Ordering::SeqCst);
    }

    // Read stderr in background thread
    if let Some(stderr) = child.stderr.take() {
        let app_clone = app.clone();
        let sid = session_id.to_string();
        let done_flag = stderr_done.clone();
        let other_done = stdout_done.clone();
        let exit_app = app.clone();
        let exit_sid = session_id.to_string();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut reader = std::io::BufReader::new(stderr);
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app_clone.emit(
                            "terminal-data",
                            json!({
                                "type": "terminal-data",
                                "sessionId": sid,
                                "data": text,
                            }),
                        );
                    }
                    Err(_) => break,
                }
            }
            done_flag.store(true, Ordering::SeqCst);
            // If both streams are done, emit exit event
            if other_done.load(Ordering::SeqCst) {
                let _ = exit_app.emit(
                    "terminal-exit",
                    json!({
                        "type": "terminal-exit",
                        "sessionId": exit_sid,
                        "code": null,
                        "signal": null,
                    }),
                );
            }
        });
    } else {
        stderr_done.store(true, Ordering::SeqCst);
    }

    let session = TerminalSession {
        process: child,
        cwd: work_dir,
        shell: shell_cmd,
    };

    Ok(session)
}

pub fn create_session(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    cwd: Option<String>,
    shell: Option<String>,
) -> Result<(), String> {
    let mut sessions = state
        .terminal_sessions
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;

    if sessions.contains_key(session_id) {
        return Ok(());
    }

    let session = spawn_and_stream(app, session_id, cwd, shell)?;
    let log_msg = format!("created cwd={} shell={}", session.cwd, session.shell);

    let _ = app.emit(
        "terminal-init-log",
        json!({
            "type": "terminal-init-log",
            "sessionId": session_id,
            "log": log_msg,
        }),
    );

    sessions.insert(session_id.to_string(), session);
    Ok(())
}

pub fn attach_session(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    cwd: Option<String>,
    shell: Option<String>,
) -> Result<(), String> {
    {
        let sessions = state
            .terminal_sessions
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;

        if let Some(session) = sessions.get(session_id) {
            let _ = app.emit(
                "terminal-attached",
                json!({
                    "type": "terminal-attached",
                    "sessionId": session_id,
                    "cwd": session.cwd,
                    "shell": session.shell,
                }),
            );
            return Ok(());
        }
    }

    // Session doesn't exist — create it, then emit attached
    create_session(app, state, session_id, cwd, shell)?;

    let sessions = state
        .terminal_sessions
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;

    if let Some(session) = sessions.get(session_id) {
        let _ = app.emit(
            "terminal-attached",
            json!({
                "type": "terminal-attached",
                "sessionId": session_id,
                "cwd": session.cwd,
                "shell": session.shell,
            }),
        );
    }

    Ok(())
}

pub fn write_session(state: &AppState, session_id: &str, data: &str) -> Result<(), String> {
    let mut sessions = state
        .terminal_sessions
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;

    let session = sessions
        .get_mut(session_id)
        .ok_or_else(|| format!("Terminal session '{}' not found", session_id))?;

    if let Some(ref mut stdin) = session.process.stdin {
        stdin
            .write_all(data.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
        stdin.flush().map_err(|e| format!("Flush error: {}", e))?;
    } else {
        return Err("Terminal stdin not available".to_string());
    }

    Ok(())
}

pub fn close_session(state: &AppState, session_id: &str) -> Result<(), String> {
    let mut sessions = state
        .terminal_sessions
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;

    if let Some(mut session) = sessions.remove(session_id) {
        session.kill();
        tracing::info!("Terminal session '{}' closed", session_id);
    }

    Ok(())
}
