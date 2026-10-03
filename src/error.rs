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

    #[error("erro de banco de dados: {0}")]
    Db(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, SpiderError>;
