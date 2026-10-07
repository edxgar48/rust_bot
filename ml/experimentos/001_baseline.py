"""Experimento 001 — baseline (1.5.2, 2026-10-06).

Para cada dataset (lido pelo manifesto, só o trecho de busca):

(a) o acaso K/N: log loss, Brier e top-k esperado (k·K/N) nas 5 dobras do
    walk-forward do Rust;
(b) um perfil ``.toml`` (padrão ``padrao_v1``) avaliado em Python: índice,
    calibração pela posição (m = 100) e as mesmas métricas.

Com ``--referencia`` (JSON de ``examples/referencia_avaliacao.rs``), confere
(b) com o Rust e mostra a maior diferença por métrica. Saída só em texto,
determinística (sem tempo nem caminhos absolutos). Nenhum modelo de ML.

Uso (a partir da raiz do repositório)::

    ml/.venv/Scripts/python ml/experimentos/001_baseline.py \
        datasets/lotofacil/manifesto_parquet.json [mais manifestos…] \
        [--perfil padrao_v1] [--perfis perfis] [--referencia ref_lotofacil.json …]
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

RAIZ_ML = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RAIZ_ML))

from rust_spider_ml import avaliar_perfil, carregar_perfil, ler_dataset  # noqa: E402
from rust_spider_ml.avaliacao import AvaliacaoFaixa, AvaliacaoPerfil  # noqa: E402
from rust_spider_ml.conferencia import (  # noqa: E402
    TOLERANCIA,
    conferir,
    dentro_da_tolerancia,
    ler_referencia,
)


def _linha_faixa(rotulo: str, f: AvaliacaoFaixa) -> list[str]:
    ll, br, tk = f.log_loss, f.brier, f.topk.acertos
    return [
        f"  {rotulo:>6}  {f.primeiro:>5}–{f.ultimo:<5}  {f.concursos_avaliados:>5}  "
        f"{ll.perfil:.6f}  {ll.acaso:.6f}  {ll.diferenca:+.3e}  {ll.erro_padrao:.3e}  "
        f"{br.perfil:.6f}  {br.acaso:.6f}  {br.diferenca:+.3e}  "
        f"{tk.perfil:7.4f}  {tk.acaso:7.4f}  {tk.diferenca:+.4f}  {tk.erro_padrao:.4f}"
    ]


def relatorio(av: AvaliacaoPerfil, ds) -> list[str]:
    p = av.perfil
    m = ds.manifesto
    linhas = [
        f"== {av.loteria} ==",
        f"Dataset: {m['arquivo']} (formato {ds.formato}, feature_version {ds.feature_version}, "
        f"sha256 {m['sha256'][:16]}…), concursos {ds.primeiro_concurso}–{ds.ultimo_concurso}, "
        f"corte da busca {ds.corte}, trecho busca.",
        f"Do dataset: N = {av.n_dezenas} dezenas, K = {av.k_sorteadas} sorteadas; "
        f"acaso p = K/N = {av.k_sorteadas / av.n_dezenas:.6f}; top-{av.k} esperado "
        f"k·K/N = {av.total.topk.acertos.acaso:.4f}.",
        f"Perfil: {p.nome} (janela {p.janela}, {p.normalizacao}, hash {p.hash_toml[:16]}…), "
        "calibração pela posição com m = 100 desde o concurso 1.",
        "",
        f"  {'dobra':>6}  {'concursos':^11}  {'T':>5}  {'LL perfil':>8}  {'LL acaso':>8}  "
        f"{'LL dif':>10}  {'EP':>9}  {'Br perfil':>8}  {'Br acaso':>8}  {'Br dif':>10}  "
        f"{'top-' + str(av.k):>7}  {'acaso':>7}  {'dif':>7}  {'EP':>6}",
    ]
    for i, d in enumerate(av.dobras, start=1):
        linhas += _linha_faixa(str(i), d)
    linhas += _linha_faixa("total", av.total)
    t = av.total
    linhas += [
        "",
        "Leitura (menor é melhor em LL/Brier; maior é melhor no top-k):",
        f"  log loss perfil − acaso no trecho inteiro: {t.log_loss.diferenca:+.3e} "
        f"(IC 95% normal [{t.log_loss.ic95[0]:+.3e}, {t.log_loss.ic95[1]:+.3e}]);",
        f"  Brier perfil − acaso: {t.brier.diferenca:+.3e} "
        f"(IC [{t.brier.ic95[0]:+.3e}, {t.brier.ic95[1]:+.3e}]);",
        f"  top-{av.k}: {t.topk.total_acertos} acertos em {t.concursos_avaliados} concursos, "
        f"média {t.topk.acertos.perfil:.4f} × {t.topk.acertos.acaso:.4f} ao acaso "
        f"(IC da diferença [{t.topk.acertos.ic95[0]:+.4f}, {t.topk.acertos.ic95[1]:+.4f}]).",
        "  O IC normal ignora a correlação serial (sai estreito); o veredito formal é o do",
        "  bootstrap por blocos do `sync avaliar`. Esperado: empatar com o acaso.",
    ]
    return linhas


def main(argv: list[str] | None = None) -> int:
    for fluxo in (sys.stdout, sys.stderr):
        if hasattr(fluxo, "reconfigure"):
            fluxo.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("manifestos", nargs="+", type=Path)
    ap.add_argument("--perfil", default="padrao_v1")
    ap.add_argument("--perfis", type=Path, default=RAIZ_ML.parent / "perfis")
    ap.add_argument("--referencia", type=Path, action="append", default=[])
    args = ap.parse_args(argv)

    referencias = {}
    for caminho in args.referencia:
        ref = ler_referencia(caminho)
        referencias[ref["loteria"]] = ref

    saida: list[str] = ["Experimento 001 — baseline (acaso K/N e perfil refeito em Python)", ""]
    falhou = False
    for manifesto in args.manifestos:
        ds = ler_dataset(manifesto)
        perfil = carregar_perfil(args.perfis / ds.loteria / f"{args.perfil}.toml")
        av = avaliar_perfil(ds, perfil)
        saida += relatorio(av, ds)
        ref = referencias.get(ds.loteria)
        if ref is not None:
            dif = conferir(av, ref)
            ok = dentro_da_tolerancia(dif)
            falhou |= not ok
            saida += [
                "",
                f"Conferência com o Rust (tolerância {TOLERANCIA:g}): "
                f"{'OK' if ok else 'DIVERGE'}; faixas das dobras, T, acertos e distribuição "
                "observada idênticos. Maior |Python − Rust| por métrica (dobras e total):",
            ]
            for metrica in ("log_loss", "brier", "topk"):
                campos = {k.split(".", 1)[1]: v for k, v in dif.items() if k.startswith(metrica + ".")}
                texto = ", ".join(f"{c} {v:.1e}" for c, v in sorted(campos.items()))
                saida.append(f"  {metrica}: {texto}")
        saida.append("")
    print("\n".join(saida))
    return 1 if falhou else 0


if __name__ == "__main__":
    raise SystemExit(main())
