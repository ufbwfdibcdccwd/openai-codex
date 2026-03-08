---
description: Workflow para investigar e corrigir bugs
---

# 🐛 Debug Cycle — Workflow de Investigação

## Quando Usar

- Funcionalidade que não funciona (ex: VS Code não abre)
- Diferença visual entre o nosso app e o original
- Erros no console do DevTools ou nos logs do Rust
- Crash ou comportamento inesperado

## Passos

### 1. Reproduzir o Problema

```bash
# Compilar e abrir o app
cargo run 2>&1
```

- Descrever exatamente o que acontece vs o que deveria acontecer
- Screenshot se for visual
- Abrir o app original lado a lado para comparar

### 2. Investigar

#### Se é um problema de UI:
- F12 no app → DevTools → Console (verificar erros JS)
- Comparar HTML/CSS com o app original
- Verificar se o React bundle está a receber dados corretos

#### Se é um problema de Backend (Rust):
- Verificar logs: `C:\Users\Pedro\AppData\Local\com.codex.windows\logs\`
- Adicionar `tracing::info!()` temporários
- Verificar se o command está registado em `main.rs` invoke_handler

#### Se é um problema de Bridge:
- Console logs com `[bridge]` prefix
- Verificar se o handler existe em `routeFetch`
- Verificar se o invoke corresponde a um command Rust

#### Se é um problema de comunicação com codex.exe:
- Verificar se `codex.exe` está a correr: `tasklist | findstr codex`
- Verificar WebSocket logs em `codex_backend.rs`
- Verificar se `CODEX_HOME` e `OPENAI_API_KEY` estão corretos

### 3. Consultar Referência

- **App original** (comparação visual): abrir o Codex original
- **Código do Electron** (lógica): `app_original_unpacked_full/.vite/build/main.js`
- **Decisões anteriores**: `.agents/decisions.md`

### 4. Corrigir

```bash
git checkout develop
git checkout -b fix/<scope>-<descrição>
# Implementar fix
cargo check 2>&1
git add -A
git commit -m "fix(scope): descrição do fix"
```

### 5. Verificar

```bash
cargo run 2>&1
```
- Confirmar que o bug está resolvido
- Confirmar que não introduziu regressões

### 6. Fechar

```bash
git checkout develop
git merge --no-ff fix/<nome>
git branch -d fix/<nome>
```

### 7. Documentar

- Atualizar `.agents/state.md` (remover bug da lista)
- Se foi uma decisão importante → adicionar em `.agents/decisions.md`
