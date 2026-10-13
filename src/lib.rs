pub mod avaliacao;
pub mod backup;
pub mod calendario;
pub mod config;
/// Ferramenta dos vetores de conformidade (testes e `examples/`), não é produção.
#[doc(hidden)]
pub mod conformidade;
pub mod correcoes;
pub mod dataset;
pub mod db;
pub mod downloader;
pub mod error;
pub mod exportacao;
pub mod manuais;
pub mod model;
pub mod motor;
pub mod orquestracao;
pub mod parser;
pub mod perfil;
pub mod publicacao;
pub mod registros;
pub mod tabela;
pub mod tempo;
pub mod trava;
pub mod validacao;

pub use error::{Result, SpiderError};
pub use model::Resultado;
