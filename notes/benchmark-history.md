# Benchmark History

Historico dos testes locais e das decisoes de performance para a submissao da Rinha de Backend 2026.

## Objetivo

Manter uma trilha audivel de:

- configuracoes testadas
- resultados medidos
- decisoes tomadas
- variacao entre execucoes

## Ambiente de referencia

- topologia: `HAProxy + 2 APIs Rust`
- porta externa: `9999`
- algoritmo atual: `ivf`
- indice atual: `fixtures/official/resources/references.n2048.s65536.i8.ivf`
- `nprobe`: `8`
- orcamento local validado:
  - `LB_CPUS=0.15`
  - `API_CPUS=0.425`
  - `APP_WORKERS=1`

## Comandos de reproducao

Smoke:

```bash
LB_CPUS=0.15 API_CPUS=0.425 APP_WORKERS=1 RINHA_ALGORITHM=ivf ./smoke.sh
```

Calibracao dentro da rede Docker:

```bash
LB_CPUS=0.15 API_CPUS=0.425 APP_WORKERS=1 RINHA_ALGORITHM=ivf \
TARGET_BASE_URL=http://lb:9999 TARGET_RPS=250 DURATION=20s \
./scripts/calibrate_in_network.sh
```

Previa oficial local:

```bash
LB_CPUS=0.15 API_CPUS=0.425 APP_WORKERS=1 RINHA_ALGORITHM=ivf ./run.sh
```

## Linha do tempo

### 1. Baseline exato

Observacao:

- o scan exato serviu como oraculo de corretude
- ele nao era competitivo em throughput/p99

Referencia offline:

- `flat`: `fp=0`, `fn=0`, `avg_us=4897.2`, `p99_us=12797`

Decisao:

- manter o exato apenas como validacao
- buscar shortlist agressivo antes de otimizar microdetalhes

### 2. Gargalo no load balancer

Calibracao a `250 rps`:

| Caso | avg | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| API direta | 0.93 ms | 3.26 ms | 1 | 1 | 0 | `test/results.calibrate.net.api1-9999.r250.json` |
| LB antigo | 281.13 ms | 1643.20 ms | 1 | 1 | 31 | `test/results.calibrate.net.lb-9999.r250.json` |
| LB com `0.10 CPU` | 6.68 ms | 174.64 ms | 1 | 1 | 0 | `test/results.calibrate.net.lb-9999.cpu010.r250.json` |
| LB com `0.15 CPU` | 1.17 ms | 4.99 ms | 1 | 1 | 0 | `test/results.calibrate.net.lb-9999.cpu015.r250.json` |

Decisao:

- o gargalo principal naquele ponto era o LB, nao a engine
- `LB_CPUS=0.15` passou a ser o baseline minimo aceitavel

### 3. Sweep de variacoes do IVF

Previa oficial local:

| Variante | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `ivf4` com tuning inicial | 3590.30 | 86.25 ms | 7 | 10 | 0 | `test/results.ivf4.lb015.json` |
| `1024 / 65536 / 12` | 3632.97 | 78.18 ms | 7 | 10 | 0 | `test/results.references.n1024.s65536.i12.ivf.json` |
| `2048 / 65536 / 8` | 3774.51 | 56.44 ms | 7 | 10 | 0 | `test/results.references.n2048.s65536.i8.ivf.json` |

Decisao:

- promover `references.n2048.s65536.i8.ivf` como candidato principal

### 4. Sweep de `nprobe`

Mesmo indice `2048 / 65536 / 8`:

| `nprobe` | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 6 | 3955.86 | 42.15 ms | 9 | 5 | 0 | `test/results.references.n2048.s65536.i8.nprobe6.json` |
| 7 | 3889.15 | 51.06 ms | 6 | 5 | 0 | `test/results.references.n2048.s65536.i8.nprobe7.json` |
| 8 | 4009.13 | 39.86 ms | 7 | 4 | 0 | `test/results.references.n2048.s65536.i8.nprobe8.json` |

Decisao:

- `nprobe=8` virou o melhor ponto local
- apesar de custar um pouco mais no scorer, o ganho liquido no score final compensou

### 5. Tentativa de IVF particionado

