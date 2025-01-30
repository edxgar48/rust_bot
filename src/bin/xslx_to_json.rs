use clap::{Arg, Command};
use calamine::{open_workbook, Reader, Xlsx};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

#[derive(Serialize)]
struct RowData {
    data: HashMap<String, String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configuração do CLI usando clap
    let matches = Command::new("xlsx_to_json")
        .version("1.0")
        .author("Seu Nome <seu.email@example.com>")
        .about("Extrai dados de um arquivo .xlsx e salva em formato JSON")
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .value_name("FILE")
                .help("Define o arquivo .xlsx de entrada")
                .required(true),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Define o arquivo JSON de saída")
                .required(true),
        )
        .arg(
            Arg::new("cells")
                .short('c')
                .long("cells")
                .value_name("CELLS")
                .help("Lista de colunas para extrair (ex: A,B,C)")
                .required(true),
        )
        .get_matches();

    // Obtém os argumentos da CLI
    let input_file = matches.value_of("input").unwrap();
    let output_file = matches.value_of("output").unwrap();
    let columns: Vec<&str> = matches.value_of("cells").unwrap().split(',').collect();

    // Abre o arquivo .xlsx
    let mut workbook: Xlsx<_> = open_workbook(input_file)?;
    let mut extracted_data = Vec::new();

    if let Some(Ok(sheet)) = workbook.worksheet_range_at(0) {
        for row_index in 0..sheet.height() {
            let mut row_data = RowData {
                data: HashMap::new(),
            };

            for column in &columns {
                if let Some(cell_value) = sheet.get_value((row_index, column_to_index(column))) {
                    row_data
                        .data
                        .insert(column.to_string(), cell_value.to_string());
                } else {
                    row_data.data.insert(column.to_string(), "".to_string());
                }
            }

            extracted_data.push(row_data);
        }
    } else {
        eprintln!("Não foi possível ler a planilha.");
        return Ok(());
    }

    // Serializa os dados para JSON
    let json_data = serde_json::to_string_pretty(&extracted_data)?;

    // Salva os dados no arquivo de saída
    let mut file = File::create(output_file)?;
    file.write_all(json_data.as_bytes())?;

    println!("Dados salvos com sucesso em {}", output_file);

    Ok(())
}

// Função auxiliar para converter colunas (A, B, C...) em índices numéricos (0, 1, 2...)
fn column_to_index(column: &str) -> usize {
    column
        .chars()
        .fold(0, |acc, c| acc * 26 + (c as usize - 'A' as usize))
}

/*
Entrada de Colunas (--cells):
Agora, o argumento --cells aceita apenas os nomes das colunas (ex: A,B,C) em vez de referências de células específicas (ex: A1,B2,C3).
Isso permite que o programa extraia os valores dessas colunas para cada linha da planilha.
Iteração por Linhas:
O programa itera sobre todas as linhas da planilha usando sheet.height() para determinar o número total de linhas.
Para cada linha, ele extrai os valores das colunas especificadas.
Função Auxiliar column_to_index:
Converte o nome da coluna (ex: A, B, C) em um índice numérico (ex: 0, 1, 2) para acessar os valores na planilha.
Estrutura de Dados:
Cada linha da planilha é representada como um objeto JSON contendo um HashMap com os valores das colunas especificadas.
Saída JSON:
Os dados são salvos como um array JSON, onde cada elemento corresponde a uma linha da planilha.
Exemplo de Uso
Suponha que o arquivo input.xlsx tenha os seguintes valores:

A	B	C
10	20	30
40	50	60
Se você executar o comando:


./target/release/xlsx_to_json -i input.xlsx -o output.json -c A,B,C
O arquivo output.json será:


[
  {
    "data": {
      "A": "10",
      "B": "20",
      "C": "30"
    }
  },
  {
    "data": {
      "A": "40",
      "B": "50",
      "C": "60"
    }
  }
]
*/