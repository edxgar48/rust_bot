"""Busca de pesos com walk-forward (1.5.3), gerando perfis ``.toml``.

Regras (``docs/PLANEJAMENTO.md``, 1.5.3, decisões de 2026-10-06):

- **o que varia**: os 9 pesos com sinal livre, a janela (0, 10, 50, 100) e a
  normalização (zscore, minmax). Pesos normalizados (soma dos ``|w|`` = 1) e
  arredondados a 6 casas **antes** de avaliar: o candidato avaliado é
  exatamente o que vai para o ``.toml`` (os números do relatório já são os
  dos pesos arredondados). Todos os pesos zero é recusado;
- **método**: busca aleatória, só numpy, com semente fixa e o mesmo número de
  candidatos em cada combinação janela × normalização. Cada combinação tem o
  seu gerador (``SeedSequence(semente).spawn``), então um orçamento menor é
  um prefixo do maior. Sem refinamento peso a peso (opcional no plano);
- **métrica otimizada**: log loss médio por concurso, com a calibração da
  1.5.2 (pela posição, m = 100, desde o primeiro concurso, só o passado). A
  pontuação em lote reproduz :func:`avaliacao.avaliar_perfil` operação por
  operação (mesmos ``f64`` por concurso); Brier e top-k só como informação;
- **walk-forward aninhado** nas 5 dobras da 1c.3: para i = 2 … 5, o melhor
  nas dobras 1 … i−1 é medido na dobra i. A escolha de i só olha as somas
  das dobras anteriores, e o log loss de um concurso só depende dele e do
  passado (features até t−1, calibração com os concursos antes de t): os
  dados da dobra i e seguintes não mudam a escolha para i. A estimativa
  honesta é o log loss nessas dobras de teste contra o acaso K/N. O perfil
  final é o melhor no trecho de busca inteiro (dobras 1 a 5);
- a busca **só lê o dataset da busca** (:func:`dataset.ler_dataset`, que
  recusa o trancado); não grava no banco nem troca o perfil ativo. A única
  saída é o ``.toml`` (``busca_v<N>``, a versão seguinte se já existir).

Com vários candidatos, o p-valor do ``sync avaliar`` no trecho de busca sai
otimista para o vencedor (ele foi escolhido nesses mesmos sorteios); a
validação que vale é a do trancado, uma única vez, decisão do usuário.
"""

from __future__ import annotations

import math
import re
import tempfile
from collections.abc import Sequence
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass
from pathlib import Path

import numpy as np

from .avaliacao import (
    CONCURSOS_FICTICIOS,
    LIMITE_PROBABILIDADE,
    AvaliacaoPerfil,
    Comparacao,
    avaliar_perfil,
    calibrar,
    comparar,
    dobras_da_busca,
    metricas_por_concurso,
    soma_sequencial,
)
from .dataset import COMPONENTES, Dataset
from .indice import ESCALA_TOTAL, calcular_indice, normalizar
from .perfil import JANELAS, NORMALIZACOES, Perfil, interpretar_perfil

SEMENTE_PADRAO = 20261006
#: Candidatos por combinação janela × normalização (8 combinações).
ORCAMENTO_PADRAO = 2000
CASAS_PESOS = 6
NOME_BASE = "busca"
#: Elementos (candidatos × concursos × dezenas) por lote na pontuação.
ELEMENTOS_POR_LOTE = 2_000_000
#: Primeira dobra de teste do walk-forward (as anteriores só treinam).
PRIMEIRA_DOBRA_TESTE = 2


class ErroBusca(Exception):
    """Entrada que não permite a busca (pesos todos zero, janela ausente…)."""


@dataclass(frozen=True)
class Candidato:
    #: Ordem do candidato na lista (desempate: o menor índice vence).
    indice: int
    janela: int
    normalizacao: str
    #: Os 9 pesos na ordem de :data:`COMPONENTES`, soma dos ``|w|`` ≈ 1,
    #: arredondados a :data:`CASAS_PESOS` casas.
    pesos: tuple[float, ...]

    def perfil(self, loteria: str, nome: str = "candidato_v1") -> Perfil:
        """Perfil em memória (sem arquivo; ``hash_toml`` vazio)."""
        return Perfil(
            nome=nome,
            versao=1,
            loteria=loteria,
            janela=self.janela,
            normalizacao=self.normalizacao,
            pesos=self.pesos,
            hash_toml="",
        )