Hipotese:

- reduzir o espaco de busca antes do IVF usando bits fortes do proprio vetor
- `p3`: `last_transaction` ausente, `is_online`, `card_present`
- `p4`: `p3` + `unknown_merchant`

Indices gerados:

- `fixtures/official/resources/references.n2048.s65536.i8.p3.ivf`
- `fixtures/official/resources/references.n2048.s65536.i8.p4.ivf`

Benchmark offline em `10k` requests:

| Variante | `nprobe` | fp | fn | avg_us | p99_us | Observacao |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| baseline `references.n2048.s65536.i8.ivf` | 8 | 1 | 0 | 83.2 | 377 | melhor recall offline |
| `p3` | 8 | 1 | 3 | 92.1 | 611 | piorou recall e latencia |
| `p4` | 8 | 1 | 1 | 86.0 | 380 | melhor que `p3`, pior que baseline |
| `p4` | 6 | 1 | 1 | 68.6 | 431 | menor media, mas ainda pior que baseline em recall |

Arquivos de apoio:

- `/tmp/rinha_compare_p3.out`
- `/tmp/rinha_compare_p4.out`
- `/tmp/rinha_compare_baseline_n8.out`
- `/tmp/rinha_compare_p4_n6.out`

Previa oficial local do melhor candidato particionado:

| Variante | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `p4`, `nprobe=6` | 3568.10 | 113.59 ms | 8 | 3 | 0 | `test/results.references.n2048.s65536.i8.p4.nprobe6.json` |

Decisao:

- rejeitar o IVF particionado como default
- o indice atual sem particionamento continua superior no score final
- manter o experimento registrado porque ele mostra que fragmentar o espaco antes do IVF piorou a qualidade global nesta base

### 6. Retreino com `k-means++`

Motivacao:

- a documentacao do Faiss recomenda `k-means` para treinar o quantizador, com regra pratica de ate `20` iteracoes
- a FAQ do Faiss indica zona confortavel de treino entre `39 * k` e `256 * k` pontos por centroide, e cita que acima de `20` iteracoes normalmente nao ha ganho consistente
- o paper de `k-means++` mostra que um seeding cuidadoso costuma melhorar a qualidade do clustering

Mudancas:

- builder do IVF passou a usar `k-means++` para inicializacao
- geramos tres indices sem particionamento:
  - `references.n2048.s65536.i8.kpp.ivf`
  - `references.n2048.s131072.i12.kpp.ivf`
  - `references.n2048.s262144.i16.kpp.ivf`

Benchmark offline em `10k` requests com `nprobe=8`:

| Variante | fp | fn | avg_us | p99_us | Leitura |
| --- | ---: | ---: | ---: | ---: | --- |
| baseline `references.n2048.s65536.i8.ivf` | 1 | 0 | 83.2 | 377 | referencia |
| `references.n2048.s65536.i8.kpp.ivf` | 0 | 1 | 111.0 | 471 | melhorou tipo de erro, piorou latencia |
| `references.n2048.s131072.i12.kpp.ivf` | 2 | 1 | 109.8 | 561 | pior que baseline |
| `references.n2048.s262144.i16.kpp.ivf` | 1 | 1 | 118.0 | 469 | pior que baseline |

Arquivos de apoio:

- `/tmp/compare_n2048_s65536_i8_kpp.out`
- `/tmp/compare_n2048_s131072_i12_kpp.out`
- `/tmp/compare_n2048_s262144_i16_kpp.out`

Sweep de `nprobe` para `references.n2048.s65536.i8.kpp.ivf`:

| `nprobe` | fp | fn | avg_us | p99_us | Arquivo |
| --- | ---: | ---: | ---: | ---: | --- |
| 5 | 0 | 4 | 76.5 | 546 | `/tmp/compare_n2048_s65536_i8_kpp_nprobe5.out` |
| 6 | 0 | 2 | 95.9 | 502 | `/tmp/compare_n2048_s65536_i8_kpp_nprobe6.out` |
| 7 | 0 | 2 | 94.3 | 382 | `/tmp/compare_n2048_s65536_i8_kpp_nprobe7.out` |
| 8 | 0 | 1 | 106.9 | 385 | `/tmp/compare_n2048_s65536_i8_kpp_nprobe8.out` |

