# Arquitetura interna

## Modulos

- `src/main.rs`: entrada, dispatch dos comandos, fluxo principal.
- `src/cli.rs`: definicao de argumentos/subcomandos (`clap`).
- `src/config.rs`: leitura/criacao de `config.toml` e validacoes.
- `src/profile.rs`: resolucao de `.cloak` e escrita do arquivo local.
- `src/paths.rs`: paths XDG e funcoes de permissao/validacao.
- `src/exec.rs`: montagem de env + exec do CLI alvo.
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

## Seguranca

- Diretorios de perfil e subdirs: `0700` no Unix.
- Arquivos sensiveis de config criados pelo `cloak`: `0600` no Unix.
- `cloak` nao implementa OAuth nem guarda segredo proprio; apenas isola homes.
