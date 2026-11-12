//! Conditional formatting (Home › Styles › Conditional Formatting): adding a rule, clearing rules,
//! and the Rules Manager's edits (replace, reorder, Applies To, Stop If True, duplicate, delete).
//!
//! A rule travels as JSON in one form both ways: [`parse_rule`] reads it (the `rule` of
//! `home.conditionalFormat`) and [`rule_params`] writes it, so the rule editor can be filled from a
//! stored rule and hand it back unchanged.

use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::{BorderLine, BorderStyle, Borders, CfOperator, CfRule, CfValueKind, Color, CondFormat, Fill, Style, icon_count};
use serde_json::{Map, Value as Json, json};

use super::{EngineError, Result, Session, bad, bool_param, edit, ok, str_param, target_ranges, target_sheet, u32_param};

const CMD: &str = "home.conditionalFormat";

pub fn preset_style(name: &str) -> Style {
    let mut s = Style::default();
    match name {
        "redText" => s.font.color = Color::rgb(0x9C, 0x00, 0x06),
        "yellowFill" | "yellowFillDarkYellowText" => {
            s.fill = Fill::solid(Color::rgb(0xFF, 0xEB, 0x9C));
            s.font.color = Color::rgb(0x9C, 0x57, 0x00);
        }
        "greenFill" | "greenFillDarkGreenText" => {
            s.fill = Fill::solid(Color::rgb(0xC6, 0xEF, 0xCE));
            s.font.color = Color::rgb(0x00, 0x61, 0x00);
        }
        "lightRedFill" => s.fill = Fill::solid(Color::rgb(0xFF, 0xC7, 0xCE)),
        "redBorder" => {
            let l = BorderLine { style: BorderStyle::Thin, color: Color::rgb(0xC0, 0x00, 0x00) };
            s.borders = Borders { left: l, right: l, top: l, bottom: l, ..Default::default() };
        }
        _ => {
            s.fill = Fill::solid(Color::rgb(0xFF, 0xC7, 0xCE));
            s.font.color = Color::rgb(0x9C, 0x00, 0x06);
        }
    }
    s
}

/// Whether `v` is one formula string literal: `"…"` with every inner quote doubled.
fn is_string_literal(v: &str) -> bool {
    v.len() >= 2 && v.starts_with('"') && v.ends_with('"') && v.get(1..v.len() - 1).is_some_and(|inner| !inner.replace("\"\"", "").contains('"'))
}

/// A conditional format's `style` as the model's `Style` JSON (`font`, `fill`, `borders`, `align`,
/// `num_fmt`, `protection`, merged over the default by the caller), where a colour may also be
/// written as `format_range` takes it (`"#RRGGBB"`, `{theme, tint}`, `"auto"`) and `fill` may be a
/// colour alone (a solid fill) or `"none"` (no fill). An unknown key is an error rather than
/// dropped: dropped, it left a rule with an empty format that matches and shows nothing.
fn cf_style_json(st: &Json) -> Result<Json> {
    const KEYS: [&str; 7] = ["font", "fill", "borders", "align", "num_fmt", "protection", "dxf_stated"];
    let err = |m: String| bad("home.conditionalFormat", m);
    let Some(obj) = st.as_object() else { return Err(err("`style` must be an object, e.g. {\"fill\": \"#FFC7CE\"}".into())) };
    let mut st = obj.clone();
    if let Some(k) = st.keys().find(|k| !KEYS.contains(&k.as_str())) {
        let hint = if k == "preset" { "; a preset goes in `rule.preset`" } else { "" };
        return Err(err(format!("unknown key `{k}` in `style` (known: {}){hint}", KEYS.join(", "))));
    }
    // A colour written the way the other formatting commands take it, as the model stores it.
    let color = |v: &mut Json, what: &str| -> Result<()> {
        if v.is_string() || v.get("theme").is_some() {
            let c = super::format::color_param(Some(v)).ok_or_else(|| err(format!("invalid colour {v} for `{what}`")))?;
            *v = serde_json::to_value(c).map_err(|e| EngineError::Other(e.to_string()))?;
        }
        Ok(())
    };
    if let Some(f) = st.get_mut("font").and_then(Json::as_object_mut)
        && let Some(c) = f.get_mut("color")
    {
        color(c, "font.color")?;
    }
    if let Some(fill) = st.get_mut("fill") {
        // `"none"` (or `"auto"`) is no fill, as `home.fillColor` reads it, not a solid automatic one.
        if fill.is_string() && super::format::color_param(Some(fill)) == Some(Color::Auto) {
            *fill = serde_json::to_value(Fill::default()).map_err(|e| EngineError::Other(e.to_string()))?;
        } else if fill.is_string() || fill.get("theme").is_some() {
            *fill = json!({"pattern": "Solid", "fg": fill.clone()});
        }
        if let Some(f) = fill.as_object_mut() {
            for k in ["fg", "bg"] {
                if let Some(c) = f.get_mut(k) {
                    color(c, &format!("fill.{k}"))?;
                }
            }
            // A colour with no pattern is a solid fill, not an invisible one.
            if f.contains_key("fg") && !f.contains_key("pattern") {
                f.insert("pattern".into(), json!("Solid"));
            }
        }
    }
    if let Some(b) = st.get_mut("borders").and_then(Json::as_object_mut) {
        for (side, line) in b.iter_mut() {
            if let Some(c) = line.get_mut("color") {
                color(c, &format!("borders.{side}.color"))?;
            }
        }
    }
    Ok(Json::Object(st))
}

