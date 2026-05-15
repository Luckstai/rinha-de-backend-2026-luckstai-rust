# Rinha de Backend 2026 - Luckstai Rust

Repositorio da submissao `Rust` para a Rinha de Backend 2026.

## Status

- topologia aderente a regra: `load balancer + 2 APIs`
- porta externa: `9999`
- engine vetorial propria em Rust
- baseline atual: `IVF` com indice binario proprio
- benchmark e historico em [notes/benchmark-history.md](./notes/benchmark-history.md)

## Regras da Rinha

Este repositorio segue o modelo pedido pelo evento:

- branch `main`: codigo-fonte
- branch `submission`: apenas os arquivos necessarios para o teste
- repositorio publico
- licenca MIT

Referencia oficial da organizacao:

- [docs/br/README.md](https://github.com/zanfranceschi/rinha-de-backend-2026/blob/main/docs/br/README.md)
- [docs/br/SUBMISSAO.md](https://github.com/zanfranceschi/rinha-de-backend-2026/blob/main/docs/br/SUBMISSAO.md)

## O que nao vai para o Git

Este repositorio **nao versiona**:

- `references.json.gz`
- `test-data.json`
- indices gerados localmente
- resultados de benchmark

Tudo isso pode ser sincronizado localmente a partir do repositorio oficial.

## Setup local

1. Clone tambem o repositorio oficial da Rinha.

2. Sincronize os fixtures oficiais para uso local:

```bash
RINHA_OFFICIAL_REPO_PATH=/caminho/para/rinha-de-backend-2026 \
./scripts/sync_official_fixtures.sh
```

3. Suba a stack local:

```bash
./scripts/prepare_local_test.sh
```

4. Smoke test:

```bash
./smoke.sh
```

5. Previa local:

```bash
./run.sh
```

## Estrutura

- [src](./src): API e engine vetorial
- [deploy/haproxy/haproxy.cfg](./deploy/haproxy/haproxy.cfg): load balancer
- [docker-compose.yml](./docker-compose.yml): topologia local
- [info.json](./info.json): metadados pedidos pela Rinha
- [notes/benchmark-history.md](./notes/benchmark-history.md): historico de testes

## Stack

- Rust
- Actix Web
- HAProxy
- indice vetorial proprio em memoria
