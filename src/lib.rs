pub mod config;
pub mod db;
pub mod downloader;
pub mod error;
pub mod model;
pub mod parser;

pub use error::{Result, SpiderError};
pub use model::Resultado;
