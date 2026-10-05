//! Perfis de pesos do índice de força (`perfis/<loteria>/<nome>.toml`).
//!
//! Regras em `docs/PLANEJAMENTO.md`, "Índice de força" → "Perfil de pesos"
//! (decisões de 2026-10-04): validação rígida (os 9 pesos obrigatórios,
//! chave desconhecida é erro, `janela` em [`JANELAS`], `normalizacao` só
//! `zscore`/`minmax`), `versao` conferida com o sufixo `_v<N>` do nome, e
//! hash SHA-256 do texto com fim de linha em LF.
//!
//! [`interpretar`] é puro (texto → perfil validado); só [`carregar`] lê do
//! disco, a partir de uma raiz recebida por parâmetro (`perfis/` em produção,
//! um diretório temporário nos testes). O registro na tabela `perfis` fica em
//! `orquestracao::registrar_perfil`.

use crate::config::{self, LoteriaConfig};
use crate::error::{Result, SpiderError};
use crate::motor::JANELAS;
/// Tipos do perfil que o motor usa no índice; moram em `motor` para o motor
/// não depender deste módulo (que lê disco).
pub use crate::motor::{Normalizacao, Pesos};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Conteúdo de um `.toml` de perfil. Chave desconhecida (no topo ou em
/// `[pesos]`) é erro de leitura.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Perfil {
    pub nome: String,
    pub versao: u32,
    pub loteria: String,
    /// Janela das features usada pelo índice; `0` = total.
    pub janela: u32,
    pub normalizacao: Normalizacao,
    pub pesos: Pesos,
}

impl Perfil {
    /// Nome e versão da próxima versão livre da série (`padrao_v1` →
    /// `padrao_v2`, 2), para a mensagem que pede para subir a versão. Pula as
    /// versões para as quais `ocupado(nome)` é verdadeiro (arquivo que já
    /// existe ou perfil já registrado), para não sugerir um nome em uso.
    pub fn proxima_versao(&self, ocupado: impl Fn(&str) -> bool) -> (String, u32) {
        let prefixo = separar_sufixo(&self.nome).map_or(self.nome.as_str(), |(p, _)| p);
        let mut proxima = self.versao.saturating_add(1);
        loop {
            let nome = format!("{prefixo}_v{proxima}");
            if proxima == u32::MAX || !ocupado(&nome) {
                return (nome, proxima);
            }
            proxima += 1;
        }
    }
}

/// Perfil validado junto com o texto e o hash que vão para a tabela `perfis`.
#[derive(Debug, Clone, PartialEq)]
pub struct PerfilCarregado {
    pub perfil: Perfil,
    /// Arquivo de onde o perfil veio (em [`interpretar`], só identifica o
    /// texto nas mensagens). Usado para procurar as versões vizinhas.
    pub origem: PathBuf,
    /// Texto do `.toml` com fim de linha já normalizado para LF: é dele que
    /// sai [`PerfilCarregado::hash_toml`], então `hash_toml(toml) == hash_toml`.
    pub toml: String,
    /// SHA-256 (hex minúsculo) de [`PerfilCarregado::toml`].
    pub hash_toml: String,
}

/// Tira o BOM UTF-8 do início (editores do Windows costumam gravá-lo) e troca
/// CRLF por LF. O `.gitattributes` já força LF no checkout, mas o hash não
/// pode depender disso (cópia manual, editor que grava CRLF ou BOM).
pub fn normalizar_fim_de_linha(texto: &str) -> String {
    texto
        .strip_prefix('\u{feff}')
        .unwrap_or(texto)
        .replace("\r\n", "\n")
}

