---
description: Gitflow workflow — branching, merging, and release process
---

# Gitflow Workflow

## Branch Structure

| Branch | Propósito | Merges From | Merges Into |
|--------|-----------|-------------|-------------|
| `main` | Produção estável — sempre compilável | `release/*`, `hotfix/*` | — |
| `develop` | Integração contínua — todas as features | `feature/*` | `release/*` |
| `feature/*` | Desenvolvimento de funcionalidades | `develop` | `develop` |
| `release/*` | Preparação de versão (freeze + bugs) | `develop` | `main` + `develop` |
| `hotfix/*` | Correções urgentes em produção | `main` | `main` + `develop` |

## Criar Feature Branch

```bash
# Partir sempre de develop
git checkout develop
git pull origin develop
git checkout -b feature/<nome-descritivo>
```

## Fechar Feature Branch

```bash
git checkout develop
git merge --no-ff feature/<nome>
git branch -d feature/<nome>
git push origin develop
```

## Criar Release

```bash
git checkout develop
git checkout -b release/v<X.Y.Z>
# Fazer testes finais + bump version
git checkout main
git merge --no-ff release/v<X.Y.Z>
git tag -a v<X.Y.Z> -m "Release v<X.Y.Z>"
git checkout develop
git merge --no-ff release/v<X.Y.Z>
git branch -d release/v<X.Y.Z>
git push origin main develop --tags
```

## Criar Hotfix

```bash
git checkout main
git checkout -b hotfix/<nome>
# Fix + commit
git checkout main
git merge --no-ff hotfix/<nome>
git tag -a v<X.Y.Z+1> -m "Hotfix"
git checkout develop
git merge --no-ff hotfix/<nome>
git branch -d hotfix/<nome>
git push origin main develop --tags
```

## Convenção de Commits

Formato: `<tipo>(<scope>): <descrição>`

| Tipo | Uso |
|------|-----|
| `feat` | Nova funcionalidade |
| `fix` | Correção de bug |
| `refactor` | Refactoring sem alterar comportamento |
| `style` | Formatação, CSS, UI visual |
| `perf` | Melhoria de performance |
| `docs` | Documentação |
| `chore` | Build, CI, configs |
| `test` | Testes |

Exemplos:
- `feat(bridge): add dynamic WSL detection`
- `fix(titlebar): implement custom dark titlebar`
- `refactor(commands): use absolute paths for open-in targets`
