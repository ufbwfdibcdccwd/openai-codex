---
description: Mapa técnico do projeto — arquitetura e comunicação entre componentes
---

# 🏗️ Arquitetura — Codex Tauri

## Diagrama de Comunicação

```
┌─────────────────────────────────────────────────────────┐
│                    TAURI WINDOW                         │
│  ┌──────────────────────────────────────────────────┐   │
│  │  React Bundle (minificado, read-only)            │   │
│  │  - index-Dm4mOZGo.js (UI principal)              │   │
│  │  - Espera `window.electronBridge`                 │   │
│  │  - Envia mensagens via sendMessageFromView()      │   │
│  └──────────┬───────────────────────────┬───────────┘   │
│             │ window.dispatchEvent       │ electronBridge│
│  ┌──────────▼───────────────────────────▼───────────┐   │
│  │  tauri-bridge.js (1866 linhas, editável)         │   │
│  │  - Implementa window.electronBridge              │   │
│  │  - 67+ fetch handlers (routeFetch)               │   │
│  │  - Traduz React msgs → Tauri invoke              │   │
│  │  - Gere authCache, pendingQueue, terminals       │   │
│  └──────────┬───────────────────────────────────────┘   │
│             │ __TAURI__.core.invoke()                    │
└─────────────┼───────────────────────────────────────────┘
              │ IPC (Tauri v2)
┌─────────────▼───────────────────────────────────────────┐
│  RUST BACKEND (src-tauri/src/)                          │
│  ┌─────────────────┐  ┌──────────────────────────────┐  │
│  │  main.rs         │  │  commands.rs (67+ commands)  │  │
│  │  - spawn codex   │  │  - get_app_context           │  │
│  │  - WebSocket     │  │  - detect_open_targets       │  │
│  │  - AppState      │  │  - open_in_target            │  │
│  │  - invoke_handler│  │  - git_status/origins/push   │  │
│  └────────┬────────┘  │  - check_wsl                  │  │
│           │            │  - create/write/close_terminal│  │
│           │            │  - pick_folder/file           │  │
│           │            │  - show_native_context_menu   │  │
│           │            └──────────────────────────────┘  │
│  ┌────────▼────────┐  ┌──────────────────────────────┐  │
│  │ codex_backend.rs│  │  terminal.rs                  │  │
│  │ - WebSocket     │  │  - PowerShell sessions        │  │
│  │ - JSON-RPC      │  │  - stdin/stdout streaming     │  │
│  │ - Reconnect     │  └──────────────────────────────┘  │
│  └────────┬────────┘                                    │
└───────────┼─────────────────────────────────────────────┘
            │ WebSocket ws://127.0.0.1:5557
┌───────────▼─────────────────────────────────────────────┐
│  codex.exe app-server (vendor binary)                   │
│  - MCP protocol (JSON-RPC 2.0)                          │
│  - Sessões de conversação                               │
│  - Execução de código sandboxed                         │
│  - Integração com OpenAI API                            │
└─────────────────────────────────────────────────────────┘
```

## Fluxo de Mensagens

### React → Backend (pedido do utilizador)
```
React.sendMessageFromView({type:"mcp-request", request:{method:"...", params:{...}}})
  → tauri-bridge.js traduz para JSON-RPC: {jsonrpc:"2.0", id:N, method:"...", params:{...}}
  → invoke("send_to_codex", {message: jsonRpcMsg})
  → Rust envia via WebSocket → codex.exe
```

### Backend → React (resposta do codex)
```
codex.exe envia JSON-RPC via WebSocket
  → Rust recebe e emite evento Tauri: emit("codex-message", data)
  → tauri-bridge.js ouve: listen("codex-message", fn)
  → Despacha para React: window.dispatchEvent(new MessageEvent("message", {data}))
```

### React → Bridge (fetch local)
```
React.sendMessageFromView({type:"fetch", url:"vscode://codex/os-info"})
  → tauri-bridge.js.handleFetch() → routeFetch("os-info")
  → Resposta local (ou invoke para Rust se necessário)
  → toReact({type:"fetch-response", requestId, bodyJsonString})
```

## Tauri Commands Registados

Lista completa dos commands em `main.rs` invoke_handler:

| Command | Ficheiro | Função |
|---------|----------|--------|
| `get_app_context` | commands.rs | CWD, home, hostId, sessionId |
| `get_codex_status` | commands.rs | Estado da conexão ao codex.exe |
| `send_to_codex` | commands.rs | Envia JSON-RPC ao codex.exe |
| `log_diag` | commands.rs | Logging de diagnóstico |
| `open_external_url` | commands.rs | Abre URL no browser |
| `create_terminal` | commands.rs | Cria sessão PowerShell |
| `attach_terminal` | commands.rs | Reattach a terminal existente |
| `write_terminal` | commands.rs | Envia input ao terminal |
| `resize_terminal` | commands.rs | Resize (no-op atualmente) |
| `detach_terminal` | commands.rs | Detach (client-side) |
| `close_terminal` | commands.rs | Fecha sessão de terminal |
| `pick_folder` | commands.rs | Diálogo nativo de pasta |
| `pick_file` | commands.rs | Diálogo nativo de ficheiro |
| `pick_files` | commands.rs | Diálogo multi-ficheiro |
| `git_status` | commands.rs | Branch, staged, unstaged |
| `git_origins` | commands.rs | Remotes, root, commonDir |
| `git_push` | commands.rs | Git push |
| `git_create_branch` | commands.rs | Cria branch |
| `git_checkout_branch` | commands.rs | Checkout branch |
| `git_apply_patch` | commands.rs | Aplica patch |
| `git_merge_base` | commands.rs | Merge base |
| `gh_pr_create` | commands.rs | Cria PR via gh CLI |
| `gh_pr_status` | commands.rs | Status dos PRs |
| `find_files` | commands.rs | Lista ficheiros (git ls-files) |
| `paths_exist` | commands.rs | Verifica existência de paths |
| `read_file_contents` | commands.rs | Lê ficheiro como texto |
| `read_file_binary` | commands.rs | Lê ficheiro como base64 |
| `read_git_file_binary` | commands.rs | Lê ficheiro de commit git |
| `detect_open_targets` | commands.rs | Detecta VS Code, Terminal, etc. |
| `gh_cli_status` | commands.rs | GitHub CLI instalado/autenticado |
| `check_wsl` | commands.rs | WSL instalado + distros |
| `open_in_target` | commands.rs | Abre app externo (VS Code, etc.) |
| `show_native_context_menu` | commands.rs | Menu de contexto OS nativo |

## CSP (Content Security Policy)

Definida em `frontend/index.html`:
```
default-src 'none';
img-src 'self' blob: data: https:;
script-src 'self' '<hash>' 'wasm-unsafe-eval';
style-src 'self' 'unsafe-inline';
font-src 'self' data:;
connect-src 'self' https://ab.chatgpt.com https://cdn.openai.com;
```

`dangerousDisableAssetCspModification: true` em `tauri.conf.json` para evitar que o Tauri modifique a CSP.
