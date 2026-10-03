use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::parser::normalizar_data;
use playwright::api::page::{Event, EventType};
use playwright::api::Viewport;
use playwright::Playwright;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const TENTATIVAS_MAX: u32 = 3;
const TIMEOUT_SELETOR_MS: f64 = 20_000.0;

/// Abre a página da loteria, clica no link/botão de download e salva o
/// arquivo .htm resultante em `dir_destino`. Faz algumas tentativas com
/// backoff, já que o site é conhecido por bloquear acessos automatizados.
pub async fn baixar_resultado(
    config: &LoteriaConfig,
    seletor_override: Option<&str>,
    dir_destino: &Path,
    headless: bool,
) -> Result<PathBuf> {
    std::fs::create_dir_all(dir_destino).map_err(|e| SpiderError::Io {
        caminho: dir_destino.to_path_buf(),
        origem: e,
    })?;

    let seletor = seletor_override.unwrap_or(config.seletor_botao_download);

    let mut ultimo_erro = None;
    for tentativa in 1..=TENTATIVAS_MAX {
        match tentativa_unica(config, seletor, dir_destino, headless).await {
            Ok(caminho) => return Ok(caminho),
            Err(e) => {
                eprintln!(
                    "[{}] tentativa {}/{} falhou: {}",
                    config.slug, tentativa, TENTATIVAS_MAX, e
                );
                ultimo_erro = Some(e);
                if tentativa < TENTATIVAS_MAX {
                    tokio::time::sleep(Duration::from_secs(2 * tentativa as u64)).await;
                }
            }
        }
    }
    Err(ultimo_erro.expect("loop executa ao menos uma tentativa"))
}

