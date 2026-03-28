# Troubleshooting

## `cloak: command not found`

Instale globalmente:

```bash
cd /home/junior/apps/jm/cloak
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

## Editor abre com a conta errada de vez em quando

Isso normalmente acontece com apps GUI como VS Code ou Cursor quando a CLI reaproveita uma
instancia ja aberta, que estava autenticada em outra conta.

Quando `cloak exec cursor ...` roda em um terminal interativo, o `cloak` agora abre o Cursor como
processo filho desacoplado, em vez de substituir o processo do shell com `exec(2)`. Isso evita que
o terminal fique bloqueado ate a janela do editor ser fechada.

Configure o editor com isolamento por perfil no launch:

```toml
[cli.cursor]
binary = "cursor"
launch_args = ["--user-data-dir", "{profile_dir}", "--extensions-dir", "{profile_dir}/extensions", "--new-window"]

[cli.cursor.extra_env]
CURSOR_USER_DATA_DIR = "{profile_dir}"
CURSOR_EXTENSIONS_DIR = "{profile_dir}/extensions"
```

No VS Code, use o mesmo padrao de `launch_args` com `binary = "code"`.

No WSL, se `cursor` resolver para o wrapper do Windows
(`/mnt/c/.../cursor/resources/app/bin/cursor`), o `cloak` agora tenta abrir o `Cursor.exe`
diretamente. Nesse modo ele mantem `--user-data-dir` para isolar o perfil e remove
`--extensions-dir`, para as extensoes instaladas continuarem disponiveis enquanto o
`User/globalStorage` vai para o diretorio do perfil.

Se por algum motivo o `Cursor.exe` nao puder ser aberto diretamente, e ainda aparecer algo como:

```text
Ignoring option 'user-data-dir': not supported for cursor.
Ignoring option 'extensions-dir': not supported for cursor.
```

entao o estado GUI do Cursor ainda esta caindo no perfil global do Windows.

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
