use playwright::Playwright;
#[tokio::main]
async fn main() -> Result<(), playwright::Error> {
    let playwright = Playwright::initialize().await?;
    playwright.prepare()?; // Install browsers
    let chromium = playwright.chromium();
    let browser = chromium.launcher().headless(true).launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;

    println!("Navigating to example.com...");
    page.goto_builder("https://example.com/").goto().await?;

    let s: String = page.eval("() => location.href").await?;
    println!("Page URL: {}", s);

    assert_eq!(s, "https://example.com/");
    println!("Clicking the link...");
    page.click_builder("a").click().await?;
    
    Ok(())
}
