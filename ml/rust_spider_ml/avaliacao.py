"""Avaliação de um perfil contra o acaso, como o ``sync avaliar`` (1c).

Reproduz ``src/avaliacao/`` (``docs/PLANEJAMENTO.md``, 1c, decisões de
2026-10-05):

- **calibração pela posição**, só com o passado:
  ``p_t(r) = (saídas da posição r antes de t + m·K/N) / (concursos antes de t + m)``
  com ``m`` = 100 e o histórico inteiro desde o primeiro concurso da entrada
  (o primeiro do índice), em inteiros e uma única divisão, como no Rust;
- **log loss** (ln, ``p`` limitado a [1e-15, 1 − 1e-15]) e **Brier** (sem
  limite), médias por concurso e depois sobre os concursos;
- **top-k**: acertos das dezenas com posição ≤ k contra a média
  hipergeométrica k·K/N;
- **walk-forward**: do segundo concurso da entrada até o corte, em 5 dobras
  consecutivas de ⌊T/5⌋ concursos, a sobra na última. A calibração começa
  sempre no primeiro concurso; a dobra só escolhe o que entra nas métricas;
- incerteza por concurso: EP = desvio amostral (÷(T−1)) / √T e IC normal
  ±1,96·EP. O veredito por bootstrap e o p-valor exato do top-k ficam no
  Rust (``sync avaliar``).

Somas na mesma ordem do Rust (sequenciais), para a conferência com ~1e-12.
O dataset já vem só com o trecho de busca (``dataset.ler_dataset``).
"""

from __future__ import annotations

import math
from dataclasses import dataclass

import numpy as np

from .dataset import Dataset
from .indice import calcular_indice
from .perfil import Perfil

CONCURSOS_FICTICIOS = 100
LIMITE_PROBABILIDADE = 1e-15
Z_95 = 1.96
QTD_DOBRAS = 5


class ErroAvaliacao(Exception):
    """Entrada que não permite avaliar (série curta, k inválido, perfil de
    outra loteria…)."""


@dataclass(frozen=True)
class Comparacao:
    """Métrica do perfil contra a do acaso (``avaliacao::Comparacao``)."""

    perfil: float
    acaso: float
    diferenca: float
    erro_padrao: float | None
    ic95: tuple[float, float] | None


@dataclass(frozen=True)
class FaixaAcertos:
    acertos: int
    observados: int
    esperados: float


@dataclass(frozen=True)
class ResumoTopK:
    k: int
    total_acertos: int
    acertos: Comparacao
    distribuicao: tuple[FaixaAcertos, ...]


@dataclass(frozen=True)
class AvaliacaoFaixa:
    primeiro: int
    ultimo: int
    concursos_avaliados: int
    log_loss: Comparacao
    brier: Comparacao
    topk: ResumoTopK


@dataclass(frozen=True)
class AvaliacaoPerfil:
    loteria: str
    perfil: Perfil
    k: int
    #: K e N tirados do dataset.
    k_sorteadas: int
    n_dezenas: int
    corte: int
    dobras: tuple[AvaliacaoFaixa, ...]
    #: O trecho inteiro (a união das dobras), o do veredito no Rust.
    total: AvaliacaoFaixa


def soma_sequencial(x: np.ndarray, axis: int = -1) -> np.ndarray:
    """Soma da esquerda para a direita (``cumsum``), como ``Iterator::sum``
    no Rust; ``np.sum`` soma aos pares e pode diferir no último bit."""
    return np.cumsum(x, axis=axis).take(-1, axis=axis)


def taxa_do_acaso(k_sorteadas: int, n_dezenas: int) -> float:
    return k_sorteadas / n_dezenas