def normalizar_pesos(pesos: Sequence[float]) -> tuple[float, ...]:
    """Divide pela soma dos ``|w|`` e arredonda a 6 casas (sem ``-0.0``).

    Multiplicar os pesos por uma constante positiva não muda o índice (só a
    escala do total); a normalização escolhe um representante de cada
    direção. Recusa pesos não finitos, quantidade errada e todos zero.
    """
    w = np.asarray(pesos, dtype=np.float64)
    if w.shape != (len(COMPONENTES),):
        raise ErroBusca(f"esperados {len(COMPONENTES)} pesos, vieram {w.shape}")
    if not np.all(np.isfinite(w)):
        raise ErroBusca(f"pesos não finitos: {w.tolist()}")
    soma = float(np.abs(w).sum())
    if soma == 0.0:
        raise ErroBusca("perfil com todos os pesos zero é proibido")
    saida = tuple(_arredondar_peso(float(x) / soma) for x in w)
    if all(x == 0.0 for x in saida):
        raise ErroBusca("todos os pesos zeram ao arredondar a 6 casas")
    return saida


def _arredondar_peso(x: float) -> float:
    # O texto com 6 casas é o que vai para o `.toml`; o float é o que o
    # leitor (Python ou Rust) tira desse texto.
    y = float(f"{x:.{CASAS_PESOS}f}")
    return 0.0 if y == 0.0 else y


def texto_peso(x: float) -> str:
    """Peso como vai para o ``.toml`` (6 casas, sem ``-0``)."""
    return f"{_arredondar_peso(x):.{CASAS_PESOS}f}"


def combinacoes(
    janelas: Sequence[int] = JANELAS, normalizacoes: Sequence[str] = NORMALIZACOES
) -> list[tuple[int, str]]:
    return [(j, n) for j in janelas for n in normalizacoes]


def gerar_candidatos(
    semente: int = SEMENTE_PADRAO,
    orcamento: int = ORCAMENTO_PADRAO,
    janelas: Sequence[int] = JANELAS,
    normalizacoes: Sequence[str] = NORMALIZACOES,
) -> list[Candidato]:
    """``orcamento`` candidatos em cada combinação janela × normalização.

    Pesos ~ normal padrão (direção uniforme, sinal livre), normalizados por
    :func:`normalizar_pesos`. Um gerador por combinação, filho da semente:
    o resultado não depende de quantas combinações vêm antes, e um orçamento
    menor dá um prefixo dos candidatos de um orçamento maior.
    """
    if orcamento < 1:
        raise ErroBusca(f"orçamento {orcamento} < 1")
    combos = combinacoes(janelas, normalizacoes)
    filhos = np.random.SeedSequence(semente).spawn(len(JANELAS) * len(NORMALIZACOES))
    posicao = {c: i for i, c in enumerate(combinacoes())}
    candidatos: list[Candidato] = []
    for janela, normalizacao in combos:
        if (janela, normalizacao) not in posicao:
            raise ErroBusca(f"combinação fora do plano: janela {janela}, {normalizacao}")
        gerador = np.random.default_rng(filhos[posicao[(janela, normalizacao)]])
        brutos = gerador.standard_normal((orcamento, len(COMPONENTES)))
        for linha in brutos:
            candidatos.append(
                Candidato(len(candidatos), janela, normalizacao, normalizar_pesos(linha))
            )
    return candidatos


# --- pontuação em lote ---------------------------------------------------------


def normalizados(dataset: Dataset, janela: int, normalizacao: str) -> np.ndarray:
    """Componentes normalizados ``float64[9, T, N]`` (por concurso, um bloco
    contíguo por componente), com a mesma :func:`indice.normalizar` do índice."""
    comp = dataset.componentes_da_janela(janela)
    return np.stack([normalizar(comp[:, :, i], normalizacao) for i in range(comp.shape[2])])


