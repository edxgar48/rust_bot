"""Experimento 001 — baseline (1.5.2, 2026-10-06).

Para cada dataset (lido pelo manifesto, só o trecho de busca):

(a) o acaso K/N: log loss, Brier e top-k esperado (k·K/N) nas 5 dobras do
    walk-forward do Rust;
(b) um perfil ``.toml`` (padrão ``padrao_v1``) avaliado em Python: índice,
    calibração pela posição (m = 100) e as mesmas métricas.

Com ``--referencia`` (JSON de ``examples/referencia_avaliacao.rs``), confere
(b) com o Rust e mostra a maior diferença por métrica. Saída só em texto,
determinística (sem tempo nem caminhos absolutos). Nenhum modelo de ML.
EP/IC indisponíveis (faixa com menos de 2 concursos) saem como ``n/d``.

Código de saída: 0 = ok; 1 = diverge do Rust (além da tolerância ou
referência de outro perfil/loteria/corte), com a mensagem na saída; 2 =
entrada recusada (dataset, perfil ou referência), com ``Erro: …`` no stderr.
Os cortes da busca vêm sempre de ``CORTES_BUSCA`` (não há opção para trocar).

Uso (a partir da raiz do repositório)::

    ml/.venv/Scripts/python ml/experimentos/001_baseline.py \
        datasets/lotofacil/manifesto_parquet.json [mais manifestos…] \
        [--perfil padrao_v1] [--perfis perfis] [--referencia ref_lotofacil.json …]
"""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Mapping
from pathlib import Path

RAIZ_ML = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RAIZ_ML))

from rust_spider_ml import (  # noqa: E402
    CORTES_BUSCA,
    ErroAvaliacao,
    ErroDataset,
    ErroPerfil,
    avaliar_perfil,
    carregar_perfil,
    ler_dataset,
)
from rust_spider_ml.avaliacao import AvaliacaoFaixa, AvaliacaoPerfil  # noqa: E402
from rust_spider_ml.conferencia import (  # noqa: E402
    TOLERANCIA,
    DivergenciaRust,
    conferir,
    dentro_da_tolerancia,
    ler_referencia,
)


#: Texto no lugar de EP/IC indisponíveis (faixa com menos de 2 concursos).
INDISPONIVEL = "n/d"


def _num(valor: float | None, formato: str) -> str:
    """``valor`` formatado, ou :data:`INDISPONIVEL` se ``None``."""
    return INDISPONIVEL if valor is None else format(valor, formato)


def _ic(ic95: tuple[float, float] | None, formato: str) -> str:
    """``[inf, sup]`` formatado, ou :data:`INDISPONIVEL` se ``None``."""
    if ic95 is None:
        return INDISPONIVEL
    return f"[{format(ic95[0], formato)}, {format(ic95[1], formato)}]"


def _linha_faixa(rotulo: str, f: AvaliacaoFaixa) -> list[str]:
    ll, br, tk = f.log_loss, f.brier, f.topk.acertos
    return [
        f"  {rotulo:>6}  {f.primeiro:>5}–{f.ultimo:<5}  {f.concursos_avaliados:>5}  "
        f"{ll.perfil:.6f}  {ll.acaso:.6f}  {ll.diferenca:+.3e}  {_num(ll.erro_padrao, '.3e'):>9}  "
        f"{br.perfil:.6f}  {br.acaso:.6f}  {br.diferenca:+.3e}  "
        f"{tk.perfil:7.4f}  {tk.acaso:7.4f}  {tk.diferenca:+.4f}  {_num(tk.erro_padrao, '.4f'):>6}"
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
        f"(IC 95% normal {_ic(t.log_loss.ic95, '+.3e')});",
        f"  Brier perfil − acaso: {t.brier.diferenca:+.3e} (IC {_ic(t.brier.ic95, '+.3e')});",
        f"  top-{av.k}: {t.topk.total_acertos} acertos em {t.concursos_avaliados} concursos, "
        f"média {t.topk.acertos.perfil:.4f} × {t.topk.acertos.acaso:.4f} ao acaso "
        f"(IC da diferença {_ic(t.topk.acertos.ic95, '+.4f')}).",
        "  O IC normal ignora a correlação serial (sai estreito); o veredito formal é o do",
        "  bootstrap por blocos do `sync avaliar`. Esperado: empatar com o acaso.",
    ]
    return linhas


