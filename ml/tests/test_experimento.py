"""O experimento 001 roda na fixture, confere com o Rust e é determinístico."""

from __future__ import annotations

import importlib.util
import json

import pytest
from conftest import CORTES_TESTE, FIXTURES, RAIZ_REPO, escrever_dataset, linhas_sinteticas

CAMINHO = RAIZ_REPO / "ml" / "experimentos" / "001_baseline.py"


def _modulo():
    spec = importlib.util.spec_from_file_location("experimento_001", CAMINHO)
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


def test_001_baseline_deterministico_e_confere(capsys):
    exp = _modulo()
    args = [
        str(FIXTURES / "lotofacil" / "manifesto_parquet.json"),
        str(FIXTURES / "lotofacil" / "manifesto_csv.json"),
        "--referencia",
        str(FIXTURES / "lotofacil" / "referencia_padrao_v1.json"),
    ]
    assert exp.main(args) == 0
    primeira = capsys.readouterr().out
    assert exp.main(args) == 0
    assert capsys.readouterr().out == primeira
    assert primeira.count("Conferência com o Rust (tolerância 1e-12): OK") == 2
    assert "N = 25 dezenas, K = 15 sorteadas" in primeira


# --- (a) EP/IC indisponíveis -------------------------------------------------

PERFIL_SINTETICO = """
nome = "teste_v1"
versao = 1
loteria = "teste"
janela = 10
normalizacao = "zscore"

[pesos]
frequencia = 1.0
maior_espaco = 1.0
maior_espaco_ocorrencias = 0.3
menor_espaco = 0.5
menor_espaco_ocorrencias = 0.3
soma_variacoes = -0.8
espaco_final = 1.2
repeticoes_total = 0.7
repeticao_atual = 0.4
"""


def _perfis_sinteticos(tmp_path):
    pasta = tmp_path / "perfis"
    (pasta / "teste").mkdir(parents=True)
    (pasta / "teste" / "teste_v1.toml").write_text(PERFIL_SINTETICO, encoding="utf-8")
    return pasta


def test_001_baseline_formata_ep_e_ic_indisponiveis(tmp_path, capsys):
    # 6 concursos: avaliados 2…6, uma dobra por concurso → EP/IC das dobras
    # são None (menos de 2 concursos) e antes quebravam com TypeError.
    exp = _modulo()
    m = escrever_dataset(tmp_path / "ds", linhas_sinteticas(t=6))
    args = [str(m), "--perfil", "teste_v1", "--perfis", str(_perfis_sinteticos(tmp_path))]
    assert exp.main(args, cortes=CORTES_TESTE) == 0
    saida = capsys.readouterr().out
    linhas_dobras = [l for l in saida.splitlines() if l.strip().startswith(("1 ", "2 ", "3 "))]
    assert len(linhas_dobras) == 3
    assert all(l.count(exp.INDISPONIVEL) == 2 for l in linhas_dobras)
    # O total (5 concursos) tem EP e IC.
    assert "IC 95% normal [" in saida


def test_001_baseline_formatadores_de_valor_indisponivel():
    exp = _modulo()
    assert exp._num(None, ".3e") == "n/d"
    assert exp._num(0.5, ".3e") == "5.000e-01"
    assert exp._ic(None, "+.3e") == "n/d"
    assert exp._ic((-1.0, 2.0), "+.1f") == "[-1.0, +2.0]"


def test_001_baseline_recusa_loteria_sintetica_sem_tabela_injetada(tmp_path, capsys):
    # Pela linha de comando vale sempre a tabela de produção.
    exp = _modulo()
    m = escrever_dataset(tmp_path / "ds", linhas_sinteticas(t=6))
    args = [str(m), "--perfil", "teste_v1", "--perfis", str(_perfis_sinteticos(tmp_path))]
    assert exp.main(args) == exp.SAIDA_ENTRADA
    erro = capsys.readouterr().err
    assert "Erro: loteria `teste` desconhecida" in erro
    assert "Traceback" not in erro


# --- (b) referência de outro perfil -------------------------------------------


def test_001_baseline_referencia_de_outro_perfil_sai_com_mensagem(capsys):
    exp = _modulo()
    args = [
        str(FIXTURES / "lotofacil" / "manifesto_csv.json"),
        "--referencia",
        str(FIXTURES / "lotofacil" / "referencia_teste_minmax_v1.json"),
    ]
    assert exp.main(args) == exp.SAIDA_DIVERGE != 0
    cap = capsys.readouterr()
    assert (
        "Conferência com o Rust: DIVERGE — perfil: 'padrao_v1' × Rust 'teste_minmax_v1'" in cap.out
    )
    assert "use a referência gerada com o mesmo perfil" in cap.out
    assert "diverge da referência do Rust" in cap.err


@pytest.mark.parametrize(
    "conteudo, mensagem",
    [
        (None, "referência não encontrada"),
        ("{", "JSON inválido"),
        ("{}", "sem o campo `loteria`"),
    ],
)
def test_001_baseline_referencia_invalida_sai_com_mensagem(tmp_path, capsys, conteudo, mensagem):
    exp = _modulo()
    ref = tmp_path / "ref.json"
    if conteudo is not None:
        ref.write_text(conteudo, encoding="utf-8")
    args = [str(FIXTURES / "lotofacil" / "manifesto_csv.json"), "--referencia", str(ref)]
    assert exp.main(args) == exp.SAIDA_ENTRADA
    assert mensagem in capsys.readouterr().err


def test_001_baseline_referencia_malformada_diverge(tmp_path, capsys):
    exp = _modulo()
    ref = json.loads((FIXTURES / "lotofacil" / "referencia_padrao_v1.json").read_text("utf-8"))
    del ref["walk_forward"]
    caminho = tmp_path / "ref.json"
    caminho.write_text(json.dumps(ref), encoding="utf-8")
    args = [str(FIXTURES / "lotofacil" / "manifesto_csv.json"), "--referencia", str(caminho)]
    assert exp.main(args) == exp.SAIDA_DIVERGE
    assert "referência malformada" in capsys.readouterr().out
