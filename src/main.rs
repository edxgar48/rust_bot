use playwright::Playwright;
//use std::time::Duration;

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

    // Aguarda sem o operador ?, já que wait_for_timeout retorna ()
    page.wait_for_timeout(5000.0).await;

    // Tenta diferentes seletores para encontrar os números
    let valores: Vec<String> = page
        .eval(
            r#"
            () => {
                // Tenta diferentes estratégias de seleção
                const dezenas = document.querySelectorAll('.dezena');
                if (dezenas.length > 0) {
                    return Array.from(dezenas).map(d => d.textContent.trim());
                }

                // Segunda tentativa com seletor mais específico
                const dezenasAlt = document.querySelectorAll('div[class*="dezena"]');
                if (dezenasAlt.length > 0) {
                    return Array.from(dezenasAlt).map(d => d.textContent.trim());
                }

                // Terceira tentativa, procurando qualquer elemento com número
                const resultado = document.querySelector('#ulDezenas');
                if (resultado) {
                    return Array.from(resultado.children).map(d => d.textContent.trim());
                }

                return [];
            }
            "#,
        )
        .await?;

    println!("Valores extraídos:");
    for (i, valor) in valores.iter().enumerate() {
        println!("Valor {}: {}", i + 1, valor);
    }

    // Debug: imprimir o HTML da página
    let html_content = page.content().await?;
    println!("\nEstrutura da página:");
    println!("{}", html_content);

    // Debug: tentar encontrar elementos que contêm números
    let debug_info = page
        .eval(
            r#"
            () => {
                const elements = document.querySelectorAll('*');
                const numerosElements = Array.from(elements).filter(el => 
                    el.textContent && /\d{2}/.test(el.textContent.trim())
                ).map(el => ({
                    tag: el.tagName,
                    classes: el.className,
                    id: el.id,
                    texto: el.textContent.trim()
                }));
                return JSON.stringify(numerosElements, null, 2);
            }
            "#,
        )
        .await?;
    
    println!("\nElementos encontrados com números:");
    println!("{}", debug_info);

    // Fecha o navegador
    browser.close().await?;

    Ok(())
}