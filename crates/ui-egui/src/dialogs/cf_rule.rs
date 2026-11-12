//! New Formatting Rule and Edit Formatting Rule (Home › Conditional Formatting › New Rule…, and
//! the Rules Manager's New Rule… and Edit Rule…), the format previews they and the Rules Manager
//! draw, and the descriptions the Rules Manager lists.
//!
//! The dialog keeps its controls in `values` as flat keys (`ruleType`, `formatStyle`, `minType`, …)
//! so `ui.dialog.set` reaches every one; [`values_to_rule`] turns them into the rule JSON
//! `home.conditionalFormat` takes, and [`rule_to_values`] fills them from a stored rule.

use egui::{Color32, Stroke, StrokeKind, vec2};
use gridcraft_engine::cmd::cond_format::{ICON_SETS, parse_rule, rule_params};
use gridcraft_engine::model::{BorderStyle, CfOperator, CfRule, Color, CondFormat, Style, Theme, Underline};

use super::{Dialog, Json, Map, RefPick, SheetApp, json};
use crate::i18n::Language;

/// The rule types of Select a Rule Type, in Excel's order.
pub(super) const RULE_TYPES: [(&str, &str); 6] = [
    ("values", "Format all cells based on their values"),
    ("contain", "Format only cells that contain"),
    ("rank", "Format only top or bottom ranked values"),
    ("average", "Format only values that are above or below average"),
    ("unique", "Format only unique or duplicate values"),
    ("formula", "Use a formula to determine which cells to format"),
];

const FORMAT_STYLES: [(&str, &str); 4] =
    [("2color", "2-Color Scale"), ("3color", "3-Color Scale"), ("dataBar", "Data Bar"), ("iconSet", "Icon Sets")];

/// Threshold types: Minimum lists Lowest Value first, Maximum Highest Value, Midpoint neither.
const VALUE_TYPES: [(&str, &str); 6] = [
    ("min", "Value Type|Lowest Value"),
    ("num", "Value Type|Number"),
    ("percent", "Value Type|Percent"),
    ("formula", "Value Type|Formula"),
    ("percentile", "Value Type|Percentile"),
    ("max", "Value Type|Highest Value"),
];

const CONTAIN_KINDS: [(&str, &str); 7] = [
    ("cellValue", "Cell Value"),
    ("specificText", "Specific Text"),
    ("dates", "Dates Occurring"),
    ("blanks", "Rule|Blanks"),
    ("noBlanks", "Rule|No Blanks"),
    ("errors", "Rule|Errors"),
    ("noErrors", "Rule|No Errors"),
];

pub(super) const OPERATORS: [(&str, &str); 8] = [
    ("between", "CF Operator|between"),
    ("notBetween", "CF Operator|not between"),
    ("equal", "CF Operator|equal to"),
    ("notEqual", "CF Operator|not equal to"),
    ("greaterThan", "CF Operator|greater than"),
    ("lessThan", "CF Operator|less than"),
    ("greaterThanOrEqual", "CF Operator|greater than or equal to"),
    ("lessThanOrEqual", "CF Operator|less than or equal to"),
];

const TEXT_OPS: [(&str, &str); 4] =
    [("containsText", "containing"), ("notContainsText", "not containing"), ("beginsWith", "beginning with"), ("endsWith", "ending with")];

pub(super) const PERIODS: [(&str, &str); 10] = [
    ("yesterday", "Yesterday"),
    ("today", "Today"),
    ("tomorrow", "Tomorrow"),
    ("last7Days", "In the last 7 days"),
    ("lastWeek", "Last week"),
    ("thisWeek", "This week"),
    ("nextWeek", "Next week"),
    ("lastMonth", "Last month"),
    ("thisMonth", "This month"),
    ("nextMonth", "Next month"),
];

const AVERAGES: [(&str, &str); 10] = [
    ("above", "above"),
    ("below", "below"),
    ("equalAbove", "equal or above"),
    ("equalBelow", "equal or below"),
    ("std1Above", "1 std dev above"),
    ("std1Below", "1 std dev below"),
    ("std2Above", "2 std dev above"),
    ("std2Below", "2 std dev below"),
    ("std3Above", "3 std dev above"),
    ("std3Below", "3 std dev below"),
];

/// Icon Style names, by the set's file name.
const ICON_SET_NAMES: [(&str, &str); 20] = [
    ("3Arrows", "3 Arrows (Colored)"),
    ("3ArrowsGray", "3 Arrows (Gray)"),
    ("3Flags", "3 Flags"),
    ("3TrafficLights1", "3 Traffic Lights (Unrimmed)"),
    ("3TrafficLights2", "3 Traffic Lights (Rimmed)"),
    ("3Signs", "3 Signs"),
    ("3Symbols", "3 Symbols (Circled)"),
    ("3Symbols2", "3 Symbols (Uncircled)"),
    ("3Stars", "3 Stars"),
    ("3Triangles", "3 Triangles"),
    ("4Arrows", "4 Arrows (Colored)"),
    ("4ArrowsGray", "4 Arrows (Gray)"),
    ("4RedToBlack", "Red To Black"),
    ("4Rating", "4 Ratings"),
    ("4TrafficLights", "4 Traffic Lights"),
    ("5Arrows", "5 Arrows (Colored)"),
    ("5ArrowsGray", "5 Arrows (Gray)"),
    ("5Rating", "5 Ratings"),
    ("5Quarters", "5 Quarters"),
    ("5Boxes", "5 Boxes"),
];

/// Excel's default colours: a 2-color scale runs orange to light yellow, a 3-color scale red,
/// yellow, green.
const TWO_COLOR: [&str; 2] = ["#FF7128", "#FFEF9C"];
const THREE_COLOR: [&str; 3] = ["#F8696B", "#FFEB84", "#63BE7B"];

