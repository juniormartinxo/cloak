# AGENTS.md

## Contexto do Projeto

**Projeto**: `cloak`.
**Objetivo**: isolar perfis por diretório para CLIs e editores ligados a LLMs, roteando autenticação, diretórios de configuração e variáveis de ambiente para o
perfil correto sem exigir wrappers persistentes ou daemons.
**Stack Principal**: Rust 2021 com `clap`, `serde`, `toml`, `color-eyre`,
`owo-colors`, `dirs` e `which`.
**Runtime**: toolchain Rust compatível com `edition = "2021"`.
**Package Manager / Build Tool**: Cargo (`Cargo.toml`, `Cargo.lock`).

## Comandos e Ferramentas

Para alterações em código Rust, você deve garantir formatação, lint e testes antes de finalizar qualquer implementação.

- **Instalar / compilar dependências**: `cargo build`
- **Rodar a CLI localmente**: `cargo run -- <comando>`
- **Rodar diagnóstico**: `cargo run -- doctor`
- **Formatar**: `cargo fmt`
- **Lint**: `cargo clippy --all-targets -- -D warnings`
- **Testes**: `cargo test`
- **Testes de integração**:
  `cargo test --test exec_integration -- --nocapture`

Observação: o repositório é uma CLI Rust única. A maior parte da lógica vive em `src/`, com cobertura adicional em `tests/exec_integration.rs` e documentação em `docs/`.

## Arquitetura

- **CLI principal**: `src/main.rs` faz o dispatch dos subcomandos e integra config, profile resolution, exec, doctor e inspeção de contas.
- **Definição de comandos**: `src/cli.rs` concentra os structs e enums do `clap`.
- **Configuração**: `src/config.rs` carrega e valida `~/.config/cloak/config.toml` e os blocos `[cli.*]`.
- **Resolução de perfil**: `src/profile.rs` sobe a árvore de diretórios em busca de `.cloak` e também grava esse arquivo local quando necessário.
- **Execução isolada**: `src/exec.rs` resolve perfil, cria diretórios seguros, prepara variáveis de ambiente, aplica `launch_args` e delega para a CLI real.
- **Paths e segurança de diretórios**: `src/paths.rs` resolve caminhos XDG, valida nomes de perfil/CLI e endurece diretórios de perfil.
- **Inspeção de credenciais**: `src/account.rs` tenta inferir a identidade ativa de cada CLI suportada a partir de arquivos locais.
- **Saúde e migração**: `src/doctor.rs` verifica configuração, binários, estrutura de perfis e blocos recomendados ausentes.
- **Testes**:
  - unitários nos módulos de `src/`
  - integração em `tests/exec_integration.rs`, cobrindo fluxo de `exec`, forwarding de args, remoção de env conflitante, fallback de perfil e casos de integração com Cursor/WSL

## Restrições de Código

- Preserve compatibilidade com Rust 2021.
- Sempre use `cargo fmt` ao final de mudanças em Rust.
- Sempre rode `cargo clippy --all-targets -- -D warnings` e `cargo test` antes de concluir implementações relevantes.
- Não introduza dependências novas em `Cargo.toml` sem necessidade técnica clara.
- Não enfraqueça o isolamento de perfis, a sanitização de variáveis de ambiente nem o endurecimento de permissões de diretórios.
- Não use caminhos hardcoded dependentes do ambiente do autor quando a lógica já possui helpers em `paths.rs`.
- Ao documentar caminhos de arquivos neste repositório, use caminhos relativos quando o contexto for documentação interna.

## Restrições de Comandos

- Quando o prompt iniciar com `CLAUDE`, apenas opine sobre o conteúdo, não implemente:
  - numere os itens
  - categorize a opinião (`concordo` | `discordo` | `concordo parcialmente`)
  - categorize o item opinado (`corrigir` | `falso positivo` | `documentar`)
    - use `corrigir` quando concordar e houver necessidade de correção de código
    - use `documentar` quando for alerta para entendimento de outros devs
    - use `falso positivo` quando o apontamento não fizer sentido
    - use `nada a acrescentar` quando não houver nada a acrescentar
  - seja explícito se compensa ou não corrigir
  - traga sempre as categorias no início de cada item dentro de `[]`, separadas por `|`

