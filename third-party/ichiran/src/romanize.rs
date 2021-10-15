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

    /// Part-of-speech split
    #[test]
    fn test_pos_split() {
        let gloss = Gloss {
            pos: "[n,n-adv,prt]".to_owned(),
            gloss: "".to_owned(),
            info: None,
        };
        assert_eq!(gloss.pos_split(), vec!["n", "n-adv", "prt"]);
    }

    /// Full match nikaime
    #[test]
    fn test_nikaime() {
        const ICHIRAN_FULL: &str = r#"[[[[["nikaime",{"reading":"2回目 【にかいめ】","text":"2回目","kana":"にかいめ","score":696,"counter":{"value":"Value: 2nd","ordinal":true},"seq":1199330,"gloss":[{"pos":"[ctr]","gloss":"counter for occurrences"}]},[]]],696]]]"#;
        let a = serde_json::from_str::<Root>(ICHIRAN_FULL).unwrap();
        let b = Root(vec![Segment::Clauses(vec![Clause(
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
        assert_eq!(a, b);
    }

    /// Multiple vias example
    #[test]
    fn test_furaseteiru() {
        const ICHIRAN_FULL: &str = r#"[[[[["furaseteiru",{"reading":"\u964D\u3089\u305B\u3066\u3044\u308B \u3010\u3075\u3089\u305B\u3066\u3044\u308B\u3011","text":"\u964D\u3089\u305B\u3066\u3044\u308B","kana":"\u3075\u3089\u305B\u3066\u3044\u308B","score":704,"compound":["\u964D\u3089\u305B\u3066","\u3044\u308B"],"components":[{"reading":"\u964D\u3089\u305B\u3066 \u3010\u3075\u3089\u305B\u3066\u3011","text":"\u964D\u3089\u305B\u3066","kana":"\u3075\u3089\u305B\u3066","score":0,"seq":10383239,"conj":[{"prop":[{"pos":"v1","type":"Conjunctive (~te)"}],"reading":"\u964D\u3089\u305B\u308B \u3010\u3075\u3089\u305B\u308B\u3011","gloss":[{"pos":"[vt,v1]","gloss":"to send (rain); to shed"}],"readok":true},{"prop":[{"pos":"v1","type":"Conjunctive (~te)"}],"via":[{"prop":[{"pos":"v5s","type":"Potential"}],"reading":"\u964D\u3089\u3059 \u3010\u3075\u3089\u3059\u3011","gloss":[{"pos":"[vt,v5s]","gloss":"to send (rain); to shed"}],"readok":true},{"prop":[{"pos":"v5r","type":"Causative"}],"reading":"\u964D\u308B \u3010\u3075\u308B\u3011","gloss":[{"pos":"[v5r,vi]","gloss":"to fall (of rain, snow, ash, etc.); to come down"},{"pos":"[v5r,vi]","gloss":"to form (of frost)"},{"pos":"[v5r,vi]","gloss":"to beam down (of sunlight or moonlight); to pour in"},{"pos":"[vi,v5r]","gloss":"to visit (of luck, misfortune, etc.); to come; to arrive"}],"readok":true}],"readok":true}]},{"reading":"\u3044\u308B","text":"\u3044\u308B","kana":"\u3044\u308B","score":0,"seq":1577980,"suffix":"indicates continuing action (to be ...ing)","conj":[]}]},[]]],704]]]"#;
        let _root = serde_json::from_str::<Root>(ICHIRAN_FULL).unwrap();
    }

    /// readok false example
    #[test]
    fn test_naidesho() {
        const ICHIRAN_FULL: &str = r#"[[[[["nai desho",{"alternative":[{"reading":"\u306A\u3044\u3067\u3057\u3087","text":"\u306A\u3044\u3067\u3057\u3087","kana":"\u306A\u3044 \u3067\u3057\u3087","score":430,"compound":["\u306A\u3044","\u3067\u3057\u3087"],"components":[{"reading":"\u306A\u3044","text":"\u306A\u3044","kana":"\u306A\u3044","score":0,"seq":10560524,"conj":[{"prop":[{"pos":"v5r-i","type":"Non-past","neg":true}],"reading":"\u5728\u308B \u3010\u3042\u308B\u3011","gloss":[{"pos":"[v5r-i,vi]","gloss":"to be; to exist; to live","info":"usu. of inanimate objects"},{"pos":"[v5r-i,vi]","gloss":"to have"},{"pos":"[v5r-i,vi]","gloss":"to be located"},{"pos":"[v5r-i,vi]","gloss":"to be equipped with"},{"pos":"[vi,v5r-i]","gloss":"to happen; to come about"}],"readok":true}]},{"reading":"\u3067\u3057\u3087","text":"\u3067\u3057\u3087","kana":"\u3067\u3057\u3087","score":0,"seq":1008420,"suffix":"it seems/perhaps/don't you think?","conj":[{"prop":[{"pos":"cop","type":"Volitional","fml":true}],"reading":"\u3060","gloss":[{"pos":"[cop,cop-da]","gloss":"be; is","info":"plain copula"}],"readok":[]}]}]},{"reading":"\u306A\u3044\u3067\u3057\u3087","text":"\u306A\u3044\u3067\u3057\u3087","kana":"\u306A\u3044 \u3067\u3057\u3087","score":313,"compound":["\u306A\u3044","\u3067\u3057\u3087"],"components":[{"reading":"\u306A\u3044","text":"\u306A\u3044","kana":"\u306A\u3044","score":0,"seq":10625233,"conj":[{"prop":[{"pos":"v5u","type":"Continuative (~i)"}],"reading":"\u7DAF\u3046 \u3010\u306A\u3046\u3011","gloss":[{"pos":"[v5u]","gloss":"to twine (fibers to make rope); to twist"}],"readok":true}]},{"reading":"\u3067\u3057\u3087","text":"\u3067\u3057\u3087","kana":"\u3067\u3057\u3087","score":0,"seq":1008420,"suffix":"it seems/perhaps/don't you think?","conj":[{"prop":[{"pos":"cop","type":"Volitional","fml":true}],"reading":"\u3060","gloss":[{"pos":"[cop,cop-da]","gloss":"be; is","info":"plain copula"}],"readok":[]}]}]}]},[]]],430],[[["nai",{"alternative":[{"reading":"\u306A\u3044","text":"\u306A\u3044","kana":"\u306A\u3044","score":40,"seq":10560524,"conj":[{"prop":[{"pos":"v5r-i","type":"Non-past","neg":true}],"reading":"\u5728\u308B \u3010\u3042\u308B\u3011","gloss":[{"pos":"[v5r-i,vi]","gloss":"to be; to exist; to live","info":"usu. of inanimate objects"},{"pos":"[v5r-i,vi]","gloss":"to have"},{"pos":"[v5r-i,vi]","gloss":"to be located"},{"pos":"[v5r-i,vi]","gloss":"to be equipped with"},{"pos":"[vi,v5r-i]","gloss":"to happen; to come about"}],"readok":true}]},{"reading":"\u306A\u3044","text":"\u306A\u3044","kana":"\u306A\u3044","score":40,"seq":1529520,"gloss":[{"pos":"[adj-i]","gloss":"nonexistent; not being (there)"},{"pos":"[adj-i]","gloss":"unowned; not had; unpossessed"},{"pos":"[adj-i]","gloss":"unique"},{"pos":"[adj-i]","gloss":"not; impossible; won't happen","info":"as ...\u3053\u3068\u304C\u306A\u3044, etc.; indicates negation, inexperience, unnecessariness or impossibility"},{"pos":"[aux-adj]","gloss":"not","info":"after the ren'youkei form of an adjective"},{"pos":"[aux-adj]","gloss":"to not be; to have not","info":"after the -te form of a verb"}],"conj":[]}]},[]],["desho",{"reading":"\u3067\u3057\u3087","text":"\u3067\u3057\u3087","kana":"\u3067\u3057\u3087","score":12,"seq":1008420,"gloss":[{"pos":"[exp]","gloss":"it seems; I think; I guess; I wonder"},{"pos":"[exp]","gloss":"right?; don't you agree?"}],"conj":[]},[]]],52],[[["na",{"reading":"\u306A","text":"\u306A","kana":"\u306A","score":6,"seq":2029110,"gloss":[{"pos":"[prt]","gloss":"don't","info":"prohibitive; used with dictionary form verb"},{"pos":"[prt]","gloss":"do","info":"imperative (from \u306A\u3055\u3044); used with -masu stem of verb"},{"pos":"[int]","gloss":"hey; listen; you"},{"pos":"[prt]","gloss":"now, ...; well, ...; I tell you!; you know","info":"when seeking confirmation, for emphasis, etc.; used at sentence end"},{"pos":"[prt]","gloss":"wow; ooh","info":"used to express admiration, emotionality, etc.; used at sentence end"},{"pos":"[prt]","gloss":"indicates \u306A-adjective"}],"conj":[]},[]],["i",{"reading":"\u3044","text":"\u3044","kana":"\u3044","score":0},[]],["desho",{"reading":"\u3067\u3057\u3087","text":"\u3067\u3057\u3087","kana":"\u3067\u3057\u3087","score":12,"seq":1008420,"gloss":[{"pos":"[exp]","gloss":"it seems; I think; I guess; I wonder"},{"pos":"[exp]","gloss":"right?; don't you agree?"}],"conj":[]},[]]],-482],[[["naide",{"reading":"\u306A\u3044\u3067","text":"\u306A\u3044\u3067","kana":"\u306A\u3044\u3067","score":90,"seq":10560532,"conj":[{"prop":[{"pos":"v5r-i","type":"Conjunctive (~te)","neg":true}],"reading":"\u5728\u308B \u3010\u3042\u308B\u3011","gloss":[{"pos":"[v5r-i,vi]","gloss":"to be; to exist; to live","info":"usu. of inanimate objects"},{"pos":"[v5r-i,vi]","gloss":"to have"},{"pos":"[v5r-i,vi]","gloss":"to be located"},{"pos":"[v5r-i,vi]","gloss":"to be equipped with"},{"pos":"[vi,v5r-i]","gloss":"to happen; to come about"}],"readok":true}]},[]],["sho",{"reading":"\u3057\u3087","text":"\u3057\u3087","kana":"\u3057\u3087","score":0},[]]],-910],[[["nai",{"alternative":[{"reading":"\u306A\u3044","text":"\u306A\u3044","kana":"\u306A\u3044","score":40,"seq":10560524,"conj":[{"prop":[{"pos":"v5r-i","type":"Non-past","neg":true}],"reading":"\u5728\u308B \u3010\u3042\u308B\u3011","gloss":[{"pos":"[v5r-i,vi]","gloss":"to be; to exist; to live","info":"usu. of inanimate objects"},{"pos":"[v5r-i,vi]","gloss":"to have"},{"pos":"[v5r-i,vi]","gloss":"to be located"},{"pos":"[v5r-i,vi]","gloss":"to be equipped with"},{"pos":"[vi,v5r-i]","gloss":"to happen; to come about"}],"readok":true}]},{"reading":"\u306A\u3044","text":"\u306A\u3044","kana":"\u306A\u3044","score":40,"seq":1529520,"gloss":[{"pos":"[adj-i]","gloss":"nonexistent; not being (there)"},{"pos":"[adj-i]","gloss":"unowned; not had; unpossessed"},{"pos":"[adj-i]","gloss":"unique"},{"pos":"[adj-i]","gloss":"not; impossible; won't happen","info":"as ...\u3053\u3068\u304C\u306A\u3044, etc.; indicates negation, inexperience, unnecessariness or impossibility"},{"pos":"[aux-adj]","gloss":"not","info":"after the ren'youkei form of an adjective"},{"pos":"[aux-adj]","gloss":"to not be; to have not","info":"after the -te form of a verb"}],"conj":[]}]},[]],["de",{"reading":"\u3067","text":"\u3067","kana":"\u3067","score":11,"seq":2028980,"gloss":[{"pos":"[prt]","gloss":"at; in","info":"indicates location of action; \u306B\u3066 is the formal literary form"},{"pos":"[prt]","gloss":"at; when","info":"indicates time of action"},{"pos":"[prt]","gloss":"by; with","info":"indicates means of action"},{"pos":"[conj]","gloss":"and then; so"},{"pos":"[aux]","gloss":"and; then","info":"indicates continuing action; alternative form of \u301C\u3066 used for some verb types"},{"pos":"[prt]","gloss":"let me tell you; don't you know","info":"at sentence-end; indicates certainty, emphasis, etc."}],"conj":[{"prop":[{"pos":"cop","type":"Conjunctive (~te)"}],"reading":"\u3060","gloss":[{"pos":"[cop,cop-da]","gloss":"be; is","info":"plain copula"}],"readok":true}]},[]],["sho",{"reading":"\u3057\u3087","text":"\u3057\u3087","kana":"\u3057\u3087","score":0},[]]],-949]]]"#;
        let _root = serde_json::from_str::<Root>(ICHIRAN_FULL).unwrap();
    }
}