/// #1999: a formula that doesn't parse must never reach the workbook — Excel refuses to open
/// files carrying it, even in repair mode. Validated here so the API and the New Rule dialog
/// (which dispatches this command) share one gate, the way Excel's object model refuses.
fn check_cf_formula(cmd: &str, what: &str, formula: &str) -> Result<()> {
    // Strip exactly one leading '=', the way the XLSX writer does: a value like '===A1'
    // only parses after several strips and would still be saved unparseable. (#1999 review)
    if gridcraft_formula::parse(formula.strip_prefix('=').unwrap_or(formula)).is_err() {
        return Err(bad(cmd, format!("invalid {what} formula `{formula}` — it does not parse; Excel would reject the saved file")));
    }
    Ok(())
}

fn check_cf_value(what: &str, v: &CfValueKind) -> Result<()> {
    if let gridcraft_model::features::CfValueKind::Formula(f) = v {
        check_cf_formula("home.conditionalFormat", what, f)?;
    }
    Ok(())
}

fn check_cf_rule(rule: &CfRule) -> Result<()> {
    match rule {
        CfRule::CellIs { a, b, .. } => {
            check_cf_formula(CMD, "comparison value", a)?;
            if let Some(b) = b {
                check_cf_formula(CMD, "second comparison value", b)?;
            }
        }
        CfRule::Expression { formula, .. } => check_cf_formula(CMD, "rule", formula)?,
        CfRule::DataBar { min, max, .. } => {
            check_cf_value("data bar minimum", min)?;
            check_cf_value("data bar maximum", max)?;
        }
        CfRule::ColorScale { stops } => {
            for (kind, _) in stops {
                check_cf_value("color scale threshold", kind)?;
            }
        }
        CfRule::IconSet { thresholds, .. } => {
            for kind in thresholds {
                check_cf_value("icon threshold", kind)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// A threshold (`cfvo`) in the JSON form: `{type: min|max|num|percent|percentile|formula, value}`.
/// A value that isn't a number makes a `num` threshold a formula (`=$A$1`), as Excel's New Rule
/// dialog accepts a reference under Number.
fn value_kind(v: Option<&Json>, default: CfValueKind) -> CfValueKind {
    let Some(Json::Object(o)) = v else { return default };
    let raw = o.get("value");
    let text = raw.map(super::edit::json_to_input).unwrap_or_default();
    let n = raw.and_then(Json::as_f64).or_else(|| text.trim().parse::<f64>().ok()).filter(|n| n.is_finite());
    // Kept as typed: the writer strips one `=`, and what is left must parse (#1999), so `===A1`
    // is refused rather than saved.
    let formula = || CfValueKind::Formula(text.trim().to_string());
    match o.get("type").and_then(Json::as_str).unwrap_or("num") {
        "min" | "lowest" | "autoMin" => CfValueKind::Min,
        "max" | "highest" | "autoMax" => CfValueKind::Max,
        "percent" => n.map(CfValueKind::Percent).unwrap_or_else(formula),
        "percentile" => n.map(CfValueKind::Percentile).unwrap_or_else(formula),
        "formula" => formula(),
        _ => n.map(CfValueKind::Number).unwrap_or_else(formula),
    }
}

/// The JSON form of a threshold, read back by [`value_kind`].
fn value_kind_params(v: &CfValueKind) -> Json {
    match v {
        CfValueKind::Min => json!({"type": "min"}),
        CfValueKind::Max => json!({"type": "max"}),
        CfValueKind::Number(n) => json!({"type": "num", "value": n}),
        CfValueKind::Percent(n) => json!({"type": "percent", "value": n}),
        CfValueKind::Percentile(n) => json!({"type": "percentile", "value": n}),
        CfValueKind::Formula(f) if f.starts_with('=') => json!({"type": "formula", "value": f}),
        CfValueKind::Formula(f) => json!({"type": "formula", "value": format!("={f}")}),
    }
}

/// A colour in the form `color_param` reads (`"#RRGGBB"`, `{theme, tint}`, `"auto"`).
pub fn color_params(c: &Color) -> Json {
    match *c {
        Color::Auto => json!("auto"),
        Color::Rgb(v) => json!(format!("#{:06X}", v & 0xFF_FFFF)),
        Color::Theme(i, t) => json!({"theme": i, "tint": f64::from(t) / 1000.0}),
    }
}

/// A rule's format from the `style` (model `Style` JSON, where colours may also be written as the
/// formatting commands take them) or the `preset` of the quick dialogs.
fn rule_style(rule: &Json) -> Result<Box<Style>> {
    Ok(Box::new(match rule.get("style") {
        Some(st) if !st.is_null() => {
            let st = cf_style_json(st)?;
            let base = serde_json::to_value(Style::default()).map_err(|e| EngineError::Other(e.to_string()))?;
            serde_json::from_value(super::format::merge_json(base, &st)).map_err(|e| bad(CMD, e.to_string()))?
        }
        _ => preset_style(rule.get("preset").and_then(Json::as_str).unwrap_or("lightRedFillDarkRedText")),
    }))
}

fn flag(rule: &Json, k: &str) -> bool {
    rule.get(k).and_then(Json::as_bool).unwrap_or(false)
}

/// Reads a rule in the JSON form `home.conditionalFormat` takes (see its spec) and checks its
/// formulas: a formula that doesn't parse is refused, never stored.
pub fn parse_rule(rule: &Json) -> Result<CfRule> {
    let g = |k: &str| rule.get(k).and_then(Json::as_str).map(str::to_string);
    let kind = g("type").unwrap_or_else(|| "cellIs".into());
    let style = || rule_style(rule);
    let rule = match kind.as_str() {
        "cellIs" => {
            // Excel's file and VBA names (`lessThan`, …) are accepted too.
            let op = match g("operator").as_deref().unwrap_or("greater") {
                "between" => CfOperator::Between,
                "notBetween" => CfOperator::NotBetween,
                "equal" => CfOperator::Equal,
                "notEqual" => CfOperator::NotEqual,
                "less" | "lessThan" => CfOperator::Less,
                "greaterOrEqual" | "greaterThanOrEqual" => CfOperator::GreaterOrEqual,
                "lessOrEqual" | "lessThanOrEqual" => CfOperator::LessOrEqual,
                "greater" | "greaterThan" => CfOperator::Greater,
                other => return Err(bad(CMD, format!("unknown operator `{other}`"))),
            };
            let a = rule.get("value").map(super::edit::json_to_input).unwrap_or_default();
            let between = matches!(op, CfOperator::Between | CfOperator::NotBetween);
            // Only Between and Not Between have a second value; a stale one would be saved as a
            // second `<formula>` Excel doesn't expect.
            let b = rule.get("value2").filter(|_| between).map(super::edit::json_to_input);
            let quote = |v: String| {
                // A number, a formula, or text already written as a string literal (`"Closed"`, as
                // in the file and in openpyxl's `CellIsRule`) is kept; other text is quoted.
                if gridcraft_core::parse::parse_number_text(&v).is_some() || v.starts_with('=') {
                    v.trim_start_matches('=').to_string()
                } else if is_string_literal(&v) {
                    v
                } else {
                    format!("\"{}\"", v.replace('"', "\"\""))
                }
            };
            if a.trim().is_empty() || (between && b.as_deref().is_none_or(|b| b.trim().is_empty())) {
                return Err(bad(CMD, "enter a value to compare the cells with"));
            }
            CfRule::CellIs { op, a: quote(a), b: b.map(quote), style: style()? }
        }
        "expression" => {
            let formula = g("formula").unwrap_or_default().trim().trim_start_matches('=').to_string();
            if formula.is_empty() {
                return Err(bad(CMD, "enter a formula for the rule"));
            }
            CfRule::Expression { formula, style: style()? }
        }
        "containsText" => CfRule::ContainsText { text: g("text").unwrap_or_default(), style: style()? },
        "notContainsText" => CfRule::NotContainsText { text: g("text").unwrap_or_default(), style: style()? },
        "beginsWith" => CfRule::BeginsWith { text: g("text").unwrap_or_default(), style: style()? },
        "endsWith" => CfRule::EndsWith { text: g("text").unwrap_or_default(), style: style()? },
        "blanks" => CfRule::Blanks { style: style()? },
        "noBlanks" => CfRule::NoBlanks { style: style()? },
        "errors" => CfRule::Errors { style: style()? },
        "noErrors" => CfRule::NoErrors { style: style()? },
        "duplicate" => CfRule::Duplicate { style: style()? },
        "unique" => CfRule::Unique { style: style()? },
        "top10" => {
            let percent = flag(rule, "percent");
            // Excel takes 1 to 1000 items, or 1 to 100 percent.
            let rank = rule.get("rank").and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())).unwrap_or(10.0);
            let max = if percent { 100.0 } else { 1000.0 };
            if !(1.0..=max).contains(&rank) || rank.fract() != 0.0 {
                return Err(bad(CMD, format!("the rank must be a whole number from 1 to {max}")));
            }
            CfRule::Top10 { bottom: flag(rule, "bottom"), percent, rank: rank as u32, style: style()? }
        }
        "aboveAverage" => CfRule::AboveAverage {
            below: flag(rule, "below"),
            equal: flag(rule, "equal"),
            std_dev: rule.get("stdDev").and_then(Json::as_u64).unwrap_or(0).min(3) as u8,
            style: style()?,
        },
        "timePeriod" => {
            const PERIODS: [&str; 10] =
                ["yesterday", "today", "tomorrow", "last7Days", "lastWeek", "thisWeek", "nextWeek", "lastMonth", "thisMonth", "nextMonth"];
            let period = g("period").unwrap_or_else(|| "today".into());
            if !PERIODS.contains(&period.as_str()) {
                return Err(bad(CMD, format!("unknown period `{period}` (known: {})", PERIODS.join(", "))));
            }
            CfRule::TimePeriod { period, style: style()? }
        }
        "colorScale" => {
            let stops = if let Some(stops) = rule.get("stops").and_then(Json::as_array) {
                if !(2..=3).contains(&stops.len()) {
                    return Err(bad(CMD, "a colour scale has 2 or 3 stops"));
                }
                let last = stops.len() - 1;
                stops
                    .iter()
                    .enumerate()
                    .map(|(i, st)| {
                        let default = match i {
                            0 => CfValueKind::Min,
                            i if i == last => CfValueKind::Max,
                            _ => CfValueKind::Percentile(50.0),
                        };
                        (value_kind(Some(st), default), super::format::color_param(st.get("color")).unwrap_or_default())
                    })
                    .collect()
            } else {
                let cols: Vec<Color> = rule
                    .get("colors")
                    .and_then(Json::as_array)
                    .map(|a| a.iter().filter_map(|c| super::format::color_param(Some(c))).collect())
                    .unwrap_or_else(|| vec![Color::rgb(0xF8, 0x69, 0x6B), Color::rgb(0xFF, 0xEB, 0x84), Color::rgb(0x63, 0xBE, 0x7B)]);
                match cols.as_slice() {
                    [a, b] => vec![(CfValueKind::Min, *a), (CfValueKind::Max, *b)],
                    _ => vec![
                        (CfValueKind::Min, cols.first().copied().unwrap_or_default()),
                        (CfValueKind::Percentile(50.0), cols.get(1).copied().unwrap_or_default()),
                        (CfValueKind::Max, cols.get(2).copied().unwrap_or_default()),
                    ],
                }
            };
            CfRule::ColorScale { stops }
        }
        "dataBar" => CfRule::DataBar {
            min: value_kind(rule.get("min"), CfValueKind::Min),
            max: value_kind(rule.get("max"), CfValueKind::Max),
            color: super::format::color_param(rule.get("color")).unwrap_or(Color::rgb(0x63, 0x8E, 0xC6)),
            gradient: rule.get("gradient").and_then(Json::as_bool).unwrap_or(true),
            show_value: rule.get("showValue").and_then(Json::as_bool).unwrap_or(true),
        },
        "iconSet" => {
            let set = g("set").unwrap_or_else(|| "3TrafficLights1".into());
            if !ICON_SETS.contains(&set.as_str()) {
                return Err(bad(CMD, format!("unknown icon set `{set}`")));
            }
            // One threshold per icon, the lowest first; a list one short leaves out the first
            // (always the lower bound), as Excel's dialog shows them.
            let k = icon_count(&set);
            let mut thresholds: Vec<CfValueKind> = rule
                .get("thresholds")
                .and_then(Json::as_array)
                .map(|a| a.iter().map(|t| value_kind(Some(t), CfValueKind::Percent(0.0))).collect())
                .unwrap_or_default();
            if thresholds.len() + 1 == k {
                thresholds.insert(0, CfValueKind::Percent(0.0));
            }
            if !thresholds.is_empty() && thresholds.len() != k {
                return Err(bad(CMD, format!("`{set}` takes {} thresholds", k - 1)));
            }
            CfRule::IconSet {
                set,
                thresholds,
                reverse: flag(rule, "reverse"),
                show_value: rule.get("showValue").and_then(Json::as_bool).unwrap_or(true),
            }
        }
        other => return Err(bad(CMD, format!("unknown rule type `{other}`"))),
    };
    check_cf_rule(&rule)?;
    Ok(rule)
}

/// The icon sets of Excel's Icon Style list, as the file names them.
pub const ICON_SETS: [&str; 20] = [
    "3Arrows",
    "3ArrowsGray",
    "3Flags",
    "3TrafficLights1",
    "3TrafficLights2",
    "3Signs",
    "3Symbols",
    "3Symbols2",
    "3Stars",
    "3Triangles",
    "4Arrows",
    "4ArrowsGray",
    "4RedToBlack",
    "4Rating",
    "4TrafficLights",
    "5Arrows",
    "5ArrowsGray",
    "5Rating",
    "5Quarters",
    "5Boxes",
];

/// The JSON form of a rule, as [`parse_rule`] reads it back: what Manage Rules lists and the rule
/// editor is filled from. The format is the model's `style`.
pub fn rule_params(rule: &CfRule) -> Json {
    let mut m = Map::new();
    let mut put = |k: &str, v: Json| {
        m.insert(k.into(), v);
    };
    // A stored comparison value is formula text: a number or a string literal stays as it is, and
    // anything else (a reference, a formula) gets back its `=`.
    let value = |v: &str| {
        if gridcraft_core::parse::parse_number_text(v).is_some() || is_string_literal(v) { v.to_string() } else { format!("={v}") }
    };
    let ty = match rule {
        CfRule::CellIs { op, a, b, .. } => {
            let op = match op {
                CfOperator::Between => "between",
                CfOperator::NotBetween => "notBetween",
                CfOperator::Equal => "equal",
                CfOperator::NotEqual => "notEqual",
                CfOperator::Greater => "greaterThan",
                CfOperator::Less => "lessThan",
                CfOperator::GreaterOrEqual => "greaterThanOrEqual",
                CfOperator::LessOrEqual => "lessThanOrEqual",
            };
            put("operator", json!(op));
            put("value", json!(value(a)));
            if let Some(b) = b {
                put("value2", json!(value(b)));
            }
            "cellIs"
        }
        CfRule::Expression { formula, .. } => {
            put("formula", json!(format!("={formula}")));
            "expression"
        }
        CfRule::ContainsText { text, .. }
        | CfRule::NotContainsText { text, .. }
        | CfRule::BeginsWith { text, .. }
        | CfRule::EndsWith { text, .. } => {
            put("text", json!(text));
            match rule {
                CfRule::ContainsText { .. } => "containsText",
                CfRule::NotContainsText { .. } => "notContainsText",
                CfRule::BeginsWith { .. } => "beginsWith",
                _ => "endsWith",
            }
        }
        CfRule::Blanks { .. } => "blanks",
        CfRule::NoBlanks { .. } => "noBlanks",
        CfRule::Errors { .. } => "errors",
        CfRule::NoErrors { .. } => "noErrors",
        CfRule::Duplicate { .. } => "duplicate",
        CfRule::Unique { .. } => "unique",
        CfRule::Top10 { bottom, percent, rank, .. } => {
            put("rank", json!(rank));
            put("bottom", json!(bottom));
            put("percent", json!(percent));
            "top10"
        }
        CfRule::AboveAverage { below, equal, std_dev, .. } => {
            put("below", json!(below));
            put("equal", json!(equal));
            put("stdDev", json!(std_dev));
            "aboveAverage"
        }
        CfRule::TimePeriod { period, .. } => {
            put("period", json!(period));
            "timePeriod"
        }
        CfRule::ColorScale { stops } => {
            let stops: Vec<Json> = stops
                .iter()
                .map(|(v, c)| {
                    let mut s = value_kind_params(v);
                    s["color"] = color_params(c);
                    s
                })
                .collect();
            put("stops", json!(stops));
            "colorScale"
        }
        CfRule::DataBar { min, max, color, gradient, show_value } => {
            put("min", value_kind_params(min));
            put("max", value_kind_params(max));
            put("color", color_params(color));
            put("gradient", json!(gradient));
            put("showValue", json!(show_value));
            "dataBar"
        }
        CfRule::IconSet { set, thresholds, reverse, show_value } => {
            put("set", json!(set));
            put("thresholds", json!(thresholds.iter().map(value_kind_params).collect::<Vec<_>>()));
            put("reverse", json!(reverse));
            put("showValue", json!(show_value));
            "iconSet"
        }
    };
    m.insert("type".into(), json!(ty));
    if let Some(st) = rule.style()
        && let Ok(v) = serde_json::to_value(st)
    {
        m.insert("style".into(), v);
    }
    Json::Object(m)
}

/// An Applies To reference (`=$A$1:$A$10,$C$2`, with or without `$`, `=` and a sheet name), as
/// ranges; `None` when any part isn't a reference. Areas may be separated by commas or spaces.
pub fn parse_applies_to(text: &str) -> Option<Vec<RangeRef>> {
    let t = text.trim().trim_start_matches('=');
    let mut out = Vec::new();
    for part in t.split([',', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
        let (_, body) = super::split_sheet(part);
        out.push(RangeRef::parse(body)?);
    }
    (!out.is_empty()).then_some(out)
}

pub(super) fn add_cf(s: &mut Session, p: &Json) -> Result<Json> {
    let ranges = target_ranges(s, p)?;
    let sheet = target_sheet(s, p)?;
    let Some(rule) = p.get("rule") else {
        s.ui_requests.push(crate::UiRequest::Dialog("newFormattingRule".into(), json!({})));
        return ok();
    };
    let stop_if_true = flag(rule, "stopIfTrue") || bool_param(p, "stopIfTrue").unwrap_or(false);
    let rule = parse_rule(rule)?;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        sort_by_priority(&mut sh.cond_formats);
        // New rules take precedence over older ones.
        sh.cond_formats.insert(0, CondFormat { ranges: ranges.clone(), rule, priority: 1, stop_if_true });
        renumber(&mut sh.cond_formats);
        Ok(json!({"rules": sh.cond_formats.len()}))
    })
}

pub(super) fn clear_cf(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let whole = bool_param(p, "sheet").unwrap_or(false);
    let ranges = target_ranges(s, p)?;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        if whole {
            sh.cond_formats.clear();
        } else {
            for cf in sh.cond_formats.iter_mut() {
                let before = RangeRef::top_left(&cf.ranges);
                cf.ranges.retain(|r| !ranges.iter().any(|x| x.intersects(r)));
                // The relative formulas are written for the top-left cell: when it goes, they're
                // rewritten for the new one, so the areas left keep reading the same cells.
                if let (Some(from), Some(to)) = (before, RangeRef::top_left(&cf.ranges)) {
                    rebase_cf_formulas(&mut cf.rule, from, to);
                }
            }
            sh.cond_formats.retain(|c| !c.ranges.is_empty());
        }
        Ok(Json::Null)
    })
}

