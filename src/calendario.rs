//! Calendário de sorteios (tarefa 3.2): funções puras, sem I/O nem SQL, sobre
//! o calendário de cada loteria que mora em `config.rs`
//! (`LoteriaConfig::dias_sorteio` e `procurar_a_partir_das`).
//!
//! Responde a quatro perguntas, que a 3.3 (retentativas) e a 3.6 (alertas)
//! consomem:
//!
//! 1. hoje é dia de sorteio e desde que horário vale procurar o resultado?
//!    ([`consultar_dia`])
//! 2. o calendário configurado ainda bate com os dias dos últimos concursos do
//!    banco? ([`conferir`])
//! 3. dado o último concurso do banco e o instante de agora, está faltando um
//!    concurso que já deveria existir? ([`situacao`])
//!
//! # Fuso e unidades
//!
//! Brasília fixo em UTC−3 (sem horário de verão desde 2019), sem crate de
//! calendário, como `tempo`. Instantes são segundos Unix (`i64`); um "dia" é o
//! número de dias desde 1970-01-01 **no relógio de Brasília** (por isso 23h30
//! em Brasília, que já é o dia seguinte em UTC, ainda conta como o dia de
//! Brasília). As datas dos concursos (`Resultado::data_sorteio`) são datas
//! civis de Brasília, sem horário.
//!
//! Quem lê o relógio é a CLI; aqui o "agora" sempre entra por parâmetro.

use crate::config::{DiaSemana, LoteriaConfig, SorteioEspecial};
use crate::model::Resultado;
use crate::tempo;
use std::fmt;

/// Brasília em relação ao UTC, em segundos (UTC−3).
pub const FUSO_SEGUNDOS: i64 = -3 * 3600;

const SEGUNDOS_POR_DIA: i64 = 86_400;

/// Janela da [`conferir`], em dias corridos (8 semanas), contada para trás a
/// partir da data do último concurso, inclusive. Oito semanas cheias dão
/// exatamente oito ocorrências de cada dia da semana: um feriado não esvazia o
/// dia, mas uma mudança de calendário aparece em menos de dois meses.
pub const JANELA_CONFERENCIA_DIAS: i64 = 56;

/// Dia de Brasília (dias desde 1970-01-01) do instante `segundos` Unix.
pub fn dia_local(segundos: i64) -> i64 {
    (segundos + FUSO_SEGUNDOS).div_euclid(SEGUNDOS_POR_DIA)
}

/// Dia da semana do dia `dias` (desde 1970-01-01, que foi uma quinta-feira).
pub fn dia_da_semana(dias: i64) -> DiaSemana {
    match (dias + 3).rem_euclid(7) {
        0 => DiaSemana::Segunda,
        1 => DiaSemana::Terca,
        2 => DiaSemana::Quarta,
        3 => DiaSemana::Quinta,
        4 => DiaSemana::Sexta,
        5 => DiaSemana::Sabado,
        _ => DiaSemana::Domingo,
    }
}

/// `true` se a Caixa sorteia a loteria nesse dia da semana, pelo calendário
/// configurado.
pub fn eh_dia_de_sorteio(cfg: &LoteriaConfig, dias: i64) -> bool {
    cfg.dias_sorteio.contains(&dia_da_semana(dias))
}

/// Instante (segundos Unix) a partir do qual vale procurar o resultado do dia
/// `dias`; `None` se não é dia de sorteio. O próprio instante já vale
/// ("a partir de").
pub fn procurar_desde(cfg: &LoteriaConfig, dias: i64) -> Option<i64> {
    eh_dia_de_sorteio(cfg, dias).then(|| {
        dias * SEGUNDOS_POR_DIA + cfg.procurar_a_partir_das.segundos_do_dia() - FUSO_SEGUNDOS
    })
}

/// Resposta para (loteria, dia): é dia de sorteio e desde quando procurar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsultaDia {
    /// `AAAA-MM-DD`.
    pub data: String,
    pub dia_da_semana: DiaSemana,
    pub eh_dia_de_sorteio: bool,
    /// Segundos Unix; `None` se não é dia de sorteio. Mostre com
    /// [`instante_brasilia`].
    pub procurar_desde: Option<i64>,
}

/// A saída da 3.2 para (loteria, dia): é a função que a 3.3 e a 3.6 chamam.
pub fn consultar_dia(cfg: &LoteriaConfig, dias: i64) -> ConsultaDia {
    ConsultaDia {
        data: tempo::data_iso_de_dias(dias),
        dia_da_semana: dia_da_semana(dias),
        eh_dia_de_sorteio: eh_dia_de_sorteio(cfg, dias),
        procurar_desde: procurar_desde(cfg, dias),
    }
}

/// Sorteio especial cuja janela de parada contém o dia `dias` (tarefa 3.6):
/// dentro dela a loteria pode estar sem concursos regulares e o `Atrasado` é
/// aviso, não falha. Olha o ano do dia e os vizinhos, porque a janela da Mega
/// da Virada atravessa a virada do ano.
pub fn especial_em(cfg: &LoteriaConfig, dias: i64) -> Option<&'static SorteioEspecial> {
    let (ano, _, _) = tempo::data_civil(dias);
    cfg.sorteios_especiais.iter().find(|e| {
        (ano - 1..=ano + 1).any(|a| {
            let data = tempo::dias_desde_epoca(a, e.mes, e.dia);
            dias >= data - e.dias_antes && dias <= data + e.dias_depois
        })
    })
}

