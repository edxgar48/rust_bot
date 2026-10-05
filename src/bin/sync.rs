use clap::{Parser, Subcommand};
use rust_spider::{
    backup, config, correcoes, db, downloader, manuais, orquestracao, parser, perfil, tempo,
    validacao, SpiderError,
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
    /// e grava no banco só se for um concurso novo. Se faltar concurso entre o
    /// último salvo e o da página (salto), copia o banco para `backups/`
    /// (nome com data e hora em UTC) e baixa a planilha completa, como `baixar`.
    Atualizar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Pasta onde salvar a planilha (.xlsx), se houver salto
        #[arg(long, default_value = "downloads")]
        dir: PathBuf,
        /// Pasta dos backups do banco feitos antes de baixar a planilha
        /// (resultados_<loteria>_<AAAAMMDD-HHMMSS>.db, horário em UTC)
        #[arg(long, default_value = backup::PASTA_PADRAO)]
        backups: PathBuf,
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
    /// Calcula o índice de força (total, posição e variação de posição) de
    /// cada dezena em cada concurso com um perfil de pesos e grava em `indice`,
    /// regenerando o índice inteiro do perfil. Recusa rodar se as features não
    /// estiverem em dia (rode `features` antes).
    Indice {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Nome do perfil de pesos (arquivo `<perfis>/<loteria>/<nome>.toml`,
        /// ex.: padrao_v1)
        #[arg(long)]
        perfil: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
    },
}

/// Quantos problemas imprimir antes de resumir o restante numa contagem.
const LIMITE_PROBLEMAS_IMPRESSOS: usize = 50;

/// Por que `indice` não aceita `todas`.
const MOTIVO_INDICE_UMA_LOTERIA: &str =
    "calcule uma loteria por vez (cada uma tem seu banco e seus perfis)";

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
            dir,
            backups,
            db,
            visivel,
            cache,
        } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            let pastas = PastasAtualizar {
                downloads: &dir,
                backups: &backups,
                cache: &cache,
            };
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                let r = atualizar(cfg, &db_caminho, &pastas, visivel).await;
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
        Comando::Indice {
            loteria,
            perfil,
            db,
            perfis,
        } => {
            let cfg = buscar_config_unica(&loteria, "indice", MOTIVO_INDICE_UMA_LOTERIA)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            calcular_indice(cfg, &db_caminho, &perfis, &perfil)?;
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
    let arquivo = baixar_planilha(cfg, dir, seletor, visivel).await?;
    processar_e_salvar(&arquivo, cfg, db_caminho, cache)
}

/// Só o download da planilha (sem gravar): a parte do `baixar` que o
/// `atualizar` com salto reaproveita.
async fn baixar_planilha(
    cfg: &'static config::LoteriaConfig,
    dir: &std::path::Path,
    seletor: Option<&str>,
    visivel: bool,
) -> anyhow::Result<PathBuf> {
    println!("Baixando resultados de {}...", cfg.nome_exibicao);
    let arquivo = downloader::baixar_resultado(cfg, seletor, dir, !visivel).await?;
    println!("Arquivo salvo em {}", arquivo.display());
    Ok(arquivo)
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

/// Pastas usadas pelo `atualizar`: planilha (se houver salto), backups do
/// banco e cache de inserções manuais.
struct PastasAtualizar<'a> {
    downloads: &'a std::path::Path,
    backups: &'a std::path::Path,
    cache: &'a std::path::Path,
}

async fn atualizar(
    cfg: &'static config::LoteriaConfig,
    db_caminho: &std::path::Path,
    pastas: &PastasAtualizar<'_>,
    visivel: bool,
) -> anyhow::Result<()> {
    println!("Verificando o último resultado de {}...", cfg.nome_exibicao);
    let mut resultado = downloader::raspar_ultimo_resultado(cfg, !visivel).await?;
    imprimir_correcoes(&correcoes::aplicar(
        cfg.slug,
        std::slice::from_mut(&mut resultado),
    ));
    let downloads = pastas.downloads;
    aplicar_atualizacao(cfg, &resultado, db_caminho, pastas, agora_segundos, || {
        baixar_planilha(cfg, downloads, None, visivel)
    })
    .await
}

