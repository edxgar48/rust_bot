"""Busca de pesos (1.5.3): pontuação em lote igual à avaliação, determinismo,
walk-forward sem olhar a dobra de teste, invariância à escala, ``.toml``
relido sem erro e o experimento 002."""

from __future__ import annotations

import dataclasses
import importlib.util

import numpy as np
import pytest
from conftest import CORTES_TESTE, FIXTURES, RAIZ_REPO, escrever_dataset, linhas_sinteticas

from rust_spider_ml.avaliacao import avaliar_perfil
from rust_spider_ml.busca import (
    CASAS_PESOS,
    Candidato,
    ErroBusca,
    buscar,
    combinacoes,
    gerar_candidatos,
    gravar_perfil,
    log_loss_por_concurso_lote,
    normalizados,
    normalizar_pesos,
    pontuar,
    proxima_versao,
    series_por_concurso,
    soma_abs,
)
from rust_spider_ml.dataset import COMPONENTES, ler_dataset
from rust_spider_ml.perfil import carregar_perfil, interpretar_perfil

ORCAMENTO = 6  # por combinação: 48 candidatos na fixture


@pytest.fixture(scope="module")
def ds():
    return ler_dataset(FIXTURES / "lotofacil" / "manifesto_parquet.json")


@pytest.fixture(scope="module")
def candidatos():
    return gerar_candidatos(semente=123, orcamento=ORCAMENTO)


@pytest.fixture(scope="module")
def resultado(ds, candidatos):
    return buscar(ds, candidatos, semente=123, orcamento=ORCAMENTO)


def _assinatura(r):
    """O que a busca decide e mede (para comparar duas execuções)."""
    return (
        [(p.dobra, p.escolhido) for p in r.passos],
        r.final,
        r.fora_da_amostra,
        r.avaliacao_final.total,
        r.avaliacao_final.dobras,
    )


# --- pesos e candidatos ---------------------------------------------------------


def test_normalizar_pesos_soma_um_seis_casas_e_sem_menos_zero():
    w = normalizar_pesos([3.0, -1.0, 0.0, 2.0, -0.0000000001, 1.0, 1.0, 1.0, 1.0])
    assert abs(soma_abs(w) - 1.0) <= 9 * 0.5 * 10.0**-CASAS_PESOS
    assert all(round(x, CASAS_PESOS) == x for x in w)
    assert w[0] == 0.3 and w[1] == -0.1
    assert w[4] == 0.0 and str(w[4]) == "0.0"  # nada de -0.0 no .toml


@pytest.mark.parametrize(
    "pesos",
    [[0.0] * 9, [0.0] * 8, [1.0, float("nan")] + [0.0] * 7, [float("inf")] + [0.0] * 8],
)
def test_normalizar_pesos_recusa(pesos):
    with pytest.raises(ErroBusca):
        normalizar_pesos(pesos)


def _brutos(qtd: int, semente: int = 11) -> np.ndarray:
    return np.random.default_rng(semente).standard_normal((qtd, 9))


@pytest.mark.parametrize("fator", [1e-3, 0.37, 2.0, 7.3, 1e6])
def test_multiplicar_pesos_por_constante_positiva_nao_muda_o_candidato(fator):
    for bruto in _brutos(200):
        assert normalizar_pesos(fator * bruto) == normalizar_pesos(bruto)


def test_mesma_semente_mesmos_candidatos_e_orcamento_menor_e_prefixo():
    a = gerar_candidatos(semente=7, orcamento=5)
    assert a == gerar_candidatos(semente=7, orcamento=5)
    assert a != gerar_candidatos(semente=8, orcamento=5)
    assert len(a) == 5 * len(combinacoes()) == 40
    maior = gerar_candidatos(semente=7, orcamento=9)
    for i, (j, n) in enumerate(combinacoes()):
        bloco = [c for c in a if (c.janela, c.normalizacao) == (j, n)]
        bloco_maior = [c for c in maior if (c.janela, c.normalizacao) == (j, n)]
        assert [c.pesos for c in bloco] == [c.pesos for c in bloco_maior[:5]], (j, n)
        assert [c.indice for c in bloco] == list(range(5 * i, 5 * i + 5))
    assert all(any(w < 0 for w in c.pesos) for c in a[:10])  # sinal livre