/// Moves the relative references of a rule's per-cell formulas written for `from` to `to`.
fn rebase_cf_formulas(rule: &mut CfRule, from: CellRef, to: CellRef) {
    let mv = |f: &str| crate::cmd::data::move_rule_formula(f, from, to);
    match rule {
        CfRule::CellIs { a, b, .. } => {
            *a = mv(a);
            *b = b.as_deref().map(mv);
        }
        CfRule::Expression { formula, .. } => *formula = mv(formula),
        _ => {}
    }
}

/// Puts the rules in the order they apply. A file's rules are read block by block, which need not
/// be their priority order; the Rules Manager's indexes count in priority order.
fn sort_by_priority(rules: &mut [CondFormat]) {
    rules.sort_by_key(|c| c.priority);
}

/// Priorities 1..n in list order, the first applying first.
fn renumber(rules: &mut [CondFormat]) {
    for (k, cf) in rules.iter_mut().enumerate() {
        cf.priority = k as u32 + 1;
    }
}

/// One rule as `home.manageRules` lists it and takes it in `set`.
fn listed(c: &CondFormat) -> Json {
    json!({
        "appliesTo": c.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(","),
        "ranges": c.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>(),
        "rule": rule_params(&c.rule),
        "stopIfTrue": c.stop_if_true,
        "priority": c.priority,
    })
}