/// O `atualizar` depois de raspar a página: nada a fazer, gravar o próximo
/// concurso ou, num salto, fazer backup e seguir o caminho do `baixar`
/// (tarefa 0.8). O relógio (`agora`, lido só no salto com banco) e o download
/// da planilha (`obter_planilha`) vêm de fora para o fluxo ser testável sem
/// rede.
async fn aplicar_atualizacao<F, Fut>(
    cfg: &config::LoteriaConfig,
    resultado: &rust_spider::Resultado,
    db_caminho: &std::path::Path,
    pastas: &PastasAtualizar<'_>,
    agora: impl FnOnce() -> anyhow::Result<u64>,
    obter_planilha: F,
) -> anyhow::Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<PathBuf>>,
{
    let existentes = carregar_existentes(cfg, db_caminho)?;
    let ultimo_salvo = existentes.iter().map(|r| r.concurso).max();

    let classificacao = orquestracao::classificar_atualizacao(ultimo_salvo, resultado.concurso);
    let (ultimo, pagina) = match classificacao {
        orquestracao::Atualizacao::SemNovidade { ultimo } => {
            println!(
                "Já está atualizado: último concurso salvo é {ultimo}, a página mostra {}.",
                resultado.concurso
            );
            // Uma inserção manual do mesmo concurso ainda pode ser conciliada.
            return conciliar_cache(cfg, pastas.cache, std::slice::from_ref(resultado));
        }
        orquestracao::Atualizacao::Proximo => {
            return gravar_da_pagina(cfg, &existentes, resultado, db_caminho, pastas.cache);
        }
        orquestracao::Atualizacao::Salto { ultimo, pagina } => (ultimo, pagina),
    };

    // Salto: gravar só o concurso da página deixaria buraco.
    let copia = if ultimo == 0 {
        println!(
            "O banco não tem concursos de {} e a página mostra o {pagina}: \
             baixando a planilha completa.",
            cfg.nome_exibicao
        );
        if db_caminho.exists() {
            println!(
                "O banco {} não tem concursos de {}: nenhum backup feito.",
                db_caminho.display(),
                cfg.nome_exibicao
            );
        } else {
            println!(
                "Não há banco para copiar ({}): nenhum backup feito.",
                db_caminho.display()
            );
        }
        None
    } else {
        println!(
            "Salto: o último concurso salvo é {ultimo} e a página mostra {pagina} \
             ({} concurso(s) perdido(s) no meio): baixando a planilha completa.",
            classificacao.faltando()
        );
        let copia = backup::criar(db_caminho, pastas.backups, cfg.slug, agora()?)?;
        println!("Backup do banco em {}", copia.display());
        Some(copia)
    };

    let feito = async {
        let arquivo = obter_planilha().await?;
        processar_e_salvar(&arquivo, cfg, db_caminho, pastas.cache)?;

        // A planilha pode estar atrasada em relação à página.
        let mut existentes = carregar_existentes(cfg, db_caminho)?;
        if !existentes.iter().any(|r| r.concurso == resultado.concurso) {
            println!(
                "A planilha ainda não traz o concurso {} da página; gravando-o a partir da página.",
                resultado.concurso
            );
            gravar_da_pagina(cfg, &existentes, resultado, db_caminho, pastas.cache)?;
            existentes = carregar_existentes(cfg, db_caminho)?;
        }
        Ok::<_, anyhow::Error>(existentes)
    }
    .await;
    let existentes = feito.map_err(|e| match &copia {
        Some(c) => e.context(format!(
            "{}: o caminho da planilha completa falhou; o backup do banco de antes \
             da tentativa está em {}",
            cfg.slug,
            c.display()
        )),
        None => e.context(format!(
            "{}: o caminho da planilha completa falhou",
            cfg.slug
        )),
    })?;

    // Buraco que sobrou é só aviso (decisão da 0.7): o comando termina bem.
    // Sem `imprimir_dica_buracos` aqui: o estado final é exatamente o que o
    // último `validar_lote` (da planilha ou da página) avaliou, e ele reporta
    // todos os buracos desse estado; logo a dica já saiu se há buraco.
    let buracos = faixas_faltando(&existentes);
    if !buracos.is_empty() {
        let total: u32 = buracos.iter().map(|(i, f)| f - i + 1).sum();
        let faixas: Vec<String> = buracos
            .iter()
            .map(|&(i, f)| descrever_faixa(i, f))
            .collect();
        println!(
            "Atenção: ainda falta(m) {total} concurso(s) de {} no banco (nem a planilha \
             nem a página os trazem): {}.",
            cfg.nome_exibicao,
            faixas.join(", ")
        );
    }
    Ok(())
}

