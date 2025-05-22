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
