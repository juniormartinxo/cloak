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

## `doctor` mostra "no credential file detected"

Isso normalmente significa que voce ainda nao autenticou nesse perfil.

Faça login no contexto do perfil:

```bash
cloak login claude <perfil>
cloak login codex <perfil>
cloak login gemini <perfil>
```

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
