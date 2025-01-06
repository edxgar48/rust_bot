use playwright::Playwright;

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    let playwright = Playwright::initialize().await?;
    playwright.prepare()?;
    let chromium = playwright.chromium();
    let browser = chromium.launcher().headless(true).launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;

    // Navegar para a página
    page.goto_builder("https://loterias.caixa.gov.br/Paginas/Lotofacil.aspx")
        .goto()
        .await?;
