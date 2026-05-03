# Template Do Comando De Opinar

Use este template quando o prompt pedir apenas uma opiniao sobre apontamentos de outro agente, sem implementar alteracoes.

## Regra De Entrada

- Quando o prompt iniciar com `OPINE`, apenas opine sobre o conteudo; nao implemente.
- `OPINE` e o gatilho unico esperado em `AGENTS.md` e `CLAUDE.md`.

## Objetivo

Classifique cada apontamento de forma acionavel. A categoria deve responder:
"qual e a proxima acao correta se alguem aceitar esta opiniao?".

## Ordem De Decisao

Para cada item, decida nesta ordem:

1. O apontamento procede?
   - Se o diagnostico nao procede, use `discordo`; escolha a acao conforme exista ou nao outro problema proximo a corrigir.
   - Se procede parcialmente, use `[concordo parcialmente | ...]` e escolha a acao real.
   - Se procede, use `[concordo | ...]` e escolha a acao real.
2. A implementacao precisa mudar?
   - Se sim, a categoria do item e sempre `corrigir` e a severidade (`bloqueante`, `relevante` ou `polimento`) deve aparecer no terceiro campo do cabecalho.
   - Use `corrigir` mesmo que tambem valha documentar depois.
   - Se houver duvida entre `corrigir` e `documentar`, e qualquer codigo, teste, config, contrato ou comportamento precisar mudar, use `corrigir`.
3. O codigo esta correto, mas falta registrar uma decisao, restricao, trade-off ou contrato para evitar erro futuro?
   - Se sim, use `documentar`.
   - Nao use `documentar` para mascarar bug, regressao, comportamento incompleto ou implementacao desalinhada.
4. O apontamento procede, mas nao pede mudanca de codigo nem documentacao?
   - Use `nada a acrescentar`.

## Categorias

### Opiniao

- `concordo`: o apontamento esta correto no essencial.
- `discordo`: o diagnostico ou justificativa do apontamento nao procede.
- `concordo parcialmente`: ha um problema real, mas a justificativa, severidade, escopo ou solucao proposta esta incompleta ou imprecisa.

### Acao

- `corrigir`: ha necessidade ou recomendacao concreta de alterar codigo, teste, config, contrato, comportamento, validacao, permissao, fluxo de erro ou integracao.
- `documentar`: a implementacao esta aceitavel como esta, mas falta registrar uma regra, decisao, limite, trade-off ou expectativa para outros devs.
- `falso positivo`: o apontamento nao faz sentido diante do codigo, contrato, comportamento esperado ou escopo da tarefa.
- `nada a acrescentar`: o apontamento procede, sendo informativo, neutro ou ja atendido, mas nao gera proxima acao alem da analise.

## Regras Anti-Ambiguidade

- `corrigir` tem precedencia sobre `documentar`.
- Se um apontamento contem subproblemas com acoes distintas, desmembre em itens numerados separados seguindo o padrao de numeracao definido em "Formato Obrigatorio"; nao esconda uma acao dentro de outra.
- Se a frase natural seria "precisa ajustar", "deveria mudar", "esta errado", "quebra", "nao cobre", "falta teste", "regrediu" ou "nao atende ao contrato", a acao e `corrigir`.
- Use `[discordo | corrigir | <severidade>]` quando o diagnostico do apontamento estiver errado, mas a analise revelar outro problema proximo que realmente deve ser corrigido.
- Use `documentar` somente quando a melhor proxima acao for escrever ou atualizar documentacao, sem mudar a implementacao.
- Use `nada a acrescentar` para apontamentos corretos mas inacionaveis, confirmacoes, observacoes ja atendidas ou itens neutros, desde que nao haja correcao, documentacao ou decisao pendente.
- Se o apontamento for vago ou faltar contexto para classificar, peca esclarecimento antes de opinar; nao classifique no escuro.
- Toda opiniao termina com a linha de compensacao definida em "Formato Obrigatorio".
- Calibracao anti-vies: em rodadas com pelo menos quatro itens, se mais de 70% deles tiverem acao `corrigir` (independente da opiniao: `concordo`, `concordo parcialmente` ou `discordo`), releia. Provavelmente voce esta tratando tudo como correcao por inercia. Reabra os itens mais fracos e teste se sustentam evidencia verificavel. Em rodadas curtas (1 a 3 itens) a regra nao se aplica para evitar falso alerta.
- Encerramento de rodada: se voce so encontrar ajustes marginais sem evidencia verificavel ou sem valor operacional (reescrita estetica, preferencia pessoal, comentario sobre o proprio output), responda apenas com a linha `Nada a acrescentar nesta rodada.` no lugar da lista numerada. Essa linha substitui o "Formato Obrigatorio" inteiro: nao produza itens, classificacao ou linha de compensacao. Achados de `polimento` legitimos (renomeacao, mensagem de erro, formatacao) com evidencia concreta continuam validos como `[... | corrigir | polimento]` e nao caem nessa regra.

