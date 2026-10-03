use clap::{Parser, Subcommand};
use rust_spider::{config, correcoes, db, downloader, parser, validacao};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Baixa e processa resultados de loterias da Caixa (arquivo .htm -> SQLite/JSON)"
)]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Navega até o site oficial, baixa o .htm de resultados e já grava no banco.
    Baixar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina)
        loteria: String,
        /// Pasta onde salvar o .htm baixado
        #[arg(long, default_value = "downloads")]
        dir: PathBuf,
        /// Caminho do banco SQLite. Por padrão, um arquivo por loteria
        /// (resultados_<loteria>.db), para não misturar dados de loterias diferentes.
        #[arg(long)]
        db: Option<PathBuf>,
        /// Sobrescreve o seletor CSS do botão de download configurado para essa loteria
        #[arg(long)]
        seletor: Option<String>,
        /// Mostra a janela do navegador (útil para depurar bloqueios/seletores)
        #[arg(long)]
        visivel: bool,
    },
    /// Processa um arquivo .htm já baixado manualmente, sem abrir navegador.
    Importar {
        /// Slug da loteria (define como interpretar as colunas da tabela)
        loteria: String,
        /// Caminho do arquivo .htm
        arquivo: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Lista os resultados já salvos no banco, em JSON.
    Listar {
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Lê o último resultado direto da página (sem baixar a planilha inteira)
    /// e grava no banco só se for um concurso novo.
    Atualizar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina)
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Mostra a janela do navegador (útil para depurar bloqueios/seletores)
        #[arg(long)]
        visivel: bool,
    },
    /// Confere a integridade dos resultados salvos: quantidade de dezenas,
    /// faixa, repetidas, datas, buracos na numeração e datas fora de ordem.
    /// Sai com erro se encontrar algum erro; datas fora de ordem são só avisos.
    Validar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina)
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
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
        } => {
            let cfg = buscar_config(&loteria)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));

            println!("Baixando resultados de {}...", cfg.nome_exibicao);
            let arquivo =
                downloader::baixar_resultado(cfg, seletor.as_deref(), &dir, !visivel).await?;
            println!("Arquivo salvo em {}", arquivo.display());

            processar_e_salvar(&arquivo, cfg, &db_caminho)?;
        }
        Comando::Importar {
            loteria,
            arquivo,
            db,
        } => {
            let cfg = buscar_config(&loteria)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            processar_e_salvar(&arquivo, cfg, &db_caminho)?;
        }
        Comando::Listar { loteria, db } => {
            let cfg = buscar_config(&loteria)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let conn = db::abrir(&db_caminho)?;
            let resultados = db::listar(&conn, cfg.slug)?;
            println!("{}", serde_json::to_string_pretty(&resultados)?);
        }
        Comando::Atualizar {
            loteria,
            db,
            visivel,
        } => {
            let cfg = buscar_config(&loteria)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));

            println!("Verificando o último resultado de {}...", cfg.nome_exibicao);
            let mut resultado = downloader::raspar_ultimo_resultado(cfg, !visivel).await?;
            imprimir_correcoes(&correcoes::aplicar(
                cfg.slug,
                std::slice::from_mut(&mut resultado),
            ));

            let existentes = carregar_existentes(cfg, &db_caminho)?;
            let ultimo_salvo = existentes.iter().map(|r| r.concurso).max();

            if let Some(u) = ultimo_salvo {
                if u >= resultado.concurso {
                    println!(
                        "Já está atualizado: último concurso salvo é {u}, a página mostra {}.",
                        resultado.concurso
                    );
                    return Ok(());
                }
            }

            let lote = std::slice::from_ref(&resultado);
            let decisao = validar_lote(
                cfg,
                &existentes,
                lote,
                validacao::ModoGravacao::UltimoConcurso,
                &db_caminho,
            )?;
            // O buraco já saiu na lista de avisos; aqui só a orientação.
            if decisao
                .avisos
                .iter()
                .any(|p| matches!(p, validacao::Problema::ConcursosFaltando { .. }))
            {
                println!(
                    "A raspagem só traz o concurso mais recente. Rode `baixar {loteria}` para \
                     preencher o histórico completo."
                );
            }

            // Só depois de aprovado: `db::abrir` cria o arquivo se não existir.
            let mut conn = db::abrir(&db_caminho)?;
            let gravados = db::salvar_resultados(&mut conn, lote)?;
            println!(
                "Concurso {} ({}) gravado em {} ({gravados} registro(s) novo(s)/atualizado(s)).",
                resultado.concurso,
                resultado.data_sorteio,
                db_caminho.display()
            );
        }
        Comando::Validar { loteria, db } => {
            let cfg = buscar_config(&loteria)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            validar_banco(cfg, &db_caminho)?;
        }
    }

    Ok(())
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

fn imprimir_problemas<P: std::fmt::Display>(rotulo: &str, problemas: &[P], saida: Saida) {
    if problemas.is_empty() {
        return;
    }
    let imprimir = |linha: String| match saida {
        Saida::Padrao => println!("{linha}"),
        Saida::Erro => eprintln!("{linha}"),
    };
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
    modo: validacao::ModoGravacao,
    db_caminho: &std::path::Path,
) -> anyhow::Result<validacao::DecisaoGravacao> {
    let decisao = validacao::avaliar_gravacao(cfg, existentes, novos, modo);
    imprimir_problemas("aviso(s)", &decisao.avisos, Saida::Padrao);
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

fn buscar_config(slug: &str) -> anyhow::Result<&'static config::LoteriaConfig> {
    config::buscar(slug).ok_or_else(|| {
        anyhow::anyhow!(
            "loteria desconhecida: `{slug}`. Use uma de: {}",
            config::slugs_disponiveis()
        )
    })
}

fn caminho_db_padrao(slug: &str) -> PathBuf {
    PathBuf::from(format!("resultados_{slug}.db"))
}

fn processar_e_salvar(
    arquivo: &std::path::Path,
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
) -> anyhow::Result<()> {
    let mut resultados = parser::parsear_arquivo(arquivo, cfg)?;
    println!("{} concurso(s) extraído(s) do arquivo.", resultados.len());
    imprimir_correcoes(&correcoes::aplicar(cfg.slug, &mut resultados));

    let existentes = carregar_existentes(cfg, db_caminho)?;
    validar_lote(
        cfg,
        &existentes,
        &resultados,
        validacao::ModoGravacao::HistoricoCompleto,
        db_caminho,
    )?;
    // Só depois de aprovado: `db::abrir` cria o arquivo se não existir.
    let mut conn = db::abrir(db_caminho)?;
    let gravados = db::salvar_resultados(&mut conn, &resultados)?;
    println!(
        "{gravados} registro(s) novo(s)/atualizado(s) em {}",
        db_caminho.display()
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