/// The dialog's values before anything is chosen: Excel's defaults for every rule type.
pub(super) fn default_values() -> Map<String, Json> {
    let mut v = json!({
        "ruleType": "values",
        "formatStyle": "2color",
        "minType": "min", "minValue": "", "minColor": TWO_COLOR[0],
        "midType": "percentile", "midValue": "50", "midColor": THREE_COLOR[1],
        "maxType": "max", "maxValue": "", "maxColor": TWO_COLOR[1],
        "barColor": "#638EC6", "barFill": "gradient", "barOnly": false,
        "iconSet": "3TrafficLights1", "reverse": false, "iconOnly": false,
        "containKind": "cellValue", "operator": "between", "value": "", "value2": "",
        "textOp": "containsText", "text": "", "period": "yesterday",
        "topBottom": "top", "rank": 10, "percent": false,
        "avg": "above",
        "dupUnique": "duplicate",
        "formula": "",
    });
    set_icon_defaults(&mut v, "3TrafficLights1");
    v["style"] = serde_json::to_value(Style::default()).unwrap_or(Json::Null);
    v.as_object().cloned().unwrap_or_default()
}

/// Excel's thresholds for a newly chosen icon set: even percent steps, listed from the top icon
/// down as the dialog shows them (`67`, `33` for three icons).
fn set_icon_defaults(v: &mut Json, set: &str) {
    let k = gridcraft_engine::model::icon_count(set);
    let values: Vec<String> = (1..k).rev().map(|i| format!("{}", (i as f64 * 100.0 / k as f64).round())).collect();
    v["iconValues"] = json!(values);
    v["iconTypes"] = json!(vec!["percent"; k - 1]);
}

fn s<'a>(v: &'a Map<String, Json>, k: &str) -> &'a str {
    v.get(k).and_then(Json::as_str).unwrap_or("")
}

fn b(v: &Map<String, Json>, k: &str) -> bool {
    v.get(k).and_then(Json::as_bool).unwrap_or(false)
}

/// A text value as typed: numbers stay numbers in the rule JSON.
fn typed(v: &Map<String, Json>, k: &str) -> Json {
    match v.get(k) {
        Some(Json::String(t)) => json!(t),
        Some(Json::Number(n)) => json!(n),
        _ => json!(""),
    }
}

/// The rule JSON (`home.conditionalFormat`'s `rule`) the dialog's values describe.
pub(super) fn values_to_rule(v: &Map<String, Json>) -> Json {
    let stop = |p: &str| json!({"type": s(v, &format!("{p}Type")), "value": typed(v, &format!("{p}Value")), "color": s(v, &format!("{p}Color"))});
    let kind = |p: &str| json!({"type": s(v, &format!("{p}Type")), "value": typed(v, &format!("{p}Value"))});
    let mut rule = match s(v, "ruleType") {
        "values" => match s(v, "formatStyle") {
            "3color" => json!({"type": "colorScale", "stops": [stop("min"), stop("mid"), stop("max")]}),
            "dataBar" => json!({
                "type": "dataBar",
                "min": kind("min"),
                "max": kind("max"),
                "color": s(v, "barColor"),
                "gradient": s(v, "barFill") != "solid",
                "showValue": !b(v, "barOnly"),
            }),
            "iconSet" => {
                // The dialog lists thresholds from the top icon down; the rule takes them lowest first.
                let types = v.get("iconTypes").and_then(Json::as_array).cloned().unwrap_or_default();
                let values = v.get("iconValues").and_then(Json::as_array).cloned().unwrap_or_default();
                let thresholds: Vec<Json> = types.iter().zip(values.iter()).rev().map(|(t, x)| json!({"type": t, "value": x})).collect();
                json!({"type": "iconSet", "set": s(v, "iconSet"), "reverse": b(v, "reverse"), "showValue": !b(v, "iconOnly"), "thresholds": thresholds})
            }
            _ => json!({"type": "colorScale", "stops": [stop("min"), stop("max")]}),
        },
        "contain" => match s(v, "containKind") {
            "specificText" => json!({"type": s(v, "textOp"), "text": s(v, "text")}),
            "dates" => json!({"type": "timePeriod", "period": s(v, "period")}),
            k @ ("blanks" | "noBlanks" | "errors" | "noErrors") => json!({"type": k}),
            _ => {
                let mut r = json!({"type": "cellIs", "operator": s(v, "operator"), "value": typed(v, "value")});
                if matches!(s(v, "operator"), "between" | "notBetween") {
                    r["value2"] = typed(v, "value2");
                }
                r
            }
        },
        "rank" => json!({
            "type": "top10",
            "rank": v.get("rank").and_then(|r| r.as_f64().or_else(|| r.as_str()?.trim().parse().ok())).unwrap_or(10.0),
            "bottom": s(v, "topBottom") == "bottom",
            "percent": b(v, "percent"),
        }),
        "average" => {
            let a = s(v, "avg");
            let std_dev = a.strip_prefix("std").and_then(|x| x.get(..1)).and_then(|d| d.parse::<u8>().ok()).unwrap_or(0);
            json!({"type": "aboveAverage", "below": a.ends_with("elow"), "equal": a.starts_with("equal"), "stdDev": std_dev})
        }
        "unique" => json!({"type": if s(v, "dupUnique") == "unique" { "unique" } else { "duplicate" }}),
        _ => json!({"type": "expression", "formula": s(v, "formula")}),
    };
    if s(v, "ruleType") != "values" {
        rule["style"] = v.get("style").cloned().unwrap_or(Json::Null);
    }
    rule
}

/// A colour of the rule JSON as the dialog's `#RRGGBB`.
fn hex(c: &Json, theme: &Theme) -> String {
    let color = match c {
        Json::String(t) => Color::from_hex(t),
        Json::Object(o) => o.get("theme").and_then(Json::as_u64).map(|i| {
            let tint = o.get("tint").and_then(Json::as_f64).unwrap_or(0.0);
            Color::Theme(i.min(11) as u8, (tint * 1000.0).round() as i16)
        }),
        _ => None,
    };
    color.and_then(|c| c.hex(theme)).unwrap_or_else(|| "#000000".into())
}

/// A threshold's value as the dialog's text box shows it.
fn value_text(v: &Json) -> String {
    match v.get("value") {
        Some(Json::Number(n)) => {
            n.as_f64().map(|f| if f.fract() == 0.0 && f.abs() < 1e15 { format!("{}", f as i64) } else { f.to_string() }).unwrap_or_default()
        }
        Some(Json::String(t)) => t.clone(),
        _ => String::new(),
    }
}

