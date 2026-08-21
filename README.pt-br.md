# 🎭 cloak

> **Gerenciador de perfis por diretório para CLIs de LLM** — isole credenciais, contextos e identidades por projeto com zero atrito.

[English](README.md) | [Português](README.pt-br.md)

[![Rust](https://img.shields.io/badge/Rust-2021_edition-orange?logo=rust)](https://www.rust-lang.org/)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
![Status: MVP](https://img.shields.io/badge/Status-MVP-green)

---

## O Problema

Você trabalha com várias contas ao mesmo tempo — uma conta de trabalho para o `claude`, uma pessoal para o `codex`, talvez a chave de API de um cliente para um repositório específico. Mas ambas as CLIs mantêm seus estados de autenticação **globalmente** em um único diretório home.

Mudar de contexto significa exportar variáveis de ambiente manualmente, mover arquivos de configuração ou torcer para não vazar a chave errada no projeto errado.

**cloak resolve isso de forma limpa.**

---

## Como Funciona

O `cloak` resolve o perfil correto para o diretório atual percorrendo o sistema de arquivos para cima procurando por um arquivo `.cloak`, e então define a variável de ambiente apropriada (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, etc.) **antes** de passar o controle para a CLI real via `exec(2)`.

```text
~/repos/
├── company-api/        ← .cloak (perfil = "work")
│   └── ...                 └─► CLAUDE_CONFIG_DIR → ~/.config/cloak/profiles/work/claude
│
└── side-project/       ← .cloak (perfil = "personal")
    └── ...                  └─► CLAUDE_CONFIG_DIR → ~/.config/cloak/profiles/personal/claude
```

Sem wrappers rodando em segundo plano. Sem daemons. Sem estado persistente. Apenas um `exec` limpo substituindo o processo atual.

---

## Funcionalidades

| Funcionalidade | Descrição |
| --- | --- |
| 📁 **Perfis com escopo de diretório** | Arquivos `.cloak` vinculam repositórios a perfis nomeados |
| 🔗 **Exec sem sobrecarga (zero-overhead)** | Perfil resolvido → env definido → `exec(2)` do binário real |
| 🔒 **Isolamento de credenciais** | Variáveis de ambiente conflitantes (ex: `ANTHROPIC_API_KEY`) são removidas antes do exec |
| 🔍 **Resolução automática** | Percorre até a raiz; faz fallback para `default_profile` da configuração |
| 👤 **Inspeção de conta** | Mostra com qual conta cada CLI do perfil parece estar autenticada |
| 📊 **Limites locais de uso** | Lê snapshots de Claude e Codex e ranqueia perfis pela capacidade semanal disponível |
| 🩺 **Comando doctor** | Valida a configuração, binários, estrutura do perfil, dicas de credenciais e ferramentas de backup |
| 💻 **Completions de shell** | Bash, Zsh, Fish, PowerShell e Elvish |
| 🖥️ **Statusline do Claude** | Provisiona automaticamente um script de statusline mostrando modelo/contexto/custo e persistindo snapshots de limites |
| 🔌 **Ciclo de vida de MCPs** | Catálogo, instalação nativa, remoção idempotente e diagnóstico JSON-RPC por perfil |
| 🛡️ **Política de permissões** | Questionário para shell, arquivos, rede e comandos, com sincronização para perfis Claude |
| 📦 **Backup e restauração cifrados** | Conhecimento selecionado por allowlist, credenciais opcionais, merge seguro e reescrita de caminhos |

---

## Documentacao Completa

A documentacao detalhada esta em:

- Ingles: [`docs/`](./docs/README.md)
- Portugues (Brasil): [`docs/pt-br/`](./docs/pt-br/README.md)

---

## Instalação

```bash
# A partir do código fonte
cargo install --path .

# Desenvolvimento
cargo run -- <comando>
```

---

## Guia Rápido

```bash
# 1. Crie perfis
cloak profile create work
cloak profile create personal

# 2. Vincule um repositório a um perfil
cd ~/repos/company-api
cloak use work

# 3. Adicione aliases no shell
alias claude='cloak exec claude'
alias codex='cloak exec codex'
alias gemini='cloak exec gemini'

# 4. Autentique-se uma vez por perfil — o cloak roteia a CLI automaticamente
cd ~/repos/company-api && claude   # ← usa o perfil "work"
cd ~/side-project      && claude   # ← usa o perfil "personal"

# 5. Inspecione o contexto atual
cloak profile show
cloak profile account work
cloak limits work

# 6. Instale um MCP do catálogo embutido
cloak mcp add filesystem --for codex,claude --profile work --yes

# 7. Confira e gere um backup cifrado
cloak backup --dry-run
cloak backup
```

`cloak profile account <nome>` inspeciona cada home de CLI configurada dentro do perfil e imprime o
melhor indício local de identidade que encontrar:

- `claude`: lê `.credentials.json`; mostra email/nome quando existir e, caso contrário, informa que
  existem credenciais e pode incluir o plano detectado.
- `codex`: lê `auth.json`; prioriza o `id_token` decodificado e depois faz fallback para
  `account_id` ou para uma indicação de API key.
- `gemini`: lê `gemini/.gemini/oauth_creds.json`, `gemini/.gemini/.env` e
  `gemini/.gemini/settings.json`.
- outras CLIs configuradas: se o diretório do perfil não estiver vazio, o `cloak` informa que
  existem credenciais, mas que aquela CLI ainda não tem suporte específico.

Exemplo de saída:

```text
Profile 'work'
claude -> credentials detected, but account identifier unavailable (plan: max)
codex -> Jane Doe <jane@example.com>
gemini -> Gem User <gem@example.com>
```

`cloak limits [nome]` lê os snapshots locais disponíveis. Se omitido o nome do perfil, exibe de todos os perfis:

- `claude`: le `claude/usage-limits.json`, preenchido pelo statusline padrao do Claude depois que o
  Claude recebe pelo menos uma resposta naquele perfil. Mostra os percentuais mais recentes das
  janelas de 5 horas e 7 dias, logica adaptativa de pacing (%/h ou %/d) baseada no tempo restante,
  alem dos timestamps de reset. Para atualizar dados ausentes ou expirados, abra ou continue o
  Claude naquele perfil e aguarde uma resposta; nao e preciso rodar `/usage`.
- `codex`: le o evento `token_count` mais recente em `codex/sessions` e mostra as janelas
  registradas, o percentual restante, a taxa de pacing e os timestamps de reset. Para atualizar
  dados ausentes ou expirados, abra ou continue o Codex naquele perfil; nao e preciso rodar
  `/status`.

`cloak limits rank` usa o snapshot semanal de cada perfil e agora mostra a coluna `Snapshot`.
Snapshots frescos aparecem primeiro; snapshots expirados continuam visiveis para referencia, mas
ficam ordenados depois das linhas frescas e marcados com `expired *` em `Resets`.

`cloak mcp add` é o caminho mais rápido: sem nome, lista o catálogo embutido; com um nome,
resolve transporte, comando e CLIs suportadas. Use `--show` para conferir os comandos nativos e
`--replace` para uma reinstalação idempotente:

```bash
cloak mcp add
cloak mcp add gitnexus --for codex,claude --profile work --yes
cloak mcp add sentry --show
cloak mcp add filesystem --replace --profile work --yes
```

`cloak mcp install` continua disponível para servidores fora do catálogo. Ele instala dentro do
perfil selecionado usando a sintaxe nativa de cada CLI suportada:

- `codex`: vira `codex mcp add ...`
- `claude`: vira `claude mcp add ...`
- CLIs nao suportadas: falham com erro explicito em vez de chute

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

Use `cloak mcp remove <nome>` para remover um registro. Entradas ausentes aparecem como
`not installed`, portanto repetir o comando é seguro. Use `cloak mcp doctor` para ler os MCPs
stdio registrados, executar um handshake JSON-RPC `initialize` real e, opcionalmente, chamar
`tools/list`:

```bash
cloak mcp remove filesystem --profile work --for codex --dry-run
cloak mcp remove filesystem --profile work --for codex --yes
cloak mcp doctor --profile work --with-tools
```

---

## Resolução de Perfil

O `cloak` começa a partir do diretório atual e percorre **para cima** até a raiz do sistema de arquivos procurando pelo `.cloak` mais próximo:

```toml
# ~/repos/company-api/.cloak
profile = "work"
```

Nenhum `.cloak` encontrado? Ele fará um fallback para `general.default_profile` de `~/.config/cloak/config.toml`.

---

## Configuração

Gerada automaticamente na primeira execução em `~/.config/cloak/config.toml`:

```toml
[general]
default_profile = "personal"

[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"
remove_env_vars = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]

[cli.codex]
binary = "codex"
config_dir_env = "CODEX_HOME"
remove_env_vars = ["OPENAI_API_KEY"]

[cli.gemini]
binary = "gemini"
config_dir_env = "GEMINI_CLI_HOME"
remove_env_vars = ["GEMINI_API_KEY", "GOOGLE_API_KEY"]
```

O gerenciamento de perfis está habilitado atualmente apenas para `claude`, `codex` e `gemini`.
Blocos `[cli.<nome>]` adicionais são interpretados, mas não habilitam a CLI em `exec`, `login` ou
na criação de perfis; nomes não suportados falham com erro `temporarily disabled`.
Se seu config foi criado antes do suporte ao Gemini, rode `cloak doctor` e aceite o prompt opcional de migração para incluir os blocos recomendados ausentes.

`cloak profile account <nome>` percorre as CLIs configuradas em `[cli.*]`, então adicionar um novo
bloco tambem faz essa CLI aparecer na saída de inspeção de conta.

O schema também aceita `config_dir_env` opcional, `launch_args` prefixados e valores `extra_env`
com placeholders `{profile_dir}`, `{profile_name}` e `{cli_name}`. O formato abaixo para
Cursor/VS Code continua útil como referência de configuração, mas esses nomes não estão habilitados
na allowlist atual de gerenciamento de perfis:

```toml
[cli.cursor]
binary = "cursor"
launch_args = ["--user-data-dir", "{profile_dir}", "--extensions-dir", "{profile_dir}/extensions", "--new-window"]

[cli.cursor.extra_env]
CURSOR_USER_DATA_DIR = "{profile_dir}"
CURSOR_EXTENSIONS_DIR = "{profile_dir}/extensions"

[cli.vscode]
binary = "code"
launch_args = ["--user-data-dir", "{profile_dir}", "--extensions-dir", "{profile_dir}/extensions", "--new-window"]
```

Se o gerenciamento de perfis de editores for reativado, esse padrão evita reutilizar uma instância
GUI que já esteja autenticada em outra conta.

A camada de execução mantém tratamento específico de Cursor/WSL, inclusive um
`VSCODE_AGENT_FOLDER` por perfil, mas esse caminho está inacessível por `cloak exec cursor`
enquanto Cursor estiver fora da allowlist de CLIs habilitadas.

Limitacao conhecida: isso melhora o isolamento de estado em editores estilo Cursor/VS Code, mas
nao garante logins separados de extensao por perfil do `cloak`. Algumas extensoes, incluindo o
Codex, tambem podem usar o SecretStorage do editor ou o keyring/credential store do sistema. Quando
isso acontece, isolar `user-data`, `extensions-dir` e `VSCODE_AGENT_FOLDER` pode ainda nao ser
suficiente para manter contas diferentes separadas dentro da mesma instalacao do editor.

---

## Comandos

```text
cloak exec <cli> [--profile <nome>] [args...]
                                   Resolve o perfil, define env, remove vars conflitantes, executa a CLI
cloak use <profile>                Escreve .cloak no diretório atual
cloak profile list                 Lista todos os perfis
cloak profile account <nome>       Mostra qual conta cada CLI esta usando dentro de um perfil
cloak limits [nome]                Mostra uso/restante/pacing de Claude/Codex (omita <nome> para todos)
cloak limits rank                  Classifica perfis por limite semanal disponível (agrupado por IA)
cloak profile create <nome>        Cria diretórios de perfil (+ template de statusline do Claude no Unix)
cloak profile delete <nome> [-y]   Deleta um perfil
cloak profile show                 Mostra o perfil resolvido e caminhos de env para cada CLI
cloak login <cli> [profile]        Executa uma CLI no contexto do perfil para auth interativa
cloak mcp add [nome]               Lista o catálogo embutido ou instala uma entrada
cloak mcp install <cli> <nome>     Instala um servidor MCP usando a sintaxe nativa da CLI alvo
cloak mcp remove <nome>            Remove um registro MCP (idempotente quando ausente)
cloak mcp doctor                   Testa MCPs stdio configurados via JSON-RPC
cloak permission ask [--agent X]   Configura permissões do agente interativamente
cloak backup [opções]              Cria um backup cifrado baseado em allowlist
cloak restore <arquivo> [opções]   Restaura perfis com checagens de identidade e formato
cloak doctor                       Verifica config, binários, perfis e ferramentas de backup
cloak completions <shell>          Imprime script de autocompletar do shell
```

`cloak init <profile>` ainda é suportado como um alias de compatibilidade para `cloak use <profile>`.

Ao usar `cloak exec`, passe `--profile <nome>` antes dos argumentos repassados para a CLI.
Use `--` se quiser encaminhar um argumento como `--profile` para a própria CLI alvo.

Exemplo visual da feature em uso, executando a CLI com perfis isolados no momento do comando:

![Demonstração do cloak executando o Claude em perfis isolados](./sources/images/cloak_claude.jpg)

---

## Arquitetura

```text
src/
├── account.rs    — Helpers de inspecao de credenciais/conta por CLI
├── backup.rs     — Backup/restore cifrado, manifesto e reescrita de caminhos
├── main.rs       — Ponto de entrada da CLI, despacho de comandos (clap + derive)
├── cli.rs        — Structs de argumentos e definições de subcomandos
├── config.rs     — Parsing do arquivo de configuração e padrões (serde + toml)
├── exec.rs       — Resolução de perfil + configuração de env + wrapper de exec(2)
├── mcp.rs        — Adaptadores de instalação de MCP por CLI (`claude` / `codex`)
├── mcp_doctor.rs — Descoberta de configs MCP e diagnóstico JSON-RPC
├── mcp_registry.rs — Leitura dos catálogos embutido/local e expansão de variáveis
├── paths.rs      — Resolução de caminhos em conformidade com XDG para config/perfis
├── profile.rs    — Operações CRUD de perfil e provisionamento de statusline
└── doctor.rs     — Diagnósticos e verificações de saúde
```

**Tech stack:** Rust 2021 · `clap` (derive) · `serde`/`toml` · `color-eyre` · `owo-colors` · `which`

---

## Statusline do Claude

Quando você cria um perfil no Unix, o `cloak` provisiona um script de statusline dentro do diretório de perfil do Claude:

```json
{
  "statusLine": {
    "type": "command",
    "command": "bash '<profile-claude-dir>/statusline-command.sh'"
  }
}
```

O script le o stdin em JSON do Claude, imprime uma linha compacta com **modelo / tokens de
contexto / custo** (requer `jq`) e persiste o snapshot mais recente de `rate_limits` do Claude em
`usage-limits.json` para o `cloak limits`. Um `settings.json` existente com uma chave
`statusLine` nunca e sobrescrito.

---

## Segurança

- No uso normal, o `cloak` redireciona os homes das CLIs e não implementa OAuth por conta própria.
- Backups são sempre cifrados com GPG/AES-256; arquivos OAuth só entram quando
  `--include-credentials` é informado explicitamente.
- Diretórios de perfil e CLI são criados com **permissões apenas para o proprietário** (`0700`) no Unix.
- Arquivos criados pelo `cloak`, inclusive artefatos de backup, usam `0600` no Unix. Arquivos
  restaurados mantêm `0700` quando eram executáveis na origem, e `0600` nos demais casos; a
  permissão nunca é afrouxada para grupo ou outros.
- Variáveis de ambiente conflitantes são **removidas** antes do exec para que nenhuma credencial de ambiente vaze para uma sessão.

---

## Desenvolvimento

```bash
cargo test      # testes unitários + integração
cargo fmt       # formatação
cargo clippy    # linting
```

Os testes de integração ficam em `tests/exec_integration.rs` e `tests/backup_integration.rs`.
Eles cobrem execução/MCP com binários mock e fluxos reais de backup/restore cifrado quando o
GPG está disponível.

---

## Troubleshooting

### CLI not found

```text
"<binary>" not found in PATH
```

Instale a CLI correspondente ou defina `cli.<name>.binary` em `config.toml`.

### Wrong profile

```bash
cloak profile show   # mostra o perfil resolvido + caminhos de env
```

Em seguida, verifique se há um `.cloak` inesperado mais acima na árvore de diretórios.

### Conflict with `direnv`

Se o `direnv` exportar a mesma variável de ambiente (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`), o último que escrever ganha. Escolha um mecanismo por CLI.

---

## Licença

Apache-2.0
