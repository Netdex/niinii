use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};
use thiserror::Error;

pub mod types;

pub fn romanize(path: &str, s: &str) -> Result<types::Root, IchiranError> {
    let working_dir = Path::new(path).parent().ok_or(io::Error::new(
        io::ErrorKind::NotFound,
        "ichiran has no parent dir",
    ))?;
    let output = Command::new(path)
        .current_dir(working_dir)
        .arg("-f")
        .arg(s)
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
    #[error("process i/o error")]
    Io(#[from] io::Error),
    #[error("serde error")]
    Serde(#[from] serde_json::Error),
    #[error("ichiran-cli exited w/ status {status:?}:\n{stderr}")]
    Failure { status: ExitStatus, stderr: String },
}
