use clap::{Parser, Subcommand};
use rust_spider::{config, db, downloader, parser};
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
}

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
            let arquivo = downloader::baixar_resultado(cfg, seletor.as_deref(), &dir, !visivel).await?;
            println!("Arquivo salvo em {}", arquivo.display());

            processar_e_salvar(&arquivo, cfg, &db_caminho)?;
        }
        Comando::Importar { loteria, arquivo, db } => {
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
        Comando::Atualizar { loteria, db, visivel } => {
            let cfg = buscar_config(&loteria)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));

            println!("Verificando o último resultado de {}...", cfg.nome_exibicao);
            let resultado = downloader::raspar_ultimo_resultado(cfg, !visivel).await?;

            let mut conn = db::abrir(&db_caminho)?;
            let ultimo_salvo = db::ultimo_concurso(&conn, cfg.slug)?;

            if let Some(u) = ultimo_salvo {
                if u >= resultado.concurso {
                    println!(
                        "Já está atualizado: último concurso salvo é {u}, a página mostra {}.",
                        resultado.concurso
                    );
                    return Ok(());
                }
                if resultado.concurso - u > 1 {
                    println!(
                        "Atenção: faltam concurso(s) entre {u} e {} — a raspagem só traz o mais \
                         recente. Rode `baixar {loteria}` para preencher o histórico completo.",
                        resultado.concurso
                    );
                }
            }

            let gravados = db::salvar_resultados(&mut conn, std::slice::from_ref(&resultado))?;
            println!(
                "Concurso {} ({}) gravado em {} ({gravados} registro(s) novo(s)/atualizado(s)).",
                resultado.concurso,
                resultado.data_sorteio,
                db_caminho.display()
            );
        }
    }

    Ok(())
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
    let resultados = parser::parsear_arquivo(arquivo, cfg)?;
    println!("{} concurso(s) extraído(s) do arquivo.", resultados.len());

    let mut conn = db::abrir(db_caminho)?;
    let gravados = db::salvar_resultados(&mut conn, &resultados)?;
    println!("{gravados} registro(s) novo(s)/atualizado(s) em {}", db_caminho.display());

    Ok(())
}
