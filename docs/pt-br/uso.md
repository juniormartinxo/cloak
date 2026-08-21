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
cd cloak
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

### Catálogo MCP embutido

Para servidores comuns, prefira o comando baseado no registro:

```bash
# listar o catálogo atual
cloak mcp add

# conferir os comandos nativos sem instalar
cloak mcp add gitnexus --show

# instalar nas CLIs escolhidas e em um perfil
cloak mcp add gitnexus --for codex,claude --profile work --yes

# remover um registro existente antes de reinstalar
cloak mcp add filesystem --replace --profile work --yes
```

O registro embutido cobre servidores de referência e integrações populares como `filesystem`,
`git`, `memory`, `playwright`, `context7`, `gitnexus`, `github`, `shadcn` e `sentry`. As entradas
podem expandir variáveis de ambiente, além de `${CWD}` e `${HOME}`. A instalação para com erro
explícito quando uma variável obrigatória não existe.

Também é possível estender o registro sem alterar o binário. Adicione entradas em
`~/.config/cloak/mcp_registry.toml`; entradas do usuário substituem entradas embutidas com o
mesmo nome.

### Remover e diagnosticar servidores MCP

`mcp remove` delega para a CLI nativa e é idempotente: um servidor ausente aparece como
`not installed` em vez de interromper toda a operação.

```bash
# conferir um par perfil/CLI
cloak mcp remove filesystem --profile work --for codex --dry-run

# remover de todos os perfis existentes nas CLIs suportadas
cloak mcp remove filesystem --all-profiles --yes
```

`mcp doctor` lê os MCPs stdio configurados nos perfis Claude/Codex selecionados e executa um
handshake JSON-RPC `initialize` real. Entradas HTTP/SSE são reportadas, mas não são iniciadas como
processos stdio.

```bash
cloak mcp doctor --profile work
cloak mcp doctor --all-profiles --name gitnexus --timeout 10 --with-tools
```

`--with-tools` envia `tools/list` depois de uma inicialização bem-sucedida. Uma falha faz o comando
terminar com erro depois que todas as entradas correspondentes forem verificadas.

## Configurar permissões de agentes

Rode o questionário guiado para manter uma política `[agents.<nome>]` no `config.toml`:

```bash
cloak permission ask --agent codex
cloak permission ask --agent claude
```

O questionário cobre acesso ao shell, escrita de arquivos, rede e listas explícitas de comandos
permitidos e bloqueados. `cloak exec` verifica o primeiro token de comando encaminhado antes de
abrir o agente: bloqueios explícitos têm prioridade, comandos perigosos exigem allowlist explícita
e uma allowlist não vazia recusa comandos ausentes. Ao iniciar um agente interativo sem comando
encaminhado, não há token para classificar.

Para Claude, salvar a política também sincroniza regras `allow` e `deny` no `settings.json` de todos
os perfis Claude existentes, preservando campos não relacionados como `ask` e `defaultMode`.
Outros agentes recebem as checagens do wrapper, mas não sincronização nativa de configurações
enquanto não houver adaptador.

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

Se o perfil explicito nao existir, o `cloak` mostra os perfis ja disponiveis e pergunta se deve
criar o novo. Se a resposta for `nao`, ele encerra sem executar a CLI alvo.

Exemplo visual da execucao com perfil explicito:

![Demonstração do cloak executando o Claude em perfis isolados](../../sources/images/cloak_claude.jpg)

## Backup e restauração

Use `cloak backup` para gerar um artefato cifrado com a configuração e o conhecimento de um ou
mais perfis, e `cloak restore` para trazer esse artefato de volta em outra máquina (ou na mesma,
depois de uma reinstalação).

```text
cloak backup  [--profile <nome>] [--output <dir>] [--include-credentials] [--dry-run]
cloak restore <arquivo> [--profile <nome>] [--force] [--dry-run] [--no-rewrite-paths]
```

Sem `--profile`, `cloak backup` inclui **todos** os perfis num único artefato, e `cloak restore`
restaura todos os perfis presentes no artefato.

