"""Gera testes/fixtures/acaso-esperado.json (tarefa 7.3), de forma INDEPENDENTE do app.

Uso (a partir da raiz do repositório):
    py -3.12 app/testes/gerar-esperado-acaso.py

O que sai, e de onde vem cada número:

1. HIPERGEOMÉTRICA (pares por sorteio e repetidas do anterior). Calculada de duas formas
   que precisam concordar, senão o script para:
   a) Python exato (math.comb, inteiros): casos[i] = C(S,i)·C(N-S,E-i), total = C(N,E);
   b) o back-end Rust de verdade: um programa mínimo, gerado numa pasta temporária (nada
      do repositório muda), depende do crate `rust_spider` por caminho e chama
      `avaliacao::Hipergeometrica::nova(n, sorteadas, escolhidas)` (src/avaliacao/topk.rs,
      a mesma do top-k da avaliação 1c.2). Compara casos e total inteiro a inteiro, pmf,
      média e variância em ponto flutuante. Precisa de `cargo` e do cache de dependências
      (usa `--offline` e o `target/` do repositório para não recompilar tudo).
   Mapeamento: repetidas do anterior = escolher K dezenas (o sorteio) entre N quando K
   estão marcadas (o anterior) = `Hipergeometrica::da_loteria(cfg, K)`, a MESMA
   distribuição do top-K do back-end; pares = N dezenas, N//2 pares, sorteio de K.
   Antes, confere constantes públicas copiadas de src/avaliacao/topk/tests.rs
   (tabela da Caixa: Lotofácil 3.268.760, Mega-Sena 50.063.860, Quina 24.040.016).

2. BINOMIAL (frequência de uma dezena em N concursos: N tentativas, p = K/N_dezenas).
   O back-end não calcula binomial (o `binomial` de topk.rs é o coeficiente C(n,r)), então
   aqui a referência é só Python, em aritmética inteira exata (numeradores sobre
   total**N, sem ponto flutuante até o fim). Faixa central de 95%: inferior = menor x com
   CDF(x) >= 2,5% e superior = menor x com CDF(x) >= 97,5% (comparações exatas em inteiros);
   `cobertura_real` = P(inferior <= X <= superior) >= 95%. O script exige que nenhum
   limite fique a menos de 1e-9 de um empate com 2,5% / 97,5%.

3. Dezenas fora da faixa, por loteria e janela, usando a frequência de
   fixtures/estatisticas-esperado.json (gerada por SQL no 7.1) e a faixa do item 2.

4. SOMA das dezenas (soma de K dezenas distintas de 1..N, todas as C(N,K) escolhas
   equiprováveis). Contagem exata em inteiros por recorrência de polinômios
   f(n, k) = f(n-1, k) + x^n · f(n-1, k-1) (distinta do laço em BigInt do app). Conferida por:
   soma dos casos = C(N,K); média e variância exatas em Fraction
   (K(N+1)/2 e K(N+1)(N-K)/12); simetria; força bruta com itertools para casos pequenos e para
   a Lotofácil inteira (3.268.760 jogos). Faixa central de 95% pelos mesmos quantis exatos.

5. ATRASO: P(dezena fica >= a concursos seguidos sem sair) = ((N-K)/N)^a, em Fraction
   exata (convertida para float só no fim), e o número esperado de dezenas com esse atraso.

Fora do build do app; rode de novo quando as fixtures forem renovadas.
"""

import json
import math
import subprocess
import sys
import tempfile
from fractions import Fraction
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[2]
FIX = RAIZ / "app" / "testes" / "fixtures"
PARAMS = {"lotofacil": (25, 15), "megasena": (60, 6), "quina": (80, 5)}
JANELAS_NOMINAIS = ["10", "50", "100", "total"]

# Constantes de src/avaliacao/topk/tests.rs (testes que o back-end já passa).
CONSTANTES_RUST = {
    ("lotofacil", 15): (3_268_760, {15: 1, 14: 150, 13: 4_725, 12: 54_600, 11: 286_650, 5: 3_003}),
    ("megasena", 6): (50_063_860, {6: 1, 5: 324, 4: 21_465, 0: 25_827_165}),
    ("quina", 5): (24_040_016, {5: 1, 4: 375, 3: 27_750, 2: 675_250}),
}

