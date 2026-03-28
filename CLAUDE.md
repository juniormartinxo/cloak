# CLAUDE.md

## Contexto do Projeto

**Projeto**: `cloak` (CLI em Rust para isolamento de perfis por diretório).
**Objetivo**: resolver o perfil ativo a partir de arquivos `.cloak`, isolar
credenciais/configurações por projeto para CLIs como `claude`, `codex` e
`gemini`, e então delegar a execução ao binário real via `exec`.
**Stack Principal**: Rust edition 2021, `clap` 4 (derive), `serde`/`toml`,
`color-eyre`, `owo-colors`, `which`, `base64` e `serde_json`.
**Runtime**: toolchain Rust `1.93.1` (`rustc 1.93.1`, `cargo 1.93.1`).
**Package Manager**: `cargo` (instalação local com `cargo install --path .`).

## Comandos e Ferramentas

Somente para arquivos Rust, você deve garantir que o código passe no formato, no linter e nos testes antes de finalizar qualquer implementação.

- **Instalar a CLI localmente**: `cargo install --path .`
- **Executar ajuda da CLI**: `cargo run -- --help`
- **Rodar diagnóstico principal**: `cargo run -- doctor`
- **Rodar um comando da CLI em desenvolvimento**: `cargo run -- <subcomando>`
- **Testes**: `cargo test`
- **Teste de integração com output**:
  `cargo test --test exec_integration -- --nocapture`
- **Formatação**: `cargo fmt -- --check`
- **Linter**: `cargo clippy --all-targets --all-features -- -D warnings`

Observação: este repositório é uma CLI Rust standalone; a documentação funcional está em `docs/`, com versões em inglês e português (`docs/pt-br/`).

## Arquitetura

- **CLI principal**: `src/main.rs` faz o entrypoint, resolve o diretório atual, despacha subcomandos e integra os fluxos centrais.
- **Interface de linha de comando**: `src/cli.rs` define argumentos e subcomandos via `clap`.
- **Configuração global**: `src/config.rs` faz bootstrap, parsing e validação de `~/.config/cloak/config.toml`.
- **Resolução de perfil local**: `src/profile.rs` encontra o `.cloak` mais próximo ao subir a árvore de diretórios e grava/valida bindings locais.
- **Execução isolada**: `src/exec.rs` prepara variáveis de ambiente, remove credenciais conflitantes e delega ao binário real com `exec`.
- **Paths e segurança**: `src/paths.rs` centraliza paths XDG, permissões, criação de diretórios e validação de nomes.
- **Inspeção de contas**: `src/account.rs` lê artefatos locais de credenciais para reportar qual conta parece estar autenticada em cada perfil.
- **Diagnóstico**: `src/doctor.rs` valida toolchain, binários configurados, estrutura de perfis, hints de credenciais e migrações recomendadas.
- **Testes**: unidades ficam distribuídas em `src/*.rs`; integrações vivem em `tests/exec_integration.rs`, cobrindo wiring de ambiente, fallback de perfil padrão, resolução via `PWD` lógico e remoção de variáveis globais.

## Restrições de Código

- Preserve compatibilidade com Rust 2021 e com o fluxo atual de `cargo`.
- Não introduza `unwrap`, `expect` ou `panic!` em fluxos de usuário sem justificativa técnica clara.
- Não engula erros silenciosamente; prefira propagar contexto com `color-eyre` e mensagens explícitas.
- Não altere nomes de subcomandos, variáveis de ambiente (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_HOME`, etc.) ou layout de perfis sem necessidade técnica comprovada.
- Preserve as garantias de isolamento e permissões sensíveis (`0700` para diretórios, `0600` para arquivos criados pela aplicação em Unix).
- Não adicione ou atualize dependências no `Cargo.toml` sem justificativa técnica clara.
- Ao documentar caminhos do projeto, use sempre paths relativos ao repositório.

## Restrições de Comandos

- Quando o prompt iniciar com `CODEX`, apenas opine sobre o conteúdo, não implemente:
  - numere os itens
  - categorize a opinião (`concordo` | `discordo` | `concordo parcialmente`)
  - categorize o item opinado (`corrigir` | `falso positivo` | `documentar`)
    - use `corrigir` quando concordar e houver necessidade de correção de código
    - use `documentar` quando for alerta para entendimento de outros devs
    - use `falso positivo` quando o apontamento não fizer sentido
    - use `nada a acrescentar` quando não houver nada relevante além da análise
  - seja explícito se compensa ou não corrigir
  - traga sempre as categorias no início de cada item dentro de `[]`,
    separadas por `|`

## Comandos do Chat

- Só implemente quando o prompt iniciar com:
  - `#sdnp`
  - `#r`
- Quando o prompt iniciar com:
  - `#review`, faça code review em Markdown com foco em bugs, regressões, riscos e lacunas de teste.
  - `#r`, refatore usando a skill `Code Refactoring`.
  - `#mr`, crie uma descrição de MR da branch atual em relação à branch base apropriada, pronta para copiar e colar.
  - `#debug`, implemente logs temporários com `println!` ou `eprintln!` em pontos-chave para depurar o fluxo em implementação.
  - `#test`, implemente ou ajuste testes unitários e/ou de integração da feature em implementação.
  - `#plano`, elabore um plano para execução da tarefa solicitada e crie o documento em `docs/plans` (crie o diretório se necessário).

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