/// SHA-256, em hex minúsculo, do texto sem BOM e com fim de linha em LF.
pub fn hash_toml(texto: &str) -> String {
    let digest = Sha256::digest(normalizar_fim_de_linha(texto).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Lê e valida um perfil a partir do texto do `.toml`. Puro: `origem` só
/// identifica o arquivo nas mensagens de erro.
///
/// Confere tudo o que depende só do conteúdo: estrutura e chaves (via
/// `serde`), sufixo `_v<N>` do nome igual a `versao`, loteria existente em
/// `config::LOTERIAS`, `janela` em [`JANELAS`] e pesos finitos. A
/// coerência com a pasta e o nome do arquivo fica em [`carregar`].
pub fn interpretar(texto: &str, origem: &Path) -> Result<PerfilCarregado> {
    let invalido = |motivo: String| SpiderError::PerfilInvalido {
        origem: origem.to_path_buf(),
        motivo,
    };

    let toml = normalizar_fim_de_linha(texto);
    let perfil: Perfil =
        toml::from_str(&toml).map_err(|e| invalido(e.to_string().trim_end().to_string()))?;

    validar_caracteres_do_nome(&perfil.nome).map_err(invalido)?;
    match separar_sufixo(&perfil.nome) {
        None => {
            return Err(invalido(format!(
                "campo `nome`: `{}` não termina com o sufixo de versão `_v<N>` \
                 (N inteiro ≥ 1, sem zeros à esquerda; ex.: `padrao_v1`)",
                perfil.nome
            )))
        }
        Some((_, versao)) if versao != perfil.versao => {
            return Err(invalido(format!(
                "campo `versao`: {} não bate com o sufixo do nome `{}` (que indica {versao})",
                perfil.versao, perfil.nome
            )))
        }
        Some(_) => {}
    }

    if config::buscar(&perfil.loteria).is_none() {
        return Err(invalido(format!(
            "campo `loteria`: `{}` não existe; disponíveis: {}",
            perfil.loteria,
            config::slugs_disponiveis()
        )));
    }

    if !JANELAS.contains(&perfil.janela) {
        let validas: Vec<String> = JANELAS.iter().map(u32::to_string).collect();
        return Err(invalido(format!(
            "campo `janela`: {} não é uma janela calculada; use uma de {{{}}} (0 = total)",
            perfil.janela,
            validas.join(", ")
        )));
    }

    if let Some((nome, valor)) = perfil
        .pesos
        .componentes()
        .into_iter()
        .find(|(_, valor)| !valor.is_finite())
    {
        return Err(invalido(format!(
            "campo `pesos.{nome}`: {valor} não é um número finito"
        )));
    }

    let hash_toml = hash_toml(&toml);
    Ok(PerfilCarregado {
        perfil,
        origem: origem.to_path_buf(),
        toml,
        hash_toml,
    })
}

/// Caminho do perfil `nome` da loteria: `<raiz>/<slug>/<nome>.toml`.
pub fn caminho(raiz: &Path, cfg: &LoteriaConfig, nome: &str) -> PathBuf {
    raiz.join(cfg.slug).join(format!("{nome}.toml"))
}

/// Lê `<raiz>/<slug>/<nome>.toml`, valida com [`interpretar`] e confere que a
/// `loteria` do arquivo é a da pasta e o `nome` é o do arquivo.
pub fn carregar(raiz: &Path, cfg: &LoteriaConfig, nome: &str) -> Result<PerfilCarregado> {
    let caminho = caminho(raiz, cfg, nome);
    // O nome vira parte do caminho: nada de separadores nem `..`.
    validar_caracteres_do_nome(nome).map_err(|motivo| SpiderError::PerfilInvalido {
        origem: caminho.clone(),
        motivo,
    })?;

    let texto = std::fs::read_to_string(&caminho).map_err(|origem| SpiderError::Io {
        caminho: caminho.clone(),
        origem,
    })?;
    let carregado = interpretar(&texto, &caminho)?;

    let perfil = &carregado.perfil;
    if perfil.loteria != cfg.slug {
        return Err(SpiderError::PerfilInvalido {
            origem: caminho,
            motivo: format!(
                "campo `loteria`: `{}` não bate com a pasta `{}`",
                perfil.loteria, cfg.slug
            ),
        });
    }
    if perfil.nome != nome {
        return Err(SpiderError::PerfilInvalido {
            origem: caminho,
            motivo: format!(
                "campo `nome`: `{}` não bate com o nome do arquivo `{nome}.toml`",
                perfil.nome
            ),
        });
    }
    Ok(carregado)
}

/// Nome de perfil só com letras/dígitos ASCII, `_` e `-`: ele também é nome
/// de arquivo, então não pode ter separador de caminho, ponto nem espaço.
fn validar_caracteres_do_nome(nome: &str) -> std::result::Result<(), String> {
    let valido = !nome.is_empty()
        && nome
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valido {
        Ok(())
    } else {
        Err(format!(
            "campo `nome`: `{nome}` inválido; use só letras e dígitos ASCII, `_` e `-`"
        ))
    }
}

/// Separa `<prefixo>_v<N>` em (`prefixo`, N). Exige prefixo não vazio e N
/// inteiro ≥ 1 escrito só com dígitos e sem zeros à esquerda, para que cada
/// versão tenha um único nome possível (`padrao_v01` não é `padrao_v1`).
fn separar_sufixo(nome: &str) -> Option<(&str, u32)> {
    let (prefixo, digitos) = nome.rsplit_once("_v")?;
    if prefixo.is_empty()
        || digitos.is_empty()
        || !digitos.bytes().all(|b| b.is_ascii_digit())
        || digitos.starts_with('0')
    {
        return None;
    }
    let versao = digitos.parse().ok()?;
    Some((prefixo, versao))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motor::FeaturesDezena;

    const VALIDO: &str = r#"nome = "teste_v2"
versao = 2
loteria = "quina"
janela = 0
normalizacao = "minmax"

[pesos]
frequencia                = 1.0
maior_espaco              = 1.0
maior_espaco_ocorrencias  = 0.3
menor_espaco              = 0.5
menor_espaco_ocorrencias  = 0.3
soma_variacoes            = -0.8
espaco_final              = 1.2
repeticoes_total          = 0.7
repeticao_atual           = 0
"#;

    fn raiz_do_repositorio() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("perfis")
    }

    fn origem() -> PathBuf {
        PathBuf::from("perfis/quina/teste_v2.toml")
    }

    /// Erro de validação: devolve o motivo, conferindo que a origem veio junto.
    fn motivo_do_erro(texto: &str) -> String {
        match interpretar(texto, &origem()) {
            Err(SpiderError::PerfilInvalido { origem: o, motivo }) => {
                assert_eq!(o, origem());
                motivo
            }
            outro => panic!("esperava PerfilInvalido, veio {outro:?}"),
        }
    }

    fn trocar(texto: &str, de: &str, para: &str) -> String {
        assert!(texto.contains(de), "`{de}` não está no texto");
        texto.replacen(de, para, 1)
    }

    fn dir_temporario(nome: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rust_spider_perfis_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn gravar(raiz: &Path, loteria: &str, arquivo: &str, texto: &str) {
        let pasta = raiz.join(loteria);
        std::fs::create_dir_all(&pasta).expect("cria pasta");
        std::fs::write(pasta.join(arquivo), texto).expect("grava perfil");
    }

    fn cfg(slug: &str) -> &'static LoteriaConfig {
        config::buscar(slug).expect("loteria do config")
    }

    // --- perfis reais do repositório ---------------------------------------

    #[test]
    fn perfis_do_repositorio_carregam_com_os_pesos_do_plano() {
        let esperado_pesos = Pesos {
            frequencia: 1.0,
            maior_espaco: 1.0,
            maior_espaco_ocorrencias: 0.3,
            menor_espaco: 0.5,
            menor_espaco_ocorrencias: 0.3,
            soma_variacoes: -0.8,
            espaco_final: 1.2,
            repeticoes_total: 0.7,
            repeticao_atual: 0.4,
        };
        for cfg in config::LOTERIAS {
            let carregado =
                carregar(&raiz_do_repositorio(), cfg, "padrao_v1").expect("perfil padrão carrega");
            assert_eq!(
                carregado.perfil,
                Perfil {
                    nome: "padrao_v1".to_string(),
                    versao: 1,
                    loteria: cfg.slug.to_string(),
                    janela: 50,
                    normalizacao: Normalizacao::Zscore,
                    pesos: esperado_pesos,
                },
                "{}",
                cfg.slug
            );
            assert!(!carregado.toml.contains('\r'));
            assert_eq!(carregado.hash_toml, hash_toml(&carregado.toml));
        }
    }

    /// Trava do conteúdo dos perfis versionados: editar um `.toml` já
    /// publicado muda o hash e o `sync indice` passaria a recusá-lo nos
    /// bancos onde ele já foi registrado. Mudou o conteúdo → novo arquivo
    /// com a versão seguinte (e um hash novo aqui).
    #[test]
    fn hash_dos_perfis_do_repositorio_nao_muda() {
        let esperados = [
            (
                "lotofacil",
                "02106f8563c2ee47364ecc6b0ec49803a53d127d8ecfdba41bdd1f725b2a5b94",
            ),
            (
                "megasena",
                "95a208125b2da6bd034564770f628a74185023864e19ec415be7ad0183ddbeb5",
            ),
            (
                "quina",
                "5bea2957a4aa375ffaf102aab3ca21d39252213f1a133b4f3538589df6d2c873",
            ),
        ];
        for (slug, hash) in esperados {
            let carregado = carregar(&raiz_do_repositorio(), cfg(slug), "padrao_v1").expect("lê");
            assert_eq!(carregado.hash_toml, hash, "{slug}");
        }
    }

    #[test]
    fn nomes_dos_pesos_sao_os_das_features_do_motor() {
        let features = FeaturesDezena {
            concurso: 1,
            dezena: 1,
            janela: 0,
            frequencia: 0,
            maior_espaco: None,
            maior_espaco_ocorrencias: None,
            menor_espaco: None,
            menor_espaco_ocorrencias: None,
            soma_variacoes: None,
            espaco_final: None,
            repeticoes_total: 0,
            repeticao_atual: 0,
            saiu: None,
        };
        let json = serde_json::to_value(&features).expect("serializa");
        let campos: Vec<&str> = json
            .as_object()
            .expect("objeto")
            .keys()
            .map(String::as_str)
            .filter(|c| !["concurso", "dezena", "janela", "saiu"].contains(c))
            .collect();
        let pesos = interpretar(VALIDO, &origem()).expect("válido").perfil.pesos;
        let mut nomes: Vec<&str> = pesos.componentes().iter().map(|(n, _)| *n).collect();
        let mut campos_ordenados = campos.clone();
        nomes.sort_unstable();
        campos_ordenados.sort_unstable();
        assert_eq!(nomes, campos_ordenados);
    }

    // --- parse válido -------------------------------------------------------

    #[test]
    fn interpreta_perfil_valido() {
        let carregado = interpretar(VALIDO, &origem()).expect("válido");
        let p = &carregado.perfil;
        assert_eq!(p.nome, "teste_v2");
        assert_eq!(p.versao, 2);
        assert_eq!(p.loteria, "quina");
        assert_eq!(p.janela, 0);
        assert_eq!(p.normalizacao, Normalizacao::Minmax);
        assert_eq!(p.pesos.soma_variacoes, -0.8);
        // Peso inteiro (`= 0`) é aceito como 0.0.
        assert_eq!(p.pesos.repeticao_atual, 0.0);
        assert_eq!(carregado.toml, VALIDO);
        assert_eq!(p.proxima_versao(|_| false), ("teste_v3".to_string(), 3));
        // Versões já em uso são puladas.
        assert_eq!(
            p.proxima_versao(|nome| nome == "teste_v3" || nome == "teste_v4"),
            ("teste_v5".to_string(), 5)
        );
    }

    #[test]
    fn toda_janela_calculada_e_aceita() {
        for janela in JANELAS {
            let texto = trocar(VALIDO, "janela = 0", &format!("janela = {janela}"));
            let p = interpretar(&texto, &origem()).expect("janela válida");
            assert_eq!(p.perfil.janela, janela);
        }
    }

    #[test]
    fn crlf_e_normalizado_no_texto_guardado() {
        let crlf = VALIDO.replace('\n', "\r\n");
        let a = interpretar(&crlf, &origem()).expect("CRLF válido");
        let b = interpretar(VALIDO, &origem()).expect("LF válido");
        assert_eq!(a, b);
    }

    // --- hash ---------------------------------------------------------------

    #[test]
    fn hash_e_sha256_hex_minusculo() {
        // Vetor do FIPS 180-2 para "abc".
        assert_eq!(
            hash_toml("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hash_igual_para_crlf_e_lf() {
        // sha256 de "a\nb\n" (conferido com `sha256sum`).
        let esperado = "911169ddaaf146aff539f58c26c489af3b892dff0fe283c1c264c65ae5aa59a2";
        assert_eq!(hash_toml("a\nb\n"), esperado);
        assert_eq!(hash_toml("a\r\nb\r\n"), esperado);
        assert_eq!(hash_toml("a\r\nb\n"), esperado);
        assert_ne!(hash_toml("a\nb"), esperado);
    }

    #[test]
    fn bom_e_ignorado_no_texto_e_no_hash() {
        let com_bom = format!("\u{feff}{}", VALIDO.replace('\n', "\r\n"));
        let a = interpretar(&com_bom, &origem()).expect("BOM aceito");
        let b = interpretar(VALIDO, &origem()).expect("válido");
        assert_eq!(a.toml, VALIDO);
        assert_eq!(a.hash_toml, b.hash_toml);
        assert_eq!(a.perfil, b.perfil);
    }

    // --- validação: um caso de erro por regra -------------------------------

    #[test]
    fn peso_faltando_e_erro() {
        let texto = trocar(VALIDO, "espaco_final              = 1.2\n", "");
        let motivo = motivo_do_erro(&texto);
        assert!(motivo.contains("espaco_final"), "{motivo}");
    }

    #[test]
    fn chave_desconhecida_no_topo_e_erro() {
        let texto = trocar(VALIDO, "janela = 0\n", "janela = 0\ndescricao = \"x\"\n");
        let motivo = motivo_do_erro(&texto);
        assert!(motivo.contains("descricao"), "{motivo}");
    }

    #[test]
    fn chave_desconhecida_em_pesos_e_erro() {
        let texto = format!("{VALIDO}soma_espacos = 1.0\n");
        let motivo = motivo_do_erro(&texto);
        assert!(motivo.contains("soma_espacos"), "{motivo}");
    }

    #[test]
    fn tabela_desconhecida_e_erro() {
        let texto = format!("{VALIDO}\n[extra]\nx = 1\n");
        let motivo = motivo_do_erro(&texto);
        assert!(motivo.contains("extra"), "{motivo}");
    }

    #[test]
    fn janela_invalida_e_erro() {
        for janela in ["20", "-10"] {
            let texto = trocar(VALIDO, "janela = 0", &format!("janela = {janela}"));
            let motivo = motivo_do_erro(&texto);
            assert!(motivo.contains("janela"), "{motivo}");
        }
    }

    #[test]
    fn normalizacao_invalida_e_erro() {
        for norm in ["\"robusta\"", "\"ZSCORE\""] {
            let texto = trocar(
                VALIDO,
                "normalizacao = \"minmax\"",
                &format!("normalizacao = {norm}"),
            );
            let motivo = motivo_do_erro(&texto);
            assert!(motivo.contains("normalizacao"), "{motivo}");
        }
    }

    #[test]
    fn versao_que_nao_bate_com_o_sufixo_e_erro() {
        let texto = trocar(VALIDO, "versao = 2", "versao = 1");
        let motivo = motivo_do_erro(&texto);
        assert!(motivo.contains("versao"), "{motivo}");
    }

    #[test]
    fn versao_faltando_e_erro() {
        let texto = trocar(VALIDO, "versao = 2\n", "");
        let motivo = motivo_do_erro(&texto);
        assert!(motivo.contains("versao"), "{motivo}");
    }

    #[test]
    fn nome_sem_sufixo_de_versao_valido_e_erro() {
        for nome in [
            "teste",
            "teste_v",
            "teste_v0",
            "teste_v02",
            "teste_v+2",
            "teste_vx",
            "_v2",
            "teste-v2",
            "teste_v2a",
        ] {
            let texto = trocar(VALIDO, "\"teste_v2\"", &format!("\"{nome}\""));
            let motivo = motivo_do_erro(&texto);
            assert!(motivo.contains("nome"), "{nome}: {motivo}");
        }
    }

    #[test]
    fn nome_com_caracteres_de_caminho_e_erro() {
        for nome in ["../teste_v2", "a/teste_v2", "a b_v2", "teste.x_v2"] {
            let texto = trocar(VALIDO, "\"teste_v2\"", &format!("\"{nome}\""));
            let motivo = motivo_do_erro(&texto);
            assert!(motivo.contains("nome"), "{nome}: {motivo}");
        }
    }

    #[test]
    fn loteria_inexistente_e_erro() {
        let texto = trocar(VALIDO, "\"quina\"", "\"lotomania\"");
        let motivo = motivo_do_erro(&texto);
        assert!(motivo.contains("lotomania"), "{motivo}");
    }

    #[test]
    fn peso_nao_finito_e_erro() {
        for valor in ["nan", "inf", "-inf"] {
            let texto = trocar(
                VALIDO,
                "menor_espaco              = 0.5",
                &format!("menor_espaco = {valor}"),
            );
            let motivo = motivo_do_erro(&texto);
            assert!(motivo.contains("pesos.menor_espaco"), "{valor}: {motivo}");
        }
    }

    #[test]
    fn toml_malformado_e_erro() {
        motivo_do_erro("nome = \n");
    }

    // --- leitura do disco ---------------------------------------------------

    #[test]
    fn carrega_do_disco_a_partir_da_raiz() {
        let raiz = dir_temporario("carrega");
        gravar(
            &raiz,
            "quina",
            "teste_v2.toml",
            &VALIDO.replace('\n', "\r\n"),
        );
        let carregado = carregar(&raiz, cfg("quina"), "teste_v2").expect("carrega");
        assert_eq!(carregado.perfil.nome, "teste_v2");
        assert_eq!(carregado.toml, VALIDO);
        assert_eq!(carregado.hash_toml, hash_toml(VALIDO));
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn nome_que_nao_bate_com_o_arquivo_e_erro() {
        let raiz = dir_temporario("nome_arquivo");
        // Conteúdo de `teste_v2` gravado como `outro_v2.toml`.
        gravar(&raiz, "quina", "outro_v2.toml", VALIDO);
        match carregar(&raiz, cfg("quina"), "outro_v2") {
            Err(SpiderError::PerfilInvalido { origem, motivo }) => {
                assert!(origem.ends_with("quina/outro_v2.toml"), "{origem:?}");
                assert!(motivo.contains("nome"), "{motivo}");
            }
            outro => panic!("esperava PerfilInvalido, veio {outro:?}"),
        }
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn loteria_que_nao_bate_com_a_pasta_e_erro() {
        let raiz = dir_temporario("loteria_pasta");
        // Perfil da Quina gravado na pasta da Mega-Sena.
        gravar(&raiz, "megasena", "teste_v2.toml", VALIDO);
        match carregar(&raiz, cfg("megasena"), "teste_v2") {
            Err(SpiderError::PerfilInvalido { origem, motivo }) => {
                assert!(origem.ends_with("megasena/teste_v2.toml"), "{origem:?}");
                assert!(motivo.contains("loteria"), "{motivo}");
                assert!(motivo.contains("megasena"), "{motivo}");
            }
            outro => panic!("esperava PerfilInvalido, veio {outro:?}"),
        }
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn arquivo_inexistente_e_erro_de_io_com_o_caminho() {
        let raiz = dir_temporario("inexistente");
        match carregar(&raiz, cfg("quina"), "nao_existe_v1") {
            Err(SpiderError::Io { caminho, .. }) => {
                assert!(caminho.ends_with("quina/nao_existe_v1.toml"), "{caminho:?}");
            }
            outro => panic!("esperava Io, veio {outro:?}"),
        }
    }

    #[test]
    fn nome_com_separador_de_caminho_nao_chega_ao_disco() {
        let raiz = dir_temporario("separador");
        assert!(matches!(
            carregar(&raiz, cfg("quina"), "../quina/teste_v2"),
            Err(SpiderError::PerfilInvalido { .. })
        ));
    }
}
