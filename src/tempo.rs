//! Data e hora em texto, sem crate de calendário.
//!
//! `perfis.criado_em` é ISO-8601 UTC com segundos (`2026-10-04T18:30:00Z`),
//! formatado a partir de `SystemTime` (decisão de 2026-10-05,
//! `docs/PLANEJAMENTO.md`, esquema de `perfis`). Quem lê o relógio é a CLI;
//! aqui só há funções puras sobre segundos desde 1970-01-01T00:00:00Z, para
//! serem testáveis.
//!
//! O nome dos backups do `sync atualizar` usa um carimbo compacto
//! (`AAAAMMDD-HHMMSS`, [`carimbo_utc`]), também em UTC: sem crate de fuso,
//! não há como saber o horário local de forma confiável.

/// Segundos de um dia (UTC não tem horário de verão; segundos bissextos não
/// existem no tempo Unix).
const SEGUNDOS_POR_DIA: u64 = 86_400;

/// ISO-8601 UTC com segundos (`AAAA-MM-DDTHH:MM:SSZ`) dos `segundos` desde a
/// época Unix (1970-01-01T00:00:00Z). O ano tem no mínimo 4 dígitos.
pub fn iso8601_utc(segundos: u64) -> String {
    let (ano, mes, dia, h, m, s) = decompor(segundos);
    format!("{ano:04}-{mes:02}-{dia:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Carimbo compacto UTC (`AAAAMMDD-HHMMSS`) dos `segundos` desde a época
/// Unix, para nomes de arquivo (sem `:`, que o Windows não aceita). Ordena
/// cronologicamente como texto enquanto o ano tiver 4 dígitos.
pub fn carimbo_utc(segundos: u64) -> String {
    let (ano, mes, dia, h, m, s) = decompor(segundos);
    format!("{ano:04}{mes:02}{dia:02}-{h:02}{m:02}{s:02}")
}

/// (ano, mês, dia, hora, minuto, segundo) em UTC.
fn decompor(segundos: u64) -> (i64, u32, u32, u64, u64, u64) {
    let dias = segundos / SEGUNDOS_POR_DIA;
    let no_dia = segundos % SEGUNDOS_POR_DIA;
    // `u64::MAX / 86400` cabe com folga em `i64`.
    let (ano, mes, dia) = data_civil(dias as i64);
    (
        ano,
        mes,
        dia,
        no_dia / 3600,
        no_dia % 3600 / 60,
        no_dia % 60,
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

/// Inverso de [`data_civil`]: dias desde 1970-01-01 da data (ano, mês 1–12,
/// dia 1–31) do calendário gregoriano proléptico. Não valida o dia contra o
/// mês (31 de abril vira 1º de maio); quem lê texto confere antes, como
/// [`dias_da_data_iso`].
///
/// Algoritmo `days_from_civil` de Howard Hinnant, o espelho de `data_civil`.
pub fn dias_desde_epoca(ano: i64, mes: u32, dia: u32) -> i64 {
    let a = ano - i64::from(mes <= 2);
    let era = a.div_euclid(400);
    let yoe = a.rem_euclid(400); // 0 ..= 399
    let mp = i64::from(if mes > 2 { mes - 3 } else { mes + 9 }); // março = 0
    let doy = (153 * mp + 2) / 5 + i64::from(dia) - 1; // dia do ano desde 1º/3
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Dias desde 1970-01-01 de uma data `AAAA-MM-DD` (o formato de
/// `Resultado::data_sorteio`), ou `None` se o texto não é uma data real
/// (formato, mês, dia fora do mês, 29/02 de ano não bissexto).
pub fn dias_da_data_iso(texto: &str) -> Option<i64> {
    let b = texto.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let numero = |trecho: &str| -> Option<u32> {
        if trecho.bytes().all(|c| c.is_ascii_digit()) {
            trecho.parse().ok()
        } else {
            None
        }
    };
    let ano = numero(texto.get(0..4)?)?;
    let mes = numero(texto.get(5..7)?)?;
    let dia = numero(texto.get(8..10)?)?;
    if !(1..=12).contains(&mes) || dia == 0 {
        return None;
    }
    let dias = dias_desde_epoca(i64::from(ano), mes, dia);
    // Dia que não existe no mês transborda para o mês seguinte: a volta pela
    // fórmula inversa denuncia.
    (data_civil(dias) == (i64::from(ano), mes, dia)).then_some(dias)
}

/// `AAAA-MM-DD` do dia `dias` desde 1970-01-01.
pub fn data_iso_de_dias(dias: i64) -> String {
    let (ano, mes, dia) = data_civil(dias);
    format!("{ano:04}-{mes:02}-{dia:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dias_desde_epoca_e_o_inverso_de_data_civil() {
        for dias in (-30_000..=160_000).step_by(7).chain([0, -1, 1, 20_000]) {
            let (a, m, d) = data_civil(dias);
            assert_eq!(dias_desde_epoca(a, m, d), dias, "{a}-{m}-{d}");
        }
        assert_eq!(dias_desde_epoca(1970, 1, 1), 0);
        assert_eq!(dias_desde_epoca(2000, 3, 1), 11_017);
        assert_eq!(dias_desde_epoca(2026, 10, 13), 20_739);
    }

    #[test]
    fn data_iso_ida_e_volta_e_recusas() {
        assert_eq!(dias_da_data_iso("2026-10-13"), Some(20_739));
        assert_eq!(data_iso_de_dias(20_739), "2026-10-13");
        assert_eq!(dias_da_data_iso("2024-02-29"), Some(19_782));
        for ruim in [
            "",
            "2026-10-1",
            "2026/10/13",
            "2026-13-01",
            "2026-00-10",
            "2026-10-00",
            "2026-04-31",
            "2025-02-29",
            "2100-02-29",
            "26-10-13x",
            "2026-1a-13",
            "+026-10-13",
            "２０２６-10-13",
        ] {
            assert_eq!(dias_da_data_iso(ruim), None, "{ruim:?}");
        }
        assert_eq!(
            dias_da_data_iso("2000-02-29")
                .map(data_iso_de_dias)
                .as_deref(),
            Some("2000-02-29")
        );
    }

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

    #[test]
    fn carimbo_compacto_utc() {
        for (segundos, esperado) in [
            (0, "19700101-000000"),
            (3_600 + 60 + 1, "19700101-010101"),
            (951_782_400, "20000229-000000"),
            (1_791_138_600, "20261004-183000"),
            (1_798_761_599, "20261231-235959"),
            (1_798_761_600, "20270101-000000"),
        ] {
            assert_eq!(carimbo_utc(segundos), esperado, "{segundos}");
        }
    }

    #[test]
    fn carimbo_e_iso8601_descrevem_o_mesmo_instante() {
        for segundos in [0, 86_399, 68_212_800, 1_791_138_600, 4_107_542_400] {
            let iso = iso8601_utc(segundos);
            let esperado: String = iso
                .trim_end_matches('Z')
                .replace('T', "-")
                .chars()
                .filter(|c| *c != ':')
                .collect::<String>();
            // `AAAA-MM-DD-HH:MM:SS` sem `:` → `AAAA-MM-DD-HHMMSS`; tira os `-` da data.
            let esperado = format!("{}{}", esperado[..10].replace('-', ""), &esperado[10..]);
            assert_eq!(carimbo_utc(segundos), esperado, "{segundos}");
        }
    }

    #[test]
    fn carimbos_ordenam_como_o_tempo() {
        let instantes = [0, 59, 60, 3_599, 86_400, 951_782_400, 1_791_138_600];
        for par in instantes.windows(2) {
            assert!(carimbo_utc(par[0]) < carimbo_utc(par[1]), "{par:?}");
        }
    }
}
