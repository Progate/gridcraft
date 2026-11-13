//! Format Cells › Number: Excel's categories and the format code each one's options build, and
//! the category and options a code is shown with when the dialog opens on a formatted cell.
//!
//! The codes are the ones Excel writes from this tab, in the Japanese region (Japanese Excel's
//! yen, `△`/`▲` negatives, `年月日` dates and `[DBNum]` numerals) or elsewhere en-US's.

use serde::{Deserialize, Serialize};

/// A category of the Number tab, in Excel's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Category {
    #[default]
    General,
    Number,
    Currency,
    Accounting,
    Date,
    Time,
    Percentage,
    Fraction,
    Scientific,
    Text,
    Special,
    Custom,
}

impl Category {
    pub(crate) const ALL: [Category; 12] = [
        Category::General,
        Category::Number,
        Category::Currency,
        Category::Accounting,
        Category::Date,
        Category::Time,
        Category::Percentage,
        Category::Fraction,
        Category::Scientific,
        Category::Text,
        Category::Special,
        Category::Custom,
    ];

    /// The English label, a key of the i18n catalogs (context-prefixed where the word has other
    /// meanings: Japanese Excel calls this Special その他 and Custom ユーザー定義).
    pub(crate) fn label(self) -> &'static str {
        match self {
            Category::General => "General",
            Category::Number => "Number",
            Category::Currency => "Currency",
            Category::Accounting => "Accounting",
            Category::Date => "Date",
            Category::Time => "Time",
            Category::Percentage => "Percentage",
            Category::Fraction => "Fraction",
            Category::Scientific => "Scientific",
            Category::Text => "Text",
            Category::Special => "Number Category|Special",
            Category::Custom => "Number Category|Custom",
        }
    }

    /// Whether the category has 小数点以下の桁数 (Decimal places).
    pub(crate) fn has_decimals(self) -> bool {
        matches!(self, Category::Number | Category::Currency | Category::Accounting | Category::Percentage | Category::Scientific)
    }
}

/// The options of the Number tab's categories; each category reads the ones it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Options {
    pub(crate) category: Category,
    /// Decimal places, 0–30.
    pub(crate) decimals: u8,
    /// Use 1000 Separator (,) (Number).
    pub(crate) separator: bool,
    /// Row of the Negative numbers list (Number, Currency).
    pub(crate) negative: usize,
    /// Row of the Symbol list (Currency, Accounting).
    pub(crate) symbol: usize,
    /// Row of the Type list (Date, Time, Fraction, Special).
    pub(crate) kind: usize,
    /// Calendar type 和暦 (Japanese era) instead of Gregorian (Date, Japanese region).
    pub(crate) era: bool,
}

impl Options {
    /// The options a category starts with when picked, as Excel's: two decimals except the yen's
    /// none, Japanese Excel's ¥ (else $), the first row of each list.
    pub(crate) fn new(category: Category, ja: bool) -> Options {
        let decimals = match category {
            Category::Currency | Category::Accounting if ja => 0,
            _ => 2,
        };
        Options { category, decimals, separator: false, negative: 0, symbol: if ja { 1 } else { 2 }, kind: 0, era: false }
    }
}

/// The Symbol list: label and the text the code starts each number with (empty for None).
pub(crate) const SYMBOLS: [(&str, &str); 6] = [
    ("Symbol|None", ""),
    ("¥", "\"¥\""),
    ("$", "\"$\""),
    ("€ Euro (€ 123)", "[$€-2]\\ "),
    ("¥ Japanese", "[$¥-411]"),
    ("$ English (United States)", "[$$-409]"),
];

/// Japanese Excel's Number negatives: -1234, -1234 in red, △ 1234, ▲ 1234.
const NUMBER_NEGATIVES_JA: usize = 4;
/// Excel's (en-US) Number negatives: -1234.10, 1234.10 in red, (1234.10), (1234.10) in red.
const NUMBER_NEGATIVES_EN: usize = 4;
/// Currency negatives: Japanese Excel's ¥-1,234, ¥-1,234 in red, -¥1,234, -¥1,234 in red; en-US's
/// -$1,234.10, $1,234.10 in red, ($1,234.10), ($1,234.10) in red.
const CURRENCY_NEGATIVES: usize = 4;

