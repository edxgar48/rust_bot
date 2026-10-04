use clap::{Parser, Subcommand};
use rust_spider::{
    config, correcoes, db, downloader, manuais, orquestracao, parser, validacao, SpiderError,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Baixa e processa resultados de loterias da Caixa (planilha .xlsx -> SQLite/JSON)"
)]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Navega até o site oficial, baixa a planilha (.xlsx) de resultados e já grava no banco.
    Baixar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Pasta onde salvar a planilha (.xlsx) baixada
        #[arg(long, default_value = "downloads")]
        dir: PathBuf,
        /// Caminho do banco SQLite. Por padrão, um arquivo por loteria
        /// (resultados_<loteria>.db), para não misturar dados de loterias diferentes.
        /// Não pode ser usado com `todas`.
        #[arg(long)]
        db: Option<PathBuf>,
        /// Sobrescreve o seletor CSS do botão de download configurado para essa
        /// loteria. Não pode ser usado com `todas`.
        #[arg(long)]
        seletor: Option<String>,
        /// Mostra a janela do navegador (útil para depurar bloqueios/seletores)
        #[arg(long)]
        visivel: bool,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
    },
    /// Processa uma planilha (.xlsx) de resultados já baixada manualmente, sem abrir navegador.
    Importar {
        /// Slug da loteria (define como interpretar as colunas da tabela);
        /// não aceita `todas`, pois o arquivo é de uma loteria só
        loteria: String,
        /// Caminho da planilha (.xlsx) de resultados
        arquivo: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
    },
    /// Lista os resultados já salvos no banco, em JSON.
    Listar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Lê o último resultado direto da página (sem baixar a planilha inteira)
    /// e grava no banco só se for um concurso novo.
    Atualizar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
        /// Mostra a janela do navegador (útil para depurar bloqueios/seletores)
        #[arg(long)]
        visivel: bool,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
    },
    /// Confere a integridade dos resultados salvos: quantidade de dezenas,
    /// faixa, repetidas, datas, buracos na numeração e datas fora de ordem.
    /// Sai com erro se encontrar algum erro; datas fora de ordem são só avisos.
    Validar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Lista os concursos que faltam no banco, com as datas dos vizinhos,
    /// para fechá-los um a um com `inserir`.
    Buracos {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Insere à mão um concurso que falta no banco (fecha um buraco) e o
    /// registra no cache de inserções manuais, em ordem de data de sorteio.
    Inserir {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Número do concurso que falta
        concurso: u32,
        /// Data do sorteio, no formato AAAA-MM-DD
        #[arg(long)]
        data: String,
        /// Dezenas sorteadas, separadas por vírgula (ex.: 1,5,10,22,40)
        #[arg(long)]
        dezenas: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
    },
    /// Calcula as features de cada dezena e janela (série temporal) e grava em
    /// `features_dezena`, retomando do último concurso já calculado. Recusa
    /// rodar se o `validar` apontar erro (buraco incluso).
    Features {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Ignora o estado salvo e recalcula todas as features do zero
        #[arg(long)]
        completo: bool,
    },
}

