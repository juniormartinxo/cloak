# Arquitetura interna

## Modulos

- `src/main.rs`: entrada, dispatch dos comandos, fluxo principal.
- `src/cli.rs`: definicao de argumentos/subcomandos (`clap`).
- `src/account.rs`: inspecao local de arquivos de credenciais para `profile account`.
- `src/backup.rs`: coleta por allowlist, manifesto, orquestração GPG e restore seguro.
- `src/config.rs`: leitura/criacao de `config.toml` e validacoes.
- `src/profile.rs`: resolucao de `.cloak` e escrita do arquivo local.
- `src/paths.rs`: paths XDG e funcoes de permissao/validacao.
- `src/exec.rs`: montagem de env + exec do CLI alvo.
- `src/mcp.rs`: adaptadores nativos de instalação/remoção de MCP por CLI.
- `src/mcp_registry.rs`: leitura e resolução dos catálogos embutido e do usuário.
- `src/mcp_doctor.rs`: parsing de configurações MCP e probes JSON-RPC stdio.
- `src/doctor.rs`: checks de saúde (binários, perfis, credenciais e ferramentas de backup).

## Fluxo do comando exec

1. Carrega config global.
2. Resolve perfil por `.cloak` (ou fallback default).
3. Busca CLI em `config.cli`.
4. Garante diretorio `profiles/<perfil>/<cli>`.
5. Seta env var (`config_dir_env`) para esse path.
6. Remove env vars em `remove_env_vars`.
7. Aplica `[agents.<cli>]` ao primeiro token de comando encaminhado, quando existir.
8. Executa o binario real (`exec` no Unix).

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

## Ciclo de vida de MCPs

### `mcp add`

1. Carrega o registro compilado de `resources/mcp_registry.toml`.
2. Mescla `~/.config/cloak/mcp_registry.toml`, com precedência para entradas do usuário.
3. Resolve CLIs alvo, escopo de perfis, transporte, placeholders de ambiente e comandos.
4. Permite conferir com `--show` ou remover a entrada anterior com `--replace`.
5. Delega a instalação ao adaptador nativo de `mcp.rs` para cada par CLI/perfil.

### `mcp install`

1. Resolve o perfil solicitado, ou o perfil do diretorio atual quando `--profile` nao foi passado.
2. Em terminal interativo, pergunta se a instalacao deve valer para todos os perfis quando
   `--all-profiles` nao foi informado.
3. Valida o formato da requisicao de MCP de acordo com o transporte selecionado.
4. Traduz a requisicao para a sintaxe nativa de MCP da CLI alvo.
5. Executa a CLI alvo dentro de cada home de perfil selecionado para que a configuracao do MCP seja gravada por perfil.

### `mcp remove` e `mcp doctor`

- A remoção lê primeiro a configuração nativa do perfil. Registros ausentes são ignorados,
  tornando a operação idempotente; registros presentes são removidos pela CLI nativa.
- O doctor interpreta `config.toml` do Codex e `.claude.json` do Claude. Entradas stdio são
  iniciadas e recebem `initialize` via JSON-RPC; transportes remotos aparecem como ignorados.
  `--with-tools` acrescenta `tools/list` depois da inicialização.

## Fluxo da política de permissões

1. `permission ask` carrega a política `[agents.<nome>]` atual.
2. O questionário atualiza shell, escrita de arquivos, rede, allowlist e denylist.
3. `config.rs` valida e grava `config.toml` com permissão `0600`.
4. `exec.rs` classifica o primeiro comando encaminhado e aplica bloqueios explícitos, categorias
   de capacidade, opt-in de comandos perigosos e allowlists não vazias antes de abrir a CLI.
5. Para Claude, as regras `allow`/`deny` geradas são sincronizadas em `settings.json` de todos os
   perfis existentes; campos não relacionados, como `ask` e `defaultMode`, são preservados.

Uma abertura interativa sem comando encaminhado não tem token para classificar. A sincronização
de configurações nativas é específica do Claude; a aplicação pelo wrapper vale para toda CLI
habilitada.

## Fluxo de backup e restauração

### Backup

1. Seleciona um perfil ou todos os perfis existentes.
2. Coleta apenas arquivos das allowlists embutida/do usuário e monta um relatório agregado do que
   ficou de fora.
3. Adiciona o `config.toml` global e um manifesto versionado com origem, perfis, indícios de OAuth,
   MCPs e itens não cobertos.
4. Cria o `tar.gz` em staging privado com `0700`.
5. Cifra com GPG/AES-256 em arquivo `.partial`, aplica `0600` e o renomeia atomicamente para o
   nome final.
6. Executa o `upload_command` opcional, com quoting, depois que o artefato local está completo.

### Restore

1. Decifra e extrai em staging privado.
2. Interpreta o manifesto e recusa versões futuras de formato não suportadas.
3. Verifica uid de destino, indícios de identidade OAuth, perfis pedidos e condições de overwrite.
4. Opcionalmente reescreve home/raiz de perfis da origem em arquivos de texto suportados.
5. Mescla arquivos no destino usando diretórios `0700`, `0700` para arquivos que eram
   executáveis na origem e `0600` para os demais; nunca apaga arquivos exclusivos do destino e
   os reporta como preservados.
6. Grava o `config.toml` global arquivado em `config.toml.from-backup` como referência, com a
   reescrita de paths já aplicada. O `config.toml` em uso nunca é sobrescrito nem mesclado.

## Seguranca

- Diretorios de perfil e subdirs: `0700` no Unix.
- Arquivos sensiveis de config criados pelo `cloak`: `0600` no Unix.
- O staging decifrado de backup é privado e temporário; artefatos finais são sempre cifrados.
- As credenciais OAuth continuam sob responsabilidade das CLIs alvo. Elas ficam fora dos backups,
  a menos que o usuário passe `--include-credentials` explicitamente.
