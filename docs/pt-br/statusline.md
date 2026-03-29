# Statusline do Claude por perfil

Ao rodar `cloak profile create <nome>`, o `cloak` provisiona no perfil do Claude:

- `~/.config/cloak/profiles/<nome>/claude/statusline-command.sh`
- `~/.config/cloak/profiles/<nome>/claude/settings.json` (chave `statusLine`)
- `~/.config/cloak/profiles/<nome>/claude/usage-limits.json` (gravado depois pelo script, apos respostas do Claude)

## Comportamento

- Se o script nao existe: cria.
- Se o script corresponder ao template gerado antigo: atualiza para o padrao atual.
- Se `settings.json` nao existe: cria.
- Se `settings.json` ja tem `statusLine`: nao sobrescreve.

## O que o script padrao faz

- Le JSON via stdin (enviado pelo Claude Code).
- Se tiver `jq`, tenta mostrar: modelo, contexto e custo.
- Quando o Claude envia `rate_limits`, tambem persiste o snapshot mais recente das janelas de 5
  horas / 7 dias em `usage-limits.json` para o `cloak profile limits <nome>`.
- Se nao tiver `jq`, imprime uma linha simples `Claude`.

## Personalizacao

Voce pode editar por perfil:

- `~/.config/cloak/profiles/work/claude/statusline-command.sh`

Se quiser reaplicar template em perfil antigo:

```bash
cloak profile create work
```

Esse refresh de provisionamento tambem acontece em `cloak exec claude`, `cloak login claude` e
`cloak profile limits <nome>`.

Se quiser reaplicar para todos os perfis:

```bash
for p in $(cloak profile list); do
  cloak profile create "$p"
done
```
