//! Data ▸ Get Data ▸ From Text/CSV preview, laid out as Excel's: File Origin, Delimiter and Data
//! Type Detection above a preview of what loads, then Load, Transform Data or Cancel. The preview
//! comes from `data.textPreview` and follows every choice. Transform Data turns the window into a
//! small Power Query Editor: each column's header icon chooses its data type (Auto, Text, Whole
//! Number, … or Remove), asking Change Column Type first as Power Query does, and Close & Load
//! loads. Load and Close & Load run `data.fromTextCsv`.

use super::{Dialog, Json, SheetApp, json, vec2};

/// File Origin (元のファイル) choices: code page and caption.
const ORIGINS: [(&str, &str); 3] =
    [("932", "932: Japanese (Shift-JIS)"), ("65001", "65001: Unicode (UTF-8)"), ("1252", "1252: Western European (Windows)")];

/// Delimiter (区切り記号) choices.
const DELIMITERS: [(&str, &str); 5] = [("comma", "Comma"), ("tab", "Tab"), ("semicolon", "Semicolon"), ("space", "Space"), ("custom", "--Custom--")];

/// Data Type Detection (データ型検出) choices.
const DETECTS: [(&str, &str); 3] =
    [("first200", "Based on first 200 rows"), ("all", "Based on entire dataset"), ("none", "Do not detect data types")];

/// Column data types (データ型) a preview header offers, as Power Query's header menu names them,
/// with the short glyph its icon shows; `auto` is From Text/CSV's own detection and `skip`
/// removes the column.
const COLUMN_TYPES: [(&str, &str, &str); 9] = [
    ("auto", "Column type|Auto", ""),
    ("text", "Column type|Text", "ABC"),
    ("integer", "Column type|Whole Number", "1²3"),
    ("decimal", "Column type|Decimal Number", "1.2"),
    ("date", "Column type|Date", ""),
    ("datetime", "Column type|Date/Time", ""),
    ("time", "Column type|Time", ""),
    ("boolean", "Column type|True/False", ""),
    ("skip", "Column type|Remove", ""),
];

/// The params of `data.textPreview` / `data.fromTextCsv` for the dialog's choices.
pub(super) fn import_params(d: &Dialog) -> Json {
    let v = |k: &str| d.values.get(k).cloned().unwrap_or(Json::Null);
    let delimiter = match d.values.get("delimiter").and_then(Json::as_str) {
        Some("custom") => v("customDelimiter"),
        _ => v("delimiter"),
    };
    let mut p = json!({"encoding": v("encoding"), "delimiter": delimiter, "detect": v("detect")});
    for k in ["path", "name", "base64", "text", "sheet", "columns", "types"] {
        if let Some(x) = d.values.get(k) {
            p[k] = x.clone();
        }
    }
    p
}

/// What the preview depends on: it is asked for again when this changes.
fn preview_key(p: &Json) -> String {
    json!([p["encoding"], p["delimiter"], p["detect"], p["columns"], p["types"]]).to_string()
}

/// Types the dialog was opened with by header name (`types`) become per-column choices once the
/// preview shows the header, so a header's menu can change them again.
fn fold_named_types(d: &mut Dialog, preview: &Json) {
    let Some(types) = d.values.get("types").and_then(Json::as_object).cloned() else { return };
    let header: Vec<&str> =
        preview.pointer("/rows/0").and_then(Json::as_array).map(|h| h.iter().filter_map(Json::as_str).collect()).unwrap_or_default();
    let mut columns = column_choices(d, header.len());
    for (name, t) in &types {
        if let Some(c) = header.iter().position(|h| h == name) {
            columns[c] = t.clone();
        }
    }
    d.values.remove("types");
    d.values.insert("columns".into(), Json::Array(columns));
    d.values.insert("previewKey".into(), json!(preview_key(&import_params(d))));
}

/// The chosen type of each of `n` columns (`auto` where none was chosen).
fn column_choices(d: &Dialog, n: usize) -> Vec<Json> {
    let mut c: Vec<Json> = d.values.get("columns").and_then(Json::as_array).cloned().unwrap_or_default();
    if c.len() < n {
        c.resize(n, json!("auto"));
    }
    c
}

/// Whether the dialog shows the Power Query Editor rather than the preview.
fn in_editor(d: &Dialog) -> bool {
    d.values.get("step").and_then(Json::as_str) == Some("editor")
}

/// Whether choosing a type for `col` asks Change Column Type first: Power Query asks when the
/// column already has a type step, which detection gives every column and a choice gives one.
fn asks_before_changing(d: &Dialog, col: usize) -> bool {
    let detects = d.values.get("detect").and_then(Json::as_str).is_some_and(|x| x != "none");
    detects || column_choices(d, col + 1)[col].as_str().is_some_and(|c| c != "auto")
}

