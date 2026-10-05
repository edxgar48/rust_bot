//! Testes de `orquestracao::avaliar_perfil` (comando `sync avaliar`, 1c.5) e
//! de `orquestracao::testar_aleatoriedade_do_banco` (`sync aleatoriedade`).
//!
//! Duas camadas:
//! - orquestração sobre bancos em memória (busca, trancado, Bonferroni, `k`,
//!   recusas, ponto no tempo); o cálculo das métricas em si já tem testes
//!   unitários em `src/avaliacao/`, aqui se confere a montagem: trecho,
//!   dobras, registro do trancado e o que nunca pode ser gravado;
//! - o binário `sync` (`CARGO_BIN_EXE_sync`) com banco, perfis e registros em
//!   pastas temporárias únicas, apagadas ao fim.
//!
//! O histórico é sintético (xorshift), então o esperado de qualquer perfil é
//! empatar com o acaso. Para o trancado sem históricos enormes, usa-se um
//! config com corte baixo (`ultimo_concurso_busca: 200`); a CLI usa o config
//! real (corte 3416).

use rusqlite::types::Value;
use rusqlite::Connection;
use rust_spider::avaliacao::{self, ALFA, BLOCO_BOOTSTRAP, SEMENTE_BOOTSTRAP};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{
    avaliar_perfil, avaliar_perfil_e_formatar, sincronizar_features, sincronizar_indice,
    testar_aleatoriedade_do_banco, OpcoesAvaliacao, RelatorioAvaliacao, TrechoAvaliado,
    UsoDoTrancado, TENTATIVAS_REGISTRO,
};
use rust_spider::perfil::{self, Normalizacao, PerfilCarregado};
use rust_spider::registros::{self, AvaliacaoTrancado};
use rust_spider::Resultado;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const CRIADO_EM: &str = "2026-10-05T10:00:00Z";
const AVALIADO_EM: &str = "2026-10-06T12:00:00Z";
const AVALIADO_DEPOIS: &str = "2026-10-07T08:30:00Z";
const SEMENTE: u64 = 0x9E37_79B9_7F4A_7C15;
const CORTE_BAIXO: u32 = 200;
const NOME: &str = "teste_v1";

// ---------------------------------------------------------------------------
// Auxiliares.
// ---------------------------------------------------------------------------

fn lotofacil() -> &'static LoteriaConfig {
    config::buscar("lotofacil").expect("lotofácil configurada")
}

/// Lotofácil com o corte da busca trocado (a orquestração recebe o config).
fn cfg_corte(corte: u32) -> LoteriaConfig {
    LoteriaConfig {
        ultimo_concurso_busca: corte,
        ..*lotofacil()
    }
}

fn res(cfg: &LoteriaConfig, concurso: u32, dezenas: &[u32]) -> Resultado {
    Resultado {
        loteria: cfg.slug.to_string(),
        concurso,
        data_sorteio: format!("2020-01-{:02}", 1 + concurso % 28),
        dezenas: dezenas.to_vec(),
    }
}

/// Histórico sintético determinístico (xorshift64): o prefixo de `n` concursos
/// é o mesmo para qualquer `n` maior com a mesma semente.
fn historico(cfg: &LoteriaConfig, n: u32, semente: u64) -> Vec<Resultado> {
    let mut x = semente;
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
            res(cfg, c, &dezenas)
        })
        .collect()
}

/// Mesmo concurso com outro conjunto de dezenas: cada dezena vira a seguinte
/// da faixa (a última volta à primeira).
fn com_dezenas_trocadas(cfg: &LoteriaConfig, r: &Resultado) -> Resultado {
    let mut novo = r.clone();
    novo.dezenas = r
        .dezenas
        .iter()
        .map(|&d| {
            if d == cfg.dezena_max {
                cfg.dezena_min
            } else {
                d + 1
            }
        })
        .collect();
    novo
}

fn banco() -> Connection {
    db::abrir(Path::new(":memory:")).expect("banco em memória")
}

fn gravar(conn: &mut Connection, resultados: &[Resultado]) {
    db::salvar_resultados(conn, resultados).expect("resultados gravados");
}

/// Texto de um perfil válido; `frequencia` varia para gerar conteúdos
/// (e hashes) diferentes.
fn texto_perfil(
    nome: &str,
    loteria: &str,
    janela: u32,
    normalizacao: Normalizacao,
    frequencia: f64,
) -> String {
    let versao: u32 = nome
        .rsplit("_v")
        .next()
        .and_then(|v| v.parse().ok())
        .expect("nome termina em _v<N>");
    format!(
        r#"nome = "{nome}"
versao = {versao}
loteria = "{loteria}"
janela = {janela}
normalizacao = "{norm}"

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
"#,
        norm = normalizacao.como_str()
    )
}

fn carregar_texto(texto: &str, slug: &str, nome: &str) -> PerfilCarregado {
    perfil::interpretar(texto, Path::new(&format!("perfis/{slug}/{nome}.toml")))
        .expect("perfil válido")
}

/// Perfil padrão dos testes: Lotofácil, janela 10, minmax, frequencia 1.0.
fn perfil_teste() -> PerfilCarregado {
    perfil_com(NOME, 1.0)
}

fn perfil_com(nome: &str, frequencia: f64) -> PerfilCarregado {
    let texto = texto_perfil(nome, "lotofacil", 10, Normalizacao::Minmax, frequencia);
    carregar_texto(&texto, "lotofacil", nome)
}

fn indexar(conn: &mut Connection, cfg: &LoteriaConfig, p: &PerfilCarregado) {
    sincronizar_indice(conn, cfg, p, CRIADO_EM).expect("índice sincronizado");
}

/// Banco novo com `hist` gravado e as features calculadas (sem índice).
fn com_features(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = banco();
    gravar(&mut conn, hist);
    sincronizar_features(&mut conn, cfg, false).expect("features calculadas");
    conn
}

/// Banco pronto para avaliar: resultados, features e índice do perfil.
fn preparado(cfg: &LoteriaConfig, hist: &[Resultado], p: &PerfilCarregado) -> Connection {
    let mut conn = com_features(cfg, hist);
    indexar(&mut conn, cfg, p);
    conn
}

fn avaliar_em(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    p: &PerfilCarregado,
    opcoes: &OpcoesAvaliacao,
    registros: &Path,
    quando: &str,
) -> Result<RelatorioAvaliacao, SpiderError> {
    avaliar_perfil(conn, cfg, p, opcoes, registros, quando)
}

fn avaliar(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    p: &PerfilCarregado,
    opcoes: &OpcoesAvaliacao,
    registros: &Path,
) -> Result<RelatorioAvaliacao, SpiderError> {
    avaliar_em(conn, cfg, p, opcoes, registros, AVALIADO_EM)
}

fn opcoes_busca() -> OpcoesAvaliacao {
    OpcoesAvaliacao::default()
}

fn opcoes_trancado() -> OpcoesAvaliacao {
    OpcoesAvaliacao {
        trancado: true,
        ..OpcoesAvaliacao::default()
    }
}

/// Busca com as opções padrão; a pasta de registros nunca é usada.
fn busca(conn: &mut Connection, cfg: &LoteriaConfig, p: &PerfilCarregado) -> RelatorioAvaliacao {
    let pasta = Pasta::nova("busca_auxiliar");
    let relatorio =
        avaliar(conn, cfg, p, &opcoes_busca(), &pasta.0.join("registros")).expect("busca avaliada");
    assert_eq!(entradas(&pasta.0), 0, "a busca não grava arquivo");
    relatorio
}

fn walk_forward_de(r: &RelatorioAvaliacao) -> &avaliacao::WalkForward {
    match &r.trecho {
        TrechoAvaliado::Busca(wf) => wf,
        TrechoAvaliado::Trancado(_) => panic!("esperava o trecho da busca"),
    }
}

fn uso_do_trancado(r: &RelatorioAvaliacao) -> &UsoDoTrancado {
    match &r.trecho {
        TrechoAvaliado::Trancado(uso) => uso,
        TrechoAvaliado::Busca(_) => panic!("esperava o trancado"),
    }
}

/// Pasta temporária única, apagada ao sair (mesmo se o teste falhar).
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
            "rust_spider_avaliar_{rotulo}_{}_{unico}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&caminho).expect("pasta temporária");
        Pasta(caminho)
    }

    /// `<pasta>/registros`: a pasta padrão dos registros do trancado, que o
    /// teste deixa para a orquestração criar.
    fn registros(&self) -> PathBuf {
        self.0.join("registros")
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// Quantas entradas a pasta tem (0 = nada foi criado dentro dela).
fn entradas(pasta: &Path) -> usize {
    std::fs::read_dir(pasta).expect("pasta lida").count()
}

fn arquivo_do_registro(registros: &Path) -> PathBuf {
    registros::caminho(registros, "lotofacil")
}

fn registro_lido(registros: &Path) -> Vec<AvaliacaoTrancado> {
    registros::carregar(&arquivo_do_registro(registros), "lotofacil").expect("registro lido")
}

/// Foto do banco: o conteúdo de `sqlite_master` e todas as linhas de cada
/// tabela, na ordem natural (a mesma enquanto nada é gravado).
type Foto = Vec<(String, Vec<Vec<Value>>)>;

fn linhas_de(conn: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut stmt = conn.prepare(sql).expect("consulta preparada");
    let colunas = stmt.column_count();
    let linhas = stmt
        .query_map([], |row| {
            (0..colunas)
                .map(|i| row.get::<_, Value>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .expect("consulta")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("linhas lidas");
    linhas
}

fn foto(conn: &Connection) -> Foto {
    let mut foto = vec![(
        "sqlite_master".to_string(),
        linhas_de(
            conn,
            "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name",
        ),
    )];
    let nomes: Vec<String> = linhas_de(
        conn,
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .into_iter()
    .map(|l| match &l[0] {
        Value::Text(t) => t.clone(),
        outro => panic!("nome de tabela inesperado: {outro:?}"),
    })
    .collect();
    for nome in nomes {
        let linhas = linhas_de(conn, &format!("SELECT * FROM \"{nome}\""));
        foto.push((nome, linhas));
    }
    foto
}

/// Tabelas gravadas por `sync features` e `sync indice` mais `resultados`:
/// a foto cobre todas, então a conferência abaixo só garante que existem.
fn nomes_das_tabelas(foto: &Foto) -> Vec<&str> {
    foto.iter().skip(1).map(|(n, _)| n.as_str()).collect()
}

/// Avalia esperando recusa e confere que nada foi gravado: o banco fica igual
/// e a pasta de registros (vazia, só com a pasta-mãe) não ganha nem um
/// arquivo.
fn recusar(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    p: &PerfilCarregado,
    opcoes: &OpcoesAvaliacao,
) -> SpiderError {
    let pasta = Pasta::nova("recusa");
    let antes = foto(conn);
    let erro = avaliar(conn, cfg, p, opcoes, &pasta.registros()).expect_err("avaliação recusada");
    assert!(foto(conn) == antes, "a recusa alterou o banco: {erro}");
    assert_eq!(
        entradas(&pasta.0),
        0,
        "a recusa criou arquivo ou pasta de registros: {erro}"
    );
    erro
}

fn dobras_esperadas() -> [(u32, u32); 5] {
    // Corte 200: avaliados do 2º concurso do índice (2) ao 200 = 199
    // concursos; 199 / 5 = 39 por dobra e a sobra (4) na última.
    [(2, 40), (41, 79), (80, 118), (119, 157), (158, 200)]
}

// ---------------------------------------------------------------------------
// 1. Busca: caminho feliz.
// ---------------------------------------------------------------------------

#[test]
fn busca_relatorio_identifica_perfil_k_corte_e_indice() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);

    let r = busca(&mut conn, &cfg, &p);

    assert_eq!(r.loteria, "lotofacil");
    assert_eq!(r.perfil, NOME);
    assert_eq!(r.versao, 1);
    assert_eq!(r.janela, 10);
    assert_eq!(r.normalizacao, "minmax");
    assert_eq!(r.hash_toml, p.hash_toml);
    // k padrão = K da loteria (15 dezenas sorteadas).
    assert_eq!(r.k, 15);
    assert_eq!(r.corte, CORTE_BAIXO);
    // O índice começa no concurso 1 e vai até o 331 (snapshot do próximo).
    assert_eq!(r.primeiro_indice, 1);
    assert_eq!(r.ultimo_concurso, 330);
    assert!(r.bonferroni.is_none(), "sem --testados não há correção");
}

#[test]
fn busca_tem_cinco_dobras_consecutivas_terminando_no_corte() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);

    let r = busca(&mut conn, &cfg, &p);
    let wf = walk_forward_de(&r);

    assert_eq!(wf.corte, CORTE_BAIXO);
    let faixas: Vec<(u32, u32)> = wf
        .dobras
        .iter()
        .map(|d| (d.faixa.primeiro, d.faixa.ultimo))
        .collect();
    assert_eq!(faixas, dobras_esperadas());
    // Cada dobra começa logo depois da anterior (sem buraco nem sobreposição).
    for par in wf.dobras.windows(2) {
        assert_eq!(par[1].faixa.primeiro, par[0].faixa.ultimo + 1);
    }
}

#[test]
fn busca_veredito_cobre_a_uniao_das_dobras() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);

    let r = busca(&mut conn, &cfg, &p);
    let wf = walk_forward_de(&r);
    let faixa = r.veredito.avaliacao.faixa;

    assert_eq!(faixa.primeiro, wf.dobras[0].faixa.primeiro);
    assert_eq!(faixa.ultimo, wf.dobras[4].faixa.ultimo);
    assert_eq!((faixa.primeiro, faixa.ultimo), (2, 200));
    assert_eq!(faixa.tamanho(), 199);
    let soma: usize = wf
        .dobras
        .iter()
        .map(|d| d.metricas.concursos_avaliados)
        .sum();
    assert_eq!(soma, 199, "as dobras cobrem exatamente o trecho");
    assert_eq!(r.veredito.avaliacao.metricas.concursos_avaliados, 199);
    assert_eq!(r.veredito.avaliacao.topk.concursos_avaliados, 199);
}

