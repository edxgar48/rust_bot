//! Data e hora em texto, sem crate de calendário.
//!
//! `perfis.criado_em` é ISO-8601 UTC com segundos (`2026-10-04T18:30:00Z`),
//! formatado a partir de `SystemTime` (decisão de 2026-10-05,
//! `docs/PLANEJAMENTO.md`, esquema de `perfis`). Quem lê o relógio é a CLI;
//! aqui só há funções puras sobre segundos desde 1970-01-01T00:00:00Z, para
//! serem testáveis.

/// Segundos de um dia (UTC não tem horário de verão; segundos bissextos não
/// existem no tempo Unix).
const SEGUNDOS_POR_DIA: u64 = 86_400;

/// ISO-8601 UTC com segundos (`AAAA-MM-DDTHH:MM:SSZ`) dos `segundos` desde a
/// época Unix (1970-01-01T00:00:00Z). O ano tem no mínimo 4 dígitos.
pub fn iso8601_utc(segundos: u64) -> String {
    let dias = segundos / SEGUNDOS_POR_DIA;
    let no_dia = segundos % SEGUNDOS_POR_DIA;
    // `u64::MAX / 86400` cabe com folga em `i64`.
    let (ano, mes, dia) = data_civil(dias as i64);
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        no_dia / 3600,
        no_dia % 3600 / 60,
        no_dia % 60
    )
}

/// Data do calendário gregoriano proléptico (ano, mês 1–12, dia 1–31) do dia
/// `dias` contado a partir de 1970-01-01 (dia 0; negativo = antes).
///
/// Algoritmo `civil_from_days` de Howard Hinnant ("chrono-Compatible
/// Low-Level Date Algorithms"): conta em eras de 400 anos (146 097 dias)
/// com o ano começando em 1º de março, para o dia bissexto cair no fim.
pub fn data_civil(dias: i64) -> (i64, u32, u32) {
    let z = dias + 719_468; // dias desde 0000-03-01
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // dia da era: 0 ..= 146 096
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // 0 ..= 399
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // dia do ano (desde 1º/3): 0 ..= 365
    let mp = (5 * doy + 2) / 153; // mês desde março: 0 ..= 11
    let dia = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let ano = yoe + era * 400 + i64::from(mes <= 2);
    (ano, mes, dia)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datas_de_referencia() {
        // Segundos conferidos com `date -u -d <data> +%s` (GNU coreutils).
        for (segundos, esperado) in [
            (0, "1970-01-01T00:00:00Z"),
            (86_399, "1970-01-01T23:59:59Z"),
            (86_400, "1970-01-02T00:00:00Z"),
            (68_212_800, "1972-02-29T12:00:00Z"),
            (946_684_799, "1999-12-31T23:59:59Z"),
            (946_684_800, "2000-01-01T00:00:00Z"),
            (951_782_400, "2000-02-29T00:00:00Z"),
            (951_868_800, "2000-03-01T00:00:00Z"),
            (1_791_138_600, "2026-10-04T18:30:00Z"),
            (2_147_483_648, "2038-01-19T03:14:08Z"),
            (4_107_542_399, "2100-02-28T23:59:59Z"),
            (4_107_542_400, "2100-03-01T00:00:00Z"),
            (13_574_563_200, "2400-02-29T00:00:00Z"),
        ] {
            assert_eq!(iso8601_utc(segundos), esperado, "{segundos}");
        }
    }

    #[test]
    fn fim_de_ano_vira_o_ano() {
        // 2026-12-31T23:59:59Z e o segundo seguinte.
        let ultimo_de_2026 = 1_798_761_599;
        assert_eq!(iso8601_utc(ultimo_de_2026), "2026-12-31T23:59:59Z");
        assert_eq!(iso8601_utc(ultimo_de_2026 + 1), "2027-01-01T00:00:00Z");
    }

    fn bissexto(ano: i64) -> bool {
        (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0
    }

    fn dias_no_mes(ano: i64, mes: u32) -> u32 {
        match mes {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if bissexto(ano) => 29,
            2 => 28,
            _ => unreachable!("mês {mes}"),
        }
    }

    #[test]
    fn dia_a_dia_confere_com_o_calendario() {
        // Oráculo independente: anda um dia por vez a partir de 1970-01-01,
        // pelas regras do calendário, de antes de 1900 (não bissexto) até
        // depois de 2400 (bissexto), e compara com a fórmula fechada.
        let inicio = -30_000; // 1887-11-…
        let fim = 160_000; // 2408-…
        let (mut ano, mut mes, mut dia) = data_civil(inicio);
        for dias in inicio..=fim {
            assert_eq!(data_civil(dias), (ano, mes, dia), "dia {dias}");
            dia += 1;
            if dia > dias_no_mes(ano, mes) {
                dia = 1;
                mes += 1;
                if mes > 12 {
                    mes = 1;
                    ano += 1;
                }
            }
        }
        assert_eq!(data_civil(0), (1970, 1, 1));
        assert_eq!(data_civil(-1), (1969, 12, 31));
        // 1900 não é bissexto; 2000 é.
        assert_eq!(data_civil(-25_508), (1900, 3, 1));
        assert_eq!(data_civil(-25_509), (1900, 2, 28));
    }

    #[test]
    fn horas_minutos_e_segundos_com_dois_digitos() {
        assert_eq!(iso8601_utc(3_600 + 60 + 1), "1970-01-01T01:01:01Z");
        assert_eq!(iso8601_utc(9 * 3_600 + 5 * 60 + 7), "1970-01-01T09:05:07Z");
    }
}
