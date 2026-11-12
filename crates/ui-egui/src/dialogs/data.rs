#[cfg(test)]
use super::show::build_params;
use super::show::{form_fields, ok_cancel};
use super::{Dialog, Field, Icon, Json, SheetApp, json, vec2};

/// Initial values of the Data Validation form: the rule stored on the active cell, so OK without
/// edits keeps it; a plain list rule when the cell has none.
pub(super) fn validation_defaults(app: &SheetApp) -> Json {
    let mut v = json!({
        "type": "list",
        "operator": "between",
        "ignoreBlank": true,
        "dropdown": true,
        "showInputMessage": true,
        "showErrorMessage": true,
        "errorStyle": "stop"
    });
    let Some(doc) = app.session.active() else { return v };
    let Some(dv) = doc.wb.active().and_then(|s| s.validations.iter().find(|d| d.ranges.iter().any(|r| r.contains(doc.selection.active)))) else {
        return v;
    };
    // The enums serialize as their variant names; the command takes them in camelCase.
    let camel = |x: Json| {
        let name = x.as_str().unwrap_or_default();
        let mut chars = name.chars();
        chars.next().map(|c| c.to_lowercase().chain(chars).collect::<String>()).unwrap_or_default()
    };
    v["type"] = json!(camel(json!(dv.kind)));
    v["operator"] = json!(camel(json!(dv.op)));
    v["errorStyle"] = json!(camel(json!(dv.error_style)));
    let shown = |f: &str| bound_text(dv.kind, f, app.ui.language, doc.wb.date_system);
    v["formula1"] = json!(shown(&dv.f1));
    if let Some(f2) = &dv.f2 {
        v["formula2"] = json!(shown(f2));
    }
    v["ignoreBlank"] = json!(dv.allow_blank);
    v["dropdown"] = json!(dv.in_cell_dropdown);
    v["showInputMessage"] = json!(dv.show_input);
    v["showErrorMessage"] = json!(dv.show_error);
    v["inputTitle"] = json!(dv.input_title);
    v["inputMessage"] = json!(dv.input_message);
    v["errorTitle"] = json!(dv.error_title);
    v["errorMessage"] = json!(dv.error_message);
    v
}

/// A Date or Time rule's bound as the Data Validation dialog shows it: the serial number a file
/// stores (`46113`, `0.375`) as the date or time Excel shows (`2026/4/1` in Japanese, `4/1/2026`
/// otherwise; `9:00`), which OK reads back to the same serial. A formula is shown as one (`=TODAY()`),
/// a quoted list source as its items; other rules show the stored text.
fn bound_text(
    kind: gridcraft_engine::model::ValidationKind,
    f: &str,
    lang: crate::i18n::Language,
    sys: gridcraft_engine::core::DateSystem,
) -> String {
    use gridcraft_engine::model::ValidationKind;
    let date = kind == ValidationKind::Date;
    if kind == ValidationKind::List || kind == ValidationKind::Custom {
        // A literal list shows as typed (`a,b`), a source formula with its `=` (`=INDIRECT($D2)`):
        // shown bare, OK took `INDIRECT($D2)` for a one-item literal list.
        return match f.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
            Some(body) if kind == ValidationKind::List => body.replace("\"\"", "\""),
            _ if f.is_empty() || f.starts_with('=') => f.to_string(),
            _ => format!("={f}"),
        };
    }
    if !date && kind != ValidationKind::Time {
        return f.to_string();
    }
    let Ok(n) = f.trim().parse::<f64>() else {
        return if f.is_empty() { String::new() } else { format!("={f}") };
    };
    let day = if lang == crate::i18n::Language::Ja { "yyyy/m/d" } else { "m/d/yyyy" };
    let seconds = ((n * 86_400.0).round() as i64) % 60 != 0;
    let time = if seconds { "h:mm:ss" } else { "h:mm" };
    let code = match (date, n.fract() != 0.0, n >= 1.0) {
        (true, false, _) => day.to_string(),
        (true, true, _) | (false, _, true) => format!("{day} {time}"),
        (false, _, false) => time.to_string(),
    };
    gridcraft_engine::numfmt::text_function(&gridcraft_engine::core::Value::Number(n), &code, sys).unwrap_or_else(|_| f.to_string())
}