/// Quantos problemas imprimir antes de resumir o restante numa contagem.
const LIMITE_PROBLEMAS_IMPRESSOS: usize = 50;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.comando {
        Comando::Baixar {
            loteria,
            dir,
            db,
            seletor,
            visivel,
            cache,
        } => {
            let alvos = resolver_alvos(&loteria, db, seletor.as_deref())?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                let r = baixar(cfg, &dir, &db_caminho, seletor.as_deref(), visivel, &cache).await;
                execucao.registrar(cfg, r)?;
            }
            execucao.concluir()?;
        }
        Comando::Importar {
            loteria,
            arquivo,
            db,
            cache,
        } => {
            let cfg = buscar_config_unica(&loteria, "importar", "o arquivo é de uma loteria só")?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            processar_e_salvar(&arquivo, cfg, &db_caminho, &cache)?;
        }
        Comando::Listar { loteria, db } => {
            let cfg = buscar_config_unica(
                &loteria,
                "listar",
                "liste uma loteria por vez (cada uma tem seu banco)",
            )?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let resultados = listar(cfg, &db_caminho)?;
            println!("{}", serde_json::to_string_pretty(&resultados)?);
        }
        Comando::Atualizar {
            loteria,
            db,
            visivel,
            cache,
        } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                let r = atualizar(cfg, &db_caminho, visivel, &cache).await;
                execucao.registrar(cfg, r)?;
            }
            execucao.concluir()?;
        }
        Comando::Validar { loteria, db } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                execucao.registrar(cfg, validar_banco(cfg, &db_caminho))?;
            }
            execucao.concluir()?;
        }
        Comando::Buracos { loteria, db } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                execucao.registrar(cfg, listar_buracos(cfg, &db_caminho))?;
            }
            execucao.concluir()?;
        }
        Comando::Inserir {
            loteria,
            concurso,
            data,
            dezenas,
            db,
            cache,
        } => {
            let cfg = buscar_config_unica(&loteria, "inserir", "as dezenas são de uma loteria só")?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let entrada = manuais::EntradaManual {
                concurso,
                data_sorteio: data,
                dezenas: manuais::parsear_dezenas(&dezenas)?,
            };
            inserir_manual(cfg, entrada, &db_caminho, &cache)?;
        }
        Comando::Features {
            loteria,
            db,
            completo,
        } => {
            let cfg = buscar_config_unica(
                &loteria,
                "features",
                "calcule uma loteria por vez (cada uma tem seu banco)",
            )?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            calcular_features(cfg, &db_caminho, completo)?;
        }
    }

    Ok(())
}

/// Loterias a processar por um comando, cada uma com seu banco.
struct Alvos {
    /// `true` quando o argumento foi `todas`: decide cabeçalhos e resumo.
    multiplas: bool,
    itens: Vec<(&'static config::LoteriaConfig, PathBuf)>,
}

impl Alvos {
    fn execucao(&self) -> Execucao {
        Execucao::nova(self.multiplas)
    }
}

/// Resolve o argumento `loteria` (slug ou `todas`) nas loterias a processar.
/// Concentra as recusas de opções que só valem com slug único: `--db` (um
/// banco por loteria) e `--seletor` (o seletor é de uma loteria só; só
/// `baixar` o recebe, os demais comandos passam `None`).
fn resolver_alvos(
    loteria: &str,
    db: Option<PathBuf>,
    seletor: Option<&str>,
) -> anyhow::Result<Alvos> {
    let cfgs = config::resolver(loteria).ok_or_else(|| loteria_desconhecida(loteria, true))?;
    let multiplas = config::eh_todas(loteria);
    if multiplas && db.is_some() {
        anyhow::bail!(
            "`--db` não pode ser usado com `{}`: cada loteria tem seu próprio banco \
             (resultados_<loteria>.db); informe o slug para usar outro caminho",
            config::TODAS
        );
    }
    if multiplas && seletor.is_some() {
        anyhow::bail!(
            "`--seletor` não pode ser usado com `{}`: o seletor é de uma \
             loteria só; informe o slug",
            config::TODAS
        );
    }
    let itens = cfgs
        .into_iter()
        .map(|cfg| {
            let caminho = db.clone().unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            (cfg, caminho)
        })
        .collect();
    Ok(Alvos { multiplas, itens })
}

/// Execução de um comando sobre uma ou mais loterias. Com slug único é
/// transparente (o erro sobe como antes); com `todas`, imprime um cabeçalho
/// por loteria, segue adiante quando uma falha e resume no fim.
struct Execucao {
    multiplas: bool,
    situacoes: Vec<(&'static str, bool)>,
}

impl Execucao {
    fn nova(multiplas: bool) -> Self {
        Execucao {
            multiplas,
            situacoes: Vec::new(),
        }
    }

    fn cabecalho(&self, cfg: &config::LoteriaConfig) {
        if self.multiplas {
            println!("=== {} ({}) ===", cfg.nome_exibicao, cfg.slug);
        }
    }

