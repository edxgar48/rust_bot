/* FUNCIOAA PARCIALMENTE PRECISO FAZER ALTEREAÇÔES, QUE ESTÃO AO FINAL DESTE CÓDIGO 
*/
use playwright::Playwright;
use std::time::Duration;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    // Initialize Playwright and Chromium browser
    let playwright = Playwright::initialize().await?;
    playwright.prepare()?;
    let chromium = playwright.chromium();
    
    // Configure browser with longer timeout and needed permissions
    let browser = chromium
        .launcher()
        .headless(true)
        .launch()
        .await?;
    
    let context = browser
        .context_builder()
        .viewport(Some(playwright::api::Viewport {
            width: 1920,
            height: 1080,
        }))
        .build()
        .await?;
    
    let page = context.new_page().await?;

    // Navigate to the page
    page.goto_builder("https://loterias.caixa.gov.br/Paginas/Lotofacil.aspx")
        .goto()
        .await?;

    // Aguarda um tempo fixo para garantir que o JavaScript execute
    tokio::time::sleep(Duration::from_secs(35)).await;

    // Tenta diferentes seletores que podem estar presentes após o carregamento dinâmico
    let numeros = page
        .eval(r#"
            () => {
                // Tenta diferentes padrões de seletores que podem existir
                const seletores = [
                    'div.resultado-loteria',
                    'div[id*="resultados"]',
                    'ul.numbers',
                    'div[class*="numero"]',
                    // Adicione mais seletores conforme necessário
                ];

                for (const seletor of seletores) {
                    const elementos = document.querySelectorAll(seletor);
                    if (elementos.length > 0) {
                        return Array.from(elementos)
                            .map(el => el.textContent.trim())
                            .filter(texto => texto.match(/^\d+$/));
                    }
                }
                
                // Se nenhum seletor funcionar, tenta buscar todos os números visíveis
                return Array.from(document.querySelectorAll('*'))
                    .map(el => el.textContent.trim())
                    .filter(texto => texto.match(/^\d+$/))
                    .slice(0, 15); // Limita aos primeiros 15 números encontrados
            }
        "#)
        .await?;

    // Exibe os números encontrados
    match numeros {
        Value::Array(arr) => {
            println!("Números encontrados:");
            for (i, numero) in arr.iter().enumerate() {
                if let Some(num) = numero.as_str() {
                    println!("Número {}: {}", i + 1, num);
                }
            }
        }
        _ => println!("Nenhum número encontrado ou formato inesperado"),
    }

    // Captura screenshot para debug
    page.screenshot_builder()
        .path("lottery-debug.png".into())
        .full_page(true)
        .screenshot()
        .await?;

    // Cleanup
    browser.close().await?;
   // playwright.stop().await?;

    Ok(())
}

/*
As principais mudanças nesta versão incluem:

1. Removemos o `wait_for_load_state` e substituímos por um `sleep` fixo
   - Isso garante que o JavaScript tenha tempo de executar
   - Não é a solução mais elegante, mas é mais confiável neste caso

2. JavaScript mais robusto:
   - Tenta múltiplos seletores diferentes
   - Inclui uma fallback strategy para buscar números em qualquer elemento
   - Filtra apenas texto que corresponde a números

3. Adicionei screenshot para debug:
   - Salva uma captura da página para você poder verificar como ela estava no momento da extração

4. Melhor configuração do browser:
   - Viewport definido
   - Melhor tratamento de erros

Para usar este código, você ainda precisará das mesmas dependências no `Cargo.toml`:

```toml
[dependencies]
playwright = "0.0.20"
tokio = { version = "1.0", features = ["full"] }
```

Algumas sugestões para melhorar ainda mais:

1. Você pode ajustar o tempo de sleep conforme necessário (35 não deu certo nesse caso , pois a página tem proteção para contra spiders)
2. Pode adicionar mais seletores no array `seletores` baseado na sua inspeção da página (presiso achar os seletores corretos, parce que a renderização do Javascript muda alguma coisa nas classes de seletores)
3. Pode implementar um sistema de retry caso a primeira tentativa falhe
4. Pode adicionar logging mais detalhado para debug (Não sei ainda do que se trata e preciso estudar isso)

*/