# --- pontuação em lote = avaliação de um perfil ------------------------------------


def _casos(ds):
    cands = gerar_candidatos(semente=5, orcamento=2)
    # Bordas: um só componente (empates de posição em massa), sinais negativos.
    extremos = []
    for i in (0, 8):
        for sinal in (1.0, -1.0):
            w = [0.0] * 9
            w[i] = sinal
            for j, n in combinacoes():
                extremos.append(Candidato(0, j, n, normalizar_pesos(w)))
    return cands + extremos


def test_lote_igual_bit_a_bit_a_avaliacao(ds):
    casos = _casos(ds)
    for (j, n) in combinacoes():
        grupo = [c for c in casos if (c.janela, c.normalizacao) == (j, n)]
        pesos = np.array([c.pesos for c in grupo])
        ll = log_loss_por_concurso_lote(normalizados(ds, j, n), pesos, ds.saiu, ds.k_sorteadas)
        for c, linha in zip(grupo, ll):
            esperado = series_por_concurso(ds, c.perfil(ds.loteria))["log_loss_perfil"]
            assert np.array_equal(linha, esperado), (c, np.abs(linha - esperado).max())


def test_lote_igual_a_avaliacao_com_nulls(tmp_path):
    """Dataset sintético com NULL (≈30% dos opcionais) e N pequeno."""
    m = escrever_dataset(tmp_path / "ds", linhas_sinteticas(t=30, n=8, k=3, semente=4))
    d = ler_dataset(m, cortes=CORTES_TESTE)
    cands = gerar_candidatos(semente=9, orcamento=3, janelas=(0, 10))
    for c in cands:
        ll = log_loss_por_concurso_lote(
            normalizados(d, c.janela, c.normalizacao), np.array([c.pesos]), d.saiu, d.k_sorteadas
        )[0]
        assert np.array_equal(ll, series_por_concurso(d, c.perfil("teste"))["log_loss_perfil"])


def test_pontuacao_soma_por_dobra_bate_com_avaliar_perfil(ds, candidatos):
    pont = pontuar(ds, candidatos[:8], elementos_por_lote=1)  # lote de 1: outro caminho
    for i, c in enumerate(candidatos[:8]):
        av = avaliar_perfil(ds, c.perfil(ds.loteria))
        for d, faixa in enumerate(av.dobras):
            assert pont.concursos[d] == faixa.concursos_avaliados
            media = pont.somas[i, d] / pont.concursos[d]
            assert media == pytest.approx(faixa.log_loss.perfil, abs=1e-15)
        assert pont.media(range(1, 6))[i] == pytest.approx(av.total.log_loss.perfil, abs=1e-14)


def test_processos_nao_mudam_a_pontuacao(ds, candidatos):
    um = pontuar(ds, candidatos, processos=1)
    dois = pontuar(ds, candidatos, processos=2, elementos_por_lote=25 * 40 * 3)
    assert np.array_equal(um.somas, dois.somas)
    assert np.array_equal(um.concursos, dois.concursos)


# --- busca e walk-forward ----------------------------------------------------------


def test_mesma_semente_mesmo_resultado(ds, resultado):
    de_novo = buscar(ds, gerar_candidatos(semente=123, orcamento=ORCAMENTO), semente=123,
                     orcamento=ORCAMENTO)
    assert _assinatura(de_novo) == _assinatura(resultado)
    outra = buscar(ds, gerar_candidatos(semente=124, orcamento=ORCAMENTO))
    assert outra.final != resultado.final