Previa oficial local:

| Variante | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `kpp`, `nprobe=8` | 3958.08 | 47.94 ms | 3 | 4 | 0 | `test/results.references.n2048.s65536.i8.kpp.nprobe8.json` |
| `kpp`, `nprobe=7` | 642.61 | 1283.41 ms | 4 | 6 | 216 | `test/results.references.n2048.s65536.i8.kpp.nprobe7.json` |

Decisao:

- manter o indice default atual, sem `k-means++`, como vencedor
- o retreino com `k-means++` melhorou alguns perfis de erro, mas nao superou o score final do baseline atual
- `nprobe=7` mostrou um risco importante: microbench bom nao garante estabilidade sob a carga oficial

### 7. Microajuste de runtime no hot path

Hipotese:

- trocar pequenas alocacoes dinamicas no seletor de listas do IVF por estruturas fixas em stack
- reduzir branches e custo de manutencao do `top-k` local no scorer

Resultado da previa oficial local:

| Variante | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| baseline atual antes do ajuste | 4009.13 | 39.86 ms | 7 | 4 | 0 | `test/results.references.n2048.s65536.i8.nprobe8.json` |
| `poststack`, `nprobe=8` | 3537.91 | 117.97 ms | 7 | 4 | 0 | `test/results.references.n2048.s65536.i8.poststack.nprobe8.json` |

Decisao:

- rejeitar o microajuste como default
- a mudanca nao alterou a qualidade de deteccao, mas piorou fortemente o `p99` no benchmark oficial local
- o codigo foi revertido para voltar ao baseline vencedor

### 8. Validacao pos-revert e variancia local

Objetivo:

- confirmar que a reversao do `k-means++` e do microajuste de runtime devolveu o projeto ao melhor caminho conhecido
- medir a dispersao do benchmark oficial local apos a reversao

Resultados:

| Teste | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| calibracao `250 rps` | n/a | 5.43 ms | 1 | 0 | 0 | `test/results.verify.calibrate.lb250.revertcheck.json` |
| previa oficial local, rodada 1 | 3431.26 | 150.81 ms | 7 | 4 | 0 | `test/results.references.n2048.s65536.i8.revertcheck.nprobe8.json` |
| previa oficial local, rodada 2 | 4100.27 | 32.32 ms | 7 | 4 | 0 | `test/results.references.n2048.s65536.i8.revertcheck2.nprobe8.json` |

Leitura:

- a calibracao curta confirmou que o caminho HTTP permaneceu saudavel apos a reversao
- as duas rodadas longas exibiram variancia relevante no ambiente local, apesar da mesma configuracao e do mesmo `fp/fn`
- a segunda rodada pos-revert virou o melhor score observado ate agora

Decisao:

- manter o baseline atual sem `k-means++` e sem o microajuste `poststack`
- considerar a previa oficial local longa como teste sujeito a ruido operacional do host
- preservar calibracao curta e multiplas rerodadas como mecanismo de confirmacao antes de aceitar qualquer regressao

### 9. Experimento `ivf-pure-gate`

Hipotese:

- usar o proprio IVF vencedor como oraculo barato para um gate
- se a lista mais proxima for pura (`100% legit` ou `100% fraud`) e a margem para a segunda lista for suficiente, responder sem scan dos registros
- fallback para o `ivf` normal nos casos ambiguos

Implementacao:

- nova variante de algoritmo: `ivf-pure-gate`
- parametro: `RINHA_IVF_GATE_MARGIN`
- o gate so dispara para listas puras e com `margin_ratio >= 1.02`

Microbench:

| Variante | fp | fn | avg_us | p50_us | p95_us | p99_us |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `ivf` | 1 | 0 | 78.9 | 74 | 119 | 169 |
| `ivf-pure-gate` | 1 | 0 | 16.6 | 3 | 88 | 125 |

Benchmark do caminho completo (`JSON -> parse -> score -> encode`):

