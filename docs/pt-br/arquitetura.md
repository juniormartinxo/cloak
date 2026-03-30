# Arquitetura interna

## Modulos

- `src/main.rs`: entrada, dispatch dos comandos, fluxo principal.
- `src/cli.rs`: definicao de argumentos/subcomandos (`clap`).
- `src/account.rs`: inspecao local de arquivos de credenciais para `profile account`.
- `src/config.rs`: leitura/criacao de `config.toml` e validacoes.
- `src/profile.rs`: resolucao de `.cloak` e escrita do arquivo local.
- `src/paths.rs`: paths XDG e funcoes de permissao/validacao.
- `src/exec.rs`: montagem de env + exec do CLI alvo.
- `src/mcp.rs`: adaptadores de instalacao de MCP por CLI para fluxos nativos suportados.
- `src/doctor.rs`: checks de saude (binarios, perfis, credenciais).

## Fluxo do comando exec

1. Carrega config global.
2. Resolve perfil por `.cloak` (ou fallback default).
3. Busca CLI em `config.cli`.
4. Garante diretorio `profiles/<perfil>/<cli>`.
5. Seta env var (`config_dir_env`) para esse path.
6. Remove env vars em `remove_env_vars`.
7. Executa o binario real (`exec` no Unix).

## Resolucao de diretorio atual

`main.rs` prioriza o `PWD` logico quando ele aponta para o mesmo caminho real de `current_dir()`.
Isso preserva comportamento esperado com symlinks/worktrees.

## Fluxo do comando `profile account`

1. Valida o nome do perfil solicitado.
2. Garante que `profiles/<perfil>` existe.
3. Percorre os nomes de CLI configurados em `config.cli`.
4. Inspeciona o diretorio home especifico de cada CLI.
5. Imprime uma conta identificada, uma dica de presenca de credenciais ou `not authenticated`.

Detectores especificos atuais:

- `claude`: `.credentials.json`
- `codex`: `auth.json` (incluindo claims JWT decodificados de `id_token`)
- `gemini`: `gemini/.gemini/oauth_creds.json`, `gemini/.gemini/.env`,
  `gemini/.gemini/settings.json`
- outras CLIs: deteccao generica por diretorio nao vazio

## Fluxo do comando `mcp install`

1. Resolve o perfil solicitado, ou o perfil do diretorio atual quando `--profile` nao foi passado.
2. Em terminal interativo, pergunta se a instalacao deve valer para todos os perfis quando
   `--all-profiles` nao foi informado.
3. Valida o formato da requisicao de MCP de acordo com o transporte selecionado.
4. Traduz a requisicao para a sintaxe nativa de MCP da CLI alvo.
5. Executa a CLI alvo dentro de cada home de perfil selecionado para que a configuracao do MCP seja gravada por perfil.

## Seguranca

- Diretorios de perfil e subdirs: `0700` no Unix.
- Arquivos sensiveis de config criados pelo `cloak`: `0600` no Unix.
- `cloak` nao implementa OAuth nem guarda segredo proprio; apenas isola homes.