/// Instante em texto no relógio de Brasília (`2026-10-13T21:00:00-03:00`).
pub fn instante_brasilia(segundos: i64) -> String {
    // Desloca o instante para o relógio local e reaproveita o formatador UTC,
    // trocando o `Z` pelo deslocamento. Antes de 1970 não há instante útil.
    let local = u64::try_from(segundos + FUSO_SEGUNDOS).unwrap_or(0);
    let utc = tempo::iso8601_utc(local);
    format!("{}-03:00", utc.trim_end_matches('Z'))
}

// ---------------------------------------------------------------------------
// Conferência do calendário contra o banco
// ---------------------------------------------------------------------------

/// Divergência entre o calendário configurado e os concursos do banco: o
/// calendário provavelmente ficou desatualizado. É aviso, nunca erro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvisoCalendario {
    /// Dia do calendário sem nenhum concurso na janela (a Caixa pode ter
    /// parado de sortear nesse dia).
    DiaSemConcurso { dia_da_semana: DiaSemana },
    /// Dia fora do calendário com dois ou mais concursos na janela (a Caixa
    /// pode ter passado a sortear nesse dia). `datas` em ordem, uma por
    /// concurso.
    DiaForaDoCalendario {
        dia_da_semana: DiaSemana,
        datas: Vec<String>,
    },
}

impl fmt::Display for AvisoCalendario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AvisoCalendario::DiaSemConcurso { dia_da_semana } => write!(
                f,
                "calendário possivelmente desatualizado: nenhum concurso em {} nas últimas \
                 {} semanas, mas {} está nos dias de sorteio configurados",
                dia_da_semana.nome(),
                JANELA_CONFERENCIA_DIAS / 7,
                dia_da_semana.nome()
            ),
            AvisoCalendario::DiaForaDoCalendario {
                dia_da_semana,
                datas,
            } => write!(
                f,
                "calendário possivelmente desatualizado: {} concursos em {} nas últimas {} \
                 semanas ({}), mas {} não está nos dias de sorteio configurados",
                datas.len(),
                dia_da_semana.nome(),
                JANELA_CONFERENCIA_DIAS / 7,
                datas.join(", "),
                dia_da_semana.nome()
            ),
        }
    }
}

const TODOS_OS_DIAS: [DiaSemana; 7] = [
    DiaSemana::Segunda,
    DiaSemana::Terca,
    DiaSemana::Quarta,
    DiaSemana::Quinta,
    DiaSemana::Sexta,
    DiaSemana::Sabado,
    DiaSemana::Domingo,
];

/// Confere `cfg.dias_sorteio` contra as datas de `resultados` (qualquer ordem).
///
/// A janela são os [`JANELA_CONFERENCIA_DIAS`] dias que terminam na data do
/// último concurso (a maior data), inclusive. Avisos, nesta ordem (dias do
/// calendário na ordem do config, depois dias de fora na ordem da semana):
///
/// - [`AvisoCalendario::DiaSemConcurso`]: um dia do calendário sem nenhum
///   concurso na janela;
/// - [`AvisoCalendario::DiaForaDoCalendario`]: um dia fora do calendário com
///   **2 ou mais datas** distintas na janela. Uma só data (Mega da Virada,
///   sorteio remarcado, dois concursos especiais no mesmo dia) não avisa.
///
/// Banco com menos de 56 dias de histórico: a janela começa na data do
/// primeiro concurso, e só se cobra dia do calendário que ocorre dentro dela.
/// Datas que não são `AAAA-MM-DD` válidas são ignoradas (o `validar` as
/// reprova). Sem datas válidas, sem avisos.
pub fn conferir(cfg: &LoteriaConfig, resultados: &[Resultado]) -> Vec<AvisoCalendario> {
    let mut dias_dos_concursos: Vec<i64> = resultados
        .iter()
        .filter_map(|r| tempo::dias_da_data_iso(&r.data_sorteio))
        .collect();
    dias_dos_concursos.sort_unstable();
    let (Some(&primeiro), Some(&ultimo)) = (dias_dos_concursos.first(), dias_dos_concursos.last())
    else {
        return Vec::new();
    };
    let inicio = (ultimo - (JANELA_CONFERENCIA_DIAS - 1)).max(primeiro);
    let na_janela: Vec<i64> = dias_dos_concursos
        .into_iter()
        .filter(|&d| d >= inicio)
        .collect();

    let mut avisos = Vec::new();
    for &dia_cfg in cfg.dias_sorteio {
        let ocorre_na_janela = (inicio..=ultimo).any(|d| dia_da_semana(d) == dia_cfg);
        let teve_concurso = na_janela.iter().any(|&d| dia_da_semana(d) == dia_cfg);
        if ocorre_na_janela && !teve_concurso {
            avisos.push(AvisoCalendario::DiaSemConcurso {
                dia_da_semana: dia_cfg,
            });
        }
    }
    for dia_semana in TODOS_OS_DIAS {
        if cfg.dias_sorteio.contains(&dia_semana) {
            continue;
        }
        // `na_janela` está ordenada: `dedup` conta datas, não concursos.
        let mut dias: Vec<i64> = na_janela
            .iter()
            .copied()
            .filter(|&d| dia_da_semana(d) == dia_semana)
            .collect();
        dias.dedup();
        let datas: Vec<String> = dias.into_iter().map(tempo::data_iso_de_dias).collect();
        if datas.len() >= 2 {
            avisos.push(AvisoCalendario::DiaForaDoCalendario {
                dia_da_semana: dia_semana,
                datas,
            });
        }
    }
    avisos
}

