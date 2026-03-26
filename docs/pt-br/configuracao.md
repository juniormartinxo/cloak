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

## Adicionar outro CLI

Adicione no `config.toml`:

```toml
[cli.aider]
binary = "aider"
config_dir_env = "AIDER_CONFIG_HOME"
remove_env_vars = ["OPENAI_API_KEY"]
```

Depois use normalmente:

```bash
cloak exec aider
```

Ela tambem passa a aparecer em `cloak profile account <nome>`. Para CLIs sem logica dedicada de
inspecao, o comando cai em uma mensagem generica de "credentials detected" quando o diretorio do
perfil nao esta vazio.

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