| Variante | score avg_us | score p95_us | score p99_us | total avg_us | total p99_us |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ivf-pure-gate` | 15.7 | 84 | 121 | 16.4 | 122 |

Previa oficial local:

| Variante | workers | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `ivf-pure-gate`, `margin=1.02` | 1 | 3656.76 | 91.12 ms | 6 | 4 | 0 | `test/results.references.n2048.s65536.i8.pure-gate.nprobe8.margin102.json` |
| `ivf-pure-gate`, `margin=1.02` | 1 | 2965.62 | 447.46 ms | 6 | 4 | 0 | `test/results.references.n2048.s65536.i8.pure-gate.nprobe8.margin102.rerun.json` |

Calibracao curta:

| Variante | workers | avg | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `ivf-pure-gate`, `margin=1.02` | 1 | 3.44 ms | 82.30 ms | 1 | 0 | 0 | `test/results.pure-gate.calibrate.lb250.json` |
| `ivf-pure-gate`, `margin=1.02` | 2 | 3.87 ms | 74.01 ms | 1 | 0 | 0 | `test/results.pure-gate.calibrate.w2.lb250.json` |

Decisao:

- rejeitar `ivf-pure-gate` como default
- o microbench melhorou fortemente, mas esse ganho nao apareceu de forma confiavel na previa oficial local
- `APP_WORKERS=2` nao resolveu o rabo de latencia
- manter o `ivf` puro como baseline vencedor

### 10. Experimento `ivf-adaptive`

Hipotese:

- manter o mesmo indice vencedor
- usar `nprobe` menor so quando a separacao entre os dois melhores centroides for suficiente
- fallback para `nprobe=8` nos casos ambiguos

Implementacao:

- nova variante de algoritmo: `ivf-adaptive`
- parametros:
  - `RINHA_IVF_LOW_NPROBE`
  - `RINHA_IVF_ADAPTIVE_MARGIN`
- tambem adicionamos `BENCH_ALGORITHMS` ao [src/bin/bench_algorithms.rs](/Users/lucas/projects/rinha-de-backend-2026-luckstai-rust/src/bin/bench_algorithms.rs) para rodar benchmark offline por algoritmo isolado

Offline em `10k` requests:

| Variante | fp | fn | avg_us | p50_us | p95_us | p99_us |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `ivf` | 1 | 0 | 69.0 | 67 | 99 | 125 |
| `ivf-adaptive`, `low=4`, `margin=1.10` | 1 | 1 | 43.8 | 40 | 74 | 89 |
| `ivf-adaptive`, `low=6`, `margin=1.20` | 1 | 0 | 60.6 | 56 | 89 | 128 |

Offline em `54.100` requests usando `BENCH_ALGORITHMS`:

| Variante | fp | fn | avg_us | p50_us | p95_us | p99_us |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `ivf` | 7 | 4 | 75.3 | 69 | 119 | 198 |
| `ivf-adaptive`, `low=6`, `margin=1.20` | 6 | 4 | 62.0 | 58 | 96 | 149 |

Benchmark do caminho completo:

| Variante | score avg_us | score p95_us | score p99_us | total avg_us | total p99_us |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ivf-adaptive`, `low=6`, `margin=1.20` | 60.6 | 90 | 120 | 61.5 | 121 |

Calibracao curta:

| Variante | workers | target_rps | avg | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `ivf-adaptive`, `low=6`, `margin=1.20` | 1 | 250 | 2.21 ms | 19.00 ms | 1 | 0 | 0 | `test/results.adaptive.low6.margin12.calibrate.lb250.json` |
| `ivf-adaptive`, `low=6`, `margin=1.20` | 1 | 350 | 2.70 ms | 58.71 ms | 1 | 1 | 0 | `test/results.adaptive.low6.margin12.w1.r350.json` |
| `ivf-adaptive`, `low=6`, `margin=1.20` | 2 | 350 | 3.22 ms | 66.15 ms | 1 | 1 | 0 | `test/results.adaptive.low6.margin12.w2.r350.json` |

Previa oficial local:

