//! Testes de `orquestracao::evidencia_do_banco` e do comando
//! `sync evidencia` (1d.2): evidência bayesiana por dezena sobre
//! `resultados`, só leitura, com a recusa do `validar`.

use rusqlite::types::Value;
use rust_spider::avaliacao::{self, Grau, Janela, DESVIOS_PRIORI, QTD_PRIORIS};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{
    avaliar_perfil, evidencia_completa, evidencia_do_banco, evidencia_do_indice_no_banco,
    sincronizar_features, sincronizar_indice, IndiceEvidenciado, IndiceIndisponivel,
    OpcoesAvaliacao, SecaoIndice, TrechoAvaliado,
};
use rust_spider::perfil::{self, PerfilCarregado};
use rust_spider::Resultado;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const SEMENTE: u64 = 0x9E37_79B9_7F4A_7C15;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

/// Histórico sintético determinístico (xorshift64), concursos 1..=`n`.
fn historico(cfg: &LoteriaConfig, n: u32) -> Vec<Resultado> {
    let mut x = SEMENTE;
    let faixa: Vec<u32> = cfg.dezenas().collect();
    (1..=n)
        .map(|c| {
            let mut pool = faixa.clone();
            let mut dezenas = Vec::with_capacity(cfg.qtd_dezenas);
            for _ in 0..cfg.qtd_dezenas {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                dezenas.push(pool.swap_remove((x % pool.len() as u64) as usize));
            }
            Resultado {
                loteria: cfg.slug.to_string(),
                concurso: c,
                data_sorteio: format!(
                    "{}-{:02}-{:02}",
                    1990 + c / 336,
                    1 + (c % 336) / 28,
                    1 + c % 28
                ),
                dezenas,
            }
        })
        .collect()
}

fn banco(resultados: &[Resultado]) -> rusqlite::Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    db::salvar_resultados(&mut conn, resultados).expect("resultados");
    conn
}

#[test]
fn relatorio_cobre_as_duas_janelas_para_o_proximo_concurso() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let c = cfg(slug);
        let n = 700;
        let conn = banco(&historico(c, n));
        let r = evidencia_do_banco(&conn, c).expect("relatório");

        assert_eq!(r.loteria, slug);
        assert_eq!(r.ultimo_concurso, n);
        assert_eq!(r.qtd_dezenas, c.qtd_dezenas);
        assert_eq!(r.dezenas_possiveis, c.qtd_dezenas_possiveis());
        assert!(r.avisos.is_empty());
        assert_eq!(r.janelas.len(), 2);

        let (inteira, recente) = (&r.janelas[0], &r.janelas[1]);
        assert_eq!(inteira.evidencia.janela, Janela::Inteira);
        assert_eq!(
            recente.evidencia.janela,
            Janela::Ultimos(avaliacao::JANELA_SECUNDARIA)
        );
        // Snapshot do próximo concurso: só os já sorteados.
        for j in &r.janelas {
            assert_eq!(j.evidencia.t, n + 1);
            assert_eq!(j.evidencia.ultimo_concurso, n);
            assert_eq!(
                j.evidencia.dezenas.len(),
                c.qtd_dezenas_possiveis() as usize
            );
            // O esperado é simulado com o mesmo n da janela, nas 3 prioris.
            assert_eq!(j.esperado.concursos, j.evidencia.concursos);
            assert_eq!(j.esperado.replicas, avaliacao::REPLICAS_ESPERADO);
            assert_eq!(j.esperado.semente, avaliacao::SEMENTE_ESPERADO);
            assert_eq!(j.esperado.por_priori.len(), QTD_PRIORIS);
            for (esp, s) in j.esperado.por_priori.iter().zip(DESVIOS_PRIORI) {
                assert_eq!(esp.desvio, s);
            }
        }
        assert_eq!(inteira.evidencia.concursos, n as usize);
        assert_eq!(inteira.evidencia.primeiro_concurso, 1);
        assert_eq!(recente.evidencia.concursos, 500);
        assert_eq!(recente.evidencia.primeiro_concurso, n - 499);
    }
}

#[test]
fn historico_curto_faz_as_duas_janelas_coincidirem() {
    let c = cfg("lotofacil");
    let conn = banco(&historico(c, 40));
    let r = evidencia_do_banco(&conn, c).expect("relatório");
    assert_eq!(r.janelas[1].evidencia.concursos, 40);
    assert_eq!(
        r.janelas[0].evidencia.dezenas,
        r.janelas[1].evidencia.dezenas
    );
}