def test_walk_forward_estrutura(ds, resultado, candidatos):
    assert [p.dobra for p in resultado.passos] == [2, 3, 4, 5]
    assert resultado.candidatos_testados == len(candidatos) == 8 * ORCAMENTO
    assert resultado.dobras == tuple(tuple(x) for x in resultado.dobras)
    pont = pontuar(ds, candidatos)
    for p in resultado.passos:
        medias = pont.media(range(1, p.dobra))
        assert p.escolhido == candidatos[int(np.argmin(medias))]
        # o escolhido é medido na dobra de teste com a avaliação completa
        assert p.teste.primeiro == resultado.dobras[p.dobra - 1][0]
    assert resultado.final == candidatos[int(np.argmin(pont.media(range(1, 6))))]
    # Fora da amostra = os concursos das dobras 2–5, cada um com o seu escolhido.
    assert resultado.concursos_fora_da_amostra == sum(
        p.teste.concursos_avaliados for p in resultado.passos
    )
    soma = sum(p.teste.log_loss.perfil * p.teste.concursos_avaliados for p in resultado.passos)
    assert resultado.fora_da_amostra.perfil == pytest.approx(
        soma / resultado.concursos_fora_da_amostra, abs=1e-14
    )


def _baguncar_desde(ds, primeiro_concurso: int, semente: int):
    """Troca sorteios (mantendo K) e features de todo concurso ≥
    ``primeiro_concurso``: a dobra de teste e as seguintes."""
    g = np.random.default_rng(semente)
    linhas = ds.concursos >= primeiro_concurso
    saiu = ds.saiu.copy()
    for t in np.flatnonzero(linhas):
        saiu[t] = g.permutation(saiu[t])
    componentes = {}
    for j, comp in ds.componentes.items():
        novo = comp.copy()
        bloco = novo[linhas]
        nulos = np.isnan(bloco)
        bloco = g.integers(0, 30, size=bloco.shape).astype(np.float64)
        bloco[nulos] = np.nan
        novo[linhas] = bloco
        componentes[j] = novo
    return dataclasses.replace(ds, saiu=saiu, componentes=componentes)


@pytest.mark.parametrize("dobra", [2, 3, 4, 5])
def test_dobra_de_teste_nao_influencia_a_escolha(ds, candidatos, resultado, dobra):
    inicio = resultado.dobras[dobra - 1][0]
    for semente in (1, 2):
        alterado = _baguncar_desde(ds, inicio, semente)
        r = buscar(alterado, candidatos)
        antes = {p.dobra: p.escolhido for p in resultado.passos}
        depois = {p.dobra: p.escolhido for p in r.passos}
        for i in range(2, dobra + 1):
            assert depois[i] == antes[i], (dobra, i)
        # Mais forte que o argmin: as somas das dobras de treino são idênticas.
        pa, pd = pontuar(ds, candidatos), pontuar(alterado, candidatos)
        assert np.array_equal(pa.somas[:, : dobra - 1], pd.somas[:, : dobra - 1])
        assert not np.array_equal(pa.somas[:, dobra - 1], pd.somas[:, dobra - 1])
        # A alteração muda o que é medido na dobra de teste (o teste não é vazio).
        teste_antes = resultado.passos[dobra - 2].teste.log_loss.perfil
        assert r.passos[dobra - 2].teste.log_loss.perfil != teste_antes


def _de_brutos(brutos: np.ndarray, fatores: np.ndarray | None = None) -> list[Candidato]:
    combos = combinacoes()
    fatores = np.ones(len(brutos)) if fatores is None else fatores
    return [
        Candidato(i, *combos[i % len(combos)], normalizar_pesos(f * b))
        for i, (b, f) in enumerate(zip(brutos, fatores))
    ]


@pytest.mark.parametrize("t0", [5, 17, 30])
def test_log_loss_de_t_nao_depende_de_concursos_futuros(ds, t0):
    """Alterar sorteios e features de t0 em diante não muda o log loss por
    concurso antes de t0 (calibração e índice só com o passado)."""
    alterado = _baguncar_desde(ds, t0, semente=t0)
    linhas = ds.concursos < t0
    for c in gerar_candidatos(semente=31, orcamento=2):
        pesos = np.array([c.pesos])
        a = log_loss_por_concurso_lote(
            normalizados(ds, c.janela, c.normalizacao), pesos, ds.saiu, ds.k_sorteadas
        )[0]
        b = log_loss_por_concurso_lote(
            normalizados(alterado, c.janela, c.normalizacao), pesos, alterado.saiu,
            alterado.k_sorteadas,
        )[0]
        assert np.array_equal(a[linhas], b[linhas])
        assert not np.array_equal(a, b)


