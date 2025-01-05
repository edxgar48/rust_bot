use playwright::Playwright;

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    // Inicializa o Playwright e o navegador Chromium
    let playwright = Playwright::initialize().await?;
    playwright.prepare()?; // Instala navegadores
    let chromium = playwright.chromium();
    let browser = chromium.launcher().headless(true).launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;

    // Navegar para a página
    page.goto_builder("https://loterias.caixa.gov.br/Paginas/Lotofacil.aspx").goto().await?;

    //page.wait_for_timeout(5000).await?;
    //page.wait_for_timeout(5000).await;
    page.wait_for_timeout(5000.0).await;// dessa maneira ele compila sem erro até aqui
    
    //page.screenshot("screenshot.png").await?; <<-- ISSO não funciona para verficação doque está sendo apresentado até o momento dos 5 sec. -->>

    // Espera pela presença da <ul>
    page.wait_for_selector("ul.simple-container lista-dezenas lotofacil").await?;

    // Extrai os valores de todos os <li> dentro da <ul>
    let valores: Vec<String> = page
        .eval(
            r#"
            () => {
                // Encontra a <ul> pela classe 'simple-container lista-dezenas lotofacil'
                const ul = document.querySelector('ul.simple-container lista-dezenas lotofacil');
                if (!ul) return []; // Retorna vazio se a <ul> não existir
                
                // Coleta os valores dos <li> filhos
                const items = ul.querySelectorAll('li.ng-binding dezena ng-scope');
                return Array.from(items).map(item => item.textContent.trim());
            }
            "#,
        )
        .await?;

    // Exibe os valores extraídos no console
    println!("Valores extraídos:");
    for (i, valor) in valores.iter().enumerate() {
        println!("Valor {}: {}", i + 1, valor);
    }

    // Verifica se encontrou os 15 valores esperados
    assert_eq!(valores.len(), 15, "O número de valores extraídos não é 15!");

    Ok(())
}
