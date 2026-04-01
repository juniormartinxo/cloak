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
cloak profile account work
cloak limits work
cloak limits rank
cloak doctor
```

## Instalar servidores MCP em um perfil

Use `cloak mcp install` quando quiser que a configuracao do MCP fique dentro de um perfil do
`cloak`, e nao no home global da CLI.

Instaladores nativos suportados hoje:

- `codex`: traduz para `codex mcp add ...`
- `claude`: traduz para `claude mcp add ...`
- CLIs nao suportadas: falham com erro claro

Exemplos:

```bash
# MCP stdio no Codex em um perfil
cloak mcp install codex filesystem --profile work -- npx @modelcontextprotocol/server-filesystem /tmp

# MCP HTTP no Codex com env var de bearer token
cloak mcp install codex sentry --profile work --transport http --url https://example.com/mcp --bearer-token-env-var SENTRY_TOKEN

# MCP HTTP no Claude com header
cloak mcp install claude sentry --profile work --transport http --url https://mcp.sentry.dev/mcp -H "Authorization: Bearer token"

# Instalar o mesmo MCP em todos os perfis existentes
cloak mcp install codex filesystem --all-profiles -- npx @modelcontextprotocol/server-filesystem /tmp
```

Se voce nao passar `--profile` nem `--all-profiles` em um terminal interativo, o `cloak` resolve o
perfil atual primeiro e depois pergunta se voce quer aplicar a instalacao em todos os perfis.

## Inspecionar contas autenticadas em um perfil

Use isso quando quiser confirmar qual identidade ficou gravada dentro de um perfil apos o login:

```bash
cloak profile account work
```

Saida tipica:

```text
Profile 'work'

Accounts
╭────────┬──────────────────────────────────────────────────────────────────────╮
│ CLI    ┆ Account                                                              │
╞════════╪══════════════════════════════════════════════════════════════════════╡
│ Claude ┆ credentials detected, but account identifier unavailable (plan: max) │
│ Codex  ┆ Jane Doe <jane@example.com>                                          │
│ Gemini ┆ Gem User <gem@example.com>                                           │
╰────────┴──────────────────────────────────────────────────────────────────────╯
```

Como o `cloak` detecta isso:

- `claude`: inspeciona `claude/.credentials.json`
- `codex`: inspeciona `codex/auth.json`
- `gemini`: inspeciona `gemini/.gemini/oauth_creds.json`, `gemini/.gemini/.env` e
  `gemini/.gemini/settings.json`
- outras CLIs configuradas: mostra uma mensagem generica de "credentials detected" quando o
  diretorio do perfil nao esta vazio

Esse comando apenas inspeciona arquivos locais dentro de `profiles/<nome>/<cli>`; ele nao consulta
nenhuma API remota.

## Inspecionar limites de uso

Use isso quando quiser os snapshots locais de limites mais recentes. Se voce omitir o nome do perfil, o comando exibe os limites de **todos** os perfis registrados:

```bash
# Inspecionar limites de todos os perfis
cloak limits

# Inspecionar limites de um perfil especifico
cloak limits work
```

Por padrao, os horarios de reset sao exibidos em UTC. Use `--utc` para converter para um offset
UTC especifico:

```bash
# Exibir resets em UTC-3 (ex.: Brasilia)
cloak limits work --utc -3

# Exibir resets em UTC+5
cloak limits work --utc 5
```

Saida tipica:

```text
Profile 'work'

Claude
  Status: usage snapshot available
  Details: plan: team, tier: default_raven
  Observed: 2026-03-28T18:12:44Z
  ╭───────────┬────────┬───────┬───────────┬─────────┬─────────────────────────╮
  │ Limit     ┆ Window ┆  Used ┆ Remaining ┆  Pacing ┆ Resets                  │
  ╞═══════════╪════════╪═══════╪═══════════╪═════════╪═════════════════════════╡
  │ five_hour ┆ 5h     ┆ 12.5% ┆     87.5% ┆ 18.2%/h ┆ 2026-03-28 17:42:39 UTC │
  │ seven_day ┆ 1w     ┆   37% ┆       63% ┆ 12.4%/d ┆ 2026-04-03 13:36:17 UTC │
  ╰───────────┴────────┴───────┴───────────┴─────────┴─────────────────────────╯

