# Visao geral e uso

O `cloak` isola credenciais por diretorio para CLIs de LLM (ex.: `claude`, `codex`).

## Como funciona

1. O comando resolve o perfil ativo no diretorio atual.
2. Seta a env var do CLI para o diretorio do perfil.
3. Remove env vars conflitantes (API key global).
4. Executa o binario real via `exec`.

## Instalacao

Instalacao global a partir do projeto:

```bash
cd /home/junior/apps/jm/cloak
cargo install --path . --force
```

Validacao:

```bash
which cloak
cloak --help
```

## Fluxo rapido

```bash
# 1) Criar perfis
cloak profile create work
cloak profile create personal

# 2) Associar repo ao perfil
cd ~/repos/company-api
cloak use work

# 3) Login no contexto do perfil
cloak login claude work
cloak login codex work
cloak login gemini work

# 4) Inspecionar contexto atual
cloak profile show
cloak doctor
```

## Trocar perfil de um repo

No diretorio do repo:

```bash
cloak use personal
```

Observacao: `cloak init <profile>` continua funcionando como alias de compatibilidade.

## Alias (opcional)

Sem alias, voce precisa chamar `cloak exec` sempre:

```bash
cloak exec claude
cloak exec codex
```

Com alias no shell:

```bash
alias claude='cloak exec claude'
alias codex='cloak exec codex'
alias gemini='cloak exec gemini'
```

Com isso, `claude`, `codex` e `gemini` passam automaticamente pelo `cloak`.