PROGRAMA_RUST = """
use rust_spider::avaliacao::Hipergeometrica;

fn main() {
    let casos: [(&str, u32, u32, u32); 6] = [
        ("lotofacil_pares", 25, 12, 15),
        ("lotofacil_repetidas", 25, 15, 15),
        ("megasena_pares", 60, 30, 6),
        ("megasena_repetidas", 60, 6, 6),
        ("quina_pares", 80, 40, 5),
        ("quina_repetidas", 80, 5, 5),
    ];
    for (rot, n, s, e) in casos {
        let h = Hipergeometrica::nova(n, s, e).expect("valida");
        let c: Vec<String> = (0..=e).map(|i| h.casos(i).to_string()).collect();
        let p: Vec<String> = (0..=e).map(|i| format!("{:?}", h.pmf(i))).collect();
        println!("{rot}|{n}|{s}|{e}|{}|{}|{:?}|{:?}|{}", h.total(), c.join(","), h.media(), h.variancia(), p.join(","));
    }
}
"""


def hipergeometrica_python(n, sorteadas, escolhidas):
    total = math.comb(n, escolhidas)
    casos = [math.comb(sorteadas, i) * math.comb(n - sorteadas, escolhidas - i)
             for i in range(escolhidas + 1)]
    assert sum(casos) == total  # Vandermonde
    return total, casos