> **Aviso: guarde a passphrase.** O artefato é sempre cifrado com `gpg --symmetric` (AES-256). Se
> você perder a passphrase, **o backup se torna irrecuperável** — não existe modo de recuperação.
> Guarde a passphrase em um gerenciador de senhas.

### Exemplo de fluxo

```bash
# ver o que entraria no backup, sem gerar nenhum arquivo
cloak backup --dry-run

# backup de todos os perfis
cloak backup --output /caminho/destino

# restaurar na máquina nova
cloak restore /caminho/cloak-backup-20260725-122130.tar.gz.gpg
```

### O que entra no backup

A seleção usa uma allowlist, não uma cópia integral do perfil. Entram:

- `settings.json`, `keybindings.json`, arquivos `*.md` no topo e os diretórios completos `skills/`
  e `.agents/`;
- o arquivo `.cloak` na raiz do perfil, quando existir;
- do `claude`: `statusline-command.sh`, `plans/`, memórias de projeto em
  `projects/*/memory/` e os manifestos de plugin (`plugins/installed_plugins.json`,
  `plugins/known_marketplaces.json`, `plugins/blocklist.json`);
- do `codex`: `config.toml`, `hooks.json` e o diretório `memories/`;
- do `gemini`: `.gemini/settings.json` e os arquivos `*.md` de `.gemini/` (incluindo o
  `GEMINI.md`). O `gemini` aninha toda a configuração em `<perfil>/gemini/.gemini/`, então os
  padrões de topo não alcançam nada dele.
- o `config.toml` global do Cloak e um `manifest.json` versionado na raiz do artefato.

Ficam de fora sessões, logs, caches, plugins baixados e histórico de projetos. No `gemini` isso
inclui `history/`, `tmp/`, os caches de IDE sob `.gemini/antigravity*` e o estado de máquina
(`installation_id`, `state.json`, `projects.json`, `trustedFolders.json`), reconstruído pela CLI.
Em perfis reais isso costuma reduzir vários GB para poucos MB.

A cada backup, o `cloak` lista o que encontrou no perfil e **não** entrou no artefato — isso
existe porque uma allowlist, por natureza, omite o desconhecido, e o relatório garante que uma
omissão apareça antes de virar perda de dado. Um diretório totalmente fora da allowlist aparece
como uma única linha com o tamanho agregado; um diretório parcialmente coberto lista cada arquivo
que ficou de fora.

Um arquivo que casa com a allowlist mas não pode ser lido — tipicamente um symlink quebrado
deixado por um plugin ou skill desinstalado, ou um arquivo removido durante o backup — é **pulado
e reportado em `stderr`**, não aborta o backup. O artefato é gerado com todo o resto.

### Credenciais

`claude/.credentials.json`, `codex/auth.json`, `gemini/.gemini/oauth_creds.json` e
`gemini/.gemini/.env` ficam **fora do backup por padrão**. São tokens OAuth e chaves de API que
expiram e podem ser regenerados em minutos com `cloak login` na máquina de destino. Use
`--include-credentials` para incluí-los explicitamente — nesse caso, um vazamento do artefato
junto com a passphrase dá acesso direto às contas, então avalie o risco antes de usar essa flag.

Com `--include-credentials`, esses arquivos passam a contar como cobertos: deixam de aparecer no
relatório de não-cobertos e no `uncovered` do manifesto, porque estão dentro do artefato.

### Configuração em `config.toml`

O bloco opcional `[backup]` em `~/.config/cloak/config.toml` controla destino e upload:

```toml
[backup]
output_dir = "/mnt/c/Users/junior/OneDrive/cloak-backups"
upload_command = "rclone copy {archive} gdrive:cloak/"
include = []
```

- `output_dir`: diretório de saída padrão. A resolução final segue esta ordem de prioridade:
  `--output` na linha de comando, depois `output_dir` do `config.toml`, depois o padrão
  `~/.config/cloak/backups`.
