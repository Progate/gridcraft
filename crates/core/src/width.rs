//! Full-width (zenkaku) and half-width (hankaku) forms of Japanese text, as Excel's `ASC` and
//! `JIS` (`DBCS`) functions convert them, and as the Japanese text order folds them.
//!
//! The pairs are the Unicode Halfwidth and Fullwidth Forms block (U+FF00–U+FFEF) and its
//! decomposition mappings: full-width ASCII `！`–`～` and the ideographic space U+3000 against
//! ASCII, the half-width katakana U+FF61–U+FF9F against the katakana and Japanese punctuation, and
//! `￠￡￢￣￤￥` against `¢£¬¯¦¥`. A voiced or semi-voiced katakana (`ガ`, `パ`, `ヴ`) has no
//! half-width form of its own: it becomes the base letter followed by the half-width sound mark
//! (`ｶﾞ`, `ﾊﾟ`, `ｳﾞ`), and the full-width conversion joins them again. Hiragana and the katakana
//! without a half-width form (`ヮ`, `ヰ`, `ヱ`, `ヵ`, `ヶ`) are left alone, as Excel leaves them.

/// The full-width characters of U+FF61 (`｡`) to U+FF9F (`ﾟ`), in that order.
const HALF_KANA_FULL: [char; 63] = [
    '。', '「', '」', '、', '・', 'ヲ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ャ', 'ュ', 'ョ', 'ッ', 'ー', 'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク',
    'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ', 'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ', 'マ', 'ミ',
    'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ', 'ン', '゛', '゜',
];

/// Half-width voiced sound mark `ﾞ` (U+FF9E).
pub const HALF_DAKUTEN: char = '\u{FF9E}';
/// Half-width semi-voiced sound mark `ﾟ` (U+FF9F).
pub const HALF_HANDAKUTEN: char = '\u{FF9F}';

/// Symbols of the Fullwidth Forms block beyond ASCII, with their narrow forms.
const SYMBOLS: [(char, char); 6] = [('￠', '¢'), ('￡', '£'), ('￢', '¬'), ('￣', '¯'), ('￤', '¦'), ('￥', '¥')];

/// The full-width form of a half-width character, or `None` when it has none.
fn full_of(c: char) -> Option<char> {
    let u = u32::from(c);
    match u {
        0x20 => Some('\u{3000}'),
        0x21..=0x7E => char::from_u32(u + 0xFEE0),
        0xFF61..=0xFF9F => usize::try_from(u - 0xFF61).ok().and_then(|i| HALF_KANA_FULL.get(i).copied()),
        _ => SYMBOLS.iter().find(|(_, n)| *n == c).map(|(f, _)| *f),
    }
}

/// The half-width form of a full-width character that has a single-character one.
fn half_of(c: char) -> Option<char> {
    let u = u32::from(c);
    match u {
        0x3000 => Some(' '),
        0xFF01..=0xFF5E => char::from_u32(u - 0xFEE0),
        _ => {
            if let Some(i) = HALF_KANA_FULL.iter().position(|&f| f == c) {
                return u32::try_from(i).ok().and_then(|i| char::from_u32(0xFF61 + i));
            }
            SYMBOLS.iter().find(|(f, _)| *f == c).map(|(_, n)| *n)
        }
    }
}

/// Katakana with a voiced sound mark, each one code point after its base letter.
const VOICED: &str = "ガギグゲゴザジズゼゾダヂヅデドバビブベボ";
/// Katakana with a semi-voiced sound mark, each two code points after its base letter.
const SEMI_VOICED: &str = "パピプペポ";

/// Splits a voiced or semi-voiced katakana into its base letter and its (full-width) mark.
pub fn split_voiced(c: char) -> Option<(char, char)> {
    let u = u32::from(c);
    if c == 'ヴ' {
        return Some(('ウ', '゛'));
    }
    if VOICED.contains(c) {
        return char::from_u32(u - 1).map(|b| (b, '゛'));
    }
    if SEMI_VOICED.contains(c) {
        return char::from_u32(u - 2).map(|b| (b, '゜'));
    }
    None
}

