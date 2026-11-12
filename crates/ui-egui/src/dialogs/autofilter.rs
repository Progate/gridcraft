//! The AutoFilter condition dialogs opened from a filter dropdown's Text, Number and Date Filters
//! submenus: Custom AutoFilter (two conditions joined by And or Or) and Top 10 AutoFilter.

use gridcraft_engine::cmd::data::{FilterColumnKind, filter_column_kind, filter_criterion_at, filter_values};
use gridcraft_engine::core::CellRef;
use gridcraft_engine::model::FilterCriterion;
use serde_json::{Value as Json, json};

use super::Dialog;
use super::show::ok_cancel;
use crate::SheetApp;

/// Operators of the Custom AutoFilter dialog for text and number columns: the `data.filterBy`
/// operator and the English label (translated through the catalog).
const TEXT_OPS: &[(&str, &str)] = &[
    ("=", "equals"),
    ("<>", "does not equal"),
    (">", "is greater than"),
    (">=", "is greater than or equal to"),
    ("<", "is less than"),
    ("<=", "is less than or equal to"),
    ("beginsWith", "begins with"),
    ("notBeginsWith", "does not begin with"),
    ("endsWith", "ends with"),
    ("notEndsWith", "does not end with"),
    ("contains", "contains"),
    ("notContains", "does not contain"),
];

/// Operators for a date column, as Excel words them there.
const DATE_OPS: &[(&str, &str)] = &[
    ("=", "equals"),
    ("<>", "does not equal"),
    (">", "is after"),
    (">=", "is after or equal to"),
    ("<", "is before"),
    ("<=", "is before or equal to"),
    ("beginsWith", "begins with"),
    ("notBeginsWith", "does not begin with"),
    ("endsWith", "ends with"),
    ("notEndsWith", "does not end with"),
    ("contains", "contains"),
    ("notContains", "does not contain"),
];

/// The column (sheet index) a dialog filters, from its `col` value.
fn column(d: &Dialog) -> Option<u32> {
    d.values.get("col").and_then(Json::as_u64).and_then(|c| u32::try_from(c).ok())
}

/// The header text of filter column `col`, which the dialog shows above the conditions.
fn header(app: &SheetApp, col: u32) -> String {
    let Some(doc) = app.session.active() else { return String::new() };
    let Some(sh) = doc.wb.active() else { return String::new() };
    let row = sh.autofilter.as_ref().map_or(0, |af| af.range.start.row);
    gridcraft_engine::display::cell_text(&doc.wb, sh, CellRef::new(row, col))
}

/// A stored condition as the dialog shows it: Excel's wildcards back to the text operator they
/// came from (`= abc*` is "begins with abc"), and a date column's serial number as a date.
pub(crate) fn shown_condition(op: &str, value: &str, kind: FilterColumnKind) -> (String, String) {
    let plain = |v: &str| !v.contains(['*', '?', '~']);
    let negate = op == "<>";
    let text_op = |pos: &str, neg: &str| if negate { neg.to_string() } else { pos.to_string() };
    if matches!(op, "=" | "<>")
        && value.len() > 2
        && let Some(inner) = value.strip_prefix('*').and_then(|v| v.strip_suffix('*')).filter(|v| plain(v))
    {
        return (text_op("contains", "notContains"), inner.to_string());
    }
    if matches!(op, "=" | "<>") && value.len() > 1 {
        if let Some(inner) = value.strip_suffix('*').filter(|v| plain(v)) {
            return (text_op("beginsWith", "notBeginsWith"), inner.to_string());
        }
        if let Some(inner) = value.strip_prefix('*').filter(|v| plain(v)) {
            return (text_op("endsWith", "notEndsWith"), inner.to_string());
        }
    }
    let value = match (kind, value.parse::<f64>()) {
        (FilterColumnKind::Date, Ok(n)) => gridcraft_engine::core::date::datetime_from_serial(gridcraft_engine::core::date::DateSystem::D1900, n)
            .map_or_else(|| value.to_string(), |d| format!("{}/{}/{}", d.year, d.month, d.day)),
        _ => value.to_string(),
    };
    (op.to_string(), value)
}