/// The dialog's values for a stored rule (the JSON [`rule_params`] gives).
pub(super) fn rule_to_values(rule: &Json, theme: &Theme) -> Map<String, Json> {
    let mut v = Json::Object(default_values());
    let g = |k: &str| rule.get(k).cloned().unwrap_or(Json::Null);
    let gs = |k: &str| rule.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let gb = |k: &str| rule.get(k).and_then(Json::as_bool).unwrap_or(false);
    let ty = gs("type");
    if let Some(st) = rule.get("style") {
        v["style"] = st.clone();
    }
    match ty.as_str() {
        "colorScale" => {
            v["ruleType"] = json!("values");
            let stops = rule.get("stops").and_then(Json::as_array).cloned().unwrap_or_default();
            v["formatStyle"] = json!(if stops.len() == 3 { "3color" } else { "2color" });
            let prefixes: &[&str] = if stops.len() == 3 { &["min", "mid", "max"] } else { &["min", "max"] };
            for (p, st) in prefixes.iter().zip(&stops) {
                v[format!("{p}Type")] = st.get("type").cloned().unwrap_or(json!("num"));
                v[format!("{p}Value")] = json!(value_text(st));
                v[format!("{p}Color")] = json!(hex(st.get("color").unwrap_or(&Json::Null), theme));
            }
        }
        "dataBar" => {
            v["ruleType"] = json!("values");
            v["formatStyle"] = json!("dataBar");
            for p in ["min", "max"] {
                let k = g(p);
                v[format!("{p}Type")] = k.get("type").cloned().unwrap_or(json!(p));
                v[format!("{p}Value")] = json!(value_text(&k));
            }
            v["barColor"] = json!(hex(&g("color"), theme));
            v["barFill"] = json!(if rule.get("gradient").and_then(Json::as_bool) == Some(false) { "solid" } else { "gradient" });
            v["barOnly"] = json!(rule.get("showValue").and_then(Json::as_bool) == Some(false));
        }
        "iconSet" => {
            v["ruleType"] = json!("values");
            v["formatStyle"] = json!("iconSet");
            let set = gs("set");
            v["iconSet"] = json!(set);
            set_icon_defaults(&mut v, &set);
            let thresholds = rule.get("thresholds").and_then(Json::as_array).cloned().unwrap_or_default();
            if thresholds.len() == gridcraft_engine::model::icon_count(&set) {
                // Lowest first in the rule, the first being the lower bound; top icon first here.
                let shown: Vec<&Json> = thresholds.iter().skip(1).rev().collect();
                v["iconTypes"] = json!(shown.iter().map(|t| t.get("type").cloned().unwrap_or(json!("percent"))).collect::<Vec<_>>());
                v["iconValues"] = json!(shown.iter().map(|t| value_text(t)).collect::<Vec<_>>());
            }
            v["reverse"] = json!(gb("reverse"));
            v["iconOnly"] = json!(rule.get("showValue").and_then(Json::as_bool) == Some(false));
        }
        "cellIs" => {
            v["ruleType"] = json!("contain");
            v["containKind"] = json!("cellValue");
            v["operator"] = json!(gs("operator"));
            v["value"] = json!(gs("value"));
            v["value2"] = json!(gs("value2"));
        }
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => {
            v["ruleType"] = json!("contain");
            v["containKind"] = json!("specificText");
            v["textOp"] = json!(ty);
            v["text"] = json!(gs("text"));
        }
        "timePeriod" => {
            v["ruleType"] = json!("contain");
            v["containKind"] = json!("dates");
            v["period"] = json!(gs("period"));
        }
        "blanks" | "noBlanks" | "errors" | "noErrors" => {
            v["ruleType"] = json!("contain");
            v["containKind"] = json!(ty);
        }
        "top10" => {
            v["ruleType"] = json!("rank");
            v["topBottom"] = json!(if gb("bottom") { "bottom" } else { "top" });
            v["rank"] = g("rank");
            v["percent"] = json!(gb("percent"));
        }
        "aboveAverage" => {
            v["ruleType"] = json!("average");
            let (below, equal) = (gb("below"), gb("equal"));
            let std_dev = rule.get("stdDev").and_then(Json::as_u64).unwrap_or(0);
            v["avg"] = json!(match (std_dev, below, equal) {
                (1..=3, false, _) => format!("std{std_dev}Above"),
                (1..=3, true, _) => format!("std{std_dev}Below"),
                (_, false, false) => "above".into(),
                (_, true, false) => "below".into(),
                (_, false, true) => "equalAbove".into(),
                (_, true, true) => "equalBelow".into(),
            });
        }
        "duplicate" | "unique" => {
            v["ruleType"] = json!("unique");
            v["dupUnique"] = json!(ty);
        }
        _ => {
            v["ruleType"] = json!("formula");
            v["formula"] = json!(gs("formula"));
        }
    }
    v.as_object().cloned().unwrap_or_default()
}

/// Opens the rule editor: a new rule for the selection, or (from the Rules Manager) a new or
/// edited rule of the manager's list. `params`: `ruleType` (the type to start on), `manager`
/// (opened from the Rules Manager), `index` (the rule being edited there) and `rule` (its JSON).
pub(super) fn open(app: &SheetApp, params: &Json) -> Dialog {
    let editing = params.get("rule").is_some_and(|r| r.is_object());
    let theme = app.session.active().map(|d| d.wb.theme.clone()).unwrap_or_default();
    let values = match params.get("rule") {
        Some(r) if r.is_object() => rule_to_values(r, &theme),
        _ => default_values(),
    };
    let mut d = Dialog::custom("newFormattingRule", if editing { "Edit Formatting Rule" } else { "New Formatting Rule" }, Json::Object(values));
    if let Some(t) = params.get("ruleType").and_then(Json::as_str).filter(|t| RULE_TYPES.iter().any(|(k, _)| k == t)) {
        d.values.insert("ruleType".into(), json!(t));
    }
    if let Some(f) = params.get("formatStyle").and_then(Json::as_str).filter(|f| FORMAT_STYLES.iter().any(|(k, _)| k == f)) {
        d.values.insert("formatStyle".into(), json!(f));
        if f == "3color" {
            for (k, c) in [("minColor", THREE_COLOR[0]), ("midColor", THREE_COLOR[1]), ("maxColor", THREE_COLOR[2])] {
                d.values.insert(k.into(), json!(c));
            }
        }
    }
    let manager = params.get("manager").and_then(Json::as_bool).unwrap_or(false);
    d.values.insert("manager".into(), json!(manager));
    if let Some(i) = params.get("index").filter(|i| i.is_u64()) {
        d.values.insert("index".into(), i.clone());
    }
    // The rule goes on the selection the dialog opened on, as Excel's does, even when a formula
    // box then points at another sheet.
    let (sheet, applies_to) = app
        .session
        .active()
        .map(|doc| {
            (
                doc.wb.active().map(|sh| sh.name.clone()).unwrap_or_default(),
                doc.selection.ranges.iter().map(gridcraft_engine::core::RangeRef::a1).collect::<Vec<_>>().join(","),
            )
        })
        .unwrap_or_default();
    if !manager {
        d.command = Some("home.conditionalFormat");
        d.values.insert("sheet".into(), json!(sheet));
        d.values.insert("range".into(), json!(applies_to));
    }
    d.pick = Some(RefPick {
        key: "formula",
        sheet,
        caret: 0,
        point: None,
        anchor: gridcraft_engine::core::CellRef::new(0, 0),
        refocus: false,
        applies_to,
    });
    sync_pick(&mut d);
    d
}

