use std::{io, process::ExitStatus};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum IchiranError {
    #[error("I/O Error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON Error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("ichiran-cli exited w/ {status}\n{stderr}")]
    Failure { status: ExitStatus, stderr: String },
    #[error("Parse Error:\n{0}")]
    Parsing(String),
    #[error("CSV Error: {0}")]
    CsvError(#[from] csv_async::Error),
    #[error("Lexpr Error: {0}")]
    SerdeLisp(#[from] serde_lexpr::Error),
}
