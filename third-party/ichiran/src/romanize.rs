use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::coerce::*;

// Reverse-engineered from the JSON output of ichiran-cli since I can't read lisp.
// Disclaimer: Might be wrong in several ways. I pulled names for some of the
// grammatical structures out of my ass.

/// The root of a parse tree.
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct Root(Vec<Segment>);
impl Root {
    /// Get all segments under the parse tree.
    pub fn segments(&self) -> &[Segment] {
        &self.0
    }
}

/// A segment, representing either a skipped string or a list of candidate clauses.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(untagged, deny_unknown_fields)]
pub enum Segment {
    Skipped(String),
    Clauses(Vec<Clause>),
}

/// A clause, representing a segmented romanization.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct Clause(Vec<Romanized>, i32);
impl Clause {
    /// Get all romanized blocks in this clause.
    pub fn romanized(&self) -> &[Romanized] {
        &self.0
    }
    /// Get the cumulative score of this clause.
    pub fn score(&self) -> i32 {
        self.1
    }
}

/// A romanized term along with metadata.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
// TODO not sure what the last field is, maybe for limit != 1
pub struct Romanized(String, Term, Vec<u8>);
impl Romanized {
    /// Get the romaji string of this romanized block.
    pub fn romaji(&self) -> &str {
        self.0.as_str()
    }
    /// Get the split metadata of this romanized block.
    pub fn term(&self) -> &Term {
        &self.1
    }
}

/// A term, representing either a word or a list of alternatives.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(untagged, deny_unknown_fields)]
pub enum Term {
    Word(Word),
    Alternative(Alternative),
}
impl Term {
    /// Get the original text for this term
    pub fn text(&self) -> &str {
        self.best().meta().text()
    }

    /// Get the kana for this term
    pub fn kana(&self) -> &str {
        self.best().meta().kana()
    }

    /// Get the word or best alternative
    pub fn best(&self) -> &Word {
        match self {
            Term::Word(word) => word,
            Term::Alternative(alt) => alt.alts().iter().max_by_key(|x| x.meta().score()).unwrap(),
        }
    }
}

/// An alternative, representing multiple words.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Alternative {
    alternative: Vec<Word>,
}
impl Alternative {
    /// Get a list of alternative words.
    pub fn alts(&self) -> &[Word] {
        &self.alternative
    }
}

/// A word, representing either a plain word or a compound word.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(untagged, deny_unknown_fields)]
pub enum Word {
    Plain(Plain),
    Compound(Compound),
}
impl Word {
    /// Get the metadata block of this word.
    pub fn meta(&self) -> &Meta {
        match self {
            Word::Plain(Plain { meta, .. }) | Word::Compound(Compound { meta, .. }) => meta,
        }
    }
}

/// A plain word.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct Plain {
    #[serde(flatten)]
    meta: Meta,

    seq: Option<u32>,
    #[serde(default)]
    gloss: Vec<Gloss>,
    #[serde(default)]
    conj: Vec<Conjugation>,

    counter: Option<Counter>,
    suffix: Option<String>,
}
impl Plain {
    // Get the meta of this word.
    pub fn meta(&self) -> &Meta {
        &self.meta
    }
    /// Get the sequence number of this word.
    pub fn seq(&self) -> Option<u32> {
        self.seq
    }
    /// Get a list of glosses.
    pub fn gloss(&self) -> &[Gloss] {
        &self.gloss
    }
    /// Get the conjugation of this word.
    pub fn conj(&self) -> &[Conjugation] {
        &self.conj
    }
    /// Get the counter data of this word.
    pub fn counter(&self) -> Option<&Counter> {
        self.counter.as_ref()
    }
    /// Get the suffix data of this word.
    pub fn suffix(&self) -> Option<&str> {
        self.suffix.as_deref()
    }
}

/// A compound word.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct Compound {
    #[serde(flatten)]
    meta: Meta,

    compound: Vec<String>,
    components: Vec<Term>,
}
impl Compound {
    // Get the meta of this compound.
    pub fn meta(&self) -> &Meta {
        &self.meta
    }
    /// Get a list of romaji components in this compound.
    pub fn compound(&self) -> &[String] {
        &self.compound
    }
    /// Get the split metadata for each component of this compound.
    pub fn components(&self) -> &[Term] {
        &self.components
    }
}

/// Common metadata for a term.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Meta {
    reading: String,
    text: String,
    kana: String,
    score: u32,
}
impl Meta {
    /// Get the reading of this term.
    pub fn reading(&self) -> &str {
        self.reading.as_str()
    }
    /// Get the original text of this term.
    pub fn text(&self) -> &str {
        self.text.as_str()
    }
    /// Get the kana representation of this term.
    pub fn kana(&self) -> &str {
        self.kana.as_str()
    }
    /// Get the score of this term.
    pub fn score(&self) -> u32 {
        self.score
    }
}