## Comandos do Chat

- Só implemente quando o prompt iniciar com:
  - `#sdnp`
  - `#r`
- Quando o prompt iniciar com:
  - `#review`, crie um relatório em Markdown com foco em bugs, riscos, regressões comportamentais e cobertura de testes.
  - `#r`, refatore preservando comportamento e reduzindo complexidade.
  - `#debug`, implemente logs ou instrumentação mínima para depurar o fluxo da feature em implementação, sem flags adicionais.
  - `#test`, implemente testes unitários e, quando fizer sentido, testes de integração para a feature em implementação.
  - `#plano`, elabore um plano para execução da tarefa solicitada e crie o documento em `docs/plans` se isso fizer sentido para o escopo.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **cloak** (458 symbols, 866 relationships, 39 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## When Debugging

1. `gitnexus_query({query: "<error or symptom>"})` — find execution flows related to the issue
2. `gitnexus_context({name: "<suspect function>"})` — see all callers, callees, and process participation
3. `READ gitnexus://repo/cloak/process/{processName}` — trace the full execution flow step by step
4. For regressions: `gitnexus_detect_changes({scope: "compare", base_ref: "main"})` — see what your branch changed

## When Refactoring

- **Renaming**: MUST use `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` first. Review the preview — graph edits are safe, text_search edits need manual review. Then run with `dry_run: false`.
- **Extracting/Splitting**: MUST run `gitnexus_context({name: "target"})` to see all incoming/outgoing refs, then `gitnexus_impact({target: "target", direction: "upstream"})` to find all external callers before moving code.
- After any refactor: run `gitnexus_detect_changes({scope: "all"})` to verify only expected files changed.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Tools Quick Reference

| Tool | When to use | Command |
|------|-------------|---------|
| `query` | Find code by concept | `gitnexus_query({query: "auth validation"})` |
| `context` | 360-degree view of one symbol | `gitnexus_context({name: "validateUser"})` |
| `impact` | Blast radius before editing | `gitnexus_impact({target: "X", direction: "upstream"})` |
| `detect_changes` | Pre-commit scope check | `gitnexus_detect_changes({scope: "staged"})` |
| `rename` | Safe multi-file rename | `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` |
| `cypher` | Custom graph queries | `gitnexus_cypher({query: "MATCH ..."})` |

## Impact Risk Levels

| Depth | Meaning | Action |
|-------|---------|--------|
| d=1 | WILL BREAK — direct callers/importers | MUST update these |
| d=2 | LIKELY AFFECTED — indirect deps | Should test |
| d=3 | MAY NEED TESTING — transitive | Test if critical path |

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/cloak/context` | Codebase overview, check index freshness |
| `gitnexus://repo/cloak/clusters` | All functional areas |
| `gitnexus://repo/cloak/processes` | All execution flows |
| `gitnexus://repo/cloak/process/{name}` | Step-by-step execution trace |

## Self-Check Before Finishing

Before completing any code modification task, verify:
1. `gitnexus_impact` was run for all modified symbols
2. No HIGH/CRITICAL risk warnings were ignored
3. `gitnexus_detect_changes()` confirms changes match expected scope
4. All d=1 (WILL BREAK) dependents were updated

## Keeping the Index Fresh

After committing code changes, the GitNexus index becomes stale. Re-run analyze to update it:

```bash
npx gitnexus analyze
```

If the index previously included embeddings, preserve them by adding `--embeddings`:

```bash
npx gitnexus analyze --embeddings
```

To check whether embeddings exist, inspect `.gitnexus/meta.json` — the `stats.embeddings` field shows the count (0 means no embeddings). **Running analyze without `--embeddings` will delete any previously generated embeddings.**

> Claude Code users: A PostToolUse hook handles this automatically after `git commit` and `git merge`.

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
