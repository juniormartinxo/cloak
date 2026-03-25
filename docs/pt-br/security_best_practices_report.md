# Relatorio de Boas Praticas de Seguranca

## Resumo Executivo

Escopo: revisao manual de seguranca da implementacao da CLI em Rust em `src/`, com enfase em manipulacao do sistema de arquivos, resolucao de perfis, execucao de processos e limites locais de confianca.

Resumo:

- Nao encontrei um problema critico de memory safety nem um bug obvio de exclusao arbitraria de arquivos no codigo Rust atual.
- O problema mais importante e um problema de limite de confianca: arquivos `.cloak` locais ao repositorio sao confiados automaticamente, o que pode trocar silenciosamente o usuario para um perfil local privilegiado quando ele executa a ferramenta dentro de um repositorio nao confiavel.
- Encontrei mais duas lacunas de hardening de severidade media: endurecimento incompleto de permissoes em diretorios intermediarios de perfis e resolucao repetida de binarios via `PATH` para execucoes privilegiadas da CLI.
- Ponto positivo: o codigo valida nomes de perfis e CLIs, remove variaveis de ambiente conflitantes antes de `exec` e usa permissoes apenas para o proprietario em varios arquivos sensiveis e diretorios finais.

## Alta Severidade

### SBP-001: `.cloak` local ao repositorio e implicitamente confiado

Impacto: um repositorio controlado por um atacante pode fazer com que o `cloak` inicie uma CLI real de LLM sob um perfil local sensivel ja existente, como `work` ou `personal`, aumentando a chance de uso indevido de credenciais ou divulgacao acidental em um workspace nao confiavel.

Evidencias:

- `src/profile.rs:40` sobe a arvore a partir do diretorio atual e aceita o primeiro arquivo `.cloak` que encontrar.
- `src/profile.rs:45` usa `candidate.is_file()`, e `Path::is_file()` do Rust segue links simbolicos.
- `src/profile.rs:46` e `src/profile.rs:75` leem o arquivo e extraem o nome de perfil solicitado.
- `src/main.rs:38` e `src/main.rs:50` usam esse perfil resolvido automaticamente para `exec`.
- `src/main.rs:116` e `src/main.rs:131` fazem o mesmo para `login` quando nenhum perfil explicito e informado.

Por que isso importa:

- O modelo de confianca atual assume que um arquivo `.cloak` local ao repositorio e seguro para ser respeitado.
- Na pratica, um repositorio clonado ou de terceiros pode incluir `.cloak = "work"` ou `.cloak = "personal"`.
- Se o usuario tiver aliases de shell como `codex='cloak exec codex'`, simplesmente executar a CLI dentro desse repositorio pode trocar a sessao para um contexto de conta mais privilegiado sem nenhuma confirmacao.
- Como a descoberta de `.cloak` percorre diretorios pai, o limite de confianca na pratica passa a ser "qualquer diretorio no caminho ate o diretorio de trabalho atual".

Correcao recomendada:

- Adicionar um fluxo explicito de confianca para arquivos `.cloak` locais ao repositorio, por exemplo `cloak trust`, e respeitar esses caminhos somente depois da primeira aprovacao.
- Como alternativa, tratar `.cloak` como nao confiavel, a menos que ele tenha sido criado por `cloak use` na maquina local e registrado em uma allowlist local separada.
- No minimo, exibir um aviso interativo antes de respeitar um `.cloak` pertencente ao repositorio cujo caminho ainda nao esteja confiado.
- Considere rejeitar arquivos `.cloak` que sejam links simbolicos usando `symlink_metadata` e recusando `FileType::is_symlink()` nesse limite de confianca.

## Media Severidade

### SBP-002: Diretorios intermediarios de perfil podem manter permissoes mais amplas do que o desejado

Evidencias:

