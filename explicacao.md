# Explicação do Código
CLI com clap:

* A biblioteca clap é usada para criar os comandos download e extract.

Comando download:

* Recebe a URL da página e o seletor CSS do link a ser clicado.
* Navega até a página e clica no link especificado.

Comando extract:

* Recebe a URL da página, um seletor CSS para localizar os elementos <li> e o caminho do arquivo JSON de saída.
* Extrai os textos dos elementos e os salva no arquivo JSON.

Execução de Comandos:

*O match garante que o comando correto seja executado com base na entrada do usuário.

JSON Output:

* Usa o serde_json para serializar os textos extraídos em formato JSON.

Dependências

* Adicione as seguintes dependências ao Cargo.toml:

```toml

[dependencies]
playwright = "0.0.18"
clap = { version = "4.5.23", features = ["derive"] }
serde = { version = "1.0.217", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.42.0", features = ["full"] }
```

Como Usar a CLI
Instalar navegadores do Playwright:

````bash
playwright install
```

Compilar o programa:

```bash
cargo build --release
```

Comando para Download:

```bash
./target/release/playwright_cli download https://example.com ".download-link"
```

Comando para Extrair e Salvar em JSON:

```bash
./target/release/playwright_cli extract https://example.com ".minha-classe li" output.json
```
JSON Exemplo

Se usar extract com sucesso, o arquivo output.json terá um conteúdo como:
    

````json
[
    "Item 1",
    "Item 2",
    "Item 3"
]
```
Melhorias Futuras

Tratamento de erros mais robusto para casos em que o seletor não encontra elementos.
Feedback visual com logs detalhados.