#[test]
fn busca_com_199_concursos_tem_bootstrap_com_parametros_fixos() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);

    let r = busca(&mut conn, &cfg, &p);
    let b = r.veredito.bootstrap.as_ref().expect("199 >= 100 concursos");

    assert_eq!(r.veredito.concursos_minimos, 100);
    assert_eq!(b.log_loss.concursos, 199);
    assert_eq!(b.log_loss.bloco, BLOCO_BOOTSTRAP);
    assert_eq!(b.log_loss.reamostragens, 10_000);
    assert_eq!(b.log_loss.semente, SEMENTE_BOOTSTRAP);
    // O Brier usa as mesmas reamostras (mesmo T e mesma semente).
    assert_eq!(b.brier.concursos, 199);
    assert_eq!(b.brier.semente, SEMENTE_BOOTSTRAP);
    assert!((0.0..=1.0).contains(&b.log_loss.p_valor));
}

#[test]
fn busca_com_historico_aleatorio_nao_supera_o_acaso() {
    // Sorteios independentes: o esperado é empatar. A semente é fixa, então o
    // resultado é reproduzível (p do log loss conferido ao escolher a semente;
    // com outra semente aleatória, ~5% dos casos dariam falso positivo).
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);

    let r = busca(&mut conn, &cfg, &p);
    let b = r.veredito.bootstrap.as_ref().expect("bootstrap");

    assert!(
        b.log_loss.p_valor > ALFA,
        "p do log loss {} deveria passar de {ALFA}",
        b.log_loss.p_valor
    );
    assert_eq!(r.veredito.supera_o_acaso(), Some(false));
}

#[test]
fn busca_e_deterministica_em_duas_chamadas() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);

    let primeira = busca(&mut conn, &cfg, &p);
    let segunda = busca(&mut conn, &cfg, &p);

    assert_eq!(primeira, segunda);
}

#[test]
fn busca_com_trecho_curto_sai_sem_bootstrap_e_sem_erro() {
    // 80 concursos, todos abaixo do corte: avaliados 2..=80 = 79 < 100.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 80, SEMENTE), &p);

    let r = busca(&mut conn, &cfg, &p);

    assert!(r.veredito.bootstrap.is_none());
    assert_eq!(r.veredito.supera_o_acaso(), None);
    assert_eq!(r.veredito.concursos_minimos, 100);
    assert_eq!(r.veredito.avaliacao.faixa.tamanho(), 79);
    // O corte é 200, mas o trecho vai só até o último sorteado (80).
    assert_eq!(r.veredito.avaliacao.faixa.ultimo, 80);
    assert_eq!(walk_forward_de(&r).dobras.len(), 5);
}

#[test]
fn busca_curta_com_testados_corrige_so_o_topk() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 80, SEMENTE), &p);
    let opcoes = OpcoesAvaliacao {
        testados: Some(2),
        ..opcoes_busca()
    };
    let pasta = Pasta::nova("curta_testados");

    let r = avaliar(&mut conn, &cfg, &p, &opcoes, &pasta.registros()).expect("avaliada");

    let c = r.bonferroni.as_ref().expect("com --testados");
    assert_eq!(c.comparacoes, 2);
    assert_eq!(c.p_log_loss, None, "sem bootstrap não há p do log loss");
    assert_eq!(c.supera_o_acaso, None);
    let p_topk = r.veredito.avaliacao.topk.p_valor.probabilidade;
    assert_eq!(c.p_topk.probabilidade, (p_topk * 2.0).min(1.0));
}

#[test]
fn busca_aceita_k_maximo_n_menos_1() {
    // k válido vai de 1 a N-1 = 24.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);
    let opcoes = OpcoesAvaliacao {
        k: Some(24),
        ..opcoes_busca()
    };
    let pasta = Pasta::nova("k24");

    let r = avaliar(&mut conn, &cfg, &p, &opcoes, &pasta.registros()).expect("k = 24 vale");

    assert_eq!(r.k, 24);
    assert_eq!(r.veredito.avaliacao.topk.k, 24);
}

// ---------------------------------------------------------------------------
// 2. Nada gravado no banco (nem na pasta de registros, na busca).
// ---------------------------------------------------------------------------

#[test]
fn busca_nao_grava_nada_no_banco_nem_cria_pasta_de_registros() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 250, SEMENTE), &p);
    let pasta = Pasta::nova("busca_sem_gravar");
    let antes = foto(&conn);
    assert!(
        nomes_das_tabelas(&antes).contains(&"indice"),
        "a foto cobre as tabelas derivadas: {:?}",
        nomes_das_tabelas(&antes)
    );

    avaliar(&mut conn, &cfg, &p, &opcoes_busca(), &pasta.registros()).expect("busca");

    assert!(foto(&conn) == antes, "a busca alterou o banco");
    assert!(
        !pasta.registros().exists(),
        "a busca nunca cria a pasta de registros"
    );
    assert_eq!(entradas(&pasta.0), 0);
}

#[test]
fn trancado_nao_grava_nada_no_banco() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    let pasta = Pasta::nova("trancado_sem_gravar_no_banco");
    let antes = foto(&conn);

    avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("trancado");

    assert!(foto(&conn) == antes, "o trancado alterou o banco");
    // O único arquivo gravado é o registro.
    assert!(arquivo_do_registro(&pasta.registros()).is_file());
    assert_eq!(entradas(&pasta.registros()), 1);
}

#[test]
fn busca_com_testados_e_k_nao_grava_nada() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 250, SEMENTE), &p);
    let pasta = Pasta::nova("busca_testados_sem_gravar");
    let antes = foto(&conn);
    let opcoes = OpcoesAvaliacao {
        k: Some(7),
        testados: Some(4),
        ..opcoes_busca()
    };

    avaliar(&mut conn, &cfg, &p, &opcoes, &pasta.registros()).expect("busca");

    assert!(foto(&conn) == antes);
    assert_eq!(entradas(&pasta.0), 0);
}

// ---------------------------------------------------------------------------
// 3. Recusas: nada gravado, nem no banco nem nos registros.
// ---------------------------------------------------------------------------

fn recusa_de_opcao(opcoes: OpcoesAvaliacao) -> SpiderError {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);
    recusar(&mut conn, &cfg, &p, &opcoes)
}

#[test]
fn recusa_repetir_sem_trancado() {
    let erro = recusa_de_opcao(OpcoesAvaliacao {
        repetir: true,
        ..opcoes_busca()
    });
    assert!(
        matches!(&erro, SpiderError::AvaliacaoOpcaoInvalida(m) if m.contains("--repetir")),
        "{erro:?}"
    );
}

#[test]
fn recusa_testados_zero() {
    let erro = recusa_de_opcao(OpcoesAvaliacao {
        testados: Some(0),
        ..opcoes_busca()
    });
    assert!(
        matches!(&erro, SpiderError::AvaliacaoOpcaoInvalida(m) if m.contains("--testados")),
        "{erro:?}"
    );
}

#[test]
fn recusa_testados_com_trancado() {
    let erro = recusa_de_opcao(OpcoesAvaliacao {
        testados: Some(3),
        ..opcoes_trancado()
    });
    assert!(
        matches!(&erro, SpiderError::AvaliacaoOpcaoInvalida(m) if m.contains("--testados")),
        "{erro:?}"
    );
}

#[test]
fn recusa_perfil_de_outra_loteria() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);
    let da_quina = carregar_texto(
        &texto_perfil(NOME, "quina", 10, Normalizacao::Minmax, 1.0),
        "quina",
        NOME,
    );

    let erro = recusar(&mut conn, &cfg, &da_quina, &opcoes_busca());

    let SpiderError::PerfilDeOutraLoteria {
        loteria_perfil,
        loteria_banco,
        ..
    } = erro
    else {
        panic!("esperava PerfilDeOutraLoteria, veio {erro:?}");
    };
    assert_eq!(loteria_perfil, "quina");
    assert_eq!(loteria_banco, "lotofacil");
}

#[test]
fn recusa_banco_sem_resultados() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let mut conn = banco();

    let erro = recusar(&mut conn, &cfg, &perfil_teste(), &opcoes_busca());

    assert!(
        matches!(&erro, SpiderError::SemResultados(l) if l == "lotofacil"),
        "{erro:?}"
    );
}

#[test]
fn recusa_resultados_com_buraco() {
    // `salvar_resultados` não confere buraco: grava 1..=150 sem o 77.
    let cfg = cfg_corte(CORTE_BAIXO);
    let hist: Vec<Resultado> = historico(&cfg, 150, SEMENTE)
        .into_iter()
        .filter(|r| r.concurso != 77)
        .collect();
    assert_eq!(hist.len(), 149);
    let mut conn = banco();
    gravar(&mut conn, &hist);

    let erro = recusar(&mut conn, &cfg, &perfil_teste(), &opcoes_busca());

    let SpiderError::ResultadosReprovados { erros, .. } = erro else {
        panic!("esperava ResultadosReprovados, veio {erro:?}");
    };
    assert!(!erros.is_empty());
}