    fn registrar(
        &mut self,
        cfg: &'static config::LoteriaConfig,
        resultado: anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if !self.multiplas {
            return resultado;
        }
        if let Err(e) = &resultado {
            eprintln!("Erro em {}: {e:#}", cfg.slug);
        }
        self.situacoes.push((cfg.slug, resultado.is_ok()));
        Ok(())
    }

    fn concluir(self) -> anyhow::Result<()> {
        if !self.multiplas {
            return Ok(());
        }
        println!("=== Resumo ===");
        for (slug, ok) in &self.situacoes {
            let situacao = if *ok { "ok" } else { "falhou" };
            println!("  {slug}: {situacao}");
        }
        let falhas: Vec<&str> = self
            .situacoes
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(slug, _)| *slug)
            .collect();
        if !falhas.is_empty() {
            anyhow::bail!(
                "{} de {} loteria(s) falharam: {}",
                falhas.len(),
                self.situacoes.len(),
                falhas.join(", ")
            );
        }
        Ok(())
    }
}

async fn baixar(
    cfg: &'static config::LoteriaConfig,
    dir: &std::path::Path,
    db_caminho: &std::path::Path,
    seletor: Option<&str>,
    visivel: bool,
    cache: &std::path::Path,
) -> anyhow::Result<()> {
    println!("Baixando resultados de {}...", cfg.nome_exibicao);
    let arquivo = downloader::baixar_resultado(cfg, seletor, dir, !visivel).await?;
    println!("Arquivo salvo em {}", arquivo.display());

    processar_e_salvar(&arquivo, cfg, db_caminho, cache)
}

fn listar(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
) -> anyhow::Result<Vec<rust_spider::Resultado>> {
    // `db::abrir` cria o arquivo se não existir; listar não deve criar banco
    // vazio (nem responder `[]` como se a loteria não tivesse concursos).
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let conn = db::abrir(db_caminho)?;
    Ok(db::listar(&conn, cfg.slug)?)
}

async fn atualizar(
    cfg: &'static config::LoteriaConfig,
    db_caminho: &std::path::Path,
    visivel: bool,
    cache: &std::path::Path,
) -> anyhow::Result<()> {
    println!("Verificando o último resultado de {}...", cfg.nome_exibicao);
    let mut resultado = downloader::raspar_ultimo_resultado(cfg, !visivel).await?;
    imprimir_correcoes(&correcoes::aplicar(
        cfg.slug,
        std::slice::from_mut(&mut resultado),
    ));

    let existentes = carregar_existentes(cfg, db_caminho)?;
    let ultimo_salvo = existentes.iter().map(|r| r.concurso).max();

    if let Some(u) = ultimo_salvo {
        if u >= resultado.concurso {
            println!(
                "Já está atualizado: último concurso salvo é {u}, a página mostra {}.",
                resultado.concurso
            );
            // Uma inserção manual do mesmo concurso ainda pode ser conciliada.
            return conciliar_cache(cfg, cache, std::slice::from_ref(&resultado));
        }
    }

    let lote = std::slice::from_ref(&resultado);
    validar_lote(cfg, &existentes, lote, db_caminho)?;

    // Só depois de aprovado: `db::abrir` cria o arquivo se não existir.
    let mut conn = db::abrir(db_caminho)?;
    let gravados = db::salvar_resultados(&mut conn, lote)?;
    println!(
        "Concurso {} ({}) gravado em {} ({gravados} registro(s) novo(s)/atualizado(s)).",
        resultado.concurso,
        resultado.data_sorteio,
        db_caminho.display()
    );
    conciliar_cache(cfg, cache, lote)
}

fn validar_banco(cfg: &config::LoteriaConfig, db_caminho: &std::path::Path) -> anyhow::Result<()> {
    // `db::abrir` cria o arquivo se não existir; validar não deve criar banco vazio.
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let conn = db::abrir(db_caminho)?;
    let resultados = db::listar(&conn, cfg.slug)?;

    println!(
        "Validando {} em {}...",
        cfg.nome_exibicao,
        db_caminho.display()
    );
    let (Some(menor), Some(maior)) = (
        resultados.iter().map(|r| r.concurso).min(),
        resultados.iter().map(|r| r.concurso).max(),
    ) else {
        anyhow::bail!("nenhum concurso de {} no banco", cfg.slug);
    };
    println!(
        "{} concurso(s) salvo(s), do {menor} ao {maior}.",
        resultados.len()
    );

    let problemas = validacao::validar(cfg, &resultados);
    let (erros, avisos): (Vec<_>, Vec<_>) = problemas
        .iter()
        .partition(|p| p.severidade() == validacao::Severidade::Erro);

    imprimir_problemas("aviso(s)", &avisos, Saida::Padrao);
    imprimir_problemas("erro(s)", &erros, Saida::Padrao);
    if erros
        .iter()
        .any(|p| matches!(p, validacao::Problema::ConcursosFaltando { .. }))
    {
        imprimir_dica_buracos(cfg);
    }

    if erros.is_empty() {
        if avisos.is_empty() {
            println!("Nenhum problema encontrado.");
        } else {
            println!("Nenhum erro encontrado ({} aviso(s)).", avisos.len());
        }
        return Ok(());
    }
    anyhow::bail!(
        "{}: {} erro(s) de integridade em {}",
        cfg.slug,
        erros.len(),
        db_caminho.display()
    )
}

/// Onde imprimir: `validar` é um relatório e usa a saída padrão; a recusa de
/// um lote ao gravar é falha do comando e vai para a saída de erro.
#[derive(Clone, Copy)]
enum Saida {
    Padrao,
    Erro,
}

impl Saida {
    fn imprimir(self, linha: &str) {
        match self {
            Saida::Padrao => println!("{linha}"),
            Saida::Erro => eprintln!("{linha}"),
        }
    }
}

fn imprimir_problemas<P: std::fmt::Display>(rotulo: &str, problemas: &[P], saida: Saida) {
    if problemas.is_empty() {
        return;
    }
    let imprimir = |linha: String| saida.imprimir(&linha);
    imprimir(format!("{} {rotulo}:", problemas.len()));
    for p in problemas.iter().take(LIMITE_PROBLEMAS_IMPRESSOS) {
        imprimir(format!("  - {p}"));
    }
    if problemas.len() > LIMITE_PROBLEMAS_IMPRESSOS {
        imprimir(format!(
            "  ... e mais {} não exibido(s).",
            problemas.len() - LIMITE_PROBLEMAS_IMPRESSOS
        ));
    }
}

/// Valida o lote contra o que já está no banco antes de gravar. Imprime os
/// avisos; se houver erro, imprime-os e falha sem gravar nada (tudo ou nada).
fn validar_lote(
    cfg: &config::LoteriaConfig,
    existentes: &[rust_spider::Resultado],
    novos: &[rust_spider::Resultado],
    db_caminho: &std::path::Path,
) -> anyhow::Result<validacao::DecisaoGravacao> {
    let decisao = validacao::avaliar_gravacao(cfg, existentes, novos);
    imprimir_problemas("aviso(s)", &decisao.avisos, Saida::Padrao);
    if decisao
        .avisos
        .iter()
        .any(|p| matches!(p, validacao::Problema::ConcursosFaltando { .. }))
    {
        imprimir_dica_buracos(cfg);
    }
    if !decisao.pode_gravar() {
        imprimir_problemas("erro(s)", &decisao.erros, Saida::Erro);
        anyhow::bail!(
            "{}: lote recusado por {} erro(s) de integridade; nada foi gravado em {}",
            cfg.slug,
            decisao.erros.len(),
            db_caminho.display()
        );
    }
    Ok(decisao)
}

/// O que já está gravado, sem criar o banco: `db::abrir` cria o arquivo, e um
/// lote recusado sobre banco inexistente não deve deixar um `.db` vazio.
fn carregar_existentes(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
) -> anyhow::Result<Vec<rust_spider::Resultado>> {
    if !db_caminho.exists() {
        return Ok(Vec::new());
    }
    let conn = db::abrir(db_caminho)?;
    Ok(db::listar(&conn, cfg.slug)?)
}

/// Para comandos que só fazem sentido com uma loteria (`importar`, `inserir`,
/// `listar`, `features`): recusa `todas` explicando o porquê.
fn buscar_config_unica(
    loteria: &str,
    comando: &str,
    motivo: &str,
) -> anyhow::Result<&'static config::LoteriaConfig> {
    if config::eh_todas(loteria) {
        anyhow::bail!(
            "`{comando}` não aceita `{}`: {motivo}. Informe uma de: {}",
            config::TODAS,
            config::slugs_disponiveis()
        );
    }
    config::buscar(loteria).ok_or_else(|| loteria_desconhecida(loteria, false))
}

