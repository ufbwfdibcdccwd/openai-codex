---
description: Estado vivo do projeto — atualizado a cada sessão
---

# 📊 Estado do Projeto — Codex Tauri

> **Última atualização:** 2026-03-08T11:13Z
> **Último agente:** 🎯 Orquestrador
> **Branch ativa:** `develop`

## Git

- **Branch atual:** `develop`
- **Último commit:** `548809a29` — chore: setup gitflow, team structure, workflows, cleanup project
- **Working tree:** Limpo
- **Remotes:**
  - `origin` → `github.com/ufbwfdibcdccwd/openai-codex` (nosso fork)
  - `upstream` → `github.com/openai/codex` (OpenAI)

## Tarefas

### 🔧 Em Progresso (Sprint Atual)
- [ ] **Titlebar custom dark** — Eliminar a barra branca nativa, implementar titlebar HTML integrado com tema dark do Codex. Agentes: 🌐 Bridge + 🎨 UI
- [ ] **Terminal label** — O terminal mostra o path completo `C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe` em vez de apenas "PowerShell". Agente: 🌐 Bridge
- [ ] **Open-in targets** — VS Code / Git Bash / WSL precisam ser testados com as novas funções `find_vscode`, `find_git_bash`. Agente: 🧪 QA

### 📋 Backlog (Próximo Sprint)
- [ ] Painel de alterações de ficheiros (lado direito da UI)
- [ ] Indicador git (+N -N) no topo
- [ ] Branch "main ▾" no rodapé
- [ ] "Acesso completo ✓" vs "Personalizado (config.toml)"
- [ ] Botão "Commit ▾" ao lado de "Aberto ▾"
- [ ] Threads de outros projetos na sidebar
- [ ] Build release (`cargo tauri build`)
- [ ] Testes de performance
- [ ] Auto-update mechanism

### ✅ Concluído
- [x] Extração da UI do Codex original (app.asar)
- [x] Ponte Tauri básica (`tauri-bridge.js`, 1866 linhas)
- [x] Backend Rust (spawn codex.exe, WebSocket connection)
- [x] Terminal integrado
- [x] Detecção de open-in targets com ícones
- [x] Git commands (status, origins, push, branch, checkout, merge-base)
- [x] Context menu nativo
- [x] Autenticação OAuth flow (ChatGPT + API key)
- [x] Ícones extraídos (Terminal, Git Bash, WSL)
- [x] Detecção dinâmica WSL via `check_wsl` Rust command
- [x] Paths absolutos para VS Code/Git Bash (`find_vscode`, `find_git_bash`)
- [x] Gitflow implementado (main, develop, feature/*)
- [x] Ecossistema MCP implementado

## Bugs Conhecidos
- **Barra branca** — `decorations: false` remove o titlebar nativo mas a UI React não renderiza um titlebar custom (o original usa Electron `titleBarStyle: hidden`). Precisa de solução HTML/CSS custom.
- **Terminal label** — React mostra o shellPath em vez de um label limpo. Provavelmente a informação enviada pelo Rust no `terminal-init-log` inclui o path completo.

## Notas para Próximo Agente
- O React bundle é minificado e não deve ser editado
- A variável CSS `--codex-titlebar-tint` já existe no bundle — pode ser usada para colorir o titlebar
- O ficheiro `main.js` do app original Electron está em `app_original_unpacked_full/.vite/build/main.js` para referência
- Os vendor binaries (`codex.exe`, `rg.exe`) estão em `src-tauri/vendor/x86_64-pc-windows-msvc/`