/// Sets column `col`'s type choice.
fn set_column_type(d: &mut Dialog, col: usize, val: &str) {
    let mut columns = column_choices(d, col + 1);
    columns[col] = json!(val);
    d.set_value("columns", Json::Array(columns));
}

/// An editor column's header: Power Query's type icon (the type chosen, or detected for an
/// automatic column) opening the data type menu, then the column name.
fn column_header(ui: &mut egui::Ui, d: &mut Dialog, col: usize, name: &str, shown_type: &str) {
    let lang = crate::i18n::current(ui.ctx());
    let chosen = column_choices(d, col + 1)[col].as_str().unwrap_or("auto").to_string();
    let (_, label, glyph) = COLUMN_TYPES.iter().find(|(v, ..)| *v == shown_type).copied().unwrap_or(COLUMN_TYPES[0]);
    let icon = if glyph.is_empty() { format!("{} ▾", lang.tr(label)) } else { format!("{glyph} {} ▾", lang.tr(label)) };
    ui.vertical(|ui| {
        let mut picked = None;
        ui.menu_button(egui::RichText::new(icon).small(), |ui| {
            for (val, text, _) in COLUMN_TYPES {
                if ui.selectable_label(chosen == val, lang.tr(text)).clicked() {
                    picked = Some(val);
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text(lang.tr("Data Type"));
        if let Some(val) = picked
            && val != chosen
        {
            if asks_before_changing(d, col) {
                d.values.insert("pendingType".into(), json!([col, val]));
            } else {
                set_column_type(d, col, val);
            }
        }
        let text: String = name.chars().take(60).collect();
        let text = egui::RichText::new(text).strong();
        ui.label(if chosen == "skip" { text.strikethrough().weak() } else { text });
    });
}

fn combo(ui: &mut egui::Ui, d: &mut Dialog, key: &str, label: &str, options: &[(&str, &str)], width: f32) {
    let lang = crate::i18n::current(ui.ctx());
    ui.vertical(|ui| {
        let l = ui.label(egui::RichText::new(lang.tr(label)).strong());
        let cur = d.values.get(key).and_then(Json::as_str).unwrap_or_default().to_string();
        let shown = options.iter().find(|(v, _)| *v == cur).map_or(cur.as_str(), |(_, t)| lang.tr(t)).to_string();
        craft_ui::ComboBox::from_id_salt(("textImport", key.to_string()))
            .selected_text(shown)
            .width(width)
            .show_ui(ui, |ui| {
                for (val, text) in options {
                    if ui.selectable_label(cur == *val, lang.tr(text)).clicked() {
                        d.set_value(key, json!(val));
                    }
                }
            })
            .response
            .labelled_by(l.id);
    });
}

/// Change Column Type, over the editor: Replace current and Add new step both make the pending
/// type the column's (the load has one type per column, so a new step ends the same); Cancel and
/// Escape keep the old one.
fn change_type_question(ui: &egui::Ui, d: &mut Dialog) {
    let Some((col, val)) = d.values.get("pendingType").and_then(Json::as_array).and_then(|p| {
        let col = usize::try_from(p.first()?.as_u64()?).ok()?;
        Some((col, p.get(1)?.as_str()?.to_string()))
    }) else {
        return;
    };
    let lang = crate::i18n::current(ui.ctx());
    let mut answer: Option<bool> = None;
    let modal = egui::Modal::new(egui::Id::unique("gridcraft.change_column_type")).show(ui.ctx(), |ui| {
        ui.set_max_width(420.0);
        ui.heading(lang.tr("Change Column Type"));
        ui.add_space(6.0);
        ui.label(lang.tr("The selected column already has a type conversion. Do you want to replace the current conversion, or add a new step?"));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let t = crate::theme::Tokens::get(ui.ctx());
            let replace = egui::Button::new(egui::RichText::new(lang.tr("Replace current")).color(t.on_accent)).fill(t.accent);
            if ui.add(replace).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                answer = Some(true);
            }
            if ui.button(lang.tr("Add new step")).clicked() {
                answer = Some(true);
            }
            if ui.button(lang.tr("Cancel")).clicked() {
                answer = Some(false);
            }
        });
        // Escape answers the question, not the editor behind it.
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            answer = Some(false);
        }
    });
    if modal.should_close() && answer.is_none() {
        answer = Some(false);
    }
    if let Some(yes) = answer {
        d.values.remove("pendingType");
        if yes {
            set_column_type(d, col, &val);
        }
    }
}

