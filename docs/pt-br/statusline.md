# Statusline do Claude por perfil

Ao rodar `cloak profile create <nome>`, o `cloak` provisiona no perfil do Claude:

- `~/.config/cloak/profiles/<nome>/claude/statusline-command.sh`
- `~/.config/cloak/profiles/<nome>/claude/settings.json` (chave `statusLine`)

## Comportamento

- Se o script nao existe: cria.
- Se `settings.json` nao existe: cria.
- Se `settings.json` ja tem `statusLine`: nao sobrescreve.

## O que o script padrao faz

- Le JSON via stdin (enviado pelo Claude Code).
- Se tiver `jq`, tenta mostrar: modelo, contexto e custo.
- Se nao tiver `jq`, imprime uma linha simples `Claude`.

## Personalizacao

Voce pode editar por perfil:

- `~/.config/cloak/profiles/work/claude/statusline-command.sh`

Se quiser reaplicar template em perfil antigo:

```bash
cloak profile create work
```

Se quiser reaplicar para todos os perfis:

```bash
for p in $(cloak profile list); do
  cloak profile create "$p"
done
```
