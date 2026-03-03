# Guia de desenvolvimento

## Requisitos

- Rust toolchain (cargo/rustc)

## Build e execucao local

```bash
cargo run -- --help
cargo run -- doctor
```

## Testes

```bash
cargo test
cargo test --test exec_integration -- --nocapture
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

- Unitarios nos modulos (`src/*.rs`) para parsing e resolucao de perfil.
- Integracao em `tests/exec_integration.rs` para validar:
  - env var do perfil no `exec`
  - remocao de API key
  - fallback para `default_profile`
  - resolucao com caminho logico (`PWD`)
