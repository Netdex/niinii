use serde::{Deserialize, Serialize};
use strum_macros::Display;

/// http://www.edrdg.org/wiki/index.php/KANJIDIC_Project
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Kanji {
    /// The kanji as a text literal
    text: String,
    /// Radical code (Classical)
    rc: u32,
    /// Radical code (Nelson)
    rn: u32,
    /// Number of strokes
    strokes: u32,
    /// Total samples in sample text
    total: u32,
    /// Number of irregular usages in sample text
    irr: u32,
    /// Percentage of irregular usages in sample text
    irr_perc: String,
    /// Readings for this kanji
    readings: Vec<Reading>,
    /// Meanings of this kanji
    meanings: Vec<String>,
    /// Frequency out of 2501 most-used characters
    freq: Option<u32>,
    /// "Grade" of this kanji
    grade: Option<u32>,
}

impl Kanji {
    pub fn text(&self) -> &str {
        self.text.as_str()
    }
    pub fn radical_code(&self) -> u32 {
        self.rc
    }
    pub fn stroke_count(&self) -> u32 {
        self.strokes
    }
    pub fn total_usage_count(&self) -> u32 {
        self.total
    }
    pub fn irregular_usage_count(&self) -> u32 {
        self.irr
    }
    pub fn irregular_percentage(&self) -> &str {
        &self.irr_perc
    }
    pub fn readings(&self) -> &[Reading] {
        &self.readings
    }
    pub fn meanings(&self) -> &[String] {
        &self.meanings
    }
    pub fn freq(&self) -> Option<u32> {
        self.freq
    }
    pub fn grade(&self) -> Option<u32> {
        self.grade
    }
    pub fn grade_desc(&self) -> String {
        match self.grade {
            Some(x @ 1..=6) => format!("Grade {} (elementary) kyōiku, jōyō kanji", x),
            Some(x @ 8) => format!("Grade {} (secondary) jōyō kanji", x),
            Some(x @ 9) => format!("Grade {} jinmeiyō, regular name kanji", x),
            Some(x @ 10) => format!("Grade {} jinmeiyō, jōyō variant kanji", x),
            _ => "hyōgai kanji".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Reading {
    /// Kana reading
    text: String,
    /// Romanized reading
    rtext: String,
    /// Reading type
    #[serde(rename = "type")]
    rtype: ReadingType,
    /// Okurigana (kana suffixes)
    okuri: Vec<String>,
    /// Number of usages in sample text
    sample: u32,
    /// Percentage of usages in sample text
    perc: String,
    /// Reading associated with prefix
    prefixp: Option<bool>,
    /// Reading associated with suffix
    suffixp: Option<bool>,
}

impl Reading {
    pub fn kana(&self) -> &str {
        self.text.as_str()
    }
    pub fn romaji(&self) -> &str {
        self.rtext.as_str()
    }
    pub fn rtype(&self) -> ReadingType {
        self.rtype
    }
    pub fn okuri(&self) -> &[String] {
        &self.okuri
    }
    pub fn usage_count(&self) -> u32 {
        self.sample
    }
    pub fn usage_percentage(&self) -> &str {
        &self.perc
    }
    pub fn prefix(&self) -> bool {
        self.prefixp.unwrap_or(false)
    }
    pub fn suffix(&self) -> bool {
        self.suffixp.unwrap_or(false)
    }
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, Display)]
#[serde(rename_all = "snake_case")]
pub enum ReadingType {
    /// Japanese on-yomi
    #[strum(serialize = "On")]
    JaOn,
    // Japanese kun-yomi
    #[strum(serialize = "Kun")]
    JaKun,
    // Japanese on/kun-yomi
    #[strum(serialize = "On/Kun")]
    JaOnkun,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn test_match() {
        let (ichiran, _pg) = fixture::ichiran();

        let a = ichiran.kanji(&['来']).unwrap();
        let a = a.get(&'来').unwrap();
        let b = &Kanji {
            text: "来".into(),
            rc: 75,
            rn: 4,
            strokes: 7,
            total: 76,
            irr: 0,
            irr_perc: "0.00%".into(),
            readings: vec![
                Reading {
                    text: "らい".into(),
                    rtext: "rai".into(),
                    rtype: ReadingType::JaOn,
                    okuri: vec![],
                    sample: 54,
                    perc: "71.05%".into(),
                    prefixp: None,
                    suffixp: None,
                },
                Reading {
                    text: "たい".into(),
                    rtext: "tai".into(),
                    rtype: ReadingType::JaOn,
                    okuri: vec![],
                    sample: 1,
                    perc: "1.32%".into(),
                    prefixp: None,
                    suffixp: None,
                },
                Reading {
                    text: "き".into(),
                    rtext: "ki".into(),
                    rtype: ReadingType::JaKun,
                    okuri: vec!["たす".into(), "たる".into()],
                    sample: 16,
                    perc: "21.05%".into(),
                    prefixp: None,
                    suffixp: None,
                },
                Reading {
                    text: "く".into(),
                    rtext: "ku".into(),
                    rtype: ReadingType::JaKun,
                    okuri: vec!["る".into()],
                    sample: 5,
                    perc: "6.58%".into(),
                    prefixp: None,
                    suffixp: None,
                },
                Reading {
                    text: "きた".into(),
                    rtext: "kita".into(),
                    rtype: ReadingType::JaKun,
                    okuri: vec!["す".into(), "る".into()],
                    sample: 0,
                    perc: "0.00%".into(),
                    prefixp: None,
                    suffixp: None,
                },
                Reading {
                    text: "こ".into(),
                    rtext: "ko".into(),
                    rtype: ReadingType::JaKun,
                    okuri: vec![],
                    sample: 0,
                    perc: "0.00%".into(),
                    prefixp: None,
                    suffixp: None,
                },
            ],
            meanings: vec![
                "come".into(),
                "due".into(),
                "next".into(),
                "cause".into(),
                "become".into(),
            ],
            freq: Some(102),
            grade: Some(2),
        };
        assert_eq!(a, b);
    }
}