const MANAGE: &str = "home.manageRules";

/// A `set` entry: `{appliesTo | ranges, rule | cfRule, stopIfTrue?}`, where `cfRule` is the model's
/// own form of the rule (what the Rules Manager sends, so a rule it didn't touch is kept exactly).
fn entry(e: &Json) -> Result<CondFormat> {
    let ranges = match e.get("appliesTo").and_then(Json::as_str) {
        Some(t) => parse_applies_to(t).ok_or_else(|| bad(MANAGE, format!("`{t}` is not a reference")))?,
        None => {
            let list = e.get("ranges").and_then(Json::as_array).ok_or_else(|| bad(MANAGE, "each rule needs `appliesTo`"))?;
            let text: Vec<&str> = list.iter().filter_map(Json::as_str).collect();
            parse_applies_to(&text.join(",")).ok_or_else(|| bad(MANAGE, "`ranges` must be references"))?
        }
    };
    let rule = match e.get("cfRule") {
        Some(model) => {
            let rule: CfRule = serde_json::from_value(model.clone()).map_err(|err| bad(MANAGE, format!("invalid `cfRule`: {err}")))?;
            check_cf_rule(&rule)?;
            rule
        }
        None => parse_rule(e.get("rule").ok_or_else(|| bad(MANAGE, "each rule needs `rule`"))?)?,
    };
    Ok(CondFormat { ranges, rule, priority: 0, stop_if_true: flag(e, "stopIfTrue") })
}