pub(super) fn text_import(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let editor = in_editor(d);
    // The preview is asked for again only when a choice changed.
    let params = import_params(d);
    let key = preview_key(&params);
    if d.values.get("previewKey").and_then(Json::as_str) != Some(key.as_str()) {
        let preview = app.session.run("data.textPreview", params);
        d.values.insert("previewKey".into(), json!(key));
        match preview {
            Ok(r) => {
                // Automatic choices show what was detected, as Excel's boxes do.
                let mut detected = false;
                for k in ["encoding", "delimiter"] {
                    if d.values.get(k).and_then(Json::as_str) == Some("auto")
                        && let Some(x) = r.get(k).and_then(Json::as_str)
                    {
                        let x = if k == "delimiter" && !DELIMITERS.iter().any(|(v, _)| *v == x) {
                            d.values.insert("customDelimiter".into(), json!(x));
                            "custom"
                        } else {
                            x
                        };
                        d.values.insert(k.into(), json!(x));
                        detected = true;
                    }
                }
                if detected {
                    let p = import_params(d);
                    d.values.insert("previewKey".into(), json!(preview_key(&p)));
                }
                fold_named_types(d, &r);
                d.result = Some(r);
            }
            Err(e) => d.result = Some(json!({"error": crate::clean_error(&e)})),
        }
    }
    if !editor {
        ui.horizontal(|ui| {
            combo(ui, d, "encoding", "File Origin", &ORIGINS, 230.0);
            combo(ui, d, "delimiter", "Delimiter", &DELIMITERS, 130.0);
            combo(ui, d, "detect", "Data Type Detection", &DETECTS, 200.0);
        });
        if d.values.get("delimiter").and_then(Json::as_str) == Some("custom") {
            ui.horizontal(|ui| {
                let l = ui.label(lang.tr("Custom delimiter:"));
                let mut t = d.values.get("customDelimiter").and_then(Json::as_str).unwrap_or_default().to_string();
                if ui.add(egui::TextEdit::singleline(&mut t).desired_width(40.0).char_limit(1)).labelled_by(l.id).changed() {
                    d.values.insert("customDelimiter".into(), json!(t));
                }
            });
        }
    }
    ui.add_space(6.0);
    let rows: Vec<Vec<String>> = d
        .result
        .as_ref()
        .and_then(|r| r.get("rows"))
        .and_then(Json::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| row.as_array().map(|c| c.iter().map(|x| x.as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    let types: Vec<String> = d
        .result
        .as_ref()
        .and_then(|r| r.get("types"))
        .and_then(Json::as_array)
        .map(|t| t.iter().map(|x| x.as_str().unwrap_or("text").to_string()).collect())
        .unwrap_or_default();
    if let Some(e) = d.result.as_ref().and_then(|r| r.get("error")).and_then(Json::as_str) {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
    let t = crate::theme::Tokens::get(ui.ctx());
    egui::Frame::new().stroke(egui::Stroke::new(1.0, t.input_border)).inner_margin(4.0).show(ui, |ui| {
        egui::ScrollArea::both().max_height(320.0).max_width(760.0).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("text_import_preview").striped(true).spacing(vec2(14.0, 3.0)).show(ui, |ui| {
                let skipped: Vec<bool> = types.iter().map(|t| t == "skip").collect();
                for (i, row) in rows.iter().enumerate() {
                    for (c, cell) in row.iter().enumerate() {
                        if i == 0 {
                            if editor {
                                column_header(ui, d, c, cell, types.get(c).map_or("text", String::as_str));
                            } else {
                                // The preview names the columns only: types are chosen in the editor, as in Excel.
                                let text = egui::RichText::new(cell.chars().take(60).collect::<String>()).strong();
                                ui.label(if skipped.get(c).copied().unwrap_or(false) { text.strikethrough().weak() } else { text });
                            }
                            continue;
                        }
                        let text = egui::RichText::new(cell.chars().take(60).collect::<String>());
                        // A removed column stays in the preview, dimmed, so its menu can bring it back.
                        ui.label(if skipped.get(c).copied().unwrap_or(false) { text.weak() } else { text });
                    }
                    ui.end_row();
                }
            });
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if editor {
                let close = egui::Button::new(egui::RichText::new(format!("   {}   ", lang.tr("Close & Load"))).color(t.on_accent)).fill(t.accent);
                if ui.add(close).clicked() {
                    *confirm = true;
                }
                return;
            }
            if ui.button(format!(" {} ", lang.tr("Cancel"))).clicked() {
                *open = false;
            }
            if ui.button(format!(" {} ", lang.tr("Transform Data"))).clicked() {
                d.values.insert("step".into(), json!("editor"));
                d.title = "Power Query Editor".into();
            }
            let load = ui.add(egui::Button::new(egui::RichText::new(format!("   {}   ", lang.tr("Load"))).color(t.on_accent)).fill(t.accent));
            if load.clicked() {
                *confirm = true;
            }
        });
    });
    change_type_question(ui, d);
}