fn loteria_desconhecida(loteria: &str, aceita_todas: bool) -> anyhow::Error {
    let todas = if aceita_todas {
        format!(" (ou `{}` para todas)", config::TODAS)
    } else {
        String::new()
    };
    anyhow::anyhow!(
        "loteria desconhecida: `{loteria}`. Use uma de: {}{todas}",
        config::slugs_disponiveis()
    )
}

fn caminho_db_padrao(slug: &str) -> PathBuf {
    PathBuf::from(format!("resultados_{slug}.db"))
}

fn processar_e_salvar(
    arquivo: &std::path::Path,
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    cache_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let mut resultados = parser::parsear_arquivo(arquivo, cfg)?;
    println!("{} concurso(s) extraído(s) do arquivo.", resultados.len());
    imprimir_correcoes(&correcoes::aplicar(cfg.slug, &mut resultados));

    // Concursos inseridos à mão que a planilha ainda não traz entram no mesmo
    // lote: assim um banco recriado do zero recupera os manuais.
    let cache_caminho = manuais::caminho(cache_dir, cfg.slug);
    let entradas = manuais::carregar(&cache_caminho, cfg.slug)?;
    let (restantes, mut conciliacoes) = manuais::conciliar(&entradas, &resultados);
    let existentes = carregar_existentes(cfg, db_caminho)?;
    let (pendentes, difere_do_banco) =
        manuais::pendentes(&restantes, &resultados, &existentes, cfg.slug);
    conciliacoes.extend(difere_do_banco);
    if !pendentes.is_empty() {
        println!(
            "{} concurso(s) inserido(s) à mão reaplicado(s) a partir de {}.",
            pendentes.len(),
            cache_caminho.display()
        );
        resultados.extend(pendentes);
    }

    validar_lote(cfg, &existentes, &resultados, db_caminho)?;
    // Só depois de aprovado: `db::abrir` cria o arquivo se não existir.
    let mut conn = db::abrir(db_caminho)?;
    let gravados = db::salvar_resultados(&mut conn, &resultados)?;
    println!(
        "{gravados} registro(s) novo(s)/atualizado(s) em {}",
        db_caminho.display()
    );

    imprimir_conciliacoes(&conciliacoes);
    if restantes != entradas {
        manuais::salvar(&cache_caminho, cfg.slug, &restantes)?;
    }
    Ok(())
}

