//! Data ▸ Get Data ▸ From Text/CSV preview, laid out as Excel's: File Origin, Delimiter and Data
//! Type Detection above a preview of what loads, then Load or Cancel. The preview comes from
//! `data.textPreview` and follows every choice; Load runs `data.fromTextCsv`.

use super::{Dialog, Json, SheetApp, json, vec2};

/// File Origin (元のファイル) choices: code page and caption.
const ORIGINS: [(&str, &str); 3] =
    [("932", "932: Japanese (Shift-JIS)"), ("65001", "65001: Unicode (UTF-8)"), ("1252", "1252: Western European (Windows)")];

/// Delimiter (区切り記号) choices.
const DELIMITERS: [(&str, &str); 5] = [("comma", "Comma"), ("tab", "Tab"), ("semicolon", "Semicolon"), ("space", "Space"), ("custom", "--Custom--")];

/// Data Type Detection (データ型検出) choices.
const DETECTS: [(&str, &str); 3] =
    [("first200", "Based on first 200 rows"), ("all", "Based on entire dataset"), ("none", "Do not detect data types")];

/// The params of `data.textPreview` / `data.fromTextCsv` for the dialog's choices.
pub(super) fn import_params(d: &Dialog) -> Json {
    let v = |k: &str| d.values.get(k).cloned().unwrap_or(Json::Null);
    let delimiter = match d.values.get("delimiter").and_then(Json::as_str) {
        Some("custom") => v("customDelimiter"),
        _ => v("delimiter"),
    };
    let mut p = json!({"encoding": v("encoding"), "delimiter": delimiter, "detect": v("detect")});
    for k in ["path", "name", "base64", "text", "sheet"] {
        if let Some(x) = d.values.get(k) {
            p[k] = x.clone();
        }
    }
    p
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

pub(super) fn text_import(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    // The preview is asked for again only when a choice changed.
    let params = import_params(d);
    let key = json!([params["encoding"], params["delimiter"], params["detect"]]).to_string();
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
                    d.values.insert("previewKey".into(), json!(json!([p["encoding"], p["delimiter"], p["detect"]]).to_string()));
                }
                d.result = Some(r);
            }
            Err(e) => d.result = Some(json!({"error": crate::clean_error(&e)})),
        }
    }
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
    if let Some(e) = d.result.as_ref().and_then(|r| r.get("error")).and_then(Json::as_str) {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
    let t = crate::theme::Tokens::get(ui.ctx());
    egui::Frame::new().stroke(egui::Stroke::new(1.0, t.input_border)).inner_margin(4.0).show(ui, |ui| {
        egui::ScrollArea::both().max_height(320.0).max_width(760.0).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("text_import_preview").striped(true).spacing(vec2(14.0, 3.0)).show(ui, |ui| {
                for (i, row) in rows.iter().enumerate() {
                    for cell in row {
                        let text: String = cell.chars().take(60).collect();
                        if i == 0 {
                            ui.label(egui::RichText::new(text).strong())
                        } else {
                            ui.label(text)
                        };
                    }
                    ui.end_row();
                }
            });
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!(" {} ", lang.tr("Cancel"))).clicked() {
                *open = false;
            }
            let load = ui.add(egui::Button::new(egui::RichText::new(format!("   {}   ", lang.tr("Load"))).color(t.on_accent)).fill(t.accent));
            if load.clicked() {
                *confirm = true;
            }
        });
    });
}
