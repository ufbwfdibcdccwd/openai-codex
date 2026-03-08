---
description: Protocolo MCP — leitura OBRIGATÓRIA para qualquer agente ao iniciar sessão
---

# 🔷 PROTOCOL.md — Codex Tauri MCP

> **Este ficheiro é o ponto de entrada de qualquer agente.** Leia-o por completo antes de fazer QUALQUER alteração ao projeto.

## Boot Sequence

Ao iniciar uma sessão, o agente DEVE:

1. **Ler este ficheiro** (`PROTOCOL.md`)
2. **Ler o estado vivo** → [state.md](file:///C:/Projeto/.agents/state.md)
3. **Verificar a branch ativa** → `git branch --show-current`
4. **Verificar status do git** → `git status --short`
5. **Identificar o papel** → Ler [team.md](file:///C:/Projeto/.agents/workflows/team.md)
6. **Seguir o workflow** → Ver [feature-cycle.md](file:///C:/Projeto/.agents/workflows/feature-cycle.md) ou [debug-cycle.md](file:///C:/Projeto/.agents/workflows/debug-cycle.md)

## Regras de Ouro

> [!CAUTION]
> Violar estas regras pode corromper o projeto. São invioláveis.

1. **NUNCA** commitar diretamente em `main` — sempre via `release/*` ou `hotfix/*`
2. **NUNCA** fazer merge sem compilar (`cargo check` mínimo)
3. **NUNCA** inventar ícones/assets — extrair do app original ou do sistema
4. **NUNCA** modificar o React bundle (`frontend/assets/*.js`) — é código minificado do Codex original
5. **SEMPRE** atualizar `state.md` ao final de cada sessão
6. **SEMPRE** usar convenção de commits: `tipo(scope): descrição`
7. **SEMPRE** criar feature branch antes de implementar

## Mapa de Ficheiros Críticos

| Ficheiro | Função | Quem edita |
|----------|--------|------------|
| `src-tauri/src/main.rs` | Entry point Rust, spawn codex.exe, invoke_handler | 🦀 Backend |
| `src-tauri/src/commands.rs` | Todos os Tauri commands (67+) | 🦀 Backend |
| `src-tauri/src/terminal.rs` | Terminal sessions | 🦀 Backend |
| `src-tauri/src/codex_backend.rs` | WebSocket ao codex.exe | 🦀 Backend |
| `src-tauri/tauri.conf.json` | Config Tauri (janela, CSP, resources) | 🦀 Backend / 🎨 UI |
| `frontend/tauri-bridge.js` | Ponte React ↔ Rust (1866 linhas) | 🌐 Bridge |
| `frontend/index.html` | Entry point HTML | 🎨 UI |
| `frontend/apps/` | Ícones dos targets | 🎨 UI |
| `.agents/state.md` | Estado vivo do projeto | 🎯 Orquestrador |
| `.agents/decisions.md` | Log de decisões | 🎯 Orquestrador |

## Referências Externas

- **App Codex original** (para comparação visual): instalado em `C:\Program Files\WindowsApps\OpenAI.Codex_*`
- **Asar descompactado** (referência de código): `C:\Users\Pedro\.gemini\antigravity\scratch\app_original_unpacked_full\`
- **Repositório GitHub**: `https://github.com/ufbwfdibcdccwd/openai-codex`
- **Upstream OpenAI**: `https://github.com/openai/codex`

## Encerramento de Sessão

Antes de terminar qualquer sessão, o agente DEVE:

1. Commitar alterações pendentes (se compilam)
2. Atualizar `.agents/state.md` com:
   - O que foi feito
   - Bugs encontrados
   - Próximos passos
3. Se houve decisão arquitetónica → documentar em `decisions.md`
4. Push para `origin` se o trabalho está estável
