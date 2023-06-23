//! An high-performance implementation of core ichiran logic which is in the
//! critical path.

use const_format::formatcp;
use fancy_regex::Regex;
use lazy_static::lazy_static;

const NUM_WORD_REGEX: &str = "[0-9０-９〇々ヶ〆一-龯ァ-ヺヽヾぁ-ゔゝゞー]";
const WORD_REGEX: &str = "[々ヶ〆一-龯ァ-ヺヽヾぁ-ゔゝゞー〇]";
const DIGIT_REGEX: &str = "[0-9０-９〇]";
const DECIMAL_POINT_REGEX: &str = "[.,]";

const BASIC_SPLIT_REGEX: &str = formatcp!(
    "((?:(?<!{0}|{1}){1}+|{2}){3}*{2}|{2})",
    DECIMAL_POINT_REGEX,
    DIGIT_REGEX,
    WORD_REGEX,
    NUM_WORD_REGEX,
);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Split {
    Text,
    Skip,
}

/// Split a string into alternating text and skip blocks using the same
/// algorithm as ichiran.
pub fn basic_split(text: &str) -> Vec<(Split, &str)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(BASIC_SPLIT_REGEX).unwrap();
    }
    let mut captures = RE.find_iter(text);
    let mut last: usize = 0;
    let mut matches = vec![];
    while let Some(Ok(capture)) = captures.next() {
        let index = capture.start();
        if last != index {
            matches.push((Split::Skip, &text[last..index]))
        }
        matches.push((Split::Text, capture.as_str()));
        last = index + capture.range().len();
    }
    if last < text.len() {
        matches.push((Split::Skip, &text[last..]));
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split() {
        assert_eq!(
            basic_split("6月20日は国連が決めた「世界難民の日」です。国連のUNHCRとユニクロの会社は、世界の難民が働いて、自分で生活ができるように助けたいと考えています。"),
            vec![
                (Split::Text, "6月20日は国連が決めた".into()),
                (Split::Skip, "「".into()),
                (Split::Text, "世界難民の日".into()),
                (Split::Skip, "」".into()),
                (Split::Text, "です".into()),
                (Split::Skip, "。".into()),
                (Split::Text, "国連の".into()),
                (Split::Skip, "UNHCR".into()),
                (Split::Text, "とユニクロの会社は".into()),
                (Split::Skip, "、".into()),
                (Split::Text, "世界の難民が働いて".into()),
                (Split::Skip, "、".into()),
                (Split::Text, "自分で生活ができるように助けたいと考えています".into()),
                (Split::Skip, "。".into())
            ])
    }
}
