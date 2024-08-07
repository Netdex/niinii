pub fn is_hiragana(c: &char) -> bool {
    *c >= '\u{3041}' && *c <= '\u{3096}'
}

pub fn is_katakana(c: &char) -> bool {
    *c >= '\u{30a0}' && *c <= '\u{30ff}'
}

pub fn is_kanji(c: &char) -> bool {
    (*c >= '\u{3400}' && *c <= '\u{4db5}')
        || (*c >= '\u{4e00}' && *c <= '\u{9fcb}')
        || (*c >= '\u{f900}' && *c <= '\u{fa6a}')
}

pub fn is_radical(c: &char) -> bool {
    *c >= '\u{2e80}' && *c <= '\u{2fd5}'
}

pub fn is_half_katakana(c: &char) -> bool {
    *c >= '\u{ff5f}' && *c <= '\u{ff9f}'
}

pub fn is_jp_symbol(c: &char) -> bool {
    *c >= '\u{3000}' && *c <= '\u{303f}'
}

pub fn is_jp_misc(c: &char) -> bool {
    (*c >= '\u{31f0}' && *c <= '\u{31ff}')
        || (*c >= '\u{3220}' && *c <= '\u{3243}')
        || (*c >= '\u{3280}' && *c <= '\u{337f}')
}

pub fn is_full_alphanum(c: &char) -> bool {
    *c >= '\u{ff01}' && *c <= '\u{ff5e}'
}

pub fn is_japanese(c: &char) -> bool {
    is_hiragana(c)
        || is_katakana(c)
        || is_kanji(c)
        || is_radical(c)
        || is_half_katakana(c)
        || is_jp_symbol(c)
        || is_jp_misc(c)
        || is_full_alphanum(c)
}
