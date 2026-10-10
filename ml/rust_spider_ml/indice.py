"""Índice de força refeito em Python a partir das features do dataset.

Reproduz ``src/motor/indice.rs`` (``docs/PLANEJAMENTO.md``, "Índice de
força", decisões de 2026-10-04), operação por operação, para dar os mesmos
``f64``:

- normalização **por concurso**, entre as dezenas do concurso (nunca com o
  histórico: vazaria o futuro);
- z-score com desvio populacional (÷N); minmax ``(x − mín)/(máx − mín)``;
  estatísticas só com as dezenas com valor; ``NULL`` recebe o neutro (0 no
  z-score, 0,5 no minmax); componente sem variação → todas no neutro;
- total = Σ pesoₖ × normalizadoₖ na ordem fixa dos 9 componentes;
- total arredondado a 1e-9 (meio para longe do zero, como ``f64::round``),
  sem ``-0.0``; posição pelo total decrescente, empate → menor dezena;
- variação de posição = posição no concurso anterior − posição atual
  (positivo = subiu); o primeiro concurso não tem.

Ponto no tempo: o índice de *t* usa só as features de *t* (que olham até
*t−1*); ``saiu`` não é lido.
"""

from __future__ import annotations

from collections.abc import Sequence

import numpy as np

ESCALA_TOTAL = 1e9


class ErroIndice(Exception):
    """Pesos ou features que não permitem calcular o índice."""


def normalizar(valores: np.ndarray, normalizacao: str) -> np.ndarray:
    """Normaliza ``float64[T, N]`` (``NaN`` = ``NULL``) linha a linha."""
    if normalizacao == "zscore":
        neutro = 0.0
    elif normalizacao == "minmax":
        neutro = 0.5
    else:
        raise ErroIndice(f"normalização desconhecida: {normalizacao!r}")

    presente = ~np.isnan(valores)
    qtd = presente.sum(axis=1)
    minimo = np.where(presente, valores, np.inf).min(axis=1)
    maximo = np.where(presente, valores, -np.inf).max(axis=1)
    # Máx = mín (ou nenhum valor) é "desvio 0": decidido nos inteiros, sem
    # depender de ponto flutuante, como no Rust.
    varia = (qtd > 0) & (minimo < maximo)
    saida = np.full(valores.shape, neutro)
    if not np.any(varia):
        return saida

    v = valores[varia]
    p = presente[varia]
    with np.errstate(invalid="ignore", divide="ignore"):
        if normalizacao == "zscore":
            n = qtd[varia].astype(np.float64)
            # Soma de inteiros: exata em qualquer ordem (u64 no Rust).
            media = np.where(p, v, 0.0).sum(axis=1) / n
            d = np.where(p, v - media[:, None], 0.0)
            # Variância somada na ordem das dezenas (cumsum é sequencial,
            # como o `sum` do iterador no Rust); somar 0.0 não muda nada.
            variancia = np.cumsum(d * d, axis=1)[:, -1] / n
            desvio = np.sqrt(variancia)
            norm = np.where(p, (v - media[:, None]) / desvio[:, None], neutro)
            ok = (desvio > 0.0) & np.isfinite(desvio)
            norm[~ok] = neutro
        else:
            amplitude = (maximo - minimo)[varia]
            norm = np.where(p, (v - minimo[varia][:, None]) / amplitude[:, None], neutro)
    saida[varia] = norm
    return saida


def arredondar(total: np.ndarray) -> np.ndarray:
    """``(x·1e9).round()/1e9`` com o ``round`` do Rust (meio para longe do
    zero; ``np.round`` arredonda o meio para o par) e sem ``-0.0``."""
    y = total * ESCALA_TOTAL
    inteiro = np.trunc(y)
    fracao = y - inteiro  # exato em ponto flutuante
    inteiro = inteiro + np.where(np.abs(fracao) >= 0.5, np.sign(y), 0.0)
    r = inteiro / ESCALA_TOTAL
    return np.where(r == 0.0, 0.0, r)


def posicoes(totais: np.ndarray) -> np.ndarray:
    """Posição 1…N de cada dezena (colunas em ordem crescente de dezena):
    total decrescente, empate → menor dezena à frente."""
    # Ordenação estável do total negado: no empate, fica a ordem das colunas
    # (a menor dezena primeiro).
    ordem = np.argsort(-totais, axis=1, kind="stable")
    pos = np.empty(totais.shape, dtype=np.int64)
    linhas = np.arange(totais.shape[0])[:, None]
    pos[linhas, ordem] = np.arange(1, totais.shape[1] + 1)[None, :]
    return pos


def variacao_posicao(posicoes: np.ndarray) -> np.ndarray:
    """Variação de posição de ``posicoes`` ``int64[T, N]``: posição no concurso
    anterior − posição atual (positivo = subiu), ``int64[T-1, N]``.

    A linha ``i`` é a variação do concurso ``i + 1`` da entrada; o primeiro
    concurso não tem anterior (``None`` no Rust) e por isso não aparece."""
    if posicoes.ndim != 2:
        raise ErroIndice(f"posições com forma {posicoes.shape}, esperada (T, N)")
    return posicoes[:-1] - posicoes[1:]


def calcular_indice(
    componentes: np.ndarray, normalizacao: str, pesos: Sequence[float]
) -> tuple[np.ndarray, np.ndarray]:
    """Totais (arredondados) e posições de ``componentes`` ``float64[T, N, 9]``
    (dezenas em ordem crescente). Devolve ``(totais[T, N], posicoes[T, N])``."""
    if componentes.ndim != 3 or componentes.shape[2] != len(pesos):
        raise ErroIndice(
            f"componentes com forma {componentes.shape}, esperados (T, N, {len(pesos)})"
        )
    pesos = [float(p) for p in pesos]
    if not all(np.isfinite(pesos)):
        raise ErroIndice(f"pesos não finitos: {pesos} — corrija os pesos do perfil")
    totais = np.zeros(componentes.shape[:2])
    for i, peso in enumerate(pesos):
        totais = totais + peso * normalizar(componentes[:, :, i], normalizacao)
    totais = arredondar(totais)
    if not np.all(np.isfinite(totais)):
        raise ErroIndice("total não finito: pesos grandes demais — corrija os pesos do perfil")
    return totais, posicoes(totais)