/// Grava o concurso lido na página pelo caminho do salto de 1: validação
/// contra o banco, gravação e conciliação com o cache de inserções manuais.
fn gravar_da_pagina(
    cfg: &config::LoteriaConfig,
    existentes: &[rust_spider::Resultado],
    resultado: &rust_spider::Resultado,
    db_caminho: &std::path::Path,
    cache: &std::path::Path,
) -> anyhow::Result<()> {
    let lote = std::slice::from_ref(resultado);
    validar_lote(cfg, existentes, lote, db_caminho)?;

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

/// Faixas de concursos faltando (início, fim), como o `validar` as vê.
fn faixas_faltando(resultados: &[rust_spider::Resultado]) -> Vec<(u32, u32)> {
    validacao::validar_sequencia(resultados)
        .into_iter()
        .filter_map(|p| match p {
            validacao::Problema::ConcursosFaltando { inicio, fim } => Some((inicio, fim)),
            _ => None,
        })
        .collect()
}

fn descrever_faixa(inicio: u32, fim: u32) -> String {
    if inicio == fim {
        format!("{inicio}")
    } else {
        format!("{inicio}–{fim}")
    }
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
/// `listar`, `features`, `indice`): recusa `todas` explicando o porquê.
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
    let buracos = faixas_faltando(&resultados);
    if buracos.is_empty() {
        println!("{}: nenhum concurso faltando.", cfg.nome_exibicao);
        return Ok(());
    }
    let total: u32 = buracos.iter().map(|(i, f)| f - i + 1).sum();
    println!("{}: {total} concurso(s) faltando.", cfg.nome_exibicao);
    for (inicio, fim) in buracos {
        let faixa = descrever_faixa(inicio, fim);
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

/// `sync indice`: carrega o perfil, calcula e grava o índice (tudo em
/// `orquestracao`) e imprime o resumo. Banco inexistente é erro, sem criar
/// arquivo. É aqui que se lê o relógio para `perfis.criado_em`.
fn calcular_indice(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    nome_perfil: &str,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let carregado = perfil::carregar(raiz_perfis, cfg, nome_perfil)?;
    let criado_em = agora_iso8601_utc()?;
    let mut conn = db::abrir(db_caminho)?;
    let p = &carregado.perfil;
    println!(
        "Calculando o índice de {} em {} com o perfil {} (versão {}, janela {}, {})...",
        cfg.nome_exibicao,
        db_caminho.display(),
        p.nome,
        p.versao,
        descrever_janela(p.janela),
        p.normalizacao.como_str()
    );
    let inicio = std::time::Instant::now();
    let resumo = orquestracao::sincronizar_indice(&mut conn, cfg, &carregado, &criado_em)?;
    let duracao = inicio.elapsed();

    let registro = match resumo.registro {
        orquestracao::RegistroPerfil::Novo => "registrado agora em perfis",
        orquestracao::RegistroPerfil::JaRegistrado => "já registrado em perfis, sem alteração",
    };
    println!(
        "Perfil {}: {registro} (hash {}).",
        resumo.perfil, carregado.hash_toml
    );
    println!(
        "{} concurso(s) indexado(s), do {} ao {}; {} linha(s) gravada(s) em indice \
         ({} substituída(s)).",
        resumo.concursos,
        resumo.primeiro_concurso,
        resumo.proximo_concurso,
        resumo.linhas_gravadas,
        resumo.linhas_apagadas
    );
    println!(
        "Último concurso: {}; índice do próximo ({}) incluído. Tempo: {:.2} s.",
        resumo.ultimo_concurso,
        resumo.proximo_concurso,
        duracao.as_secs_f64()
    );
    Ok(())
}

fn descrever_janela(janela: u32) -> String {
    if janela == rust_spider::motor::JANELA_TOTAL {
        "total".to_string()
    } else {
        janela.to_string()
    }
}

/// Momento atual em ISO-8601 UTC com segundos, para `perfis.criado_em`.
fn agora_iso8601_utc() -> anyhow::Result<String> {
    Ok(tempo::iso8601_utc(agora_segundos()?))
}

/// Segundos desde 1970-01-01T00:00:00Z: o único ponto que lê o relógio.
fn agora_segundos() -> anyhow::Result<u64> {
    let desde_1970 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| anyhow::anyhow!("relógio do sistema antes de 1970-01-01; acerte a data"))?;
    Ok(desde_1970.as_secs())
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
    fn indice_banco_inexistente_da_erro_sem_criar_arquivo() {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_indice_{}_{unico}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&caminho);

        let erro = calcular_indice(
            quina(),
            &caminho,
            std::path::Path::new("perfis"),
            "padrao_v1",
        )
        .expect_err("banco inexistente");
        let existe = caminho.exists();
        let _ = std::fs::remove_file(&caminho);

        assert!(erro.to_string().contains("banco não encontrado"), "{erro}");
        assert!(!existe, "indice não deve criar {}", caminho.display());
    }

    #[test]
    fn indice_recusa_todas() {
        let erro = buscar_config_unica("todas", "indice", MOTIVO_INDICE_UMA_LOTERIA)
            .expect_err("todas é recusado");
        let texto = erro.to_string();
        assert!(texto.contains("`indice` não aceita `todas`"), "{texto}");
    }

    #[test]
    fn indice_exige_perfil_e_usa_a_pasta_perfis_por_padrao() {
        assert!(Cli::try_parse_from(["sync", "indice", "quina"]).is_err());
        let cli = Cli::try_parse_from(["sync", "indice", "quina", "--perfil", "padrao_v1"])
            .expect("com --perfil");
        match cli.comando {
            Comando::Indice {
                loteria,
                perfil,
                db,
                perfis,
            } => {
                assert_eq!(loteria, "quina");
                assert_eq!(perfil, "padrao_v1");
                assert_eq!(db, None);
                assert_eq!(perfis, PathBuf::from("perfis"));
            }
            _ => panic!("esperava o comando indice"),
        }
    }

    #[test]
    fn agora_e_iso8601_utc_com_segundos() {
        let agora = agora_iso8601_utc().expect("relógio");
        assert_eq!(agora.len(), "2026-10-04T18:30:00Z".len(), "{agora}");
        assert!(agora.ends_with('Z'), "{agora}");
        assert_eq!(agora.as_bytes()[10], b'T', "{agora}");
        assert!(agora.as_str() > "2026-01-01T00:00:00Z", "{agora}");
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

    // --- `atualizar` (tarefa 0.8), sem rede: a planilha é a fixture da Quina
    // (concursos 1 a 5) e o concurso "da página" é montado no teste.

    /// 2026-10-04T18:30:00Z.
    const AGORA: u64 = 1_791_138_600;

    /// Pasta temporária com o banco, o cache, `downloads/` e `backups/`.
    struct Cenario {
        raiz: PathBuf,
        db: PathBuf,
        cache: PathBuf,
        downloads: PathBuf,
        backups: PathBuf,
    }

    impl Cenario {
        fn novo(rotulo: &str) -> Self {
            let unico = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let raiz = std::env::temp_dir().join(format!(
                "rust_spider_atualizar_{rotulo}_{}_{unico}",
                std::process::id()
            ));
            std::fs::create_dir_all(&raiz).unwrap();
            Cenario {
                db: raiz.join("resultados_quina.db"),
                cache: raiz.join("cache"),
                downloads: raiz.join("downloads"),
                backups: raiz.join("backups"),
                raiz,
            }
        }

        fn pastas(&self) -> PastasAtualizar<'_> {
            PastasAtualizar {
                downloads: &self.downloads,
                backups: &self.backups,
                cache: &self.cache,
            }
        }

        fn gravar(&self, lote: &[rust_spider::Resultado]) {
            let mut conn = db::abrir(&self.db).unwrap();
            db::salvar_resultados(&mut conn, lote).unwrap();
        }

        fn concursos(&self) -> Vec<u32> {
            let conn = db::abrir(&self.db).unwrap();
            let mut c: Vec<u32> = db::listar(&conn, "quina")
                .unwrap()
                .iter()
                .map(|r| r.concurso)
                .collect();
            c.sort_unstable();
            c
        }

        fn backups_feitos(&self) -> Vec<PathBuf> {
            match std::fs::read_dir(&self.backups) {
                Ok(it) => it.map(|e| e.unwrap().path()).collect(),
                Err(_) => Vec::new(),
            }
        }
    }

    impl Drop for Cenario {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.raiz);
        }
    }

    fn fixture_quina() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("quina.xlsx")
    }

    /// Concursos 1 a 5 da fixture, como a planilha os entrega.
    fn planilha_quina() -> Vec<rust_spider::Resultado> {
        parser::parsear_arquivo(&fixture_quina(), quina()).unwrap()
    }

    fn da_planilha(concurso: u32) -> rust_spider::Resultado {
        planilha_quina()
            .into_iter()
            .find(|r| r.concurso == concurso)
            .unwrap()
    }

    /// Concurso que a planilha (fixture) ainda não traz.
    fn so_na_pagina(concurso: u32) -> rust_spider::Resultado {
        rust_spider::Resultado {
            loteria: "quina".to_string(),
            concurso,
            data_sorteio: "1994-04-30".to_string(),
            dezenas: vec![3, 17, 28, 42, 66],
        }
    }

    fn relogio_proibido() -> anyhow::Result<u64> {
        panic!("o relógio só é lido num salto com banco")
    }

    async fn planilha_proibida() -> anyhow::Result<PathBuf> {
        panic!("a planilha só é baixada num salto")
    }

    #[tokio::test]
    async fn atualizar_sem_novidade_nao_baixa_nem_copia() {
        let c = Cenario::novo("sem_novidade");
        c.gravar(&planilha_quina());
        aplicar_atualizacao(
            quina(),
            &da_planilha(5),
            &c.db,
            &c.pastas(),
            relogio_proibido,
            planilha_proibida,
        )
        .await
        .unwrap();
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        assert!(!c.backups.exists());
    }

    #[tokio::test]
    async fn atualizar_salto_de_um_grava_so_o_da_pagina_sem_backup() {
        let c = Cenario::novo("proximo");
        c.gravar(&planilha_quina()[..4]);
        aplicar_atualizacao(
            quina(),
            &da_planilha(5),
            &c.db,
            &c.pastas(),
            relogio_proibido,
            planilha_proibida,
        )
        .await
        .unwrap();
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        assert!(!c.backups.exists());
    }

    #[tokio::test]
    async fn atualizar_salto_faz_backup_e_grava_a_planilha_completa() {
        let c = Cenario::novo("salto");
        c.gravar(&planilha_quina()[..2]);
        aplicar_atualizacao(
            quina(),
            &da_planilha(5),
            &c.db,
            &c.pastas(),
            || Ok(AGORA),
            || async { Ok(fixture_quina()) },
        )
        .await
        .unwrap();

        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        let backup = c.backups.join("resultados_quina_20261004-183000.db");
        assert_eq!(c.backups_feitos(), std::slice::from_ref(&backup));
        let copia = db::listar_crescente(&db::abrir(&backup).unwrap(), "quina").unwrap();
        assert_eq!(copia, planilha_quina()[..2].to_vec());
    }

    #[tokio::test]
    async fn atualizar_salto_com_planilha_atrasada_grava_tambem_o_da_pagina() {
        let c = Cenario::novo("atrasada");
        c.gravar(&planilha_quina()[..2]);
        aplicar_atualizacao(
            quina(),
            &so_na_pagina(6),
            &c.db,
            &c.pastas(),
            || Ok(AGORA),
            || async { Ok(fixture_quina()) },
        )
        .await
        .unwrap();
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5, 6]);
        assert_eq!(c.backups_feitos().len(), 1);
    }

    #[tokio::test]
    async fn atualizar_salto_que_ainda_deixa_buraco_avisa_e_termina_bem() {
        let c = Cenario::novo("buraco");
        c.gravar(&planilha_quina()[..2]);
        aplicar_atualizacao(
            quina(),
            &so_na_pagina(8),
            &c.db,
            &c.pastas(),
            || Ok(AGORA),
            || async { Ok(fixture_quina()) },
        )
        .await
        .expect("buraco é só aviso");
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5, 8]);
    }

    #[tokio::test]
    async fn atualizar_salto_com_download_falhando_nao_grava_e_aponta_o_backup() {
        let c = Cenario::novo("download_falha");
        c.gravar(&planilha_quina()[..2]);
        let erro = aplicar_atualizacao(
            quina(),
            &da_planilha(5),
            &c.db,
            &c.pastas(),
            || Ok(AGORA),
            || async { Err(anyhow::anyhow!("sem rede")) },
        )
        .await
        .expect_err("download falhou");

        let backup = c.backups.join("resultados_quina_20261004-183000.db");
        let texto = format!("{erro:#}");
        assert!(texto.contains("sem rede"), "{texto}");
        assert!(texto.contains(&backup.display().to_string()), "{texto}");
        assert_eq!(c.concursos(), [1, 2]);
        assert_eq!(c.backups_feitos(), [backup]);
    }

    #[tokio::test]
    async fn atualizar_salto_com_planilha_ilegivel_nao_grava_nem_o_da_pagina() {
        let c = Cenario::novo("planilha_ruim");
        c.gravar(&planilha_quina()[..2]);
        let corrompida = fixture_quina().with_file_name("corrompida.xlsx");
        let erro = aplicar_atualizacao(
            quina(),
            &so_na_pagina(6),
            &c.db,
            &c.pastas(),
            || Ok(AGORA),
            || async { Ok(corrompida) },
        )
        .await
        .expect_err("planilha ilegível");
        assert!(format!("{erro:#}").contains("backup"), "{erro:#}");
        assert_eq!(c.concursos(), [1, 2]);
        assert_eq!(c.backups_feitos().len(), 1);
    }

    #[tokio::test]
    async fn atualizar_salto_com_lote_reprovado_nao_grava_nada() {
        let c = Cenario::novo("lote_reprovado");
        // A página traz um concurso com dezena fora da faixa que a planilha
        // não traz: a planilha (válida) é gravada e o lote da página, recusado.
        c.gravar(&planilha_quina()[..2]);
        let mut invalido = so_na_pagina(6);
        invalido.dezenas = vec![1, 2, 3, 4, 99];
        let erro = aplicar_atualizacao(
            quina(),
            &invalido,
            &c.db,
            &c.pastas(),
            || Ok(AGORA),
            || async { Ok(fixture_quina()) },
        )
        .await
        .expect_err("concurso da página inválido");
        let texto = format!("{erro:#}");
        assert!(texto.contains("lote recusado"), "{texto}");
        assert!(texto.contains("backup"), "{texto}");
        // A planilha (válida) já foi gravada; o concurso inválido, não.
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn atualizar_banco_inexistente_baixa_a_planilha_sem_backup() {
        let c = Cenario::novo("sem_banco");
        aplicar_atualizacao(
            quina(),
            &da_planilha(5),
            &c.db,
            &c.pastas(),
            relogio_proibido,
            || async { Ok(fixture_quina()) },
        )
        .await
        .unwrap();
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        assert!(!c.backups.exists());
    }

    #[tokio::test]
    async fn atualizar_banco_inexistente_com_planilha_falhando_nao_cria_o_banco() {
        let c = Cenario::novo("sem_banco_falha");
        aplicar_atualizacao(
            quina(),
            &da_planilha(5),
            &c.db,
            &c.pastas(),
            relogio_proibido,
            || async { Err(anyhow::anyhow!("sem rede")) },
        )
        .await
        .expect_err("download falhou");
        assert!(!c.db.exists());
        assert!(!c.backups.exists());
    }

    #[test]
    fn atualizar_aceita_dir_e_backups_com_padroes() {
        let cli = Cli::try_parse_from(["sync", "atualizar", "todas"]).expect("sem opções");
        match cli.comando {
            Comando::Atualizar { dir, backups, .. } => {
                assert_eq!(dir, PathBuf::from("downloads"));
                assert_eq!(backups, PathBuf::from("backups"));
            }
            _ => panic!("esperava o comando atualizar"),
        }
        let cli = Cli::try_parse_from([
            "sync",
            "atualizar",
            "quina",
            "--dir",
            "x/planilhas",
            "--backups",
            "y/copias",
        ])
        .expect("com --dir e --backups");
        match cli.comando {
            Comando::Atualizar { dir, backups, .. } => {
                assert_eq!(dir, PathBuf::from("x/planilhas"));
                assert_eq!(backups, PathBuf::from("y/copias"));
            }
            _ => panic!("esperava o comando atualizar"),
        }
    }
}