/// The range Remove Duplicates starts with. A block selection is used as given; with only the
/// cursor placed, the table's data rows or the current region, as the command picks when it gets
/// no range (a single-cell range would remove nothing).
pub(super) fn remove_duplicates_range(app: &SheetApp, sel: &str) -> String {
    let Some(d) = app.session.active() else { return sel.to_string() };
    let Some(sh) = d.wb.active() else { return sel.to_string() };
    if !d.selection.current().is_single() {
        return sel.to_string();
    }
    let range = match sh.table_at(d.selection.active) {
        Some(t) => t.data_range().map(|dr| gridcraft_engine::core::RangeRef::new(t.range.start, dr.end)).unwrap_or(t.range),
        None => gridcraft_engine::selection::current_region(sh, d.selection.active),
    };
    range.a1()
}

/// The columns Remove Duplicates lists for `range` on `sheet` (by name, else the active sheet):
/// (column letter, label). With "My data has headers" ticked a column is named by its header
/// text, as in Excel; otherwise, or when the header is blank, it is `Column A` (列 A).
pub(super) fn remove_duplicates_columns(app: &SheetApp, range: &str, header: bool) -> Vec<(String, String)> {
    use gridcraft_engine::core::{CellRef, RangeRef, col_to_letters, split_sheet};
    let lang = app.ui.language;
    let (sheet_name, a1) = split_sheet(range.trim());
    let Some(r) = RangeRef::parse(a1) else { return vec![] };
    let doc = app.session.active();
    let sheet = doc.and_then(|d| match &sheet_name {
        Some(name) => d.wb.sheet_index(name).and_then(|i| d.wb.sheet(i)),
        None => d.wb.active(),
    });
    // At most one column per sheet column: a whole-row range must not build 16,384 rows of check boxes forever.
    (r.start.col..=r.end.col.min(r.start.col.saturating_add(1023)))
        .map(|c| {
            let letter = col_to_letters(c);
            let text = match (header, doc, sheet) {
                (true, Some(d), Some(sh)) => gridcraft_engine::display::cell_text(&d.wb, sh, CellRef::new(r.start.row, c)),
                _ => String::new(),
            };
            let label = if text.trim().is_empty() { lang.tr("Column {col}").replace("{col}", &letter) } else { text };
            (letter, label)
        })
        .collect()
}

/// Remove Duplicates as Excel draws it: Select All / Unselect All, "My data has headers", and a
/// check box per column of the range. Unticked columns are kept in `unchecked` (letters), so every
/// column of a range typed later starts ticked.
pub(super) fn remove_duplicates_form(app: &SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = app.ui.language;
    d.fields = vec![Field::Text { key: "range", label: "Range:" }];
    form_fields(app, ui, d);
    let range = d.values.get("range").and_then(Json::as_str).unwrap_or("").to_string();
    let mut header = d.values.get("header").and_then(Json::as_bool).unwrap_or(true);
    let columns = remove_duplicates_columns(app, &range, header);
    let mut unchecked: Vec<String> = d
        .values
        .get("unchecked")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    ui.label(lang.tr("To delete duplicate values, select one or more columns that contain duplicates."));
    ui.horizontal(|ui| {
        if ui.button(lang.tr("Select All")).clicked() {
            unchecked.clear();
        }
        if ui.button(lang.tr("Unselect All")).clicked() {
            unchecked = columns.iter().map(|(letter, _)| letter.clone()).collect();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(&mut header, lang.tr("My data has headers"));
        });
    });
    ui.label(egui::RichText::new(lang.tr("Columns")).strong());
    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
        for (letter, label) in &columns {
            let mut on = !unchecked.contains(letter);
            if ui.checkbox(&mut on, label.as_str()).changed() {
                if on {
                    unchecked.retain(|x| x != letter);
                } else {
                    unchecked.push(letter.clone());
                }
            }
        }
    });
    d.values.insert("header".into(), json!(header));
    d.values.insert("unchecked".into(), json!(unchecked));
    let none = !columns.is_empty() && columns.iter().all(|(letter, _)| unchecked.contains(letter));
    let mut ok = false;
    ok_cancel(ui, &mut ok, open);
    // With no column ticked there is nothing to compare: OK does nothing, as Excel greys it out.
    *confirm |= ok && !none;
}

/// Excel's message after Remove Duplicates, from the command's `{removed, remaining}`.
pub(super) fn remove_duplicates_message(lang: crate::i18n::Language, r: &Json) -> String {
    let removed = r.get("removed").and_then(Json::as_u64).unwrap_or(0);
    let remaining = r.get("remaining").and_then(Json::as_u64).unwrap_or(0);
    if removed == 0 {
        return lang.tr("No duplicate values found.").to_string();
    }
    lang.tr("{removed} duplicate values found and removed; {remaining} unique values remain.")
        .replace("{removed}", &removed.to_string())
        .replace("{remaining}", &remaining.to_string())
}