/// Lists the rules in priority order, or edits them: `set` replaces the whole list (what the
/// Rules Manager's OK and Apply send), and the single edits take the rule's `index` in that list.
pub(super) fn manage_cf(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    const EDITS: [&str; 8] = ["delete", "moveUp", "moveDown", "set", "edit", "appliesTo", "stopIfTrue", "duplicate"];
    if !EDITS.iter().any(|k| p.get(*k).is_some()) {
        let mut rules: Vec<CondFormat> = s.doc()?.wb.sheet(sheet).map(|sh| sh.cond_formats.clone()).unwrap_or_default();
        sort_by_priority(&mut rules);
        return Ok(json!({"rules": rules.iter().map(listed).collect::<Vec<_>>()}));
    }
    // Everything is read and checked before the workbook changes, so a bad rule leaves it as it was.
    let set = match p.get("set") {
        Some(Json::Array(list)) => Some(list.iter().map(entry).collect::<Result<Vec<_>>>()?),
        Some(_) => return Err(bad(MANAGE, "`set` is a list of rules")),
        None => None,
    };
    let replacement = p.get("rule").map(parse_rule).transpose()?;
    let applies_to = match str_param(p, "appliesTo") {
        Some(t) => Some(parse_applies_to(t).ok_or_else(|| bad(MANAGE, format!("`{t}` is not a reference")))?),
        None => None,
    };
    let index = u32_param(p, "index").or_else(|| u32_param(p, "edit")).map(|v| v as usize);
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        if let Some(list) = set {
            sh.cond_formats = list;
            renumber(&mut sh.cond_formats);
            return Ok(json!({"rules": sh.cond_formats.len()}));
        }
        sort_by_priority(&mut sh.cond_formats);
        let n = sh.cond_formats.len();
        let at = |i: Option<usize>| i.filter(|i| *i < n).ok_or_else(|| bad(MANAGE, "no rule at that index"));
        if let Some(rule) = replacement {
            let i = at(index)?;
            if let Some(cf) = sh.cond_formats.get_mut(i) {
                cf.rule = rule;
            }
        }
        if let Some(ranges) = applies_to {
            let i = at(index)?;
            if let Some(cf) = sh.cond_formats.get_mut(i) {
                cf.ranges = ranges;
            }
        }
        if let Some(stop) = p.get("stopIfTrue").and_then(Json::as_bool) {
            let i = at(index)?;
            if let Some(cf) = sh.cond_formats.get_mut(i) {
                cf.stop_if_true = stop;
            }
        }
        // The copy goes right after the original, as Duplicate Rule puts it.
        if let Some(i) = u32_param(p, "duplicate").map(|v| v as usize)
            && let Some(cf) = sh.cond_formats.get(i).cloned()
        {
            sh.cond_formats.insert(i + 1, cf);
        }
        // The count is taken again after each step: a delete shortens the list.
        if let Some(i) = u32_param(p, "delete").map(|v| v as usize)
            && i < sh.cond_formats.len()
        {
            sh.cond_formats.remove(i);
        }
        if let Some(i) = u32_param(p, "moveUp").map(|v| v as usize)
            && i > 0
            && i < sh.cond_formats.len()
        {
            sh.cond_formats.swap(i, i - 1);
        }
        if let Some(i) = u32_param(p, "moveDown").map(|v| v as usize)
            && i + 1 < sh.cond_formats.len()
        {
            sh.cond_formats.swap(i, i + 1);
        }
        renumber(&mut sh.cond_formats);
        Ok(Json::Null)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.new_workbook();
        s
    }

    fn rules(s: &mut Session) -> Vec<Json> {
        s.execute("home.manageRules", json!({})).unwrap()["rules"].as_array().cloned().unwrap_or_default()
    }

    #[test]
    fn every_rule_type_survives_its_own_params() {
        let custom = json!({"font": {"bold": true, "color": "#9C0006"}, "fill": "#FFC7CE"});
        let samples = [
            json!({"type": "cellIs", "operator": "greaterThan", "value": 5000}),
            json!({"type": "cellIs", "operator": "between", "value": "100", "value2": "=$B$1"}),
            json!({"type": "cellIs", "operator": "equal", "value": "東"}),
            json!({"type": "expression", "formula": "=$C2>100"}),
            json!({"type": "containsText", "text": "東"}),
            json!({"type": "notContainsText", "text": "x"}),
            json!({"type": "beginsWith", "text": "A"}),
            json!({"type": "endsWith", "text": "Z"}),
            json!({"type": "blanks"}),
            json!({"type": "noErrors"}),
            json!({"type": "duplicate"}),
            json!({"type": "unique"}),
            json!({"type": "top10", "rank": 5, "bottom": true, "percent": true}),
            json!({"type": "aboveAverage", "below": true, "equal": true}),
            json!({"type": "aboveAverage", "stdDev": 2}),
            json!({"type": "timePeriod", "period": "last7Days"}),
            json!({"type": "colorScale", "stops": [{"type": "num", "value": 0, "color": "#FF0000"}, {"type": "percent", "value": 50, "color": {"theme": 4, "tint": 0.399}}, {"type": "formula", "value": "=$A$1", "color": "#00FF00"}]}),
            json!({"type": "dataBar", "min": {"type": "percentile", "value": 10}, "max": {"type": "max"}, "color": "#638EC6", "gradient": false, "showValue": false}),
            json!({"type": "iconSet", "set": "3Arrows", "thresholds": [{"type": "num", "value": 10}, {"type": "percent", "value": 80}], "reverse": true, "showValue": false}),
        ];
        for sample in samples {
            let mut with_style = sample.clone();
            with_style["style"] = custom.clone();
            let rule = parse_rule(&with_style).unwrap();
            let again = parse_rule(&rule_params(&rule)).unwrap();
            assert_eq!(rule, again, "{sample}");
        }
    }

    #[test]
    fn options_the_old_command_dropped_are_kept() {
        let r = parse_rule(&json!({"type": "aboveAverage", "below": true, "equal": true, "stdDev": 1})).unwrap();
        assert!(matches!(r, CfRule::AboveAverage { below: true, equal: true, std_dev: 1, .. }));
        let r = parse_rule(&json!({"type": "iconSet", "set": "4Rating", "thresholds": [{"type": "percent", "value": 25}, {"type": "percent", "value": 50}, {"type": "percent", "value": 75}]})).unwrap();
        let CfRule::IconSet { thresholds, .. } = r else { panic!() };
        assert_eq!(thresholds, [CfValueKind::Percent(0.0), CfValueKind::Percent(25.0), CfValueKind::Percent(50.0), CfValueKind::Percent(75.0)]);
        // The second value of an operator that has none is not kept.
        let r = parse_rule(&json!({"type": "cellIs", "operator": "greaterThan", "value": 1, "value2": 2})).unwrap();
        assert!(matches!(r, CfRule::CellIs { b: None, .. }));
        for bad_rule in [
            json!({"type": "top10", "rank": 0}),
            json!({"type": "top10", "rank": 101, "percent": true}),
            json!({"type": "timePeriod", "period": "someday"}),
            json!({"type": "iconSet", "set": "3Arrows", "thresholds": [{"type": "num", "value": 1}]}),
            json!({"type": "cellIs", "operator": "between", "value": 1}),
            json!({"type": "expression", "formula": "=SUM("}),
        ] {
            assert!(parse_rule(&bad_rule).is_err(), "{bad_rule}");
        }
    }

    #[test]
    fn manage_rules_edits_in_priority_order() {
        let mut s = session();
        for v in [1, 2, 3] {
            s.execute("home.conditionalFormat", json!({"range": "A1:A5", "rule": {"type": "cellIs", "operator": "greaterThan", "value": v}}))
                .unwrap();
        }
        // Newest first.
        let values: Vec<_> = rules(&mut s).iter().map(|r| r["rule"]["value"].clone()).collect();
        assert_eq!(values, [json!("3"), json!("2"), json!("1")]);
        s.execute("home.manageRules", json!({"index": 0, "rule": {"type": "top10", "rank": 3}, "appliesTo": "=$B$1:$B$9,$D$2", "stopIfTrue": true}))
            .unwrap();
        s.execute("home.manageRules", json!({"duplicate": 2})).unwrap();
        let list = rules(&mut s);
        assert_eq!(list.len(), 4);
        assert_eq!(list[0]["rule"]["type"], "top10");
        assert_eq!(list[0]["appliesTo"], "B1:B9,D2");
        assert_eq!(list[0]["stopIfTrue"], true);
        assert_eq!(list[2]["rule"], list[3]["rule"]);
        assert_eq!(list.iter().map(|r| r["priority"].as_u64().unwrap()).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert!(s.execute("home.manageRules", json!({"index": 9, "stopIfTrue": true})).is_err());
        assert!(s.execute("home.manageRules", json!({"index": 0, "appliesTo": "nope!"})).is_err());
        // `set` replaces the list in the order given, the model's form of a rule included.
        let model = serde_json::to_value(CfRule::Duplicate { style: Box::default() }).unwrap();
        s.execute(
            "home.manageRules",
            json!({"set": [{"appliesTo": "C1:C3", "cfRule": model}, {"appliesTo": "A1", "rule": {"type": "blanks"}, "stopIfTrue": true}]}),
        )
        .unwrap();
        let list = rules(&mut s);
        assert_eq!(list.len(), 2);
        assert_eq!((list[0]["rule"]["type"].as_str(), list[1]["stopIfTrue"].as_bool()), (Some("duplicate"), Some(true)));
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(rules(&mut s).len(), 4);
    }

    #[test]
    fn a_files_rules_are_managed_in_priority_order() {
        let mut s = session();
        s.execute("home.conditionalFormat", json!({"range": "A1", "rule": {"type": "blanks"}})).unwrap();
        s.execute("home.conditionalFormat", json!({"range": "A2", "rule": {"type": "errors"}})).unwrap();
        // As read from a file whose blocks are not in priority order.
        if let Some(sh) = s.active_mut().and_then(|d| std::sync::Arc::make_mut(&mut d.wb).sheet_mut(0)) {
            sh.cond_formats.reverse();
        }
        s.execute("home.manageRules", json!({"moveDown": 0})).unwrap();
        let types: Vec<_> = rules(&mut s).iter().map(|r| r["rule"]["type"].clone()).collect();
        assert_eq!(types, [json!("blanks"), json!("errors")]);
    }
}
