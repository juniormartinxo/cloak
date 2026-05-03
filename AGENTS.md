# AGENTS.md

## Contexto do Projeto

**Projeto**: `cloak`, uma CLI Rust para isolar perfis por diretório para CLIs e editores ligados a LLMs.
**Objetivo**: resolver o perfil ativo a partir de `.cloak`, redirecionar autenticação/configuração para o perfil correto e delegar ao binário real sem wrappers persistentes ou daemons.
**Stack Principal**: Rust 2021 com `clap`, `serde`, `toml`, `color-eyre`, `owo-colors`, `dirs`, `which`, `base64`, `serde_json`, `comfy-table` e `rustyline`.
**Runtime local verificado**: `rustc 1.93.1`, `cargo 1.93.1`.
**Build Tool**: Cargo (`Cargo.toml`, `Cargo.lock`).

Este arquivo é um entrypoint operacional. Mantenha-o curto: ele deve mudar decisões do agente, não substituir `README`, documentação de arquitetura ou exploração dirigida do código.

## Comandos e Ferramentas

Para alterações em código Rust, garanta formatação, lint e testes antes de finalizar a implementação.

- **Compilar**: `cargo build`
- **Instalar localmente**: `cargo install --path .`
- **Ajuda da CLI**: `cargo run -- --help`
- **Rodar subcomando em desenvolvimento**: `cargo run -- <subcomando>`
- **Diagnóstico**: `cargo run -- doctor`
- **Formatar**: `cargo fmt`
- **Lint**: `cargo clippy --all-targets -- -D warnings`
- **Testes**: `cargo test`
- **Testes de integração**:
  `cargo test --test exec_integration -- --nocapture`

Para mudanças só em documentação, não rode a suíte Rust inteira por obrigação; valide o diff e, se fizer sentido, rode apenas verificações leves como `git diff --check`.

## Roteamento Técnico

- `src/main.rs`: entrypoint e dispatch dos subcomandos.
- `src/cli.rs`: structs/enums do `clap`.
- `src/config.rs`: carga e validação de `~/.config/cloak/config.toml` e blocos `[cli.*]`.
- `src/profile.rs`: busca/gravação do `.cloak` e fallback de perfil.
- `src/exec.rs`: preparação de ambiente isolado, remoção de env conflitante, `launch_args` e delegação ao binário real.
- `src/paths.rs`: paths XDG, validação de nomes e permissões de diretórios/arquivos.
- `src/account.rs`: inspeção de identidade ativa a partir de artefatos locais.
- `src/doctor.rs`: diagnóstico de configuração, binários, perfis e migrações recomendadas.
- `tests/exec_integration.rs`: integração de `exec`, forwarding de args, `PWD` lógico, fallback e integração Cursor/WSL.
- Documentação funcional: `docs/` e `docs/pt-br/`.

## Restrições de Código

- Preserve compatibilidade com Rust 2021.
- Não introduza `unwrap`, `expect` ou `panic!` em fluxos de usuário sem justificativa técnica clara.
- Não engula erros silenciosamente; prefira propagar contexto com `color-eyre`.
- Não altere nomes de subcomandos, variáveis de ambiente (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_HOME`, etc.) ou layout de perfis sem necessidade técnica comprovada.
- Não introduza dependências novas em `Cargo.toml` sem necessidade técnica clara.
- Não enfraqueça isolamento de perfis, sanitização de variáveis de ambiente nem permissões sensíveis (`0700` para diretórios, `0600` para arquivos criados pela aplicação em Unix).
- Não use caminhos hardcoded dependentes do ambiente do autor quando a lógica já possui helpers em `paths.rs`.
- Ao documentar caminhos deste repositório, use paths relativos.

## Política de Contexto

- Trate `AGENTS.md` e `CLAUDE.md` como roteamento curto e regras de entrada.
- Inclua aqui apenas instruções estáveis e específicas que mudam comportamento do agente.
- Prefira comandos verificáveis, gates e arquivos de estado a texto explicativo longo.
- Não copie documentação existente nem adicione visão geral extensa de diretórios; use `docs/`, `rg --files`, GitNexus ou leitura dirigida quando precisar de detalhes.
- Quando um erro recorrente exigir contexto novo, adicione a menor regra que teria prevenido o erro.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **cloak** (1178 symbols, 2694 relationships, 103 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/cloak/context` | Codebase overview, check index freshness |
| `gitnexus://repo/cloak/clusters` | All functional areas |
| `gitnexus://repo/cloak/processes` | All execution flows |
| `gitnexus://repo/cloak/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