/// Whether Remove Duplicates opens with "My data has headers" ticked: a table's own header row
/// setting inside a table, else ticked.
pub(super) fn remove_duplicates_header(app: &SheetApp) -> bool {
    app.session.active().and_then(|d| d.wb.active().and_then(|sh| sh.table_at(d.selection.active)).map(|t| t.header_row)).unwrap_or(true)
}

pub(super) fn abs(r: &gridcraft_engine::core::RangeRef) -> String {
    let a = |c: gridcraft_engine::core::CellRef| format!("${}${}", gridcraft_engine::core::col_to_letters(c.col), c.row + 1);
    if r.is_single() { a(r.start) } else { format!("{}:{}", a(r.start), a(r.end)) }
}

/// The criterion controls in Excel's Settings tab depend on both Allow and Data.
/// `formula1` is the sole source for a list or custom rule; `formula2` is a
/// second bound only for Between and Not between.
pub(super) fn data_validation_fields(kind: &str, operator: &str) -> Vec<Field> {
    use Field::*;
    let mut fields = vec![Choice {
        key: "type",
        label: "Allow",
        options: vec![
            ("any", "Any value"),
            ("whole", "Whole number"),
            ("decimal", "Decimal"),
            ("list", "List"),
            ("date", "Date"),
            ("time", "Time"),
            ("textLength", "Text length"),
            ("custom", "Custom"),
        ],
    }];
    let two_bounds = matches!(operator, "between" | "notBetween");
    match kind {
        "list" => {
            fields.push(Text { key: "formula1", label: "Source:" });
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
            fields.push(Check { key: "dropdown", label: "In-cell dropdown" });
        }
        "custom" => {
            fields.push(Text { key: "formula1", label: "Formula:" });
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
        }
        "whole" | "decimal" | "date" | "time" | "textLength" => {
            fields.push(Choice {
                key: "operator",
                label: "Data",
                options: vec![
                    ("between", "between"),
                    ("notBetween", "not between"),
                    ("equal", "equal to"),
                    ("notEqual", "not equal to"),
                    ("greater", "greater than"),
                    ("less", "less than"),
                    ("greaterOrEqual", "greater than or equal to"),
                    ("lessOrEqual", "less than or equal to"),
                ],
            });
            let lower = matches!(operator, "greater" | "greaterOrEqual");
            let (first, second) = match kind {
                "date" => (
                    if two_bounds || lower {
                        "Start date:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "End date:"
                    } else {
                        "Date:"
                    },
                    "End date:",
                ),
                "time" => (
                    if two_bounds || lower {
                        "Start time:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "End time:"
                    } else {
                        "Time:"
                    },
                    "End time:",
                ),
                _ => (
                    if two_bounds || lower {
                        "Minimum:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "Maximum:"
                    } else {
                        "Value:"
                    },
                    "Maximum:",
                ),
            };
            fields.push(Text { key: "formula1", label: first });
            if two_bounds {
                fields.push(Text { key: "formula2", label: second });
            }
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
        }
        _ => {}
    }
    // Excel's Input Message tab, then its Error Alert tab, in their order.
    fields.push(Check { key: "showInputMessage", label: "Show input message when cell is selected" });
    fields.push(Text { key: "inputTitle", label: "Input message title:" });
    fields.push(Text { key: "inputMessage", label: "Input message:" });
    fields.push(Check { key: "showErrorMessage", label: "Show error alert after invalid data is entered" });
    fields.push(Choice {
        key: "errorStyle",
        label: "Error style",
        options: vec![("stop", "Stop"), ("warning", "Warning"), ("information", "Information")],
    });
    fields.push(Text { key: "errorTitle", label: "Error alert title:" });
    fields.push(Text { key: "errorMessage", label: "Error message:" });
    fields
}

pub(super) fn data_validation_form(app: &SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let kind = d.values.get("type").and_then(Json::as_str).unwrap_or("any");
    let operator = d.values.get("operator").and_then(Json::as_str).unwrap_or("between");
    d.fields = data_validation_fields(kind, operator);
    form_fields(app, ui, d);
    ok_cancel(ui, confirm, open);
}

