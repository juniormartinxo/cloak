# Backup e restauração de perfis

**Data**: 2026-07-24
**Status**: aprovado, pendente de plano de implementação

## Problema

Os perfis do cloak acumulam estado que não existe em nenhum outro lugar: memórias, skills,
agentes, configurações de MCP, `settings.json`, `keybindings.json` e arquivos de contexto
por CLI. Uma formatação de máquina destrói tudo isso, e reconstruir manualmente é inviável.

Não há hoje nenhum caminho de backup no cloak.

## Objetivo

Permitir gerar um artefato portátil com a configuração e o conhecimento dos perfis,
enviá-lo para armazenamento externo (Google Drive, OneDrive ou qualquer outro) e restaurá-lo
em uma máquina nova.

## Não-objetivos

- Retenção automática de backups antigos.
- Backup incremental ou deduplicação.
- Agendamento (cron, timers).
- Preservar histórico de sessões e conversas.
- Integração nativa com API de provedor de nuvem.

## Decisões

### Escopo: configuração e conhecimento, não histórico

O backup preserva configuração e conhecimento. Fica de fora tudo que é reproduzível ou
descartável: sessões, logs, caches, plugins baixados, histórico de projetos.

Medição feita nos perfis reais em 2026-07-24:

| Conjunto | Tamanho |
|---|---|
| `~/.config/cloak/profiles/` completo | ~7 GB |
| Conteúdo relevante para backup | **7,1 MB** |

O que resta depois de descartar sessões, logs, caches, plugins baixados e histórico de
projetos é da ordem de poucos megabytes por perfil (`amjr` 3,3 MB, `gojunior` 3,8 MB).

### Seleção por allowlist, com relatório de não-cobertos

O cloak copia apenas o que está numa allowlist built-in por CLI, e ao final imprime um
relatório de tudo que encontrou no perfil e **não** entrou no backup.

A escolha entre allowlist e denylist é uma escolha de qual falha silenciosa o usuário
prefere. A denylist copia tudo exceto lixo reconhecido: nunca omite, mas quando uma CLI
passa a gravar um arquivo novo — possivelmente com um segredo — esse arquivo vai
silenciosamente para a nuvem. A allowlist copia só o conhecido: nunca vaza o desconhecido,
mas quando uma CLI introduz um arquivo importante, ele fica de fora e o usuário só descobre
ao restaurar, no pior momento possível.

Para uma ferramenta cujo propósito é isolar credenciais e minimizar o que sai da máquina,
"saber exatamente o que sobe" pesa mais que "nunca omitir". A allowlist é a escolha alinhada
a esse propósito. O único defeito grave dela — a omissão silenciosa — é neutralizado pelo
relatório obrigatório de não-cobertos: a cada backup o cloak lista os itens do perfil que
ficaram de fora, com tamanhos, para o usuário decidir se algo novo precisa entrar. Sem esse
relatório a allowlist seria imprópria para backup; com ele, entrega controle e privacidade
sem o risco de perda silenciosa.

Allowlist inicial, relativa à raiz de cada CLI dentro do perfil:

```
# comum a todas as CLIs
settings.json      keybindings.json   *.md
skills/            .agents/

# claude
CLAUDE.md          statusline-command.sh
plugins/installed_plugins.json         plugins/known_marketplaces.json
plugins/blocklist.json

# codex
config.toml        AGENTS.md          hooks.json         memories/

# nível do perfil e global
<perfil>/.cloak    config.toml (global do cloak)
```

A lista de MCP (`.claude.json` → `mcpServers`, `codex/config.toml` → `mcp_servers`) e a
identidade da conta (`.claude.json` → `oauthAccount`) são lidas para o manifesto e para o
relatório de reconstrução, sem copiar o `.claude.json` inteiro — ele carrega os 51
`projectPath` e metadados voláteis que não pertencem a um backup de configuração.

Extensível por `include` em `[backup]` no `config.toml`: os padrões do usuário somam-se aos
built-in. Não há como remover um item built-in da allowlist por configuração; o built-in é o
piso do que o cloak considera essencial.

### Transporte: artefato local mais comando de upload plugável

`cloak backup` grava o artefato em um diretório configurável. Se `upload_command` estiver
definido, o cloak o executa em seguida, substituindo `{archive}` pelo caminho do arquivo.

O cloak não fala com a rede e não conhece nenhum provedor. Isso cobre OneDrive (já montado
na máquina do usuário), Google Drive via `rclone`, S3, `scp` ou qualquer outro destino, com
o mesmo código. Quando o upload falha, o artefato local continua existindo.

Integração nativa com a API do Google Drive foi recusada: adicionaria `reqwest` mais uma
stack OAuth ao `Cargo.toml` contra a política de dependências do projeto, amarraria a um
provedor único, e faria o cloak custodiar um refresh token de longa duração — uma
credencial permanente com acesso ao backup inteiro, dentro da ferramenta cujo propósito é
isolar credenciais.

### Proteção: criptografia simétrica mais manifesto de identidade

Duas camadas, com papéis distintos.

