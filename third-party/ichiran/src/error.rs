use super::MAX_TEXT_LENGTH;
use std::{io, process::ExitStatus};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum IchiranError {
    #[error("I/O Error: {0}")]
    Io(#[from] io::Error),
    #[error("Serde Error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("ichiran-cli exited w/ {status}\n{stderr}")]
    Failure { status: ExitStatus, stderr: String },
    #[error("Text too long ({length}/{MAX_TEXT_LENGTH} chars)")]
    TextTooLong { length: usize },
    #[error("Parse Error:\n{0}")]
    Parsing(String),
    #[error("CSV Error: {0}")]
    CsvError(#[from] csv::Error),
    #[error("Lisp Error:\n{0}")]
    KetosError(String),
}

// ketos::Error is non-Send so we need to serialize it.
impl From<ketos::Error> for IchiranError {
    fn from(err: ketos::Error) -> Self {
        IchiranError::KetosError(format!("{:#?}", err))
    }
}