- `upload_command`: comando executado após gerar o artefato, com `{archive}` substituído pelo
  caminho do arquivo gerado (com quoting seguro, então caminhos com espaço funcionam sem escapar
  manualmente).
- `include`: padrões adicionais de arquivo/diretório que **somam** à allowlist embutida — não
  removem nenhum item padrão.

O nome do artefato segue o formato `cloak-backup-<YYYYMMDD-HHMMSS>.tar.gz.gpg` e é criado com
permissão `0600`. A cifragem é escrita primeiro em um caminho `.partial`, que só recebe o nome
final depois que o GPG termina com sucesso; assim, uma interrupção não deixa um artefato truncado
com nome definitivo.

### Uso não interativo (cron/CI)

Por padrão, a passphrase de cifragem é pedida via `pinentry`. Para rodar `cloak backup` sem
interação (por exemplo, num cron job ou pipeline de CI), defina a variável de ambiente
`CLOAK_BACKUP_PASSPHRASE` — com ela definida, o `gpg` roda em modo não interativo.

### Restaurando um backup

`cloak restore <arquivo>` decifra o artefato, valida o manifesto e verifica a identidade (uid e
conta OAuth) **antes** de escrever qualquer coisa no destino. Pontos importantes:

- Recusa sobrescrever um perfil já existente; use `--force` para permitir.
- Se a identidade registrada no manifesto não puder ser verificada, o restore também exige
  `--force` — a falha é segura por padrão, nunca silenciosa.
- Um backup com `format_version` mais novo é recusado mesmo com `--force`; atualize o `cloak`
  antes de restaurá-lo.
- **É um merge, não uma substituição**: nada do perfil de destino é apagado. Arquivos que já
  existem no destino e não vêm no artefato são preservados, e o restore lista explicitamente
  esses arquivos preservados ao final.
- Reescreve caminhos absolutos da máquina de origem (o `$HOME` e a raiz de perfis originais)
  dentro de arquivos `.json`, `.toml`, `.md` e `.sh`. Use `--no-rewrite-paths` para desativar essa
  reescrita.
- Ao final, relata o que não veio no backup e será reconstruído automaticamente pelas CLIs na
  primeira execução (plugins e marketplaces). O manifesto também registra os nomes de MCPs do
  Claude detectados em `.claude.json` para reconciliação manual.
- O `config.toml` global do Cloak entra no artefato como referência, mas o restore atual só mescla
  `profiles/`; ele não substitui a configuração global do destino.
- Um perfil listado no manifesto que não tem diretório dentro do artefato (artefato truncado, ou
  perfil cujo conteúdo era todo não-coberto) gera um **aviso explícito** em `stderr`. Se nenhum
  perfil chegar a ser restaurado, o `cloak restore` **falha com código diferente de 0** em vez de
  sair com sucesso sem ter escrito nada.
- Um arquivo que não pode ser lido como texto (por exemplo um `.md` salvo em latin-1, ou um `.sh`
  com byte não-UTF-8) é restaurado **intacto, porém sem reescrita de paths**, com um aviso em
  `stderr`. Ele não derruba mais o restore. Use `--no-rewrite-paths` para desligar a reescrita em
  todos os arquivos.
- Arquivos executáveis voltam executáveis: um arquivo que era executável na origem é restaurado
  com `0700`, os demais com `0600`. Isso vale para `statusline-command.sh`, hooks referenciados em
  `codex/hooks.json` e executáveis sob `skills/` e `.agents/`. Em nenhum caso a permissão é
  afrouxada para grupo ou outros.
- Use `--dry-run` para ver o plano de restauração sem tocar no destino.

### Dependências de sistema

`cloak backup` e `cloak restore` dependem dos binários `tar`, `gzip` e `gpg` no `PATH`. Rode
`cloak doctor` para verificar: ele tem uma seção "Backup Tools" que reporta a presença dos três
e, no caso do `gpg`, confirma que ele realmente consegue cifrar um arquivo de teste (não apenas
que o binário existe).