#[test]
fn relatorio_e_deterministico() {
    let c = cfg("megasena");
    let conn = banco(&historico(c, 300));
    let a = evidencia_do_banco(&conn, c).expect("relatório");
    let b = evidencia_do_banco(&conn, c).expect("relatório");
    assert_eq!(a, b);
}

#[test]
fn banco_vazio_e_sem_resultados() {
    let c = cfg("quina");
    let conn = banco(&[]);
    assert!(matches!(
        evidencia_do_banco(&conn, c),
        Err(SpiderError::SemResultados(_))
    ));
}

#[test]
fn buraco_e_recusado_pelo_validar() {
    let c = cfg("lotofacil");
    let mut hist = historico(c, 50);
    hist.remove(24); // sem o concurso 25
    let conn = banco(&hist);
    match evidencia_do_banco(&conn, c) {
        Err(SpiderError::ResultadosReprovados { loteria, erros, .. }) => {
            assert_eq!(loteria, "lotofacil");
            assert!(!erros.is_empty());
        }
        outro => panic!("esperado ResultadosReprovados, veio {outro:?}"),
    }
}

#[test]
fn dezena_fora_da_faixa_e_recusada_pelo_validar() {
    let c = cfg("lotofacil");
    let mut hist = historico(c, 20);
    hist[5].dezenas[0] = 99;
    let conn = banco(&hist);
    assert!(matches!(
        evidencia_do_banco(&conn, c),
        Err(SpiderError::ResultadosReprovados { .. })
    ));
}

#[test]
fn calcular_nao_grava_nem_cria_tabela() {
    let c = cfg("lotofacil");
    let conn = banco(&historico(c, 120));
    let tabelas = |conn: &rusqlite::Connection| -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master ORDER BY name")
            .expect("consulta");
        stmt.query_map([], |row| row.get(0))
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("nomes")
    };
    let antes = tabelas(&conn);
    let total_antes = db::listar_crescente(&conn, c.slug).expect("lista").len();
    evidencia_do_banco(&conn, c).expect("relatório");
    assert_eq!(tabelas(&conn), antes);
    assert_eq!(
        db::listar_crescente(&conn, c.slug).expect("lista").len(),
        total_antes
    );
    for nome in ["features_dezena", "motor_estado", "indice", "perfis"] {
        assert!(!db::tabela_existe(&conn, nome).expect("consulta"), "{nome}");
    }
}

#[test]
fn dezena_viciada_aparece_como_muito_forte_a_favor_de_vies() {
    // A dezena 7 sai em todos os 300 concursos; é a única que o relatório
    // deve apontar como "muito forte".
    let c = cfg("lotofacil");
    let mut hist = historico(c, 300);
    for r in &mut hist {
        if !r.dezenas.contains(&7) {
            r.dezenas[0] = 7;
            r.dezenas.sort_unstable();
            r.dezenas.dedup();
        }
        assert_eq!(r.dezenas.len(), 15, "o ajuste não deve repetir dezena");
    }
    let conn = banco(&hist);
    let r = evidencia_do_banco(&conn, c).expect("relatório");
    let d7 = &r.janelas[0].evidencia.dezenas[6];
    assert_eq!(d7.saidas, 300);
    assert_eq!(d7.classificacao[0].grau, Grau::MuitoForte);
    assert!(d7.log10_fb[0] > 2.0);
    // Confere o que o comentário afirma: a 7 é a única dezena "muito forte" (em
    // qualquer direção) na priori principal.
    let muito_fortes: Vec<u32> = r.janelas[0]
        .evidencia
        .dezenas
        .iter()
        .filter(|d| d.classificacao[0].grau == Grau::MuitoForte)
        .map(|d| d.dezena)
        .collect();
    assert_eq!(muito_fortes, [7], "só a dezena viciada é muito forte");
}

// ---------------------------------------------------------------------------
// CLI.
// ---------------------------------------------------------------------------

struct Pasta(PathBuf);

