use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpiderError {
    #[error("loteria desconhecida: `{0}`")]
    LoteriaDesconhecida(String),

    #[error("erro do navegador ao acessar `{url}`: {detalhe}")]
    Navegador { url: String, detalhe: String },

    #[error("elemento de download não encontrado (seletor `{0}`) — a página pode ter mudado de layout ou bloqueado o acesso")]
    BotaoDownloadNaoEncontrado(String),

    #[error("nenhum download foi iniciado após clicar no seletor `{0}`")]
    DownloadNaoIniciado(String),

    #[error("elemento não encontrado (seletor `{0}`) — a página pode ter mudado de layout ou bloqueado o acesso")]
    ElementoNaoEncontrado(String),

    #[error("erro de E/S em `{caminho}`")]
    Io {
        caminho: PathBuf,
        #[source]
        origem: std::io::Error,
    },

    #[error("erro ao ler a planilha `{caminho}`: {detalhe}")]
    Planilha { caminho: PathBuf, detalhe: String },

    #[error("tabela de resultados não encontrada ou em layout inesperado em `{0}`")]
    TabelaNaoEncontrada(PathBuf),

    #[error("dado inválido: {0}")]
    LinhaInvalida(String),

    #[error("cache de inserções manuais `{caminho}` inválido: {motivo}")]
    CacheManualInvalido { caminho: PathBuf, motivo: String },

    /// O motor conta as janelas pelo número do concurso, então não aceita
    /// buraco na numeração (ver "Convenções" em `docs/PLANEJAMENTO.md`).
    #[error("motor: faltam os concursos {esperado}..={} antes do {recebido} — feche o buraco (`sync buracos`/`sync inserir`) antes de calcular as features", recebido.saturating_sub(1))]
    MotorBuraco { esperado: u32, recebido: u32 },

    #[error("motor: concurso {0} recebido de novo — cada concurso entra uma única vez")]
    MotorConcursoRepetido(u32),

    #[error("motor: concurso {recebido} fora de ordem (o último incorporado foi o {ultimo}) — os concursos devem vir em ordem crescente")]
    MotorForaDeOrdem { ultimo: u32, recebido: u32 },

    #[error("motor: concurso {concurso} inválido: {detalhe} — rode `sync validar`")]
    MotorResultadoInvalido { concurso: u32, detalhe: String },

    #[error(
        "motor: estado incompatível com a configuração atual: {0} — regenere as features do zero"
    )]
    MotorEstadoIncompativel(String),

    #[error("erro de banco de dados: {0}")]
    Db(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, SpiderError>;