def rodar_rust():
    """Roda o back-end e devolve {rotulo: dict} com os números dele."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        (tmp / "src").mkdir()
        (tmp / "Cargo.toml").write_text(
            '[package]\nname = "ref_rust"\nversion = "0.0.0"\nedition = "2021"\n\n'
            f'[dependencies]\nrust_spider = {{ path = "{RAIZ.as_posix()}" }}\n',
            encoding="utf-8")
        (tmp / "src" / "main.rs").write_text(PROGRAMA_RUST, encoding="utf-8")
        lock = RAIZ / "Cargo.lock"
        if lock.exists():
            (tmp / "Cargo.lock").write_bytes(lock.read_bytes())
        env = {**__import__("os").environ, "CARGO_TARGET_DIR": str(RAIZ / "target")}
        r = subprocess.run(["cargo", "run", "--quiet", "--offline"], cwd=tmp, env=env,
                           capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit("falha ao rodar o back-end Rust:\n" + r.stderr)
    saida = {}
    for linha in r.stdout.strip().splitlines():
        rot, n, s, e, total, casos, media, var, pmf = linha.split("|")
        saida[rot] = {
            "n": int(n), "sorteadas": int(s), "escolhidas": int(e), "total": int(total),
            "casos": [int(c) for c in casos.split(",")],
            "media": float(media), "variancia": float(var),
            "pmf": [float(p) for p in pmf.split(",")],
        }
    return saida


def gerar_hipergeometrica():
    # 0) as constantes públicas do back-end batem com a aritmética exata deste script.
    for (slug, k), (total, casos) in CONSTANTES_RUST.items():
        n = PARAMS[slug][0]
        t, c = hipergeometrica_python(n, k, k)
        assert t == total, (slug, t, total)
        for i, v in casos.items():
            assert c[i] == v, (slug, i, c[i], v)
    # 1) Python exato contra o Rust, inteiro a inteiro.
    rust = rodar_rust()
    saida = {}
    for rot, r in rust.items():
        n, s, e = r["n"], r["sorteadas"], r["escolhidas"]
        total, casos = hipergeometrica_python(n, s, e)
        assert total == r["total"], rot
        assert casos == r["casos"], rot
        pmf = [float(Fraction(c, total)) for c in casos]
        for a, b in zip(pmf, r["pmf"]):
            assert abs(a - b) <= 1e-15 * max(1.0, abs(a)) + 1e-18, (rot, a, b)
        media = e * s / n
        var = e * (s / n) * ((n - s) / n) * ((n - e) / (n - 1))
        assert abs(media - r["media"]) < 1e-12 and abs(var - r["variancia"]) < 1e-12, rot
        saida[rot] = {
            "n": n, "sorteadas": s, "escolhidas": e,
            "total": str(total), "casos": [str(c) for c in casos],
            "pmf": pmf, "media": r["media"], "variancia": r["variancia"],
            "conferido_com_rust": True,
        }
    return saida


def binomial_exata(n_concursos, k, total_dezenas):
    """Numeradores inteiros de P(X=x), x = 0..N, sobre a base total**N."""
    base = total_dezenas ** n_concursos
    nums = [math.comb(n_concursos, x) * k**x * (total_dezenas - k) ** (n_concursos - x)
            for x in range(n_concursos + 1)]
    assert sum(nums) == base
    return nums, base


def faixa_binomial(n_concursos, k, total_dezenas, cobertura=Fraction(95, 100)):
    nums, base = binomial_exata(n_concursos, k, total_dezenas)
    cauda = (1 - cobertura) / 2  # 2,5% de cada lado
    acum = 0
    cdf = []
    for v in nums:
        acum += v
        cdf.append(acum)
    inferior = next(x for x, c in enumerate(cdf) if Fraction(c, base) >= cauda)
    superior = next(x for x, c in enumerate(cdf) if Fraction(c, base) >= 1 - cauda)
    # nenhum empate "por um fio": a comparação em ponto flutuante do app não pode divergir.
    for x in (inferior - 1, inferior):
        if 0 <= x < len(cdf):
            assert abs(float(Fraction(cdf[x], base) - cauda)) > 1e-9, (n_concursos, k, "inf", x)
    for x in (superior - 1, superior):
        if 0 <= x < len(cdf):
            assert abs(float(Fraction(cdf[x], base) - (1 - cauda))) > 1e-9, (n_concursos, k, "sup", x)
    dentro = cdf[superior] - (cdf[inferior - 1] if inferior > 0 else 0)
    return {
        "inferior": inferior,
        "superior": superior,
        "cobertura_real": float(Fraction(dentro, base)),
        "fora_esperado_por_dezena": float(1 - Fraction(dentro, base)),
    }, nums, base


def gerar_binomial():
    saida = {}
    total_concursos = {}
    for slug in PARAMS:
        h = json.loads((FIX / slug / "historico.json").read_text(encoding="utf-8"))
        total_concursos[slug] = len(h["concursos"])
    stats = json.loads((FIX / "estatisticas-esperado.json").read_text(encoding="utf-8"))
    for slug, (total_dezenas, k) in PARAMS.items():
        ns = sorted({1, 2, 10, 33, 50, 100, total_concursos[slug]})
        janelas = []
        for n in ns:
            faixa, nums, base = faixa_binomial(n, k, total_dezenas)
            p = Fraction(k, total_dezenas)
            media = float(n * p)
            desvio = math.sqrt(float(n * p * (1 - p)))
            pmf = [float(Fraction(v, base)) for v in nums]
            x0 = 0
            if n > 100:  # corta as pontas desprezíveis (< 1e-15) para a fixture ficar pequena
                util = [i for i, v in enumerate(pmf) if v >= 1e-15]
                x0, pmf = util[0], pmf[util[0]: util[-1] + 1]
            janelas.append({
                "n": n, "media": media, "desvio": desvio,
                "inferior": faixa["inferior"], "superior": faixa["superior"],
                "cobertura_real": faixa["cobertura_real"],
                "fora_esperado_por_dezena": faixa["fora_esperado_por_dezena"],
                "x0": x0, "pmf": pmf,
            })
        # dezenas fora da faixa, por janela nominal, a partir da frequência por SQL (7.1)
        fora = {}
        for nome in JANELAS_NOMINAIS:
            j = stats["loterias"][slug][nome]
            n = j["tamanho"]
            faixa = next(w for w in janelas if w["n"] == n) if any(w["n"] == n for w in janelas) \
                else None
            if faixa is None:
                f, _, _ = faixa_binomial(n, k, total_dezenas)
            else:
                f = faixa
            abaixo = [d + 1 for d, v in enumerate(j["frequencia"]) if v < f["inferior"]]
            acima = [d + 1 for d, v in enumerate(j["frequencia"]) if v > f["superior"]]
            fora[nome] = {"n": n, "inferior": f["inferior"], "superior": f["superior"],
                          "abaixo": abaixo, "acima": acima}
        saida[slug] = {"total_dezenas": total_dezenas, "k": k, "janelas": janelas, "fora": fora}
    return saida


def polinomio_somas(n, k):
    """Coeficientes de f(n, k): casos[s] = subconjuntos de k de 1..n com soma s."""
    memo = {}

    def f(nn, kk):
        if kk == 0:
            return [1]
        if kk > nn:
            return [0]
        if (nn, kk) in memo:
            return memo[(nn, kk)]
        a = f(nn - 1, kk)
        b = [0] * nn + f(nn - 1, kk - 1)  # multiplica por x^nn
        tam = max(len(a), len(b))
        r = [(a[i] if i < len(a) else 0) + (b[i] if i < len(b) else 0) for i in range(tam)]
        memo[(nn, kk)] = r
        return r

    sys.setrecursionlimit(10000)
    r = f(n, k)
    return r + [0] * (k * n + 1 - len(r))


def somas_forca_bruta(n, k):
    from collections import Counter
    from itertools import combinations
    c = Counter(map(sum, combinations(range(1, n + 1), k)))
    return [c.get(s, 0) for s in range(k * n + 1)]


def gerar_soma():
    for n, k in [(6, 3), (10, 4), (12, 5), (9, 9), (7, 0)]:
        assert polinomio_somas(n, k) == somas_forca_bruta(n, k), (n, k)
    saida = {}
    for slug, (n, k) in PARAMS.items():
        casos = polinomio_somas(n, k)
        total = math.comb(n, k)
        assert sum(casos) == total, slug
        minimo = k * (k + 1) // 2
        maximo = k * (2 * n - k + 1) // 2
        assert all(c == 0 for c in casos[:minimo]) and all(c > 0 for c in casos[minimo:maximo + 1])
        assert all(c == 0 for c in casos[maximo + 1:])
        media = Fraction(sum(s * c for s, c in enumerate(casos)), total)
        var = Fraction(sum(s * s * c for s, c in enumerate(casos)), total) - media ** 2
        assert media == Fraction(k * (n + 1), 2), slug
        assert var == Fraction(k * (n + 1) * (n - k), 12), slug
        assert all(casos[minimo + i] == casos[maximo - i] for i in range(maximo - minimo + 1))
        if slug == "lotofacil":
            assert casos == somas_forca_bruta(n, k), slug  # 3.268.760 jogos, um a um
        cobertura = Fraction(95, 100)
        cauda = (1 - cobertura) / 2
        acum, cdf = 0, []
        for c in casos:
            acum += c
            cdf.append(acum)
        inferior = next(x for x, c in enumerate(cdf) if Fraction(c, total) >= cauda)
        superior = next(x for x, c in enumerate(cdf) if Fraction(c, total) >= 1 - cauda)
        for x in (inferior - 1, inferior):
            assert abs(float(Fraction(cdf[x], total) - cauda)) > 1e-9, (slug, "inf", x)
        for x in (superior - 1, superior):
            assert abs(float(Fraction(cdf[x], total) - (1 - cauda))) > 1e-9, (slug, "sup", x)
        dentro = cdf[superior] - (cdf[inferior - 1] if inferior > 0 else 0)
        saida[slug] = {
            "total_dezenas": n, "k": k, "minimo": minimo, "maximo": maximo,
            "total_casos": str(total),
            "casos": [str(c) for c in casos[minimo:maximo + 1]],  # indice 0 = soma `minimo`
            "media": float(media), "variancia": float(var),
            "inferior": inferior, "superior": superior,
            "cobertura_real": float(Fraction(dentro, total)),
            "fora_esperado_por_sorteio": float(1 - Fraction(dentro, total)),
            "conferido_forca_bruta": slug == "lotofacil",
        }
    return saida


def gerar_atraso():
    saida = {}
    for slug, (n, k) in PARAMS.items():
        q = Fraction(n - k, n)
        pontos = []
        for a in [0, 1, 2, 3, 5, 8, 10, 15, 20, 30, 50, 75, 100, 200, 300]:
            p = q ** a
            pontos.append({"atraso": a, "chance": float(p), "dezenas_esperadas": float(n * p)})
        saida[slug] = {"total_dezenas": n, "k": k, "pontos": pontos}
    return saida


def main():
    obj = {
        "gerado_de": "Python exato (math.comb) conferido com o back-end Rust "
                     "(avaliacao::Hipergeometrica) e binomial exata em inteiros",
        "cobertura": 0.95,
        "hipergeometrica": gerar_hipergeometrica(),
        "binomial": gerar_binomial(),
        "soma": gerar_soma(),
        "atraso": gerar_atraso(),
    }
    texto = json.dumps(obj, ensure_ascii=False, indent=1) + "\n"
    (FIX / "acaso-esperado.json").write_text(texto, encoding="utf-8", newline="\n")
    print("ok", len(texto), "bytes")


if __name__ == "__main__":
    main()