/// Depois de gravar dados oficiais: remove do cache as inserções manuais
/// confirmadas e avisa as divergentes.
fn conciliar_cache(
    cfg: &config::LoteriaConfig,
    cache_dir: &std::path::Path,
    oficiais: &[rust_spider::Resultado],
) -> anyhow::Result<()> {
    let cache_caminho = manuais::caminho(cache_dir, cfg.slug);
    let entradas = manuais::carregar(&cache_caminho, cfg.slug)?;
    let (restantes, conciliacoes) = manuais::conciliar(&entradas, oficiais);
    imprimir_conciliacoes(&conciliacoes);
    if restantes != entradas {
        manuais::salvar(&cache_caminho, cfg.slug, &restantes)?;
    }
    Ok(())
}

fn imprimir_conciliacoes(conciliacoes: &[manuais::Conciliacao]) {
    for c in conciliacoes {
        match c {
            manuais::Conciliacao::Resolvida { .. } => println!("{c}"),
            manuais::Conciliacao::Divergente { .. }
            | manuais::Conciliacao::DifereDoBanco { .. } => eprintln!("{c}"),
        }
    }
}

fn imprimir_dica_buracos(cfg: &config::LoteriaConfig) {
    println!(
        "Para fechar os buracos: `sync buracos {0}` lista os concursos que faltam e \
         `sync inserir {0} <concurso> --data AAAA-MM-DD --dezenas ...` fecha um a um \
         (ou `sync baixar {0}` para o histórico completo).",
        cfg.slug
    );
}