#[test]
fn recusa_perfil_sem_tabela_perfis() {
    // Features calculadas, mas `sync indice` nunca rodou: não há `perfis`.
    let cfg = cfg_corte(CORTE_BAIXO);
    let mut conn = com_features(&cfg, &historico(&cfg, 120, SEMENTE));

    let erro = recusar(&mut conn, &cfg, &perfil_teste(), &opcoes_busca());

    let SpiderError::PerfilNaoRegistrado { nome, loteria } = &erro else {
        panic!("esperava PerfilNaoRegistrado, veio {erro:?}");
    };
    assert_eq!((nome.as_str(), loteria.as_str()), (NOME, "lotofacil"));
    // A mensagem diz o que fazer.
    assert!(erro.to_string().contains("sync indice lotofacil"), "{erro}");
}

#[test]
fn recusa_perfil_ausente_de_perfis_existente() {
    // A tabela `perfis` existe (outro perfil registrado), mas não este nome.
    let cfg = cfg_corte(CORTE_BAIXO);
    let outro = perfil_com("outro_v1", 1.5);
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &outro);
    assert!(db::tabela_existe(&conn, "perfis").expect("sqlite_master"));

    let erro = recusar(&mut conn, &cfg, &perfil_teste(), &opcoes_busca());

    assert!(
        matches!(&erro, SpiderError::PerfilNaoRegistrado { nome, .. } if nome == NOME),
        "{erro:?}"
    );
}

#[test]
fn recusa_perfil_com_hash_diferente_do_toml() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let registrado = perfil_com(NOME, 1.0);
    let editado = perfil_com(NOME, 2.0);
    assert_ne!(registrado.hash_toml, editado.hash_toml);
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &registrado);

    let erro = recusar(&mut conn, &cfg, &editado, &opcoes_busca());

    let SpiderError::PerfilAlterado {
        nome,
        hash_registrado,
        hash_arquivo,
        ..
    } = erro
    else {
        panic!("esperava PerfilAlterado, veio {erro:?}");
    };
    assert_eq!(nome, NOME);
    assert_eq!(hash_registrado, registrado.hash_toml);
    assert_eq!(hash_arquivo, editado.hash_toml);
}

#[test]
fn recusa_features_desatualizadas() {
    // Resultado novo gravado depois do índice, sem rodar `sync features`.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let hist = historico(&cfg, 121, SEMENTE);
    let mut conn = preparado(&cfg, &hist[..120], &p);
    gravar(&mut conn, &hist[120..]);

    let erro = recusar(&mut conn, &cfg, &p, &opcoes_busca());

    let SpiderError::FeaturesDesatualizadas {
        ultimo_concurso,
        snapshot,
        ..
    } = erro
    else {
        panic!("esperava FeaturesDesatualizadas, veio {erro:?}");
    };
    // As features vão até o snapshot do 121 (o do "próximo" de 120), mas o
    // último resultado agora é o 121.
    assert_eq!((ultimo_concurso, snapshot), (121, 121));
}

#[test]
fn recusa_indice_desatualizado() {
    // Resultado novo e features refeitas, mas o índice não.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let hist = historico(&cfg, 121, SEMENTE);
    let mut conn = preparado(&cfg, &hist[..120], &p);
    gravar(&mut conn, &hist[120..]);
    sincronizar_features(&mut conn, &cfg, false).expect("features refeitas");

    let erro = recusar(&mut conn, &cfg, &p, &opcoes_busca());

    let SpiderError::IndiceDesatualizado {
        perfil,
        ultimo_indice,
        esperado,
        ..
    } = erro
    else {
        panic!("esperava IndiceDesatualizado, veio {erro:?}");
    };
    // O índice vai até o snapshot do 121; com o resultado 121 tem de ir até
    // o 122.
    assert_eq!(perfil, NOME);
    assert_eq!(ultimo_indice, Some(121));
    assert_eq!(esperado, 122);
}

#[test]
fn recusa_sem_tabela_indice() {
    // `indice` e `perfis` nascem juntas; só um banco mexido por fora perde a
    // tabela. O perfil continua registrado.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);
    conn.execute("DROP TABLE indice", []).expect("apaga indice");

    let erro = recusar(&mut conn, &cfg, &p, &opcoes_busca());

    assert!(
        matches!(
            &erro,
            SpiderError::IndiceDesatualizado {
                ultimo_indice: None,
                esperado: 121,
                ..
            }
        ),
        "{erro:?}"
    );
}

#[test]
fn recusa_indice_incoerente_com_linha_faltando_no_meio() {
    // A checagem do último concurso passa (o 121 está lá); falta só uma linha
    // (concurso 50, dezena 7).
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);
    let apagadas = conn
        .execute(
            "DELETE FROM indice WHERE concurso = 50 AND dezena = 7 AND perfil = ?1",
            [NOME],
        )
        .expect("apaga uma linha");
    assert_eq!(apagadas, 1);

    let erro = recusar(&mut conn, &cfg, &p, &opcoes_busca());

    assert!(
        matches!(&erro, SpiderError::IndiceIncoerente { perfil, .. } if perfil == NOME),
        "{erro:?}"
    );
}

fn eh_indice_de_outras_features(erro: &SpiderError) -> bool {
    matches!(erro, SpiderError::IndiceIncoerente { perfil, detalhe, .. }
        if perfil == NOME && detalhe.contains("diferem do índice recalculado com as features atuais"))
}

#[test]
fn recusa_indice_de_features_regeneradas_depois_de_corrigir_resultado_antigo() {
    // Cenário da auditoria: o concurso 5 é corrigido e `sync features`
    // regenera tudo, mas o `sync indice` não é refeito. O índice velho
    // continua terminando no último + 1 (passa na checagem de cobertura) e
    // tem as mesmas chaves; só a comparação com o índice recalculado o pega.
    // Busca e trancado recusam sem gravar nada (`recusar` confere banco e
    // pasta de registros).
    let (cfg, p, mut conn) = cenario_trancado();
    let hist = historico(&cfg, 330, SEMENTE);
    let indice_antes = db::ler_indice(&conn, NOME).expect("índice lido");
    gravar(&mut conn, &[com_dezenas_trocadas(&cfg, &hist[4])]);
    sincronizar_features(&mut conn, &cfg, false).expect("features regeneradas");
    let indice_gravado = db::ler_indice(&conn, NOME).expect("índice lido");
    assert_eq!(indice_gravado, indice_antes, "o índice não foi refeito");
    assert_eq!(indice_gravado.last().map(|l| l.concurso), Some(331));

    for opcoes in [opcoes_busca(), opcoes_trancado()] {
        let erro = recusar(&mut conn, &cfg, &p, &opcoes);
        assert!(
            eh_indice_de_outras_features(&erro),
            "trancado = {}: {erro:?}",
            opcoes.trancado
        );
        assert!(
            erro.to_string()
                .contains("sync indice lotofacil --perfil teste_v1"),
            "{erro}"
        );
    }

    // Refeito o índice, as duas avaliam (e o trancado registra o uso).
    indexar(&mut conn, &cfg, &p);
    busca(&mut conn, &cfg, &p);
    let pasta = Pasta::nova("recalculado_trancado");
    avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros())
        .expect("trancado com o índice em dia");
    assert_eq!(registro_lido(&pasta.registros()).len(), 1);
}

#[test]
fn so_a_data_corrigida_com_features_regeneradas_mantem_o_indice_valido() {
    // A data não entra nas features: regeneradas, ficam iguais, e o índice
    // gravado continua idêntico ao recalculado. A checagem não recusa à toa.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let hist = historico(&cfg, 120, SEMENTE);
    let mut conn = preparado(&cfg, &hist, &p);
    let antes = busca(&mut conn, &cfg, &p);
    let mut corrigido = hist[4].clone();
    corrigido.data_sorteio = "2020-02-29".to_string();
    gravar(&mut conn, &[corrigido]);
    sincronizar_features(&mut conn, &cfg, false).expect("features regeneradas");

    assert_eq!(busca(&mut conn, &cfg, &p), antes);
}

/// Alteração feita por fora numa tabela do banco.
type Mexida = Box<dyn Fn(&Connection)>;

#[test]
fn recusa_indice_com_total_posicao_ou_variacao_mexidos_por_fora() {
    // Cada coluna do índice, mexida sozinha numa linha, é recusa: `total`
    // bit a bit (um ulp já basta), `posicao` (troca entre duas dezenas do
    // mesmo concurso, que mantém a permutação) e `variacao_posicao`.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let original = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);
    let linha = db::ler_indice(&original, NOME)
        .expect("índice lido")
        .into_iter()
        .find(|l| l.concurso == 60 && l.total != 0.0)
        .expect("linha com total não nulo");
    let proximo_ulp = f64::from_bits(linha.total.to_bits() + 1);
    let posicao_de = |dezena: u32| {
        db::ler_indice(&original, NOME)
            .expect("índice lido")
            .into_iter()
            .find(|l| (l.concurso, l.dezena) == (60, dezena))
            .expect("linha presente")
            .posicao
    };
    let posicoes = (posicao_de(1), posicao_de(2));
    // (coluna, mexida, linhas que passam a diferir do recalculado).
    let mexidas: [(&str, Mexida, usize); 3] = [
        (
            "total",
            Box::new(move |c: &Connection| {
                c.execute(
                    "UPDATE indice SET total = ?1 WHERE perfil = ?2 AND concurso = ?3 AND dezena = ?4",
                    rusqlite::params![proximo_ulp, NOME, linha.concurso, linha.dezena],
                )
                .expect("total mexido");
            }),
            1,
        ),
        (
            "posicao",
            Box::new(move |c: &Connection| {
                // Uma linha por vez, com as posições lidas antes: um único
                // UPDATE com subconsulta veria a primeira linha já trocada.
                for (dezena, posicao) in [(1, posicoes.1), (2, posicoes.0)] {
                    c.execute(
                        "UPDATE indice SET posicao = ?1
                         WHERE perfil = ?2 AND concurso = 60 AND dezena = ?3",
                        rusqlite::params![posicao, NOME, dezena],
                    )
                    .expect("posição trocada");
                }
            }),
            2,
        ),
        (
            "variacao_posicao",
            Box::new(|c: &Connection| {
                c.execute(
                    "UPDATE indice SET variacao_posicao = variacao_posicao + 1
                     WHERE perfil = ?1 AND concurso = 60 AND dezena = 3",
                    [NOME],
                )
                .expect("variação mexida");
            }),
            1,
        ),
    ];
    for (coluna, mexer, diferem) in &mexidas {
        let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);
        mexer(&conn);
        assert_ne!(
            db::ler_indice(&conn, NOME).expect("lido"),
            db::ler_indice(&original, NOME).expect("lido"),
            "{coluna}: a mexida mudou o índice"
        );

        let erro = recusar(&mut conn, &cfg, &p, &opcoes_busca());

        assert!(eh_indice_de_outras_features(&erro), "{coluna}: {erro:?}");
        assert!(
            erro.to_string()
                .contains(&format!("{diferem} linha(s) diferem")),
            "{coluna}: {erro}"
        );
    }
}

#[test]
fn recusa_k_zero_e_k_igual_a_n() {
    // k vale de 1 a N-1 = 24 (N = 25 dezenas).
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);
    for k in [0, 25, 100] {
        let opcoes = OpcoesAvaliacao {
            k: Some(k),
            ..opcoes_busca()
        };
        let erro = recusar(&mut conn, &cfg, &p, &opcoes);
        assert!(
            matches!(erro, SpiderError::AvaliacaoEntradaInvalida(_)),
            "k = {k}: {erro:?}"
        );
    }
}

