"""Experimento 002 — busca de pesos com walk-forward (1.5.3, 2026-10-06).

Para cada dataset (lido pelo manifesto, só o trecho de busca; o trancado
nunca é lido):

1. gera os candidatos: busca aleatória com semente fixa, o mesmo número de
   candidatos em cada uma das 8 combinações janela (0, 10, 50, 100) ×
   normalização (zscore, minmax); 9 pesos com sinal livre, soma dos ``|w|`` = 1,
   arredondados a 6 casas antes de avaliar;
2. pontua todos pelo log loss médio por concurso (calibração pela posição,
   m = 100), somado por dobra, nas 5 dobras da 1c.3;
3. walk-forward aninhado: para i = 2 … 5, o melhor nas dobras 1 … i−1 é
   medido na dobra i (estimativa honesta, contra o acaso K/N);
4. perfil final = o melhor no trecho de busca inteiro, gravado em
   ``<perfis>/<loteria>/busca_v<N>.toml`` (N = a versão seguinte; nunca
   sobrescreve) com o cabeçalho do experimento.

Tudo ou nada: todos os manifestos são lidos e validados (e todas as buscas
rodam) antes de gravar o primeiro ``.toml``; com qualquer entrada recusada,
nada é gravado. A versão ``N`` só olha os arquivos da pasta: o nome não pode
já estar registrado na tabela ``perfis`` de um banco com outro conteúdo (o
Rust recusa; ver :func:`busca.proxima_versao`).

A busca não grava no banco, não roda o trancado e não troca o perfil ativo.
Saída em texto, determinística (o tempo vai para o stderr).

Código de saída: 0 = ok; 2 = entrada recusada (``Erro: …`` no stderr).

Uso (a partir da raiz do repositório)::

    ml/.venv/Scripts/python ml/experimentos/002_busca_pesos.py \\
        datasets/lotofacil/manifesto_parquet.json [mais manifestos…] \\
        [--semente 20261006] [--orcamento 2000] [--processos 4] [--perfis perfis]
"""

from __future__ import annotations

import argparse
import os
import sys
import time
from collections.abc import Mapping
from pathlib import Path

RAIZ_ML = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RAIZ_ML))

from rust_spider_ml import (  # noqa: E402
    COMPONENTES,
    CORTES_BUSCA,
    ErroAvaliacao,
    ErroDataset,
    ErroPerfil,
    ler_dataset,
)
from rust_spider_ml.avaliacao import AvaliacaoFaixa, Comparacao  # noqa: E402
from rust_spider_ml.busca import (  # noqa: E402
    NOME_BASE,
    ORCAMENTO_PADRAO,
    SEMENTE_PADRAO,
    ErroBusca,
    ResultadoBusca,
    buscar,
    gerar_candidatos,
    gravar_perfil,
    soma_abs,
    texto_peso,
)

EXPERIMENTO = "ml/experimentos/002_busca_pesos.py"
SAIDA_ENTRADA = 2
INDISPONIVEL = "n/d"


def _num(valor: float | None, formato: str) -> str:
    return INDISPONIVEL if valor is None else format(valor, formato)


def _ic(c: Comparacao, formato: str) -> str:
    if c.ic95 is None:
        return INDISPONIVEL
    return f"[{format(c.ic95[0], formato)}, {format(c.ic95[1], formato)}]"


def _leitura(c: Comparacao) -> str:
    """Leitura do log loss perfil − acaso (menor é melhor) pelo IC 95% normal.

    Indicativa: o IC normal ignora a correlação serial (sai estreito); o
    veredito formal é o do bootstrap por blocos do ``sync avaliar``."""
    if c.ic95 is None:
        return "sem IC (poucos concursos)"
    inferior, superior = c.ic95
    if inferior <= 0.0 <= superior:
        return "EMPATA com o acaso (o IC 95% contém 0)"
    if inferior > 0.0:
        return "PIOR que o acaso (o IC 95% fica acima de 0)"
    return (
        "ABAIXO do acaso (o IC 95% fica abaixo de 0): tratar como suspeita de vazamento "
        "ou sobreajuste até a validação no trancado"
    )


def _pesos(pesos) -> str:
    return ", ".join(f"{c} {texto_peso(w)}" for c, w in zip(COMPONENTES, pesos))


def _linha_faixa(rotulo: str, f: AvaliacaoFaixa) -> str:
    ll, br, tk = f.log_loss, f.brier, f.topk.acertos
    return (
        f"  {rotulo:>6}  {f.primeiro:>5}–{f.ultimo:<5}  {f.concursos_avaliados:>5}  "
        f"{ll.perfil:.6f}  {ll.acaso:.6f}  {ll.diferenca:+.3e}  {_num(ll.erro_padrao, '.3e'):>9}  "
        f"{br.perfil:.6f}  {br.acaso:.6f}  {br.diferenca:+.3e}  "
        f"{tk.perfil:7.4f}  {tk.acaso:7.4f}  {tk.diferenca:+.4f}"
    )