**Criptografia — controle de acesso real.** O artefato é sempre cifrado com
`gpg --symmetric` por passphrase. É a única proteção que sobrevive à saída do arquivo da
máquina de origem. Na máquina do usuário, `/mnt/c` está montado como `9p/drvfs` sem a opção
`metadata`: o diretório do OneDrive aparece como `drwxrwxrwx` e `chmod 600` é no-op. No
destino pretendido, permissão de arquivo não protege nada.

Simétrico e não assimétrico porque o cenário é recuperação de desastre: com GPG assimétrico
a chave privada moraria na máquina a ser formatada, e sem um backup separado dela o backup
principal se torna irrecuperável. Uma passphrase memorizada ou guardada em gerenciador de
senhas sobrevive à formatação naturalmente.

**Manifesto de identidade — trava contra acidente.** Registra origem e permite ao restore
detectar divergência de perfil, conta ou usuário. Não é segurança: quem já decifrou o
artefato pode editar o manifesto. Protege contra o erro honesto de restaurar o perfil errado
sobre o perfil certo, que é o risco mais frequente.

**Credenciais ficam fora por padrão.** `claude/.credentials.json` e `codex/auth.json` são
tokens OAuth que expiram e se regeneram em minutos com `cloak login`. Enviá-los para nuvem
de terceiros é risco alto com benefício quase nulo. A flag `--include-credentials` existe
para quem aceitar o trade-off, e só opera com criptografia ativa.

### Sem dependências novas

`tar`, `gzip` e `gpg` são invocados como processos externos via `std::process::Command`.

As alternativas em crate somariam dezenas de dependências transitivas contra a política do
`CLAUDE.md`. Delegar a binários do sistema é coerente com a arquitetura do cloak, que existe
para preparar ambiente e fazer `exec` no binário real.

`gzip` e não `zstd`: o `zstd` da máquina de origem veio do linuxbrew e pode não existir na
máquina restaurada. Em um artefato de 7 MB, a diferença de compressão não justifica o risco
de um backup que não abre.

## Interface

```
cloak backup  [--profile <nome>] [--output <dir>] [--include-credentials] [--dry-run]
cloak restore <arquivo> [--profile <nome>] [--force] [--dry-run] [--no-rewrite-paths]
```

`backup` sem `--profile` inclui todos os perfis em um artefato único — o caso "vou formatar
a máquina". `--dry-run` imprime o relatório de itens incluídos e não cobertos sem gerar
arquivo.

`restore` sem `--profile` restaura todos os perfis contidos no artefato. Com `--profile`,
restaura apenas o perfil indicado, que precisa existir no artefato sob pena de erro.

`--dry-run` no `restore` decifra o artefato, valida o manifesto e imprime o plano completo —
perfis afetados, colisões, divergências de identidade e paths que seriam reescritos — sem
tocar no destino.

O diretório de saída é resolvido nesta ordem: `--output`, depois `output_dir` do
`config.toml`, depois o padrão `~/.config/cloak/backups`. O padrão é criado com `0700`
quando ausente.

A allowlist opera sobre os diretórios de perfil, não sobre o `output_dir`, então o padrão
`~/.config/cloak/backups` fica naturalmente fora do backup. Ainda assim, o cloak nunca inclui
o diretório de saída no artefato mesmo que ele seja configurado dentro de um perfil, para
evitar que cada execução engula os artefatos anteriores.

Configuração em `~/.config/cloak/config.toml`:

```toml
[backup]
output_dir = "/mnt/c/Users/junior/OneDrive/cloak-backups"
upload_command = "rclone copy {archive} gdrive:cloak/"
include = []
```

`include` acrescenta padrões à allowlist built-in; não remove nenhum built-in.

## Formato do artefato

`cloak-backup-<YYYYMMDD-HHMMSS>.tar.gz.gpg`

```
manifest.json
config.toml
profiles/<perfil>/...
```

O manifesto fica dentro do envelope cifrado. Em claro, vazaria e-mail da conta, hostname e
lista de perfis para qualquer um com posse do arquivo.

Campos: versão do formato, versão do cloak, data, hostname, uid, perfis incluídos com a
conta OAuth e os servidores MCP registrados de cada um, `profile_root` de origem, `$HOME` de
origem, itens do perfil não cobertos pela allowlist e indicação de se credenciais foram
incluídas.

## Fluxo de backup

1. Resolve perfis alvo e diretório de saída.
2. Aplica a allowlist (built-in mais `include`) e monta a lista de inclusão; em paralelo,
   registra os itens do perfil que não casaram com nenhum padrão.
3. Coleta identidade de cada perfil (`oauthAccount` do `.claude.json`) e monta o manifesto.
4. Imprime o relatório: itens incluídos e itens não cobertos, ambos com tamanhos. Em
   `--dry-run`, encerra aqui.
5. Gera o tar comprimido em diretório temporário com permissão restrita.
6. Cifra com `gpg --symmetric`, solicitando a passphrase.
7. Move o artefato para `output_dir` com `0600` e remove o intermediário.
8. Executa `upload_command`, se configurado. Falha aqui não invalida o artefato local.

