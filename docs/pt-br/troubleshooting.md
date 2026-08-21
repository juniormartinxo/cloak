# Troubleshooting

## `cloak: command not found`

Instale globalmente:

```bash
cd cloak
cargo install --path . --force
```

Garanta PATH:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

## `CLI '<nome>' not configured in config.toml`

Adicione bloco `[cli.<nome>]` no `~/.config/cloak/config.toml`.

## `"<binary>" not found in PATH`

- instale o binario do CLI, ou
- configure `binary = "/caminho/absoluto/binario"` no `config.toml`.

## Perfil errado sendo resolvido

```bash
cloak profile show
```

Cheque se existe um `.cloak` em diretorio pai que esta ganhando prioridade.

## `cloak exec cursor` (ou outra CLI customizada) está temporariamente desabilitado

Adicionar `[cli.cursor]`, `[cli.vscode]` ou outro bloco customizado não habilita a execução por
perfil. A allowlist compilada atual contém apenas `claude`, `codex` e `gemini`, portanto o erro
esperado é:

```text
profile management for CLI 'cursor' is temporarily disabled; enabled CLIs: claude, codex, gemini
```

Esse é um limite do produto, não um `config.toml` malformado. O schema e a camada de execução
mantêm `launch_args`, `extra_env`, launch desacoplado e helpers de WSL voltados a editores, mas
esses caminhos não podem ser alcançados enquanto a CLI não for habilitada na implementação.

## `doctor` mostra "no credential file detected"

Isso normalmente significa que voce ainda nao autenticou nesse perfil.

Faça login no contexto do perfil:

```bash
cloak login claude <perfil>
cloak login codex <perfil>
cloak login gemini <perfil>
```

## `cloak profile account <perfil>` mostra `not authenticated`

Isso significa que o `cloak` nao encontrou nenhum arquivo local de credencial suportado dentro do
diretorio da CLI nesse perfil.

Cheque:

- se o login foi feito por `cloak login <cli> <perfil>` ou `cloak exec <cli> --profile <perfil>`
- se a CLI realmente grava credenciais dentro do home configurado
- se o nome da CLI existe em `[cli.<nome>]` no `config.toml`

Depois rode de novo:

```bash
cloak profile account <perfil>
```

## `cloak profile account <perfil>` diz que a CLI ainda nao tem suporte

Esse e o fallback para CLIs configuradas que possuem arquivos no diretorio do perfil, mas ainda nao
tem logica de parse em `src/account.rs`.

O isolamento de perfil continua funcionando no `cloak exec`; apenas a identificacao da conta fica
generica.

## `cloak login gemini <perfil>` falha com `illegal access` (Snap)

Sintomas comuns:

- `starting express`
- `SNAP env is defined, updater is disabled`
- `illegal access`
- `snap-confine ... cap_dac_override not found`

Isso normalmente acontece quando o Gemini foi instalado via Snap e roda com restricoes de confinamento que entram em conflito com o isolamento por `GEMINI_CLI_HOME`.

Correcao recomendada:

```bash
# 1) Remover pacote snap
sudo snap remove gemini

# 2) Instalar Gemini CLI fora do snap (exemplo: npm)
npm install -g @google/gemini-cli

# 3) Validar binario
which gemini
gemini --version
```

Depois, configure caminho explicito do binario em `~/.config/cloak/config.toml`:

```toml
[cli.gemini]
binary = "/caminho/absoluto/para/gemini"
config_dir_env = "GEMINI_CLI_HOME"
remove_env_vars = ["GEMINI_API_KEY", "GOOGLE_API_KEY"]
```

Por fim, tente de novo:

```bash
cloak login gemini <perfil>
```

## Ja tinha perfil criado antes da feature de statusline

Reaplique com seguranca:

```bash
cloak profile create <perfil>
```

Nao sobrescreve `statusLine` existente.

## Config criado antes do suporte ao Gemini

Rode:

```bash
cloak doctor
```

Se faltar `gemini` (ou outro bloco recomendado), o `doctor` oferece um prompt opcional de migracao para incluir o bloco default.

## `mcp doctor` reporta falha no handshake

Comece pelo perfil e servidor exatos para manter a saída focada:

```bash
cloak mcp doctor --profile <perfil> --name <servidor> --timeout 10 --with-tools
```

Para servidores stdio, confira o stderr capturado, confirme que o comando configurado está no
`PATH` e valide as variáveis de ambiente exigidas no perfil. Entradas HTTP/SSE remotas aparecem
como ignoradas porque o `mcp doctor` só executa probes ativos em transportes stdio.

Se o registro estiver desatualizado, confira uma reinstalação idempotente:

```bash
cloak mcp add <servidor> --profile <perfil> --show
cloak mcp add <servidor> --profile <perfil> --replace --yes
```

## Backup ou restore não encontra `tar`, `gzip` ou `gpg`

Rode:

```bash
cloak doctor
```

A seção `Backup Tools` verifica os três binários e faz uma cifragem de teste real com GPG.
Instale ou corrija a ferramenta ausente antes de tentar de novo; apenas encontrar o executável
`gpg` não é considerado suficiente.

## GPG recusa a passphrase do backup

Execuções interativas usam o `pinentry` do GPG. Em modo não interativo, forneça a mesma
passphrase em `CLOAK_BACKUP_PASSPHRASE` tanto no backup quanto no restore. Um backup com falha
remove a saída `.partial`; a ausência de um novo artefato final não significa perda de um backup
anterior já concluído.

## Restore exige `--force`

`--force` é exigido quando o perfil de destino já existe ou quando a identidade por uid/OAuth não
pode ser verificada. A flag permite o merge e ignora checagens de identidade, mas não apaga
arquivos exclusivos do destino.

Não use `--force` para contornar este erro:

```text
this artifact uses backup format vN and this cloak supports up to vM
```

Uma `format_version` mais nova nunca é ignorada. Atualize o `cloak` para uma versão compatível com
o artefato.