#[test]
fn recusa_k_com_trancado_sem_registrar_uso() {
    // O k vale só na busca: escolher o k olhando o trancado seria uma
    // comparação múltipla que o Bonferroni dos perfis não conta. `recusar`
    // confere que nem a pasta de registros foi criada.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    for k in [Some(5), Some(15), Some(0)] {
        let opcoes = OpcoesAvaliacao {
            k,
            ..opcoes_trancado()
        };

        let erro = recusar(&mut conn, &cfg, &p, &opcoes);

        assert!(
            matches!(&erro, SpiderError::AvaliacaoOpcaoInvalida(m) if m.contains("--k")),
            "k = {k:?}: {erro:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Conjunto trancado e seu registro.
// ---------------------------------------------------------------------------

/// Cenário do trancado: corte 200, 330 concursos (130 no trancado, o bastante
/// para o bootstrap).
fn cenario_trancado() -> (LoteriaConfig, PerfilCarregado, Connection) {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    (cfg, p, conn)
}

#[test]
fn trancado_primeira_avaliacao_registra_o_uso() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_primeiro");

    let r = avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros())
        .expect("primeiro uso do trancado");

    let esperada = AvaliacaoTrancado {
        perfil: NOME.to_string(),
        hash_toml: p.hash_toml.clone(),
        ate_concurso: 330,
        avaliado_em: AVALIADO_EM.to_string(),
    };
    let uso = uso_do_trancado(&r);
    assert!(
        uso.anteriores.is_empty(),
        "primeira vez: sem usos anteriores"
    );
    assert_eq!(uso.registrada, esperada);
    assert_eq!(uso.arquivo, arquivo_do_registro(&pasta.registros()));
    // Primeiro uso: a leitura achou o arquivo inexistente, com 0 usos.
    assert!(uso.arquivo.is_absolute());
    assert_eq!((uso.usos_lidos, uso.arquivo_existia), (0, false));
    assert_eq!(registro_lido(&pasta.registros()), vec![esperada]);
}

#[test]
fn trancado_avalia_do_corte_mais_um_ate_o_ultimo_sorteado() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_faixa");

    let r = avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("trancado");

    let faixa = r.veredito.avaliacao.faixa;
    assert_eq!((faixa.primeiro, faixa.ultimo), (201, 330));
    assert_eq!(faixa.tamanho(), 130);
    // O registro guarda o último concurso da faixa avaliada.
    assert_eq!(uso_do_trancado(&r).registrada.ate_concurso, faixa.ultimo);
    assert_eq!(r.corte, CORTE_BAIXO);
    assert_eq!(r.primeiro_indice, 1);
    assert_eq!(r.ultimo_concurso, 330);
    assert_eq!(r.k, 15);
    let b = r.veredito.bootstrap.as_ref().expect("130 >= 100 concursos");
    assert_eq!(b.log_loss.concursos, 130);
}

#[test]
fn trancado_primeiro_uso_tem_uma_comparacao_e_p_sem_correcao() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_bonferroni1");

    let r = avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("trancado");

    let c = r.bonferroni.as_ref().expect("o trancado sempre corrige");
    let b = r.veredito.bootstrap.as_ref().expect("bootstrap");
    assert_eq!(c.comparacoes, 1);
    // N = 1: min(1, 1·p) = p.
    assert_eq!(c.p_log_loss, Some(b.log_loss.p_valor));
    assert_eq!(
        c.p_topk.probabilidade,
        r.veredito.avaliacao.topk.p_valor.probabilidade
    );
    assert_eq!(c.supera_o_acaso, r.veredito.supera_o_acaso());
}

#[test]
fn trancado_segunda_avaliacao_do_mesmo_perfil_e_recusada_sem_mexer_no_arquivo() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_segundo");
    avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("primeiro uso");
    let arquivo = arquivo_do_registro(&pasta.registros());
    let antes = std::fs::read(&arquivo).expect("registro lido");

    let erro = avaliar_em(
        &mut conn,
        &cfg,
        &p,
        &opcoes_trancado(),
        &pasta.registros(),
        AVALIADO_DEPOIS,
    )
    .expect_err("uso único por perfil");

    let SpiderError::TrancadoJaAvaliado {
        loteria,
        perfil,
        hash_toml,
        anteriores,
        registro,
    } = &erro
    else {
        panic!("esperava TrancadoJaAvaliado, veio {erro:?}");
    };
    assert_eq!(loteria, "lotofacil");
    assert_eq!(perfil, NOME);
    assert_eq!(hash_toml, &p.hash_toml);
    assert_eq!(anteriores.len(), 1);
    assert_eq!(anteriores[0].avaliado_em, AVALIADO_EM);
    // A recusa também diz qual registro foi lido e quantos usos ele tinha.
    assert_eq!(registro.caminho, arquivo);
    assert!(
        registro.caminho.is_absolute(),
        "{}",
        registro.caminho.display()
    );
    assert_eq!(registro.usos, 1);
    let mensagem = erro.to_string();
    assert!(
        mensagem.contains(&arquivo.display().to_string()) && mensagem.contains("1 uso(s) no total"),
        "{mensagem}"
    );
    assert_eq!(std::fs::read(&arquivo).expect("registro lido"), antes);
}

#[test]
fn trancado_com_repetir_avalia_de_novo_e_registra_as_duas() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_repetir");
    let primeiro =
        avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("primeiro uso");
    let opcoes = OpcoesAvaliacao {
        repetir: true,
        ..opcoes_trancado()
    };

    let segundo = avaliar_em(
        &mut conn,
        &cfg,
        &p,
        &opcoes,
        &pasta.registros(),
        AVALIADO_DEPOIS,
    )
    .expect("--repetir avalia de novo");

    let uso = uso_do_trancado(&segundo);
    assert_eq!(uso.anteriores.len(), 1);
    assert_eq!(uso.anteriores[0], uso_do_trancado(&primeiro).registrada);
    assert_eq!((uso.usos_lidos, uso.arquivo_existia), (1, true));
    assert_eq!(uso.registrada.avaliado_em, AVALIADO_DEPOIS);
    let arquivo = registro_lido(&pasta.registros());
    assert_eq!(arquivo.len(), 2);
    assert_eq!(arquivo[0].avaliado_em, AVALIADO_EM);
    assert_eq!(arquivo[1].avaliado_em, AVALIADO_DEPOIS);
    // Mesmo perfil e hash: continua 1 perfil distinto, então 1 comparação.
    assert_eq!(registros::perfis_distintos(&arquivo), 1);
    assert_eq!(
        segundo.bonferroni.as_ref().expect("bonferroni").comparacoes,
        1
    );
    // A avaliação em si é a mesma (determinística).
    assert_eq!(segundo.veredito, primeiro.veredito);
}

#[test]
fn trancado_de_outro_perfil_conta_duas_comparacoes_e_corrige_os_p() {
    let (cfg, p, mut conn) = cenario_trancado();
    let outro = perfil_com("outro_v1", 1.5);
    indexar(&mut conn, &cfg, &outro);
    let pasta = Pasta::nova("trancado_dois_perfis");
    avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("primeiro perfil");

    let r = avaliar_em(
        &mut conn,
        &cfg,
        &outro,
        &opcoes_trancado(),
        &pasta.registros(),
        AVALIADO_DEPOIS,
    )
    .expect("outro perfil também tem o seu uso único");

    let c = r.bonferroni.as_ref().expect("bonferroni");
    let b = r.veredito.bootstrap.as_ref().expect("bootstrap");
    assert_eq!(c.comparacoes, 2);
    // min(1, 2·p) do log loss (bootstrap) e do top-k (exato).
    let p_log_loss = (2.0 * b.log_loss.p_valor).min(1.0);
    assert_eq!(c.p_log_loss, Some(p_log_loss));
    let p_topk = r.veredito.avaliacao.topk.p_valor.probabilidade;
    assert_eq!(c.p_topk.probabilidade, (2.0 * p_topk).min(1.0));
    assert_eq!(c.supera_o_acaso, Some(p_log_loss <= ALFA));
    // Este perfil nunca usou o trancado: sem usos anteriores, mas o
    // registro lido tinha o uso do primeiro perfil.
    assert!(uso_do_trancado(&r).anteriores.is_empty());
    assert_eq!(
        (
            uso_do_trancado(&r).usos_lidos,
            uso_do_trancado(&r).arquivo_existia
        ),
        (1, true)
    );
    assert_eq!(registro_lido(&pasta.registros()).len(), 2);
}

#[test]
fn trancado_com_registro_existente_e_vazio_le_zero_usos_com_arquivo() {
    // Arquivo existente sem avaliações: 0 usos, mas não "inexistente".
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_registro_vazio");
    registros::salvar(&arquivo_do_registro(&pasta.registros()), "lotofacil", &[])
        .expect("registro vazio");

    let r = avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("trancado");

    let uso = uso_do_trancado(&r);
    assert_eq!((uso.usos_lidos, uso.arquivo_existia), (0, true));
    assert_eq!(registro_lido(&pasta.registros()).len(), 1);
}

#[test]
fn trancado_do_mesmo_nome_com_outro_hash_e_outro_perfil() {
    // Registro com o mesmo nome e outro conteúdo (hash): não bloqueia, mas
    // conta como perfil distinto.
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_outro_hash");
    let antigo = AvaliacaoTrancado {
        perfil: NOME.to_string(),
        hash_toml: "ff".repeat(32),
        ate_concurso: 300,
        avaliado_em: "2026-10-05T00:00:00Z".to_string(),
    };
    registros::salvar(
        &arquivo_do_registro(&pasta.registros()),
        "lotofacil",
        std::slice::from_ref(&antigo),
    )
    .expect("registro preparado");

    let r = avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros())
        .expect("hash diferente não é o mesmo perfil");

    assert!(uso_do_trancado(&r).anteriores.is_empty());
    assert_eq!(r.bonferroni.as_ref().expect("bonferroni").comparacoes, 2);
    let arquivo = registro_lido(&pasta.registros());
    assert_eq!(arquivo.len(), 2);
    assert_eq!(arquivo[0], antigo);
}

#[test]
fn trancado_com_registro_ilegivel_e_recusado_sem_mexer_no_arquivo() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_lixo");
    let arquivo = arquivo_do_registro(&pasta.registros());
    std::fs::create_dir_all(pasta.registros()).expect("pasta de registros");
    std::fs::write(&arquivo, "isto não é json").expect("lixo gravado");
    let antes = foto(&conn);

    let erro = avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros())
        .expect_err("registro ilegível");

    assert!(
        matches!(&erro, SpiderError::RegistroTrancadoInvalido { caminho, .. } if *caminho == arquivo),
        "{erro:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&arquivo).expect("arquivo lido"),
        "isto não é json"
    );
    assert_eq!(entradas(&pasta.registros()), 1, "nenhum arquivo novo");
    assert!(foto(&conn) == antes);
}

#[test]
fn trancado_com_registro_de_outra_loteria_e_recusado() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_outra_loteria");
    let arquivo = arquivo_do_registro(&pasta.registros());
    std::fs::create_dir_all(pasta.registros()).expect("pasta de registros");
    let texto = "{\n  \"loteria\": \"quina\",\n  \"avaliacoes\": []\n}\n";
    std::fs::write(&arquivo, texto).expect("registro de outra loteria");

    let erro = avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros())
        .expect_err("registro de outra loteria");

    assert!(
        matches!(erro, SpiderError::RegistroTrancadoInvalido { .. }),
        "{erro:?}"
    );
    assert_eq!(std::fs::read_to_string(&arquivo).expect("lido"), texto);
}

#[test]
fn trancado_recusado_por_indice_desatualizado_nao_cria_registro() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let hist = historico(&cfg, 331, SEMENTE);
    let mut conn = preparado(&cfg, &hist[..330], &p);
    gravar(&mut conn, &hist[330..]);
    sincronizar_features(&mut conn, &cfg, false).expect("features refeitas");

    // `recusar` confere que nem a pasta de registros foi criada.
    let erro = recusar(&mut conn, &cfg, &p, &opcoes_trancado());

    assert!(
        matches!(erro, SpiderError::IndiceDesatualizado { .. }),
        "{erro:?}"
    );
}