def calibrar(posicoes: np.ndarray, saiu: np.ndarray, k_sorteadas: int) -> np.ndarray:
    """Probabilidade calibrada ``float64[T, N]`` de cada dezena de cada
    concurso (todos sorteados e seguidos), pela posição, só com o passado.

    O concurso ``t`` usa as contagens dos concursos ``0 … t−1`` da entrada:
    ``t`` só entra nas contagens depois de receber a sua probabilidade.
    """
    t, n = posicoes.shape
    m = CONCURSOS_FICTICIOS
    linhas = np.arange(t)[:, None]
    saidas = np.zeros((t, n), dtype=np.int64)
    saidas[linhas, posicoes - 1] = saiu
    # Saídas por posição acumuladas **antes** de cada concurso.
    antes = np.zeros((t, n), dtype=np.int64)
    antes[1:] = np.cumsum(saidas, axis=0)[:-1]
    s = antes[linhas, posicoes - 1]
    concursos_antes = np.arange(t, dtype=np.int64)[:, None]
    # Inteiros (exatos) e uma única divisão: o mesmo f64 do Rust.
    numerador = (s * n + m * k_sorteadas).astype(np.float64)
    denominador = ((concursos_antes + m) * n).astype(np.float64)
    return numerador / denominador


def log_loss(p: np.ndarray, saiu: np.ndarray) -> np.ndarray:
    p = np.clip(p, LIMITE_PROBABILIDADE, 1.0 - LIMITE_PROBABILIDADE)
    return np.where(saiu == 1, -np.log(p), -np.log(1.0 - p))


def brier(p: np.ndarray, saiu: np.ndarray) -> np.ndarray:
    erro = p - saiu.astype(np.float64)
    return erro * erro


def metricas_por_concurso(
    probabilidades: np.ndarray, saiu: np.ndarray, k_sorteadas: int
) -> dict[str, np.ndarray]:
    """Médias por concurso (``float64[T]``) do perfil e do acaso."""
    n = probabilidades.shape[1]
    acaso = np.full(probabilidades.shape, taxa_do_acaso(k_sorteadas, n))
    return {
        "log_loss_perfil": soma_sequencial(log_loss(probabilidades, saiu)) / n,
        "log_loss_acaso": soma_sequencial(log_loss(acaso, saiu)) / n,
        "brier_perfil": soma_sequencial(brier(probabilidades, saiu)) / n,
        "brier_acaso": soma_sequencial(brier(acaso, saiu)) / n,
    }


def comparar(perfil: np.ndarray, acaso: np.ndarray) -> Comparacao:
    """Como ``metricas::comparar``: médias, diferença média por concurso,
    EP amostral e IC normal (``None`` com menos de 2 concursos)."""
    t = float(perfil.size)
    if perfil.size == 0:
        raise ErroAvaliacao("série vazia")
    media_perfil = float(soma_sequencial(perfil)) / t
    media_acaso = float(soma_sequencial(acaso)) / t
    diferencas = perfil - acaso
    diferenca = float(soma_sequencial(diferencas)) / t
    ep = None
    ic = None
    if perfil.size >= 2:
        desvios = diferencas - diferenca
        ep = math.sqrt(float(soma_sequencial(desvios * desvios)) / (t - 1.0)) / math.sqrt(t)
        ic = (diferenca - Z_95 * ep, diferenca + Z_95 * ep)
    return Comparacao(media_perfil, media_acaso, diferenca, ep, ic)


def hipergeometrica(n: int, sorteadas: int, escolhidas: int) -> list[float]:
    """``P(i)`` para ``i`` em ``0 … min(escolhidas, sorteadas)``, em inteiros
    e uma divisão (``Hipergeometrica::pmf``)."""
    total = math.comb(n, escolhidas)
    return [
        float(math.comb(sorteadas, i) * math.comb(n - sorteadas, escolhidas - i)) / float(total)
        for i in range(min(escolhidas, sorteadas) + 1)
    ]


def media_topk(k: int, k_sorteadas: int, n_dezenas: int) -> float:
    """Acertos esperados ao acaso: k·K/N (na ordem de contas do Rust)."""
    return float(k) * float(k_sorteadas) / float(n_dezenas)


def resumir_topk(acertos: np.ndarray, k: int, k_sorteadas: int, n_dezenas: int) -> ResumoTopK:
    esperado = media_topk(k, k_sorteadas, n_dezenas)
    pmf = hipergeometrica(n_dezenas, k_sorteadas, k)
    t = acertos.size
    contagem = np.bincount(acertos, minlength=len(pmf))
    if contagem.size > len(pmf):
        raise ErroAvaliacao(f"acertos acima do máximo possível no top-{k}")
    return ResumoTopK(
        k=k,
        total_acertos=int(acertos.sum()),
        acertos=comparar(acertos.astype(np.float64), np.full(t, esperado)),
        distribuicao=tuple(
            FaixaAcertos(i, int(contagem[i]), t * pmf[i]) for i in range(len(pmf))
        ),
    )


