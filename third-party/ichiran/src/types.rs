use serde::{
    de::{self, IgnoredAny, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use std::fmt;

// Reverse-engineered from the JSON output of ichiran-cli since I can't read lisp.
// Disclaimer: Might be wrong in several ways.
// We don't use zero-copy because JSON has escapes.

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Root(Vec<Segment>);

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Segment {
    Skipped(String),
    Clauses(Vec<Clause>),
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Clause(Vec<Romaji>, u32);

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Romaji(String, Term, Vec<u8>);

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Meta {
    reading: String,
    text: String,
    kana: String,
    score: u32,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Word {
    Plain {
        #[serde(flatten)]
        meta: Meta,

        seq: Option<u32>,
        #[serde(default)]
        gloss: Vec<Gloss>,
        #[serde(default)]
        conj: Vec<Conjugation>,

        counter: Option<Counter>,
        suffix: Option<String>,
    },
    Compound {
        #[serde(flatten)]
        meta: Meta,

        compound: Vec<String>,
        components: Vec<Term>,
    },
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Term {
    Word(Word),
    Alternative { alternative: Vec<Word> },
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Gloss {
    pos: String,
    gloss: String,
    info: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Conjugation {
    prop: Vec<Property>,
    reading: Option<String>,
    #[serde(default)]
    gloss: Vec<Gloss>,
    #[serde(default)]
    via: Vec<Conjugation>,
    readok: bool,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Property {
    /// Part-of-speech
    pos: String,
    /// Type
    #[serde(rename = "type")]
    kind: String,
    /// Negative
    #[serde(default)]
    neg: bool,
    /// Formal
    #[serde(default)]
    fml: bool,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Counter {
    value: String,
    #[serde(deserialize_with = "deserialize_anomalous_bool")]
    ordinal: bool,
}

/// Special deserializer to handle `Counter::ordinal` which is either `[]` (false) or `bool`.
fn deserialize_anomalous_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoolVisitor;
    impl<'de> Visitor<'de> for BoolVisitor {
        type Value = bool;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("[] or bool")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            match seq.next_element()? {
                Some(IgnoredAny) => Err(de::Error::invalid_length(0, &self)),
                None => Ok(false),
            }
        }
        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v)
        }
    }
    deserializer.deserialize_any(BoolVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        const ICHIRAN_FULL: &str = r#"[
			[ [ [ [ "nikaime", { "reading":"2\u56DE\u76EE \u3010\u306B\u304B\u3044\u3081\u3011",
			"text":"2\u56DE\u76EE", "kana":"\u306B\u304B\u3044\u3081", "score":696, "counter":{
			"value":"Value: 2nd", "ordinal":true }, "seq":1199330, "gloss":[ { "pos":"[ctr]",
			"gloss":"counter for occurrences" } ] }, [ ] ] ], 696 ] ] ]"#;
        let a = serde_json::from_str::<Root>(ICHIRAN_FULL).unwrap();
        let b = Root(vec![Segment::Clauses(vec![Clause(
            vec![Romaji(
                "nikaime".into(),
                Term::Word(Word::Plain {
                    meta: Meta {
                        reading: "2回目 【にかいめ】".into(),
                        text: "2回目".into(),
                        kana: "にかいめ".into(),
                        score: 696,
                    },
                    seq: Some(1199330),
                    gloss: vec![Gloss {
                        pos: "[ctr]".into(),
                        gloss: "counter for occurrences".into(),
                        info: None,
                    }],
                    conj: vec![],
                    counter: Some(Counter {
                        value: "Value: 2nd".into(),
                        ordinal: true,
                    }),
                    suffix: None,
                }),
                vec![],
            )],
            696,
        )])]);
        assert_eq!(a, b);
    }
}