#[test]
fn trancado_recusado_nao_acrescenta_ao_registro_existente() {
    // Primeiro uso feito; depois o banco ganha um concurso e o índice fica
    // desatualizado: `--repetir` é recusado e o arquivo não muda.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let hist = historico(&cfg, 331, SEMENTE);
    let mut conn = preparado(&cfg, &hist[..330], &p);
    let pasta = Pasta::nova("trancado_recusa_com_registro");
    avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("primeiro uso");
    let arquivo = arquivo_do_registro(&pasta.registros());
    let antes = std::fs::read(&arquivo).expect("registro lido");
    gravar(&mut conn, &hist[330..]);
    sincronizar_features(&mut conn, &cfg, false).expect("features refeitas");
    let opcoes = OpcoesAvaliacao {
        repetir: true,
        ..opcoes_trancado()
    };

    let erro = avaliar_em(
        &mut conn,
        &cfg,
        &p,
        &opcoes,
        &pasta.registros(),
        AVALIADO_DEPOIS,
    )
    .expect_err("índice desatualizado");

    assert!(
        matches!(erro, SpiderError::IndiceDesatualizado { .. }),
        "{erro:?}"
    );
    assert_eq!(std::fs::read(&arquivo).expect("registro lido"), antes);
}

#[test]
fn trancado_sem_concursos_acima_do_corte_nao_registra_uso() {
    // 150 concursos, corte 200: não há trancado a avaliar.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 150, SEMENTE), &p);

    let erro = recusar(&mut conn, &cfg, &p, &opcoes_trancado());

    assert!(
        matches!(erro, SpiderError::AvaliacaoSemConcursos(_)),
        "{erro:?}"
    );
}

#[test]
fn trancado_grava_json_deterministico_com_lf() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_json");
    avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &pasta.registros()).expect("trancado");
    let arquivo = arquivo_do_registro(&pasta.registros());

    let texto = std::fs::read_to_string(&arquivo).expect("registro lido");

    assert_eq!(
        texto,
        format!(
            "{{\n  \"loteria\": \"lotofacil\",\n  \"avaliacoes\": [\n    \
             {{\"perfil\":\"teste_v1\",\"hash_toml\":\"{}\",\"ate_concurso\":330,\
             \"avaliado_em\":\"{AVALIADO_EM}\"}}\n  ]\n}}\n",
            p.hash_toml
        )
    );
    assert!(!texto.contains('\r'), "o registro usa só LF");
    // Ler e gravar de volta dá os mesmos bytes.
    let lido = registro_lido(&pasta.registros());
    registros::salvar(&arquivo, "lotofacil", &lido).expect("regravado");
    assert_eq!(std::fs::read_to_string(&arquivo).expect("lido"), texto);
    assert!(!arquivo.with_extension("json.tmp").exists());
}

#[test]
fn trancado_em_duas_pastas_grava_os_mesmos_bytes() {
    let (cfg, p, mut conn) = cenario_trancado();
    let a = Pasta::nova("trancado_bytes_a");
    let b = Pasta::nova("trancado_bytes_b");

    avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &a.registros()).expect("a");
    avaliar(&mut conn, &cfg, &p, &opcoes_trancado(), &b.registros()).expect("b");

    assert_eq!(
        std::fs::read(arquivo_do_registro(&a.registros())).expect("a lido"),
        std::fs::read(arquivo_do_registro(&b.registros())).expect("b lido")
    );
}

// ---------------------------------------------------------------------------
// 4b. Ordem: relatório montado antes de gravar; recarga antes de salvar.
// ---------------------------------------------------------------------------

fn avaliar_formatando<T>(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    p: &PerfilCarregado,
    opcoes: &OpcoesAvaliacao,
    registros: &Path,
    formatar: impl FnMut(&RelatorioAvaliacao) -> T,
) -> Result<(RelatorioAvaliacao, T), SpiderError> {
    avaliar_perfil_e_formatar(conn, cfg, p, opcoes, registros, AVALIADO_EM, formatar)
}

/// Uso de outro perfil, como se gravado por outra execução.
fn uso_alheio(perfil: &str, hash: &str, quando: &str) -> AvaliacaoTrancado {
    AvaliacaoTrancado {
        perfil: perfil.to_string(),
        hash_toml: hash.to_string(),
        ate_concurso: 330,
        avaliado_em: quando.to_string(),
    }
}

/// Grava no registro `uso` além do que já houver lá (outra execução).
fn acrescentar_ao_registro(registros: &Path, uso: AvaliacaoTrancado) {
    let arquivo = arquivo_do_registro(registros);
    let mut todas = registros::carregar(&arquivo, "lotofacil").expect("registro lido");
    todas.push(uso);
    registros::salvar(&arquivo, "lotofacil", &todas).expect("registro gravado");
}

#[test]
fn busca_formata_uma_vez_e_nao_grava() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    let pasta = Pasta::nova("busca_formata");
    let mut chamadas = 0;

    let (r, texto) = avaliar_formatando(
        &mut conn,
        &cfg,
        &p,
        &opcoes_busca(),
        &pasta.registros(),
        |r| {
            chamadas += 1;
            r.perfil.clone()
        },
    )
    .expect("busca");

    assert_eq!(chamadas, 1);
    assert_eq!(texto, NOME);
    assert_eq!(r, busca(&mut conn, &cfg, &p));
    assert_eq!(entradas(&pasta.0), 0, "a busca não grava arquivo");
}

#[test]
fn trancado_formata_o_relatorio_antes_de_gravar_o_uso() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_formata_antes");
    let arquivo = arquivo_do_registro(&pasta.registros());
    let mut chamadas = 0;

    let (r, existia_ao_formatar) = avaliar_formatando(
        &mut conn,
        &cfg,
        &p,
        &opcoes_trancado(),
        &pasta.registros(),
        |r| {
            chamadas += 1;
            // O relatório já diz onde o uso vai ficar, mas ainda não gravou.
            assert_eq!(uso_do_trancado(r).arquivo, arquivo);
            arquivo.exists()
        },
    )
    .expect("trancado");

    assert_eq!(chamadas, 1);
    assert!(
        !existia_ao_formatar,
        "o relatório é montado antes de o uso ser gravado"
    );
    assert_eq!(
        registro_lido(&pasta.registros()),
        vec![uso_do_trancado(&r).registrada.clone()]
    );
}

#[test]
fn trancado_recarrega_o_registro_antes_de_gravar_e_junta_o_uso_de_outra_execucao() {
    // Outra execução grava o uso de outro perfil enquanto este relatório é
    // montado: a recarga logo antes de salvar acha esse uso, o relatório é
    // refeito com N = 2 e o arquivo fica com os dois.
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_recarga");
    let alheio = uso_alheio("outro_v1", &"aa".repeat(32), "2026-10-06T11:00:00Z");
    let mut comparacoes_vistas = Vec::new();

    let (r, comparacoes) = avaliar_formatando(
        &mut conn,
        &cfg,
        &p,
        &opcoes_trancado(),
        &pasta.registros(),
        |r| {
            let n = r.bonferroni.as_ref().expect("bonferroni").comparacoes;
            if comparacoes_vistas.is_empty() {
                acrescentar_ao_registro(&pasta.registros(), alheio.clone());
            }
            comparacoes_vistas.push(n);
            n
        },
    )
    .expect("trancado");

    assert_eq!(comparacoes_vistas, [1, 2], "refeito depois da recarga");
    assert_eq!(comparacoes, 2, "o texto devolvido é o da última montagem");
    assert_eq!(r.bonferroni.as_ref().expect("bonferroni").comparacoes, 2);
    assert!(uso_do_trancado(&r).anteriores.is_empty());
    // O relatório diz o que a leitura sobre a qual o uso foi gravado tinha:
    // a recarga, com o uso da outra execução.
    assert_eq!(
        (
            uso_do_trancado(&r).usos_lidos,
            uso_do_trancado(&r).arquivo_existia
        ),
        (1, true)
    );
    let gravado = registro_lido(&pasta.registros());
    assert_eq!(
        gravado,
        registros::ordenar(vec![alheio, uso_do_trancado(&r).registrada.clone()]),
        "o uso da outra execução não é apagado"
    );
}

#[test]
fn trancado_recarga_com_o_mesmo_perfil_ja_registrado_recusa_sem_gravar() {
    // Duas execuções do mesmo perfil ao mesmo tempo: a que grava primeiro
    // vale; a outra, na recarga, vira a recusa do uso único.
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_recarga_mesmo");
    let primeira = uso_alheio(NOME, &p.hash_toml, "2026-10-06T11:00:00Z");

    let erro = avaliar_formatando(
        &mut conn,
        &cfg,
        &p,
        &opcoes_trancado(),
        &pasta.registros(),
        |_| acrescentar_ao_registro(&pasta.registros(), primeira.clone()),
    )
    .expect_err("o mesmo perfil já registrado na recarga");

    assert!(
        matches!(&erro, SpiderError::TrancadoJaAvaliado { anteriores, .. } if anteriores == std::slice::from_ref(&primeira)),
        "{erro:?}"
    );
    assert_eq!(registro_lido(&pasta.registros()), vec![primeira]);
}

#[test]
fn trancado_com_registro_mudando_sempre_desiste_sem_gravar() {
    let (cfg, p, mut conn) = cenario_trancado();
    let pasta = Pasta::nova("trancado_concorrente");
    let mut chamadas = 0u32;

    let erro = avaliar_formatando(
        &mut conn,
        &cfg,
        &p,
        &opcoes_trancado(),
        &pasta.registros(),
        |_| {
            chamadas += 1;
            let hash = format!("{chamadas:064}");
            acrescentar_ao_registro(
                &pasta.registros(),
                uso_alheio(
                    &format!("outro{chamadas}_v1"),
                    &hash,
                    "2026-10-06T11:00:00Z",
                ),
            );
        },
    )
    .expect_err("registro mudando a cada tentativa");

    assert!(
        matches!(
            &erro,
            SpiderError::RegistroTrancadoConcorrente { tentativas, .. }
                if *tentativas == TENTATIVAS_REGISTRO
        ),
        "{erro:?}"
    );
    assert_eq!(chamadas, TENTATIVAS_REGISTRO);
    let gravado = registro_lido(&pasta.registros());
    assert_eq!(gravado.len() as u32, TENTATIVAS_REGISTRO);
    assert!(
        gravado.iter().all(|a| a.perfil != NOME),
        "o uso desta execução não foi gravado; os das outras ficaram"
    );
}

// ---------------------------------------------------------------------------
// 5. Bonferroni na busca.
// ---------------------------------------------------------------------------

#[test]
fn busca_com_testados_tres_corrige_os_p_valores() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    let pasta = Pasta::nova("bonferroni_busca");
    let opcoes = OpcoesAvaliacao {
        testados: Some(3),
        ..opcoes_busca()
    };

    let r = avaliar(&mut conn, &cfg, &p, &opcoes, &pasta.registros()).expect("busca");

    let c = r.bonferroni.as_ref().expect("com --testados");
    let b = r.veredito.bootstrap.as_ref().expect("bootstrap");
    assert_eq!(c.comparacoes, 3);
    let p_log_loss = (3.0 * b.log_loss.p_valor).min(1.0);
    assert_eq!(c.p_log_loss, Some(p_log_loss));
    let p_topk = r.veredito.avaliacao.topk.p_valor.probabilidade;
    assert_eq!(c.p_topk.probabilidade, (3.0 * p_topk).min(1.0));
    assert_eq!(c.supera_o_acaso, Some(p_log_loss <= ALFA));
    assert!(c.p_topk.log10 <= 0.0, "a cauda corrigida nunca passa de 1");
    assert_eq!(entradas(&pasta.0), 0);
}

