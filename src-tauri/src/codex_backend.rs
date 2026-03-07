// WebSocket client that connects to codex.exe and translates JSON-RPC messages.
// Replaces the WebSocket logic from bridge.js.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::AppState;

const RECONNECT_DELAY_MS: u64 = 2000;
const INIT_REQUEST_ID: &str = "__bridge_init__";
const INTERNAL_REQUEST_PREFIX: &str = "__bridge_internal__";

/// Updates the codex status in AppState and emits an event to the frontend.
fn set_status(app: &AppHandle, status: &str) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut s) = state.codex_status.lock() {
            *s = status.to_string();
        }
    }
    let _ = app.emit("codex-status-changed", status);
    tracing::info!("codex status: {}", status);
}

/// Translates a JSON-RPC message from codex.exe into the React internal format
/// and emits it as a Tauri event.
fn dispatch_to_frontend(app: &AppHandle, data: &Value) {
    // Response: has id + (result or error)
    if data.get("id").is_some()
        && (data.get("result").is_some() || data.get("error").is_some())
    {
        // Skip the initialize response — handled internally
        if let Some(id_str) = data.get("id").and_then(|v| v.as_str()) {
            if id_str == INIT_REQUEST_ID || id_str.starts_with(INTERNAL_REQUEST_PREFIX) {
                return;
            }
        }

        let mut message = json!({ "id": data["id"] });
        if let Some(err) = data.get("error") {
            message["error"] = err.clone();
        } else if let Some(result) = data.get("result") {
            message["result"] = result.clone();
        }

        let payload = json!({
            "type": "mcp-response",
            "hostId": "local",
            "message": message,
        });
        let _ = app.emit("codex-message", &payload);
        return;
    }

    // Server request: has method + id
    if data.get("method").is_some() && data.get("id").is_some() {
        let payload = json!({
            "type": "mcp-request",
            "hostId": "local",
            "request": {
                "id": data["id"],
                "method": data["method"],
                "params": data.get("params").unwrap_or(&json!({})),
            },
        });
        let _ = app.emit("codex-message", &payload);
        return;
    }

    // Notification: has method, no id
    if data.get("method").is_some() && data.get("id").is_none() {
        let payload = json!({
            "type": "mcp-notification",
            "hostId": "local",
            "method": data["method"],
            "params": data.get("params").unwrap_or(&json!({})),
        });
        let _ = app.emit("codex-message", &payload);
        return;
    }

    // Unknown — forward as-is
    let _ = app.emit("codex-message", data);
}

/// Main connection loop. Connects to codex.exe via WebSocket, sends initialize,
/// and routes messages. Reconnects automatically on disconnection.
pub async fn run_codex_connection(app: AppHandle, ws_port: u16) {
    let url = format!("ws://127.0.0.1:{}", ws_port);

    loop {
        set_status(&app, "connecting");
        tracing::info!("Connecting to codex.exe at {}", url);

        match connect_async(&url).await {
            Ok((ws_stream, _)) => {
                tracing::info!("WebSocket connected to codex.exe");

                let (mut ws_write, mut ws_read) = ws_stream.split();

                // Send initialize request
                let init_msg = json!({
                    "jsonrpc": "2.0",
                    "id": INIT_REQUEST_ID,
                    "method": "initialize",
                    "params": {
                        "clientInfo": {
                            "name": "codex-windows",
                            "version": "1.0.0",
                            "title": "Codex Windows",
                        },
                        "capabilities": {
                            "experimentalApi": true,
                        },
                    },
                });

                if let Err(e) = ws_write
                    .send(Message::Text(init_msg.to_string()))
                    .await
                {
                    tracing::error!("Failed to send initialize: {}", e);
                    set_status(&app, "disconnected");
                    sleep(Duration::from_millis(RECONNECT_DELAY_MS)).await;
                    continue;
                }

                // Create channel for sending messages from commands to the WS writer
                let (tx, mut rx) = mpsc::unbounded_channel::<String>();

                // Store tx in AppState so commands can send messages
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut ws_tx) = state.codex_ws_tx.lock() {
                        *ws_tx = Some(tx);
                    }
                }

                let mut initialized = false;

                // Process messages in both directions
                loop {
                    tokio::select! {
                        // Messages FROM codex.exe
                        msg = ws_read.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    match serde_json::from_str::<Value>(&text) {
                                        Ok(data) => {
                                            // Handle initialize response
                                            if !initialized {
                                                if data.get("id").and_then(|v| v.as_str()) == Some(INIT_REQUEST_ID) {
                                                    if data.get("error").is_some() {
                                                        tracing::error!("Initialize failed: {}", data["error"]);
                                                        set_status(&app, "disconnected");
                                                        break;
                                                    }
                                                    tracing::info!("Initialize OK");
                                                    initialized = true;
                                                    set_status(&app, "connected");

                                                    // Emit initialized event
                                                    let _ = app.emit("codex-message", &json!({
                                                        "type": "codex-app-server-initialized",
                                                    }));

                                                    continue;
                                                }
                                            }

                                            // Route all other messages
                                            dispatch_to_frontend(&app, &data);
                                        }
                                        Err(e) => {
                                            tracing::warn!("Non-JSON message from codex: {}", e);
                                        }
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    tracing::warn!("WebSocket closed by codex.exe");
                                    break;
                                }
                                Some(Err(e)) => {
                                    tracing::error!("WebSocket error: {}", e);
                                    break;
                                }
                                _ => {} // Ping/Pong/Binary — ignore
                            }
                        }

                        // Messages TO codex.exe (from Tauri commands)
                        Some(msg) = rx.recv() => {
                            if let Err(e) = ws_write.send(Message::Text(msg)).await {
                                tracing::error!("Failed to send to codex.exe: {}", e);
                                break;
                            }
                        }
                    }
                }

                // Clear tx on disconnect
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut ws_tx) = state.codex_ws_tx.lock() {
                        *ws_tx = None;
                    }
                }

                set_status(&app, "disconnected");
            }
            Err(e) => {
                tracing::warn!("Failed to connect to codex.exe: {}", e);
                set_status(&app, "disconnected");
            }
        }

        tracing::info!(
            "Reconnecting in {}ms...",
            RECONNECT_DELAY_MS
        );
        sleep(Duration::from_millis(RECONNECT_DELAY_MS)).await;
    }
}
