//! UI formula syntax boundary. Engine commands, control/API callers and XLSX keep English.
use crate::i18n::Language;
use gridcraft_engine::formula::{self, FormulaLocale, ParseError};
use gridcraft_engine::model::Workbook;

pub(crate) fn canonical(
    text: &str,
    locale: FormulaLocale,
    wb: &Workbook,
    sheet: usize,
    cell: gridcraft_engine::core::CellRef,
) -> Result<String, ParseError> {
    // The editor hid the `_xludf.` prefixes of this cell's formula; confirming it unchanged keeps
    // the user-defined calls instead of binding them to the built-ins of the same name.
    if let Some(stored) = wb.sheet(sheet).and_then(|sh| sh.cell(cell)).filter(|c| c.formula.is_some()).map(|c| c.input_text())
        && has_udf_prefix(&stored)
        && text == display(&stored, locale, wb, sheet)
    {
        return Ok(stored);
    }
    if locale == FormulaLocale::En || wb.sheet(sheet).is_some_and(|sh| wb.styles.get(sh.style_id(cell)).num_fmt.as_str() == "@") {
        return Ok(text.to_string());
    }
    // Same formula detection as the engine's `input_to_cell` (no trimming: " =1" is text).
    let body = text;
    if body.starts_with('=')
        || ((body.starts_with('+') || body.starts_with('-')) && body.chars().nth(1).is_some_and(|c| c.is_ascii_alphabetic() || c == '('))
    {
        let known = |name: &str| wb.name(name, sheet).is_some() || wb.table(name).is_some();
        // Match the engine's existing convenience for missing closing parentheses.
        let expr = formula::parse_closing_parens(body, |text| formula::parse_input(text, locale, known));
        return Ok(format!("={}", formula::print(&expr?)));
    }
    // Decimal-comma number entry follows the same preference, without rewriting text or dates.
    let normalized = body.trim().replace(locale.decimal_separator(), ".");
    let number = normalized.strip_suffix('%').unwrap_or(&normalized);
    if number.parse::<f64>().is_ok_and(f64::is_finite) {
        return Ok(normalized);
    }
    Ok(text.to_string())
}