/// Lista cada faixa de concursos faltando com as datas dos vizinhos, que
/// ajudam a localizar o sorteio na fonte oficial.
fn listar_buracos(cfg: &config::LoteriaConfig, db_caminho: &std::path::Path) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let resultados = carregar_existentes(cfg, db_caminho)?;
    let data_de = |concurso: u32| {
        resultados
            .iter()
            .find(|r| r.concurso == concurso)
            .map(|r| r.data_sorteio.as_str())
    };
    let buracos: Vec<(u32, u32)> = validacao::validar_sequencia(&resultados)
        .into_iter()
        .filter_map(|p| match p {
            validacao::Problema::ConcursosFaltando { inicio, fim } => Some((inicio, fim)),
            _ => None,
        })
        .collect();
    if buracos.is_empty() {
        println!("{}: nenhum concurso faltando.", cfg.nome_exibicao);
        return Ok(());
    }
    let total: u32 = buracos.iter().map(|(i, f)| f - i + 1).sum();
    println!("{}: {total} concurso(s) faltando.", cfg.nome_exibicao);
    for (inicio, fim) in buracos {
        let faixa = if inicio == fim {
            format!("{inicio}")
        } else {
            format!("{inicio}–{fim}")
        };
        let antes = inicio
            .checked_sub(1)
            .and_then(|c| data_de(c).map(|d| format!("depois do {c} ({d})")));
        let depois = data_de(fim + 1).map(|d| format!("antes do {} ({d})", fim + 1));
        let contexto: Vec<String> = [antes, depois].into_iter().flatten().collect();
        if contexto.is_empty() {
            println!("  - {faixa}");
        } else {
            println!("  - {faixa}: {}", contexto.join(", "));
        }
    }
    Ok(())
}

/// Fecha um buraco à mão: valida como qualquer gravação (com data fora de
/// ordem tratada como erro, já que é dado digitado), grava no banco e
/// registra no cache.
fn inserir_manual(
    cfg: &config::LoteriaConfig,
    entrada: manuais::EntradaManual,
    db_caminho: &std::path::Path,
    cache_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let existentes = carregar_existentes(cfg, db_caminho)?;
    if existentes.iter().any(|r| r.concurso == entrada.concurso) {
        anyhow::bail!(
            "o concurso {} de {} já está no banco; `inserir` só fecha buracos",
            entrada.concurso,
            cfg.slug
        );
    }
    let lote = [entrada.para_resultado(cfg.slug)];
    let decisao = validar_lote(cfg, &existentes, &lote, db_caminho)?;
    if decisao
        .avisos
        .iter()
        .any(|p| matches!(p, validacao::Problema::DataForaDeOrdem { .. }))
    {
        anyhow::bail!(
            "a data {} do concurso {} fica fora de ordem em relação aos vizinhos; \
             confira a data — nada foi gravado",
            entrada.data_sorteio,
            entrada.concurso
        );
    }

    // Cache primeiro: ele é a fonte da verdade do dado digitado. Se o banco
    // falhar depois, a entrada é reaplicada pelo próximo `importar`/`baixar`
    // e um novo `inserir` funciona (substitui a entrada do mesmo concurso).
    let cache_caminho = manuais::caminho(cache_dir, cfg.slug);
    let mut entradas = manuais::carregar(&cache_caminho, cfg.slug)?;
    let concurso = entrada.concurso;
    let data = entrada.data_sorteio.clone();
    manuais::incluir(&mut entradas, entrada);
    manuais::salvar(&cache_caminho, cfg.slug, &entradas)?;

    let mut conn = db::abrir(db_caminho)?;
    db::salvar_resultados(&mut conn, &lote)?;
    println!(
        "Concurso {concurso} ({data}) inserido em {} e registrado em {}.",
        db_caminho.display(),
        cache_caminho.display()
    );
    Ok(())
}

