use ketos::FromValue;

pub fn lisp_interpret<T>(expr: &str) -> Result<T, ketos::Error>
where
    T: FromValue,
{
    let interp = ketos::Interpreter::new();
    let result = interp.run_single_expr(expr, None)?;
    T::from_value(result).map_err(ketos::Error::ExecError)
}

pub fn lisp_escape_string(text: impl AsRef<str>) -> String {
    let text = text.as_ref();
    let mut output = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => {
                output += r#"\""#;
            }
            '\\' => {
                output += r#"\\"#;
            }
            x => output.push(x),
        }
    }
    output
}