/// Points the grid at the box that takes references for the chosen type: the formula, or the
/// first Cell Value box. Other types take none, so a click on the grid selects as usual.
fn sync_pick(d: &mut Dialog) {
    let want = match (s(&d.values, "ruleType"), s(&d.values, "containKind")) {
        ("formula", _) => Some("formula"),
        ("contain", "cellValue") => Some("value"),
        _ => None,
    };
    if let Some(pick) = d.pick.as_mut()
        && let Some(key) = want
        && pick.key != key
    {
        pick.key = key;
        pick.point = None;
        pick.caret = s(&d.values, key).chars().count();
    }
    d.pick_off = want.is_none();
}

/// `home.conditionalFormat`'s params for the dialog's OK: the rule, on the sheet and selection
/// it opened on.
pub(super) fn params(d: &Dialog) -> Json {
    let mut p = json!({"rule": values_to_rule(&d.values)});
    for k in ["sheet", "range"] {
        if let Some(v) = d.values.get(k).filter(|v| v.as_str().is_some_and(|t| !t.is_empty())) {
            p[k] = v.clone();
        }
    }
    p
}

fn combo(ui: &mut egui::Ui, d: &mut Dialog, lang: Language, key: &str, options: &[(&str, &str)], width: f32) -> bool {
    let cur = s(&d.values, key).to_string();
    let shown = options.iter().find(|(k, _)| *k == cur).map(|(_, l)| lang.tr(l)).unwrap_or("");
    let mut changed = false;
    craft_ui::ComboBox::from_id_salt(("cfRule", key)).selected_text(shown).width(width).show_ui(ui, |ui| {
        for (k, label) in options {
            if ui.selectable_label(cur == *k, lang.tr(label)).clicked() {
                d.values.insert(key.into(), json!(k));
                changed = true;
            }
        }
    });
    changed
}

fn text(ui: &mut egui::Ui, d: &mut Dialog, key: &str, width: f32, enabled: bool) -> egui::Response {
    let mut t = match d.values.get(key) {
        Some(Json::String(t)) => t.clone(),
        Some(Json::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let r = ui.add_enabled(enabled, egui::TextEdit::singleline(&mut t).desired_width(width));
    if r.changed() {
        d.values.insert(key.into(), json!(t));
    }
    if !d.pick_off
        && let Some(pick) = d.pick.as_mut().filter(|p| p.key == key)
    {
        super::show::track_pick(pick, &r, r.changed());
    }
    r
}

/// A colour box: the swatch, opening the colour palette.
fn color_box(ui: &mut egui::Ui, app: &SheetApp, d: &mut Dialog, key: &str) {
    let lang = crate::i18n::current(ui.ctx());
    let theme = app.session.active().map(|doc| doc.wb.theme.clone()).unwrap_or_default();
    let cur = Color::from_hex(s(&d.values, key)).and_then(|c| c.resolve(&theme)).map(crate::theme::color32).unwrap_or(Color32::BLACK);
    let r = ui.menu_button(egui::RichText::new("■■■■■").color(cur), |ui| {
        if let Some(c) = crate::widgets::color_palette(ui, &theme.colors, lang.tr("Automatic"))
            && c != "none"
        {
            d.values.insert(key.into(), json!(c));
            ui.close();
        }
    });
    r.response.on_hover_text(lang.tr("Color:"));
}

/// The rule editor's body.
pub(super) fn editor(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = app.ui.language;
    let theme = app.session.active().map(|doc| doc.wb.theme.clone()).unwrap_or_default();
    ui.label(egui::RichText::new(lang.tr("Select a Rule Type:")).strong());
    egui::Frame::new().stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)).inner_margin(4.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        for (k, label) in RULE_TYPES {
            let selected = s(&d.values, "ruleType") == k;
            if ui.selectable_label(selected, format!("► {}", lang.tr(label))).clicked() {
                d.values.insert("ruleType".into(), json!(k));
            }
        }
    });
    ui.add_space(6.0);
    ui.label(egui::RichText::new(lang.tr("Edit the Rule Description:")).strong());
    let ty = s(&d.values, "ruleType").to_string();
    egui::Frame::new().stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)).inner_margin(6.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        match ty.as_str() {
            "values" => values_section(app, ui, d, lang, &theme),
            "contain" => {
                ui.label(egui::RichText::new(lang.tr("Format only cells with:")).strong());
                ui.horizontal(|ui| {
                    combo(ui, d, lang, "containKind", &CONTAIN_KINDS, 110.0);
                    match s(&d.values, "containKind") {
                        "cellValue" => {
                            combo(ui, d, lang, "operator", &OPERATORS, 150.0);
                            text(ui, d, "value", 90.0, true);
                            if matches!(s(&d.values, "operator"), "between" | "notBetween") {
                                ui.label(lang.tr("Rule|and"));
                                text(ui, d, "value2", 90.0, true);
                            }
                        }
                        "specificText" => {
                            combo(ui, d, lang, "textOp", &TEXT_OPS, 130.0);
                            text(ui, d, "text", 150.0, true);
                        }
                        "dates" => {
                            combo(ui, d, lang, "period", &PERIODS, 150.0);
                        }
                        _ => {}
                    }
                });
            }
            "rank" => {
                ui.label(egui::RichText::new(lang.tr("Format values that rank in the:")).strong());
                ui.horizontal(|ui| {
                    combo(ui, d, lang, "topBottom", &[("top", "Rank|Top"), ("bottom", "Rank|Bottom")], 80.0);
                    let mut n = d.values.get("rank").and_then(|r| r.as_f64().or_else(|| r.as_str()?.parse().ok())).unwrap_or(10.0);
                    let max = if b(&d.values, "percent") { 100.0 } else { 1000.0 };
                    if ui.add(egui::DragValue::new(&mut n).range(1.0..=max).speed(0.2).max_decimals(0).update_while_editing(true)).changed() {
                        d.values.insert("rank".into(), json!(n.round()));
                    }
                    let mut pct = b(&d.values, "percent");
                    if ui.checkbox(&mut pct, lang.tr("% of the selected range")).changed() {
                        d.values.insert("percent".into(), json!(pct));
                    }
                });
            }
            "average" => {
                ui.label(egui::RichText::new(lang.tr("Format values that are:")).strong());
                ui.horizontal(|ui| {
                    combo(ui, d, lang, "avg", &AVERAGES, 150.0);
                    ui.label(lang.tr("the average for the selected range"));
                });
            }
            "unique" => {
                ui.label(egui::RichText::new(lang.tr("Format all:")).strong());
                ui.horizontal(|ui| {
                    combo(ui, d, lang, "dupUnique", &[("duplicate", "Values|duplicate"), ("unique", "Values|unique")], 100.0);
                    ui.label(lang.tr("values in the selected range"));
                });
            }
            _ => {
                ui.label(egui::RichText::new(lang.tr("Rule|Format values where this formula is true:")).strong());
                text(ui, d, "formula", ui.available_width().min(400.0), true);
            }
        }
        if ty != "values" {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(lang.tr("Preview:"));
                let st: Style = d.values.get("style").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
                style_preview(ui, &st, &theme, lang, vec2(220.0, 26.0));
                if ui.button(lang.tr("Format…")).clicked() {
                    app.open_dialog("formatCells", json!({"dxf": true, "style": d.values.get("style")}));
                }
            });
        }
    });
    sync_pick(d);
    super::show::ok_cancel(ui, confirm, open);
    if *confirm && b(&d.values, "manager") {
        // From the Rules Manager the rule goes back into its list; the manager's OK or Apply
        // writes the list.
        *confirm = false;
        match parse_rule(&values_to_rule(&d.values)) {
            Ok(rule) => {
                let index = d.values.get("index").and_then(Json::as_u64).map(|i| i as usize);
                let selection = app.session.active().map(|doc| doc.selection.ranges.clone()).unwrap_or_default();
                if let Some(back) = d.back.as_deref_mut()
                    && let Some(m) = back.cf.as_deref_mut()
                {
                    m.put(rule, index, selection);
                }
                *open = false;
            }
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e.to_string()))),
        }
    }
}