pub(crate) fn negative_count(category: Category, ja: bool) -> usize {
    match category {
        Category::Number if ja => NUMBER_NEGATIVES_JA,
        Category::Number => NUMBER_NEGATIVES_EN,
        Category::Currency => CURRENCY_NEGATIVES,
        _ => 0,
    }
}

/// `0`, `#,##0` or `#,##0.00`: the digits with `decimals` places.
fn digits(separator: bool, decimals: u8) -> String {
    let mut n = String::from(if separator { "#,##0" } else { "0" });
    if decimals > 0 {
        n.push('.');
        n.push_str(&"0".repeat(decimals.min(30) as usize));
    }
    n
}

/// The Date type list (code, star): a star marks the region's system dates, which follow the
/// operating system's date settings. Japanese Excel's Gregorian list, its 和暦 list, or en-US's.
pub(crate) fn date_list(ja: bool, era: bool) -> &'static [(&'static str, bool)] {
    if ja && era {
        &DATES_JA_ERA
    } else if ja {
        &DATES_JA
    } else {
        &DATES_EN
    }
}

const DATES_JA: [(&str, bool); 19] = [
    ("m/d/yyyy", true),
    ("[$-F800]dddd, mmmm dd, yyyy", true),
    ("yyyy-mm-dd;@", false),
    ("yyyy\"年\"m\"月\"d\"日\";@", false),
    ("yyyy\"年\"m\"月\";@", false),
    ("m\"月\"d\"日\";@", false),
    ("yyyy/m/d;@", false),
    ("yyyy/m/d\\ h:mm\\ AM/PM;@", false),
    ("yyyy/m/d\\ h:mm;@", false),
    ("m/d;@", false),
    ("m/d/yy;@", false),
    ("mm/dd/yy;@", false),
    ("[$-409]d\\-mmm;@", false),
    ("[$-409]d\\-mmm\\-yy;@", false),
    ("[$-409]dd\\-mmm\\-yy;@", false),
    ("[$-409]mmm\\-yy;@", false),
    ("[$-409]mmmm\\-yy;@", false),
    ("[$-409]mmmmm;@", false),
    ("[$-409]mmmmm\\-yy;@", false),
];

const DATES_JA_ERA: [(&str, bool); 5] = [
    ("[$-411]ge.m.d;@", false),
    ("[$-411]ggge\"年\"m\"月\"d\"日\";@", false),
    ("[$-411]ggge\"年\"m\"月\";@", false),
    ("[$-411]ggge\"年\"m\"月\"d\"日\"\\ (aaa);@", false),
    ("[$-411]gge\"年\"m\"月\"d\"日\";@", false),
];

const DATES_EN: [(&str, bool); 18] = [
    ("m/d/yyyy", true),
    ("[$-F800]dddd, mmmm dd, yyyy", true),
    ("m/d;@", false),
    ("m/d/yy;@", false),
    ("mm/dd/yy;@", false),
    ("[$-409]d-mmm;@", false),
    ("[$-409]d-mmm-yy;@", false),
    ("[$-409]dd-mmm-yy;@", false),
    ("[$-409]mmm-yy;@", false),
    ("[$-409]mmmm-yy;@", false),
    ("[$-409]mmmm d, yyyy;@", false),
    ("[$-409]m/d/yy h:mm AM/PM;@", false),
    ("m/d/yy h:mm;@", false),
    ("[$-409]mmmmm;@", false),
    ("[$-409]mmmmm-yy;@", false),
    ("m/d/yyyy;@", false),
    ("[$-409]d-mmm-yyyy;@", false),
    ("yyyy-mm-dd;@", false),
];

/// The Time type list (code, star), Japanese Excel's or en-US's.
pub(crate) fn time_list(ja: bool) -> &'static [(&'static str, bool)] {
    if ja { &TIMES_JA } else { &TIMES_EN }
}

