//! PivotTable dialogs as Excel shows them: ラベル フィルター, 値フィルター, トップテン フィルター
//! (from the ▼ menu of 行ラベル / 列ラベル), フィールドの設定, スライサーの挿入 and
//! ピボットテーブルのデータ ソースの変更. Each runs one `pivot.*` command on OK.

use serde_json::{Value as Json, json};

use super::Dialog;
use super::show::ok_cancel;
use crate::SheetApp;

/// The dialogs this module draws.
pub(super) fn handles(name: &str) -> bool {
    matches!(name, "pivotLabelFilter" | "pivotValueFilter" | "pivotTop10" | "pivotFieldSettings" | "insertSlicer" | "pivotChangeSource")
}

/// Opens one of [`handles`]' dialogs with its title (the field's name in brackets, as Excel's).
pub(super) fn open(app: &SheetApp, name: &str, params: &Json) -> Option<Dialog> {
    let lang = app.ui.language;
    let field = params.get("field").and_then(Json::as_str).unwrap_or_default();
    let title = match name {
        "pivotLabelFilter" => format!("{} ({field})", lang.tr("Label Filter")),
        "pivotValueFilter" => format!("{} ({field})", lang.tr("Value Filter")),
        "pivotTop10" => format!("{} ({field})", lang.tr("Top 10 Filter")),
        "pivotFieldSettings" => lang.tr("Field Settings").to_string(),
        "insertSlicer" => lang.tr("Insert Slicers").to_string(),
        _ => lang.tr("Change PivotTable Data Source").to_string(),
    };
    let mut values = params.clone();
    // The filter already on the field fills the dialog when its condition is the one chosen.
    if let Some(cur) = params.get("current").filter(|c| c.is_object())
        && cur["op"] == params["op"]
    {
        for k in ["value", "value2", "valueField", "bottom", "by"] {
            if !cur[k].is_null() {
                values[k] = cur[k].clone();
            }
        }
    }
    if name == "pivotChangeSource"
        && let Some(src) = app.session.active().and_then(|d| {
            d.wb.sheets.iter().flat_map(|s| s.pivots.iter()).find(|p| Some(p.name.as_str()) == params["pivot"].as_str()).map(|p| p.source.clone())
        })
    {
        values["source"] = json!(src);
    }
    Some(Dialog::custom(name, &title, values))
}