// ---------------------------------------------------------------------------
// Situação do último concurso (tolerância)
// ---------------------------------------------------------------------------

/// Se há concurso esperado que ainda não está no banco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoCalendario {
    /// Nenhum concurso esperado faltando.
    EmDia,
    /// Falta um concurso esperado, mas ainda dentro da tolerância: é aviso
    /// (feriado, adiamento, Mega da Virada, site atrasado), não falha.
    Aguardando,
    /// Passou da tolerância: é o que a 3.6 conta como falha.
    Atrasado,
}

impl EstadoCalendario {
    /// Nome estável para saída e logs (`em_dia`, `aguardando`, `atrasado`).
    pub const fn nome(self) -> &'static str {
        match self {
            EstadoCalendario::EmDia => "em_dia",
            EstadoCalendario::Aguardando => "aguardando",
            EstadoCalendario::Atrasado => "atrasado",
        }
    }
}

/// Resultado de [`situacao`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Situacao {
    pub estado: EstadoCalendario,
    /// Data (`AAAA-MM-DD`) do primeiro sorteio esperado e ausente; `None` se
    /// `EmDia`.
    pub esperado_em: Option<String>,
    /// Segundos Unix a partir dos quais `Aguardando` vira `Atrasado` (o
    /// horário de procura do sorteio seguinte ao esperado); `None` se `EmDia`.
    pub atrasado_a_partir_de: Option<i64>,
}

