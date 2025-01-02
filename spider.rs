use clap::{Parser, Subcommand};
use playwright::Playwright;
use serde_json::json;
use std::{error::Error, fs::File, io::Write};

/// Estrutura da CLI
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Clica em um link específico para fazer o download : hhtps://loterias.ciaxa.gov.br/style%20library/js/webparts/download-resultado.js
    /// PRESICO MUDAR ISSO --->>> pois se trata de um botão que renderiza um javascript, como está  no link acima 

    ///  ---->>>> IMPORTANTE PRECISO AXEXAR TRATAMENTOS DE ERRO AQUI <<<<-----

    Download {
        /// URL da página onde está o link
        url: String,
        /// Seletor CSS do link para clicar
        selector: String,
    },
    /// Extrai elementos de uma classe e salva em JSON
    Extract {
       
        /// --->>>> TALVÉS EU POSSA ADAPTAR ESSE CÓDIGO PARA EXTRAIR LOCALMENTE DE UM ARQUIVO HTML BAIXADO, SE ACASO ESSE CÓDIOGA ABAIXO NÃO FUNCIONAR <<<<----
        
        /// URL da página para extrair os elementos : https://loterias.caixa.gov.br/Paginas/Lotofacil.aspx
        

            /// em javascript seria algo assim : 
            /// const ulElement = document.querySelector('ul.simple-container.lista-dezenas.lotofacil');
            /// const liElements = ulElement.querySelectorAll('li.ng-binding.dezena.ng-scope');

        url: String,
        /// Classe CSS de onde os elementos serão extraídos
        class_selector: String,
        /// Caminho do arquivo JSON de saída
        output: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Inicializa o Playwright
    let playwright = Playwright::initialize().await?;
    let chromium = playwright.chromium();
    let browser = chromium.launcher().headless(true).launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;

    match cli.command {
        Commands::Download { url, selector } => {
            println!("Navegando para {}", url);
            page.goto(&url).await?;

            println!("Clicando no seletor {}", selector);
            if let Some(link) = page.query_selector(&selector).await? {
                link.click().await?;
                println!("Link clicado com sucesso!");
            } else {
                eprintln!("Nenhum elemento encontrado com o seletor: {}", selector);
            }
        }
        Commands::Extract { url, class_selector, output } => {
            println!("Navegando para {}", url);
            page.goto(&url).await?;

            println!("Extraindo elementos da classe: {}", class_selector);
            let elements = page.query_selector_all(&class_selector).await?;
            let mut results = Vec::new();

            for element in elements {
                if let Some(text) = element.inner_text().await? {
                    results.push(text);
                }
            }

            println!("Salvando os resultados no arquivo: {}", output);
            let json_output = json!(results);
            let mut file = File::create(output)?;
            file.write_all(json_output.to_string().as_bytes())?;
        }
    }

    // Fecha o navegador
    browser.close().await?;
    Ok(())
}
