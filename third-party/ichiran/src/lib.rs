use std::{
    io::{self, ErrorKind},
    path::Path,
    process::{Command, ExitStatus},
};
use thiserror::Error;

mod coerce;
pub mod types;

const MAX_TEXT_LENGTH: usize = 512;

pub fn romanize(path: &str, text: &str) -> Result<types::Root, IchiranError> {
    if text.len() > MAX_TEXT_LENGTH {
        return Err(IchiranError::TextTooLong { length: text.len() });
    }
    let working_dir = Path::new(path).parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::NotFound,
            "Could not find working directory of ichiran-cli",
        )
    })?;
    let output = Command::new(path)
        .current_dir(working_dir)
        .arg("-f")
        .arg(text)
        .output()?;

    if output.status.success() {
        let root: types::Root = serde_json::from_slice(&output.stdout)?;
        Ok(root)
    } else {
        Err(IchiranError::Failure {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

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
}
