use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum IchiranError {
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("CSV Error: {0}")]
    CsvError(#[from] csv_async::Error),
    #[error("JSON Error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("ichiran-cli server error: {0}")]
    Server(String),
    #[error("ichiran-cli server has gone away")]
    ServerGone,
}