def test_multiplicar_pesos_nao_muda_o_resultado(ds):
    brutos = _brutos(48, semente=21)
    fatores = np.exp(np.random.default_rng(3).uniform(-6.0, 6.0, size=len(brutos)))
    base = _de_brutos(brutos)
    escalados = _de_brutos(brutos, fatores)
    assert escalados == base
    assert _assinatura(buscar(ds, escalados)) == _assinatura(buscar(ds, base))
    # Sem renormalizar: multiplicar os pesos do índice por potência de 2 (exata)
    # não muda nenhuma posição, logo nenhum log loss.
    for c in base[:8]:
        z = normalizados(ds, c.janela, c.normalizacao)
        ll = log_loss_por_concurso_lote(z, np.array([c.pesos]), ds.saiu, ds.k_sorteadas)
        for fator in (0.5, 4.0, 1024.0):
            escalado = np.array([[fator * w for w in c.pesos]])
            assert np.array_equal(
                log_loss_por_concurso_lote(z, escalado, ds.saiu, ds.k_sorteadas), ll
            ), (c.indice, fator)


def test_janela_ausente_e_recusada(tmp_path):
    m = escrever_dataset(tmp_path / "ds", linhas_sinteticas(t=12))
    d = ler_dataset(m, cortes=CORTES_TESTE)
    with pytest.raises(ErroBusca, match="janela"):
        buscar(d, gerar_candidatos(orcamento=1))


# --- perfil .toml ------------------------------------------------------------------


def test_toml_gravado_e_relido_sem_erro_e_versao_seguinte(tmp_path, resultado):
    raiz = tmp_path / "perfis"
    caminho, perfil = gravar_perfil(resultado, raiz, "teste")
    assert caminho == raiz / "lotofacil" / "busca_v1.toml"
    bytes_ = caminho.read_bytes()
    assert b"\r" not in bytes_ and bytes_.endswith(b"\n")
    relido = carregar_perfil(caminho)
    assert relido == perfil
    assert relido.nome == "busca_v1" and relido.versao == 1 and relido.loteria == "lotofacil"
    assert (relido.janela, relido.normalizacao, relido.pesos) == (
        resultado.final.janela,
        resultado.final.normalizacao,
        resultado.final.pesos,
    )
    # Reavaliado já arredondado: o perfil relido mede o mesmo da busca.
    assert avaliar_perfil(ler_dataset(FIXTURES / "lotofacil" / "manifesto_parquet.json"),
                          relido).total == resultado.avaliacao_final.total
    texto = bytes_.decode("utf-8")
    for trecho in ("Semente 123", f"{resultado.candidatos_testados} candidatos testados",
                   "feature_version 1", "concursos 1–40", "impressão dos incluídos fnv1a64:40:",
                   "Fora da amostra (dobras 2–5", "× acaso"):
        assert trecho in texto, trecho
    # Já existe: vai para a versão seguinte, sem tocar na anterior.
    caminho2, perfil2 = gravar_perfil(resultado, raiz, "teste")
    assert caminho2.name == "busca_v2.toml" and perfil2.versao == 2
    assert caminho.read_bytes() == bytes_
    assert proxima_versao(raiz / "lotofacil") == 3


def test_toml_tem_as_chaves_e_pesos_do_formato_do_rust(tmp_path, resultado):
    caminho, _ = gravar_perfil(resultado, tmp_path, "teste")
    texto = caminho.read_text(encoding="utf-8")
    corpo = [l for l in texto.splitlines() if l and not l.startswith("#")]
    chaves = [l.split("=")[0].strip() for l in corpo if "=" in l]
    assert chaves == ["nome", "versao", "loteria", "janela", "normalizacao", *COMPONENTES]
    for l in corpo[corpo.index("[pesos]") + 1 :]:
        valor = l.split("=")[1].strip()
        assert "." in valor and len(valor.split(".")[1]) == CASAS_PESOS, l
    interpretar_perfil(texto)  # mesmas regras do src/perfil.rs


