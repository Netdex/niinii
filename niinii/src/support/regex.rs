use fancy_regex::Regex;

#[derive(Default)]
pub struct CachedRegex {
    pattern: String,
    regex: Option<Regex>,
}

impl CachedRegex {
    pub fn get(&mut self, pattern: &str) -> Result<&Regex, fancy_regex::Error> {
        if self.regex.is_none() || self.pattern != pattern {
            self.regex = Some(Regex::new(pattern)?);
            self.pattern.clear();
            self.pattern.push_str(pattern);
        }
        Ok(self.regex.as_ref().unwrap())
    }
}