#[test]
fn busca_com_testados_um_nao_muda_os_p_valores() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    let pasta = Pasta::nova("bonferroni_um");
    let opcoes = OpcoesAvaliacao {
        testados: Some(1),
        ..opcoes_busca()
    };

    let r = avaliar(&mut conn, &cfg, &p, &opcoes, &pasta.registros()).expect("busca");

    let c = r.bonferroni.as_ref().expect("com --testados 1");
    let b = r.veredito.bootstrap.as_ref().expect("bootstrap");
    assert_eq!(c.comparacoes, 1);
    assert_eq!(c.p_log_loss, Some(b.log_loss.p_valor));
    assert_eq!(c.supera_o_acaso, r.veredito.supera_o_acaso());
}

#[test]
fn busca_com_testados_grande_limita_o_p_corrigido_a_um() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    let pasta = Pasta::nova("bonferroni_grande");
    let opcoes = OpcoesAvaliacao {
        testados: Some(1_000_000),
        ..opcoes_busca()
    };

    let r = avaliar(&mut conn, &cfg, &p, &opcoes, &pasta.registros()).expect("busca");

    // O p do log loss (sorteio aleatório) é ordens de grandeza maior que 1e-6.
    let c = r.bonferroni.as_ref().expect("bonferroni");
    assert_eq!(c.p_log_loss, Some(1.0));
    assert_eq!(c.supera_o_acaso, Some(false));
    assert!(c.p_topk.probabilidade <= 1.0);
}

#[test]
fn testados_nao_muda_o_trecho_nem_o_veredito() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    let pasta = Pasta::nova("bonferroni_veredito");
    let opcoes = OpcoesAvaliacao {
        testados: Some(3),
        ..opcoes_busca()
    };

    let sem = busca(&mut conn, &cfg, &p);
    let com = avaliar(&mut conn, &cfg, &p, &opcoes, &pasta.registros()).expect("busca");

    assert_eq!(com.trecho, sem.trecho);
    assert_eq!(com.veredito, sem.veredito);
}

// ---------------------------------------------------------------------------
// 6. Opção k.
// ---------------------------------------------------------------------------

#[test]
fn k_cinco_muda_o_topk_e_nao_muda_o_log_loss() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 330, SEMENTE), &p);
    let pasta = Pasta::nova("k_cinco");
    let opcoes = OpcoesAvaliacao {
        k: Some(5),
        ..opcoes_busca()
    };

    let padrao = busca(&mut conn, &cfg, &p);
    let r = avaliar(&mut conn, &cfg, &p, &opcoes, &pasta.registros()).expect("busca com k = 5");

    assert_eq!(r.k, 5);
    let topk = &r.veredito.avaliacao.topk;
    assert_eq!(topk.k, 5);
    // Acertos esperados ao acaso: k·K/N = 5·15/25 = 3,0 por concurso.
    assert!(
        (topk.acertos.acaso - 3.0).abs() < 1e-9,
        "acaso = {}",
        topk.acertos.acaso
    );
    assert_ne!(topk, &padrao.veredito.avaliacao.topk);
    for dobra in &walk_forward_de(&r).dobras {
        assert_eq!(dobra.topk.k, 5);
    }
    // Log loss, Brier e bootstrap não dependem de k.
    assert_eq!(
        r.veredito.avaliacao.metricas,
        padrao.veredito.avaliacao.metricas
    );
    assert_eq!(r.veredito.bootstrap, padrao.veredito.bootstrap);
    let metricas = |r: &RelatorioAvaliacao| {
        walk_forward_de(r)
            .dobras
            .iter()
            .map(|d| d.metricas.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(metricas(&r), metricas(&padrao));
}

#[test]
fn k_padrao_espera_quinze_acertos_de_quinze_dezenas_e_nove_ao_acaso() {
    // k = K = 15: k·K/N = 15·15/25 = 9,0 acertos por concurso ao acaso.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let mut conn = preparado(&cfg, &historico(&cfg, 120, SEMENTE), &p);

    let r = busca(&mut conn, &cfg, &p);

    let acaso = r.veredito.avaliacao.topk.acertos.acaso;
    assert!((acaso - 9.0).abs() < 1e-9, "acaso = {acaso}");
}

// ---------------------------------------------------------------------------
// 7. Ponto no tempo no nível do banco.
// ---------------------------------------------------------------------------

/// Cópia do histórico com `concursos` trocados por outras dezenas.
fn com_concursos_trocados(
    cfg: &LoteriaConfig,
    hist: &[Resultado],
    concursos: std::ops::RangeInclusive<u32>,
) -> Vec<Resultado> {
    hist.iter()
        .map(|r| {
            if concursos.contains(&r.concurso) {
                com_dezenas_trocadas(cfg, r)
            } else {
                r.clone()
            }
        })
        .collect()
}

#[test]
fn alterar_resultados_acima_do_corte_nao_altera_o_relatorio_da_busca() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let original = historico(&cfg, 330, SEMENTE);
    // Concursos 250..=270 (todos acima do corte 200) com outras dezenas.
    let alterado = com_concursos_trocados(&cfg, &original, 250..=270);
    assert_ne!(original, alterado);
    let mut a = preparado(&cfg, &original, &p);
    let mut b = preparado(&cfg, &alterado, &p);

    let ra = busca(&mut a, &cfg, &p);
    let rb = busca(&mut b, &cfg, &p);

    assert_eq!(ra.trecho, rb.trecho);
    assert_eq!(ra.veredito, rb.veredito);
    assert_eq!(ra, rb);
}

#[test]
fn alterar_o_ultimo_concurso_nao_altera_o_relatorio_da_busca() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let original = historico(&cfg, 330, SEMENTE);
    let alterado = com_concursos_trocados(&cfg, &original, 330..=330);
    let mut a = preparado(&cfg, &original, &p);
    let mut b = preparado(&cfg, &alterado, &p);

    assert_eq!(busca(&mut a, &cfg, &p), busca(&mut b, &cfg, &p));
}

#[test]
fn alterar_resultados_acima_do_corte_altera_o_relatorio_do_trancado() {
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let original = historico(&cfg, 330, SEMENTE);
    let alterado = com_concursos_trocados(&cfg, &original, 250..=270);
    let mut a = preparado(&cfg, &original, &p);
    let mut b = preparado(&cfg, &alterado, &p);
    let pasta_a = Pasta::nova("tempo_trancado_a");
    let pasta_b = Pasta::nova("tempo_trancado_b");

    let ra = avaliar(&mut a, &cfg, &p, &opcoes_trancado(), &pasta_a.registros()).expect("a");
    let rb = avaliar(&mut b, &cfg, &p, &opcoes_trancado(), &pasta_b.registros()).expect("b");

    assert_ne!(ra.veredito.avaliacao, rb.veredito.avaliacao);
    // As faixas são as mesmas: só o conteúdo mudou.
    assert_eq!(ra.veredito.avaliacao.faixa, rb.veredito.avaliacao.faixa);
}

#[test]
fn alterar_resultados_dentro_da_busca_altera_o_relatorio_da_busca() {
    // Controle dos testes acima: a comparação tem poder para ver diferença.
    let cfg = cfg_corte(CORTE_BAIXO);
    let p = perfil_teste();
    let original = historico(&cfg, 330, SEMENTE);
    let alterado = com_concursos_trocados(&cfg, &original, 100..=120);
    let mut a = preparado(&cfg, &original, &p);
    let mut b = preparado(&cfg, &alterado, &p);

    let ra = busca(&mut a, &cfg, &p);
    let rb = busca(&mut b, &cfg, &p);

    assert_ne!(ra.veredito, rb.veredito);
}

// ---------------------------------------------------------------------------
// 8. testar_aleatoriedade_do_banco.
// ---------------------------------------------------------------------------

#[test]
fn aleatoriedade_de_banco_vazio_e_sem_resultados() {
    let conn = banco();

    let erro = testar_aleatoriedade_do_banco(&conn, lotofacil()).expect_err("banco vazio");

    assert!(
        matches!(&erro, SpiderError::SemResultados(l) if l == "lotofacil"),
        "{erro:?}"
    );
}

#[test]
fn aleatoriedade_do_banco_e_igual_a_funcao_pura_sobre_o_historico() {
    let cfg = lotofacil();
    let hist = historico(cfg, 300, SEMENTE);
    let mut conn = banco();
    gravar(&mut conn, &hist);

    let do_banco = testar_aleatoriedade_do_banco(&conn, cfg).expect("testes do banco");

    let puro = avaliacao::testar_aleatoriedade(cfg, &hist).expect("testes puros");
    assert_eq!(do_banco, puro);
    assert_eq!(do_banco.concursos, 300);
    assert_eq!(
        (do_banco.primeiro_concurso, do_banco.ultimo_concurso),
        (1, 300)
    );
}

#[test]
fn aleatoriedade_nao_grava_nada() {
    let cfg = lotofacil();
    let mut conn = banco();
    gravar(&mut conn, &historico(cfg, 300, SEMENTE));
    let antes = foto(&conn);

    testar_aleatoriedade_do_banco(&conn, cfg).expect("testes do banco");

    assert!(foto(&conn) == antes);
}

#[test]
fn aleatoriedade_com_buraco_e_entrada_invalida() {
    let cfg = lotofacil();
    let hist: Vec<Resultado> = historico(cfg, 120, SEMENTE)
        .into_iter()
        .filter(|r| r.concurso != 60)
        .collect();
    let mut conn = banco();
    gravar(&mut conn, &hist);
    let antes = foto(&conn);

    let erro = testar_aleatoriedade_do_banco(&conn, cfg).expect_err("buraco");

    assert!(
        matches!(erro, SpiderError::AleatoriedadeEntradaInvalida(_)),
        "{erro:?}"
    );
    assert!(foto(&conn) == antes);
}

// ---------------------------------------------------------------------------
// 9. CLI: binário `sync`, com banco, perfis e registros temporários.
// ---------------------------------------------------------------------------

/// Pasta de trabalho de um teste de CLI.
struct Ambiente {
    pasta: Pasta,
    db: PathBuf,
    perfis: PathBuf,
    registros: PathBuf,
    texto_do_perfil: String,
}

/// Banco em arquivo (config real da Lotofácil, corte 3416) com `n`
/// concursos sintéticos, features e, se pedido, o índice de `teste_v1`; o
/// `.toml` do perfil tem o mesmo texto do registro.
fn ambiente(rotulo: &str, n: u32, com_indice: bool) -> Ambiente {
    let pasta = Pasta::nova(rotulo);
    let cfg = lotofacil();
    let texto = texto_perfil(NOME, "lotofacil", 10, Normalizacao::Minmax, 1.0);
    let db_caminho = pasta.0.join("resultados_lotofacil.db");
    {
        let mut conn = db::abrir(&db_caminho).expect("banco em arquivo");
        gravar(&mut conn, &historico(cfg, n, SEMENTE));
        sincronizar_features(&mut conn, cfg, false).expect("features");
        if com_indice {
            indexar(&mut conn, cfg, &carregar_texto(&texto, "lotofacil", NOME));
        }
    }
    let perfis = pasta.0.join("perfis");
    std::fs::create_dir_all(perfis.join("lotofacil")).expect("pasta de perfis");
    std::fs::write(
        perfis.join("lotofacil").join(format!("{NOME}.toml")),
        &texto,
    )
    .expect("perfil gravado");
    let registros = pasta.registros();
    Ambiente {
        pasta,
        db: db_caminho,
        perfis,
        registros,
        texto_do_perfil: texto,
    }
}

