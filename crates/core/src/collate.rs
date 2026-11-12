//! Text order by language: which collation Sort, the SORT and SORTBY functions and the AutoFilter
//! sort buttons use for text.
//!
//! [`Collation::Default`] is [`compare_text`], GridCraft's original order. [`Collation::Japanese`]
//! is the order Japanese Excel sorts in (Windows' Japanese sort order), chosen for a workbook
//! whose region is Japanese ([`Collation::for_region`]):
//! - Kana sort in gojūon order (あいうえお…), and hiragana, katakana and half-width katakana
//!   of the same sound sort together: `あ`, `ア` and `ｱ` are equal at the first level, as are a
//!   small kana and its large one (`ッ`/`ツ`). Then voiced and semi-voiced sounds sort after the
//!   plain one (`か` < `が`, `は` < `ば` < `ぱ`); then, to break the tie, small before large,
//!   hiragana before katakana, full-width before half-width.
//! - The long-vowel mark `ー` after a kana sorts as that kana's vowel (`コーヒー` as `こおひい`,
//!   the Japanese dictionary convention), tie-broken after the written vowel.
//! - Full-width letters, digits and symbols sort with their ASCII forms (`Ａ` with `A`), and the
//!   ideographic space with the space.
//! - Kanji sort after kana in their Shift_JIS (JIS X 0208) code order: level-1 kanji by their
//!   most common on'yomi reading (亜, 唖, 娃, 阿…), level-2 kanji by radical and stroke count —
//!   not by reading, and not in Unicode order (which is by radical for every kanji). Kanji
//!   outside Shift_JIS follow, in code-point order.
//! - Excel's Sort Options ▸ "Use phonetic" (ふりがなを使う, on by default) sorts a cell that has
//!   phonetic (furigana) data by its reading; the caller passes that reading instead of the
//!   text (see [`phonetic_reading`]). A cell typed without the IME's reading — pasted or imported
//!   — has none and sorts by its characters, which is why such kanji fall into code order.
//!
//! Characters outside Japanese keep the default order, case-insensitively. Text equal under the
//! Japanese levels falls back to [`compare_text`], so the order is total and deterministic.

use std::cmp::Ordering;

use crate::value::{Value, compare, compare_text};
use crate::width::{HALF_DAKUTEN, HALF_HANDAKUTEN, join_voiced, split_voiced};

/// How text values compare when sorted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Collation {
    /// GridCraft's language-neutral order ([`compare_text`]).
    #[default]
    Default,
    /// Japanese Excel's order: see the module documentation.
    Japanese,
}

impl Collation {
    /// The collation of a region: Japanese for `ja-*`, otherwise the default.
    pub fn for_region(region: &crate::Region) -> Self {
        let lang = region.tag.split('-').next().unwrap_or("");
        if lang.eq_ignore_ascii_case("ja") { Collation::Japanese } else { Collation::Default }
    }

    /// Compares two texts in this collation.
    pub fn compare_text(self, a: &str, b: &str) -> Ordering {
        match self {
            Collation::Default => compare_text(a, b),
            Collation::Japanese => compare_japanese(a, b),
        }
    }

    /// [`crate::sort_compare`] with text compared in this collation: numbers, text, booleans,
    /// errors, then blanks last.
    pub fn sort_compare(self, a: &Value, b: &Value) -> Ordering {
        match (a, b) {
            (Value::Text(x), Value::Text(y)) => self.compare_text(x, y),
            _ => crate::value::sort_compare(a, b),
        }
    }

    /// [`compare`] with text compared in this collation.
    pub fn compare(self, a: &Value, b: &Value) -> Ordering {
        match (a, b) {
            (Value::Text(x), Value::Text(y)) => self.compare_text(x, y),
            _ => compare(a, b),
        }
    }
}

/// One character's weights in the Japanese order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Weight {
    primary: u32,
    /// 0 plain, 1 voiced (゛), 2 semi-voiced (゜), 3 a long-vowel mark read as a vowel.
    secondary: u8,
    /// Bits: 4 large (not small) kana, 2 katakana, 1 half-width.
    tertiary: u8,
}

/// Primary weights of kanji: after every other character, in Shift_JIS order, then the kanji
/// Shift_JIS lacks.
const KANJI_SJIS: u32 = 0x0020_0000;
const KANJI_OTHER: u32 = 0x0030_0000;

fn compare_japanese(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        return compare_text(a, b);
    }
    let (wa, wb) = (weights(a), weights(b));
    let primary = wa.iter().map(|w| w.primary).cmp(wb.iter().map(|w| w.primary));
    if primary != Ordering::Equal {
        return primary;
    }
    let secondary = wa.iter().map(|w| w.secondary).cmp(wb.iter().map(|w| w.secondary));
    if secondary != Ordering::Equal {
        return secondary;
    }
    let tertiary = wa.iter().map(|w| w.tertiary).cmp(wb.iter().map(|w| w.tertiary));
    if tertiary != Ordering::Equal {
        return tertiary;
    }
    compare_text(a, b)
}