| Variante | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `ivf-adaptive`, `low=6`, `margin=1.20` | 3516.33 | 109.78 ms | 11 | 6 | 0 | `test/results.references.n2048.s65536.i8.adaptive.low6.margin12.json` |
| `ivf-adaptive`, `low=6`, `margin=1.20`, rerun | 3532.85 | 105.68 ms | 11 | 6 | 0 | `test/results.references.n2048.s65536.i8.adaptive.low6.margin12.v2.json` |

Leitura:

- no offline completo, o `adaptive` parece melhor que o `ivf` puro
- no caminho HTTP curto, ele tambem ficou saudavel
- mas na previa oficial local longa ele segue piorando score e deteccao
- a propria previa longa mostrou saturacao de VUs em rodadas recentes; isso significa que o score final passa a refletir so o subconjunto efetivamente executado, nao toda a massa de `54.100`

Decisao:

- nao promover `ivf-adaptive` como default
- manter o `ivf` puro como baseline vencedor ate aparecer uma variante que ganhe no benchmark oficial local, nao apenas no offline
- usar o harness offline por algoritmo isolado para filtrar candidatos antes de gastar mais uma previa longa

## Validacao limpa de 2026-05-13

Essa rodada foi rerodada depois do ajuste no `smoke`, para registrar artefatos limpos e reproduziveis.

Resultados:

| Teste | Resultado | Arquivo |
| --- | --- | --- |
| Smoke | 5/5 iteracoes, `http_req_failed=0`, `avg=1.75 ms`, `p95=5.32 ms` | `test/smoke.js` + `/tmp/rinha_smoke_verify.out` |
| Calibracao `250 rps` | `avg=0.96 ms`, `p99=5.70 ms`, `fp=1`, `fn=0`, `http_errors=0` | `test/results.verify.calibrate.lb250.default.json` |
| Previa oficial local | `final_score=3965.37`, `p99=44.09 ms`, `fp=7`, `fn=4`, `http_errors=0` | `test/results.verify.default.json` |

Leitura:

- a rerodada ficou um pouco abaixo do melhor pico anterior (`4009.13`)
- ainda assim confirmou melhora clara sobre todos os candidatos anteriores ao `nprobe=8`
- a variacao entre `39.86 ms` e `44.09 ms` de `p99` parece ruido operacional normal do ambiente local

## Campanha de 2026-05-18

Objetivo:

- revalidar a variancia do baseline atual no host
- testar um sweep pequeno de `nprobe` acima de `8`
- validar se `workers=2` ou uma pequena realocacao de CPU ajudariam o melhor candidato

Baseline repetido com a configuracao atual (`ivf`, `nprobe=8`, `LB=0.15`, `API=0.425`, `workers=1`):

| Run | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | 5173.81 | 2.73 ms | 7 | 4 | 0 | `test/results.baseline.ivf.nprobe8.lb015.api0425.w1.run1.json` |
| 2 | 4702.18 | 8.08 ms | 7 | 4 | 0 | `test/results.baseline.ivf.nprobe8.lb015.api0425.w1.run2.json` |
| 3 | 4683.49 | 8.44 ms | 7 | 4 | 0 | `test/results.baseline.ivf.nprobe8.lb015.api0425.w1.run3.json` |

Sweep de `nprobe` no mesmo indice `references.n2048.s65536.i8.ivf`:

| `nprobe` | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 9 | 3929.73 | 49.40 ms | 5 | 4 | 0 | `test/results.ivf.nprobe9.may18.json` |
| 10 | 4207.99 | 27.49 ms | 5 | 3 | 0 | `test/results.ivf.nprobe10.may18.json` |
| 12 | 4988.85 | 4.31 ms | 5 | 4 | 0 | `test/results.ivf.nprobe12.may18.json` |

Rerodada do melhor candidato (`nprobe=12`):

| Run | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | 5059.84 | 3.66 ms | 5 | 4 | 0 | `test/results.ivf.nprobe12.lb015.api0425.w1.run1.json` |
| 2 | 4818.25 | 6.39 ms | 5 | 4 | 0 | `test/results.ivf.nprobe12.lb015.api0425.w1.run2.json` |

Variantes de runtime/CPU em cima do `nprobe=12`:

