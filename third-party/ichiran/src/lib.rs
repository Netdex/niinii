use std::{
    collections::HashMap,
    ffi::OsStr,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::Command,
};

mod coerce;
pub mod error;
pub mod jmdict_data;
pub mod kanji;
pub mod romanize;

pub use error::IchiranError;
pub use jmdict_data::JmDictData;
use kanji::{is_kanji, Kanji};
use ketos::FromValue;
use romanize::Root;

const MAX_TEXT_LENGTH: usize = 512;

#[derive(Debug)]
pub struct ConnParams {
    pub database: String,
    pub user: String,
    pub password: String,
    pub hostname: String,
    pub port: u16,
}

pub struct Ichiran<'a> {
    path: &'a str,
}
impl<'a> Ichiran<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn romanize(&self, text: &str) -> Result<Root, IchiranError> {
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
            let root: Root = serde_json::from_slice(&output.stdout)?;
            Ok(root)
        } else {
            Err(IchiranError::Failure {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    pub fn kanji(&self, chars: &[char]) -> Result<HashMap<char, Kanji>, IchiranError> {
        let mut kanji_info: HashMap<char, Kanji> = HashMap::new();
        if chars.len() == 0 {
            return Ok(kanji_info);
        }

        // this is what happens when you don't know lisp
        let commands: Vec<String> = chars
            .iter()
            .map(|chr| {
                format!(
                    r#"(jsown:to-json (ichiran/kanji:kanji-info-json #\{}))"#,
                    chr
                )
            })
            .collect();
        let expr = format!("(list {})", commands.join(" "));

        let output = self.ichiran_eval(expr)?;
        let output = format!("'{}", output); // add an apostrophe to turn it into a list
        let output: Vec<String> = self.lisp_interp(&output)?;

        output.iter().try_for_each(|x| {
            if *x != "[]" {
                let kanji: kanji::Kanji = serde_json::from_str(x)?;
                kanji_info.insert(kanji.text().chars().next().unwrap(), kanji);
            }
            Ok::<(), IchiranError>(())
        })?;
        Ok(kanji_info)
    }

    pub fn kanji_from_str(&self, text: &str) -> Result<HashMap<char, Kanji>, IchiranError> {
        let mut uniq: Vec<char> = text.chars().filter(is_kanji).collect();
        uniq.sort();
        uniq.dedup();

        self.kanji(&uniq)
    }

    pub fn jmdict_data(&self) -> Result<JmDictData, IchiranError> {
        JmDictData::new(&self.jmdict_path()?)
    }

    fn jmdict_path(&self) -> Result<PathBuf, IchiranError> {
        let working_dir = self.working_dir()?;
        let jmdict_path = self.ichiran_eval(r#"(format t "~d" ichiran/dict::*jmdict-data*)"#)?;
        let jmdict_path = jmdict_path
            .lines()
            .next()
            .ok_or_else(|| IchiranError::Parsing(jmdict_path.clone()))?;
        Ok(working_dir.join(jmdict_path))
    }

    pub fn conn_params(&self) -> Result<ConnParams, IchiranError> {
        let conn_params =
            self.ichiran_eval(r#"(format t "~{~a~^,~}" ichiran/conn::*connection*)"#)?;
        let parse_error = || IchiranError::Parsing(conn_params.clone());

        let mut conn_params = conn_params
            .lines()
            .next()
            .ok_or_else(parse_error)?
            .split(',');

        let database = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let user = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let password = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let hostname = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let _ = conn_params.next().ok_or_else(parse_error)?.to_owned();
        let port = conn_params
            .next()
            .ok_or_else(parse_error)?
            .parse::<u16>()
            .map_err(|_| parse_error())?;

        Ok(ConnParams {
            database,
            user,
            password,
            hostname,
            port,
        })
    }

    /// Evaluate the expression with ichiran and return the raw output.
    fn ichiran_eval<E: AsRef<OsStr>>(&self, expr: E) -> Result<String, IchiranError> {
        let working_dir = self.working_dir()?;
        let output = Command::new(self.path)
            .current_dir(working_dir)
            .arg("-e")
            .arg(expr)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout).unwrap())
        } else {
            Err(IchiranError::Failure {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    fn lisp_interp<T>(&self, expr: &str) -> Result<T, IchiranError>
    where
        T: FromValue,
    {
        let interp = ketos::Interpreter::new();
        let result = interp.run_single_expr(expr, None)?;
        Ok(T::from_value(result).map_err(|err| ketos::Error::ExecError(err))?)
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