def log_loss_por_concurso_lote(
    z: np.ndarray, pesos: np.ndarray, saiu: np.ndarray, k_sorteadas: int
) -> np.ndarray:
    """Log loss médio por concurso ``float64[B, T]`` de ``B`` vetores de pesos.

    ``z`` vem de :func:`normalizados` (``[9, T, N]``). Dá os mesmos ``f64``,
    bit a bit (testado), de ``calcular_indice`` + ``calibrar`` +
    ``metricas_por_concurso``, com uma dimensão a mais para os candidatos:

    - total = 0 + Σ pesoₖ·zₖ na ordem dos componentes, arredondado a 1e-9
      como em :func:`indice.arredondar`, mas guardando o **inteiro** (total
      × 1e9): ordenar pelo inteiro é ordenar pelo total arredondado (|total|
      ≤ 9, longe de onde 1e-9 some no ``f64``). A chave ``−inteiro·N +
      coluna`` é única, então o ``argsort`` comum dá a ordem do estável
      (total decrescente, empate → menor dezena);
    - calibração pela posição com contagens inteiras e uma divisão;
    - ``−ln(q)`` com ``q`` = p (saiu) ou 1 − p (não saiu), ``p`` limitado a
      [1e-15, 1 − 1e-15], somado na ordem das dezenas.
    """
    b = pesos.shape[0]
    _, t, n = z.shape
    totais = np.zeros((b, t, n))
    termo = np.empty((b, t, n))
    for i in range(z.shape[0]):
        np.multiply(pesos[:, i, None, None], z[i][None], out=termo)
        np.add(totais, termo, out=totais)
    # `indice.arredondar` sem a divisão final: trunca e soma ±1 quando a
    # fração (exata) chega a meio, para longe do zero (o `round` do Rust).
    y = np.multiply(totais, ESCALA_TOTAL, out=totais)
    truncado = np.trunc(y)
    np.subtract(y, truncado, out=termo)
    inteiro = truncado.astype(np.int64)
    inteiro += termo >= 0.5
    inteiro -= termo <= -0.5
    del totais, termo, truncado
    np.negative(inteiro, out=inteiro)
    inteiro *= n
    inteiro += np.arange(n, dtype=np.int64)
    ordem = np.argsort(inteiro, axis=-1)
    del inteiro

    # Calibração pela posição: saídas por posição acumuladas antes de t.
    linhas_saiu = (np.arange(t, dtype=np.int64) * n)[None, :, None]
    saidas = saiu.ravel()[linhas_saiu + ordem].astype(np.int32)
    antes = np.zeros((b, t, n), dtype=np.int32)
    np.cumsum(saidas[:, :-1], axis=1, out=antes[:, 1:])
    linhas = (np.arange(b * t, dtype=np.int64) * n).reshape(b, t, 1)
    posicao = np.empty((b, t, n), dtype=np.int64)
    posicao.ravel()[(linhas + ordem).ravel()] = np.tile(np.arange(n, dtype=np.int64), b * t)
    s = antes.ravel()[linhas + posicao].astype(np.int64)
    m = CONCURSOS_FICTICIOS
    numerador = (s * n + m * k_sorteadas).astype(np.float64)
    denominador = ((np.arange(t, dtype=np.int64)[:, None] + m) * n).astype(np.float64)
    p = np.clip(numerador / denominador, LIMITE_PROBABILIDADE, 1.0 - LIMITE_PROBABILIDADE)
    q = np.where(saiu == 1, p, 1.0 - p)
    return soma_sequencial(-np.log(q)) / n


@dataclass(frozen=True)
class Pontuacao:
    """Soma do log loss por concurso de cada candidato em cada dobra."""

    dobras: tuple[tuple[int, int], ...]
    #: ``float64[C, D]``: soma (sequencial) do log loss por concurso.
    somas: np.ndarray
    #: ``int64[D]``: concursos avaliados em cada dobra.
    concursos: np.ndarray

    def media(self, dobras: Sequence[int]) -> np.ndarray:
        """Log loss médio por concurso nas ``dobras`` (1 = primeira)."""
        idx = [d - 1 for d in dobras]
        return self.somas[:, idx].sum(axis=1) / float(self.concursos[idx].sum())

    def escolher(self, dobras: Sequence[int]) -> int:
        """Índice do menor log loss médio nas ``dobras`` (empate → o menor
        índice, ``argmin``)."""
        return int(np.argmin(self.media(dobras)))


