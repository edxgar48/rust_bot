"""Implementação de **referência** das features, só para a conformidade.

Não é para a produção nem para os experimentos: a produção é o motor em Rust
(``src/motor/``) e os experimentos leem o dataset do ``sync dataset``. Esta
versão existe para o pytest provar, contra os vetores de
``testes/conformidade/``, que as definições de ``docs/PLANEJAMENTO.md``
("Especificação das features") dão o mesmo que o Rust.

De propósito é escrita de outro jeito que o Rust: o Rust é incremental, aqui
cada snapshot refaz tudo a partir da lista de aparições até *t−1*.

Ponto no tempo: o snapshot de *t* só recebe as aparições dos concursos
``1 … t−1``; o sorteio de *t* só entra como alvo (``saiu``) depois.
"""

from __future__ import annotations

from collections.abc import Sequence

#: Versão das definições implementadas aqui, como ``motor::FEATURE_VERSION``.
#: Sobe junto com a do Rust.
FEATURE_VERSION = 1

#: Janelas calculadas (0 = histórico inteiro), como ``motor::JANELAS``.
JANELAS = (0, 10, 50, 100)

#: Colunas de uma linha, na ordem dos JSON de conformidade.
COLUNAS_FEATURES = (
    "concurso",
    "dezena",
    "janela",
    "frequencia",
    "maior_espaco",
    "maior_espaco_ocorrencias",
    "menor_espaco",
    "menor_espaco_ocorrencias",
    "soma_variacoes",
    "espaco_final",
    "repeticoes_total",
    "repeticao_atual",
    "saiu",
)


def componentes(aparicoes: Sequence[int], t: int, janela: int) -> tuple:
    """Os nove componentes (``None`` = ``NULL``) de uma dezena no snapshot de
    ``t``, dadas todas as suas aparições em concursos ``< t`` (crescentes)."""
    if any(a >= t for a in aparicoes):
        raise ValueError(f"aparição em concurso >= {t}: vazaria o futuro")
    # Janela J em t = concursos t−J … t−1; janela 0 = 1 … t−1.
    na_janela = list(aparicoes) if janela == 0 else [a for a in aparicoes if a >= t - janela]
    frequencia = len(na_janela)
    espacos = [b - a - 1 for a, b in zip(na_janela, na_janela[1:])]

    maior = menor = None
    maior_ocorrencias = menor_ocorrencias = None
    if frequencia >= 2:
        maior, menor = max(espacos), min(espacos)
        maior_ocorrencias, menor_ocorrencias = espacos.count(maior), espacos.count(menor)
    soma_variacoes = None
    if frequencia >= 3:
        soma_variacoes = sum(abs(y - x) for x, y in zip(espacos, espacos[1:]))

    # Não é limitado pela janela: conta da última aparição de todo o histórico.
    espaco_final = (t - 1) - aparicoes[-1] if aparicoes else None

    repeticoes_total = espacos.count(0)
    # Sequência de concursos seguidos que termina em t−1, dentro da janela.
    repeticao_atual = 0
    if na_janela and na_janela[-1] == t - 1:
        repeticao_atual = 1
        i = len(na_janela) - 1
        while i > 0 and na_janela[i - 1] == na_janela[i] - 1:
            repeticao_atual += 1
            i -= 1

    return (
        frequencia,
        maior,
        maior_ocorrencias,
        menor,
        menor_ocorrencias,
        soma_variacoes,
        espaco_final,
        repeticoes_total,
        repeticao_atual,
    )


def features_de_resultados(
    resultados: Sequence[Sequence[int]], dezena_min: int, dezena_max: int
) -> list[tuple]:
    """Linhas (``COLUNAS_FEATURES``) de todos os concursos de ``resultados``
    (o concurso é a posição + 1) e do snapshot seguinte ao último, com
    ``saiu = None``. Ordenadas por (concurso, dezena, janela)."""
    aparicoes = {d: [] for d in range(dezena_min, dezena_max + 1)}
    linhas: list[tuple] = []
    for t in range(1, len(resultados) + 2):
        sorteadas = set(resultados[t - 1]) if t <= len(resultados) else None
        # 1. Snapshot com as aparições até t−1.
        for d in range(dezena_min, dezena_max + 1):
            for j in JANELAS:
                saiu = None if sorteadas is None else int(d in sorteadas)
                linhas.append((t, d, j, *componentes(aparicoes[d], t, j), saiu))
        # 2. Só agora t entra no histórico.
        if sorteadas is not None:
            for d in sorteadas:
                aparicoes[d].append(t)
    return linhas