/// Joins a katakana base letter and a sound mark (`゛`/`ﾞ` or `゜`/`ﾟ`) into one letter.
pub fn join_voiced(base: char, mark: char) -> Option<char> {
    let u = u32::from(base);
    match mark {
        '゛' | HALF_DAKUTEN if base == 'ウ' => Some('ヴ'),
        '゛' | HALF_DAKUTEN => char::from_u32(u + 1).filter(|j| VOICED.contains(*j)),
        '゜' | HALF_HANDAKUTEN => char::from_u32(u + 2).filter(|j| SEMI_VOICED.contains(*j)),
        _ => None,
    }
}

/// Excel's `ASC`: full-width letters, digits, symbols, spaces and katakana become half-width
/// (`ＡＢＣ１２３` → `ABC123`, `ガギ` → `ｶﾞｷﾞ`). Everything else is kept.
pub fn to_halfwidth(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if let Some((base, mark)) = split_voiced(c)
            && let (Some(b), Some(m)) = (half_of(base), half_of(mark))
        {
            out.push(b);
            out.push(m);
        } else {
            out.push(half_of(c).unwrap_or(c));
        }
    }
    out
}

/// Excel's `JIS` and `DBCS`: half-width characters become full-width (`ABC` → `ＡＢＣ`), and a
/// half-width katakana followed by `ﾞ` or `ﾟ` becomes the one voiced letter (`ｶﾞ` → `ガ`).
pub fn to_fullwidth(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let full = full_of(c).unwrap_or(c);
        if (0xFF66..=0xFF9D).contains(&u32::from(c))
            && let Some(&next) = chars.peek()
            && let Some(joined) = join_voiced(full, next)
        {
            chars.next();
            out.push(joined);
            continue;
        }
        out.push(full);
    }
    out
}

/// Whether `c` is a space `TRIM` removes: the ASCII space or, as in Japanese Excel, the
/// ideographic (full-width) space U+3000.
pub fn is_trim_space(c: char) -> bool {
    c == ' ' || c == '\u{3000}'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_kana_table_lines_up_with_the_block() {
        assert_eq!(half_of('ヲ'), Some('ｦ'));
        assert_eq!(half_of('ン'), Some('ﾝ'));
        assert_eq!(half_of('ー'), Some('ｰ'));
        assert_eq!(half_of('゜'), Some('ﾟ'));
        assert_eq!(full_of('｡'), Some('。'));
        assert_eq!(full_of('ﾜ'), Some('ワ'));
    }

    #[test]
    fn asc_converts_to_half_width() {
        assert_eq!(to_halfwidth("ＡＢＣ　１２３！"), "ABC 123!");
        assert_eq!(to_halfwidth("エクセル"), "ｴｸｾﾙ");
        assert_eq!(to_halfwidth("ガッコウ・パン・ヴァイオリン"), "ｶﾞｯｺｳ･ﾊﾟﾝ･ｳﾞｧｲｵﾘﾝ");
        assert_eq!(to_halfwidth("「データ」。"), "｢ﾃﾞｰﾀ｣｡");
        // Hiragana, kanji and katakana without a narrow form stay.
        assert_eq!(to_halfwidth("ひらがな漢字ヶヵ"), "ひらがな漢字ヶヵ");
        assert_eq!(to_halfwidth("￥１，０００"), "¥1,000");
        assert_eq!(to_halfwidth("abc"), "abc");
    }

    #[test]
    fn jis_converts_to_full_width_and_joins_sound_marks() {
        assert_eq!(to_fullwidth("ABC 123!"), "ＡＢＣ　１２３！");
        assert_eq!(to_fullwidth("ｶﾞｯｺｳ ﾊﾟﾝ ｳﾞｧｲｵﾘﾝ"), "ガッコウ　パン　ヴァイオリン");
        assert_eq!(to_fullwidth("ﾃﾞｰﾀ"), "データ");
        // A mark that can't join stays a mark of its own.
        assert_eq!(to_fullwidth("ｱﾞﾏﾟﾞ"), "ア゛マ゜゛");
        assert_eq!(to_fullwidth("漢字ひらがな"), "漢字ひらがな");
        assert_eq!(to_fullwidth(&to_halfwidth("ガギグゲゴパピプペポヴ")), "ガギグゲゴパピプペポヴ");
    }
}