const TIMES_JA: [(&str, bool); 12] = [
    ("[$-F400]h:mm:ss AM/PM", true),
    ("h:mm;@", false),
    ("h:mm:ss;@", false),
    ("[$-409]h:mm\\ AM/PM;@", false),
    ("[$-409]h:mm:ss\\ AM/PM;@", false),
    ("h\"時\"mm\"分\";@", false),
    ("h\"時\"mm\"分\"ss\"秒\";@", false),
    ("[$-411]AM/PMh\"時\"mm\"分\";@", false),
    ("[$-411]AM/PMh\"時\"mm\"分\"ss\"秒\";@", false),
    ("yyyy/m/d\\ h:mm;@", false),
    ("mm:ss.0;@", false),
    ("[h]:mm:ss;@", false),
];

const TIMES_EN: [(&str, bool); 9] = [
    ("[$-F400]h:mm:ss AM/PM", true),
    ("h:mm;@", false),
    ("[$-409]h:mm AM/PM;@", false),
    ("h:mm:ss;@", false),
    ("[$-409]h:mm:ss AM/PM;@", false),
    ("mm:ss.0;@", false),
    ("[h]:mm:ss;@", false),
    ("[$-409]m/d/yy h:mm AM/PM;@", false),
    ("m/d/yy h:mm;@", false),
];

/// The Fraction type list: label (an English catalog key) and code.
pub(crate) const FRACTIONS: [(&str, &str); 9] = [
    ("Up to one digit (1/4)", "# ?/?"),
    ("Up to two digits (21/25)", "# ??/??"),
    ("Up to three digits (312/943)", "# ???/???"),
    ("As halves (1/2)", "# ?/2"),
    ("As quarters (2/4)", "# ?/4"),
    ("As eighths (4/8)", "# ?/8"),
    ("As sixteenths (8/16)", "# ??/16"),
    ("As tenths (3/10)", "# ?/10"),
    ("As hundredths (30/100)", "# ??/100"),
];

/// The Special type list (label, code): Japanese Excel's postal code, Tokyo phone number and
/// kanji/full-width numerals, or en-US's.
pub(crate) fn special_list(ja: bool) -> &'static [(&'static str, &'static str)] {
    if ja { &SPECIAL_JA } else { &SPECIAL_EN }
}

const SPECIAL_JA: [(&str, &str); 8] = [
    ("Postal Code", "[<=999]000;[<=9999]000\\-00;000\\-0000"),
    ("Phone Number (Tokyo)", "[<=99999999]####\\-####;(00)\\ ####\\-####"),
    ("Kanji Numerals (十二万三千四百)", "[DBNum1][$-411]General"),
    ("Kanji Numerals (一二三四五)", "[DBNum1][$-411]0"),
    ("Formal Kanji (壱拾弐萬参阡四百)", "[DBNum2][$-411]General"),
    ("Formal Kanji (壱弐参四五)", "[DBNum2][$-411]0"),
    ("Full-width (１２３４５)", "[DBNum3][$-411]0"),
    ("Full-width with Separator (１２,３４５)", "[DBNum3][$-411]#,##0"),
];

const SPECIAL_EN: [(&str, &str); 4] = [
    ("Zip Code", "00000"),
    ("Zip Code + 4", "00000-0000"),
    ("Phone Number", "[<=9999999]###-####;(###) ###-####"),
    ("Social Security Number", "000-00-0000"),
];

/// The Type list of the Custom category: the codes Excel offers in the region, before the
/// workbook's own.
pub(crate) fn custom_list(ja: bool) -> &'static [&'static str] {
    if ja { &CUSTOM_JA } else { &CUSTOM_EN }
}