## Fluxo de restauração

1. Decifra o artefato para diretório temporário com permissão restrita.
2. Lê e valida o manifesto.
3. Verifica identidade: perfil de destino, conta OAuth e uid. Divergência aborta com
   mensagem explícita e exige `--force`.
4. Verifica colisão: perfil já existente no destino nunca é sobrescrito sem `--force`.
5. Reescreve paths absolutos, salvo `--no-rewrite-paths`.
6. Copia os arquivos e reaplica `0700` em diretórios e `0600` em arquivos.
7. Imprime o relatório do que não veio no backup e será reconstruído.
8. Remove o diretório temporário.

### Reescrita de paths absolutos

Sem ela, o perfil restaurado aponta para o home da máquina antiga. Os arquivos afetados são
`plugins/installed_plugins.json` (`installPath`, `projectPath`),
`plugins/known_marketplaces.json` (`installLocation`) e o `config.toml` do cloak (caminhos de
binários). O `.claude.json` não entra no backup, então seus `projectPath` não precisam de
reescrita — o Claude os recria na primeira execução.

A substituição troca apenas as raízes exatas — `profile_root` e `$HOME` de origem — e só em
arquivos de texto (JSON, TOML, MD, SH). Cada arquivo alterado aparece no relatório.

Esta é a parte do desenho com maior chance de efeito indesejado, por ser substituição
textual em arquivos de configuração. Restringir à raiz exata e relatar cada alteração são as
mitigações; `--no-rewrite-paths` é a saída de emergência.

### Relatório de reconstrução

Após restaurar, o cloak informa o que o backup deliberadamente não carrega e como será
reconstruído. Plugins e marketplaces vêm de `installed_plugins.json` e
`known_marketplaces.json`, que estão no backup. A lista de servidores MCP vem do manifesto —
capturada do `.claude.json` no momento do backup, já que o `.claude.json` em si não é
copiado — e de `codex/config.toml`, que está no backup.

O cloak relata, não reinstala. Executar comandos de instalação de CLIs de terceiros criaria
acoplamento a superfícies que mudam sem aviso, e a quebra apareceria durante uma
restauração de desastre. O Claude Code já reconcilia plugins a partir desses arquivos na
primeira execução.

## Código

Módulo novo `src/backup.rs`, contendo backup, restauração e manifesto. O `main.rs` já tem
3506 linhas; recebe apenas o dispatch dos dois subcomandos. `src/cli.rs` recebe as
definições `clap`. `src/config.rs` recebe o bloco `[backup]`. `src/doctor.rs` recebe a
checagem de presença de `tar`, `gzip` e `gpg`.

Erros propagados com `color-eyre` e contexto explícito. Sem `unwrap`, `expect` ou `panic!`
em fluxo de usuário, conforme o `CLAUDE.md`.

## Tratamento de erros

| Situação | Comportamento |
|---|---|
| `tar`, `gzip` ou `gpg` ausente | Aborta antes de qualquer escrita, indicando o binário |
| Passphrase incorreta no restore | Erro do gpg propagado com contexto, temporário removido |
| `output_dir` inexistente ou sem permissão | Aborta antes de gerar o artefato |
| `upload_command` falha | Reporta a falha e o caminho local; exit code diferente de zero |
| Manifesto ausente ou ilegível | Aborta; não tenta adivinhar o layout |
| Identidade divergente | Aborta com comparação explícita; `--force` prossegue |
| Perfil já existe no destino | Aborta; `--force` sobrescreve |
| Interrupção no meio do restore | Temporário em diretório próprio, removido; destino só é tocado após validação |

## Testes

`tests/backup_integration.rs`, no padrão de `tests/exec_integration.rs`, sobre `tempfile`:

- Roundtrip backup e restore preservando conteúdo e permissões.
- Allowlist inclui os padrões previstos e ignora o resto.
- Relatório de não-cobertos lista item fora da allowlist com tamanho.
- `include` do config soma-se aos built-in sem removê-los.
- `--dry-run` não escreve nada.
- Credenciais ficam fora por padrão e entram com `--include-credentials`.
- Manifesto registra origem e é lido corretamente.
- Identidade divergente aborta sem `--force` e prossegue com `--force`.
- Perfil existente não é sobrescrito sem `--force`.
- Reescrita de paths corrige raízes e `--no-rewrite-paths` preserva o original.
- Ausência de `gpg` aborta antes de escrever.

Testes que dependem de `gpg` são ignorados quando o binário não está disponível no ambiente.

## Riscos conhecidos

- **Perda da passphrase torna o backup irrecuperável.** É inerente à escolha por
  criptografia simétrica e deve constar da documentação do comando.
- **Reescrita textual de paths pode alterar ocorrência não pretendida** de uma string que
  coincida com a raiz de origem. Mitigado por restrição à raiz exata e pelo relatório.
- **Allowlist pode omitir** um arquivo novo e importante que uma CLI passe a gravar fora dos
  padrões conhecidos. Mitigado pelo relatório de não-cobertos a cada execução, que obriga o
  arquivo novo a aparecer para o usuário antes que a omissão vire perda.