def _selecoes(dataset: Dataset, dobras: Sequence[tuple[int, int]]) -> np.ndarray:
    """``bool[D, T]``: os concursos avaliados de cada dobra (o primeiro da
    entrada nunca é avaliado)."""
    sel = np.zeros((len(dobras), dataset.concursos.size), dtype=bool)
    for d, (primeiro, ultimo) in enumerate(dobras):
        sel[d] = (dataset.concursos >= primeiro) & (dataset.concursos <= ultimo)
    sel[:, 0] = False
    return sel


def _somas_por_dobra(
    z: np.ndarray, pesos: np.ndarray, saiu: np.ndarray, k: int, sel: np.ndarray, lote: int
) -> np.ndarray:
    """``float64[B, D]``: soma sequencial do log loss por concurso em cada dobra."""
    somas = np.empty((pesos.shape[0], sel.shape[0]))
    for inicio in range(0, pesos.shape[0], lote):
        ll = log_loss_por_concurso_lote(z, pesos[inicio : inicio + lote], saiu, k)
        for d in range(sel.shape[0]):
            somas[inicio : inicio + lote, d] = soma_sequencial(ll[:, sel[d]])
    return somas


# Estado de cada processo de trabalho (Windows cria processos novos: o
# estado chega pelo inicializador e os componentes normalizados por arquivos
# `.npy` lidos com mmap, compartilhados pelo cache do sistema).
_TRABALHO: dict = {}


def _iniciar_trabalho(saiu: np.ndarray, k: int, sel: np.ndarray, lote: int) -> None:
    _TRABALHO.update(saiu=saiu, k=k, sel=sel, lote=lote, z={})


def _tarefa(arquivo_z: str, pesos: np.ndarray) -> np.ndarray:
    cache = _TRABALHO["z"]
    if arquivo_z not in cache:
        cache.clear()
        cache[arquivo_z] = np.load(arquivo_z, mmap_mode="r")
    t = _TRABALHO
    return _somas_por_dobra(cache[arquivo_z], pesos, t["saiu"], t["k"], t["sel"], t["lote"])