/// Opens Custom AutoFilter for `params.col`, starting from `params.op` (and `op2`) or from the
/// column's current custom filter.
pub(super) fn open_custom(app: &SheetApp, params: &Json) -> Option<Dialog> {
    let col = params.get("col").and_then(Json::as_u64).and_then(|c| u32::try_from(c).ok())?;
    let doc = app.session.active()?;
    let si = doc.wb.active_sheet();
    let kind = filter_column_kind(&doc.wb, si, col);
    let mut values = json!({"col": col, "op": "=", "value": "", "op2": "", "value2": "", "and": true});
    let explicit = params.get("op").is_some();
    if !explicit && let Some(FilterCriterion::Custom { a, b, and }) = filter_criterion_at(&doc.wb, si, col) {
        let (op, value) = shown_condition(&a.0, &a.1, kind);
        values["op"] = json!(op);
        values["value"] = json!(value);
        if let Some(b) = b {
            let (op2, value2) = shown_condition(&b.0, &b.1, kind);
            values["op2"] = json!(op2);
            values["value2"] = json!(value2);
        }
        values["and"] = json!(and);
    }
    for key in ["op", "op2", "and"] {
        if let Some(v) = params.get(key) {
            values[key] = v.clone();
        }
    }
    let mut d = Dialog::custom("customAutoFilter", "Custom AutoFilter", values);
    d.values.insert("kind".into(), json!(kind_name(kind)));
    Some(d)
}

fn kind_name(kind: FilterColumnKind) -> &'static str {
    match kind {
        FilterColumnKind::Text => "text",
        FilterColumnKind::Number => "number",
        FilterColumnKind::Date => "date",
    }
}

/// Opens Top 10 AutoFilter for `params.col`, starting from the column's Top 10 filter if it has one.
pub(super) fn open_top10(app: &SheetApp, params: &Json) -> Option<Dialog> {
    let col = params.get("col").and_then(Json::as_u64).and_then(|c| u32::try_from(c).ok())?;
    let doc = app.session.active()?;
    let (bottom, count, percent) = match filter_criterion_at(&doc.wb, doc.wb.active_sheet(), col) {
        Some(FilterCriterion::Top10 { bottom, percent, count, .. }) => (bottom, count, percent),
        _ => (false, 10, false),
    };
    Some(Dialog::custom("top10AutoFilter", "Top 10 AutoFilter", json!({"col": col, "bottom": bottom, "count": count, "percent": percent})))
}

/// An operator dropdown; `blank` adds the empty choice the second condition starts with.
fn op_combo(ui: &mut egui::Ui, id: &str, ops: &[(&str, &str)], value: &mut String, blank: bool) {
    let lang = crate::i18n::current(ui.ctx());
    let shown = ops.iter().find(|(op, _)| op == value).map_or("", |(_, label)| lang.tr(label));
    craft_ui::ComboBox::from_id_salt(id).width(170.0).selected_text(shown).show_ui(ui, |ui| {
        if blank {
            ui.selectable_value(value, String::new(), " ");
        }
        for (op, label) in ops {
            ui.selectable_value(value, (*op).to_string(), lang.tr(label));
        }
    });
}

/// A value box with the column's values in a dropdown beside it, like Excel's combo box.
fn value_combo(ui: &mut egui::Ui, id: &str, items: &[String], value: &mut String) {
    ui.add(egui::TextEdit::singleline(value).desired_width(150.0));
    craft_ui::ComboBox::from_id_salt(id).width(18.0).selected_text("").show_ui(ui, |ui| {
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for item in items {
                if ui.selectable_label(value == item, item).clicked() {
                    value.clone_from(item);
                }
            }
        });
    });
}