# --- experimento 002 ---------------------------------------------------------------


def _experimento():
    caminho = RAIZ_REPO / "ml" / "experimentos" / "002_busca_pesos.py"
    spec = importlib.util.spec_from_file_location("experimento_002", caminho)
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


def test_002_deterministico_grava_perfil_e_avisa(tmp_path, capsys):
    exp = _experimento()
    manifesto = str(FIXTURES / "lotofacil" / "manifesto_parquet.json")
    saidas = []
    for pasta in ("a", "b"):
        args = [manifesto, "--orcamento", "3", "--processos", "1", "--perfis",
                str(tmp_path / pasta)]
        assert exp.main(args) == 0
        saidas.append(capsys.readouterr().out)
    assert saidas[0] == saidas[1]
    texto = saidas[0]
    assert "24 candidatos testados" in texto
    assert "Fora da amostra (dobras 2–5" in texto
    assert "OTIMISTA" in texto and "--testados 24" in texto
    assert "<perfis>/lotofacil/busca_v1.toml" in texto
    a = (tmp_path / "a" / "lotofacil" / "busca_v1.toml").read_bytes()
    assert a == (tmp_path / "b" / "lotofacil" / "busca_v1.toml").read_bytes()
    carregar_perfil(tmp_path / "a" / "lotofacil" / "busca_v1.toml")


def test_002_recusa_manifesto_do_trancado(tmp_path, capsys):
    exp = _experimento()
    m = escrever_dataset(tmp_path / "ds", nome_manifesto="manifesto_trancado_csv.json")
    assert exp.main([str(m), "--orcamento", "1", "--perfis", str(tmp_path / "p")],
                    cortes=CORTES_TESTE) == 2
    assert "TRANCADO" in capsys.readouterr().err
    assert not (tmp_path / "p").exists()


def _manifesto_adulterado(pasta):
    """Cópia do dataset da fixture com o SHA-256 do manifesto trocado."""
    import json
    import shutil

    shutil.copytree(FIXTURES / "lotofacil", pasta)
    m = pasta / "manifesto_parquet.json"
    dados = json.loads(m.read_text(encoding="utf-8"))
    dados["sha256"] = "0" * 64
    m.write_text(json.dumps(dados, ensure_ascii=False, indent=2), encoding="utf-8")
    return m


def test_002_manifesto_invalido_depois_de_um_valido_nao_grava_nada(tmp_path, capsys):
    exp = _experimento()
    valido = str(FIXTURES / "lotofacil" / "manifesto_parquet.json")
    ruim = str(_manifesto_adulterado(tmp_path / "ds"))
    perfis = tmp_path / "p"
    args = [valido, ruim, "--orcamento", "1", "--processos", "1", "--perfis", str(perfis)]
    assert exp.main(args) == 2
    assert "Erro:" in capsys.readouterr().err
    assert not perfis.exists()


def test_002_loteria_repetida_nao_grava_nada(tmp_path, capsys):
    exp = _experimento()
    a = str(FIXTURES / "lotofacil" / "manifesto_parquet.json")
    b = str(FIXTURES / "lotofacil" / "manifesto_csv.json")
    perfis = tmp_path / "p"
    assert exp.main([a, b, "--orcamento", "1", "--processos", "1", "--perfis", str(perfis)]) == 2
    assert "repetida" in capsys.readouterr().err
    assert not perfis.exists()


@pytest.mark.parametrize("campo", ["semente", "orcamento"])
def test_gravar_perfil_recusa_sem_semente_ou_orcamento(tmp_path, resultado, campo):
    incompleto = dataclasses.replace(resultado, **{campo: None})
    with pytest.raises(ErroBusca, match=campo):
        gravar_perfil(incompleto, tmp_path, "teste")
    assert not (tmp_path / "lotofacil").exists()
