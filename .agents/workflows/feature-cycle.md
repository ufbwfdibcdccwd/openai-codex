---
description: Workflow completo para implementar uma feature — passo a passo
---

# 🔄 Feature Cycle — Workflow de Implementação

## Pré-requisitos

1. Ler [PROTOCOL.md](file:///C:/Projeto/.agents/PROTOCOL.md)
2. Ler [state.md](file:///C:/Projeto/.agents/state.md)
3. Confirmar que estás em `develop`: `git branch --show-current`
4. Working tree limpa: `git status --short`

## Passos

### 1. Criar Feature Branch

```bash
git checkout develop
git pull origin develop
git checkout -b feature/<agente>-<nome-descritivo>
```

Exemplos:
- `feature/backend-titlebar-commands`
- `feature/bridge-terminal-label`
- `feature/ui-titlebar-html`

### 2. Implementar

Seguir o checklist do agente responsável (ver [team.md](file:///C:/Projeto/.agents/workflows/team.md)):

| Agente | Ficheiros |
|--------|-----------|
| 🦀 Backend | `src-tauri/src/*.rs`, `tauri.conf.json` |
| 🌐 Bridge | `frontend/tauri-bridge.js` |
| 🎨 UI | `frontend/index.html`, `frontend/apps/`, CSS |

### 3. Verificar Compilação

// turbo
```bash
cargo check 2>&1
```

Se houver erros → corrigir antes de commitar.

### 4. Commitar

```bash
git add -A
git commit -m "tipo(scope): descrição concisa"
```

### 5. Testar (se possível)

// turbo
```bash
cargo run 2>&1
```

- Abrir o app e verificar a feature
- Comparar visualmente com o app original se for UI

### 6. Merge em Develop

```bash
git checkout develop
git merge --no-ff feature/<nome>
git branch -d feature/<nome>
```

### 7. Atualizar Estado

Editar `.agents/state.md`:
- Mover tarefa de "Em Progresso" para "Concluído"
- Adicionar notas se necessário
- Atualizar "Último commit" e "Última atualização"

### 8. Push

```bash
git push origin develop
```

## Regras

- **Uma feature = uma branch** — não misturar funcionalidades
- **Commits atómicos** — cada commit deve compilar
- **Se a feature é cross-agent** (ex: titlebar precisa de Backend + Bridge + UI):
  - O Orquestrador cria a branch
  - Cada agente commita a sua parte na mesma branch
  - Merge só quando tudo está integrado
