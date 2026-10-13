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
    /// Último concurso do trecho de **busca** (busca de pesos, escolha de
    /// perfil e walk-forward); os concursos acima dele formam o conjunto
    /// **trancado**, inclusive os sorteados depois. Corte fixo pelo número do
    /// concurso (10% do histórico em 2026-10-05), não uma porcentagem que se
    /// move: `docs/PLANEJAMENTO.md`, 1c, "Conjunto final trancado". Use
    /// [`LoteriaConfig::trecho_do_concurso`] em vez de comparar à mão.
    pub ultimo_concurso_busca: u32,
    /// Perfil de pesos **ativo** da loteria: o que o `sync evidencia` usa por
    /// padrão (`docs/PLANEJAMENTO.md`, Fase 1d, item B). Nome do arquivo
    /// `perfis/<slug>/<nome>.toml`, sem a extensão. A busca de pesos (1.5.3) não
    /// trocou o perfil ativo: segue o `padrao_v1`.
    pub perfil_ativo: &'static str,
    /// Dias da semana em que a Caixa sorteia esta loteria (calendário da 3.2;
    /// funções em `calendario`). Os dias mudam com o tempo: `sync calendario`
    /// confere esta lista contra as datas dos últimos concursos do banco e
    /// avisa se ficou desatualizada. Em ordem de semana, sem repetição.
    pub dias_sorteio: &'static [DiaSemana],
    /// Horário de Brasília (UTC−3) a partir do qual vale procurar o resultado
    /// do dia: o sorteio é por volta das 20h; a folga cobre atraso e a
    /// publicação no site.
    pub procurar_a_partir_das: HoraLocal,
}

/// Dia da semana, de segunda a domingo (ordem ISO-8601).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiaSemana {
    Segunda,
    Terca,
    Quarta,
    Quinta,
    Sexta,
    Sabado,
    Domingo,
}

impl DiaSemana {
    /// Nome por extenso, em português, para mensagens.
    pub const fn nome(self) -> &'static str {
        match self {
            DiaSemana::Segunda => "segunda",
            DiaSemana::Terca => "terça",
            DiaSemana::Quarta => "quarta",
            DiaSemana::Quinta => "quinta",
            DiaSemana::Sexta => "sexta",
            DiaSemana::Sabado => "sábado",
            DiaSemana::Domingo => "domingo",
        }
    }
}

/// Hora do dia no relógio de Brasília (UTC−3 fixo; sem horário de verão desde
/// 2019).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoraLocal {
    pub hora: u32,
    pub minuto: u32,
}

impl HoraLocal {
    /// Segundos desde a meia-noite local.
    pub const fn segundos_do_dia(self) -> i64 {
        (self.hora * 3600 + self.minuto * 60) as i64
    }
}

/// A qual parte do histórico um concurso pertence, pelo corte fixo
/// `LoteriaConfig::ultimo_concurso_busca`. O padrão é a busca: o trancado só
/// é usado quando pedido explicitamente.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Trecho {
    /// Concurso ≤ corte: entra na busca de pesos, na escolha de perfil e no
    /// walk-forward.
    #[default]
    Busca,
    /// Concurso > corte: conjunto final trancado, fora da busca.
    Trancado,
}

impl Trecho {
    /// Nome estável, em minúsculas, para datasets e relatórios
    /// (`"busca"`/`"trancado"`).
    pub const fn nome(self) -> &'static str {
        match self {
            Trecho::Busca => "busca",
            Trecho::Trancado => "trancado",
        }
    }
}

/// Erro conhecido na planilha oficial da Caixa, corrigido antes de gravar.
/// Só é aplicado se a data recebida for exatamente `data_original`: se a
/// Caixa corrigir a planilha, a correção vira no-op; se a data mudar para
/// outro valor, é emitido um aviso (ver `correcoes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorrecaoData {
    pub loteria: &'static str,
    pub concurso: u32,
    pub data_original: &'static str,
    pub data_corrigida: &'static str,
    pub motivo: &'static str,
}