/// Estado do banco frente ao calendário, dado o dia do último concurso
/// (`dia_ultimo`, de `tempo::dias_da_data_iso`) e o instante `agora`.
///
/// Definição (sorteios "depois" são os de dia **estritamente posterior** a
/// `dia_ultimo`; o dia do último concurso nunca é esperado de novo):
///
/// - `D1` = primeiro dia de sorteio depois de `dia_ultimo`; `D2` = o seguinte.
///   O horário de procura de um dia vale a partir do próprio instante
///   ([`procurar_desde`], inclusive).
/// - `EmDia`: `agora` < procura de `D1` (o sorteio de `D1` ainda não podia
///   estar publicado).
/// - `Aguardando`: procura de `D1` <= `agora` < procura de `D2` (o concurso de
///   `D1` já devia existir e não existe, mas o prazo é o próximo dia de
///   sorteio).
/// - `Atrasado`: `agora` >= procura de `D2`.
///
/// O último concurso pode cair fora do calendário (sorteio especial): vale
/// igual, `D1` é o primeiro dia de sorteio depois dele. Calendário sem nenhum
/// dia de sorteio: sempre `EmDia`.
pub fn situacao(cfg: &LoteriaConfig, dia_ultimo: i64, agora: i64) -> Situacao {
    let mut proximos = (dia_ultimo + 1..=dia_ultimo + 14).filter(|&d| eh_dia_de_sorteio(cfg, d));
    let em_dia = Situacao {
        estado: EstadoCalendario::EmDia,
        esperado_em: None,
        atrasado_a_partir_de: None,
    };
    let (Some(d1), Some(d2)) = (proximos.next(), proximos.next()) else {
        return em_dia;
    };
    // Um dia da semana sempre tem procura se for dia de sorteio.
    let (Some(procura_d1), Some(procura_d2)) = (procurar_desde(cfg, d1), procurar_desde(cfg, d2))
    else {
        return em_dia;
    };
    if agora < procura_d1 {
        return em_dia;
    }
    Situacao {
        estado: if agora < procura_d2 {
            EstadoCalendario::Aguardando
        } else {
            EstadoCalendario::Atrasado
        },
        esperado_em: Some(tempo::data_iso_de_dias(d1)),
        atrasado_a_partir_de: Some(procura_d2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{buscar, HoraLocal};

    fn cfg(slug: &str) -> &'static LoteriaConfig {
        buscar(slug).expect("loteria configurada")
    }

    fn dia(data: &str) -> i64 {
        tempo::dias_da_data_iso(data).expect("data válida no teste")
    }

    /// Segundos Unix de `data` às `hora:minuto` em Brasília.
    fn brasilia(data: &str, hora: i64, minuto: i64) -> i64 {
        dia(data) * 86_400 + hora * 3600 + minuto * 60 - FUSO_SEGUNDOS
    }

    fn res(concurso: u32, data: &str) -> Resultado {
        Resultado {
            loteria: "teste".into(),
            concurso,
            data_sorteio: data.into(),
            dezenas: vec![1, 2, 3],
        }
    }

    /// Um concurso por dia, de `inicio` a `fim` (inclusive), só nos dias da
    /// semana pedidos.
    fn historico(inicio: &str, fim: &str, dias: &[DiaSemana]) -> Vec<Resultado> {
        (dia(inicio)..=dia(fim))
            .filter(|&d| dias.contains(&dia_da_semana(d)))
            .enumerate()
            .map(|(i, d)| res(i as u32 + 1, &tempo::data_iso_de_dias(d)))
            .collect()
    }

    /// Calendário de teste: terça e sexta, procura a partir de 21h00.
    fn terca_e_sexta() -> LoteriaConfig {
        LoteriaConfig {
            dias_sorteio: &[DiaSemana::Terca, DiaSemana::Sexta],
            procurar_a_partir_das: HoraLocal {
                hora: 21,
                minuto: 0,
            },
            ..*cfg("quina")
        }
    }

    // ----------------------------------------------------- dia da semana

    #[test]
    fn dia_da_semana_de_datas_conhecidas() {
        // Conferidas com `date -d <data> +%A`.
        for (data, esperado) in [
            ("1970-01-01", DiaSemana::Quinta),
            ("1969-12-31", DiaSemana::Quarta),
            ("2000-01-01", DiaSemana::Sabado),
            ("2000-02-29", DiaSemana::Terca),
            ("2024-02-29", DiaSemana::Quinta),
            ("2026-10-11", DiaSemana::Domingo),
            ("2026-10-12", DiaSemana::Segunda),
            ("2026-10-13", DiaSemana::Terca),
            ("2026-10-14", DiaSemana::Quarta),
            ("2026-10-15", DiaSemana::Quinta),
            ("2026-10-16", DiaSemana::Sexta),
            ("2026-10-17", DiaSemana::Sabado),
            ("2026-12-31", DiaSemana::Quinta),
            ("1900-01-01", DiaSemana::Segunda),
        ] {
            assert_eq!(dia_da_semana(dia(data)), esperado, "{data}");
        }
    }

    #[test]
    fn dias_consecutivos_percorrem_a_semana_em_ordem() {
        for (i, d) in (dia("2026-10-12")..dia("2026-10-12") + 21).enumerate() {
            assert_eq!(dia_da_semana(d), TODOS_OS_DIAS[i % 7], "dia {d}");
        }
    }

    // --------------------------------------------------- fuso e consulta

    #[test]
    fn dia_local_usa_brasilia_e_nao_utc() {
        // 2026-10-13T23:30 em Brasília = 2026-10-14T02:30Z: em UTC já é outro dia.
        let t = brasilia("2026-10-13", 23, 30);
        assert_eq!(tempo::iso8601_utc(t as u64), "2026-10-14T02:30:00Z");
        assert_eq!(dia_local(t), dia("2026-10-13"));
        // 00h00 de Brasília = 03h00Z; um segundo antes ainda é o dia anterior.
        let meia_noite = brasilia("2026-10-14", 0, 0);
        assert_eq!(
            tempo::iso8601_utc(meia_noite as u64),
            "2026-10-14T03:00:00Z"
        );
        assert_eq!(dia_local(meia_noite), dia("2026-10-14"));
        assert_eq!(dia_local(meia_noite - 1), dia("2026-10-13"));
        // 02h59Z ainda é o dia anterior em Brasília; 03h00Z é o novo.
        assert_eq!(
            dia_local(dia("2026-10-14") * 86_400 + 3 * 3600 - 1),
            dia("2026-10-13")
        );
        assert_eq!(
            dia_local(dia("2026-10-14") * 86_400 + 3 * 3600),
            dia("2026-10-14")
        );
        // Antes de 1970 também (divisão com piso).
        assert_eq!(dia_local(-1), -1);
        assert_eq!(dia_local(3 * 3600 - 1), -1);
        assert_eq!(dia_local(3 * 3600), 0);
    }

    #[test]
    fn dia_de_sorteio_por_loteria() {
        let lotofacil = cfg("lotofacil");
        let mega = cfg("megasena");
        let quina = cfg("quina");
        // 2026-10-12 segunda ... 2026-10-18 domingo.
        let semana = [
            "2026-10-12",
            "2026-10-13",
            "2026-10-14",
            "2026-10-15",
            "2026-10-16",
            "2026-10-17",
            "2026-10-18",
        ];
        let esperado_loto = [true, true, true, true, true, false, true];
        let esperado_mega = [false, true, false, true, false, false, true];
        for (i, data) in semana.iter().enumerate() {
            assert_eq!(
                eh_dia_de_sorteio(lotofacil, dia(data)),
                esperado_loto[i],
                "loto {data}"
            );
            assert_eq!(
                eh_dia_de_sorteio(quina, dia(data)),
                esperado_loto[i],
                "quina {data}"
            );
            assert_eq!(
                eh_dia_de_sorteio(mega, dia(data)),
                esperado_mega[i],
                "mega {data}"
            );
        }
    }

    #[test]
    fn procurar_desde_vale_21h_de_brasilia_so_em_dia_de_sorteio() {
        let mega = cfg("megasena");
        // Terça 2026-10-13 21h00 Brasília = quarta 2026-10-14 00h00Z.
        let t = procurar_desde(mega, dia("2026-10-13")).expect("terça é dia de sorteio");
        assert_eq!(tempo::iso8601_utc(t as u64), "2026-10-14T00:00:00Z");
        assert_eq!(t, brasilia("2026-10-13", 21, 0));
        assert_eq!(instante_brasilia(t), "2026-10-13T21:00:00-03:00");
        // Quarta não tem sorteio da Mega.
        assert_eq!(procurar_desde(mega, dia("2026-10-14")), None);
        // Horário com minutos: 21h30.
        let c = LoteriaConfig {
            procurar_a_partir_das: HoraLocal {
                hora: 21,
                minuto: 30,
            },
            ..terca_e_sexta()
        };
        assert_eq!(
            procurar_desde(&c, dia("2026-10-13")),
            Some(brasilia("2026-10-13", 21, 30))
        );
    }

    #[test]
    fn consultar_dia_junta_data_semana_e_horario() {
        let q = cfg("quina");
        let sorteio = consultar_dia(q, dia("2026-10-13"));
        assert_eq!(sorteio.data, "2026-10-13");
        assert_eq!(sorteio.dia_da_semana, DiaSemana::Terca);
        assert!(sorteio.eh_dia_de_sorteio);
        assert_eq!(sorteio.procurar_desde, Some(brasilia("2026-10-13", 21, 0)));

        let sem = consultar_dia(q, dia("2026-10-17"));
        assert_eq!(sem.dia_da_semana, DiaSemana::Sabado);
        assert!(!sem.eh_dia_de_sorteio);
        assert_eq!(sem.procurar_desde, None);
    }

    #[test]
    fn instante_brasilia_formata_no_relogio_local() {
        assert_eq!(
            instante_brasilia(brasilia("2026-10-13", 23, 30)),
            "2026-10-13T23:30:00-03:00"
        );
        assert_eq!(
            instante_brasilia(brasilia("2026-01-01", 0, 0)),
            "2026-01-01T00:00:00-03:00"
        );
        assert_eq!(instante_brasilia(3 * 3600), "1970-01-01T00:00:00-03:00");
    }

    #[test]
    fn calendarios_configurados_sao_validos() {
        for l in crate::config::LOTERIAS {
            assert!(!l.dias_sorteio.is_empty(), "{}", l.slug);
            let mut ordenado = l.dias_sorteio.to_vec();
            ordenado.sort();
            ordenado.dedup();
            assert_eq!(
                ordenado, l.dias_sorteio,
                "{}: em ordem e sem repetição",
                l.slug
            );
            assert!(l.procurar_a_partir_das.hora < 24 && l.procurar_a_partir_das.minuto < 60);
        }
        assert_eq!(
            cfg("megasena").dias_sorteio,
            [DiaSemana::Terca, DiaSemana::Quinta, DiaSemana::Domingo]
        );
    }

    // -------------------------------------------------------- conferência

    #[test]
    fn calendario_certo_nao_avisa() {
        let c = terca_e_sexta();
        let h = historico("2026-08-01", "2026-10-13", c.dias_sorteio);
        assert_eq!(conferir(&c, &h), []);
    }

    #[test]
    fn dia_novo_recorrente_avisa() {
        // A Caixa passou a sortear também aos sábados nas últimas semanas.
        let c = terca_e_sexta();
        let mut h = historico("2026-08-01", "2026-10-13", c.dias_sorteio);
        h.push(res(900, "2026-10-03"));
        h.push(res(901, "2026-10-10"));
        let avisos = conferir(&c, &h);
        assert_eq!(
            avisos,
            [AvisoCalendario::DiaForaDoCalendario {
                dia_da_semana: DiaSemana::Sabado,
                datas: vec!["2026-10-03".into(), "2026-10-10".into()],
            }]
        );
        let texto = avisos[0].to_string();
        assert!(
            texto.contains("sábado") && texto.contains("2026-10-10"),
            "{texto}"
        );
    }

    #[test]
    fn dois_concursos_na_mesma_data_fora_do_calendario_nao_avisam() {
        // Lotofácil 3715 e 3716 caíram ambos em 2026-06-20 (sábado especial).
        let c = terca_e_sexta();
        let mut h = historico("2026-08-01", "2026-10-13", c.dias_sorteio);
        h.push(res(900, "2026-10-03"));
        h.push(res(901, "2026-10-03"));
        assert_eq!(conferir(&c, &h), []);
    }

    #[test]
    fn especial_isolado_fora_do_calendario_nao_avisa() {
        // Mega da Virada: um sábado/quinta fora do calendário, uma vez só.
        let c = terca_e_sexta();
        let mut h = historico("2026-08-01", "2026-10-13", c.dias_sorteio);
        h.push(res(900, "2026-10-03")); // sábado
        assert_eq!(conferir(&c, &h), []);
    }

    #[test]
    fn dois_especiais_fora_da_janela_nao_somam() {
        // Dois sábados, mas um deles mais de 56 dias antes do último concurso.
        let c = terca_e_sexta();
        let mut h = historico("2026-06-01", "2026-10-13", c.dias_sorteio);
        h.push(res(900, "2026-10-03"));
        h.push(res(901, "2026-08-08")); // 66 dias antes de 2026-10-13
        assert_eq!(conferir(&c, &h), []);
    }

    #[test]
    fn dia_do_calendario_sumido_avisa() {
        // Sem nenhuma sexta nas últimas 8 semanas.
        let c = terca_e_sexta();
        let h = historico("2026-08-01", "2026-10-13", &[DiaSemana::Terca]);
        assert_eq!(
            conferir(&c, &h),
            [AvisoCalendario::DiaSemConcurso {
                dia_da_semana: DiaSemana::Sexta
            }]
        );
        let texto = conferir(&c, &h)[0].to_string();
        assert!(
            texto.contains("sexta") && texto.contains("8 semanas"),
            "{texto}"
        );
    }

    #[test]
    fn feriado_isolado_no_dia_do_calendario_nao_avisa() {
        // Uma sexta sem sorteio entre oito: o dia continua com concurso na janela.
        let c = terca_e_sexta();
        let mut h = historico("2026-08-01", "2026-10-13", c.dias_sorteio);
        h.retain(|r| r.data_sorteio != "2026-09-04");
        assert_eq!(conferir(&c, &h), []);
    }

    #[test]
    fn so_o_que_esta_na_janela_conta() {
        // A última sexta com concurso é 2026-08-14: 60 dias antes do último
        // concurso (terça 2026-10-13) fica fora da janela de 56 dias.
        let c = terca_e_sexta();
        let mut h = historico("2026-08-01", "2026-08-14", c.dias_sorteio);
        h.extend(historico("2026-08-15", "2026-10-13", &[DiaSemana::Terca]));
        assert_eq!(
            conferir(&c, &h),
            [AvisoCalendario::DiaSemConcurso {
                dia_da_semana: DiaSemana::Sexta
            }]
        );
    }

    #[test]
    fn borda_da_janela_de_56_dias_inclusive() {
        // Último concurso: terça 2026-10-13; a janela começa em 2026-08-19
        // (quarta, 56 dias contando o último). Sexta 2026-08-21 é a primeira
        // sexta dentro; sexta 2026-08-14 está fora.
        let c = terca_e_sexta();
        let tercas = historico("2026-08-19", "2026-10-13", &[DiaSemana::Terca]);
        let mut so_a_primeira = tercas.clone();
        so_a_primeira.push(res(900, "2026-08-21"));
        assert_eq!(conferir(&c, &so_a_primeira), []);
        let mut fora = tercas;
        fora.push(res(900, "2026-08-14"));
        assert_eq!(conferir(&c, &fora).len(), 1);
    }

    #[test]
    fn fora_do_calendario_na_borda_da_janela() {
        // Sábados em 2026-08-22 (dentro) e 2026-08-15 (fora): conta só um.
        let c = terca_e_sexta();
        let mut h = historico("2026-08-01", "2026-10-13", c.dias_sorteio);
        h.push(res(900, "2026-08-22"));
        h.push(res(901, "2026-08-15"));
        assert_eq!(conferir(&c, &h), []);
        h.push(res(902, "2026-08-29"));
        assert_eq!(conferir(&c, &h).len(), 1);
    }

    #[test]
    fn banco_curto_so_cobra_dia_que_ocorre_na_janela() {
        // Três dias de histórico (seg a qua): a sexta nem ocorre na janela.
        let c = terca_e_sexta();
        let h = vec![
            res(1, "2026-10-12"),
            res(2, "2026-10-13"),
            res(3, "2026-10-14"),
        ];
        assert_eq!(conferir(&c, &h), []);
        // Já a terça ocorre na janela e se faltar, avisa.
        let h = vec![res(1, "2026-10-12"), res(3, "2026-10-14")];
        assert_eq!(
            conferir(&c, &h),
            [AvisoCalendario::DiaSemConcurso {
                dia_da_semana: DiaSemana::Terca
            }]
        );
    }

    #[test]
    fn conferencia_ignora_datas_invalidas_e_banco_vazio() {
        let c = terca_e_sexta();
        assert_eq!(conferir(&c, &[]), []);
        assert_eq!(conferir(&c, &[res(1, "ontem"), res(2, "2026-02-30")]), []);
        let mut h = historico("2026-08-01", "2026-10-13", c.dias_sorteio);
        h.push(res(900, "lixo"));
        assert_eq!(conferir(&c, &h), []);
    }

    #[test]
    fn conferencia_nao_depende_da_ordem_dos_resultados() {
        let c = terca_e_sexta();
        let mut h = historico("2026-08-01", "2026-10-13", &[DiaSemana::Terca]);
        let esperado = conferir(&c, &h);
        h.reverse();
        assert_eq!(conferir(&c, &h), esperado);
        assert_eq!(esperado.len(), 1);
    }

    #[test]
    fn avisos_vem_na_ordem_do_config_e_depois_da_semana() {
        // Calendário terça+sexta; o banco só tem quarta e sábado (2 de cada).
        let c = terca_e_sexta();
        let h = vec![
            res(1, "2026-10-03"),
            res(2, "2026-10-10"),
            res(3, "2026-10-07"),
            res(4, "2026-10-14"),
        ];
        let avisos = conferir(&c, &h);
        assert_eq!(avisos.len(), 4);
        assert!(matches!(
            avisos[0],
            AvisoCalendario::DiaSemConcurso {
                dia_da_semana: DiaSemana::Terca
            }
        ));
        assert!(matches!(
            avisos[1],
            AvisoCalendario::DiaSemConcurso {
                dia_da_semana: DiaSemana::Sexta
            }
        ));
        assert!(matches!(
            avisos[2],
            AvisoCalendario::DiaForaDoCalendario {
                dia_da_semana: DiaSemana::Quarta,
                ..
            }
        ));
        assert!(matches!(
            avisos[3],
            AvisoCalendario::DiaForaDoCalendario {
                dia_da_semana: DiaSemana::Sabado,
                ..
            }
        ));
    }

    // ---------------------------------------------------------- tolerância

    fn estado(c: &LoteriaConfig, ultimo: &str, agora: i64) -> EstadoCalendario {
        situacao(c, dia(ultimo), agora).estado
    }

    #[test]
    fn em_dia_ate_o_instante_da_procura_e_aguardando_a_partir_dele() {
        // Calendário terça+sexta; último concurso: terça 2026-10-06.
        // D1 = sexta 2026-10-09 (procura 21h00), D2 = terça 2026-10-13.
        let c = terca_e_sexta();
        let ultimo = "2026-10-06";
        let procura = brasilia("2026-10-09", 21, 0);
        assert_eq!(estado(&c, ultimo, procura - 1), EstadoCalendario::EmDia);
        let exato = situacao(&c, dia(ultimo), procura);
        assert_eq!(exato.estado, EstadoCalendario::Aguardando);
        assert_eq!(exato.esperado_em.as_deref(), Some("2026-10-09"));
        assert_eq!(
            exato.atrasado_a_partir_de,
            Some(brasilia("2026-10-13", 21, 0))
        );
        assert_eq!(
            estado(&c, ultimo, procura + 1),
            EstadoCalendario::Aguardando
        );
    }

    #[test]
    fn aguardando_ate_o_instante_da_procura_do_sorteio_seguinte() {
        let c = terca_e_sexta();
        let ultimo = "2026-10-06";
        let limite = brasilia("2026-10-13", 21, 0);
        assert_eq!(estado(&c, ultimo, limite - 1), EstadoCalendario::Aguardando);
        assert_eq!(estado(&c, ultimo, limite), EstadoCalendario::Atrasado);
        assert_eq!(
            estado(&c, ultimo, limite + 86_400 * 30),
            EstadoCalendario::Atrasado
        );
    }

    #[test]
    fn antes_de_qualquer_sorteio_pendente_esta_em_dia() {
        // Último concurso: sexta 2026-10-09 (hoje já tem o dele). Sábado,
        // domingo, segunda: sem sorteio, em dia o fim de semana todo.
        let c = terca_e_sexta();
        let ultimo = "2026-10-09";
        for (data, h, m) in [
            ("2026-10-09", 23, 59),
            ("2026-10-10", 12, 0),
            ("2026-10-11", 23, 59),
            ("2026-10-12", 23, 59),
            ("2026-10-13", 20, 59),
        ] {
            assert_eq!(
                estado(&c, ultimo, brasilia(data, h, m)),
                EstadoCalendario::EmDia,
                "{data} {h}:{m}"
            );
        }
        assert_eq!(
            estado(&c, ultimo, brasilia("2026-10-13", 21, 0)),
            EstadoCalendario::Aguardando
        );
    }

    #[test]
    fn dia_de_sorteio_sem_o_concurso_no_mesmo_dia_cedo_nao_e_atraso() {
        // Terça 2026-10-13 às 10h: o concurso de hoje ainda nem aconteceu.
        let c = terca_e_sexta();
        assert_eq!(
            estado(&c, "2026-10-09", brasilia("2026-10-13", 10, 0)),
            EstadoCalendario::EmDia
        );
    }

    #[test]
    fn virada_de_semana_e_domingo_sem_sorteio() {
        // Calendário só de segunda a sexta (como a Lotofácil sem domingo).
        let c = LoteriaConfig {
            dias_sorteio: &[
                DiaSemana::Segunda,
                DiaSemana::Terca,
                DiaSemana::Quarta,
                DiaSemana::Quinta,
                DiaSemana::Sexta,
            ],
            ..terca_e_sexta()
        };
        // Último: sexta 2026-10-09. D1 = segunda 2026-10-12, D2 = terça 13.
        let ultimo = "2026-10-09";
        assert_eq!(
            estado(&c, ultimo, brasilia("2026-10-11", 23, 59)),
            EstadoCalendario::EmDia
        );
        assert_eq!(
            estado(&c, ultimo, brasilia("2026-10-12", 20, 59)),
            EstadoCalendario::EmDia
        );
        assert_eq!(
            estado(&c, ultimo, brasilia("2026-10-12", 21, 0)),
            EstadoCalendario::Aguardando
        );
        assert_eq!(
            estado(&c, ultimo, brasilia("2026-10-13", 20, 59)),
            EstadoCalendario::Aguardando
        );
        assert_eq!(
            estado(&c, ultimo, brasilia("2026-10-13", 21, 0)),
            EstadoCalendario::Atrasado
        );
    }

    #[test]
    fn ultimo_concurso_em_dia_fora_do_calendario() {
        // Sorteio especial num sábado (fora do calendário terça+sexta): o
        // próximo esperado é a terça seguinte.
        let c = terca_e_sexta();
        let ultimo = "2026-10-10"; // sábado
        assert_eq!(
            estado(&c, ultimo, brasilia("2026-10-13", 20, 59)),
            EstadoCalendario::EmDia
        );
        let s = situacao(&c, dia(ultimo), brasilia("2026-10-13", 21, 0));
        assert_eq!(s.estado, EstadoCalendario::Aguardando);
        assert_eq!(s.esperado_em.as_deref(), Some("2026-10-13"));
        assert_eq!(s.atrasado_a_partir_de, Some(brasilia("2026-10-16", 21, 0)));
    }

    #[test]
    fn em_dia_nao_traz_detalhes_e_o_relogio_pode_estar_atras() {
        let c = terca_e_sexta();
        let s = situacao(&c, dia("2026-10-13"), brasilia("2026-10-13", 22, 0));
        assert_eq!(s.estado, EstadoCalendario::EmDia);
        assert_eq!(s.esperado_em, None);
        assert_eq!(s.atrasado_a_partir_de, None);
        // Último concurso no futuro do relógio: nada esperado.
        assert_eq!(
            estado(&c, "2026-12-01", brasilia("2026-10-13", 22, 0)),
            EstadoCalendario::EmDia
        );
    }

    #[test]
    fn fuso_na_procura_23h30_de_brasilia_e_o_dia_de_brasilia() {
        // Sexta 2026-10-09 às 23h30 em Brasília já é sábado em UTC; o estado
        // é pelo relógio de Brasília: sexta tem sorteio e o concurso falta.
        let c = terca_e_sexta();
        let agora = brasilia("2026-10-09", 23, 30);
        assert_eq!(tempo::iso8601_utc(agora as u64), "2026-10-10T02:30:00Z");
        assert_eq!(
            estado(&c, "2026-10-06", agora),
            EstadoCalendario::Aguardando
        );
        // Com o concurso de sexta no banco, em dia.
        assert_eq!(estado(&c, "2026-10-09", agora), EstadoCalendario::EmDia);
        assert_eq!(consultar_dia(&c, dia_local(agora)).data, "2026-10-09");
    }

    #[test]
    fn calendario_sem_dias_fica_sempre_em_dia() {
        let c = LoteriaConfig {
            dias_sorteio: &[],
            ..terca_e_sexta()
        };
        assert_eq!(
            estado(&c, "2026-10-06", brasilia("2026-12-31", 23, 0)),
            EstadoCalendario::EmDia
        );
    }

    // -------------------------------------------------- sorteios especiais

    fn no_especial(slug: &str, data: &str) -> bool {
        especial_em(cfg(slug), dia(data)).is_some()
    }

    #[test]
    fn janela_do_especial_tem_bordas_inclusivas() {
        // Lotofácil: 7/9 menos 15 dias = 23/8; mais 8 dias = 15/9.
        assert!(!no_especial("lotofacil", "2026-08-22"));
        assert!(no_especial("lotofacil", "2026-08-23"));
        assert!(no_especial("lotofacil", "2026-09-15"));
        assert!(!no_especial("lotofacil", "2026-09-16"));
        // Quina: 24/6 menos 15 dias = 9/6; mais 4 dias = 28/6.
        assert!(!no_especial("quina", "2026-06-08"));
        assert!(no_especial("quina", "2026-06-09"));
        assert!(no_especial("quina", "2026-06-28"));
        assert!(!no_especial("quina", "2026-06-29"));
        // Mega: 31/12 menos 20 dias = 11/12; mais 2 dias = 2/1 do ano seguinte.
        assert!(!no_especial("megasena", "2025-12-10"));
        assert!(no_especial("megasena", "2025-12-11"));
        assert!(no_especial("megasena", "2026-01-02"));
        assert!(!no_especial("megasena", "2026-01-03"));
        // Um dia comum.
        assert!(!no_especial("megasena", "2026-10-13"));
        assert_eq!(
            especial_em(cfg("quina"), dia("2026-06-20")).map(|e| e.nome),
            Some("Quina de São João")
        );
    }

    #[test]
    fn cada_loteria_so_tem_a_janela_do_seu_especial() {
        // O especial de uma loteria não vale para as outras.
        assert!(!no_especial("lotofacil", "2026-06-20"));
        assert!(!no_especial("quina", "2026-09-10"));
        assert!(!no_especial("lotofacil", "2026-12-25"));
    }

    #[test]
    fn paradas_vistas_nos_bancos_reais_cabem_na_janela() {
        // (loteria, último concurso regular antes da parada, data do
        // especial): conferidas contra os bancos reais em 2026-10-13. Todo dia
        // da parada, do seguinte ao último concurso até o especial, tem de
        // estar na janela.
        let paradas = [
            ("lotofacil", "2021-08-27", "2021-09-11"),
            ("lotofacil", "2022-08-26", "2022-09-10"),
            ("lotofacil", "2023-08-25", "2023-09-09"),
            ("lotofacil", "2024-08-23", "2024-09-09"),
            ("lotofacil", "2025-08-26", "2025-09-06"),
            ("lotofacil", "2026-09-03", "2026-09-15"),
            ("megasena", "2021-12-18", "2021-12-31"),
            ("megasena", "2022-12-17", "2022-12-31"),
            ("megasena", "2023-12-16", "2023-12-31"),
            ("megasena", "2024-12-17", "2024-12-31"),
            ("megasena", "2025-12-20", "2026-01-01"),
            ("quina", "2021-06-14", "2021-06-26"),
            ("quina", "2022-06-15", "2022-06-25"),
            ("quina", "2023-06-10", "2023-06-24"),
            ("quina", "2024-06-08", "2024-06-22"),
            ("quina", "2025-06-18", "2025-06-28"),
            ("quina", "2026-06-14", "2026-06-28"),
        ];
        for (slug, ultimo, especial) in paradas {
            for d in dia(ultimo) + 1..=dia(especial) {
                assert!(
                    especial_em(cfg(slug), d).is_some(),
                    "{slug}: {} fora da janela (parada {ultimo} -> {especial})",
                    tempo::data_iso_de_dias(d)
                );
            }
        }
    }

    #[test]
    fn nomes_estaveis_dos_estados() {
        assert_eq!(EstadoCalendario::EmDia.nome(), "em_dia");
        assert_eq!(EstadoCalendario::Aguardando.nome(), "aguardando");
        assert_eq!(EstadoCalendario::Atrasado.nome(), "atrasado");
    }
}