const CUSTOM_JA: [&str; 46] = [
    "General",
    "0",
    "0.00",
    "#,##0",
    "#,##0.00",
    "#,##0;-#,##0",
    "#,##0;[Red]-#,##0",
    "#,##0.00;-#,##0.00",
    "#,##0.00;[Red]-#,##0.00",
    "\"¥\"#,##0;\"¥\"\\-#,##0",
    "\"¥\"#,##0;[Red]\"¥\"\\-#,##0",
    "\"¥\"#,##0.00;\"¥\"\\-#,##0.00",
    "\"¥\"#,##0.00;[Red]\"¥\"\\-#,##0.00",
    "0%",
    "0.00%",
    "0.00E+00",
    "##0.0E+0",
    "# ?/?",
    "# ??/??",
    "_ * #,##0_ ;_ * \\-#,##0_ ;_ * \"-\"_ ;_ @_ ",
    "_ \"¥\"* #,##0_ ;_ \"¥\"* \\-#,##0_ ;_ \"¥\"* \"-\"_ ;_ @_ ",
    "_ * #,##0.00_ ;_ * \\-#,##0.00_ ;_ * \"-\"??_ ;_ @_ ",
    "_ \"¥\"* #,##0.00_ ;_ \"¥\"* \\-#,##0.00_ ;_ \"¥\"* \"-\"??_ ;_ @_ ",
    "yyyy/m/d",
    "[$-411]ge.m.d",
    "[$-411]ggge\"年\"m\"月\"d\"日\"",
    "yyyy\"年\"m\"月\"d\"日\"",
    "yyyy\"年\"m\"月\"",
    "m\"月\"d\"日\"",
    "m/d/yy",
    "d-mmm-yy",
    "d-mmm",
    "mmm-yy",
    "h:mm AM/PM",
    "h:mm:ss AM/PM",
    "h:mm",
    "h:mm:ss",
    "h\"時\"mm\"分\"",
    "h\"時\"mm\"分\"ss\"秒\"",
    "yyyy/m/d h:mm",
    "mm:ss",
    "[h]:mm:ss",
    "mm:ss.0",
    "@",
    "[DBNum1][$-411]General",
    "[DBNum3][$-411]0",
];

const CUSTOM_EN: [&str; 36] = [
    "General",
    "0",
    "0.00",
    "#,##0",
    "#,##0.00",
    "#,##0_);(#,##0)",
    "#,##0_);[Red](#,##0)",
    "#,##0.00_);(#,##0.00)",
    "#,##0.00_);[Red](#,##0.00)",
    "\"$\"#,##0_);\\(\"$\"#,##0\\)",
    "\"$\"#,##0_);[Red]\\(\"$\"#,##0\\)",
    "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)",
    "\"$\"#,##0.00_);[Red]\\(\"$\"#,##0.00\\)",
    "0%",
    "0.00%",
    "0.00E+00",
    "##0.0E+0",
    "# ?/?",
    "# ??/??",
    "m/d/yyyy",
    "d-mmm-yy",
    "d-mmm",
    "mmm-yy",
    "h:mm AM/PM",
    "h:mm:ss AM/PM",
    "h:mm",
    "h:mm:ss",
    "m/d/yyyy h:mm",
    "mm:ss",
    "mm:ss.0",
    "@",
    "[h]:mm:ss",
    "_(\"$\"* #,##0_);_(\"$\"* \\(#,##0\\);_(\"$\"* \"-\"_);_(@_)",
    "_(* #,##0_);_(* \\(#,##0\\);_(* \"-\"_);_(@_)",
    "_(\"$\"* #,##0.00_);_(\"$\"* \\(#,##0.00\\);_(\"$\"* \"-\"??_);_(@_)",
    "_(* #,##0.00_);_(* \\(#,##0.00\\);_(* \"-\"??_);_(@_)",
];

/// The format code the options build, as Excel writes it.
pub(crate) fn build(o: &Options, ja: bool) -> String {
    let d = o.decimals.min(30);
    match o.category {
        Category::General | Category::Custom => "General".into(),
        Category::Number => {
            let n = digits(o.separator, d);
            match (ja, o.negative) {
                (true, 1) => format!("{n}_ ;[Red]\\-{n}\\ "),
                (true, 2) => format!("{n};\"△ \"{n}"),
                (true, 3) => format!("{n};\"▲ \"{n}"),
                (false, 1) => format!("{n};[Red]{n}"),
                (false, 2) => format!("{n}_);\\({n}\\)"),
                (false, 3) => format!("{n}_);[Red]\\({n}\\)"),
                _ => n,
            }
        }
        Category::Currency => {
            let n = digits(true, d);
            let s = SYMBOLS.get(o.symbol).map_or("", |(_, s)| *s);
            match (ja, o.negative) {
                (true, 0) => format!("{s}{n};{s}\\-{n}"),
                (true, 1) => format!("{s}{n};[Red]{s}\\-{n}"),
                (true, 3) => format!("{s}{n};[Red]\\-{s}{n}"),
                (false, 1) => format!("{s}{n};[Red]{s}{n}"),
                (false, 2) => format!("{s}{n}_);\\({s}{n}\\)"),
                (false, 3) => format!("{s}{n}_);[Red]\\({s}{n}\\)"),
                _ => format!("{s}{n}"),
            }
        }
        Category::Accounting => {
            let n = digits(true, d);
            let s = SYMBOLS.get(o.symbol).map_or("", |(_, s)| *s);
            let q = "?".repeat(d as usize);
            if ja {
                format!("_ {s}* {n}_ ;_ {s}* \\-{n}_ ;_ {s}* \"-\"{q}_ ;_ @_ ")
            } else {
                format!("_({s}* {n}_);_({s}* \\({n}\\);_({s}* \"-\"{q}_);_(@_)")
            }
        }
        Category::Date => date_list(ja, o.era).get(o.kind).map_or("m/d/yyyy", |(c, _)| *c).into(),
        Category::Time => time_list(ja).get(o.kind).map_or("h:mm;@", |(c, _)| *c).into(),
        Category::Percentage => format!("{}%", digits(false, d)),
        Category::Fraction => FRACTIONS.get(o.kind).map_or("# ?/?", |(_, c)| *c).into(),
        Category::Scientific => format!("{}E+00", digits(false, d)),
        Category::Text => "@".into(),
        Category::Special => special_list(ja).get(o.kind).map_or("General", |(_, c)| *c).into(),
    }
}