def pontuar(
    dataset: Dataset,
    candidatos: Sequence[Candidato],
    elementos_por_lote: int = ELEMENTOS_POR_LOTE,
    processos: int = 1,
) -> Pontuacao:
    """Log loss de todos os candidatos, somado por dobra (5 dobras da 1c.3).

    Cada candidato é avaliado uma vez no trecho inteiro; a soma da dobra d
    só usa os concursos de d, e o log loss de um concurso só depende dele e
    do passado. Escolher pelas dobras 1 … i−1 não lê nada da dobra i.

    ``processos`` > 1 divide os candidatos entre processos; o resultado é o
    mesmo, bit a bit (cada candidato tem a sua linha, sem soma entre
    processos).
    """
    dobras = tuple(dobras_da_busca(dataset))
    sel = _selecoes(dataset, dobras)
    k = dataset.k_sorteadas
    t, n = dataset.saiu.shape
    lote = max(1, elementos_por_lote // (t * n))
    somas = np.zeros((len(candidatos), len(dobras)))
    por_combo: dict[tuple[int, str], list[int]] = {}
    for i, c in enumerate(candidatos):
        por_combo.setdefault((c.janela, c.normalizacao), []).append(i)

    def pesos_de(indices: list[int]) -> np.ndarray:
        return np.array([candidatos[i].pesos for i in indices], dtype=np.float64)

    if processos <= 1:
        for (janela, normalizacao), indices in por_combo.items():
            z = normalizados(dataset, janela, normalizacao)
            somas[indices] = _somas_por_dobra(z, pesos_de(indices), dataset.saiu, k, sel, lote)
    else:
        with tempfile.TemporaryDirectory(prefix="busca_pesos_") as pasta:
            tarefas = []
            for (janela, normalizacao), indices in por_combo.items():
                arquivo = str(Path(pasta) / f"z_{janela}_{normalizacao}.npy")
                np.save(arquivo, normalizados(dataset, janela, normalizacao))
                passo = lote * 8
                for inicio in range(0, len(indices), passo):
                    tarefas.append((arquivo, indices[inicio : inicio + passo]))
            with ProcessPoolExecutor(
                max_workers=processos,
                initializer=_iniciar_trabalho,
                initargs=(dataset.saiu, k, sel, lote),
            ) as executor:
                futuros = [executor.submit(_tarefa, a, pesos_de(ix)) for a, ix in tarefas]
                for (_, indices), futuro in zip(tarefas, futuros):
                    somas[indices] = futuro.result()
    return Pontuacao(
        dobras=dobras,
        somas=somas,
        concursos=sel.sum(axis=1).astype(np.int64),
    )


# --- walk-forward e resultado --------------------------------------------------


@dataclass(frozen=True)
class Passo:
    """Um passo do walk-forward: escolhido nas dobras 1 … i−1, medido em i."""

    dobra: int
    escolhido: Candidato
    #: Log loss médio (perfil, acaso) do escolhido nas dobras de treino.
    treino: Comparacao
    #: Avaliação completa do escolhido em todas as dobras (a de teste é
    #: ``avaliacao.dobras[dobra − 1]``).
    avaliacao: AvaliacaoPerfil

    @property
    def teste(self):
        return self.avaliacao.dobras[self.dobra - 1]


@dataclass(frozen=True)
class ResultadoBusca:
    loteria: str
    semente: int
    orcamento: int
    candidatos_testados: int
    combinacoes: tuple[tuple[int, str], ...]
    dobras: tuple[tuple[int, int], ...]
    passos: tuple[Passo, ...]
    #: Log loss das dobras de teste (2 a 5) juntas, cada concurso com o
    #: escolhido do seu passo, contra o acaso: a estimativa honesta.
    fora_da_amostra: Comparacao
    concursos_fora_da_amostra: int
    final: Candidato
    #: Avaliação do final (pesos já arredondados) no trecho inteiro e por dobra.
    avaliacao_final: AvaliacaoPerfil
    #: Dataset usado: feature_version, último concurso, impressão dos incluídos…
    dataset: dict


def series_por_concurso(dataset: Dataset, perfil: Perfil) -> dict[str, np.ndarray]:
    """Log loss e Brier por concurso (perfil e acaso), como no
    :func:`avaliacao.avaliar_perfil`."""
    _, posicoes = calcular_indice(
        dataset.componentes_da_janela(perfil.janela), perfil.normalizacao, perfil.pesos
    )
    p = calibrar(posicoes, dataset.saiu, dataset.k_sorteadas)
    return metricas_por_concurso(p, dataset.saiu, dataset.k_sorteadas)


def _selecao(dataset: Dataset, dobras: Sequence[tuple[int, int]]) -> np.ndarray:
    sel = np.zeros(dataset.concursos.size, dtype=bool)
    for primeiro, ultimo in dobras:
        sel |= (dataset.concursos >= primeiro) & (dataset.concursos <= ultimo)
    sel[0] = False
    return sel


def buscar(
    dataset: Dataset,
    candidatos: Sequence[Candidato],
    *,
    semente: int | None = None,
    orcamento: int | None = None,
    processos: int = 1,
) -> ResultadoBusca:
    """Walk-forward aninhado e perfil final. ``semente``/``orcamento`` só
    vão para o registro (os candidatos já vêm gerados), mas
    :func:`gravar_perfil` recusa resultado sem eles."""
    if not candidatos:
        raise ErroBusca("nenhum candidato")
    faltando = sorted({c.janela for c in candidatos} - set(dataset.janelas))
    if faltando:
        raise ErroBusca(f"janela(s) {faltando} ausente(s) do dataset (há {list(dataset.janelas)})")
    pontuacao = pontuar(dataset, candidatos, processos=processos)
    dobras = pontuacao.dobras
    qtd = len(dobras)

    passos = []
    teste_perfil, teste_acaso = [], []
    for i in range(PRIMEIRA_DOBRA_TESTE, qtd + 1):
        treino = list(range(1, i))
        escolhido = candidatos[pontuacao.escolher(treino)]
        perfil = escolhido.perfil(dataset.loteria)
        series = series_por_concurso(dataset, perfil)
        sel_treino = _selecao(dataset, [dobras[d - 1] for d in treino])
        sel_teste = _selecao(dataset, [dobras[i - 1]])
        teste_perfil.append(series["log_loss_perfil"][sel_teste])
        teste_acaso.append(series["log_loss_acaso"][sel_teste])
        passos.append(
            Passo(
                dobra=i,
                escolhido=escolhido,
                treino=comparar(
                    series["log_loss_perfil"][sel_treino], series["log_loss_acaso"][sel_treino]
                ),
                avaliacao=avaliar_perfil(dataset, perfil),
            )
        )
    final = candidatos[pontuacao.escolher(list(range(1, qtd + 1)))]
    m = dataset.manifesto
    return ResultadoBusca(
        loteria=dataset.loteria,
        semente=semente,
        orcamento=orcamento,
        candidatos_testados=len(candidatos),
        combinacoes=tuple(dict.fromkeys((c.janela, c.normalizacao) for c in candidatos)),
        dobras=dobras,
        passos=tuple(passos),
        fora_da_amostra=comparar(np.concatenate(teste_perfil), np.concatenate(teste_acaso)),
        concursos_fora_da_amostra=int(sum(x.size for x in teste_perfil)),
        final=final,
        avaliacao_final=avaliar_perfil(dataset, final.perfil(dataset.loteria)),
        dataset={
            "arquivo": m["arquivo"],
            "formato": dataset.formato,
            "sha256": m["sha256"],
            "feature_version": dataset.feature_version,
            "primeiro_concurso": dataset.primeiro_concurso,
            "ultimo_concurso": dataset.ultimo_concurso,
            "corte": dataset.corte,
            "impressao_resultados_incluidos": m["impressao_resultados_incluidos"],
        },
    )


# --- perfil .toml --------------------------------------------------------------


def proxima_versao(pasta_loteria: Path, base: str = NOME_BASE) -> int:
    """1 se não há ``<base>_v<N>.toml``; senão o maior N + 1.

    Só olha os arquivos da pasta, não os bancos. O nome gerado não pode já
    estar registrado na tabela ``perfis`` de um banco com outro conteúdo
    (por exemplo, um ``busca_v1.toml`` apagado depois de passar pelo ``sync
    indice``): perfis são imutáveis, e o Rust recusa o mesmo nome com hash
    diferente (``orquestracao::registrar_perfil``, erro
    ``SpiderError::PerfilAlterado``). Nesse caso, suba a versão à mão
    (nome do arquivo, ``nome`` e ``versao`` no ``.toml``) antes de rodar o
    ``sync indice``.
    """
    padrao = re.compile(rf"^{re.escape(base)}_v([1-9][0-9]*)\.toml$")
    versoes = [
        int(m.group(1))
        for p in (pasta_loteria.iterdir() if pasta_loteria.is_dir() else [])
        if (m := padrao.match(p.name))
    ]
    return max(versoes, default=0) + 1


def _f(x: float | None, formato: str) -> str:
    return "n/d" if x is None else format(x, formato)


def cabecalho(resultado: ResultadoBusca, experimento: str) -> list[str]:
    """Linhas de comentário do ``.toml`` (determinísticas: sem data de
    geração nem caminho absoluto)."""
    r = resultado
    d = r.dataset
    fa = r.fora_da_amostra
    tot = r.avaliacao_final.total.log_loss
    linhas = [
        f"Gerado por {experimento} (busca de pesos, 1.5.3). Proposta para revisão:",
        "não é o perfil ativo; validar no trancado (uso único) antes de adotar.",
        f"Semente {r.semente}; {r.orcamento} candidatos por combinação janela × normalização "
        f"× {len(r.combinacoes)} combinações = {r.candidatos_testados} candidatos testados.",
        f"Dataset: {d['arquivo']} ({d['formato']}, sha256 {d['sha256']}), feature_version "
        f"{d['feature_version']}, concursos {d['primeiro_concurso']}–{d['ultimo_concurso']} "
        f"(corte da busca {d['corte']}), impressão dos incluídos "
        f"{d['impressao_resultados_incluidos']}.",
        "Métrica: log loss médio por concurso, calibração pela posição (m = 100); "
        "pesos já arredondados.",
        "Walk-forward (escolhido nas dobras 1…i−1, medido na dobra i):",
    ]
    for p in r.passos:
        t = p.teste
        linhas.append(
            f"  dobra {p.dobra} ({t.primeiro}–{t.ultimo}): janela {p.escolhido.janela}, "
            f"{p.escolhido.normalizacao}, log loss {t.log_loss.perfil:.12f} × acaso "
            f"{t.log_loss.acaso:.12f} (dif {t.log_loss.diferenca:+.3e})"
        )
    linhas += [
        f"Fora da amostra (dobras {PRIMEIRA_DOBRA_TESTE}–{len(r.dobras)}, "
        f"{r.concursos_fora_da_amostra} concursos): log loss {fa.perfil:.12f} × acaso "
        f"{fa.acaso:.12f} (dif {fa.diferenca:+.3e}, EP {_f(fa.erro_padrao, '.3e')}) — estimativa "
        "do MÉTODO (um escolhido por dobra), não deste perfil.",
        f"Este perfil (melhor nas dobras 1–{len(r.dobras)}), no trecho inteiro "
        f"{r.avaliacao_final.total.primeiro}–{r.avaliacao_final.total.ultimo}: log loss "
        f"{tot.perfil:.12f} × acaso {tot.acaso:.12f} (dif {tot.diferenca:+.3e}) — dentro da "
        "amostra, otimista.",
        f"p-valor do `sync avaliar` neste trecho sai otimista (melhor de "
        f"{r.candidatos_testados}); use --testados {r.candidatos_testados}. Rodar de novo com "
        "outra semente e ficar com a melhor execução também é otimista.",
    ]
    return linhas


def texto_toml(resultado: ResultadoBusca, nome: str, versao: int, experimento: str) -> str:
    """Perfil no formato do plano (aceito por ``src/perfil.rs``), LF."""
    c = resultado.final
    largura = max(len(x) for x in COMPONENTES)
    linhas = [f"# perfis/{resultado.loteria}/{nome}.toml"]
    linhas += [f"# {l}" if l else "#" for l in cabecalho(resultado, experimento)]
    linhas += [
        f'nome = "{nome}"',
        f"versao = {versao}",
        f'loteria = "{resultado.loteria}"',
        f"janela = {c.janela}",
        f'normalizacao = "{c.normalizacao}"',
        "",
        "[pesos]",
    ]
    linhas += [f"{comp:<{largura}} = {texto_peso(w)}" for comp, w in zip(COMPONENTES, c.pesos)]
    return "\n".join(linhas) + "\n"


def gravar_perfil(
    resultado: ResultadoBusca,
    raiz_perfis: Path | str,
    experimento: str,
    base: str = NOME_BASE,
) -> tuple[Path, Perfil]:
    """Grava ``<raiz>/<loteria>/<base>_v<N>.toml`` (N = a versão seguinte),
    sem sobrescrever, e devolve o caminho e o perfil relido do texto
    (conferido com o candidato final).

    Recusa (:class:`ErroBusca`, sem gravar) resultado sem ``semente`` ou
    ``orcamento``: o cabeçalho do ``.toml`` é o registro do experimento e
    tem de permitir reproduzi-lo."""
    faltando = [c for c in ("semente", "orcamento") if getattr(resultado, c) is None]
    if faltando:
        raise ErroBusca(
            f"resultado sem {' e '.join(faltando)}: o perfil não seria reproduzível "
            "(passe semente= e orcamento= para buscar)"
        )
    pasta = Path(raiz_perfis) / resultado.loteria
    pasta.mkdir(parents=True, exist_ok=True)
    versao = proxima_versao(pasta, base)
    nome = f"{base}_v{versao}"
    texto = texto_toml(resultado, nome, versao, experimento)
    perfil = interpretar_perfil(texto, nome)
    final = resultado.final
    if (perfil.pesos, perfil.janela, perfil.normalizacao) != (
        final.pesos,
        final.janela,
        final.normalizacao,
    ):
        raise ErroBusca("o .toml relido não bate com o candidato final")
    caminho = pasta / f"{nome}.toml"
    with open(caminho, "x", encoding="utf-8", newline="\n") as f:
        f.write(texto)
    return caminho, perfil


def soma_abs(pesos: Sequence[float]) -> float:
    return math.fsum(abs(x) for x in pesos)