/// Small hiragana and the large one each sorts with.
const SMALL: [(char, char); 12] = [
    ('ぁ', 'あ'),
    ('ぃ', 'い'),
    ('ぅ', 'う'),
    ('ぇ', 'え'),
    ('ぉ', 'お'),
    ('っ', 'つ'),
    ('ゃ', 'や'),
    ('ゅ', 'ゆ'),
    ('ょ', 'よ'),
    ('ゎ', 'わ'),
    ('ゕ', 'か'),
    ('ゖ', 'け'),
];

/// The vowel of each gojūon column, for a following long-vowel mark.
const VOWEL_ROWS: [(char, &str); 5] = [
    ('あ', "あかさたなはまやらわ"),
    ('い', "いきしちにひみりゐ"),
    ('う', "うくすつぬふむゆるゔ"),
    ('え', "えけせてねへめれゑ"),
    ('お', "おこそとのほもよろを"),
];

fn vowel_of(base: char) -> Option<char> {
    VOWEL_ROWS.iter().find(|(_, row)| row.contains(base)).map(|(v, _)| *v)
}

fn is_hiragana(c: char) -> bool {
    ('\u{3041}'..='\u{3096}').contains(&c)
}

fn is_katakana(c: char) -> bool {
    ('\u{30A1}'..='\u{30F6}').contains(&c)
}

fn is_kanji(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{3FFFF}')
}

/// The Shift_JIS (Windows-31J) code of a character, when it has a two-byte one.
fn sjis(c: char) -> Option<u16> {
    let mut buf = [0u8; 4];
    let src = c.encode_utf8(&mut buf);
    let mut out = [0u8; 8];
    let mut enc = encoding_rs::SHIFT_JIS.new_encoder();
    let (result, _, written) = enc.encode_from_utf8_without_replacement(src, &mut out, true);
    match (result, out.get(..written)) {
        (encoding_rs::EncoderResult::InputEmpty, Some(&[hi, lo])) => Some(u16::from_be_bytes([hi, lo])),
        _ => None,
    }
}

/// Folds width: full-width ASCII and the ideographic space to ASCII, half-width katakana (with a
/// following sound mark joined) to full-width. Each char comes with whether it was half-width kana.
fn fold_width(s: &str) -> Vec<(char, bool)> {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let u = u32::from(c);
        match u {
            0x3000 => out.push((' ', false)),
            0xFF01..=0xFF5E => out.push((char::from_u32(u - 0xFEE0).unwrap_or(c), false)),
            0xFF61..=0xFF9F => {
                let full = crate::width::to_fullwidth(c.encode_utf8(&mut [0u8; 4]));
                let full = full.chars().next().unwrap_or(c);
                if let Some(&next) = chars.peek()
                    && (next == HALF_DAKUTEN || next == HALF_HANDAKUTEN)
                    && let Some(joined) = join_voiced(full, next)
                {
                    chars.next();
                    out.push((joined, true));
                } else {
                    out.push((full, true));
                }
            }
            _ => out.push((c, false)),
        }
    }
    out
}

fn weights(s: &str) -> Vec<Weight> {
    let mut out: Vec<Weight> = Vec::with_capacity(s.len());
    // The kana base (hiragana, unvoiced, large) of the previous character, for `ー`.
    let mut prev_kana: Option<char> = None;
    for (c, half) in fold_width(s) {
        let half_bit = u8::from(half);
        let kata = is_katakana(c);
        let kana = if kata { char::from_u32(u32::from(c) - 0x60).unwrap_or(c) } else { c };
        if is_hiragana(kana) {
            // Voicing is found on the katakana form, where `split_voiced` works.
            let k = char::from_u32(u32::from(kana) + 0x60).unwrap_or(kana);
            let (base, secondary) = match split_voiced(k) {
                Some((b, mark)) => (char::from_u32(u32::from(b) - 0x60).unwrap_or(kana), if mark == '゛' { 1 } else { 2 }),
                None => (kana, 0),
            };
            let (base, large) = match SMALL.iter().find(|(small, _)| *small == base) {
                Some((_, large)) => (*large, 0),
                None => (base, 4),
            };
            prev_kana = Some(base);
            out.push(Weight { primary: u32::from(base), secondary, tertiary: large | (u8::from(kata) << 1) | half_bit });
            continue;
        }
        if c == 'ー'
            && let Some(v) = prev_kana.and_then(vowel_of)
        {
            out.push(Weight { primary: u32::from(v), secondary: 3, tertiary: 4 | half_bit });
            continue;
        }
        prev_kana = None;
        let primary = if is_kanji(c) {
            sjis(c).map_or(KANJI_OTHER + u32::from(c) - 0x3400, |code| KANJI_SJIS + u32::from(code))
        } else {
            c.to_lowercase().next().map_or(u32::from(c), u32::from)
        };
        out.push(Weight { primary, secondary: 0, tertiary: 4 | half_bit });
    }
    out
}