/// The category and options Excel selects when the dialog opens on a cell formatted with `code`:
/// the first whose options build that code, else Custom (the code is then in its Type box).
/// Built-in codes Excel lists without `;@` (`yyyy"年"m"月"d"日"`, built-in 31) select their row too.
pub(crate) fn detect(code: &str, ja: bool) -> Options {
    let mut o = Options::new(Category::General, ja);
    if code == "General" {
        return o;
    }
    if code == "@" {
        return Options { category: Category::Text, ..o };
    }
    let found = |o: &Options| build(o, ja) == code;
    for category in [Category::Number, Category::Currency, Category::Accounting, Category::Percentage, Category::Scientific] {
        o.category = category;
        let symbols: Vec<usize> =
            if matches!(category, Category::Currency | Category::Accounting) { (0..SYMBOLS.len()).collect() } else { vec![o.symbol] };
        let negatives = negative_count(category, ja).max(1);
        let separators: &[bool] = if category == Category::Number { &[false, true] } else { &[false] };
        for decimals in 0..=30 {
            for &symbol in &symbols {
                for negative in 0..negatives {
                    for &separator in separators {
                        let candidate = Options { category, decimals, symbol, negative, separator, ..o };
                        if found(&candidate) {
                            return candidate;
                        }
                    }
                }
            }
        }
    }
    let bare = |c: &str| c.strip_suffix(";@").unwrap_or(c).to_string();
    for era in [false, true] {
        if era && !ja {
            continue;
        }
        if let Some(kind) = date_list(ja, era).iter().position(|(c, _)| *c == code || bare(c) == code || bare(c).replace('\\', "") == code) {
            return Options { category: Category::Date, kind, era, ..Options::new(Category::Date, ja) };
        }
    }
    if let Some(kind) = time_list(ja).iter().position(|(c, _)| *c == code || bare(c) == code || bare(c).replace('\\', "") == code) {
        return Options { category: Category::Time, kind, ..Options::new(Category::Time, ja) };
    }
    if let Some(kind) = FRACTIONS.iter().position(|(_, c)| *c == code) {
        return Options { category: Category::Fraction, kind, ..Options::new(Category::Fraction, ja) };
    }
    if let Some(kind) = special_list(ja).iter().position(|(_, c)| *c == code) {
        return Options { category: Category::Special, kind, ..Options::new(Category::Special, ja) };
    }
    Options { category: Category::Custom, ..Options::new(Category::Custom, ja) }
}

