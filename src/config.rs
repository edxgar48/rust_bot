/// Configuração de uma loteria: onde buscar o arquivo/resultado e como
/// interpretar os dados.
///
/// Todos os seletores e o layout de colunas abaixo foram confirmados contra
/// o site real em 2026-10-01/02. Importante: `seletor_concurso_data` e
/// `seletor_dezenas` usam estratégias diferentes por loteria porque a Caixa
/// ainda não terminou de migrar o template das páginas — Lotofácil usa o
/// AngularJS antigo (classes `ng-*`), Mega-Sena e Quina já usam IDs fixos.
#[derive(Debug, Clone, Copy)]
pub struct LoteriaConfig {
    pub slug: &'static str,
    pub nome_exibicao: &'static str,
    pub pagina_url: &'static str,
    /// Botão que dispara o download da planilha completa de resultados.
    pub seletor_botao_download: &'static str,
    /// Elemento cujo texto é algo como "Concurso 3794 (01/10/2026)".
    pub seletor_concurso_data: &'static str,
    /// Seletor que casa com cada `<li>` de dezena do último resultado, na página (não na planilha).
    pub seletor_dezenas: &'static str,
    pub qtd_dezenas: usize,
    pub coluna_concurso: usize,
    pub coluna_data: usize,
    pub primeira_coluna_dezena: usize,
}

pub const LOTERIAS: &[LoteriaConfig] = &[
    LoteriaConfig {
        slug: "lotofacil",
        nome_exibicao: "Lotofácil",
        pagina_url: "https://loterias.caixa.gov.br/Paginas/Lotofacil.aspx",
        seletor_botao_download: "#btnResultados",
        seletor_concurso_data: ".title-bar h2 span.ng-binding",
        seletor_dezenas: "ul.simple-container.lista-dezenas.lotofacil li.dezena",
        qtd_dezenas: 15,
        coluna_concurso: 0,
        coluna_data: 1,
        primeira_coluna_dezena: 2,
    },
    LoteriaConfig {
        slug: "megasena",
        nome_exibicao: "Mega-Sena",
        pagina_url: "https://loterias.caixa.gov.br/Paginas/Mega-Sena.aspx",
        seletor_botao_download: "#btnResultados",
        seletor_concurso_data: "#tituloResultadoConcurso",
        seletor_dezenas: "#ulDezenas li",
        qtd_dezenas: 6,
        coluna_concurso: 0,
        coluna_data: 1,
        primeira_coluna_dezena: 2,
    },
    LoteriaConfig {
        slug: "quina",
        nome_exibicao: "Quina",
        pagina_url: "https://loterias.caixa.gov.br/Paginas/Quina.aspx",
        seletor_botao_download: "#btnResultados",
        seletor_concurso_data: "#tituloResultadoConcurso",
        seletor_dezenas: "#ulDezenas li",
        qtd_dezenas: 5,
        coluna_concurso: 0,
        coluna_data: 1,
        primeira_coluna_dezena: 2,
    },
];

pub fn buscar(slug: &str) -> Option<&'static LoteriaConfig> {
    LOTERIAS.iter().find(|l| l.slug.eq_ignore_ascii_case(slug))
}

pub fn slugs_disponiveis() -> String {
    LOTERIAS.iter().map(|l| l.slug).collect::<Vec<_>>().join(", ")
}
