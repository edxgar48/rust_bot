---
name: padroes-python-ml
description: Convenções da pasta ml/ — Python só para experimentos, leitura de datasets gerados pelo Rust, saída em perfis .toml, reprodutibilidade, pytest e vetores de conformidade. Use ao escrever ou revisar código Python, notebooks ou experimentos de ML.
user-invocable: false
paths: ["ml/**", "testes/conformidade/**", "perfis/**"]
---

# Python / ML

## Fronteira com o Rust
- Entrada: **somente** datasets exportados por `sync dataset` ou leitura
  read-only dos `.db`. Nunca recalcular features em Python para produção.
- Saída: perfis `perfis/<nome>.toml` no formato do plano. É o único artefato
  que volta para o Rust.
- Se o experimento precisar de uma feature nova, ela é especificada no
  plano e implementada no Rust primeiro.

## Estrutura
```
ml/
├── pyproject.toml        # dependências fixadas
├── rust_spider_ml/       # pacote: leitura de dataset, avaliação, busca de pesos
├── experimentos/         # scripts/notebooks numerados e datados
└── tests/                # pytest, incluindo conformidade
```

## Reprodutibilidade
- Seeds fixas; versões fixadas em `pyproject.toml`.
- Todo experimento registra: dataset (`feature_version`, último concurso),
  perfis testados, protocolo e resultado vs. acaso.
- Avaliação segue a skill `avaliacao-estatistica` (walk-forward, trancado).

## Conformidade
- Vetores em `testes/conformidade/*.json` (entrada → saída esperada).
- O mesmo arquivo é verificado em `cargo test` e `python -m pytest ml/`.
- Divergência entre Rust e Python: o Rust é a referência; corrigir o Python.
