---
description: Equipa de desenvolvimento e orquestração do projeto Codex Tauri
---

# Equipa de Desenvolvimento — Codex Tauri

## Visão Geral

Equipa de 5 agentes especializados com orquestração centralizada. Cada agente tem responsabilidades definidas e opera dentro do Gitflow.

## Agentes

### 🎯 ORQUESTRADOR (Lead)
**Responsabilidade:** Coordena todo o workflow. Prioriza tarefas, cria feature branches, revisa merges, mantém a qualidade.

- Decide que features entram em cada sprint
- Cria e fecha branches (feature, release, hotfix)
- Faz merge reviews antes de integrar em `develop`
- Mantém o backlog atualizado
- Garante que cada agente segue o padrão do projeto

**Regras:**
- Nunca commita diretamente em `main` ou `develop`
- Sempre usa `--no-ff` nos merges
- Valida compilação antes de qualquer merge

---

### 🦀 RUST BACKEND (Backend Engineer)
**Responsabilidade:** Todo o código Rust — `src-tauri/src/main.rs`, `commands.rs`, `terminal.rs`, `codex_backend.rs`.

- Implementa novos Tauri commands
- Corrige bugs no backend (spawn, WebSocket, terminal)
- Otimiza performance (async, process management)
- Mantém a ponte Rust ↔ codex.exe

**Branch pattern:** `feature/backend-*`, `fix/backend-*`

**Checklist antes de commit:**
1. `cargo check` passa sem erros
2. `cargo clippy` sem warnings críticos
3. Novos commands registados em `main.rs` invoke_handler

---

### 🌐 BRIDGE ENGINEER (Integration)
**Responsabilidade:** O ficheiro `frontend/tauri-bridge.js` — a ponte entre o React bundle e o Rust backend.

- Implementa handlers de fetch (routeFetch)
- Traduz mensagens React ↔ JSON-RPC ↔ Rust
- Gere o estado de conexão (codexConnected, pendingQueue)
- Mantém a autenticação (authCache, OAuth flow)
- Implementa o titlebar custom e integrações de UI

**Branch pattern:** `feature/bridge-*`, `fix/bridge-*`

**Checklist antes de commit:**
1. Nenhum `require()` ou `import` (é vanilla JS IIFE)
2. Todos os invokes correspondem a commands registados no Rust
3. Console logs com prefixo `[bridge]`

---

### 🎨 UI/UX ENGINEER (Frontend)
**Responsabilidade:** HTML, CSS, assets visuais, comparação com o app original.

- Mantém `index.html` e CSS customizado
- Gere ícones em `frontend/apps/`
- Compara visualmente com o Codex original
- Implementa o titlebar HTML custom
- Resolve problemas de CSP (Content Security Policy)

**Branch pattern:** `feature/ui-*`, `fix/ui-*`

**Checklist antes de commit:**
1. Ícones são .png extraídos (não inventados)
2. CSS usa variáveis do tema do React bundle
3. Testado em dark mode

---

### 🧪 QA & RELEASE (Quality + Deploy)
**Responsabilidade:** Testes, builds, releases, e comparação funcional.

- Executa `cargo run` e testa todas as funcionalidades
- Compara side-by-side com o app original
- Executa `cargo tauri build` para releases
- Gere tags de versão e changelogs
- Reporta bugs e cria issues

**Branch pattern:** `release/*`, `hotfix/*`

**Checklist de release:**
1. Todos os botões "Open in" funcionam
2. Terminal mostra label correto
3. Titlebar integrado (sem barra branca)
4. WebSocket ao codex.exe conecta e mantém
5. Autenticação OAuth funciona
6. Build release compila sem erros

---

## Workflow de Desenvolvimento

```
1. ORQUESTRADOR identifica tarefa prioritária
2. ORQUESTRADOR cria feature branch a partir de develop
3. Agente responsável implementa (RUST/BRIDGE/UI)
4. Agente commita com convenção de commits
5. ORQUESTRADOR revisa e merge em develop (--no-ff)
6. QA testa em develop
7. Quando pronto → ORQUESTRADOR cria release branch
8. QA valida release → merge em main + tag
```

## Estado Atual do Projeto

### ✅ Concluído
- [x] Extração da UI do Codex original (app.asar)
- [x] Ponte Tauri básica (tauri-bridge.js)
- [x] Backend Rust (spawn codex.exe, WebSocket)
- [x] Terminal integrado
- [x] Detecção de open-in targets
- [x] Git status/origins/push commands
- [x] Contexto menu nativo
- [x] Autenticação OAuth flow
- [x] Ícones extraídos (Terminal, Git Bash, WSL)
- [x] Detecção dinâmica de WSL
- [x] Paths absolutos para VS Code/Git Bash

### 🔧 Em Progresso
- [ ] Titlebar custom dark (sem barra branca)
- [ ] Terminal label "PowerShell" (em vez do path completo)
- [ ] VS Code / Git Bash / WSL — testar se realmente abre

### 📋 Backlog
- [ ] Painel de alterações de ficheiros (lado direito)
- [ ] Indicador git (+981 -0)
- [ ] Branch "main ▾" no rodapé
- [ ] "Acesso completo ✓" vs "Personalizado"
- [ ] Botão "Commit ▾"
- [ ] Threads de outros projetos
- [ ] Build release (cargo tauri build)
- [ ] Testes de performance
- [ ] Auto-update mechanism