/// Lista de exceções conhecidas. Para adicionar uma, confirme a data correta
/// pelo calendário de sorteios da época (concursos vizinhos) e registre o
/// motivo; `sync validar` aponta candidatas como aviso de data fora de ordem.
/// No máximo uma correção por (loteria, concurso) — entradas não são
/// encadeadas. Não remova uma entrada enquanto a planilha mantiver o erro:
/// o próximo `baixar` gravaria a data errada de volta.
pub const CORRECOES_DATA: &[CorrecaoData] = &[CorrecaoData {
    loteria: "quina",
    concurso: 380,
    data_original: "1997-03-12",
    data_corrigida: "1998-03-12",
    motivo: "ano digitado errado na planilha: 379 é 1998-03-08 e 381 é \
             1998-03-15 (sorteios às quintas e domingos)",
}];

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
        ultimo_concurso_busca: 3416,
        perfil_ativo: "padrao_v1",
        // Dias vistos nos últimos 60 concursos do banco real em 2026-10-13
        // (`docs/PROGRESSO.md`, 3.2): domingo a sexta; o sábado aparece uma só
        // vez (2026-10-03, sorteio isolado).
        // Troca de sábado para domingo vista nos bancos em 2026: último sábado
        // regular em 2026-07-11, primeiro domingo regular em 2026-07-19 (antes
        // disso houve só um domingo isolado, 2026-06-14). O sábado de
        // 2026-10-03 é provável antecipação pela eleição de 2026-10-04 (não
        // confirmado).
        dias_sorteio: &[
            DiaSemana::Segunda,
            DiaSemana::Terca,
            DiaSemana::Quarta,
            DiaSemana::Quinta,
            DiaSemana::Sexta,
            DiaSemana::Domingo,
        ],
        procurar_a_partir_das: HoraLocal {
            hora: 21,
            minuto: 0,
        },
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
        ultimo_concurso_busca: 2759,
        perfil_ativo: "padrao_v1",
        // Dias vistos nos últimos 60 concursos do banco real em 2026-10-13
        // (`docs/PROGRESSO.md`, 3.2): terça, quinta e domingo; o sábado, que já
        // foi dia de sorteio, aparece uma só vez (2026-10-03, isolado).
        // Troca de sábado para domingo vista no banco em 2026: último sábado
        // regular em 2026-07-11, primeiro domingo regular em 2026-07-19 (antes
        // disso houve só domingos isolados, 2026-05-24 e 2026-06-14). O sábado
        // de 2026-10-03 é provável antecipação pela eleição de 2026-10-04 (não
        // confirmado).
        dias_sorteio: &[DiaSemana::Terca, DiaSemana::Quinta, DiaSemana::Domingo],
        procurar_a_partir_das: HoraLocal {
            hora: 21,
            minuto: 0,
        },
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
        ultimo_concurso_busca: 6421,
        perfil_ativo: "padrao_v1",
        // Dias vistos nos últimos 60 concursos do banco real em 2026-10-13
        // (`docs/PROGRESSO.md`, 3.2): domingo a sexta; o sábado aparece uma só
        // vez (2026-10-03, sorteio isolado).
        // Troca de sábado para domingo vista nos bancos em 2026: último sábado
        // regular em 2026-07-11, primeiro domingo regular em 2026-07-19 (antes
        // disso houve só um domingo isolado, 2026-06-14). O sábado de
        // 2026-10-03 é provável antecipação pela eleição de 2026-10-04 (não
        // confirmado).
        dias_sorteio: &[
            DiaSemana::Segunda,
            DiaSemana::Terca,
            DiaSemana::Quarta,
            DiaSemana::Quinta,
            DiaSemana::Sexta,
            DiaSemana::Domingo,
        ],
        procurar_a_partir_das: HoraLocal {
            hora: 21,
            minuto: 0,
        },
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

    /// Parte do histórico de `concurso`: [`Trecho::Busca`] até
    /// `ultimo_concurso_busca` (inclusive), [`Trecho::Trancado`] acima. Só
    /// depende do número do concurso, então serve igual à avaliação e ao
    /// dataset do Python (1.5.1).
    pub const fn trecho_do_concurso(&self, concurso: u32) -> Trecho {
        if concurso <= self.ultimo_concurso_busca {
            Trecho::Busca
        } else {
            Trecho::Trancado
        }
    }
}

pub fn buscar(slug: &str) -> Option<&'static LoteriaConfig> {
    LOTERIAS.iter().find(|l| l.slug.eq_ignore_ascii_case(slug))
}

/// Palavra aceita no lugar do slug para aplicar um comando a todas as
/// loterias configuradas (ex.: `sync validar todas`).
pub const TODAS: &str = "todas";