- `src/paths.rs:31` chama `fs::create_dir_all(path)` e depois endurece apenas o `path` exato via `set_owner_only_dir`.
- `src/paths.rs:102` define modo `0700` apenas no caminho de diretorio fornecido.
- `src/main.rs:175` cria `~/.config/cloak/profiles/<profile>` chamando `ensure_secure_dir` apenas no diretorio final do perfil.
- `src/exec.rs:50` e `src/exec.rs:54` endurecem o diretorio do perfil e o diretorio final da CLI, mas nao a raiz `profiles` se ela tiver sido criada como pai intermediario.

Por que isso importa:

- Quando `create_dir_all` cria pais ausentes, esses diretorios intermediarios mantem as permissoes padrao.
- Em uma configuracao Unix tipica, isso pode deixar `~/.config/cloak/profiles` com um modo derivado do `umask` atual, em vez do modo apenas para o proprietario que era a intencao.
- Mesmo que cada diretorio final de perfil seja `0700`, uma raiz `profiles` mais ampla ainda pode vazar nomes de perfis, como nomes de clientes, contas ou ambientes, para outros usuarios locais.

Correcao recomendada:

- Endurecer explicitamente `cloak_config_dir()`, `profiles_dir()`, o diretorio do perfil e o diretorio da CLI como etapas separadas.
- Se continuar usando `create_dir_all`, percorra os ancestrais relevantes criados depois da chamada e aplique permissoes apenas para o proprietario em cada diretorio dentro da raiz `cloak`.
- Adicionar um teste de integracao que valide os modos finais de `cloak`, `profiles`, `<profile>` e `<profile>/<cli>`.

### SBP-003: Binarios de CLI sao resolvidos a partir do `PATH` ambiente em cada execucao

Evidencias:

- `src/exec.rs:16` resolve o binario alvo com `which::which(&cli_cfg.binary)`.
- `src/doctor.rs:35` usa a mesma busca baseada em `PATH` para diagnosticos.
- `src/config.rs:27` armazena o binario como string livre, em vez de exigir um caminho absoluto.

Por que isso importa:

- Se o `PATH` contiver diretorios gravaveis por um atacante antes do binario real, o `cloak` pode executar um binario de CLI falsificado.
- Nesse cenario, o binario falsificado recebe o diretorio home especifico do perfil e se beneficia da confianca do usuario no `cloak`.
- Isso nao e, por si so, uma falha de execucao remota de codigo; depende de um ambiente de shell comprometido ou de uma composicao insegura do `PATH`. Ainda assim, e uma lacuna relevante de hardening para um launcher sensivel do ponto de vista de seguranca.

Correcao recomendada:

- Preferir caminhos absolutos para binarios em `config.toml`, ou resolver o binario uma unica vez durante o setup e persistir o caminho absoluto.
- Adicionar um aviso no `doctor` quando um `binary` configurado nao for absoluto.
- Opcionalmente, alertar quando o `PATH` contiver entradas relativas como `.` ou diretorios claramente gravaveis pelo usuario antes de locais comuns do sistema.

## Pontos Positivos

- `src/paths.rs:41` e `src/paths.rs:72` validam nomes de perfil e CLI, reduzindo riscos de path traversal e de smuggling de flags.
- `src/exec.rs:30` remove variaveis de ambiente de credenciais conflitantes antes de transferir o controle para a CLI real.
- `src/config.rs:164` e `src/config.rs:173` criam a configuracao principal com permissoes de arquivo apenas para o proprietario no Unix.
- `src/main.rs:381` e `src/main.rs:425` aplicam permissoes apenas para o proprietario aos assets gerados da statusline do Claude.

## Observacoes

- Intencionalmente nao sinalizei `src/main.rs:241` (`fs::remove_dir_all`) como um bug de exclusao com seguimento de link simbolico. A documentacao da biblioteca padrao do Rust informa que `remove_dir_all` nao segue links simbolicos e remove o proprio link simbolico.
- O problema de confianca em `.cloak` descrito acima e a principal preocupacao de seguranca, porque ele afeta o limite central de selecao de conta que esta ferramenta pretende proteger.
