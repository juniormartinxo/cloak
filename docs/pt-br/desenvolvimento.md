# Guia de desenvolvimento

## Requisitos

- Rust toolchain (`cargo`/`rustc`); o checkout atual foi verificado com Rust 1.93.1

## Build e execucao local

```bash
cargo run -- --help
cargo run -- doctor
```

## Testes

```bash
cargo test
cargo test --test exec_integration -- --nocapture
cargo test --test backup_integration -- --nocapture
```

## Qualidade

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Instalacao global de desenvolvimento

Atualizar binario global com versao local:

```bash
cargo install --path . --force
```

## Estrategia de testes atual

- Unitários em `src/*.rs` para parsing, resolução de perfil, registro/probes MCP, permissões,
  seleção de backup, manifestos e reescrita de caminhos.
- Integracao em `tests/exec_integration.rs` para validar:
  - env var do perfil no `exec`
  - remocao de API key
  - fallback para `default_profile`
  - resolucao com caminho logico (`PWD`)
  - instalação MCP nativa e fluxo `mcp add` baseado em catálogo
- Integração em `tests/backup_integration.rs` para validar:
  - backup e restore realmente cifrados por GPG, quando a ferramenta está disponível
  - guardas de identidade/overwrite, comportamento de merge e reescrita de caminhos
  - permissões seguras e limpeza de artefatos parciais em caso de falha
