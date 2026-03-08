---
description: Log de decisões arquitetónicas (ADR)
---

# 📝 Decisões Arquitetónicas

## ADR-001: Usar Tauri em vez de Electron
- **Data:** 2026-03-07
- **Contexto:** O Codex Windows original usa Electron. Queremos uma versão portátil mais leve.
- **Decisão:** Usar Tauri v2 com Rust backend. O React bundle do Electron é reaproveitado como-está, a ponte `electronBridge` é reimplementada em vanilla JS para chamar APIs Tauri.
- **Consequência:** Zero dependência de Electron runtime. App mais leve (~10MB vs ~200MB). Precisa de ponte (tauri-bridge.js) para traduzir a interface.

## ADR-002: Extrair UI do app original em vez de recriar
- **Data:** 2026-03-07
- **Contexto:** Tentámos recriar a UI do zero e também usar o build do macOS. Nenhum deu resultado correto.
- **Decisão:** Extrair o `app.asar` do Codex Windows original instalado no sistema e usar diretamente o frontend dele.
- **Consequência:** UI 100% idêntica ao original. Código minificado (não editável). Todas as customizações precisam ser feitas na camada bridge ou via HTML/CSS injectado.

## ADR-003: `decorations: false` no Tauri
- **Data:** 2026-03-08
- **Contexto:** Com `decorations: true` aparece uma barra branca nativa do Windows que não existe no app original. O original usa Electron `titleBarStyle: hidden`.
- **Decisão:** Usar `decorations: false` e implementar titlebar custom em HTML para ter minimize/maximize/close.
- **Consequência:** Precisa de implementação HTML do titlebar com `-webkit-app-region: drag` e chamadas a `window.__TAURI__.window` para os botões.

## ADR-004: Paths absolutos para open-in targets
- **Data:** 2026-03-08
- **Contexto:** `gui_command("code")` falhava porque o PATH não estava disponível quando o app era lançado como Admin ou via Start Menu.
- **Decisão:** Implementar `find_vscode()`, `find_git_bash()`, `find_executable()` que resolvem paths absolutos primeiro e usam `where.exe` como fallback.
- **Consequência:** VS Code, Git Bash e WSL devem funcionar independentemente de como o app é lançado.

## ADR-005: CODEX_HOME partilhado (sem isolamento)
- **Data:** 2026-03-07
- **Contexto:** Inicialmente usámos um CODEX_HOME isolado para evitar conflitos de WAL locks do SQLite com outras instâncias do Codex.
- **Decisão:** Remover o isolamento e usar `~/.codex` diretamente para partilhar sessões e worktrees com o app oficial.
- **Consequência:** Continuidade total com o app original. Risco teórico de WAL lock se ambos os apps estiverem abertos simultaneamente — mitigado por kill do processo original antes de iniciar.

## ADR-006: Gitflow como workflow de desenvolvimento
- **Data:** 2026-03-08
- **Contexto:** Precisamos de um processo formal para que múltiplos agentes/sessões trabalhem no projeto sem conflitos.
- **Decisão:** Implementar Gitflow completo: `main` (produção), `develop` (integração), `feature/*`, `release/*`, `hotfix/*`.
- **Consequência:** Todo o trabalho passa por feature branches. Nenhum commit direto em main. Merges sempre com `--no-ff`.