fn values_section(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, lang: Language, theme: &Theme) {
    ui.label(egui::RichText::new(lang.tr("Format all cells based on their values:")).strong());
    ui.horizontal(|ui| {
        ui.label(lang.tr("Format Style:"));
        let before = s(&d.values, "formatStyle").to_string();
        if combo(ui, d, lang, "formatStyle", &FORMAT_STYLES, 140.0) {
            // A scale's colours start from Excel's defaults for its number of stops.
            match (before.as_str(), s(&d.values, "formatStyle")) {
                (_, "2color") => {
                    d.values.insert("minColor".into(), json!(TWO_COLOR[0]));
                    d.values.insert("maxColor".into(), json!(TWO_COLOR[1]));
                }
                (_, "3color") => {
                    d.values.insert("minColor".into(), json!(THREE_COLOR[0]));
                    d.values.insert("midColor".into(), json!(THREE_COLOR[1]));
                    d.values.insert("maxColor".into(), json!(THREE_COLOR[2]));
                }
                _ => {}
            }
        }
    });
    let style = s(&d.values, "formatStyle").to_string();
    match style.as_str() {
        "iconSet" => icon_section(ui, d, lang),
        _ => {
            let bar = style == "dataBar";
            if bar {
                let mut only = b(&d.values, "barOnly");
                if ui.checkbox(&mut only, lang.tr("Show Bar Only")).changed() {
                    d.values.insert("barOnly".into(), json!(only));
                }
            }
            let stops: &[(&str, &str)] = if style == "3color" {
                &[("min", "Minimum"), ("mid", "Midpoint"), ("max", "Maximum")]
            } else {
                &[("min", "Minimum"), ("max", "Maximum")]
            };
            egui::Grid::new("cfStops").num_columns(stops.len() + 1).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                ui.label("");
                for (_, label) in stops {
                    ui.label(egui::RichText::new(lang.tr(label)).strong());
                }
                ui.end_row();
                ui.label(lang.tr("Type:"));
                for (p, _) in stops {
                    let options: Vec<(&str, &str)> = VALUE_TYPES
                        .iter()
                        .copied()
                        .filter(|(k, _)| match *p {
                            "min" => *k != "max",
                            "max" => *k != "min",
                            _ => !matches!(*k, "min" | "max"),
                        })
                        .collect();
                    combo(ui, d, lang, &format!("{p}Type"), &options, 110.0);
                }
                ui.end_row();
                ui.label(lang.tr("Value:"));
                for (p, _) in stops {
                    let fixed = matches!(s(&d.values, &format!("{p}Type")), "min" | "max");
                    text(ui, d, &format!("{p}Value"), 110.0, !fixed);
                }
                ui.end_row();
                if !bar {
                    ui.label(lang.tr("Color:"));
                    for (p, _) in stops {
                        color_box(ui, app, d, &format!("{p}Color"));
                    }
                    ui.end_row();
                }
            });
            if bar {
                ui.label(egui::RichText::new(lang.tr("Bar Appearance:")).strong());
                ui.horizontal(|ui| {
                    ui.label(lang.tr("Bar|Fill"));
                    combo(ui, d, lang, "barFill", &[("gradient", "Gradient Fill"), ("solid", "Solid Fill")], 150.0);
                    ui.label(lang.tr("Color:"));
                    color_box(ui, app, d, "barColor");
                });
            }
            ui.horizontal(|ui| {
                ui.label(lang.tr("Preview:"));
                if let Ok(rule) = parse_rule(&values_to_rule(&d.values)) {
                    rule_preview(ui, &rule, theme, lang, vec2(220.0, 22.0));
                }
            });
        }
    }
}

