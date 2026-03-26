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
| 🩺 **Comando doctor** | Valida a configuração, binários, estrutura do perfil e dicas de credenciais |
| 💻 **Completions de shell** | Bash, Zsh, Fish, PowerShell e Elvish |
| 🖥️ **Statusline do Claude** | Provisiona automaticamente um script de statusline mostrando modelo/contexto/custo |

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

Adicionar uma nova CLI é tão simples quanto adicionar um novo bloco `[cli.<nome>]`.
Se seu config foi criado antes do suporte ao Gemini, rode `cloak doctor` e aceite o prompt opcional de migração para incluir os blocos recomendados ausentes.

`cloak profile account <nome>` percorre as CLIs configuradas em `[cli.*]`, então adicionar um novo
bloco tambem faz essa CLI aparecer na saída de inspeção de conta.

---

## Comandos

```text
cloak exec <cli> [--profile <nome>] [args...]
                                   Resolve o perfil, define env, remove vars conflitantes, executa a CLI
cloak use <profile>                Escreve .cloak no diretório atual
cloak profile list                 Lista todos os perfis
cloak profile account <nome>       Mostra qual conta cada CLI esta usando dentro de um perfil
cloak profile create <nome>        Cria diretórios de perfil (+ template de statusline do Claude no Unix)
cloak profile delete <nome> [-y]   Deleta um perfil
cloak profile show                 Mostra o perfil resolvido e caminhos de env para cada CLI
cloak login <cli> [profile]        Executa uma CLI no contexto do perfil para auth interativa
cloak doctor                       Roda verificações de saúde
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
├── main.rs       — Ponto de entrada da CLI, despacho de comandos (clap + derive)
├── cli.rs        — Structs de argumentos e definições de subcomandos
├── config.rs     — Parsing do arquivo de configuração e padrões (serde + toml)
├── exec.rs       — Resolução de perfil + configuração de env + wrapper de exec(2)
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

O script lê o stdin em JSON do Claude e imprime uma linha compacta com **modelo / tokens de contexto / custo** (requer `jq`). Um `settings.json` existente com uma chave `statusLine` nunca é sobrescrito.

---

## Segurança

- O `cloak` **nunca armazena ou criptografa credenciais** — ele apenas redireciona os diretórios de config (homes).
- Diretórios de perfil e CLI são criados com **permissões apenas para o proprietário** (`0700`) no Unix.
- Variáveis de ambiente conflitantes são **removidas** antes do exec para que nenhuma credencial de ambiente vaze para uma sessão.

---

## Desenvolvimento

```bash
cargo test      # testes unitários + integração
cargo fmt       # formatação
cargo clippy    # linting
```

Os testes de integração ficam em `tests/exec_integration.rs` e validam o pipeline completo do `cloak exec` com um binário mock: ligação de env, remoção de chave de API e fallback para perfil padrão.

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