/// A code as Japanese Excel shows it in the Type box: General is `G/標準` and colours are in
/// Japanese (`[赤]`). Elsewhere the code is shown as stored.
pub(crate) fn shown_code(code: &str, ja: bool) -> String {
    if !ja {
        return code.to_string();
    }
    let code = code.replace("General", "G/標準");
    let mut out = String::with_capacity(code.len());
    let mut quoted = false;
    let mut rest = code.as_str();
    while let Some(c) = rest.chars().next() {
        if c == '"' {
            quoted = !quoted;
        }
        if !quoted && c == '[' {
            let names = [("[Red]", "[赤]"), ("[Black]", "[黒]"), ("[Blue]", "[青]"), ("[Green]", "[緑]"), ("[White]", "[白]"), ("[Yellow]", "[黄]")];
            if let Some((en, jp)) = names.iter().find(|(en, _)| rest.starts_with(en)) {
                out.push_str(jp);
                rest = &rest[en.len()..];
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// A code typed in the Type box as stored: `G/標準` is General (colour names are made English by
/// the command).
pub(crate) fn stored_code(typed: &str) -> String {
    typed.replace("G/標準", "General")
}

/// The custom codes the workbook's cells, rows and columns use that the region's Type list
/// doesn't have, in sheet order: they follow the built-in codes in Excel's list and can be deleted.
pub(crate) fn workbook_codes(wb: &gridcraft_engine::model::Workbook, ja: bool) -> Vec<String> {
    let builtin = custom_list(ja);
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for sh in &wb.sheets {
        let ids = sh.cells.iter().map(|(_, c)| c.style).chain(sh.rows.values().chain(sh.cols.values()).filter_map(|l| l.style));
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let code = wb.styles.get(id).num_fmt.as_str();
            if code != "General" && !builtin.contains(&code) && !out.iter().any(|c| c == code) {
                out.push(code.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(category: Category) -> Options {
        Options::new(category, true)
    }

    #[test]
    fn number_category_builds_japanese_excels_codes() {
        let o = |decimals, separator, negative| Options { decimals, separator, negative, ..opts(Category::Number) };
        assert_eq!(build(&o(2, false, 0), true), "0.00");
        assert_eq!(build(&o(0, true, 0), true), "#,##0");
        assert_eq!(build(&o(0, false, 1), true), "0_ ;[Red]\\-0\\ ");
        assert_eq!(build(&o(2, true, 1), true), "#,##0.00_ ;[Red]\\-#,##0.00\\ ");
        assert_eq!(build(&o(0, true, 2), true), "#,##0;\"△ \"#,##0");
        assert_eq!(build(&o(1, false, 3), true), "0.0;\"▲ \"0.0");
        assert_eq!(build(&o(2, false, 1), false), "0.00;[Red]0.00");
        assert_eq!(build(&o(0, true, 2), false), "#,##0_);\\(#,##0\\)");
        assert_eq!(build(&o(0, true, 3), false), "#,##0_);[Red]\\(#,##0\\)");
    }

    #[test]
    fn currency_and_accounting_default_to_yen_in_japanese() {
        let cur = opts(Category::Currency);
        assert_eq!((cur.decimals, SYMBOLS[cur.symbol].0), (0, "¥"));
        // Japanese Excel's built-in 5–8 and 42/44 (and Accounting's ¥ format, as the ribbon applies it).
        assert_eq!(build(&cur, true), gridcraft_engine::cmd::format::CURRENCY_JA);
        assert_eq!(build(&Options { negative: 1, ..cur }, true), "\"¥\"#,##0;[Red]\"¥\"\\-#,##0");
        assert_eq!(build(&Options { negative: 1, decimals: 2, ..cur }, true), "\"¥\"#,##0.00;[Red]\"¥\"\\-#,##0.00");
        assert_eq!(build(&Options { negative: 2, ..cur }, true), "\"¥\"#,##0");
        assert_eq!(build(&Options { symbol: 0, ..cur }, true), "#,##0;\\-#,##0");
        let acc = opts(Category::Accounting);
        assert_eq!(build(&acc, true), gridcraft_engine::cmd::format::ACCOUNTING_JA);
        assert_eq!(build(&Options { symbol: 0, ..acc }, true), gridcraft_engine::cmd::format::COMMA_JA);
        assert_eq!(build(&Options { decimals: 2, ..acc }, true), "_ \"¥\"* #,##0.00_ ;_ \"¥\"* \\-#,##0.00_ ;_ \"¥\"* \"-\"??_ ;_ @_ ");
        let en = Options::new(Category::Accounting, false);
        assert_eq!(build(&en, false), "_(\"$\"* #,##0.00_);_(\"$\"* \\(#,##0.00\\);_(\"$\"* \"-\"??_);_(@_)");
        assert_eq!(build(&Options::new(Category::Currency, false), false), "\"$\"#,##0.00");
    }

    #[test]
    fn other_categories_build_excels_codes() {
        assert_eq!(build(&Options { decimals: 0, ..opts(Category::Percentage) }, true), "0%");
        assert_eq!(build(&opts(Category::Percentage), true), "0.00%");
        assert_eq!(build(&Options { decimals: 1, ..opts(Category::Scientific) }, true), "0.0E+00");
        assert_eq!(build(&Options { kind: 3, ..opts(Category::Date) }, true), "yyyy\"年\"m\"月\"d\"日\";@");
        assert_eq!(build(&Options { kind: 1, era: true, ..opts(Category::Date) }, true), "[$-411]ggge\"年\"m\"月\"d\"日\";@");
        assert_eq!(build(&Options { kind: 5, ..opts(Category::Time) }, true), "h\"時\"mm\"分\";@");
        assert_eq!(build(&Options { kind: 6, ..opts(Category::Fraction) }, true), "# ??/16");
        assert_eq!(build(&Options { kind: 0, ..opts(Category::Special) }, true), "[<=999]000;[<=9999]000\\-00;000\\-0000");
        assert_eq!(build(&Options { kind: 2, ..opts(Category::Special) }, true), "[DBNum1][$-411]General");
        assert_eq!(build(&opts(Category::Text), true), "@");
    }

    #[test]
    fn opening_on_a_formatted_cell_selects_its_category_and_options() {
        for ja in [true, false] {
            for category in Category::ALL {
                let lists = match category {
                    Category::Date => date_list(ja, false).len(),
                    Category::Time => time_list(ja).len(),
                    Category::Fraction => FRACTIONS.len(),
                    Category::Special => special_list(ja).len(),
                    _ => 1,
                };
                for kind in 0..lists {
                    for (decimals, negative, symbol, separator) in [(0, 0, 1, false), (2, 1, 2, true), (3, 3, 0, true), (1, 2, 1, false)] {
                        let mut o = Options { kind, decimals, symbol, separator, ..Options::new(category, ja) };
                        o.negative = negative.min(negative_count(category, ja).saturating_sub(1));
                        let code = build(&o, ja);
                        let back = detect(&code, ja);
                        assert_eq!(build(&back, ja), code, "{category:?} {code}");
                        // A code two categories build (a date and time, a currency with no
                        // symbol) opens on the first of them, as in Excel.
                        let at = |c: Category| Category::ALL.iter().position(|x| *x == c);
                        if category != Category::Custom && at(back.category) != at(category) {
                            assert!(at(back.category) < at(category), "{code}");
                        } else if category != Category::Custom {
                            assert_eq!(back.category, category, "{code}");
                        }
                    }
                }
            }
        }
        // The yen built-ins and Japanese Excel's built-in dates without `;@`.
        assert_eq!(detect("\"¥\"#,##0;[Red]\"¥\"\\-#,##0", true), Options { negative: 1, ..opts(Category::Currency) });
        assert_eq!(detect("yyyy\"年\"m\"月\"d\"日\"", true), Options { kind: 3, ..opts(Category::Date) });
        assert_eq!(detect("[$-411]ge.m.d", true), Options { kind: 0, era: true, ..opts(Category::Date) });
        assert_eq!(detect("m/d/yyyy", true).kind, 0);
        assert_eq!(detect("0.000", true), Options { decimals: 3, ..opts(Category::Number) });
        assert_eq!(detect("#,##0;[Red]-#,##0", true).category, Category::Custom);
    }

    #[test]
    fn japanese_type_box_shows_general_and_colours_in_japanese() {
        assert_eq!(shown_code("#,##0;[Red]-#,##0", true), "#,##0;[赤]-#,##0");
        assert_eq!(shown_code("\"[Red]\"General", true), "\"[Red]\"G/標準");
        assert_eq!(shown_code("General", false), "General");
        assert_eq!(stored_code("G/標準"), "General");
        assert_eq!(gridcraft_engine::numfmt::canonical_code(&stored_code("#,##0;[赤]-#,##0")), "#,##0;[Red]-#,##0");
    }
}