struct Saida {
    sucesso: bool,
    codigo: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Saida {
    fn linha_que_comeca_com(&self, prefixo: &str) -> Vec<&str> {
        self.stdout
            .lines()
            .filter(|l| l.starts_with(prefixo))
            .collect()
    }
}

fn rodar_sync(amb: &Ambiente, args: &[OsString]) -> Saida {
    rodar_sync_em(&amb.pasta.0, args)
}

/// Roda o `sync` com `dir` como diretório de trabalho.
fn rodar_sync_em(dir: &Path, args: &[OsString]) -> Saida {
    let saida = Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado");
    para_saida(saida)
}

fn para_saida(saida: std::process::Output) -> Saida {
    Saida {
        sucesso: saida.status.success(),
        codigo: saida.status.code(),
        stdout: String::from_utf8_lossy(&saida.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&saida.stderr).into_owned(),
    }
}

/// `sync avaliar lotofacil --perfil teste_v1 --db ... --perfis ...` **sem**
/// `--registros` (a pasta padrão, ao lado do banco), mais as opções extras.
fn avaliar_cli_sem_registros(amb: &Ambiente, extras: &[&str]) -> Vec<OsString> {
    let mut args = texto_args(&["avaliar", "lotofacil", "--perfil", NOME]);
    for (opcao, caminho) in [("--db", &amb.db), ("--perfis", &amb.perfis)] {
        args.push(opcao.into());
        args.push(caminho.into());
    }
    args.extend(texto_args(extras));
    args
}

fn texto_args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// `sync avaliar lotofacil --perfil teste_v1 --db ... --perfis ... --registros
/// ...` mais as opções extras.
fn avaliar_cli(amb: &Ambiente, extras: &[&str]) -> Saida {
    let mut args = texto_args(&["avaliar", "lotofacil", "--perfil", NOME]);
    for (opcao, caminho) in [
        ("--db", &amb.db),
        ("--perfis", &amb.perfis),
        ("--registros", &amb.registros),
    ] {
        args.push(opcao.into());
        args.push(caminho.into());
    }
    args.extend(texto_args(extras));
    rodar_sync(amb, &args)
}

fn aleatoriedade_cli(amb: &Ambiente, extras: &[&str]) -> Saida {
    let mut args = texto_args(&["aleatoriedade"]);
    args.extend(texto_args(extras));
    rodar_sync(amb, &args)
}

#[test]
fn cli_avaliar_busca_imprime_o_relatorio_completo() {
    let amb = ambiente("cli_busca", 300, true);
    let banco_antes = std::fs::read(&amb.db).expect("banco lido");

    let saida = avaliar_cli(&amb, &[]);

    assert!(saida.sucesso, "stderr: {}", saida.stderr);
    let hash = perfil::hash_toml(&amb.texto_do_perfil);
    // Cabeçalho.
    assert!(
        saida
            .stdout
            .contains("Avaliação de Lotofácil (lotofacil) com o perfil teste_v1"),
        "{}",
        saida.stdout
    );
    assert!(saida.stdout.contains(&format!("Hash do perfil: {hash}")));
    assert!(saida.stdout.contains("Corte da busca: concurso 3416"));
    // Cada tabela (log loss, Brier e top-k) tem 5 dobras e a linha "total".
    for rotulo in ["1", "2", "3", "4", "5", "total"] {
        let prefixo = format!("  {rotulo:>8}  ");
        assert_eq!(
            saida.linha_que_comeca_com(&prefixo).len(),
            3,
            "linhas da dobra `{rotulo}`:\n{}",
            saida.stdout
        );
    }
    // Veredito.
    assert!(saida.stdout.contains("Bootstrap circular"));
    assert!(saida
        .stdout
        .contains("só valem se o perfil e o k foram escolhidos"));
    let conclusoes = saida.linha_que_comeca_com("=> ");
    assert_eq!(conclusoes.len(), 1, "{}", saida.stdout);
    assert!(
        conclusoes[0].contains("supera o acaso"),
        "{}",
        conclusoes[0]
    );
    assert!(saida.stderr.contains("Tempo:"), "{}", saida.stderr);
    // Nada criado nem alterado.
    assert!(!amb.registros.exists(), "a busca não cria registros/");
    assert_eq!(std::fs::read(&amb.db).expect("banco lido"), banco_antes);
}

#[test]
fn cli_avaliar_busca_e_deterministica_na_saida_padrao() {
    let amb = ambiente("cli_deterministico", 300, true);

    let a = avaliar_cli(&amb, &[]);
    let b = avaliar_cli(&amb, &[]);

    assert!(a.sucesso && b.sucesso);
    assert_eq!(a.stdout, b.stdout);
}

#[test]
fn cli_avaliar_com_testados_mostra_bonferroni() {
    let amb = ambiente("cli_testados", 300, true);

    let saida = avaliar_cli(&amb, &["--testados", "3"]);

    assert!(saida.sucesso, "stderr: {}", saida.stderr);
    assert!(
        saida.stdout.contains("Bonferroni, N = 3"),
        "{}",
        saida.stdout
    );
    assert!(saida.stdout.contains("p corrigido ="), "{}", saida.stdout);
    assert!(!amb.registros.exists());
}

#[test]
fn cli_avaliar_com_k_mostra_o_k_escolhido() {
    let amb = ambiente("cli_k", 300, true);

    let saida = avaliar_cli(&amb, &["--k", "5"]);

    assert!(saida.sucesso, "stderr: {}", saida.stderr);
    assert!(saida.stdout.contains("Top-k: k = 5."), "{}", saida.stdout);
    assert!(
        !saida.stdout.contains("padrão: K da loteria"),
        "k = 5 não é o padrão"
    );
}

#[test]
fn cli_avaliar_com_k_invalido_falha() {
    let amb = ambiente("cli_k_invalido", 300, true);

    let saida = avaliar_cli(&amb, &["--k", "25"]);

    assert!(!saida.sucesso);
    assert!(!saida.stderr.is_empty());
    assert!(!amb.registros.exists());
}

#[test]
fn cli_avaliar_todas_falha_com_mensagem() {
    let amb = ambiente("cli_todas", 120, true);

    let saida = rodar_sync(&amb, &texto_args(&["avaliar", "todas", "--perfil", NOME]));

    assert!(!saida.sucesso);
    assert!(saida.stderr.contains("avaliar"), "{}", saida.stderr);
    assert!(saida.stderr.contains("todas"), "{}", saida.stderr);
    assert!(!amb.registros.exists());
}

#[test]
fn cli_repetir_sem_trancado_e_recusado_pelo_clap() {
    let amb = ambiente("cli_repetir_sem_trancado", 120, true);

    let saida = avaliar_cli(&amb, &["--repetir"]);

    assert!(!saida.sucesso);
    // O clap sai com código 2 em erro de uso.
    assert_eq!(saida.codigo, Some(2), "{}", saida.stderr);
    assert!(saida.stderr.contains("--trancado"), "{}", saida.stderr);
}

#[test]
fn cli_testados_com_trancado_e_recusado_pelo_clap() {
    let amb = ambiente("cli_testados_trancado", 120, true);

    let saida = avaliar_cli(&amb, &["--trancado", "--testados", "3"]);

    assert!(!saida.sucesso);
    assert_eq!(saida.codigo, Some(2), "{}", saida.stderr);
    assert!(!amb.registros.exists());
}

#[test]
fn cli_k_com_trancado_e_recusado_pelo_clap() {
    let amb = ambiente("cli_k_trancado", 120, true);

    let saida = avaliar_cli(&amb, &["--trancado", "--k", "5"]);

    assert!(!saida.sucesso);
    assert_eq!(saida.codigo, Some(2), "{}", saida.stderr);
    assert!(!amb.registros.exists());
}

#[test]
fn cli_avaliar_perfil_nao_registrado_manda_rodar_sync_indice() {
    let amb = ambiente("cli_nao_registrado", 120, false);

    let saida = avaliar_cli(&amb, &[]);

    assert!(!saida.sucesso);
    assert!(saida.stderr.contains("sync indice"), "{}", saida.stderr);
    assert!(!amb.registros.exists());
}

#[test]
fn cli_avaliar_banco_inexistente_falha_sem_criar_arquivo() {
    let amb = ambiente("cli_sem_banco", 120, true);
    std::fs::remove_file(&amb.db).expect("banco removido");

    let saida = avaliar_cli(&amb, &[]);

    assert!(!saida.sucesso);
    assert!(
        saida.stderr.contains("banco não encontrado"),
        "{}",
        saida.stderr
    );
    assert!(!amb.db.exists(), "o banco não pode ser criado");
    assert!(!amb.registros.exists());
}

/// Histórico de 3430 concursos: o config real tem corte 3416, então o
/// trancado vai do 3417 ao 3430 (14 concursos, abaixo dos 100 do bootstrap).
const CONCURSOS_PARA_O_TRANCADO_REAL: u32 = 3430;

#[test]
fn cli_trancado_primeiro_uso_segundo_recusado_e_repetir() {
    let amb = ambiente("cli_trancado", CONCURSOS_PARA_O_TRANCADO_REAL, true);
    let arquivo = arquivo_do_registro(&amb.registros);
    let hash = perfil::hash_toml(&amb.texto_do_perfil);

    // Primeiro uso: avalia e registra.
    let primeira = avaliar_cli(&amb, &["--trancado"]);
    assert!(primeira.sucesso, "stderr: {}", primeira.stderr);
    assert!(primeira.stdout.contains("ATENÇÃO"), "{}", primeira.stdout);
    assert!(primeira.stdout.contains("TRANCADO"), "{}", primeira.stdout);
    // 14 concursos no trancado: sem bootstrap, sem veredito.
    assert!(
        primeira.stdout.contains("SEM VEREDITO"),
        "{}",
        primeira.stdout
    );
    assert!(
        primeira
            .stdout
            .contains("Registro do trancado: uso gravado em"),
        "{}",
        primeira.stdout
    );
    let registro = registros::carregar(&arquivo, "lotofacil").expect("registro lido");
    assert_eq!(registro.len(), 1);
    assert_eq!(registro[0].perfil, NOME);
    assert_eq!(registro[0].hash_toml, hash);
    assert_eq!(registro[0].ate_concurso, CONCURSOS_PARA_O_TRANCADO_REAL);
    // ISO-8601 UTC com segundos: AAAA-MM-DDThh:mm:ssZ.
    assert_eq!(
        registro[0].avaliado_em.len(),
        20,
        "{}",
        registro[0].avaliado_em
    );
    assert!(registro[0].avaliado_em.ends_with('Z'));
    let bytes = std::fs::read(&arquivo).expect("registro lido");

    // Segundo uso do mesmo perfil: recusado, sem mexer no arquivo.
    let segunda = avaliar_cli(&amb, &["--trancado"]);
    assert!(!segunda.sucesso);
    assert!(segunda.stderr.contains("--repetir"), "{}", segunda.stderr);
    assert_eq!(std::fs::read(&arquivo).expect("registro lido"), bytes);

    // Com --repetir: avalia de novo e registra as duas.
    let terceira = avaliar_cli(&amb, &["--trancado", "--repetir"]);
    assert!(terceira.sucesso, "stderr: {}", terceira.stderr);
    assert!(
        terceira
            .stdout
            .contains("Avaliações anteriores deste perfil no trancado"),
        "{}",
        terceira.stdout
    );
    assert_eq!(
        registros::carregar(&arquivo, "lotofacil")
            .expect("registro lido")
            .len(),
        2
    );
}

/// A linha do cabeçalho do relatório do trancado com o registro lido.
fn linha_do_registro_lido(saida: &Saida) -> &str {
    let linhas = saida.linha_que_comeca_com("  Registro do trancado lido: ");
    assert_eq!(linhas.len(), 1, "{}", saida.stdout);
    linhas[0]
}

#[test]
fn cli_trancado_imprime_sempre_o_registro_lido_com_caminho_absoluto_e_usos() {
    // Pasta de registros relativa ao diretório de trabalho (o temporário do
    // ambiente): a saída mostra sempre o caminho absoluto e quantos usos o
    // arquivo tinha, em toda execução com --trancado (primeira, recusa e
    // --repetir), e não só no primeiro registro ou fora do padrão.
    let amb = ambiente("cli_registro_lido", CONCURSOS_PARA_O_TRANCADO_REAL, true);
    let arquivo = arquivo_do_registro(&amb.pasta.0.join("regs_rel"));
    let caminho = arquivo.display().to_string();
    let args = |extras: &[&str]| {
        let mut args = avaliar_cli_sem_registros(&amb, extras);
        args.push("--registros".into());
        args.push("regs_rel".into());
        args
    };

    let primeira = rodar_sync(&amb, &args(&["--trancado"]));
    assert!(primeira.sucesso, "stderr: {}", primeira.stderr);
    assert_eq!(
        linha_do_registro_lido(&primeira),
        format!(
            "  Registro do trancado lido: {caminho}; usos: 0 (arquivo inexistente; é criado com este uso)."
        )
    );
    assert!(arquivo.is_absolute() && arquivo.exists());

    // Recusa do uso único: a mensagem de erro diz o arquivo e os usos.
    let segunda = rodar_sync(&amb, &args(&["--trancado"]));
    assert!(!segunda.sucesso);
    assert!(
        segunda
            .stderr
            .contains(&format!("segundo o registro {caminho} (1 uso(s) no total)")),
        "{}",
        segunda.stderr
    );

    let terceira = rodar_sync(&amb, &args(&["--trancado", "--repetir"]));
    assert!(terceira.sucesso, "stderr: {}", terceira.stderr);
    assert_eq!(
        linha_do_registro_lido(&terceira),
        format!(
            "  Registro do trancado lido: {caminho}; usos: 1 uso(s) registrado(s) antes deste, 1 deste perfil."
        )
    );
    assert_eq!(
        registros::carregar(&arquivo, "lotofacil")
            .expect("lido")
            .len(),
        2
    );

    // A busca não lê registro nenhum e não imprime a linha.
    let busca = rodar_sync(&amb, &args(&[]));
    assert!(busca.sucesso, "stderr: {}", busca.stderr);
    assert!(
        busca
            .linha_que_comeca_com("  Registro do trancado lido")
            .is_empty(),
        "{}",
        busca.stdout
    );
}

#[test]
fn cli_ajuda_do_avaliar_esconde_registros() {
    let amb = ambiente("cli_ajuda", 20, false);

    let saida = rodar_sync(&amb, &texto_args(&["avaliar", "--help"]));

    assert!(saida.sucesso, "stderr: {}", saida.stderr);
    assert!(saida.stdout.contains("--trancado"), "{}", saida.stdout);
    assert!(
        !saida.stdout.contains("--registros"),
        "`--registros` é só para testes: {}",
        saida.stdout
    );
}

#[test]
fn cli_trancado_sem_registros_usa_a_pasta_ao_lado_do_banco_em_qualquer_diretorio() {
    // Roda de um diretório que não é o do banco: o registro vai para
    // `registros/` ao lado do banco, e é achado de novo de outro diretório.
    let amb = ambiente("cli_ancora", CONCURSOS_PARA_O_TRANCADO_REAL, true);
    let outro_dir = Pasta::nova("cli_ancora_outro_dir");
    let terceiro_dir = Pasta::nova("cli_ancora_terceiro_dir");
    let arquivo = arquivo_do_registro(&amb.pasta.0.join("registros"));

    let primeira = rodar_sync_em(
        &outro_dir.0,
        &avaliar_cli_sem_registros(&amb, &["--trancado"]),
    );

    assert!(primeira.sucesso, "stderr: {}", primeira.stderr);
    assert_eq!(
        registros::carregar(&arquivo, "lotofacil")
            .expect("lido")
            .len(),
        1
    );
    assert_eq!(
        entradas(&outro_dir.0),
        0,
        "nada gravado no diretório de trabalho"
    );
    // Primeiro registro: aviso com o caminho absoluto do arquivo.
    let caminho = arquivo.display().to_string();
    assert!(
        primeira.stderr.contains("primeiro registro") && primeira.stderr.contains(&caminho),
        "{}",
        primeira.stderr
    );
    assert!(
        !primeira.stderr.contains("FORA DO PADRÃO"),
        "{}",
        primeira.stderr
    );
    assert!(primeira.stdout.contains(&caminho), "{}", primeira.stdout);

    // De um terceiro diretório, o mesmo perfil é recusado: o registro foi
    // achado.
    let segunda = rodar_sync_em(
        &terceiro_dir.0,
        &avaliar_cli_sem_registros(&amb, &["--trancado"]),
    );
    assert!(!segunda.sucesso);
    assert!(segunda.stderr.contains("--repetir"), "{}", segunda.stderr);
    assert_eq!(entradas(&terceiro_dir.0), 0);

    // Com --repetir: o arquivo já existe e a pasta é a padrão, sem aviso.
    let terceira = rodar_sync_em(
        &terceiro_dir.0,
        &avaliar_cli_sem_registros(&amb, &["--trancado", "--repetir"]),
    );
    assert!(terceira.sucesso, "stderr: {}", terceira.stderr);
    assert!(!terceira.stderr.contains("Aviso:"), "{}", terceira.stderr);
    assert_eq!(
        registros::carregar(&arquivo, "lotofacil")
            .expect("lido")
            .len(),
        2
    );
    assert_eq!(entradas(&terceiro_dir.0), 0);
}

#[test]
fn cli_trancado_com_registros_fora_do_padrao_avisa_com_o_caminho_absoluto() {
    let amb = ambiente("cli_fora_do_padrao", CONCURSOS_PARA_O_TRANCADO_REAL, true);
    let outra = amb.pasta.0.join("outra_pasta");
    let mut args = avaliar_cli_sem_registros(&amb, &["--trancado"]);
    args.push("--registros".into());
    args.push(outra.clone().into());

    let saida = rodar_sync(&amb, &args);

    assert!(saida.sucesso, "stderr: {}", saida.stderr);
    let arquivo = arquivo_do_registro(&outra);
    let padrao = arquivo_do_registro(&amb.pasta.0.join("registros"));
    assert!(saida.stderr.contains("FORA DO PADRÃO"), "{}", saida.stderr);
    assert!(
        saida.stderr.contains(&arquivo.display().to_string())
            && saida.stderr.contains(&padrao.display().to_string()),
        "{}",
        saida.stderr
    );
    assert!(
        saida.stderr.contains("primeiro registro"),
        "{}",
        saida.stderr
    );
    assert!(arquivo.exists());
    assert!(!padrao.exists(), "a pasta padrão não é tocada");
}

#[test]
fn cli_trancado_com_saida_fechada_desde_o_inicio_nao_registra_e_nao_da_panic() {
    // O aviso inicial falha antes da avaliação: o histórico curto basta.
    let amb = ambiente("cli_saida_fechada", 120, true);
    let mut filho = Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(&amb.pasta.0)
        .args(avaliar_cli_sem_registros(&amb, &["--trancado"]))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("binário sync iniciado");
    drop(filho.stdout.take());

    let saida = para_saida(filho.wait_with_output().expect("sync terminou"));

    // 101 é o código de saída de panic.
    assert_eq!(saida.codigo, Some(1), "stderr: {}", saida.stderr);
    assert!(!saida.stderr.contains("panicked"), "{}", saida.stderr);
    assert!(!amb.registros.exists(), "nada avaliado, nada registrado");
}

#[test]
fn cli_trancado_com_saida_fechada_depois_do_aviso_registra_e_mostra_o_relatorio_no_erro() {
    // Como `| head`: quem lê fecha a saída depois do aviso inicial. O uso é
    // gravado (a avaliação deu certo) e o relatório, já montado, vai para a
    // saída de erro, sem panic.
    use std::io::Read;
    let amb = ambiente(
        "cli_saida_fechada_depois",
        CONCURSOS_PARA_O_TRANCADO_REAL,
        true,
    );
    let mut filho = Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(&amb.pasta.0)
        .args(avaliar_cli_sem_registros(&amb, &["--trancado"]))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("binário sync iniciado");
    let mut stdout = filho.stdout.take().expect("stdout");
    let mut lido = Vec::new();
    let mut byte = [0u8; 1];
    while !lido.ends_with(b"\n\n") {
        let n = stdout.read(&mut byte).expect("aviso lido");
        assert_eq!(n, 1, "a saída acabou antes do aviso: {lido:?}");
        lido.push(byte[0]);
    }
    drop(stdout);

    let saida = para_saida(filho.wait_with_output().expect("sync terminou"));

    assert!(String::from_utf8_lossy(&lido).starts_with("ATENÇÃO"));
    assert_eq!(saida.codigo, Some(1), "stderr: {}", saida.stderr);
    assert!(!saida.stderr.contains("panicked"), "{}", saida.stderr);
    assert!(
        saida.stderr.contains("JÁ foi registrado"),
        "{}",
        saida.stderr
    );
    assert!(
        saida.stderr.contains("Veredito no trecho inteiro"),
        "o relatório não se perde: {}",
        saida.stderr
    );
    let registro = registros::carregar(&arquivo_do_registro(&amb.registros), "lotofacil")
        .expect("registro lido");
    assert_eq!(registro.len(), 1);
}

// ---------------------------------------------------------------------------
// 10. CLI: aleatoriedade.
// ---------------------------------------------------------------------------

#[test]
fn cli_aleatoriedade_imprime_qui_quadrado_e_runs_test_sem_mexer_no_banco() {
    // Sem features nem índice: o teste só usa `resultados`.
    let amb = ambiente("cli_aleatoriedade", 300, false);
    let banco_antes = std::fs::read(&amb.db).expect("banco lido");
    let db_arg = amb.db.to_string_lossy().into_owned();

    let saida = aleatoriedade_cli(&amb, &["lotofacil", "--db", &db_arg]);

    assert!(saida.sucesso, "stderr: {}", saida.stderr);
    assert!(saida.stdout.contains("Qui-quadrado"), "{}", saida.stdout);
    assert!(saida.stdout.contains("Runs test"), "{}", saida.stdout);
    assert!(
        saida
            .stdout
            .contains("Testes de aleatoriedade de Lotofácil (lotofacil)"),
        "{}",
        saida.stdout
    );
    assert_eq!(std::fs::read(&amb.db).expect("banco lido"), banco_antes);
}

#[test]
fn cli_aleatoriedade_banco_inexistente_falha_sem_criar_arquivo() {
    let amb = ambiente("cli_aleatoriedade_sem_banco", 120, false);
    let inexistente = amb.pasta.0.join("nao_existe.db");
    let db_arg = inexistente.to_string_lossy().into_owned();

    let saida = aleatoriedade_cli(&amb, &["lotofacil", "--db", &db_arg]);

    assert!(!saida.sucesso);
    assert!(
        saida.stderr.contains("banco não encontrado"),
        "{}",
        saida.stderr
    );
    assert!(!inexistente.exists(), "o banco não pode ser criado");
}

#[test]
fn cli_aleatoriedade_todas_com_db_falha() {
    let amb = ambiente("cli_aleatoriedade_todas", 120, false);
    let db_arg = amb.db.to_string_lossy().into_owned();

    let saida = aleatoriedade_cli(&amb, &["todas", "--db", &db_arg]);

    assert!(!saida.sucesso);
    assert!(!saida.stderr.is_empty());
    assert!(
        saida.stderr.contains("--db"),
        "a mensagem cita a opção recusada: {}",
        saida.stderr
    );
}