fn text(d: &Dialog, k: &str) -> String {
    match d.values.get(k) {
        Some(Json::String(s)) => s.clone(),
        Some(Json::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Label Filter conditions as the dialog's list shows them.
const LABEL_OPS: &[(&str, &str)] = &[
    ("equal", "PivotCond|equals"),
    ("notEqual", "PivotCond|does not equal"),
    ("beginsWith", "PivotCond|begins with"),
    ("notBeginsWith", "PivotCond|does not begin with"),
    ("endsWith", "PivotCond|ends with"),
    ("notEndsWith", "PivotCond|does not end with"),
    ("contains", "PivotCond|contains"),
    ("notContains", "PivotCond|does not contain"),
    ("greaterThan", "PivotCond|is greater than"),
    ("greaterThanOrEqual", "PivotCond|is greater than or equal to"),
    ("lessThan", "PivotCond|is less than"),
    ("lessThanOrEqual", "PivotCond|is less than or equal to"),
    ("between", "PivotCond|is between"),
    ("notBetween", "PivotCond|is not between"),
];

/// Draws one of the PivotTable dialogs.
pub(super) fn show(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let mut run: Option<(&'static str, Json)> = None;
    let pivot = d.values.get("pivot").cloned().unwrap_or(Json::Null);
    let field = d.values.get("field").cloned().unwrap_or(Json::Null);
    match d.name.as_str() {
        "pivotLabelFilter" | "pivotValueFilter" => {
            let label = d.name == "pivotLabelFilter";
            let ops: Vec<(&str, &str)> = if label {
                LABEL_OPS.to_vec()
            } else {
                LABEL_OPS.iter().copied().filter(|(o, _)| !o.contains("egins") && !o.contains("nds") && !o.contains("ontain")).collect()
            };
            let mut op = text(d, "op");
            let mut value = text(d, "value");
            let mut value2 = text(d, "value2");
            let mut vf = text(d, "valueField");
            let fields: Vec<String> = d
                .values
                .get("valueFields")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            if vf.is_empty() {
                vf = fields.first().cloned().unwrap_or_default();
            }
            ui.label(if label { lang.tr("Show items for which the label") } else { lang.tr("Show items for which") });
            ui.horizontal(|ui| {
                if !label {
                    craft_ui::ComboBox::from_id_salt("pivot_vf_field").width(120.0).selected_text(vf.clone()).show_ui(ui, |ui| {
                        for f in &fields {
                            ui.selectable_value(&mut vf, f.clone(), f.as_str());
                        }
                    });
                }
                let shown = ops.iter().find(|(o, _)| *o == op).map(|(_, l)| lang.tr(l)).unwrap_or_default();
                craft_ui::ComboBox::from_id_salt("pivot_filter_op").width(150.0).selected_text(shown).show_ui(ui, |ui| {
                    for (o, l) in &ops {
                        ui.selectable_value(&mut op, o.to_string(), lang.tr(l));
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut value).desired_width(90.0));
                if op.contains("etween") {
                    ui.label(lang.tr("PivotCond|and"));
                    ui.add(egui::TextEdit::singleline(&mut value2).desired_width(90.0));
                }
            });
            if label {
                ui.label(lang.tr("Use ? to represent any single character"));
                ui.label(lang.tr("Use * to represent any series of characters"));
            }
            for (k, v) in [("op", &op), ("value", &value), ("value2", &value2), ("valueField", &vf)] {
                d.values.insert(k.into(), json!(v));
            }
            ok_cancel(ui, confirm, open);
            if *confirm {
                let mut p = json!({"pivot": pivot, "field": field, "op": op, "value": value});
                if op.contains("etween") {
                    p["value2"] = json!(value2);
                }
                if !label {
                    p["valueField"] = json!(vf);
                }
                run = Some((if label { "pivot.labelFilter" } else { "pivot.valueFilter" }, p));
            }
        }
        "pivotTop10" => {
            let mut bottom = d.values.get("bottom").and_then(Json::as_bool).unwrap_or(false);
            let mut count = d.values.get("value").and_then(Json::as_f64).unwrap_or(10.0);
            let mut by = d.values.get("by").and_then(Json::as_str).unwrap_or("items").to_string();
            let fields: Vec<String> = d
                .values
                .get("valueFields")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let mut vf = text(d, "valueField");
            if vf.is_empty() {
                vf = fields.first().cloned().unwrap_or_default();
            }
            ui.label(lang.tr("Top 10|Show"));
            ui.horizontal(|ui| {
                let side = if bottom { lang.tr("Top 10|Bottom") } else { lang.tr("Top 10|Top") };
                craft_ui::ComboBox::from_id_salt("pivot_top_side").width(70.0).selected_text(side).show_ui(ui, |ui| {
                    ui.selectable_value(&mut bottom, false, lang.tr("Top 10|Top"));
                    ui.selectable_value(&mut bottom, true, lang.tr("Top 10|Bottom"));
                });
                ui.add(egui::DragValue::new(&mut count).range(1.0..=2_147_483_647.0));
                let unit = |b: &str| match b {
                    "percent" => lang.tr("Top 10|Percent"),
                    "sum" => lang.tr("Top 10|Sum"),
                    _ => lang.tr("Top 10|Items"),
                };
                craft_ui::ComboBox::from_id_salt("pivot_top_by").width(80.0).selected_text(unit(&by)).show_ui(ui, |ui| {
                    for b in ["items", "percent", "sum"] {
                        ui.selectable_value(&mut by, b.to_string(), unit(b));
                    }
                });
                ui.label(lang.tr("Top 10|by"));
                craft_ui::ComboBox::from_id_salt("pivot_top_field").width(120.0).selected_text(vf.clone()).show_ui(ui, |ui| {
                    for f in &fields {
                        ui.selectable_value(&mut vf, f.clone(), f.as_str());
                    }
                });
            });
            d.values.insert("bottom".into(), json!(bottom));
            d.values.insert("value".into(), json!(count));
            d.values.insert("by".into(), json!(by));
            d.values.insert("valueField".into(), json!(vf));
            ok_cancel(ui, confirm, open);
            if *confirm {
                run = Some((
                    "pivot.valueFilter",
                    json!({"pivot": pivot, "field": field, "op": "top10", "value": count, "order": if bottom { "bottom" } else { "top" }, "by": by, "valueField": vf}),
                ));
            }
        }
        "pivotFieldSettings" => {
            ui.label(format!("{} {}", lang.tr("Source Name:"), text(d, "sourceName")));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(lang.tr("Subtotals")).strong());
            let mut sub = text(d, "subtotals");
            ui.radio_value(&mut sub, "automatic".to_string(), lang.tr("PivotField|Automatic"));
            ui.radio_value(&mut sub, "none".to_string(), lang.tr("PivotField|None"));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(lang.tr("Layout & Print")).strong());
            let mut layout = text(d, "layout");
            let mut compact = layout == "compact";
            ui.radio_value(&mut layout, "outline".to_string(), lang.tr("Show item labels in outline form"));
            ui.add_enabled_ui(layout != "tabular", |ui| {
                if ui.checkbox(&mut compact, lang.tr("Display labels from the next field in the same column (compact form)")).changed() {
                    layout = if compact { "compact".into() } else { "outline".into() };
                }
            });
            if layout == "compact" && !compact {
                layout = "outline".into();
            }
            if ui.radio(layout == "tabular", lang.tr("Show item labels in tabular form")).clicked() {
                layout = "tabular".into();
            }
            let mut repeat = d.values.get("repeatLabels").and_then(Json::as_bool).unwrap_or(false);
            ui.checkbox(&mut repeat, lang.tr("Repeat item labels"));
            let mut blank = d.values.get("blankRows").and_then(Json::as_bool).unwrap_or(false);
            ui.checkbox(&mut blank, lang.tr("Insert blank line after each item label"));
            d.values.insert("subtotals".into(), json!(sub));
            d.values.insert("layout".into(), json!(layout));
            d.values.insert("repeatLabels".into(), json!(repeat));
            d.values.insert("blankRows".into(), json!(blank));
            ok_cancel(ui, confirm, open);
            if *confirm {
                run = Some((
                    "pivot.fieldSettings",
                    json!({"pivot": pivot, "field": field, "subtotals": sub, "layout": layout, "repeatLabels": repeat, "blankRows": blank}),
                ));
            }
        }
        "insertSlicer" => {
            let fields: Vec<String> = d
                .values
                .get("fields")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let mut chosen: Vec<String> = d
                .values
                .get("chosen")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                for f in &fields {
                    let mut on = chosen.contains(f);
                    if ui.checkbox(&mut on, f.as_str()).changed() {
                        if on {
                            chosen.push(f.clone());
                        } else {
                            chosen.retain(|x| x != f);
                        }
                    }
                }
            });
            d.values.insert("chosen".into(), json!(chosen));
            ok_cancel(ui, confirm, open);
            if *confirm {
                if chosen.is_empty() {
                    *confirm = false;
                    *open = false;
                    return;
                }
                // In the order the fields are listed, as Excel stacks them.
                let ordered: Vec<&String> = fields.iter().filter(|f| chosen.contains(f)).collect();
                run = Some(("pivot.insertSlicer", json!({"pivot": pivot, "fields": ordered})));
            }
        }
        _ => {
            ui.label(lang.tr("Choose the data that you want to analyze"));
            let mut src = text(d, "source");
            ui.horizontal(|ui| {
                ui.label(lang.tr("Table/Range:"));
                ui.add(egui::TextEdit::singleline(&mut src).desired_width(220.0));
            });
            d.values.insert("source".into(), json!(src));
            ok_cancel(ui, confirm, open);
            if *confirm {
                run = Some(("pivot.changeSource", json!({"pivot": pivot, "source": src.trim().trim_start_matches('=')})));
            }
        }
    }
    if let Some((id, p)) = run {
        *confirm = false;
        match app.run(id, p) {
            Ok(_) => *open = false,
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}