/// Gloss for a word.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct Gloss {
    pos: String,
    gloss: String,
    info: Option<String>,
}
impl Gloss {
    /// Get part-of-speech info.
    pub fn pos(&self) -> &str {
        self.pos.as_str()
    }
    /// Get individual part-of-speech info.
    pub fn pos_split(&self) -> Vec<&str> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"[\w-]+").unwrap();
        }
        let captures = RE.captures_iter(self.pos.as_str());
        captures
            .filter_map(|cap| cap.get(0))
            .map(|m| m.as_str())
            .collect::<Vec<&str>>()
    }
    /// Get the gloss explanation.
    pub fn gloss(&self) -> &str {
        self.gloss.as_str()
    }
    /// Get additional information.
    pub fn info(&self) -> Option<&str> {
        self.info.as_deref()
    }
}

/// Conjugations for a word.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct Conjugation {
    prop: Vec<Property>,
    reading: Option<String>,
    #[serde(default)]
    gloss: Vec<Gloss>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[allow(clippy::vec_box)]
    via: Vec<Box<Conjugation>>,
    #[serde(deserialize_with = "bool_seq::deserialize")]
    readok: bool,
}
impl Conjugation {
    /// Get a list of conjugation properties.
    pub fn prop(&self) -> &[Property] {
        &self.prop
    }
    /// Get the reading for this conjugation.
    pub fn reading(&self) -> Option<&str> {
        self.reading.as_deref()
    }
    /// Get a list of glosses.
    pub fn gloss(&self) -> &[Gloss] {
        &self.gloss
    }
    /// Get the source of the conjugation.
    pub fn vias(&self) -> Vec<&Conjugation> {
        self.via.iter().map(Box::as_ref).collect()
    }
    /// TODO no idea what this is
    pub fn readok(&self) -> bool {
        self.readok
    }
    /// Convert the via tree into a list of reverse root-to-leaf sequences,
    /// representing all possible via paths to reach this conjugation.
    /// e.g.
    /// The tree with edges
    ///     [A -> B, B -> C, A -> D]
    /// is converted to the sequence
    ///     [C -> B -> A, D -> a]
    pub fn flatten(&self) -> Vec<Vec<&Conjugation>> {
        fn chain<'a>(
            mut head: Vec<&'a Conjugation>,
            tail: &'a Conjugation,
        ) -> Vec<Vec<&'a Conjugation>> {
            head.insert(0, tail);
            if tail.vias().is_empty() {
                vec![head]
            } else {
                let mut agg = vec![];
                for via in tail.vias() {
                    agg.extend(chain(head.clone(), via));
                }
                agg
            }
        }
        chain(vec![], self)
    }
}

/// Property of a conjugation.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
impl Property {
    /// Get part-of-speech info.
    pub fn pos(&self) -> &str {
        self.pos.as_str()
    }
    /// Get the conjugation type.
    pub fn kind(&self) -> &str {
        self.kind.as_str()
    }
    /// Get whether the conjugation is negative.
    pub fn neg(&self) -> bool {
        self.neg
    }
    /// Get whether the conjugation is formal.
    pub fn fml(&self) -> bool {
        self.fml
    }
}

/// Counter info for a word.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct Counter {
    value: String,
    #[serde(deserialize_with = "bool_seq::deserialize")]
    ordinal: bool,
}
impl Counter {
    /// Get the value of the counter.
    pub fn value(&self) -> &str {
        self.value.as_str()
    }
    /// Get whether the counter is ordinal.
    pub fn ordinal(&self) -> bool {
        self.ordinal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn test_pos_split() {
        let gloss = Gloss {
            pos: "[n,n-adv,prt]".to_owned(),
            gloss: "".to_owned(),
            info: None,
        };
        assert_eq!(gloss.pos_split(), vec!["n", "n-adv", "prt"]);
    }

    #[test]
    fn test_match() {
        let (ichiran, _pg) = fixture::ichiran();
        let nikaime = ichiran.romanize("2回目", 1).unwrap();
        let nikaime_gold = Root(vec![Segment::Clauses(vec![Clause(
            vec![Romanized(
                "nikaime".into(),
                Term::Word(Word::Plain(Plain {
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
                })),
                vec![],
            )],
            696,
        )])]);
        assert_eq!(nikaime, nikaime_gold);
    }

    #[test]
    fn test_deserialize() {
        let (ichiran, _pg) = fixture::ichiran();
        let _furaseteiru = ichiran.romanize("降らせている", 1).unwrap();
        let _naidesho = ichiran.romanize("ないでしょ", 1).unwrap();
    }
}