Codex
  Status: usage snapshot available
  Details: plan: team
  Observed: 2026-03-28T15:23:12.299Z
  Limit: Codex Team
  ╭───────────┬────────┬──────┬───────────┬─────────┬─────────────────────────╮
  │ Limit     ┆ Window ┆ Used ┆ Remaining ┆  Pacing ┆ Resets                  │
  ╞═══════════╪════════╪══════╪═══════════╪═════════╪═════════════════════════╡
  │ primary   ┆ 5h     ┆   1% ┆       99% ┆ 20.6%/h ┆ 2026-03-28 17:42:39 UTC │
  │ secondary ┆ 1w     ┆  30% ┆       70% ┆ 13.8%/d ┆ 2026-04-03 13:36:17 UTC │
  ╰───────────┴────────┴──────┴───────────┴─────────┴─────────────────────────╯
```

Origem dos snapshots:

- `claude`: le `profiles/<nome>/claude/usage-limits.json`, gravado pelo statusline padrao do
  Claude depois que o Claude recebe pelo menos uma resposta naquele perfil.
- `codex`: le o evento `token_count` mais recente em `profiles/<nome>/codex/sessions` e usa o
  payload `rate_limits` persistido pela CLI do Codex.

Orientacao de refresh:

- `claude`: se ainda nao existir snapshot, ou se alguma janela aparecer como `expired *`, abra ou
  continue o Claude naquele perfil e aguarde uma resposta. O statusline grava o proximo
  `usage-limits.json` automaticamente; nao e preciso rodar `/usage`.
- `codex`: se ainda nao existir snapshot, ou se alguma janela aparecer como `expired *`, abra ou
  continue o Codex naquele perfil. O `cloak limits` vai aproveitar o proximo snapshot de
  `token_count` gravado em `codex/sessions`; nao e preciso rodar `/status`.

## Rankear limites de uso entre perfis

Para ver qual perfil tem a maior porcentagem de limite semanal disponivel para uma dada IA, use:

```bash
cloak limits rank
```

Esse comando consulta todos os snapshots locais e exibe um rank descendente dos limites semanais (a janela de 7 dias) agrupado por IA, ajudando na escolha do perfil com maior disponibilidade para balanceamento de uso.

Comportamento do ranking:

- as linhas agora incluem a coluna `Snapshot`
- `fresh` significa que o snapshot semanal ainda esta valido
- `expired` significa que o snapshot semanal ja virou; a linha continua visivel para referencia, mas
  passa a ser ordenada depois dos snapshots frescos
- linhas expiradas continuam mostrando `expired *` na coluna `Resets`, alem de uma dica abaixo da
  tabela explicando como capturar um snapshot novo

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
cloak exec codex --profile work
```

Com alias no shell:

```bash
alias claude='cloak exec claude'
alias codex='cloak exec codex'
alias gemini='cloak exec gemini'
```

Com isso, `claude`, `codex` e `gemini` passam automaticamente pelo `cloak`.

Quando precisar, `cloak exec` tambem aceita um perfil explicito:

```bash
cloak exec codex --profile work
cloak exec codex --profile work -- --model gpt-5.4
```

Passe `--profile <nome>` antes dos argumentos repassados para a CLI. Use `--` se quiser
encaminhar uma flag como `--profile` para a propria CLI alvo.

Exemplo visual da execucao com perfil explicito:

![Demonstração do cloak executando o Claude em perfis isolados](../../sources/images/cloak_claude.jpg)