fn icon_section(ui: &mut egui::Ui, d: &mut Dialog, lang: Language) {
    ui.horizontal(|ui| {
        ui.label(lang.tr("Icon Style:"));
        let set = s(&d.values, "iconSet").to_string();
        let names: Vec<(&str, &str)> = ICON_SET_NAMES.iter().copied().filter(|(k, _)| ICON_SETS.contains(k)).collect();
        if combo(ui, d, lang, "iconSet", &names, 200.0) && s(&d.values, "iconSet") != set {
            let mut v = Json::Object(std::mem::take(&mut d.values));
            let new = v["iconSet"].as_str().unwrap_or("3TrafficLights1").to_string();
            set_icon_defaults(&mut v, &new);
            d.values = v.as_object().cloned().unwrap_or_default();
        }
    });
    ui.horizontal(|ui| {
        let mut rev = b(&d.values, "reverse");
        if ui.checkbox(&mut rev, lang.tr("Reverse Icon Order")).changed() {
            d.values.insert("reverse".into(), json!(rev));
        }
        let mut only = b(&d.values, "iconOnly");
        if ui.checkbox(&mut only, lang.tr("Show Icon Only")).changed() {
            d.values.insert("iconOnly".into(), json!(only));
        }
    });
    ui.label(egui::RichText::new(lang.tr("Display each icon according to these rules:")).strong());
    let set = s(&d.values, "iconSet").to_string();
    let k = gridcraft_engine::model::icon_count(&set);
    let reverse = b(&d.values, "reverse");
    let mut types: Vec<Json> = d.values.get("iconTypes").and_then(Json::as_array).cloned().unwrap_or_default();
    let mut values: Vec<Json> = d.values.get("iconValues").and_then(Json::as_array).cloned().unwrap_or_default();
    types.resize(k - 1, json!("percent"));
    values.resize(k - 1, json!(""));
    let options: Vec<(&str, &str)> = VALUE_TYPES.iter().copied().filter(|(t, _)| !matches!(*t, "min" | "max")).collect();
    egui::Grid::new("cfIcons").num_columns(5).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
        for row in 0..k {
            // The top row shows the highest icon (the lowest when reversed).
            let icon = if reverse { row } else { k - 1 - row };
            let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
            crate::grid::paint_cf_icon(ui.painter(), rect, &set, icon);
            let prev = if row == 0 { None } else { values.get(row - 1).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())) };
            match prev {
                Some(p) if row + 1 == k => {
                    ui.label(lang.tr("when < {value}").replace("{value}", &p));
                }
                Some(p) => {
                    ui.label(lang.tr("when < {value} and").replace("{value}", &p));
                }
                None => {
                    ui.label(lang.tr("when value is"));
                }
            }
            if row + 1 < k {
                ui.label(">=");
                let mut t = values.get(row).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut t).desired_width(70.0)).changed()
                    && let Some(slot) = values.get_mut(row)
                {
                    *slot = json!(t);
                }
                let cur = types.get(row).and_then(Json::as_str).unwrap_or("percent").to_string();
                let shown = options.iter().find(|(k, _)| *k == cur).map(|(_, l)| lang.tr(l)).unwrap_or("");
                craft_ui::ComboBox::from_id_salt(("cfIconType", row)).selected_text(shown).width(100.0).show_ui(ui, |ui| {
                    for (k, label) in &options {
                        if ui.selectable_label(cur == *k, lang.tr(label)).clicked()
                            && let Some(slot) = types.get_mut(row)
                        {
                            *slot = json!(k);
                        }
                    }
                });
            }
            ui.end_row();
        }
    });
    d.values.insert("iconTypes".into(), json!(types));
    d.values.insert("iconValues".into(), json!(values));
}

/// Whether a format sets nothing: Excel's preview then reads "No Format Set".
pub(crate) fn no_format(st: &Style) -> bool {
    *st == Style::default()
}

/// The preview Excel draws for a rule's format: sample text in its font, fill and borders.
pub(crate) fn style_preview(ui: &mut egui::Ui, st: &Style, theme: &Theme, lang: Language, size: egui::Vec2) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let t = crate::theme::Tokens::get(ui.ctx());
    let fill =
        if st.fill.pattern == gridcraft_engine::model::PatternType::None { None } else { st.fill.fg.resolve(theme).map(crate::theme::color32) };
    p.rect_filled(rect, 0.0, fill.unwrap_or(Color32::WHITE));
    p.rect_stroke(rect, 0.0, Stroke::new(1.0, t.text_dim), StrokeKind::Inside);
    let side = |line: &gridcraft_engine::model::BorderLine| {
        (line.style != BorderStyle::None).then(|| Stroke::new(1.5, line.color.resolve(theme).map(crate::theme::color32).unwrap_or(Color32::BLACK)))
    };
    let r2 = rect.shrink(2.0);
    for (line, a, b2) in [
        (&st.borders.top, r2.left_top(), r2.right_top()),
        (&st.borders.bottom, r2.left_bottom(), r2.right_bottom()),
        (&st.borders.left, r2.left_top(), r2.left_bottom()),
        (&st.borders.right, r2.right_top(), r2.right_bottom()),
    ] {
        if let Some(stroke) = side(line) {
            p.line_segment([a, b2], stroke);
        }
    }
    if no_format(st) {
        p.text(rect.center(), egui::Align2::CENTER_CENTER, lang.tr("No Format Set"), crate::theme::ui_font(11.0), Color32::from_gray(0x40));
        return r;
    }
    let ink = st.font.color.resolve(theme).map(crate::theme::color32).unwrap_or(Color32::BLACK);
    let mut fmt = egui::TextFormat { font_id: crate::theme::ui_font(12.0), color: ink, italics: st.font.italic, ..Default::default() };
    if st.font.underline != Underline::None {
        fmt.underline = Stroke::new(1.0, ink);
    }
    if st.font.strike {
        fmt.strikethrough = Stroke::new(1.0, ink);
    }
    let sample = lang.tr("AaBbCcYyZz");
    let mut job = egui::text::LayoutJob::default();
    job.append(sample, 0.0, fmt);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let at = rect.center() - galley.size() / 2.0;
    p.galley(at, galley.clone(), ink);
    if st.font.bold {
        // A second pass a pixel over stands in for a bold face the UI font may not have.
        p.galley(at + vec2(0.6, 0.0), galley, ink);
    }
    r
}