pub(super) fn advanced_filter_form(app: &SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    use Field::*;
    let copy = d.values.get("action").and_then(Json::as_str) == Some("copy");
    d.fields = vec![
        Choice {
            key: "action",
            label: "Action",
            options: vec![("inPlace", "Show matching rows in place"), ("copy", "Copy matching rows elsewhere")],
        },
        Text { key: "range", label: "List range:" },
        Text { key: "criteria", label: "Criteria range (optional):" },
    ];
    if copy {
        d.fields.push(Text { key: "copyTo", label: "Copy to:" });
    }
    d.fields.push(Check { key: "unique", label: "Unique rows only" });
    form_fields(app, ui, d);
    // Constrain the button row: the shared right-to-left layout otherwise
    // consumes the window's remaining height instead of fitting the buttons.
    ui.allocate_ui(vec2(ui.available_width(), ui.spacing().interact_size.y + 8.0), |ui| ok_cancel(ui, confirm, open));
}

/// Draws the sort-level editor: header toggle, column/by/order controls, add/remove level, and `data.sort` on confirmation.
pub(super) fn sort(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    let lang = app.ui.language;
    let mut header = d.values.get("header").and_then(Json::as_bool).unwrap_or(true);
    ui.checkbox(&mut header, lang.tr("My list has headers"));
    d.values.insert("header".into(), json!(header));
    // Japanese Excel's Sort Options ▸ "Use phonetic" (on by default): cells with furigana sort by
    // their reading. Only a Japanese workbook has the option.
    let japanese = app
        .session
        .active()
        .is_some_and(|d| gridcraft_engine::core::Collation::for_region(d.wb.region) == gridcraft_engine::core::Collation::Japanese);
    let mut phonetic = d.values.get("phonetic").and_then(Json::as_bool).unwrap_or(true);
    if japanese {
        ui.checkbox(&mut phonetic, lang.tr("Use phonetic"));
        d.values.insert("phonetic".into(), json!(phonetic));
    }
    let mut levels: Vec<Json> = d.values.get("levels").and_then(Json::as_array).cloned().unwrap_or_default();
    // Columns of the region around the active cell, labelled with their header (or first value).
    let columns: Vec<(String, String)> = app
        .session
        .active()
        .and_then(|d| {
            let sh = d.wb.active()?;
            let region = gridcraft_engine::selection::current_region(sh, d.selection.active);
            Some(
                (region.start.col..=region.end.col)
                    .map(|c| {
                        let letter = gridcraft_engine::core::col_to_letters(c);
                        // The region starts at the header row when one is present, so the
                        // label always comes from start.row: header text with headers on,
                        // first data value with them off.
                        let label_row = region.start.row;
                        let text = gridcraft_engine::display::cell_text(&d.wb, sh, gridcraft_engine::core::CellRef::new(label_row, c));
                        let text: String = text.chars().take(24).collect();
                        (letter.clone(), if text.is_empty() { letter } else { format!("{letter}: {text}") })
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();
    let mut remove = None;
    for (i, lv) in levels.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let label = ui.label(lang.tr(if i == 0 { "Sort by" } else { "Then by" }));
            let mut col = lv["column"].as_str().unwrap_or("A").to_string();
            if columns.is_empty() {
                ui.add(egui::TextEdit::singleline(&mut col).desired_width(50.0)).labelled_by(label.id);
            } else {
                // The closed box shows the same "B: Revenue" label as the list, not the bare letter (#3446).
                let shown =
                    columns.iter().find(|(letter, _)| letter.eq_ignore_ascii_case(&col)).map_or_else(|| col.clone(), |(_, label)| label.clone());
                craft_ui::ComboBox::from_id_salt(("sort_col", i))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (letter, label) in &columns {
                            ui.selectable_value(&mut col, letter.clone(), label.as_str());
                        }
                    })
                    .response
                    .labelled_by(label.id);
            }
            lv["column"] = json!(col.to_ascii_uppercase());
            let mut by = lv["by"].as_str().unwrap_or("values").to_string();
            craft_ui::ComboBox::from_id_salt(("by", i))
                .selected_text(match by.as_str() {
                    "cellColor" => "Cell Color",
                    "fontColor" => "Font Color",
                    _ => "Cell Values",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut by, "values".into(), "Cell Values");
                    ui.selectable_value(&mut by, "cellColor".into(), "Cell Color");
                    ui.selectable_value(&mut by, "fontColor".into(), "Font Color");
                })
                .response
                .accessible_name(lang.tr("Sort On"));
            lv["by"] = json!(by);
            let mut desc = lv["order"].as_str() == Some("desc");
            ui.radio_value(&mut desc, false, "A to Z");
            ui.radio_value(&mut desc, true, "Z to A");
            lv["order"] = json!(if desc { "desc" } else { "asc" });
            if crate::widgets::icon_button(ui, Icon::Close, crate::theme::Tokens::get(ui.ctx()).text_dim, "Remove level", vec2(18.0, 18.0)).clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove
        && levels.len() > 1
    {
        levels.remove(i);
    }
    if ui.button("+ Add Level").clicked() {
        let used: Vec<String> = levels.iter().filter_map(|l| l.get("column").and_then(Json::as_str).map(str::to_string)).collect();
        let next = columns
            .iter()
            .map(|(l, _)| l)
            .find(|l| !used.contains(l))
            .cloned()
            .or_else(|| columns.last().map(|(l, _)| l.clone()))
            .unwrap_or_else(|| "B".into());
        levels.push(json!({"column": next, "order": "asc"}));
    }
    d.values.insert("levels".into(), Json::Array(levels.clone()));
    let mut open = true;
    ok_cancel(ui, confirm, &mut open);
    if *confirm {
        app.run_or_alert("data.sort", json!({"header": header, "keys": levels, "phonetic": phonetic}));
    }
    if !open {
        *confirm = true;
    }
}

#[cfg(test)]
mod data_validation_tests {
    use super::*;

    #[test]
    fn list_uses_one_source_and_does_not_submit_a_second_list() {
        let fields = data_validation_fields("list", "between");
        assert!(fields.iter().any(|f| matches!(f, Field::Text { key: "formula1", label: "Source:" })));
        assert!(!fields.iter().any(|f| matches!(f, Field::Text { key: "formula2", .. } | Field::Choice { key: "operator", .. })));

        let dialog = Dialog::form(
            "dataValidation",
            "Data Validation",
            "data.validation",
            fields,
            json!({"type": "list", "operator": "between", "formula1": "ABC,DEF", "formula2": "GHI,JKL", "dropdown": true}),
        );
        let params = build_params(&dialog);
        assert_eq!(params["formula1"], "ABC,DEF");
        assert!(params.get("formula2").is_none());
        assert!(params.get("operator").is_none());
    }

    #[test]
    fn only_between_operators_show_and_submit_two_bounds() {
        let two = data_validation_fields("whole", "between");
        assert!(two.iter().any(|f| matches!(f, Field::Text { key: "formula2", label: "Maximum:" })));
        let one = data_validation_fields("date", "lessOrEqual");
        assert!(one.iter().any(|f| matches!(f, Field::Text { key: "formula1", label: "End date:" })));
        assert!(!one.iter().any(|f| matches!(f, Field::Text { key: "formula2", .. })));

        let dialog = Dialog::form(
            "dataValidation",
            "Data Validation",
            "data.validation",
            one,
            json!({"type": "date", "operator": "lessOrEqual", "formula1": "2026-10-12", "formula2": "2030-01-01"}),
        );
        let params = build_params(&dialog);
        assert_eq!(params["operator"], "lessOrEqual");
        assert_eq!(params["formula1"], "2026-10-12");
        assert!(params.get("formula2").is_none());
    }

    #[test]
    fn message_tabs_follow_excel_order_with_titles_and_show_flags() {
        let keys: Vec<&str> = data_validation_fields("whole", "greater")
            .iter()
            .filter_map(|f| match f {
                Field::Text { key, .. } | Field::Check { key, .. } | Field::Choice { key, .. } => Some(*key),
                _ => None,
            })
            .skip_while(|k| *k != "showInputMessage")
            .collect();
        assert_eq!(keys, ["showInputMessage", "inputTitle", "inputMessage", "showErrorMessage", "errorStyle", "errorTitle", "errorMessage"]);
    }

    #[test]
    fn titles_and_show_flags_are_prefilled_from_the_rule_and_submitted() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let rule = json!({"range": "A1", "type": "whole", "operator": "greater", "formula1": "0", "inputTitle": "Quantity", "inputMessage": "Whole units",
            "errorTitle": "Not a quantity", "errorMessage": "Type a whole number", "showInputMessage": false});
        session.execute("data.validation", rule).unwrap();
        let app = SheetApp::new(session, Default::default());
        let v = validation_defaults(&app);
        assert_eq!(v["inputTitle"], "Quantity");
        assert_eq!(v["errorTitle"], "Not a quantity");
        assert_eq!(v["showInputMessage"], false);
        assert_eq!(v["showErrorMessage"], true);
        let dialog = Dialog::form("dataValidation", "Data Validation", "data.validation", data_validation_fields("whole", "greater"), v);
        let params = build_params(&dialog);
        for key in ["inputTitle", "inputMessage", "errorTitle", "errorMessage", "showInputMessage", "showErrorMessage"] {
            assert_eq!(params[key], dialog.values[key], "{key}");
        }
    }
}
