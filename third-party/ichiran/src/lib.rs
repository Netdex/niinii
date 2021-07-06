pub use error::IchiranError;
pub use jmdict_data::JmDictData;
use std::{
    io::{self, BufRead, ErrorKind},
    path::{Path, PathBuf},
    process::Command,
};

mod coerce;
pub mod error;
pub mod jmdict_data;
pub mod types;

const MAX_TEXT_LENGTH: usize = 512;

pub struct Ichiran<'a> {
    path: &'a str,
}
impl<'a> Ichiran<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }
    pub fn romanize(&self, text: &str) -> Result<types::Root, IchiranError> {
        if text.len() > MAX_TEXT_LENGTH {
            return Err(IchiranError::TextTooLong { length: text.len() });
        }
        let working_dir = self.working_dir()?;
        let output = Command::new(self.path)
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

    pub fn jmdict_data(&self) -> Result<JmDictData, IchiranError> {
        JmDictData::new(&self.jmdict_path()?)
    }

    fn jmdict_path(&self) -> Result<PathBuf, IchiranError> {
        let working_dir = self.working_dir()?;
        let output = Command::new(self.path)
            .current_dir(working_dir)
            .arg("-e")
            .arg(r#"(format t "~d" ichiran/dict::*jmdict-data*)"#)
            .output()?;

        if output.status.success() {
            let jmdict_path =
                output
                    .stdout
                    .lines()
                    .next()
                    .ok_or_else(|| IchiranError::Failure {
                        status: output.status,
                        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    })??;
            Ok(working_dir.join(jmdict_path))
        } else {
            Err(IchiranError::Failure {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    fn working_dir(&self) -> Result<&Path, io::Error> {
        Path::new(self.path).parent().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "Could not find working directory of ichiran-cli",
            )
        })
    }
}