/// The preview of a rule: its format, or the scale, bar or icons it draws.
pub(crate) fn rule_preview(ui: &mut egui::Ui, rule: &CfRule, theme: &Theme, lang: Language, size: egui::Vec2) -> egui::Response {
    let rgb = |c: &Color| c.resolve(theme).map(crate::theme::color32).unwrap_or(Color32::GRAY);
    match rule {
        CfRule::ColorScale { stops } => {
            let (rect, r) = ui.allocate_exact_size(size, egui::Sense::hover());
            let colors: Vec<Color32> = stops.iter().map(|(_, c)| rgb(c)).collect();
            let n = 24;
            for i in 0..n {
                let t = i as f32 / (n - 1) as f32;
                let seg = t * (colors.len().saturating_sub(1)) as f32;
                let k = (seg.floor() as usize).min(colors.len().saturating_sub(2));
                let (a, b2) = (colors.get(k).copied().unwrap_or(Color32::GRAY), colors.get(k + 1).copied().unwrap_or(Color32::GRAY));
                let c = a.lerp_to_gamma(b2, seg - k as f32);
                let x0 = rect.left() + rect.width() * i as f32 / n as f32;
                let x1 = rect.left() + rect.width() * (i + 1) as f32 / n as f32;
                ui.painter().rect_filled(egui::Rect::from_x_y_ranges(x0..=x1 + 0.5, rect.y_range()), 0.0, c);
            }
            r
        }
        CfRule::DataBar { color, gradient, .. } => {
            let (rect, r) = ui.allocate_exact_size(size, egui::Sense::hover());
            let c = rgb(color);
            let bar = egui::Rect::from_min_size(rect.min + vec2(1.0, 2.0), vec2(rect.width() * 0.7, rect.height() - 4.0));
            if *gradient {
                let n = 16;
                for i in 0..n {
                    let t = i as f32 / n as f32;
                    let x0 = bar.left() + bar.width() * t;
                    let x1 = bar.left() + bar.width() * (i + 1) as f32 / n as f32;
                    ui.painter().rect_filled(
                        egui::Rect::from_x_y_ranges(x0..=x1 + 0.5, bar.y_range()),
                        0.0,
                        c.lerp_to_gamma(Color32::WHITE, t * 0.9),
                    );
                }
                ui.painter().rect_stroke(bar, 0.0, Stroke::new(1.0, c), StrokeKind::Inside);
            } else {
                ui.painter().rect_filled(bar, 0.0, c);
            }
            r
        }
        CfRule::IconSet { set, reverse, .. } => {
            let k = gridcraft_engine::model::icon_count(set);
            let (rect, r) = ui.allocate_exact_size(vec2(size.y * k as f32 + 4.0, size.y), egui::Sense::hover());
            for i in 0..k {
                let icon = if *reverse { k - 1 - i } else { i };
                let cell = egui::Rect::from_min_size(rect.min + vec2(i as f32 * size.y, 0.0), vec2(size.y, size.y));
                crate::grid::paint_cf_icon(ui.painter(), cell.shrink(2.0), set, icon);
            }
            r
        }
        _ => style_preview(ui, rule.style().unwrap_or(&Style::default()), theme, lang, size),
    }
}

/// A comparison value as the description quotes it: the rule's own text (`5000`, `"東"`, `=$A$1`).
fn shown_value(params: &Json, k: &str) -> String {
    params.get(k).and_then(Json::as_str).unwrap_or("").to_string()
}

/// The rule as Excel's Rules Manager describes it ("Cell Value > 5000", "Formula: =$C2>100").
pub(crate) fn describe(rule: &CfRule, lang: Language) -> String {
    let p = rule_params(rule);
    let a = shown_value(&p, "value");
    let b2 = shown_value(&p, "value2");
    let with = |key: &str, pairs: &[(&str, &str)]| {
        let mut t = lang.tr(key).to_string();
        for (k, v) in pairs {
            t = t.replace(k, v);
        }
        t
    };
    match rule {
        CfRule::CellIs { op, .. } => match op {
            CfOperator::Between => with("Cell Value between {a} and {b}", &[("{a}", &a), ("{b}", &b2)]),
            CfOperator::NotBetween => with("Cell Value not between {a} and {b}", &[("{a}", &a), ("{b}", &b2)]),
            op => {
                let sym = match op {
                    CfOperator::Equal => "=",
                    CfOperator::NotEqual => "<>",
                    CfOperator::Greater => ">",
                    CfOperator::Less => "<",
                    CfOperator::GreaterOrEqual => ">=",
                    _ => "<=",
                };
                with("Cell Value {op} {a}", &[("{op}", sym), ("{a}", &a)])
            }
        },
        CfRule::Expression { formula, .. } => with("Formula: ={f}", &[("{f}", formula)]),
        CfRule::ContainsText { text, .. } => with("Cell Value contains \"{t}\"", &[("{t}", text)]),
        CfRule::NotContainsText { text, .. } => with("Cell Value does not contain \"{t}\"", &[("{t}", text)]),
        CfRule::BeginsWith { text, .. } => with("Cell Value begins with \"{t}\"", &[("{t}", text)]),
        CfRule::EndsWith { text, .. } => with("Cell Value ends with \"{t}\"", &[("{t}", text)]),
        CfRule::Blanks { .. } => lang.tr("Rule|Blanks").into(),
        CfRule::NoBlanks { .. } => lang.tr("Rule|No Blanks").into(),
        CfRule::Errors { .. } => lang.tr("Rule|Errors").into(),
        CfRule::NoErrors { .. } => lang.tr("Rule|No Errors").into(),
        CfRule::Duplicate { .. } => lang.tr("Duplicate Values").into(),
        CfRule::Unique { .. } => lang.tr("Unique Values").into(),
        CfRule::Top10 { bottom, percent, rank, .. } => {
            let key = match (bottom, percent) {
                (false, false) => "Top {n}",
                (false, true) => "Top {n}%",
                (true, false) => "Bottom {n}",
                (true, true) => "Bottom {n}%",
            };
            with(key, &[("{n}", &rank.to_string())])
        }
        CfRule::AboveAverage { below, equal, std_dev, .. } => match (std_dev, below, equal) {
            (1..=3, false, _) => with("{n} std dev above Average", &[("{n}", &std_dev.to_string())]),
            (1..=3, true, _) => with("{n} std dev below Average", &[("{n}", &std_dev.to_string())]),
            (_, false, false) => lang.tr("Above Average").into(),
            (_, true, false) => lang.tr("Below Average").into(),
            (_, false, true) => lang.tr("Equal to or Above Average").into(),
            (_, true, true) => lang.tr("Equal to or Below Average").into(),
        },
        CfRule::TimePeriod { period, .. } => {
            let name = PERIODS.iter().find(|(k, _)| k == period).map(|(_, l)| lang.tr(l)).unwrap_or(period);
            with("Dates Occurring: {p}", &[("{p}", name)])
        }
        CfRule::ColorScale { .. } => lang.tr("Graded Color Scale").into(),
        CfRule::DataBar { .. } => lang.tr("Data Bar").into(),
        CfRule::IconSet { .. } => lang.tr("Icon Set").into(),
    }
}

