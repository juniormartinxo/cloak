# Configuracao e estrutura de perfis

## Arquivo global

Caminho:

- `~/.config/cloak/config.toml`

Gerado automaticamente no primeiro uso.

Exemplo padrao:

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

## Associacao por diretorio (.cloak)

Arquivo na raiz do repo:

```toml
profile = "work"
```

O `cloak` sobe do diretorio atual ate `/` procurando o `.cloak` mais proximo.
Se nao encontrar, usa `general.default_profile`.

## Estrutura de diretorios de perfil

```text
~/.config/cloak/
├── config.toml
├── mcp_registry.toml        # catálogo MCP opcional do usuário
├── backups/                # destino padrão dos backups cifrados
└── profiles/
    ├── work/
    │   ├── claude/
    │   ├── codex/
    │   └── gemini/
    └── personal/
        ├── claude/
        ├── codex/
        └── gemini/
```

## Blocos de CLI suportados e customizados

O gerenciamento de perfis está habilitado atualmente apenas para `claude`, `codex` e `gemini`.
Um bloco customizado é uma configuração válida:

```toml
[cli.aider]
binary = "aider"
config_dir_env = "AIDER_CONFIG_HOME"
remove_env_vars = ["OPENAI_API_KEY"]
```

mas não habilita a execução sozinho. Hoje este comando falha com o erro claro
`profile management ... temporarily disabled`:

```bash
cloak exec aider
```

Blocos customizados ainda podem aparecer em caminhos de configuração/inspeção de conta, mas
`exec`, `login`, execução MCP raw e criação de diretórios de perfil obedecem à allowlist compilada.
Estender essa lista exige mudança de implementação, não apenas de configuração.

## Política de permissões de agentes

A configuração padrão inclui uma política para Codex:

```toml
[agents.codex]
allow_shell = true
allow_file_write = true
allow_network = true
allowed_commands = []
deny_commands = []
```

Prefira `cloak permission ask --agent <nome>` à edição manual. O comando valida nomes de
comandos e impede que o mesmo item termine nas duas listas. Uma política de Claude também é
sincronizada para os arrays `permissions.allow` e `permissions.deny` de todos os perfis Claude
existentes.

O wrapper `cloak exec` aplica a política ao primeiro token de comando encaminhado: bloqueios
explícitos têm prioridade; categorias de shell/arquivo/rede desabilitadas bloqueiam comandos
conhecidos; comandos perigosos exigem allowlist explícita; e uma `allowed_commands` não vazia
recusa comandos ausentes da lista. Ao abrir um agente interativo sem comando encaminhado, não há
token para classificar.

Claude também recebe regras nativas sincronizadas em `settings.json`. Outros agentes continuam com
a aplicação no wrapper, mas sem sincronização para a configuração da CLI alvo enquanto não houver
adaptador.

## Configuração de backup

O bloco opcional abaixo controla destino padrão, comando de upload pós-backup e extensões da
allowlist:

```toml
[backup]
output_dir = "/caminho/para/cloak-backups"
upload_command = "rclone copy {archive} remote:cloak/"
include = ["extra/*.json", "conhecimento-customizado/"]
```

`--output` tem prioridade sobre `output_dir`; sem ambos, os artefatos vão para
`~/.config/cloak/backups`. `{archive}` é substituído pelo caminho final do artefato com quoting
seguro. Os padrões de `include` acrescentam arquivos à allowlist embutida e nunca removem os
padrões existentes.

## Registro MCP do usuário

`resources/mcp_registry.toml` é compilado no binário. Crie
`~/.config/cloak/mcp_registry.toml` para adicionar ou substituir entradas de `cloak mcp add`.
Valores aceitam expansão de variáveis de ambiente, além de `${CWD}` e `${HOME}`; variáveis
ausentes geram erro.

## Configuracao avancada de launch

`config_dir_env` agora e opcional. Voce tambem pode definir:

- `launch_args`: argumentos inseridos antes dos args repassados para a CLI
- `[cli.<nome>.extra_env]`: variaveis de ambiente extras para injetar

Placeholders suportados:

- `{profile_dir}`
- `{profile_name}`
- `{cli_name}`

Exemplo para editores estilo VS Code/Cursor:

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

Esse formato evita reutilizar uma instância GUI já autenticada em outra conta, mas `cursor` e
`vscode` não estão habilitados na allowlist atual de gerenciamento de perfis. O exemplo documenta
o schema suportado e o caminho dormente de launch de editores, não uma CLI chamável hoje.

## Migracao opcional para configs existentes

Se seu `config.toml` ja existia antes de um novo bloco recomendado (por exemplo `gemini`), rode:

```bash
cloak doctor
```

O `doctor` detecta blocos recomendados ausentes e, em terminal interativo, pergunta se deve incluir os defaults automaticamente.

## Regras de nome de perfil

Valido:
- letras e numeros
- `-`, `_`, `.`

Invalido:
- vazio
- `.` ou `..`
- com `/` ou `\\`
- iniciando com `-`