impl Pasta {
    fn nova(rotulo: &str) -> Self {
        static CONTADOR: AtomicU32 = AtomicU32::new(0);
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("relógio")
            .as_nanos();
        let n = CONTADOR.fetch_add(1, Ordering::Relaxed);
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_evidencia_{rotulo}_{}_{unico}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&caminho).expect("pasta temporária");
        Pasta(caminho)
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sync(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn banco_em_arquivo(pasta: &Pasta, resultados: &[Resultado]) -> PathBuf {
    let caminho = pasta.0.join("resultados_lotofacil.db");
    let mut conn = db::abrir(&caminho).expect("banco em arquivo");
    db::salvar_resultados(&mut conn, resultados).expect("resultados");
    caminho
}

#[test]
fn cli_imprime_o_relatorio_sem_tocar_no_banco() {
    let pasta = Pasta::nova("ok");
    let c = cfg("lotofacil");
    let caminho = banco_em_arquivo(&pasta, &historico(c, 80));
    let antes = std::fs::read(&caminho).expect("bytes do banco");

    let saida = sync(
        &pasta.0,
        &["evidencia", "lotofacil", "--db", caminho.to_str().unwrap()],
    );
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    let texto = String::from_utf8(saida.stdout.clone()).expect("UTF-8");
    for esperado in [
        "Evidência bayesiana por dezena de Lotofácil",
        "para o concurso 81",
        "K/N = 15/25 = 0.600000",
        "5.0 %",
        "2.5 %",
        "10.0 %",
        "AVISO: com 25 dezenas",
        "Janela 1: histórico inteiro (principal)",
        "Janela 2: últimos 500 concursos (secundária)",
        "Priori 5.0 % (principal):",
        "Priori 2.5 %:",
        "Priori 10.0 %:",
        "a favor do acaso",
        "esperadas sob o acaso",
    ] {
        assert!(texto.contains(esperado), "faltou `{esperado}` em:\n{texto}");
    }
    // Uma linha por dezena em cada janela (principal e secundária, n = 80 nas
    // duas): dezena com dois dígitos, x e n como os três primeiros campos. Não
    // depende do alinhamento das colunas.
    let linhas_de_dezena = texto
        .lines()
        .filter(|l| {
            let campos: Vec<&str> = l.split_whitespace().collect();
            campos.len() >= 6
                && campos[0].len() == 2
                && campos[0].bytes().all(|b| b.is_ascii_digit())
                && campos[2] == "80"
        })
        .count();
    assert_eq!(linhas_de_dezena, 50, "{texto}");

    // Só leitura: o banco continua byte a byte igual.
    assert_eq!(std::fs::read(&caminho).expect("bytes do banco"), antes);

    // Mesma entrada, mesmos bytes (o tempo vai para o stderr).
    let de_novo = sync(
        &pasta.0,
        &["evidencia", "lotofacil", "--db", caminho.to_str().unwrap()],
    );
    assert_eq!(de_novo.stdout, saida.stdout);
}

#[test]
fn cli_recusa_todas_e_banco_inexistente_e_buraco() {
    let pasta = Pasta::nova("recusas");

    let todas = sync(&pasta.0, &["evidencia", "todas"]);
    assert!(!todas.status.success());
    let erro = String::from_utf8_lossy(&todas.stderr);
    assert!(
        erro.contains("evidencia") && erro.contains("não aceita"),
        "{erro}"
    );
    assert!(todas.stdout.is_empty());

    let sem_banco = pasta.0.join("nao_existe.db");
    let saida = sync(
        &pasta.0,
        &["evidencia", "quina", "--db", sem_banco.to_str().unwrap()],
    );
    assert!(!saida.status.success());
    assert!(String::from_utf8_lossy(&saida.stderr).contains("banco não encontrado"));
    assert!(!sem_banco.exists(), "o comando não deve criar o banco");

    let c = cfg("lotofacil");
    let mut hist = historico(c, 30);
    hist.remove(14);
    let caminho = banco_em_arquivo(&pasta, &hist);
    let saida = sync(
        &pasta.0,
        &["evidencia", "lotofacil", "--db", caminho.to_str().unwrap()],
    );
    assert!(!saida.status.success());
    assert!(!String::from_utf8_lossy(&saida.stdout).contains("Evidência bayesiana"));
    let erro = String::from_utf8_lossy(&saida.stderr);
    assert!(
        erro.contains("erro(s) de integridade") || erro.contains("faltam"),
        "{erro}"
    );
}

// ---------------------------------------------------------------------------
// Evidência do índice (B, 1d.3): orquestração.
// ---------------------------------------------------------------------------

const CRIADO_EM: &str = "2026-10-10T10:00:00Z";
const CORTE_BAIXO: u32 = 200;
const NOME: &str = "teste_v1";

/// Lotofácil com o corte da busca trocado (a orquestração recebe o config).
fn cfg_corte(corte: u32) -> LoteriaConfig {
    LoteriaConfig {
        ultimo_concurso_busca: corte,
        ..*cfg("lotofacil")
    }
}

fn texto_perfil(nome: &str, frequencia: f64) -> String {
    let versao: u32 = nome
        .rsplit("_v")
        .next()
        .and_then(|v| v.parse().ok())
        .expect("nome termina em _v<N>");
    format!(
        r#"nome = "{nome}"
versao = {versao}
loteria = "lotofacil"
janela = 10
normalizacao = "minmax"

[pesos]
frequencia = {frequencia:?}
maior_espaco = 1.0
maior_espaco_ocorrencias = 0.3
menor_espaco = 0.5
menor_espaco_ocorrencias = 0.3
soma_variacoes = -0.8
espaco_final = 1.2
repeticoes_total = 0.7
repeticao_atual = 0.4
"#
    )
}

/// Grava `<raiz>/lotofacil/<nome>.toml` e devolve o perfil carregado.
fn gravar_perfil(raiz: &Path, nome: &str, frequencia: f64) -> PerfilCarregado {
    let pasta = raiz.join("lotofacil");
    std::fs::create_dir_all(&pasta).expect("pasta do perfil");
    std::fs::write(
        pasta.join(format!("{nome}.toml")),
        texto_perfil(nome, frequencia),
    )
    .expect("perfil gravado");
    perfil::carregar(raiz, cfg("lotofacil"), nome).expect("perfil carregado")
}

/// Banco em memória com `hist`, as features e o índice do perfil.
fn indexado(c: &LoteriaConfig, hist: &[Resultado], p: &PerfilCarregado) -> rusqlite::Connection {
    let mut conn = banco(hist);
    sincronizar_features(&mut conn, c, false).expect("features");
    sincronizar_indice(&mut conn, c, p, CRIADO_EM).expect("índice");
    conn
}

/// Foto do banco: `sqlite_master` e todas as linhas de cada tabela.
fn foto(conn: &rusqlite::Connection) -> Vec<(String, Vec<Vec<Value>>)> {
    let linhas = |sql: &str| -> Vec<Vec<Value>> {
        let mut stmt = conn.prepare(sql).expect("consulta");
        let colunas = stmt.column_count();
        stmt.query_map([], |row| {
            (0..colunas)
                .map(|i| row.get::<_, Value>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .expect("linhas")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("valores")
    };
    let mut foto = vec![(
        "sqlite_master".to_string(),
        linhas("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name"),
    )];
    let nomes = linhas(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    );
    for nome in nomes {
        let Value::Text(nome) = &nome[0] else {
            panic!("nome de tabela inesperado");
        };
        foto.push((nome.clone(), linhas(&format!("SELECT * FROM \"{nome}\""))));
    }
    foto
}

fn calculada(secao: SecaoIndice) -> Box<IndiceEvidenciado> {
    match secao {
        SecaoIndice::Calculada(c) => c,
        SecaoIndice::Indisponivel(i) => panic!("esperava B calculada, veio: {i:?}"),
    }
}

fn indisponivel(secao: SecaoIndice) -> IndiceIndisponivel {
    match secao {
        SecaoIndice::Indisponivel(i) => i,
        SecaoIndice::Calculada(_) => panic!("esperava B indisponível"),
    }
}

#[test]
fn b_confere_com_a_diferenca_de_log_loss_do_avaliar() {
    // Conferência exigida pelo plano, no caminho de produção (banco, índice
    // gravado, calibração): ln FB de B = −pares × (log loss do perfil − log
    // loss do acaso) nas dobras 2 a 5 do walk-forward que o `sync avaliar`
    // produz. Mesmo número, outra escala.
    let c = cfg_corte(CORTE_BAIXO);
    let pasta = Pasta::nova("b_confere");
    let p = gravar_perfil(&pasta.0, NOME, 1.0);
    let mut conn = indexado(&c, &historico(&c, 260), &p);

    let b = calculada(evidencia_do_indice_no_banco(&mut conn, &c, &pasta.0, NOME).expect("B"));
    let registros = pasta.0.join("registros");
    let avaliacao = avaliar_perfil(
        &mut conn,
        &c,
        &p,
        &OpcoesAvaliacao::default(),
        &registros,
        "2026-10-10T12:00:00Z",
    )
    .expect("avaliação");
    let TrechoAvaliado::Busca(wf) = &avaliacao.trecho else {
        panic!("esperava a busca");
    };

    assert_eq!(b.perfil, NOME);
    assert_eq!(b.hash_toml, avaliacao.hash_toml);
    assert_eq!(b.corte, CORTE_BAIXO);
    assert_eq!(b.primeiro_indice, avaliacao.primeiro_indice);
    assert_eq!(b.ultimo_concurso, avaliacao.ultimo_concurso);
    assert_eq!(b.evidencia.por_dobra.len(), 4);
    let mut total = 0.0;
    let mut escala = 0.0;
    for d in &b.evidencia.por_dobra {
        let a = &wf.dobras[d.dobra as usize - 1];
        assert_eq!(a.faixa, d.faixa);
        let ll = &a.metricas.log_loss;
        let esperado = -(d.pares as f64) * (ll.perfil - ll.acaso);
        // 1e-13 × pares × log loss do acaso: o lado do walk-forward subtrai
        // duas médias de ~0,67 (cancelamento de ~1e-15 por par).
        let tolerancia = 1e-13 * d.pares as f64 * ll.acaso;
        assert!(
            (d.ln_fb - esperado).abs() <= tolerancia,
            "dobra {}: {} contra {esperado}",
            d.dobra,
            d.ln_fb
        );
        total += esperado;
        escala += d.pares as f64 * ll.acaso;
    }
    assert!((b.evidencia.ln_fb - total).abs() <= 1e-13 * escala);
    // O trecho de B é o das dobras 2..=5 e termina no corte.
    assert_eq!(b.evidencia.faixa.primeiro, wf.dobras[1].faixa.primeiro);
    assert_eq!(b.evidencia.faixa.ultimo, wf.dobras[4].faixa.ultimo);
    assert_eq!(b.evidencia.faixa.ultimo, CORTE_BAIXO);
    // O inerte vem junto, com as constantes do plano.
    assert_eq!(b.inerte.replicas, avaliacao::REPLICAS_INERTE);
    assert_eq!(b.inerte.semente, avaliacao::SEMENTE_INERTE);
    assert!(b.inerte.p025_log10 <= b.inerte.media_log10);
    assert!(b.inerte.media_log10 <= b.inerte.p975_log10);
    assert!(b.inerte.replicas_iguais_ou_acima <= b.inerte.replicas);
}

#[test]
fn b_nao_toca_o_trancado() {
    // Dois bancos iguais até o corte e diferentes depois dele: B é idêntico
    // bit a bit (features e índice de t usam só concursos < t, e B descarta
    // tudo acima do corte).
    let c = cfg_corte(CORTE_BAIXO);
    let pasta = Pasta::nova("b_trancado");
    let p = gravar_perfil(&pasta.0, NOME, 1.0);
    let base = historico(&c, 260);
    let alterado: Vec<Resultado> = base
        .iter()
        .map(|r| {
            if r.concurso > CORTE_BAIXO {
                let mut novo = r.clone();
                novo.dezenas = r.dezenas.iter().map(|&d| d % c.dezena_max + 1).collect();
                novo
            } else {
                r.clone()
            }
        })
        .collect();
    assert_ne!(base, alterado);
    let mut a = indexado(&c, &base, &p);
    let mut b = indexado(&c, &alterado, &p);
    let ba = calculada(evidencia_do_indice_no_banco(&mut a, &c, &pasta.0, NOME).expect("B"));
    let bb = calculada(evidencia_do_indice_no_banco(&mut b, &c, &pasta.0, NOME).expect("B"));
    assert_eq!(ba.evidencia, bb.evidencia);
    assert_eq!(ba.inerte, bb.inerte);
    assert_eq!(ba.evidencia.faixa.ultimo, CORTE_BAIXO);
}

#[test]
fn b_nao_grava_e_e_deterministica() {
    let c = cfg_corte(CORTE_BAIXO);
    let pasta = Pasta::nova("b_leitura");
    let p = gravar_perfil(&pasta.0, NOME, 1.0);
    let mut conn = indexado(&c, &historico(&c, 260), &p);
    let antes = foto(&conn);
    let a = evidencia_completa(&mut conn, &c, &pasta.0, NOME).expect("relatório");
    let b = evidencia_completa(&mut conn, &c, &pasta.0, NOME).expect("relatório");
    assert_eq!(foto(&conn), antes);
    assert_eq!(a, b);
    // A sai igual à de `evidencia_do_banco`.
    assert_eq!(a.dezenas, evidencia_do_banco(&conn, &c).expect("A"));
    assert!(matches!(a.indice, SecaoIndice::Calculada(_)));
}

#[test]
fn teto_do_relatorio_vem_de_cada_janela_e_priori() {
    let c = cfg("quina");
    let conn = banco(&historico(c, 700));
    let r = evidencia_do_banco(&conn, c).expect("A");
    let p = c.qtd_dezenas as f64 / f64::from(c.qtd_dezenas_possiveis());
    for j in &r.janelas {
        for (k, s) in DESVIOS_PRIORI.into_iter().enumerate() {
            let esperado =
                avaliacao::teto_a_favor_do_acaso(j.evidencia.concursos as u64, p, s).unwrap();
            assert_eq!(j.teto_acaso[k], esperado);
            assert!(esperado > 0.0);
        }
        // Priori mais larga → teto maior (2,5 % < 5 % < 10 %).
        assert!(j.teto_acaso[1] < j.teto_acaso[0] && j.teto_acaso[0] < j.teto_acaso[2]);
    }
    // 700 concursos contra 500: o teto cresce com n.
    assert!(r.janelas[0].teto_acaso[0] > r.janelas[1].teto_acaso[0]);
}

#[test]
fn b_indisponivel_diz_o_motivo_e_o_comando() {
    let c = cfg_corte(CORTE_BAIXO);
    let pasta = Pasta::nova("b_indisponivel");
    let p = gravar_perfil(&pasta.0, NOME, 1.0);

    // Perfil sem arquivo.
    let mut conn = indexado(&c, &historico(&c, 260), &p);
    let i =
        indisponivel(evidencia_do_indice_no_banco(&mut conn, &c, &pasta.0, "outro_v1").unwrap());
    assert_eq!(i.perfil, "outro_v1");
    assert!(i.motivo.contains("outro_v1"), "{}", i.motivo);
    assert!(i.comandos.is_empty(), "{:?}", i.comandos);

    // Features sem índice: perfil não registrado (o comando garante as features).
    let mut so_features = banco(&historico(&c, 260));
    sincronizar_features(&mut so_features, &c, false).expect("features");
    let i =
        indisponivel(evidencia_do_indice_no_banco(&mut so_features, &c, &pasta.0, NOME).unwrap());
    assert!(i.motivo.contains("não está registrado"), "{}", i.motivo);
    // Um comando por item (sem `&&`, que o PowerShell 5.1 não tem), e o
    // `sync indice` leva a pasta de perfis, que aqui não é a padrão.
    let pasta_perfis = pasta.0.display().to_string();
    let pasta_perfis = if pasta_perfis.contains(char::is_whitespace) {
        format!("\"{pasta_perfis}\"")
    } else {
        pasta_perfis
    };
    let features_e_indice = vec![
        "sync features lotofacil".to_string(),
        format!("sync indice lotofacil --perfil teste_v1 --perfis {pasta_perfis}"),
    ];
    assert_eq!(i.comandos, features_e_indice);

    // Sem features: pede features e índice.
    let mut vazio = banco(&historico(&c, 260));
    let i = indisponivel(evidencia_do_indice_no_banco(&mut vazio, &c, &pasta.0, NOME).unwrap());
    assert_eq!(i.comandos, features_e_indice);

    // Concurso novo depois do índice: features desatualizadas.
    let mut defasado = indexado(&c, &historico(&c, 260), &p);
    db::salvar_resultados(&mut defasado, &historico(&c, 261)[260..]).expect("concurso novo");
    let i = indisponivel(evidencia_do_indice_no_banco(&mut defasado, &c, &pasta.0, NOME).unwrap());
    assert_eq!(i.comandos, features_e_indice, "{i:?}");

    // Perfil editado depois de registrado: o erro já diz o que fazer.
    let mut editado = indexado(&c, &historico(&c, 260), &p);
    gravar_perfil(&pasta.0, NOME, 2.0);
    let i = indisponivel(evidencia_do_indice_no_banco(&mut editado, &c, &pasta.0, NOME).unwrap());
    assert!(i.motivo.contains("imutáveis"), "{}", i.motivo);
    assert!(i.comandos.is_empty(), "{:?}", i.comandos);
    gravar_perfil(&pasta.0, NOME, 1.0);
}

#[test]
fn b_indisponivel_com_historico_curto_demais_para_as_dobras() {
    let c = cfg_corte(CORTE_BAIXO);
    let pasta = Pasta::nova("b_curto");
    let p = gravar_perfil(&pasta.0, NOME, 1.0);
    // 5 concursos sorteados: 4 avaliáveis, menos que as 5 dobras.
    let mut conn = indexado(&c, &historico(&c, 5), &p);
    let i = indisponivel(evidencia_do_indice_no_banco(&mut conn, &c, &pasta.0, NOME).unwrap());
    assert!(i.motivo.contains("dobras"), "{}", i.motivo);
    // A continua saindo (n = 5 concursos).
    let completo = evidencia_completa(&mut conn, &c, &pasta.0, NOME).expect("relatório");
    assert_eq!(completo.dezenas.janelas[0].evidencia.concursos, 5);
    assert!(matches!(completo.indice, SecaoIndice::Indisponivel(_)));
}

#[test]
fn erro_de_integridade_dos_resultados_continua_sendo_erro_em_vez_de_b_indisponivel() {
    let c = cfg_corte(CORTE_BAIXO);
    let pasta = Pasta::nova("b_buraco");
    gravar_perfil(&pasta.0, NOME, 1.0);
    let mut hist = historico(&c, 50);
    hist.remove(24);
    let mut conn = banco(&hist);
    assert!(matches!(
        evidencia_completa(&mut conn, &c, &pasta.0, NOME),
        Err(SpiderError::ResultadosReprovados { .. })
    ));
}

// ---------------------------------------------------------------------------
// Evidência do índice (B, 1d.3): CLI.
// ---------------------------------------------------------------------------

/// Banco em arquivo (config real, corte 3416) com features e índice do perfil
/// `padrao_v1` gravado em `<pasta>/perfis`.
fn banco_com_indice(pasta: &Pasta, n: u32) -> PathBuf {
    let c = cfg("lotofacil");
    let raiz = pasta.0.join("perfis");
    let p = gravar_perfil(&raiz, "padrao_v1", 1.0);
    let caminho = pasta.0.join("resultados_lotofacil.db");
    let mut conn = db::abrir(&caminho).expect("banco em arquivo");
    db::salvar_resultados(&mut conn, &historico(c, n)).expect("resultados");
    sincronizar_features(&mut conn, c, false).expect("features");
    sincronizar_indice(&mut conn, c, &p, CRIADO_EM).expect("índice");
    caminho
}

fn args_evidencia(pasta: &Pasta, caminho: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "evidencia".to_string(),
        "lotofacil".to_string(),
        "--db".to_string(),
        caminho.to_str().unwrap().to_string(),
        "--perfis".to_string(),
        pasta.0.join("perfis").to_str().unwrap().to_string(),
    ];
    args.extend(extra.iter().map(|s| (*s).to_string()));
    args
}

fn sync_args(dir: &Path, args: &[String]) -> std::process::Output {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    sync(dir, &refs)
}

#[test]
fn cli_imprime_a_secao_b_do_perfil_ativo_sem_tocar_no_banco() {
    let pasta = Pasta::nova("cli_b");
    let caminho = banco_com_indice(&pasta, 120);
    let antes = std::fs::read(&caminho).expect("bytes do banco");

    let saida = sync_args(&pasta.0, &args_evidencia(&pasta, &caminho, &[]));
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    let texto = String::from_utf8(saida.stdout.clone()).expect("UTF-8");
    for esperado in [
        "Evidência bayesiana por dezena de Lotofácil",
        "ASSIMETRIA: a favor do acaso a evidência tem teto",
        "Máximo atingível a favor do acaso com n = 120: |log10 FB| ≤",
        "== Evidência do índice de força (B) ==",
        "Perfil: padrao_v1 (versão 1, janela 10, minmax; perfil ativo da loteria).",
        "Hash do perfil:",
        "dobras de teste 2 a 5",
        "log10 FB (índice contra o acaso, prequencial):",
        "FB > 1: a favor do índice",
        "Por dobra (só informativo",
        "Δ log loss/par",
        "–121 (até o último sorteado + 1)",
        "Esperado de um índice inerte (as mesmas posições diante de 200 históricos",
        "Observado contra o inerte:",
        "ATENÇÃO: a calibração estima uma taxa por posição",
        "pseudo-verossimilhança",
    ] {
        assert!(texto.contains(esperado), "faltou `{esperado}` em:\n{texto}");
    }
    // Quatro dobras (2 a 5) na tabela por dobra.
    let dobras: Vec<&str> = texto
        .lines()
        .skip_while(|l| !l.contains("Por dobra"))
        .skip(2)
        .take_while(|l| !l.trim().is_empty())
        .filter(|l| !l.trim_start().starts_with("Δ log loss/par:"))
        .collect();
    assert_eq!(dobras.len(), 4, "{texto}");
    for (i, l) in dobras.iter().enumerate() {
        assert_eq!(
            l.split_whitespace().next(),
            Some((i + 2).to_string().as_str()),
            "{l}"
        );
    }

    // Só leitura e determinístico.
    assert_eq!(std::fs::read(&caminho).expect("bytes do banco"), antes);
    let de_novo = sync_args(&pasta.0, &args_evidencia(&pasta, &caminho, &[]));
    assert_eq!(de_novo.stdout, saida.stdout);

    // Com `--perfil` explícito (o mesmo), sai igual, só mudando a origem.
    let explicito = sync_args(
        &pasta.0,
        &args_evidencia(&pasta, &caminho, &["--perfil", "padrao_v1"]),
    );
    assert!(explicito.status.success());
    let texto_explicito = String::from_utf8(explicito.stdout).expect("UTF-8");
    assert_eq!(
        texto_explicito,
        texto.replace("perfil ativo da loteria", "informado por --perfil")
    );
}

#[test]
fn cli_sem_b_calculavel_imprime_a_motivo_e_comando() {
    let pasta = Pasta::nova("cli_sem_b");
    let c = cfg("lotofacil");
    // Só resultados: sem features, sem índice e sem pasta de perfis.
    let caminho = banco_em_arquivo(&pasta, &historico(c, 80));

    // Perfil padrão: A sai, B explica, exit 0.
    let saida = sync_args(&pasta.0, &args_evidencia(&pasta, &caminho, &[]));
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    let texto = String::from_utf8(saida.stdout).expect("UTF-8");
    assert!(texto.contains("Evidência bayesiana por dezena de Lotofácil"));
    assert!(texto.contains("== Evidência do índice de força (B) =="));
    assert!(texto.contains("Perfil: padrao_v1 (perfil ativo da loteria)."));
    assert!(texto.contains("B não pôde ser calculada:"), "{texto}");

    // Com o arquivo do perfil mas sem features/índice: pede os comandos.
    gravar_perfil(&pasta.0.join("perfis"), "padrao_v1", 1.0);
    let saida = sync_args(&pasta.0, &args_evidencia(&pasta, &caminho, &[]));
    assert!(saida.status.success());
    let texto = String::from_utf8(saida.stdout).expect("UTF-8");
    assert!(
        texto.contains(
            "Comandos a rodar, nesta ordem:\n  sync features lotofacil\n  sync indice lotofacil \
             --perfil padrao_v1 --perfis "
        ),
        "{texto}"
    );
    assert!(!texto.contains("&&"), "{texto}");

    // `--perfil` explícito: o relatório (A inclusive) sai igual, mas o exit é erro.
    let explicito = sync_args(
        &pasta.0,
        &args_evidencia(&pasta, &caminho, &["--perfil", "padrao_v1"]),
    );
    assert!(!explicito.status.success());
    let texto_explicito = String::from_utf8(explicito.stdout).expect("UTF-8");
    assert!(texto_explicito.contains("Evidência bayesiana por dezena de Lotofácil"));
    assert!(texto_explicito.contains("informado por --perfil"));
    assert!(texto_explicito.contains("B não pôde ser calculada:"));
    let erro = String::from_utf8_lossy(&explicito.stderr);
    assert!(
        erro.contains("não pôde ser calculada com o perfil `padrao_v1`"),
        "{erro}"
    );

    // Perfil que nem existe, explícito: também erro, com o motivo no stderr.
    let inexistente = sync_args(
        &pasta.0,
        &args_evidencia(&pasta, &caminho, &["--perfil", "nao_existe_v9"]),
    );
    assert!(!inexistente.status.success());
    assert!(String::from_utf8_lossy(&inexistente.stderr).contains("nao_existe_v9"));
}
