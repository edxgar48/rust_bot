/* FUNCIOAA PARCIALMENTE ,TESTEI , FUNCIONA MAIS PRECISA DE ALTERAÇÕES
ESTÁ SENDO DETECTADO COMO BOT E A PÁGINA EM DETERMINADO MOMENTO É BLOQUEADA

TEM UMA SUGESTÃO DO CLAUDE, QUE PRECISO TESTAR

-->>> AQUI ESTÁ  <<<---

// ... imports anteriores ...
use std::time::Duration;
use rand::Rng; // Adicione esta dependência no Cargo.toml

#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    let playwright = Playwright::initialize().await?;
    playwright.prepare()?;
    let chromium = playwright.chromium();
    
    // Configurar um user agent mais comum
    let context = chromium
        .launcher()
        .headless(false) // Mudar para false pode ajudar
        .launch()
        .await?
        .context_builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .viewport(Some(playwright::api::Viewport {
            width: 1920,
            height: 1080,
        }))
        .build()
        .await?;

    let page = context.new_page().await?;

    // Adicionar delays aleatórios para simular comportamento humano
    let mut rng = rand::thread_rng();
    
    // Navegar para a página com timeout maior
    page.goto_builder("https://loterias.caixa.gov.br/Paginas/Lotofacil.aspx")
        .timeout(Duration::from_secs(30))
        .goto()
        .await?;

    // Delay aleatório entre 2 e 5 segundos
    tokio::time::sleep(Duration::from_secs_f64(rng.gen_range(2.0..5.0))).await;

    // Simular movimento do mouse (opcional)
    page.mouse()
        .move_to(rng.gen_range(100.0..500.0), rng.gen_range(100.0..500.0), None)
        .await?;

    // Seu código de extração de dados aqui...
    // ... resto do código ...

    // Adicionar delay antes de fechar
    tokio::time::sleep(Duration::from_secs_f64(rng.gen_range(1.0..3.0))).await;

    browser.close().await?;
    Ok(())
}
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
    tokio::time::sleep(Duration::from_secs(5)).await; //diminui para 5 seg

    // Tenta diferentes seletores que podem estar presentes após o carregamento dinâmico
    let numeros = page
    .eval(r#"
        () => {
            const seletores = [
                //'li.ng-binding.dezena.ng-scope',
                //'simple-container.lista-dezenas.lotofacil',
                //'li.dezena',
                'ul.simple-container.lista-dezenas.lotofacil'
            ];

            for (const seletor of seletores) {
                const elementos = document.querySelectorAll(seletor);
                if (elementos.length > 0) {
                    return Array.from(elementos)
                        .map(el => el.textContent.trim())
                        .filter(texto => texto.includes('>') && texto.includes('<'))
                        .map(texto => texto.replace('>', '').replace('<', ''));
                }
            }
            
            // Fallback para busca geral
            return Array.from(document.querySelectorAll('*'))
                .map(el => el.textContent.trim())
                .filter(texto => texto.includes('>') && texto.includes('<'))
                .map(texto => texto.replace('>', '').replace('<', ''))
                .slice(0, 15);
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

1. Ajustar o tempo de sleep conforme necessário (35 não deu certo nesse caso , pois a página tem proteção para contra spiders)
2. Pode adicionar mais seletores no array `seletores` baseado na sua inspeção da página (presiso achar os seletores corretos, parce que a renderização do Javascript muda alguma coisa nas classes de seletores)
3. Pode implementar um sistema de retry caso a primeira tentativa falhe
4. Pode adicionar logging mais detalhado para debug (Não sei ainda do que se trata e preciso estudar isso)

*/
/* Algumas sugestões adicionais:

Adicione estas dependências no seu Cargo.toml:

tomlCopy[dependencies]
rand = "0.8"

Considere implementar algumas destas técnicas:

Rotacionar user agents
Adicionar headers comuns como Accept, Accept-Language
Implementar sistema de retry com backoff exponencial
Usar proxy rotation se necessário
Considerar usar stealth mode se disponível no Playwright Rust
*/