/// A rule's Applies To as the Rules Manager shows it: `=$A$1:$A$10,$C$2`.
pub(crate) fn applies_to_text(cf: &CondFormat) -> String {
    format!("={}", cf.ranges.iter().map(super::abs).collect::<Vec<_>>().join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::model::CfValueKind;

    fn roundtrip(rule: Json) {
        let parsed = parse_rule(&rule).unwrap();
        let values = rule_to_values(&rule_params(&parsed), &Theme::default());
        let again = parse_rule(&values_to_rule(&values)).unwrap();
        assert_eq!(parsed, again, "{rule}");
    }

    #[test]
    fn every_rule_type_fills_the_editor_and_comes_back_the_same() {
        let style = json!({"font": {"bold": true, "color": "#9C0006"}, "fill": "#FFC7CE"});
        for mut rule in [
            json!({"type": "cellIs", "operator": "greaterThan", "value": 5000}),
            json!({"type": "cellIs", "operator": "notBetween", "value": 100, "value2": "=$B$1"}),
            json!({"type": "cellIs", "operator": "equal", "value": "東"}),
            json!({"type": "expression", "formula": "=$C2>100"}),
            json!({"type": "containsText", "text": "東"}),
            json!({"type": "endsWith", "text": "Z"}),
            json!({"type": "noBlanks"}),
            json!({"type": "errors"}),
            json!({"type": "unique"}),
            json!({"type": "top10", "rank": 15, "bottom": true}),
            json!({"type": "top10", "rank": 20, "percent": true}),
            json!({"type": "aboveAverage", "below": true, "equal": true}),
            json!({"type": "aboveAverage", "stdDev": 3, "below": true}),
            json!({"type": "timePeriod", "period": "nextMonth"}),
        ] {
            rule["style"] = style.clone();
            roundtrip(rule);
        }
        roundtrip(json!({"type": "colorScale", "stops": [{"type": "min", "color": "#FF0000"}, {"type": "max", "color": "#00FF00"}]}));
        roundtrip(
            json!({"type": "colorScale", "stops": [{"type": "num", "value": 1, "color": "#FF0000"}, {"type": "percentile", "value": 40, "color": "#FFFFFF"}, {"type": "formula", "value": "=$A$1", "color": "#00FF00"}]}),
        );
        roundtrip(
            json!({"type": "dataBar", "min": {"type": "num", "value": 0}, "max": {"type": "percent", "value": 90}, "color": "#63BE7B", "gradient": false, "showValue": false}),
        );
        roundtrip(
            json!({"type": "iconSet", "set": "4Rating", "thresholds": [{"type": "num", "value": 10}, {"type": "percent", "value": 50}, {"type": "percentile", "value": 90}], "reverse": true, "showValue": false}),
        );
    }

    #[test]
    fn the_editor_defaults_make_excels_default_rules() {
        let v = default_values();
        let r = parse_rule(&values_to_rule(&v)).unwrap();
        assert!(matches!(r, CfRule::ColorScale { ref stops } if stops.len() == 2 && stops[0].0 == CfValueKind::Min));
        let mut v = default_values();
        v.insert("formatStyle".into(), json!("iconSet"));
        let CfRule::IconSet { thresholds, .. } = parse_rule(&values_to_rule(&v)).unwrap() else { panic!() };
        assert_eq!(thresholds, [CfValueKind::Percent(0.0), CfValueKind::Percent(33.0), CfValueKind::Percent(67.0)]);
        let mut v = default_values();
        v.insert("ruleType".into(), json!("average"));
        v.insert("avg".into(), json!("std2Below"));
        assert!(matches!(parse_rule(&values_to_rule(&v)).unwrap(), CfRule::AboveAverage { below: true, equal: false, std_dev: 2, .. }));
    }

    #[test]
    fn descriptions_read_as_excels_in_japanese() {
        let ja = Language::Ja;
        let d = |r: Json| describe(&parse_rule(&r).unwrap(), ja);
        assert_eq!(d(json!({"type": "cellIs", "operator": "greaterThan", "value": 5000})), "セルの値 > 5000");
        assert_eq!(d(json!({"type": "cellIs", "operator": "between", "value": 100, "value2": 200})), "セルの値が 100 と 200 の間");
        assert_eq!(d(json!({"type": "top10", "rank": 10})), "上位 10 個");
        assert_eq!(d(json!({"type": "aboveAverage"})), "平均より上");
        assert_eq!(d(json!({"type": "expression", "formula": "=$C2>100"})), "数式: =$C2>100");
        assert_eq!(d(json!({"type": "duplicate"})), "重複する値");
        assert_eq!(d(json!({"type": "colorScale"})), "グラデーション カラー スケール");
        assert_eq!(d(json!({"type": "dataBar"})), "データ バー");
        assert_eq!(d(json!({"type": "iconSet"})), "アイコン セット");
        assert_eq!(d(json!({"type": "containsText", "text": "東"})), "特定の文字列を含む \"東\"");
        assert_eq!(describe(&parse_rule(&json!({"type": "top10", "rank": 5, "bottom": true, "percent": true})).unwrap(), Language::En), "Bottom 5%");
    }
}
