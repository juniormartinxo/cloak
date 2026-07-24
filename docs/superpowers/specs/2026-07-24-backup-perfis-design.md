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
| Após denylist ingênua | 367 MB |
| Após denylist refinada | **7,1 MB** |

Os 360 MB de diferença entre as duas denylists vinham de três fontes, todas reproduzíveis:
um venv Python em `gojunior/claude/security/agent-sdk-venv` (228 MB), um repositório git em
`amjr/codex/.tmp/plugins/.git` (20 MB) e `codex/history.jsonl` (2,9 MB).

### Seleção por denylist, com relatório

O cloak copia tudo do perfil exceto padrões reconhecidos como descartáveis.

A alternativa — allowlist built-in por CLI — foi recusada porque falha em silêncio: quando
uma CLI de terceiro introduz um arquivo relevante, ele fica de fora do backup e o usuário
só descobre no momento da restauração, que é exatamente o pior momento possível.

A denylist tem o defeito oposto (inchar sem aviso), mitigado pelo relatório obrigatório de
inclusão e exclusão com tamanhos. Foi esse mecanismo que revelou o venv de 228 MB durante o
próprio desenho da feature.

Denylist inicial:

```
*/cache          */sessions        */projects       */plugins/cache
*/plugins/marketplaces             */file-history   */shell-snapshots
*/shell_snapshots                  */paste-cache    */session-env
*/node_modules   */generated_images                 */logs
*.sqlite*        *venv             */site-packages  */.git
*/.tmp           history.jsonl     codex.backup
```

Extensível por `exclude` em `[backup]` no `config.toml`.

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
a máquina". `--dry-run` imprime o relatório de inclusão e exclusão sem gerar arquivo.

`restore` sem `--profile` restaura todos os perfis contidos no artefato. Com `--profile`,
restaura apenas o perfil indicado, que precisa existir no artefato sob pena de erro.

`--dry-run` no `restore` decifra o artefato, valida o manifesto e imprime o plano completo —
perfis afetados, colisões, divergências de identidade e paths que seriam reescritos — sem
tocar no destino.

O diretório de saída é resolvido nesta ordem: `--output`, depois `output_dir` do
`config.toml`, depois o padrão `~/.config/cloak/backups`. O padrão é criado com `0700`
quando ausente.

O diretório de backup é sempre excluído do próprio backup, independentemente de configuração.
Sem isso, o padrão `~/.config/cloak/backups` faria cada execução engolir os artefatos
anteriores, com crescimento exponencial.

Configuração em `~/.config/cloak/config.toml`:

```toml
[backup]
output_dir = "/mnt/c/Users/junior/OneDrive/cloak-backups"
upload_command = "rclone copy {archive} gdrive:cloak/"
exclude = []
```

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
conta OAuth de cada um, `profile_root` de origem, `$HOME` de origem, exclusões aplicadas e
indicação de se credenciais foram incluídas.

## Fluxo de backup

1. Resolve perfis alvo e diretório de saída.
2. Aplica a denylist e monta a lista de inclusão.
3. Coleta identidade de cada perfil (`oauthAccount` do `.claude.json`) e monta o manifesto.
4. Imprime o relatório de inclusão e exclusão com tamanhos. Em `--dry-run`, encerra aqui.
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
`.claude.json` (51 chaves em `projects`), `plugins/installed_plugins.json` (`installPath`,
`projectPath`), `plugins/known_marketplaces.json` (`installLocation`) e o `config.toml` do
cloak (caminhos de binários).

A substituição troca apenas as raízes exatas — `profile_root` e `$HOME` de origem — e só em
arquivos de texto (JSON, TOML, MD, SH). Cada arquivo alterado aparece no relatório.

Esta é a parte do desenho com maior chance de efeito indesejado, por ser substituição
textual em arquivos de configuração. Restringir à raiz exata e relatar cada alteração são as
mitigações; `--no-rewrite-paths` é a saída de emergência.

### Relatório de reconstrução

Após restaurar, o cloak informa o que o backup deliberadamente não carrega e como será
reconstruído, lendo os manifestos já presentes nos arquivos de configuração restaurados:
plugins e marketplaces de `installed_plugins.json` e `known_marketplaces.json`, servidores
MCP de `.claude.json` e `codex/config.toml`.

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
- Denylist exclui os padrões previstos e o relatório lista o excluído.
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
- **Denylist pode inchar** quando uma CLI passar a gerar artefato grande fora dos padrões
  conhecidos. Mitigado pelo relatório de tamanhos a cada execução.
