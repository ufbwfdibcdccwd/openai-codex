---
description: Quick start — como abrir e trabalhar no projeto
---

# Quick Start — Codex Tauri

## Localização
O projeto está em `C:\Projeto`.

## Abrir no Antigravity
Abre o Antigravity na pasta `C:\Projeto`.

## Estrutura do Projeto

```
C:\Projeto/
├── .agents/workflows/     ← Workflows do Antigravity (gitflow, team, etc.)
├── frontend/              ← UI extraída do Codex original
│   ├── index.html         ← Entry point HTML
│   ├── tauri-bridge.js    ← Ponte React ↔ Tauri (CRÍTICO)
│   ├── assets/            ← JS/CSS do React bundle (minificado)
│   └── apps/              ← Ícones dos targets (vscode.png, etc.)
├── src-tauri/             ← Backend Rust + Tauri
│   ├── src/main.rs        ← Entry point + spawn codex.exe
│   ├── src/commands.rs    ← Tauri commands (67+)
│   ├── src/terminal.rs    ← Terminal sessions
│   ├── src/codex_backend.rs ← WebSocket ao codex.exe
│   ├── tauri.conf.json    ← Configuração Tauri
│   └── vendor/            ← codex.exe + rg.exe bundled
├── codex-cli/             ← Código fonte do Codex CLI (upstream)
├── codex-rs/              ← Código fonte Rust (upstream)
└── .git/                  ← Controle de versão
```

## Compilar e Executar

// turbo
```bash
cd C:\Projeto\src-tauri && cargo run 2>&1
```

## Branches

- `main` — Produção estável
- `develop` — Integração (trabalho ativo)
- `feature/*` — Features individuais

## Remotes

- `origin` — https://github.com/ufbwfdibcdccwd/openai-codex.git (nosso fork)
- `upstream` — https://github.com/openai/codex (OpenAI original)

## Referência

O app original do Codex Windows está instalado em:
`C:\Program Files\WindowsApps\OpenAI.Codex_*`

Os ficheros de referência (asar descompactado) estão em:
`C:\Users\Pedro\.gemini\antigravity\scratch\app_original_unpacked_full\`
