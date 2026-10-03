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
    /// Quantidade de dezenas sorteadas por concurso.
    pub qtd_dezenas: usize,
    /// Menor dezena que pode ser sorteada (inclusiva). Mesmo tipo de
    /// `Resultado::dezenas`.
    pub dezena_min: u32,
    /// Maior dezena que pode ser sorteada (inclusiva).
    pub dezena_max: u32,
    /// Layout do volante; `volante.casas()` cobre exatamente a faixa de dezenas.
    pub volante: Volante,
    pub coluna_concurso: usize,
    pub coluna_data: usize,
    pub primeira_coluna_dezena: usize,
}

/// Grade do volante impresso: as dezenas aparecem em ordem crescente,
/// preenchendo linha por linha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volante {
    pub linhas: u32,
    pub colunas: u32,
}

impl Volante {
    /// Total de casas da grade (`linhas * colunas`).
    pub const fn casas(&self) -> u32 {
        self.linhas * self.colunas
    }
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
        dezena_min: 1,
        dezena_max: 25,
        volante: Volante {
            linhas: 5,
            colunas: 5,
        },
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
        dezena_min: 1,
        dezena_max: 60,
        volante: Volante {
            linhas: 6,
            colunas: 10,
        },
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
        dezena_min: 1,
        dezena_max: 80,
        volante: Volante {
            linhas: 8,
            colunas: 10,
        },
        coluna_concurso: 0,
        coluna_data: 1,
        primeira_coluna_dezena: 2,
    },
];

impl LoteriaConfig {
    /// Quantidade de dezenas possíveis na faixa (`dezena_max - dezena_min + 1`).
    pub const fn qtd_dezenas_possiveis(&self) -> u32 {
        self.dezena_max - self.dezena_min + 1
    }

    /// Faixa inclusiva de dezenas sorteáveis (`dezena_min..=dezena_max`).
    pub const fn dezenas(&self) -> std::ops::RangeInclusive<u32> {
        self.dezena_min..=self.dezena_max
    }

    /// Indica se `dezena` está dentro da faixa sorteável desta loteria.
    pub const fn contem(&self, dezena: u32) -> bool {
        dezena >= self.dezena_min && dezena <= self.dezena_max
    }

    /// Posição `(linha, coluna)` da dezena no volante, ambas a partir de 0,
    /// ou `None` se a dezena está fora da faixa. Segue a convenção de
    /// `Volante`: ordem crescente preenchendo linha por linha (na Mega-Sena,
    /// 1..=10 é a linha 0 e 11 fica na linha 1, coluna 0).
    pub const fn posicao_no_volante(&self, dezena: u32) -> Option<(u32, u32)> {
        if !self.contem(dezena) {
            return None;
        }
        let indice = dezena - self.dezena_min;
        Some((indice / self.volante.colunas, indice % self.volante.colunas))
    }
}

pub fn buscar(slug: &str) -> Option<&'static LoteriaConfig> {
    LOTERIAS.iter().find(|l| l.slug.eq_ignore_ascii_case(slug))
}

pub fn slugs_disponiveis() -> String {
    LOTERIAS
        .iter()
        .map(|l| l.slug)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn faixa_de_dezenas_e_valida() {
        for l in LOTERIAS {
            assert!(
                l.dezena_min <= l.dezena_max,
                "{}: dezena_min > dezena_max",
                l.slug
            );
        }
    }

    #[test]
    fn volante_cobre_exatamente_a_faixa() {
        for l in LOTERIAS {
            assert_eq!(
                l.volante.casas(),
                l.dezena_max - l.dezena_min + 1,
                "{}: linhas x colunas difere do tamanho da faixa",
                l.slug
            );
            assert_eq!(l.volante.casas(), l.qtd_dezenas_possiveis(), "{}", l.slug);
        }
    }

    #[test]
    fn qtd_dezenas_cabe_na_faixa() {
        for l in LOTERIAS {
            assert!(l.qtd_dezenas > 0, "{}: qtd_dezenas zero", l.slug);
            assert!(
                l.qtd_dezenas as u32 <= l.qtd_dezenas_possiveis(),
                "{}: qtd_dezenas maior que a faixa",
                l.slug
            );
        }
    }

    #[test]
    fn slugs_sao_unicos() {
        let mut vistos = HashSet::new();
        for l in LOTERIAS {
            assert!(
                vistos.insert(l.slug.to_ascii_lowercase()),
                "slug duplicado: {}",
                l.slug
            );
        }
    }

    #[test]
    fn valores_esperados_por_loteria() {
        let esperado = [
            ("lotofacil", 1, 25, 5, 5),
            ("megasena", 1, 60, 6, 10),
            ("quina", 1, 80, 8, 10),
        ];
        for (slug, min, max, linhas, colunas) in esperado {
            let l = buscar(slug).expect("loteria configurada");
            assert_eq!((l.dezena_min, l.dezena_max), (min, max), "{slug}");
            assert_eq!(l.volante, Volante { linhas, colunas }, "{slug}");
        }
    }

    #[test]
    fn volante_preenche_linha_por_linha_em_ordem_crescente() {
        let mega = buscar("megasena").expect("loteria configurada");
        assert_eq!(mega.posicao_no_volante(1), Some((0, 0)));
        assert_eq!(mega.posicao_no_volante(10), Some((0, 9)));
        assert_eq!(mega.posicao_no_volante(11), Some((1, 0)));
        assert_eq!(mega.posicao_no_volante(60), Some((5, 9)));
        assert_eq!(mega.posicao_no_volante(0), None);
        assert_eq!(mega.posicao_no_volante(61), None);

        let lotofacil = buscar("lotofacil").expect("loteria configurada");
        assert_eq!(lotofacil.posicao_no_volante(6), Some((1, 0)));
        assert_eq!(lotofacil.posicao_no_volante(25), Some((4, 4)));

        let quina = buscar("quina").expect("loteria configurada");
        assert_eq!(quina.posicao_no_volante(80), Some((7, 9)));
    }

    #[test]
    fn cada_casa_do_volante_tem_exatamente_uma_dezena() {
        for l in LOTERIAS {
            let casas: HashSet<(u32, u32)> = l
                .dezenas()
                .filter_map(|d| l.posicao_no_volante(d))
                .collect();
            assert_eq!(casas.len() as u32, l.volante.casas(), "{}", l.slug);
            assert!(
                casas
                    .iter()
                    .all(|&(lin, col)| lin < l.volante.linhas && col < l.volante.colunas),
                "{}: posição fora da grade",
                l.slug
            );
        }
    }

    #[test]
    fn dezenas_percorre_a_faixa_inteira() {
        for l in LOTERIAS {
            let faixa: Vec<u32> = l.dezenas().collect();
            assert_eq!(faixa.len() as u32, l.qtd_dezenas_possiveis(), "{}", l.slug);
            assert_eq!(faixa.first(), Some(&l.dezena_min), "{}", l.slug);
            assert_eq!(faixa.last(), Some(&l.dezena_max), "{}", l.slug);
        }
    }

    #[test]
    fn contem_respeita_as_bordas() {
        for l in LOTERIAS {
            assert!(l.contem(l.dezena_min), "{}: min", l.slug);
            assert!(l.contem(l.dezena_max), "{}: max", l.slug);
            // checked_sub: uma loteria futura com dezena_min = 0 não deve estourar.
            if let Some(abaixo) = l.dezena_min.checked_sub(1) {
                assert!(!l.contem(abaixo), "{}: min - 1", l.slug);
            }
            assert!(!l.contem(l.dezena_max + 1), "{}: max + 1", l.slug);
        }
    }
}