#: Códigos de saída: 0 = tudo certo; 1 = Python e Rust divergem (além da
#: tolerância, ou referência de outro perfil/loteria/corte); 2 = entrada
#: recusada (dataset, perfil ou referência inválidos).
SAIDA_DIVERGE = 1
SAIDA_ENTRADA = 2


class ErroEntrada(Exception):
    """Entrada do experimento recusada (mensagem já pronta para o usuário)."""


def _ler_referencias(caminhos: list[Path]) -> dict[str, dict]:
    referencias: dict[str, dict] = {}
    for caminho in caminhos:
        try:
            ref = ler_referencia(caminho)
            loteria = ref["loteria"]
        except FileNotFoundError:
            raise ErroEntrada(f"referência não encontrada: {caminho}") from None
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            raise ErroEntrada(f"referência {caminho}: JSON inválido ({e})") from None
        except (KeyError, TypeError):
            raise ErroEntrada(
                f"referência {caminho}: sem o campo `loteria` (gere com "
                "`cargo run --example referencia_avaliacao`)"
            ) from None
        if loteria in referencias:
            raise ErroEntrada(f"mais de uma referência para a loteria `{loteria}`")
        referencias[loteria] = ref
    return referencias


def _conferencia(av: AvaliacaoPerfil, ref: dict) -> tuple[bool, list[str]]:
    """Linhas da conferência com o Rust e se ela passou."""
    try:
        dif = conferir(av, ref)
    except DivergenciaRust as e:
        return False, [
            "",
            f"Conferência com o Rust: DIVERGE — {e}. A referência não corresponde a esta "
            f"avaliação (perfil {av.perfil.nome}, loteria {av.loteria}, corte {av.corte}): "
            "use a referência gerada com o mesmo perfil e o mesmo dataset.",
        ]
    except (KeyError, TypeError, IndexError) as e:
        return False, [
            "",
            f"Conferência com o Rust: DIVERGE — referência malformada (campo {e}).",
        ]
    ok = dentro_da_tolerancia(dif)
    linhas = [
        "",
        f"Conferência com o Rust (tolerância {TOLERANCIA:g}): "
        f"{'OK' if ok else 'DIVERGE'}; faixas das dobras, T, acertos e distribuição "
        "observada idênticos. Maior |Python − Rust| por métrica (dobras e total):",
    ]
    for metrica in ("log_loss", "brier", "topk"):
        campos = {k.split(".", 1)[1]: v for k, v in dif.items() if k.startswith(metrica + ".")}
        texto = ", ".join(f"{c} {v:.1e}" for c, v in sorted(campos.items()))
        linhas.append(f"  {metrica}: {texto}")
    return ok, linhas


def main(argv: list[str] | None = None, *, cortes: Mapping[str, int] = CORTES_BUSCA) -> int:
    """Roda o experimento. ``cortes`` (tabela do corte da busca) não é opção
    da linha de comando: fora dos testes, vale sempre :data:`CORTES_BUSCA`."""
    for fluxo in (sys.stdout, sys.stderr):
        if hasattr(fluxo, "reconfigure"):
            fluxo.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("manifestos", nargs="+", type=Path)
    ap.add_argument("--perfil", default="padrao_v1")
    ap.add_argument("--perfis", type=Path, default=RAIZ_ML.parent / "perfis")
    ap.add_argument("--referencia", type=Path, action="append", default=[])
    args = ap.parse_args(argv)

    saida: list[str] = ["Experimento 001 — baseline (acaso K/N e perfil refeito em Python)", ""]
    falhou = False
    try:
        referencias = _ler_referencias(args.referencia)
        for manifesto in args.manifestos:
            ds = ler_dataset(manifesto, cortes=cortes)
            perfil = carregar_perfil(args.perfis / ds.loteria / f"{args.perfil}.toml")
            av = avaliar_perfil(ds, perfil)
            saida += relatorio(av, ds)
            ref = referencias.get(ds.loteria)
            if ref is not None:
                ok, linhas = _conferencia(av, ref)
                falhou |= not ok
                saida += linhas
            saida.append("")
    except (ErroEntrada, ErroDataset, ErroPerfil, ErroAvaliacao) as e:
        print("\n".join(saida), flush=True)
        print(f"Erro: {e}", file=sys.stderr)
        return SAIDA_ENTRADA
    print("\n".join(saida), flush=True)
    if falhou:
        print("Erro: a avaliação em Python diverge da referência do Rust.", file=sys.stderr)
        return SAIDA_DIVERGE
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