def relatorio(r: ResultadoBusca, caminho_relativo: str, hash_toml: str) -> list[str]:
    d = r.dataset
    av = r.avaliacao_final
    fa = r.fora_da_amostra
    linhas = [
        f"== {r.loteria} ==",
        f"Dataset: {d['arquivo']} (formato {d['formato']}, feature_version "
        f"{d['feature_version']}, sha256 {d['sha256'][:16]}…), concursos "
        f"{d['primeiro_concurso']}–{d['ultimo_concurso']}, corte da busca {d['corte']}, "
        f"impressão dos incluídos {d['impressao_resultados_incluidos']}.",
        f"Do dataset: N = {av.n_dezenas}, K = {av.k_sorteadas}; acaso p = K/N = "
        f"{av.k_sorteadas / av.n_dezenas:.6f}.",
        f"Busca aleatória: semente {r.semente}, {r.orcamento} candidatos × "
        f"{len(r.combinacoes)} combinações (janela × normalização) = "
        f"{r.candidatos_testados} candidatos testados. Métrica: log loss médio por "
        "concurso, calibração pela posição com m = 100 desde o concurso 1; pesos "
        "normalizados (soma dos |w| = 1) e arredondados a 6 casas antes de avaliar.",
        "Dobras (1c.3): "
        + ", ".join(f"{i} = {a}–{b}" for i, (a, b) in enumerate(r.dobras, start=1))
        + ".",
        "",
        "Walk-forward aninhado (escolhido nas dobras 1…i−1, medido na dobra i):",
        f"  {'teste':>5}  {'escolhido':<24}  {'LL treino dif':>13}  {'concursos':^11}  "
        f"{'T':>5}  {'LL perfil':>9}  {'LL acaso':>9}  {'LL dif':>10}  {'EP':>9}",
    ]
    for p in r.passos:
        t = p.teste
        e = p.escolhido
        rotulo = f"#{e.indice} j{e.janela} {e.normalizacao}"
        linhas.append(
            f"  {p.dobra:>5}  {rotulo:<24}  {p.treino.diferenca:>+13.3e}  "
            f"{t.primeiro:>5}–{t.ultimo:<5}  {t.concursos_avaliados:>5}  "
            f"{t.log_loss.perfil:.7f}  {t.log_loss.acaso:.7f}  {t.log_loss.diferenca:+.3e}  "
            f"{_num(t.log_loss.erro_padrao, '.3e'):>9}"
        )
    linhas.append("  Pesos dos escolhidos:")
    for p in r.passos:
        linhas.append(f"    dobra {p.dobra}: {_pesos(p.escolhido.pesos)}")
    linhas += [
        "",
        f"Fora da amostra (dobras 2–{len(r.dobras)} juntas, {r.concursos_fora_da_amostra} "
        f"concursos): log loss {fa.perfil:.7f} × acaso {fa.acaso:.7f}; dif "
        f"{fa.diferenca:+.3e} (EP {_num(fa.erro_padrao, '.3e')}, IC 95% {_ic(fa, '+.3e')}).",
        "  É a estimativa honesta do MÉTODO (busca + escolha, um escolhido por dobra), não do",
        "  perfil final gravado, que foi escolhido olhando as 5 dobras.",
        f"  Leitura: {_leitura(fa)}.",
        "  O IC normal ignora a correlação serial (sai estreito); o veredito formal é o do",
        "  bootstrap por blocos do `sync avaliar`.",
        "",
        f"Perfil final (o melhor nas dobras 1–{len(r.dobras)}): #{r.final.indice}, janela "
        f"{r.final.janela}, {r.final.normalizacao}; soma dos |w| = "
        f"{soma_abs(r.final.pesos):.6f}.",
        f"  Pesos: {_pesos(r.final.pesos)}",
        f"  Gravado em {caminho_relativo} (hash {hash_toml[:16]}…).",
        f"  A versão só olha os arquivos da pasta: se `{Path(caminho_relativo).stem}` já estiver "
        "registrado na tabela `perfis` de um banco com outro conteúdo, o Rust recusa (perfis",
        "  são imutáveis); suba a versão à mão (arquivo, `nome` e `versao`) antes do `sync indice`.",
        "  Métricas no trecho de busca (DENTRO da amostra: foi escolhido nestes sorteios):",
        f"  {'dobra':>6}  {'concursos':^11}  {'T':>5}  {'LL perfil':>8}  {'LL acaso':>8}  "
        f"{'LL dif':>10}  {'EP':>9}  {'Br perfil':>8}  {'Br acaso':>8}  {'Br dif':>10}  "
        f"{'top-' + str(av.k):>7}  {'acaso':>7}  {'dif':>7}",
    ]
    for i, f in enumerate(av.dobras, start=1):
        linhas.append(_linha_faixa(str(i), f))
    linhas.append(_linha_faixa("total", av.total))
    t = av.total
    linhas += [
        f"  Trecho inteiro: log loss dif {t.log_loss.diferenca:+.3e} (IC {_ic(t.log_loss, '+.3e')}); "
        f"Brier dif {t.brier.diferenca:+.3e} (IC {_ic(t.brier, '+.3e')}); top-{av.k} "
        f"{t.topk.total_acertos} acertos, média {t.topk.acertos.perfil:.4f} × "
        f"{t.topk.acertos.acaso:.4f} (IC da dif {_ic(t.topk.acertos, '+.4f')}).",
        "",
        f"ATENÇÃO: {r.candidatos_testados} candidatos testados. O p-valor do `sync avaliar` "
        "no trecho de busca sai OTIMISTA para o vencedor (escolhido nesses mesmos sorteios);",
        f"  use `sync avaliar {r.loteria} --perfil {Path(caminho_relativo).stem} --testados "
        f"{r.candidatos_testados}` (Bonferroni). A validação que vale é a do trancado, uma",
        "  única vez e por decisão do usuário. Rodar de novo com outra semente e ficar com a",
        "  melhor execução reintroduz o otimismo (conte as execuções). Esperado: empatar com o acaso.",
    ]
    return linhas


