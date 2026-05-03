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
   - Se nao procede, use `[discordo | falso positivo]`.
   - Se procede parcialmente, use `[concordo parcialmente | ...]` e escolha a acao real.
   - Se procede, use `[concordo | ...]` e escolha a acao real.
2. A implementacao precisa mudar?
   - Se sim, a categoria do item e sempre `corrigir`.
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
- `discordo`: o apontamento nao procede.
- `concordo parcialmente`: ha um problema real, mas a justificativa, severidade, escopo ou solucao proposta esta incompleta ou imprecisa.

### Acao

- `corrigir`: ha necessidade ou recomendacao concreta de alterar codigo, teste, config, contrato, comportamento, validacao, permissao, fluxo de erro ou integracao.
- `documentar`: a implementacao esta aceitavel como esta, mas falta registrar uma regra, decisao, limite, trade-off ou expectativa para outros devs.
- `falso positivo`: o apontamento nao faz sentido diante do codigo, contrato, comportamento esperado ou escopo da tarefa.
- `nada a acrescentar`: o apontamento esta correto ou neutro, mas nao gera acao util alem da analise.

## Regras Anti-Ambiguidade

- `corrigir` tem precedencia sobre `documentar`.
- Se a frase natural seria "precisa ajustar", "deveria mudar", "esta errado", "quebra", "nao cobre", "falta teste", "regrediu" ou "nao atende ao contrato", a acao e `corrigir`.
- Use `documentar` somente quando a melhor proxima acao for escrever ou atualizar documentacao, sem mudar a implementacao.
- Use `nada a acrescentar` somente quando nao houver correcao, documentacao ou decisao pendente.
- Sempre diga se compensa corrigir. Para itens `documentar`, diga se compensa documentar.

## Formato Obrigatorio

- Numere os itens.
- Traga sempre as categorias no inicio de cada item dentro de `[]`, separadas por `|`.
- Use exatamente o formato `[opiniao | acao]`.
- Depois da classificacao, explique em poucas frases por que a classificacao foi escolhida.
- Termine cada item com uma linha explicita de compensacao:
  - `Compensa corrigir: sim/nao. ...`
  - `Compensa documentar: sim/nao. ...`
  - `Compensa agir: nao. ...`

## Exemplo

```markdown
1. [concordo | corrigir] O apontamento procede porque a implementacao atual aceita estado invalido e isso altera o comportamento esperado.

Compensa corrigir: sim. A mudanca reduz risco de regressao e deve vir acompanhada de teste.

2. [concordo parcialmente | corrigir] Ha um problema real, mas nao pelo motivo indicado. O risco nao e a nomenclatura; e a falta de validacao antes de persistir.

Compensa corrigir: sim. A correcao deve focar na validacao, nao em renomear o campo.

3. [concordo | documentar] O codigo esta correto, mas a decisao de manter esse fallback precisa ficar registrada porque nao e obvia para quem mantem a integracao.

Compensa documentar: sim. A documentacao evita que alguem remova o fallback achando que e codigo morto.

4. [discordo | falso positivo] Nao procede porque o contrato ja permite esse valor e ha teste cobrindo o caso.

Compensa agir: nao. Nao ha mudanca util a fazer.
```