## Formato Obrigatorio

- Numere os itens.
- Preserve a numeracao original dos apontamentos; quando o original nao for numerado, atribua numeros sequenciais e mantenha-os estaveis na resposta.
- Se precisar desmembrar um item composto, use sufixos como `3a`, `3b`.
- Nao reordene por severidade, arquivo ou tema quando isso quebrar a rastreabilidade com os apontamentos originais.
- Traga sempre as categorias no inicio de cada item dentro de `[]`, separadas por `|`.
- Use exatamente o formato `[opiniao | acao]` para acoes sem severidade e `[opiniao | acao | severidade]` quando a acao for `corrigir`.
- Severidade e obrigatoria para `corrigir` e omitida para `documentar`, `falso positivo` e `nada a acrescentar`. Valores:
  - `bloqueante`: regressao, perda de dado, risco de seguranca, contrato quebrado em fluxo critico.
  - `relevante`: bug funcional fora de fluxo critico, lacuna de teste em comportamento exposto, debito que multiplica em manutencao.
  - `polimento`: mensagem de erro, naming, organizacao, comentario, formatacao.
- Depois da classificacao, explique em poucas frases por que a classificacao foi escolhida.
- Cite uma evidencia concreta que sustenta a classificacao, escolhendo o formato mais apropriado para o caso: caminho do arquivo + intervalo de linhas, nome de simbolo, nome de teste, trecho de contrato, comando reproduzivel, saida de teste ou execucao, schema, comportamento observado, ou ausencia de simbolo/arquivo onde o apontamento o esperaria. Se nenhuma evidencia puder ser verificada, nao classifique o item; aplique a regra de "peca esclarecimento antes de opinar" definida em "Regras Anti-Ambiguidade".
- A opiniao responde apenas aos apontamentos recebidos. Achados proprios fora desse escopo nao entram na lista numerada; vao em uma secao opcional `## Fora Do Escopo` ao final, com no maximo tres bullets, sem classificacao.
- Separe a explicacao da linha de compensacao com uma linha em branco.
- Termine cada item com a linha de compensacao que corresponde a acao:
  - `corrigir`: `Compensa corrigir: sim/nao. ...`
  - `documentar`: `Compensa documentar: sim/nao. ...`
  - `falso positivo` ou `nada a acrescentar`: `Compensa agir: nao. ...`

## Exemplo

```markdown
1. [concordo | corrigir | bloqueante] O apontamento procede porque `save_profile` em `src/profile.rs:148-176` aceita `name = ""` e grava o `.cloak` com perfil vazio, e o teste `profile::tests::rejects_empty_name` cobre apenas o caso `None`, nao a string vazia.

Compensa corrigir: sim. A mudanca reduz risco de regressao e deve vir acompanhada de teste para a string vazia.

2. [concordo parcialmente | corrigir | relevante] Ha um problema real em `config::load_agent_permissions` (`src/config.rs:412-456`), mas nao pelo motivo indicado: o risco nao e a nomenclatura do campo `allow_shell`; e a ausencia de validacao antes de persistir, que deixa entrar valor fora do enum.

Compensa corrigir: sim. A correcao deve focar na validacao em `config.rs:430`, nao em renomear o campo.

3. [concordo | documentar] O codigo em `src/exec.rs:88-104` esta correto, mas a decisao de manter o fallback de `PWD` logico precisa ficar registrada (`docs/pt-br/exec.md`) porque nao e obvia para quem mantem a integracao Cursor/WSL.

Compensa documentar: sim. A documentacao evita que alguem remova o fallback achando que e codigo morto; o teste `exec_prefers_logical_pwd_for_profile_resolution` ja exercita o comportamento.

4. [discordo | falso positivo] O contrato declarado em `docs/pt-br/profiles.md` ja permite o valor `default` como nome de perfil, e o teste `profile::tests::accepts_default_as_profile_name` cobre o caso, entao o apontamento nao procede.

Compensa agir: nao. Nao ha mudanca util a fazer.

5. [discordo | corrigir | relevante] O diagnostico indicado nao procede: o problema nao esta no parser TOML em `src/config.rs:201`. Ainda assim, ha um bug proximo na validacao posterior em `src/config.rs:268-291` que permite salvar `AgentPermissions` com `allowed_commands` e `deny_commands` sobrepostos quando o usuario edita o arquivo a mao.

Compensa corrigir: sim. A correcao deve mirar a validacao em `config.rs:268-291`, nao o parser citado no apontamento.
```

Quando a opiniao incluir achados proprios fora dos apontamentos recebidos, adicione ao final:

```markdown
## Fora Do Escopo

- Observacao 1 (livre, sem classificacao).
- Observacao 2.
- Observacao 3.
```