/// `sync features`: valida, calcula e grava (tudo em `orquestracao`) e
/// imprime o resumo. Banco inexistente é erro, sem criar arquivo.
fn calcular_features(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    completo: bool,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let mut conn = db::abrir(db_caminho)?;
    println!(
        "Calculando features de {} em {}...",
        cfg.nome_exibicao,
        db_caminho.display()
    );
    let inicio = std::time::Instant::now();
    let resumo = match orquestracao::sincronizar_features(&mut conn, cfg, completo) {
        Ok(resumo) => resumo,
        Err(SpiderError::ResultadosReprovados {
            loteria,
            erros,
            avisos,
        }) => {
            imprimir_problemas("aviso(s)", &avisos, Saida::Erro);
            imprimir_problemas("erro(s)", &erros, Saida::Erro);
            if erros
                .iter()
                .any(|p| matches!(p, validacao::Problema::ConcursosFaltando { .. }))
            {
                imprimir_dica_buracos(cfg);
            }
            anyhow::bail!(SpiderError::ResultadosReprovados {
                loteria,
                erros,
                avisos,
            });
        }
        Err(e) => return Err(e.into()),
    };
    let duracao = inicio.elapsed();

    imprimir_problemas("aviso(s)", &resumo.avisos, Saida::Padrao);
    println!("Modo: {}.", resumo.modo);
    println!(
        "{} concurso(s) incorporado(s); {} linha(s) gravada(s) em features_dezena.",
        resumo.concursos_incorporados, resumo.linhas_gravadas
    );
    if resumo.concursos_incorporados == 0 {
        println!(
            "Já estava em dia até o concurso {}.",
            resumo.ultimo_concurso
        );
    }
    println!(
        "Último concurso: {}; snapshot do próximo ({}) em features_dezena com \
         saiu = NULL. Tempo: {:.2} s.",
        resumo.ultimo_concurso,
        resumo.proximo_concurso,
        duracao.as_secs_f64()
    );
    Ok(())
}