pub(crate) fn display(text: &str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> String {
    // English (and every language without a formula table) shows the stored text, `_xludf.`
    // prefixes aside, and skips a parse per frame.
    if locale == FormulaLocale::En {
        return hide_udf_prefix(text, locale);
    }
    let Some(body) = text.strip_prefix('=') else { return text.to_string() };
    let shown = formula::parse(body)
        .and_then(|expr| formula::print_input(&expr, locale, |name| wb.name(name, sheet).is_some() || wb.table(name).is_some()))
        .map(|body| format!("={body}"))
        .unwrap_or_else(|_| text.to_string());
    hide_udf_prefix(&shown, locale)
}

const UDF_PREFIX: &str = "_xludf.";

fn has_udf_prefix(text: &str) -> bool {
    text.to_ascii_lowercase().contains(UDF_PREFIX)
}

/// Excel shows a user-defined call such as `_xludf.EOMONTH()` as `=EOMONTH()`. Only function
/// names lose the prefix: text literals and names keep it.
fn hide_udf_prefix(text: &str, locale: FormulaLocale) -> String {
    let Some(body) = text.strip_prefix('=').filter(|b| has_udf_prefix(b)) else { return text.to_string() };
    let Ok(tokens) = formula::lexer::tokenize_locale(body, locale) else { return text.to_string() };
    let mut out = String::from("=");
    let mut from = 0;
    for t in tokens {
        if matches!(t.tok, formula::lexer::Tok::Func(_))
            && body.get(t.start..t.start + UDF_PREFIX.len()).is_some_and(|p| p.eq_ignore_ascii_case(UDF_PREFIX))
        {
            out.push_str(body.get(from..t.start).unwrap_or(""));
            from = t.start + UDF_PREFIX.len();
        }
    }
    out.push_str(body.get(from..).unwrap_or(""));
    out
}

/// A function's argument hint. The name and separators follow the formula syntax; the argument
/// names follow the interface language, all of them or none, so a hint never mixes two languages.
pub(crate) fn signature(name: &str, locale: FormulaLocale, language: Language) -> Option<String> {
    let english = crate::formula_bar::function_signature(locale.canonical_name(name))?;
    let Some((head, args)) = split_signature(&english) else { return Some(english) };
    let separator = format!("{} ", locale.list_separator());
    let args = match args.iter().map(|arg| translate_argument(head, arg, language)).collect::<Option<Vec<_>>>() {
        Some(translated) => translated.join(&separator),
        None => args.join(&separator),
    };
    Some(format!("{}({args})", locale.function_name(head)))
}

/// A function's description in the interface language (English while untranslated).
pub(crate) fn description(name: &str, locale: FormulaLocale, language: Language) -> Option<String> {
    let english = crate::formula_bar::function_description(locale.canonical_name(name))?;
    Some(language.function_description_of(locale.canonical_name(name), &english).map_or(english, str::to_string))
}

/// An argument's values and their labels.
type Choices = &'static [(&'static str, &'static str)];

/// The values an argument takes, listed under the hint while it is typed as Excel lists them:
/// (function, argument, [(value, label)]). Labels are catalog keys in Excel's English wording.
const ARGUMENT_CHOICES: &[(&str, &str, Choices)] = &[
    ("AGGREGATE", "function_num", AGGREGATE_FUNCTIONS),
    ("AGGREGATE", "options", AGGREGATE_OPTIONS),
    ("HLOOKUP", "range_lookup", &[("TRUE", "TRUE - Approximate match"), ("FALSE", "FALSE - Exact match")]),
    ("MATCH", "match_type", &[("1", "1 - Less than"), ("0", "0 - Exact match"), ("-1", "-1 - Greater than")]),
    ("RANK", "order", &[("0", "0 - Descending"), ("1", "1 - Ascending")]),
    ("RANK.EQ", "order", &[("0", "0 - Descending"), ("1", "1 - Ascending")]),
    ("SORT", "sort_order", &[("1", "1 - Ascending"), ("-1", "-1 - Descending")]),
    ("SORT", "by_col", &[("FALSE", "FALSE - Sort by row"), ("TRUE", "TRUE - Sort by column")]),
    ("SUBTOTAL", "function_num", SUBTOTAL_FUNCTIONS),
    ("UNIQUE", "by_col", &[("FALSE", "FALSE - Return unique rows"), ("TRUE", "TRUE - Return unique columns")]),
    ("UNIQUE", "exactly_once", &[("FALSE", "FALSE - Return every distinct item"), ("TRUE", "TRUE - Return items that occur exactly once")]),
    ("VLOOKUP", "range_lookup", &[("TRUE", "TRUE - Approximate match"), ("FALSE", "FALSE - Exact match")]),
    ("XLOOKUP", "match_mode", XMATCH_MODES),
    ("XLOOKUP", "search_mode", XSEARCH_MODES),
    ("XMATCH", "match_mode", XMATCH_MODES),
    ("XMATCH", "search_mode", XSEARCH_MODES),
    ("WEEKDAY", "return_type", WEEKDAY_TYPES),
    ("WEEKNUM", "return_type", WEEKNUM_TYPES),
    ("WORKDAY.INTL", "weekend", WEEKENDS),
    ("NETWORKDAYS.INTL", "weekend", WEEKENDS),
    ("YEARFRAC", "basis", DAY_COUNT_BASES),
];

/// SUBTOTAL's function_num: 1-11 count hidden rows, 101-111 skip them.
const SUBTOTAL_FUNCTIONS: &[(&str, &str)] = &[
    ("1", "1 - AVERAGE"),
    ("2", "2 - COUNT"),
    ("3", "3 - COUNTA"),
    ("4", "4 - MAX"),
    ("5", "5 - MIN"),
    ("6", "6 - PRODUCT"),
    ("7", "7 - STDEV"),
    ("8", "8 - STDEVP"),
    ("9", "9 - SUM"),
    ("10", "10 - VAR"),
    ("11", "11 - VARP"),
    ("101", "101 - AVERAGE"),
    ("102", "102 - COUNT"),
    ("103", "103 - COUNTA"),
    ("104", "104 - MAX"),
    ("105", "105 - MIN"),
    ("106", "106 - PRODUCT"),
    ("107", "107 - STDEV"),
    ("108", "108 - STDEVP"),
    ("109", "109 - SUM"),
    ("110", "110 - VAR"),
    ("111", "111 - VARP"),
];

const AGGREGATE_FUNCTIONS: &[(&str, &str)] = &[
    ("1", "1 - AVERAGE"),
    ("2", "2 - COUNT"),
    ("3", "3 - COUNTA"),
    ("4", "4 - MAX"),
    ("5", "5 - MIN"),
    ("6", "6 - PRODUCT"),
    ("7", "7 - STDEV.S"),
    ("8", "8 - STDEV.P"),
    ("9", "9 - SUM"),
    ("10", "10 - VAR.S"),
    ("11", "11 - VAR.P"),
    ("12", "12 - MEDIAN"),
    ("13", "13 - MODE.SNGL"),
    ("14", "14 - LARGE"),
    ("15", "15 - SMALL"),
    ("16", "16 - PERCENTILE.INC"),
    ("17", "17 - QUARTILE.INC"),
    ("18", "18 - PERCENTILE.EXC"),
    ("19", "19 - QUARTILE.EXC"),
];

const AGGREGATE_OPTIONS: &[(&str, &str)] = &[
    ("0", "0 - Ignore nested SUBTOTAL and AGGREGATE functions"),
    ("1", "1 - Ignore hidden rows, nested SUBTOTAL and AGGREGATE functions"),
    ("2", "2 - Ignore error values, nested SUBTOTAL and AGGREGATE functions"),
    ("3", "3 - Ignore hidden rows, error values, nested SUBTOTAL and AGGREGATE functions"),
    ("4", "4 - Ignore nothing"),
    ("5", "5 - Ignore hidden rows"),
    ("6", "6 - Ignore error values"),
    ("7", "7 - Ignore hidden rows and error values"),
];

const WEEKDAY_TYPES: &[(&str, &str)] = &[
    ("1", "1 - Numbers 1 (Sunday) through 7 (Saturday)"),
    ("2", "2 - Numbers 1 (Monday) through 7 (Sunday)"),
    ("3", "3 - Numbers 0 (Monday) through 6 (Sunday)"),
    ("11", "11 - Numbers 1 (Monday) through 7 (Sunday)"),
    ("12", "12 - Numbers 1 (Tuesday) through 7 (Monday)"),
    ("13", "13 - Numbers 1 (Wednesday) through 7 (Tuesday)"),
    ("14", "14 - Numbers 1 (Thursday) through 7 (Wednesday)"),
    ("15", "15 - Numbers 1 (Friday) through 7 (Thursday)"),
    ("16", "16 - Numbers 1 (Saturday) through 7 (Friday)"),
    ("17", "17 - Numbers 1 (Sunday) through 7 (Saturday)"),
];

const WEEKNUM_TYPES: &[(&str, &str)] = &[
    ("1", "1 - Sunday"),
    ("2", "2 - Monday"),
    ("11", "11 - Monday"),
    ("12", "12 - Tuesday"),
    ("13", "13 - Wednesday"),
    ("14", "14 - Thursday"),
    ("15", "15 - Friday"),
    ("16", "16 - Saturday"),
    ("17", "17 - Sunday"),
    ("21", "21 - Monday (System 2)"),
];

/// WORKDAY.INTL's and NETWORKDAYS.INTL's weekend numbers.
const WEEKENDS: &[(&str, &str)] = &[
    ("1", "1 - Saturday, Sunday"),
    ("2", "2 - Sunday, Monday"),
    ("3", "3 - Monday, Tuesday"),
    ("4", "4 - Tuesday, Wednesday"),
    ("5", "5 - Wednesday, Thursday"),
    ("6", "6 - Thursday, Friday"),
    ("7", "7 - Friday, Saturday"),
    ("11", "11 - Sunday only"),
    ("12", "12 - Monday only"),
    ("13", "13 - Tuesday only"),
    ("14", "14 - Wednesday only"),
    ("15", "15 - Thursday only"),
    ("16", "16 - Friday only"),
    ("17", "17 - Saturday only"),
];

const DAY_COUNT_BASES: &[(&str, &str)] =
    &[("0", "0 - US (NASD) 30/360"), ("1", "1 - Actual/actual"), ("2", "2 - Actual/360"), ("3", "3 - Actual/365"), ("4", "4 - European 30/360")];

const XMATCH_MODES: &[(&str, &str)] = &[
    ("0", "0 - Exact match"),
    ("-1", "-1 - Exact match or next smaller item"),
    ("1", "1 - Exact match or next larger item"),
    ("2", "2 - Wildcard character match"),
];

const XSEARCH_MODES: &[(&str, &str)] = &[
    ("1", "1 - Search first-to-last"),
    ("-1", "-1 - Search last-to-first"),
    ("2", "2 - Binary search (sorted ascending order)"),
    ("-2", "-2 - Binary search (sorted descending order)"),
];

/// The values argument `arg` (0-based) of function `name` (as the formula syntax spells it)
/// takes, with their English labels; `None` for a free-form argument.
pub(crate) fn argument_choices(name: &str, arg: usize, locale: FormulaLocale) -> Option<Choices> {
    let english = crate::formula_bar::function_signature(locale.canonical_name(name))?;
    let (head, args) = split_signature(&english)?;
    let argument = argument_name(args.get(arg)?);
    ARGUMENT_CHOICES.iter().find(|(f, a, _)| f.eq_ignore_ascii_case(head) && *a == argument).map(|(_, _, choices)| *choices)
}

/// `SUM(number1, [number2], ...)` -> `SUM` and its arguments as written.
fn split_signature(signature: &str) -> Option<(&str, Vec<&str>)> {
    let (head, rest) = signature.split_once('(')?;
    let inner = rest.strip_suffix(')')?;
    Some((head, if inner.is_empty() { Vec::new() } else { inner.split(", ").collect() }))
}

/// `[number2]` -> `number`. A bracket can open or close on its own (SWITCH's
/// `[default_or_value2, result2]`, LAMBDA's `[parameter1, ...]`). INDIRECT's `a1` (A1 or R1C1
/// style) ends in a digit that is part of its name, not an index.
fn argument_name(arg: &str) -> &str {
    let arg = arg.strip_prefix('[').unwrap_or(arg);
    let arg = arg.strip_suffix(']').unwrap_or(arg);
    if arg == "a1" {
        return arg;
    }
    arg.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// `[number2]` -> `[número2]`: brackets, index and `...` stay. `function` is the English name.
fn translate_argument(function: &str, arg: &str, language: Language) -> Option<String> {
    let name = argument_name(arg);
    if name == "..." {
        return Some(arg.to_string());
    }
    // `arg` is `[`? + `name` + index + `]`?, so the first match is the name itself.
    Some(arg.replacen(name, language.function_argument_of(function, name)?, 1))
}

/// Avoid suggesting an alias that would call a workbook/table name instead of the function.
pub(crate) fn completion_name<'a>(name: &'a str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> &'a str {
    let alias = locale.function_name(name);
    if wb.name(alias, sheet).is_some() || wb.table(alias).is_some() { name } else { alias }
}

/// Engine-generated edit templates use canonical syntax. The only incomplete template is
/// the bare function opening emitted by Insert Function; typed user input never enters here.
pub(crate) fn display_template(text: &str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> String {
    if let Some(name) = text.strip_prefix('=').and_then(|s| s.strip_suffix('('))
        && !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.'))
    {
        return format!("={}(", completion_name(name, locale, wb, sheet));
    }
    display(text, locale, wb, sheet)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hint for `name`, empty when there is none.
    fn sig(name: &str, locale: FormulaLocale, language: Language) -> String {
        signature(name, locale, language).unwrap_or_default()
    }

    /// Hints keep the formula syntax's name and separators and translate the argument names
    /// only when the interface language has them all.
    #[test]
    fn function_help_follows_the_interface_language_and_the_formula_syntax() {
        use FormulaLocale::{En, Es};
        // Spanish help over English formulas: the name and `,` stay as typed; brackets, indexes
        // and `...` survive.
        assert_eq!(sig("IF", En, Language::Es), "IF(prueba_lógica, [valor_si_verdadero], [valor_si_falso])");
        assert_eq!(sig("SUM", En, Language::Es), "SUM(número1, [número2], ...)");
        assert_eq!(description("IF", En, Language::Es).as_deref(), Some("Devuelve un valor según una condición."));
        // Spanish names and separators, from a localized name.
        assert_eq!(sig("suma", Es, Language::Es), "SUMA(número1; [número2]; ...)");
        assert_eq!(description("SUMA", Es, Language::Es).as_deref(), Some("Suma los números."));
        // An argument list Spanish covers only in part stays English instead of mixing (the name
        // is whatever the Spanish formula table calls SUMIFS).
        let sumifs = sig("SUMIFS", Es, Language::Es);
        assert!(sumifs.ends_with("(sum_range; criteria_range1; criteria1; ...)"), "{sumifs}");
        assert_eq!(description("SUMIFS", Es, Language::Es).as_deref(), Some("Adds the cells that meet every condition."));
        assert_eq!(sig("IF", En, Language::En), "IF(logical_test, [value_if_true], [value_if_false])");
    }

    /// Japanese hints use Excel's Japanese argument names, per function, with English formulas.
    #[test]
    fn japanese_hints_use_excels_argument_names() {
        use FormulaLocale::En;
        let ja = Language::Ja;
        assert_eq!(sig("XLOOKUP", En, ja), "XLOOKUP(検索値, 検索範囲, 戻り範囲, [見つからない場合], [一致モード], [検索モード])");
        assert_eq!(sig("VLOOKUP", En, ja), "VLOOKUP(検索値, 範囲, 列番号, [検索方法])");
        assert_eq!(sig("IF", En, ja), "IF(論理式, [値が真の場合], [値が偽の場合])");
        assert_eq!(sig("SUMIFS", En, ja), "SUMIFS(合計対象範囲, 条件範囲1, 条件1, ...)");
        assert_eq!(sig("sum", En, ja), "SUM(数値1, [数値2], ...)");
        assert_eq!(sig("IFNA", En, ja), "IFNA(値, NA の場合の値)");
        assert_eq!(sig("NPER", En, ja), "NPER(利率, 定期支払額, 現在価値, [将来価値], [支払期日])");
        assert_eq!(sig("INDIRECT", En, ja), "INDIRECT(参照文字列, [参照形式])");
        assert_eq!(sig("TEXTJOIN", En, ja), "TEXTJOIN(区切り文字, 空のセルは無視, テキスト1, [テキスト2], ...)");
        assert_eq!(description("COUNTIF", En, ja).as_deref(), Some("指定された範囲に含まれるセルのうち、検索条件に一致するセルの個数を返します。"));
        // A function the Japanese help doesn't cover keeps its English help, whole.
        let english = sig("BESSELJ", En, Language::En);
        assert_eq!(sig("BESSELJ", En, ja), english);
    }

    #[test]
    fn mode_arguments_list_their_values() {
        use FormulaLocale::En;
        let modes = argument_choices("xlookup", 4, En).unwrap_or_default();
        assert_eq!(modes.iter().map(|(v, _)| *v).collect::<Vec<_>>(), ["0", "-1", "1", "2"]);
        assert_eq!(Language::Ja.tr(modes[1].1), "-1 - 完全一致または次に小さい項目");
        let lookup = argument_choices("VLOOKUP", 3, En).unwrap_or_default();
        assert_eq!(lookup.iter().map(|(_, l)| Language::Ja.tr(l)).collect::<Vec<_>>(), ["TRUE - 近似一致", "FALSE - 完全一致"]);
        assert!(argument_choices("VLOOKUP", 0, En).is_none());
        assert!(argument_choices("SUM", 7, En).is_none());
        let subtotal = argument_choices("SUBTOTAL", 0, En).unwrap_or_default();
        assert_eq!((subtotal.len(), subtotal[8].0, subtotal[19].1), (22, "9", "109 - SUM"));
        let weekday = argument_choices("WEEKDAY", 1, En).unwrap_or_default();
        assert_eq!(Language::Ja.tr(weekday[1].1), "2 - 1 (月曜) ～ 7 (日曜) の数値");
        let unique = argument_choices("UNIQUE", 2, En).unwrap_or_default();
        assert_eq!(Language::Ja.tr(unique[1].1), "TRUE - 1 回だけ出現する項目を返す");
        assert!(argument_choices("NETWORKDAYS.INTL", 2, En).is_some_and(|w| w.len() == 14));
    }

    /// Every choice label has its Japanese wording, except the ones that are only a function
    /// name (SUBTOTAL's `9 - SUM`), which Excel keeps in English.
    #[test]
    fn japanese_choice_labels_are_translated() {
        for (function, argument, choices) in ARGUMENT_CHOICES {
            for (value, label) in *choices {
                let function_name = label.strip_prefix(value).and_then(|l| l.strip_prefix(" - ")).unwrap_or_default();
                if function_name.chars().all(|c| c.is_ascii_uppercase() || c == '.') {
                    continue;
                }
                assert_ne!(Language::Ja.tr(label), *label, "{function} {argument}: {label}");
            }
        }
    }
}