/// Indica se o argumento de loteria é `todas` (sem diferenciar maiúsculas,
/// como `buscar`).
pub fn eh_todas(argumento: &str) -> bool {
    argumento.eq_ignore_ascii_case(TODAS)
}

/// Resolve o argumento de loteria da CLI: `todas` vira todas as loterias, na
/// ordem de `LOTERIAS`; um slug conhecido vira só a sua; um desconhecido dá
/// `None`.
pub fn resolver(argumento: &str) -> Option<Vec<&'static LoteriaConfig>> {
    if eh_todas(argumento) {
        return Some(LOTERIAS.iter().collect());
    }
    buscar(argumento).map(|l| vec![l])
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
    fn nenhum_slug_colide_com_todas() {
        for l in LOTERIAS {
            assert!(!eh_todas(l.slug), "slug reservado: {}", l.slug);
        }
    }

    #[test]
    fn resolver_todas_devolve_todas_na_ordem_do_config() {
        let esperado: Vec<&str> = LOTERIAS.iter().map(|l| l.slug).collect();
        for argumento in ["todas", "TODAS", "Todas"] {
            let slugs: Vec<&str> = resolver(argumento)
                .expect("todas é aceito")
                .iter()
                .map(|l| l.slug)
                .collect();
            assert_eq!(slugs, esperado, "{argumento}");
        }
        assert_eq!(esperado, ["lotofacil", "megasena", "quina"]);
    }

    #[test]
    fn resolver_slug_devolve_so_a_loteria() {
        for l in LOTERIAS {
            let resolvidas = resolver(l.slug).expect("slug configurado");
            assert_eq!(resolvidas.len(), 1, "{}", l.slug);
            assert_eq!(resolvidas[0].slug, l.slug);
        }
        let mega = resolver("MegaSena").expect("sem diferenciar maiúsculas");
        assert_eq!(mega.len(), 1);
        assert_eq!(mega[0].slug, "megasena");
    }

    #[test]
    fn resolver_desconhecido_devolve_none() {
        for argumento in ["", "timemania", "toda", "todas ", "lotofacil,quina"] {
            assert!(resolver(argumento).is_none(), "{argumento:?}");
        }
        assert!(!eh_todas("lotofacil"));
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
    fn corte_da_busca_por_loteria() {
        // Decisão de 2026-10-05 (PLANEJAMENTO.md, 1c, tabela do trancado).
        for (slug, corte) in [("lotofacil", 3416), ("megasena", 2759), ("quina", 6421)] {
            let l = buscar(slug).expect("loteria configurada");
            assert_eq!(l.ultimo_concurso_busca, corte, "{slug}");
        }
    }

    #[test]
    fn trecho_do_concurso_separa_no_corte() {
        for l in LOTERIAS {
            let corte = l.ultimo_concurso_busca;
            assert_eq!(l.trecho_do_concurso(1), Trecho::Busca, "{}", l.slug);
            assert_eq!(l.trecho_do_concurso(corte - 1), Trecho::Busca, "{}", l.slug);
            assert_eq!(l.trecho_do_concurso(corte), Trecho::Busca, "{}", l.slug);
            assert_eq!(
                l.trecho_do_concurso(corte + 1),
                Trecho::Trancado,
                "{}",
                l.slug
            );
            assert_eq!(
                l.trecho_do_concurso(u32::MAX),
                Trecho::Trancado,
                "{}",
                l.slug
            );
        }
        assert_eq!(Trecho::default(), Trecho::Busca);
        assert_eq!(Trecho::Busca.nome(), "busca");
        assert_eq!(Trecho::Trancado.nome(), "trancado");
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

    #[test]
    fn perfil_ativo_e_o_padrao_v1_e_existe_em_perfis() {
        // O `sync evidencia` usa o perfil ativo por padrão (1d.3): tem de ser um
        // perfil válido da pasta `perfis/<loteria>/` do repositório.
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("perfis");
        for l in LOTERIAS {
            assert_eq!(l.perfil_ativo, "padrao_v1", "{}", l.slug);
            let carregado = crate::perfil::carregar(&raiz, l, l.perfil_ativo)
                .unwrap_or_else(|e| panic!("{}: {e}", l.slug));
            assert_eq!(carregado.perfil.loteria, l.slug);
        }
    }
}
