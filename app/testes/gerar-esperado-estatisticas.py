"""Gera testes/fixtures/estatisticas-esperado.json de forma INDEPENDENTE do app.

Uso (a partir da raiz do repositório):
    py -3.12 app/testes/gerar-esperado-estatisticas.py

Lê os bancos reais resultados_<loteria>.db e usa só consultas SQL (json_each do
SQLite); nenhum código do app é usado. Antes, confere que o historico.json de
app/testes/fixtures/<loteria>/ é exatamente o que o banco tem até o último concurso
da fixture; o esperado é calculado até esse concurso (o banco pode estar adiante).

Saída por loteria e por janela (10, 50, 100, total): tamanho, primeiro/último concurso,
frequencia[d-1], atraso[d-1] (null = não saiu na janela), distribuicao_pares,
soma_total/min/max (inteiros, para a média ser calculada pelo teste), e
distribuicao_repetidas (só concursos com o anterior na janela).
Fora do build do app; rode de novo quando as fixtures forem renovadas.
"""
import json
import sqlite3
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[2]
FIX = RAIZ / "app" / "testes" / "fixtures"
PARAMS = {"lotofacil": (25, 15), "megasena": (60, 6), "quina": (80, 5)}
JANELAS = [10, 50, 100, "total"]


def gerar(slug):
    total, k = PARAMS[slug]
    fix = json.loads((FIX / slug / "historico.json").read_text(encoding="utf-8"))
    ultimo = fix["concursos"][-1][0]
    con = sqlite3.connect(RAIZ / f"resultados_{slug}.db")
    linhas = con.execute(
        "SELECT concurso, data_sorteio, dezenas FROM resultados "
        "WHERE concurso <= ? ORDER BY concurso", (ultimo,)).fetchall()
    esperado_fix = [[c, d, json.loads(z)] for c, d, z in linhas]
    if esperado_fix != fix["concursos"]:
        sys.exit(f"{slug}: a fixture historico.json difere do banco")
    numeros = [c for c, _, _ in linhas]
    if numeros != list(range(numeros[0], numeros[0] + len(numeros))):
        sys.exit(f"{slug}: os concursos do banco não são seguidos")
    primeiro = linhas[0][0]
    saida = {}
    for j in JANELAS:
        ini = primeiro if j == "total" else max(primeiro, ultimo - j + 1)
        p = (ini, ultimo)
        freq = dict(con.execute(
            "SELECT e.value, COUNT(*) FROM resultados r, json_each(r.dezenas) e "
            "WHERE r.concurso BETWEEN ? AND ? GROUP BY e.value", p).fetchall())
        ult = dict(con.execute(
            "SELECT e.value, MAX(r.concurso) FROM resultados r, json_each(r.dezenas) e "
            "WHERE r.concurso BETWEEN ? AND ? GROUP BY e.value", p).fetchall())
        pares = [0] * (k + 1)
        for (n,) in con.execute(
            "SELECT (SELECT COUNT(*) FROM json_each(r.dezenas) e WHERE e.value % 2 = 0) "
            "FROM resultados r WHERE r.concurso BETWEEN ? AND ?", p):
            pares[n] += 1
        somas = [s for (s,) in con.execute(
            "SELECT (SELECT SUM(e.value) FROM json_each(r.dezenas) e) "
            "FROM resultados r WHERE r.concurso BETWEEN ? AND ?", p)]
        rep = [0] * (k + 1)
        for (n,) in con.execute(
            "SELECT (SELECT COUNT(*) FROM json_each(a.dezenas) x "
            "JOIN json_each(b.dezenas) y ON x.value = y.value) "
            "FROM resultados a JOIN resultados b ON b.concurso = a.concurso - 1 "
            "WHERE a.concurso BETWEEN ? AND ? AND b.concurso >= ?", (ini, ultimo, ini)):
            rep[n] += 1
        saida[str(j)] = {
            "tamanho": ultimo - ini + 1,
            "primeiro_concurso": ini,
            "ultimo_concurso": ultimo,
            "frequencia": [freq.get(d, 0) for d in range(1, total + 1)],
            "atraso": [ultimo - ult[d] if d in ult else None for d in range(1, total + 1)],
            "distribuicao_pares": pares,
            "soma_total": sum(somas),
            "soma_min": min(somas),
            "soma_max": max(somas),
            "distribuicao_repetidas": rep,
        }
    return saida


def main():
    obj = {"gerado_de": "resultados_<loteria>.db via SQL (json_each)", "loterias": {}}
    for slug in PARAMS:
        obj["loterias"][slug] = gerar(slug)
    texto = json.dumps(obj, ensure_ascii=False, indent=1) + "\n"
    (FIX / "estatisticas-esperado.json").write_text(texto, encoding="utf-8", newline="\n")
    print("ok")


if __name__ == "__main__":
    main()