def dividir_em_dobras(primeiro: int, ultimo: int, qtd: int = QTD_DOBRAS) -> list[tuple[int, int]]:
    """``walk_forward::dividir_em_dobras``: ``qtd`` faixas consecutivas de
    ⌊T/qtd⌋ concursos, a sobra na última."""
    if qtd <= 0:
        raise ErroAvaliacao("walk-forward com 0 dobras")
    total = ultimo - primeiro + 1 if primeiro <= ultimo else 0
    if total < qtd:
        raise ErroAvaliacao(
            f"a faixa {primeiro}..={ultimo} tem {total} concurso(s), menos que as {qtd} dobras"
        )
    tamanho = total // qtd
    dobras = []
    for i in range(qtd):
        inicio = primeiro + i * tamanho
        fim = ultimo if i == qtd - 1 else inicio + tamanho - 1
        dobras.append((inicio, fim))
    return dobras


def faixa_da_busca(dataset: Dataset) -> tuple[int, int]:
    """Do segundo concurso da entrada (o primeiro recebe o acaso por
    construção) até o último do dataset, que já está limitado ao corte."""
    if dataset.concursos.size < 2:
        raise ErroAvaliacao("o dataset precisa de pelo menos 2 concursos")
    ultimo = dataset.ultimo_concurso
    if ultimo > dataset.corte:
        raise ErroAvaliacao("dataset acima do corte da busca")
    return int(dataset.concursos[1]), ultimo


def dobras_da_busca(dataset: Dataset) -> list[tuple[int, int]]:
    return dividir_em_dobras(*faixa_da_busca(dataset), QTD_DOBRAS)


def avaliar_perfil(dataset: Dataset, perfil: Perfil, k: int | None = None) -> AvaliacaoPerfil:
    """Refaz índice, calibração e métricas do perfil no trecho de busca do
    dataset, por dobra e no trecho inteiro. ``k`` padrão = K."""
    if perfil.loteria != dataset.loteria:
        raise ErroAvaliacao(
            f"perfil da loteria {perfil.loteria!r}, dataset da {dataset.loteria!r}"
        )
    n = dataset.n_dezenas
    k_sorteadas = dataset.k_sorteadas
    k = k_sorteadas if k is None else k
    if not 1 <= k < n:
        raise ErroAvaliacao(f"k = {k} fora de 1..={n - 1}")

    _, posicoes = calcular_indice(
        dataset.componentes_da_janela(perfil.janela), perfil.normalizacao, perfil.pesos
    )
    probabilidades = calibrar(posicoes, dataset.saiu, k_sorteadas)
    metricas = metricas_por_concurso(probabilidades, dataset.saiu, k_sorteadas)
    acertos = ((posicoes <= k) & (dataset.saiu == 1)).sum(axis=1).astype(np.int64)

    def avaliar_faixa(primeiro: int, ultimo: int) -> AvaliacaoFaixa:
        sel = (dataset.concursos >= primeiro) & (dataset.concursos <= ultimo)
        # O primeiro concurso da entrada nunca é avaliado (`faixa_da_busca`
        # já começa no segundo).
        sel[0] = False
        return AvaliacaoFaixa(
            primeiro=primeiro,
            ultimo=ultimo,
            concursos_avaliados=int(sel.sum()),
            log_loss=comparar(metricas["log_loss_perfil"][sel], metricas["log_loss_acaso"][sel]),
            brier=comparar(metricas["brier_perfil"][sel], metricas["brier_acaso"][sel]),
            topk=resumir_topk(acertos[sel], k, k_sorteadas, n),
        )

    faixa = faixa_da_busca(dataset)
    return AvaliacaoPerfil(
        loteria=dataset.loteria,
        perfil=perfil,
        k=k,
        k_sorteadas=k_sorteadas,
        n_dezenas=n,
        corte=dataset.corte,
        dobras=tuple(avaliar_faixa(a, b) for a, b in dividir_em_dobras(*faixa)),
        total=avaliar_faixa(*faixa),
    )