fn imprimir_correcoes(ocorrencias: &[correcoes::Ocorrencia]) {
    for o in ocorrencias {
        match o {
            correcoes::Ocorrencia::Aplicada(_) => println!("Correção conhecida: {o}"),
            correcoes::Ocorrencia::Divergente { .. } => eprintln!("{o}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slugs(alvos: &Alvos) -> Vec<&'static str> {
        alvos.itens.iter().map(|(cfg, _)| cfg.slug).collect()
    }

    fn quina() -> &'static config::LoteriaConfig {
        config::buscar("quina").expect("quina configurada")
    }

    fn megasena() -> &'static config::LoteriaConfig {
        config::buscar("megasena").expect("megasena configurada")
    }

    #[test]
    fn resolver_alvos_recusa_db_com_todas() {
        let erro = resolver_alvos("todas", Some(PathBuf::from("x.db")), None)
            .err()
            .expect("--db com todas é recusado");
        assert!(erro.to_string().contains("--db"), "{erro}");
    }

    #[test]
    fn resolver_alvos_recusa_seletor_com_todas() {
        let erro = resolver_alvos("TODAS", None, Some("#botao"))
            .err()
            .expect("--seletor com todas é recusado");
        assert!(erro.to_string().contains("--seletor"), "{erro}");
    }

    #[test]
    fn resolver_alvos_slug_unico_usa_db_e_aceita_seletor() {
        let caminho = PathBuf::from("outro/quina.db");
        let alvos = resolver_alvos("quina", Some(caminho.clone()), Some("#botao"))
            .expect("slug único aceita --db e --seletor");
        assert!(!alvos.multiplas);
        assert_eq!(slugs(&alvos), ["quina"]);
        assert_eq!(alvos.itens[0].1, caminho);
    }

    #[test]
    fn resolver_alvos_slug_unico_sem_db_usa_padrao() {
        let alvos = resolver_alvos("MegaSena", None, None).expect("slug conhecido");
        assert!(!alvos.multiplas);
        assert_eq!(slugs(&alvos), ["megasena"]);
        assert_eq!(alvos.itens[0].1, PathBuf::from("resultados_megasena.db"));
    }

    #[test]
    fn resolver_alvos_todas_devolve_caminhos_padrao_na_ordem() {
        let alvos = resolver_alvos("todas", None, None).expect("todas é aceito");
        assert!(alvos.multiplas);
        let esperado: Vec<&str> = config::LOTERIAS.iter().map(|l| l.slug).collect();
        assert_eq!(slugs(&alvos), esperado);
        let caminhos: Vec<PathBuf> = alvos.itens.iter().map(|(_, p)| p.clone()).collect();
        assert_eq!(
            caminhos,
            [
                PathBuf::from("resultados_lotofacil.db"),
                PathBuf::from("resultados_megasena.db"),
                PathBuf::from("resultados_quina.db"),
            ]
        );
    }

    #[test]
    fn resolver_alvos_desconhecida_da_erro() {
        let erro = resolver_alvos("timemania", None, None)
            .err()
            .expect("slug desconhecido");
        assert!(erro.to_string().contains("loteria desconhecida"), "{erro}");
    }

    #[test]
    fn execucao_slug_unico_devolve_o_erro_em_registrar() {
        let mut execucao = Execucao::nova(false);
        assert!(execucao.registrar(quina(), Ok(())).is_ok());
        let erro = execucao
            .registrar(quina(), Err(anyhow::anyhow!("falha original")))
            .expect_err("slug único propaga o erro");
        assert_eq!(erro.to_string(), "falha original");
        assert!(execucao.concluir().is_ok());
    }

    #[test]
    fn execucao_todas_segue_adiante_e_concluir_lista_quem_falhou() {
        let mut execucao = Execucao::nova(true);
        assert!(execucao
            .registrar(megasena(), Err(anyhow::anyhow!("sem rede")))
            .is_ok());
        assert!(execucao.registrar(quina(), Ok(())).is_ok());
        let erro = execucao
            .concluir()
            .expect_err("alguma falhou: concluir dá erro");
        let texto = erro.to_string();
        assert!(texto.contains("1 de 2"), "{texto}");
        assert!(texto.contains("megasena"), "{texto}");
        assert!(!texto.contains("quina"), "{texto}");
    }

    #[test]
    fn execucao_todas_sem_falhas_conclui_ok() {
        let mut execucao = Execucao::nova(true);
        for cfg in config::LOTERIAS {
            assert!(execucao.registrar(cfg, Ok(())).is_ok());
        }
        assert!(execucao.concluir().is_ok());
    }

    #[test]
    fn buscar_config_unica_recusa_todas() {
        let erro = buscar_config_unica("todas", "importar", "o arquivo é de uma loteria só")
            .expect_err("todas é recusado");
        let texto = erro.to_string();
        assert!(texto.contains("importar"), "{texto}");
        assert!(texto.contains("o arquivo é de uma loteria só"), "{texto}");
        assert_eq!(
            buscar_config_unica("quina", "importar", "-")
                .expect("slug único")
                .slug,
            "quina"
        );
    }

    #[test]
    fn listar_banco_inexistente_da_erro_sem_criar_arquivo() {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_listar_{}_{unico}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&caminho);

        let erro = listar(quina(), &caminho).expect_err("banco inexistente dá erro");
        let existe = caminho.exists();
        let _ = std::fs::remove_file(&caminho);

        assert!(erro.to_string().contains("banco não encontrado"), "{erro}");
        assert!(!existe, "listar não deve criar {}", caminho.display());
    }

    #[test]
    fn features_banco_inexistente_da_erro_sem_criar_arquivo() {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_features_{}_{unico}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&caminho);

        let erro = calcular_features(quina(), &caminho, false).expect_err("banco inexistente");
        let existe = caminho.exists();
        let _ = std::fs::remove_file(&caminho);

        assert!(erro.to_string().contains("banco não encontrado"), "{erro}");
        assert!(!existe, "features não deve criar {}", caminho.display());
    }

    #[test]
    fn features_recusa_todas() {
        let erro = buscar_config_unica(
            "todas",
            "features",
            "calcule uma loteria por vez (cada uma tem seu banco)",
        )
        .expect_err("todas é recusado");
        assert!(erro.to_string().contains("features"), "{erro}");
    }
}