/// The reading Excel sorts a string by under "Use phonetic": the text with each phonetic run's
/// base characters replaced by the run's reading. `runs` are `(start, end, reading)` in UTF-16
/// code units of `text`, in order; runs that overlap or fall outside the text are skipped.
pub fn phonetic_reading<'a>(text: &str, runs: impl IntoIterator<Item = (u32, u32, &'a str)>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut runs = runs.into_iter().peekable();
    let mut pos: u32 = 0;
    let mut skip_to: u32 = 0;
    for c in text.chars() {
        let len = u32::try_from(c.len_utf16()).unwrap_or(1);
        while let Some(&(start, end, reading)) = runs.peek() {
            if start < pos || end <= start {
                runs.next();
                continue;
            }
            if start == pos {
                out.push_str(reading);
                skip_to = end;
                runs.next();
            }
            break;
        }
        if pos >= skip_to {
            out.push(c);
        }
        pos = pos.saturating_add(len);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(words: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = words.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| Collation::Japanese.compare_text(a, b));
        v
    }

    #[test]
    fn kana_sort_in_gojuon_order_whatever_the_script_and_width() {
        assert_eq!(sorted(&["ウメ", "いちご", "ｱﾝｽﾞ", "さくらんぼ", "かき"]), ["ｱﾝｽﾞ", "いちご", "ウメ", "かき", "さくらんぼ"]);
        // Same sound: hiragana, then katakana, then half-width; voiced after plain.
        assert_eq!(sorted(&["ｶ", "が", "カ", "か", "き"]), ["か", "カ", "ｶ", "が", "き"]);
        assert_eq!(sorted(&["ぱん", "はん", "ばん"]), ["はん", "ばん", "ぱん"]);
        // Small kana sort with the large one: きって reads きつて at the first level.
        assert_eq!(sorted(&["きつね", "きって", "きつつき"]), ["きつつき", "きって", "きつね"]);
        assert_eq!(Collation::Japanese.compare_text("ガ", "ｶﾞ"), Ordering::Less);
        assert_ne!(Collation::Japanese.compare_text("ガ", "ｶﾞ"), Ordering::Equal);
    }

    #[test]
    fn long_vowel_mark_reads_as_the_vowel() {
        // コーヒー = こおひい: after こうちゃ (こう < こお), before こおり's next letter.
        assert_eq!(sorted(&["こおり", "こうちゃ", "コーヒー", "こあら"]), ["こあら", "こうちゃ", "コーヒー", "こおり"]);
    }

    #[test]
    fn kanji_sort_after_kana_in_shift_jis_order() {
        assert_eq!(sjis('亜'), Some(0x889F));
        assert_eq!(sjis('東'), Some(0x938C));
        assert_eq!(sjis('大'), Some(0x91E5));
        assert_eq!(sjis('𠀋'), None);
        // 亜 0x889F < 愛 0x88A4 < 一 0x88EA in JIS (by reading: あ, あい, いち); in Unicode 一 is first.
        assert_eq!(sorted(&["一", "愛", "亜", "あい"]), ["あい", "亜", "愛", "一"]);
        // 大 0x91E5 < 東 0x938C < 名 0x96BC; Unicode has 名 U+540D < 大 U+5927 < 東 U+6771.
        assert_eq!(sorted(&["東京", "大阪", "名古屋"]), ["大阪", "東京", "名古屋"]);
        // Default order: by code point.
        let mut v = vec!["一", "愛", "亜"];
        v.sort_by(|a, b| Collation::Default.compare_text(a, b));
        assert_eq!(v, ["一", "亜", "愛"]);
    }

    #[test]
    fn full_width_letters_sort_with_ascii() {
        assert_eq!(sorted(&["b", "Ａ", "c", "a"]), ["a", "Ａ", "b", "c"]);
        assert_eq!(sorted(&["ｂ", "ab", "ａｃ"]), ["ab", "ａｃ", "ｂ"]);
        // Ascii-only text keeps the default order exactly.
        assert_eq!(Collation::Japanese.compare_text("Apple", "apple"), compare_text("Apple", "apple"));
    }

    #[test]
    fn region_chooses_the_collation() {
        let ja = crate::region::find("ja-JP").map(Collation::for_region);
        assert_eq!(ja, Some(Collation::Japanese));
        assert_eq!(Collation::for_region(crate::region::default_region()), Collation::Default);
    }

    #[test]
    fn values_sort_with_numbers_first_and_blanks_last() {
        let mut v = vec![Value::Empty, Value::Text("東".into()), Value::Number(2.0), Value::Text("あ".into())];
        v.sort_by(|a, b| Collation::Japanese.sort_compare(a, b));
        assert_eq!(v, vec![Value::Number(2.0), Value::Text("あ".into()), Value::Text("東".into()), Value::Empty]);
    }

    #[test]
    fn phonetic_reading_replaces_annotated_characters() {
        assert_eq!(phonetic_reading("山田太郎", [(0, 2, "ヤマダ"), (2, 4, "タロウ")]), "ヤマダタロウ");
        assert_eq!(phonetic_reading("新宿駅前", [(0, 2, "シンジュク")]), "シンジュク駅前");
        // Bad runs are skipped, never a panic.
        assert_eq!(phonetic_reading("ab", [(5, 9, "x"), (1, 0, "y")]), "ab");
        assert_eq!(phonetic_reading("a𠀋b", [(1, 3, "ツ")]), "aツb");
    }
}