async fn tentativa_unica(
    config: &LoteriaConfig,
    seletor: &str,
    dir_destino: &Path,
    headless: bool,
) -> Result<PathBuf> {
    let erro_navegador = |detalhe: String| SpiderError::Navegador {
        url: config.pagina_url.to_string(),
        detalhe,
    };

    let playwright = Playwright::initialize()
        .await
        .map_err(|e| erro_navegador(format!("falha ao iniciar o Playwright: {e}")))?;
    playwright
        .prepare()
        .map_err(|e| erro_navegador(format!("falha ao preparar navegadores: {e}")))?;

    let chromium = playwright.chromium();
    let browser = chromium
        .launcher()
        .headless(headless)
        .launch()
        .await
        .map_err(|e| erro_navegador(format!("falha ao abrir o navegador: {e}")))?;

    let context = browser
        .context_builder()
        .accept_downloads(true)
        .user_agent(USER_AGENT)
        .viewport(Some(Viewport {
            width: 1920,
            height: 1080,
        }))
        .build()
        .await
        .map_err(|e| erro_navegador(e.to_string()))?;

    let page = context
        .new_page()
        .await
        .map_err(|e| erro_navegador(e.to_string()))?;

    page.goto_builder(config.pagina_url)
        .goto()
        .await
        .map_err(|e| erro_navegador(e.to_string()))?;

    // Pequena pausa para o JS da página terminar de renderizar antes de procurar o botão.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    page.wait_for_selector_builder(seletor)
        .timeout(TIMEOUT_SELETOR_MS)
        .wait_for_selector()
        .await
        .map_err(|_| SpiderError::BotaoDownloadNaoEncontrado(seletor.to_string()))?;

    // Segue o mesmo padrão da documentação oficial do Playwright: assina o
    // evento de download e só então dispara o clique, concorrentemente, para
    // não perder o evento por causa de uma corrida entre os dois.
    let (evento, _) = tokio::try_join!(
        async {
            page.expect_event(EventType::Download)
                .await
                .map_err(|e| erro_navegador(e.to_string()))
        },
        async {
            page.click_builder(seletor)
                .click()
                .await
                .map_err(|e| erro_navegador(e.to_string()))
        },
    )?;

    let download = match evento {
        Event::Download(d) => d,
        _ => return Err(SpiderError::DownloadNaoIniciado(seletor.to_string())),
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // O botão se chama "download-resultado", mas o arquivo real que a Caixa
    // serve é uma planilha (.xlsx) — respeitamos a extensão que o próprio
    // navegador reconheceu em vez de supor .htm.
    let extensao = Path::new(download.suggested_filename())
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("xlsx")
        .to_string();
    let destino = dir_destino.join(format!("{}_{}.{}", config.slug, timestamp, extensao));

    download.save_as(&destino).await.map_err(|e| SpiderError::Io {
        caminho: destino.clone(),
        origem: std::io::Error::other(e.to_string()),
    })?;

    let _ = browser.close().await;

    Ok(destino)
}

/// Lê o último resultado exibido na própria página (sem baixar a planilha
/// inteira) — útil para atualizar a base com o concurso mais recente sem
/// reprocessar todo o histórico. Faz as mesmas tentativas com backoff que
/// `baixar_resultado`.
pub async fn raspar_ultimo_resultado(config: &LoteriaConfig, headless: bool) -> Result<Resultado> {
    let mut ultimo_erro = None;
    for tentativa in 1..=TENTATIVAS_MAX {
        match raspar_tentativa_unica(config, headless).await {
            Ok(resultado) => return Ok(resultado),
            Err(e) => {
                eprintln!(
                    "[{}] tentativa {}/{} falhou: {}",
                    config.slug, tentativa, TENTATIVAS_MAX, e
                );
                ultimo_erro = Some(e);
                if tentativa < TENTATIVAS_MAX {
                    tokio::time::sleep(Duration::from_secs(2 * tentativa as u64)).await;
                }
            }
        }
    }
    Err(ultimo_erro.expect("loop executa ao menos uma tentativa"))
}

async fn raspar_tentativa_unica(config: &LoteriaConfig, headless: bool) -> Result<Resultado> {
    let erro_navegador = |detalhe: String| SpiderError::Navegador {
        url: config.pagina_url.to_string(),
        detalhe,
    };

    let playwright = Playwright::initialize()
        .await
        .map_err(|e| erro_navegador(format!("falha ao iniciar o Playwright: {e}")))?;
    playwright
        .prepare()
        .map_err(|e| erro_navegador(format!("falha ao preparar navegadores: {e}")))?;

    let chromium = playwright.chromium();
    let browser = chromium
        .launcher()
        .headless(headless)
        .launch()
        .await
        .map_err(|e| erro_navegador(format!("falha ao abrir o navegador: {e}")))?;

    let context = browser
        .context_builder()
        .user_agent(USER_AGENT)
        .viewport(Some(Viewport {
            width: 1920,
            height: 1080,
        }))
        .build()
        .await
        .map_err(|e| erro_navegador(e.to_string()))?;

    let page = context
        .new_page()
        .await
        .map_err(|e| erro_navegador(e.to_string()))?;

    page.goto_builder(config.pagina_url)
        .goto()
        .await
        .map_err(|e| erro_navegador(e.to_string()))?;

    tokio::time::sleep(Duration::from_millis(1500)).await;

    page.wait_for_selector_builder(config.seletor_concurso_data)
        .timeout(TIMEOUT_SELETOR_MS)
        .wait_for_selector()
        .await
        .map_err(|_| SpiderError::ElementoNaoEncontrado(config.seletor_concurso_data.to_string()))?;

    // O elemento pode existir antes do Angular preencher o binding com os
    // dados reais (chega vazio por um instante) — tenta ler algumas vezes
    // em vez de falhar na primeira leitura em branco.
    let mut cabecalho_parseado = None;
    let mut ultimo_cabecalho_bruto = String::new();
    for _ in 0..10 {
        ultimo_cabecalho_bruto = page
            .inner_text(config.seletor_concurso_data, None)
            .await
            .map_err(|e| erro_navegador(e.to_string()))?;
        if let Some(par) = parsear_cabecalho_concurso(&ultimo_cabecalho_bruto) {
            cabecalho_parseado = Some(par);
            break;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    let (concurso, data_sorteio) = cabecalho_parseado.ok_or_else(|| {
        SpiderError::LinhaInvalida(format!(
            "cabeçalho de concurso em formato inesperado: {ultimo_cabecalho_bruto:?}"
        ))
    })?;

    let elementos_dezenas = page
        .query_selector_all(config.seletor_dezenas)
        .await
        .map_err(|e| erro_navegador(e.to_string()))?;

    if elementos_dezenas.len() != config.qtd_dezenas {
        return Err(SpiderError::LinhaInvalida(format!(
            "esperava {} dezenas no seletor `{}`, a página retornou {}",
            config.qtd_dezenas,
            config.seletor_dezenas,
            elementos_dezenas.len()
        )));
    }

    let mut dezenas = Vec::with_capacity(config.qtd_dezenas);
    for elemento in &elementos_dezenas {
        let texto = elemento.inner_text().await.map_err(|e| erro_navegador(e.to_string()))?;
        let valor: u32 = texto
            .trim()
            .parse()
            .map_err(|_| SpiderError::LinhaInvalida(format!("dezena inválida na página: {texto:?}")))?;
        dezenas.push(valor);
    }

    let _ = browser.close().await;

    Ok(Resultado {
        loteria: config.slug.to_string(),
        concurso,
        data_sorteio,
        dezenas,
    })
}

/// Extrai concurso e data de um texto como "Concurso 3794 (01/10/2026)".
fn parsear_cabecalho_concurso(texto: &str) -> Option<(u32, String)> {
    let texto = texto.trim();
    let resto = texto.strip_prefix("Concurso ")?;
    let (num_str, resto) = resto.split_once('(')?;
    let concurso: u32 = num_str.trim().parse().ok()?;
    let data_bruta = resto.trim_end().strip_suffix(')')?;
    let data_sorteio = normalizar_data(data_bruta)?;
    Some((concurso, data_sorteio))
}
