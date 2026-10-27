use super::*;

#[test]
fn decimal_usa_virgula_nove_casas_e_nao_escreve_menos_zero() {
    assert_eq!(decimal(1.0), "1,000000000");
    assert_eq!(decimal(0.6), "0,600000000");
    assert_eq!(decimal(-1.234_567_891_4), "-1,234567891");
    assert_eq!(decimal(-0.0), "0,000000000");
    assert_eq!(decimal(-0.000_000_000_1), "0,000000000");
    // A diferença de 1e-9 que a ordenação enxerga continua visível.
    assert_ne!(decimal(0.5), decimal(0.5 + 2e-9));
}

#[test]
fn formatar_desvio_e_descrever_janela_seguem_os_relatorios() {
    assert_eq!(formatar_desvio(0.05), "5.0 %");
    assert_eq!(formatar_desvio(0.025), "2.5 %");
    assert_eq!(descrever_janela(JANELA_TOTAL), "total");
    assert_eq!(descrever_janela(50), "50");
}
