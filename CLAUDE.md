# CLAUDE.md

## Contexto do Projeto

**Projeto**: `cloak`, uma CLI Rust para isolamento de perfis por diretório.
**Objetivo**: resolver o perfil ativo a partir de `.cloak`, isolar credenciais/configurações por projeto para CLIs como `claude`, `codex` e `gemini`, e delegar a execução ao binário real via `exec`.
**Stack Principal**: Rust 2021, `clap`, `serde`, `toml`, `color-eyre`, `owo-colors`, `dirs`, `which`, `base64`, `serde_json`, `comfy-table` e `rustyline`.
**Runtime**: toolchain Rust `1.93.1` (`rustc 1.93.1`, `cargo 1.93.1`).
**Package Manager**: `cargo` (instalação local com `cargo install --path .`).

Este arquivo é um entrypoint operacional. Mantenha-o curto: ele deve mudar decisões do agente, não substituir `README`, documentação de arquitetura ou exploração dirigida do código.

## Comandos e Ferramentas

Para alterações em código Rust, garanta formato, lint e testes antes de finalizar a implementação.

- **Compilar**: `cargo build`
- **Instalar a CLI localmente**: `cargo install --path .`
- **Executar ajuda da CLI**: `cargo run -- --help`
- **Rodar diagnóstico principal**: `cargo run -- doctor`
- **Rodar um comando da CLI em desenvolvimento**: `cargo run -- <subcomando>`
- **Formatação**: `cargo fmt`
- **Linter**: `cargo clippy --all-targets --all-features -- -D warnings`
- **Testes**: `cargo test`
- **Teste de integração com output**:
  `cargo test --test exec_integration -- --nocapture`

Para mudanças só em documentação, não rode a suíte Rust inteira por obrigação; valide o diff e, se fizer sentido, rode apenas verificações leves como `git diff --check`.

## Roteamento Técnico

- `src/main.rs`: entrypoint e dispatch dos subcomandos.
- `src/cli.rs`: argumentos e subcomandos via `clap`.
- `src/config.rs`: carga e validação de `~/.config/cloak/config.toml` e blocos `[cli.*]`.
- `src/profile.rs`: busca/gravação do `.cloak` e fallback de perfil.
- `src/exec.rs`: preparação de ambiente isolado, remoção de env conflitante, `launch_args` e delegação ao binário real.
- `src/paths.rs`: paths XDG, validação de nomes e permissões de diretórios/arquivos.
- `src/account.rs`: inspeção de identidade ativa a partir de artefatos locais.
- `src/doctor.rs`: diagnóstico de configuração, binários, perfis e migrações recomendadas.
- `tests/exec_integration.rs`: integração de `exec`, forwarding de args, `PWD` lógico, fallback e integração Cursor/WSL.
- Documentação funcional: `docs/` e `docs/pt-br/`.

## Restrições de Código

- Preserve compatibilidade com Rust 2021 e com o fluxo atual de `cargo`.
- Não introduza `unwrap`, `expect` ou `panic!` em fluxos de usuário sem justificativa técnica clara.
- Não engula erros silenciosamente; prefira propagar contexto com `color-eyre` e mensagens explícitas.
- Não altere nomes de subcomandos, variáveis de ambiente (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_HOME`, etc.) ou layout de perfis sem necessidade técnica comprovada.
- Preserve as garantias de isolamento e permissões sensíveis (`0700` para diretórios, `0600` para arquivos criados pela aplicação em Unix).
- Não adicione ou atualize dependências no `Cargo.toml` sem justificativa técnica clara.
- Ao documentar caminhos do projeto, use sempre paths relativos ao repositório.

## Política de Contexto

- Trate `CLAUDE.md` e `AGENTS.md` como roteamento curto e regras de entrada.
- Inclua aqui apenas instruções estáveis e específicas que mudam comportamento do agente.
- Prefira comandos verificáveis, gates e arquivos de estado a texto explicativo longo.
- Não copie documentação existente nem adicione visão geral extensa de diretórios; use `docs/`, `rg --files`, GitNexus ou leitura dirigida quando precisar de detalhes.
- Quando um erro recorrente exigir contexto novo, adicione a menor regra que teria prevenido o erro.

## Restrições de Comandos

- Quando o prompt iniciar com `OPINE`, use o template `docs/templates/opinar.md`: apenas opine sobre o conteúdo, não implemente.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **cloak** (1208 symbols, 2736 relationships, 106 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

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