def _relativo(caminho: Path, raiz_perfis: Path) -> str:
    """Caminho para o relatório, sem parte absoluta (saída determinística):
    relativo à raiz do repositório ou, fora dela, à pasta de perfis."""
    caminho = caminho.resolve()
    try:
        return caminho.relative_to(RAIZ_ML.parent).as_posix()
    except ValueError:
        return "<perfis>/" + caminho.relative_to(raiz_perfis.resolve()).as_posix()


def main(argv: list[str] | None = None, *, cortes: Mapping[str, int] = CORTES_BUSCA) -> int:
    """Roda o experimento. ``cortes`` só muda nos testes (loterias sintéticas)."""
    for fluxo in (sys.stdout, sys.stderr):
        if hasattr(fluxo, "reconfigure"):
            fluxo.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("manifestos", nargs="+", type=Path)
    ap.add_argument("--semente", type=int, default=SEMENTE_PADRAO)
    ap.add_argument(
        "--orcamento",
        type=int,
        default=ORCAMENTO_PADRAO,
        help="candidatos por combinação janela × normalização",
    )
    ap.add_argument(
        "--processos",
        type=int,
        default=min(4, os.cpu_count() or 1),
        help="processos na pontuação (não muda o resultado)",
    )
    ap.add_argument("--perfis", type=Path, default=RAIZ_ML.parent / "perfis")
    ap.add_argument("--nome", default=NOME_BASE, help="nome base do perfil (busca → busca_v<N>)")
    args = ap.parse_args(argv)

    saida: list[str] = ["Experimento 002 — busca de pesos com walk-forward (1.5.3)", ""]
    try:
        candidatos = gerar_candidatos(args.semente, args.orcamento)
        # Tudo ou nada: lê e valida todos os manifestos e roda todas as buscas
        # antes de gravar o primeiro `.toml`. Uma entrada ruim em qualquer
        # loteria não deixa perfis gravados nas outras (a reexecução criaria
        # busca_v2 idênticos aos v1).
        datasets = [ler_dataset(m, cortes=cortes) for m in args.manifestos]
        vistas: dict[str, Path] = {}
        for manifesto, ds in zip(args.manifestos, datasets):
            if ds.loteria in vistas:
                raise ErroBusca(
                    f"loteria {ds.loteria} repetida ({vistas[ds.loteria]} e {manifesto}): "
                    "gravaria duas versões do mesmo perfil"
                )
            vistas[ds.loteria] = manifesto
        resultados = []
        for ds in datasets:
            inicio = time.perf_counter()
            resultados.append(
                buscar(
                    ds,
                    candidatos,
                    semente=args.semente,
                    orcamento=args.orcamento,
                    processos=args.processos,
                )
            )
            print(
                f"{ds.loteria}: {resultados[-1].candidatos_testados} candidatos em "
                f"{time.perf_counter() - inicio:.1f} s ({args.processos} processo(s)).",
                file=sys.stderr,
                flush=True,
            )
        for resultado in resultados:
            caminho, perfil = gravar_perfil(resultado, args.perfis, EXPERIMENTO, args.nome)
            saida += relatorio(resultado, _relativo(caminho, args.perfis), perfil.hash_toml)
            saida.append("")
    except (ErroDataset, ErroPerfil, ErroAvaliacao, ErroBusca) as e:
        print("\n".join(saida), flush=True)
        print(f"Erro: {e}", file=sys.stderr)
        return SAIDA_ENTRADA
    print("\n".join(saida), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