/// Draws Custom AutoFilter and runs `data.filterBy` on OK.
pub(super) fn custom_autofilter(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let Some(col) = column(d) else {
        *open = false;
        return;
    };
    let ops = if d.values.get("kind").and_then(Json::as_str) == Some("date") { DATE_OPS } else { TEXT_OPS };
    let items: Vec<String> = app
        .session
        .active()
        .map(|doc| filter_values(&doc.wb, doc.wb.active_sheet(), col).into_iter().map(|(t, _)| t).filter(|t| t != "(Blanks)").collect())
        .unwrap_or_default();
    let get = |d: &Dialog, k: &str| d.values.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let (mut op, mut value, mut op2, mut value2) = (get(d, "op"), get(d, "value"), get(d, "op2"), get(d, "value2"));
    let mut and = d.values.get("and").and_then(Json::as_bool).unwrap_or(true);
    ui.label(lang.tr("Show rows where:"));
    ui.add_space(4.0);
    ui.label(egui::RichText::new(header(app, col)).strong());
    ui.horizontal(|ui| {
        op_combo(ui, "af_op1", ops, &mut op, false);
        value_combo(ui, "af_val1", &items, &mut value);
    });
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.radio_value(&mut and, true, lang.tr("Custom AutoFilter|And"));
        ui.radio_value(&mut and, false, lang.tr("Custom AutoFilter|Or"));
    });
    ui.horizontal(|ui| {
        op_combo(ui, "af_op2", ops, &mut op2, true);
        value_combo(ui, "af_val2", &items, &mut value2);
    });
    ui.add_space(6.0);
    ui.label(lang.tr("Use ? to represent any single character"));
    ui.label(lang.tr("Use * to represent any series of characters"));
    d.values.insert("op".into(), json!(op));
    d.values.insert("value".into(), json!(value));
    d.values.insert("op2".into(), json!(op2));
    d.values.insert("value2".into(), json!(value2));
    d.values.insert("and".into(), json!(and));
    ok_cancel(ui, confirm, open);
    if *confirm {
        *confirm = false;
        match app.run("data.filterBy", custom_params(col, &op, &value, &op2, &value2, and)) {
            Ok(_) => *open = false,
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}

/// The `data.filterBy` parameters for the dialog's conditions; a second condition without an
/// operator is left out, as Excel does.
pub(crate) fn custom_params(col: u32, op: &str, value: &str, op2: &str, value2: &str, and: bool) -> Json {
    let mut custom = json!({"op": op, "value": value, "and": and});
    if !op2.is_empty() {
        custom["op2"] = json!(op2);
        custom["value2"] = json!(value2);
    }
    json!({"column": gridcraft_engine::core::col_to_letters(col), "custom": custom})
}

/// Draws Top 10 AutoFilter (Show: Top/Bottom, a count, Items/Percent) and runs `data.filterBy` on OK.
pub(super) fn top10_autofilter(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let Some(col) = column(d) else {
        *open = false;
        return;
    };
    let mut bottom = d.values.get("bottom").and_then(Json::as_bool).unwrap_or(false);
    let mut percent = d.values.get("percent").and_then(Json::as_bool).unwrap_or(false);
    let mut count = d.values.get("count").and_then(Json::as_u64).unwrap_or(10);
    ui.label(lang.tr("Top 10|Show"));
    ui.horizontal(|ui| {
        let side = if bottom { lang.tr("Top 10|Bottom") } else { lang.tr("Top 10|Top") };
        craft_ui::ComboBox::from_id_salt("top10_side").width(80.0).selected_text(side).show_ui(ui, |ui| {
            ui.selectable_value(&mut bottom, false, lang.tr("Top 10|Top"));
            ui.selectable_value(&mut bottom, true, lang.tr("Top 10|Bottom"));
        });
        ui.add(egui::DragValue::new(&mut count).range(1..=if percent { 100 } else { 500 }));
        let unit = if percent { lang.tr("Top 10|Percent") } else { lang.tr("Top 10|Items") };
        craft_ui::ComboBox::from_id_salt("top10_unit").width(90.0).selected_text(unit).show_ui(ui, |ui| {
            ui.selectable_value(&mut percent, false, lang.tr("Top 10|Items"));
            ui.selectable_value(&mut percent, true, lang.tr("Top 10|Percent"));
        });
    });
    d.values.insert("bottom".into(), json!(bottom));
    d.values.insert("percent".into(), json!(percent));
    d.values.insert("count".into(), json!(count));
    ok_cancel(ui, confirm, open);
    if *confirm {
        *confirm = false;
        let p = json!({"column": gridcraft_engine::core::col_to_letters(col), "top": {"count": count, "bottom": bottom, "percent": percent}});
        match app.run("data.filterBy", p) {
            Ok(_) => *open = false,
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_wildcards_show_as_the_text_operator_they_came_from() {
        let t = FilterColumnKind::Text;
        let shown = |op, v| shown_condition(op, v, t);
        assert_eq!(shown("=", "abc*"), ("beginsWith".into(), "abc".into()));
        assert_eq!(shown("<>", "abc*"), ("notBeginsWith".into(), "abc".into()));
        assert_eq!(shown("=", "*abc"), ("endsWith".into(), "abc".into()));
        assert_eq!(shown("=", "*abc*"), ("contains".into(), "abc".into()));
        assert_eq!(shown("<>", "*abc*"), ("notContains".into(), "abc".into()));
        assert_eq!(shown("=", "a?c*"), ("=".into(), "a?c*".into()));
        assert_eq!(shown(">=", "100"), (">=".into(), "100".into()));
        assert_eq!(shown_condition(">", "46085", FilterColumnKind::Date), (">".into(), "2026/3/4".into()));
    }

    #[test]
    fn a_second_condition_without_an_operator_is_left_out() {
        assert_eq!(custom_params(1, ">", "5", "", "", true), json!({"column": "B", "custom": {"op": ">", "value": "5", "and": true}}));
        assert_eq!(
            custom_params(0, ">=", "1", "<=", "9", true),
            json!({"column": "A", "custom": {"op": ">=", "value": "1", "op2": "<=", "value2": "9", "and": true}})
        );
    }
}