| Variante | final_score | p99 | fp | fn | http_errors | Arquivo |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `workers=2`, `LB=0.15`, `API=0.425` | 3787.62 | 68.52 ms | 5 | 4 | 0 | `test/results.ivf.nprobe12.lb015.api0425.w2.may18.json` |
| `workers=1`, `LB=0.125`, `API=0.4375` | 2806.81 | 655.56 ms | 5 | 4 | 0 | `test/results.ivf.nprobe12.lb0125.api04375.w1.may18.json` |

Leitura:

- o host local ficou claramente mais favoravel do que nas rodadas registradas em `2026-05-13`, com `p99` muito menor e score final bem mais alto
- `nprobe=9` e `nprobe=10` melhoraram um pouco a deteccao, mas perderam feio em `p99`
- `nprobe=12` foi o primeiro ponto acima de `8` que melhorou score final sem sacrificar latencia
- `workers=2` continua ruim para esse orcamento
- reduzir o LB para `0.125 CPU` foi insuficiente; houve saturacao de VUs e `p99` desabou

Decisao:

- promover `nprobe=12` como melhor candidato atual no host local
- manter `APP_WORKERS=1`
- manter `LB_CPUS=0.15` e `API_CPUS=0.425`
- tratar os numeros de `2026-05-18` como nova referencia local ate prova em contrario
- se houver novo ciclo de tuning, partir deste baseline antes de explorar bibliotecas externas

## Validacao limpa de 2026-05-18 com default promovido

Objetivo:

- promover `RINHA_IVF_NPROBE=12` como default do projeto
- validar a stack sem overrides especificos de algoritmo
- confirmar que o candidato segue saudavel como versao de submissao

Mudancas:

- `src/config.rs`: default de `ivf_nprobe` promovido de `8` para `12`
- `docker-compose.yml`: default de `RINHA_IVF_NPROBE` promovido de `8` para `12`

Resultados:

| Teste | Resultado | Arquivo |
| --- | --- | --- |
| Smoke | 5/5 iteracoes, `http_req_failed=0`, `avg=1.65 ms`, `p95=2.66 ms` | `test/smoke.js` |
| Previa oficial local | `final_score=4673.75`, `p99=8.91 ms`, `fp=5`, `fn=4`, `http_errors=0` | `test/results.validate.clean.default-nprobe12.2026-05-18.json` |
| `cargo test` em container | 5 testes passando, 0 falhas | `scripts/test_in_docker.sh` |

Leitura:

- a promocao do default preservou o perfil de erro forte do `nprobe=12`
- esta rodada ficou abaixo dos melhores picos observados no mesmo dia, mas ainda bem acima das validacoes antigas do baseline com `nprobe=8`
- o comportamento permaneceu compativel com submissao: sem erros HTTP, com smoke limpo e testes Rust passando

Decisao:

- manter `nprobe=12` como default do projeto
- usar esta validacao limpa como referencia operacional para preparar a submissao

## Melhor configuracao atual

Usar:

```bash
LB_CPUS=0.15
API_CPUS=0.425
APP_WORKERS=1
RINHA_ALGORITHM=ivf
RINHA_IVF_INDEX_PATH=/app/fixtures/resources/references.n2048.s65536.i8.ivf
RINHA_IVF_NPROBE=12
```

Arquivos de referencia:

- melhor score observado: `test/results.baseline.ivf.nprobe8.lb015.api0425.w1.run1.json`
- melhor candidato atual: `test/results.ivf.nprobe12.lb015.api0425.w1.run1.json`
- rerodada confirmatoria do candidato atual: `test/results.ivf.nprobe12.lb015.api0425.w1.run2.json`
- validacao limpa com default promovido: `test/results.validate.clean.default-nprobe12.2026-05-18.json`
- ultima calibracao limpa: `test/results.verify.calibrate.lb250.revertcheck.json`

## Proximos passos

- voltar para variantes do IVF sem fragmentacao por dominio
- avaliar refinamentos de runtime no scorer e na escolha de listas, sem mexer na topologia vencedora
- avaliar uma trilha alternativa de surrogate/hibrido so se o ganho vier com erro controlado
- repetir a previa oficial local apos cada mudanca estrutural
- manter este arquivo atualizado com cada decisao que altere score, p99 ou taxa de erro
