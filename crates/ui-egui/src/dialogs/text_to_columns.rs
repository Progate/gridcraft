//! Data ▸ Text to Columns, as Excel's three-step Convert Text to Columns Wizard: 1 Delimited or
//! Fixed width, 2 the delimiters (or the break lines, set with the mouse on the preview), 3 each
//! column's data format (General, Text, Date in an order, or skipped) and the Destination.
//! Finish runs `data.textToColumns`.

use super::{Dialog, Json, SheetApp, json, vec2};

/// Rows of the selection the previews show.
const PREVIEW_ROWS: u32 = 20;

/// Date orders of step 3's Date box.
const DATE_ORDERS: [&str; 6] = ["YMD", "MDY", "DMY", "YDM", "MYD", "DYM"];

/// The defaults the wizard opens with (the destination is the selection's first cell).
pub(super) fn defaults(destination: &str) -> Json {
    json!({"step": 1, "type": "delimited", "tab": true, "comma": true, "textQualifier": "\"", "destination": destination, "breaks": [], "columns": [], "selectedColumn": 0, "dateOrder": "YMD"})
}

fn flag(d: &Dialog, k: &str) -> bool {
    d.values.get(k).and_then(Json::as_bool) == Some(true)
}

fn text(d: &Dialog, k: &str) -> String {
    d.values.get(k).and_then(Json::as_str).unwrap_or_default().to_string()
}

fn breaks(d: &Dialog) -> Vec<usize> {
    let mut b: Vec<usize> = d
        .values
        .get("breaks")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_u64().map(|v| v as usize)).collect())
        .unwrap_or_default();
    b.sort_unstable();
    b.dedup();
    b
}

fn delimiters(d: &Dialog) -> Vec<char> {
    let mut v: Vec<char> =
        [("tab", '\t'), ("semicolon", ';'), ("comma", ','), ("space", ' ')].iter().filter(|(k, _)| flag(d, k)).map(|(_, c)| *c).collect();
    if let Some(o) = text(d, "other").chars().next() {
        v.push(o);
    }
    v
}

fn quote(d: &Dialog) -> char {
    text(d, "textQualifier").chars().next().unwrap_or('\u{0}')
}

/// The fields one line splits into with the wizard's settings, as `data.textToColumns` splits it.
fn fields(d: &Dialog, line: &str) -> Vec<String> {
    if text(d, "type") == "fixed" {
        let chars: Vec<char> = line.chars().collect();
        let mut cuts = vec![0];
        cuts.extend(breaks(d).into_iter().filter(|b| *b > 0 && *b < chars.len()));
        cuts.push(chars.len());
        cuts.windows(2).map(|w| chars.get(w[0]..w[1]).map(|s| s.iter().collect::<String>().trim().to_string()).unwrap_or_default()).collect()
    } else {
        gridcraft_engine::cmd::data::split_delimited(line, &delimiters(d), flag(d, "treatConsecutive"), quote(d))
    }
}

/// The parameters Finish sends.
pub(super) fn params(d: &Dialog) -> Json {
    let mut p = json!({
        "destination": d.values.get("destination").cloned().unwrap_or(Json::Null),
        "columns": d.values.get("columns").cloned().unwrap_or(json!([])),
        "treatConsecutive": flag(d, "treatConsecutive"),
    });
    if let Some(r) = d.values.get("range") {
        p["range"] = r.clone();
    }
    if text(d, "type") == "fixed" {
        p["fixedWidths"] = json!(breaks(d));
    } else {
        p["delimiters"] = json!(delimiters(d).iter().map(char::to_string).collect::<Vec<_>>());
        let q = quote(d);
        // {none}: a character no text holds, so quotes stay part of the fields.
        p["textQualifier"] = json!(if q == '\u{0}' { "\u{1}".to_string() } else { q.to_string() });
    }
    p
}

/// The selected cells' text (first column, first rows), as the wizard previews it.
fn source_lines(app: &SheetApp) -> Vec<String> {
    let Some(d) = app.session.active() else { return vec![] };
    let Some(sh) = d.wb.active() else { return vec![] };
    let r = d.selection.ranges.first().copied().unwrap_or(gridcraft_engine::core::RangeRef::cell(d.selection.active));
    let last = r.end.row.min(r.start.row.saturating_add(PREVIEW_ROWS - 1));
    (r.start.row..=last).map(|row| sh.value(gridcraft_engine::core::CellRef::new(row, r.start.col)).display()).collect()
}

fn set_title(d: &mut Dialog, step: u64) {
    d.title = format!("Convert Text to Columns Wizard - Step {step} of 3");
    d.values.insert("step".into(), json!(step));
}

pub(super) fn text_to_columns(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let step = d.values.get("step").and_then(Json::as_u64).unwrap_or(1);
    let lines = source_lines(app);
    let fixed = text(d, "type") == "fixed";
    match step {
        1 => {
            ui.label(lang.tr("The Text Wizard has determined that your data is Delimited. If this is correct, choose Next, or choose the data type that best describes your data."));
            ui.add_space(6.0);
            ui.label(egui::RichText::new(lang.tr("Original data type")).strong());
            for (v, label) in [
                ("delimited", "Delimited - Characters such as commas or tabs separate each field."),
                ("fixed", "Fixed width - Fields are aligned in columns with spaces between each field."),
            ] {
                if ui.radio(text(d, "type") == v, lang.tr(label)).clicked() {
                    d.set_value("type", json!(v));
                }
            }
            preview_lines(ui, lang.tr("Data preview"), &lines);
        }
        2 if fixed => {
            ui.label(lang.tr("This screen lets you set field widths (column breaks). To CREATE a break line, click at the desired position. To DELETE a break line, double click on the line. To MOVE a break line, click and drag it."));
            ui.add_space(6.0);
            break_editor(ui, d, &lines);
        }
        2 => {
            ui.label(
                lang.tr("This screen lets you set the delimiters your data contains. You can see how your text is affected in the preview below."),
            );
            ui.add_space(6.0);
            ui.label(egui::RichText::new(lang.tr("Delimiters")).strong());
            ui.horizontal_wrapped(|ui| {
                for (k, label) in [("tab", "Tab"), ("semicolon", "Semicolon"), ("comma", "Comma"), ("space", "Space")] {
                    let mut b = flag(d, k);
                    if ui.checkbox(&mut b, lang.tr(label)).changed() {
                        d.set_value(k, json!(b));
                    }
                }
                let l = ui.label(lang.tr("Other:"));
                let mut o = text(d, "other");
                if ui.add(egui::TextEdit::singleline(&mut o).desired_width(28.0).char_limit(1)).labelled_by(l.id).changed() {
                    d.set_value("other", json!(o));
                }
            });
            let mut c = flag(d, "treatConsecutive");
            if ui.checkbox(&mut c, lang.tr("Treat consecutive delimiters as one")).changed() {
                d.set_value("treatConsecutive", json!(c));
            }
            ui.horizontal(|ui| {
                let l = ui.label(lang.tr("Text qualifier:"));
                let cur = text(d, "textQualifier");
                let shown = if cur.is_empty() { "{none}".to_string() } else { cur.clone() };
                craft_ui::ComboBox::from_id_salt("t2c_qualifier")
                    .selected_text(shown)
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for (v, t) in [("\"", "\""), ("'", "'"), ("", "{none}")] {
                            if ui.selectable_label(cur == v, t).clicked() {
                                d.set_value("textQualifier", json!(v));
                            }
                        }
                    })
                    .response
                    .labelled_by(l.id);
            });
            preview_grid(ui, d, &lines, false);
        }
        _ => {
            ui.label(lang.tr("This screen lets you select each column and set the Data Format."));
            ui.add_space(6.0);
            let n = lines.iter().map(|l| fields(d, l).len()).max().unwrap_or(1).max(1);
            let selected = d.values.get("selectedColumn").and_then(Json::as_u64).unwrap_or(0) as usize;
            let mut columns: Vec<String> = d
                .values
                .get("columns")
                .and_then(Json::as_array)
                .map(|a| a.iter().map(|x| x.as_str().unwrap_or("general").to_string()).collect())
                .unwrap_or_default();
            columns.resize(n.max(columns.len()), "general".into());
            let cur = columns.get(selected).cloned().unwrap_or_else(|| "general".into());
            ui.label(egui::RichText::new(lang.tr("Column data format")).strong());
            let mut choice: Option<String> = None;
            if ui.radio(cur == "general", lang.tr("General")).clicked() {
                choice = Some("general".into());
            }
            if ui.radio(cur == "text", lang.tr("Text")).clicked() {
                choice = Some("text".into());
            }
            ui.horizontal(|ui| {
                let is_date = DATE_ORDERS.iter().any(|o| o.eq_ignore_ascii_case(&cur));
                let order = if is_date { cur.to_ascii_uppercase() } else { text(d, "dateOrder") };
                if ui.radio(is_date, lang.tr("Date:")).clicked() {
                    choice = Some(order.to_ascii_lowercase());
                }
                craft_ui::ComboBox::from_id_salt("t2c_date_order").selected_text(order.clone()).width(70.0).show_ui(ui, |ui| {
                    for o in DATE_ORDERS {
                        if ui.selectable_label(order == o, o).clicked() {
                            d.values.insert("dateOrder".into(), json!(o));
                            choice = Some(o.to_ascii_lowercase());
                        }
                    }
                });
            });
            if ui.radio(cur == "skip", lang.tr("Do not import column (skip)")).clicked() {
                choice = Some("skip".into());
            }
            if let Some(c) = choice
                && let Some(slot) = columns.get_mut(selected)
            {
                *slot = c;
            }
            d.values.insert("columns".into(), json!(columns));
            ui.horizontal(|ui| {
                let l = ui.label(lang.tr("Destination:"));
                let mut dest = text(d, "destination");
                if ui.add(egui::TextEdit::singleline(&mut dest).desired_width(160.0)).labelled_by(l.id).changed() {
                    d.set_value("destination", json!(dest));
                }
            });
            preview_grid(ui, d, &lines, true);
        }
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let t = crate::theme::Tokens::get(ui.ctx());
            if ui.add(egui::Button::new(egui::RichText::new(format!("  {}  ", lang.tr("Finish"))).color(t.on_accent)).fill(t.accent)).clicked() {
                *confirm = true;
            }
            if ui.add_enabled(step < 3, egui::Button::new(lang.tr("Next >"))).clicked() {
                set_title(d, step + 1);
            }
            if ui.add_enabled(step > 1, egui::Button::new(lang.tr("< Back"))).clicked() {
                set_title(d, step - 1);
            }
            if ui.button(lang.tr("Cancel")).clicked() {
                *open = false;
            }
        });
    });
}

fn preview_frame(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).strong());
    let t = crate::theme::Tokens::get(ui.ctx());
    egui::Frame::new().stroke(egui::Stroke::new(1.0, t.input_border)).inner_margin(4.0).show(ui, |ui| {
        egui::ScrollArea::both().id_salt("t2c_preview").max_height(200.0).max_width(560.0).auto_shrink([false, true]).show(ui, body);
    });
}

fn preview_lines(ui: &mut egui::Ui, title: &str, lines: &[String]) {
    preview_frame(ui, title, |ui| {
        for l in lines {
            ui.monospace(l);
        }
    });
}

/// The split preview; in step 3 a click on a column selects it, and each column's header shows its format.
fn preview_grid(ui: &mut egui::Ui, d: &mut Dialog, lines: &[String], pick: bool) {
    let lang = crate::i18n::current(ui.ctx());
    let rows: Vec<Vec<String>> = lines.iter().map(|l| fields(d, l)).collect();
    let n = rows.iter().map(Vec::len).max().unwrap_or(0);
    let selected = d.values.get("selectedColumn").and_then(Json::as_u64).unwrap_or(0) as usize;
    let formats: Vec<String> = d
        .values
        .get("columns")
        .and_then(Json::as_array)
        .map(|a| a.iter().map(|x| x.as_str().unwrap_or("general").to_string()).collect())
        .unwrap_or_default();
    let mut clicked = None;
    preview_frame(ui, lang.tr("Data preview"), |ui| {
        egui::Grid::new("t2c_grid").striped(true).spacing(vec2(12.0, 2.0)).show(ui, |ui| {
            if pick {
                for c in 0..n {
                    let f = formats.get(c).map_or("general", String::as_str);
                    let head = match f {
                        "general" => lang.tr("General").to_string(),
                        "text" => lang.tr("Text").to_string(),
                        "skip" => lang.tr("Skip Column").to_string(),
                        other => other.to_ascii_uppercase(),
                    };
                    if ui.selectable_label(c == selected, head).clicked() {
                        clicked = Some(c);
                    }
                }
                ui.end_row();
            }
            for row in &rows {
                for c in 0..n {
                    let cell = row.get(c).map_or("", String::as_str);
                    let r = if pick { ui.selectable_label(c == selected, cell) } else { ui.label(cell) };
                    if pick && r.clicked() {
                        clicked = Some(c);
                    }
                }
                ui.end_row();
            }
        });
    });
    if let Some(c) = clicked {
        d.values.insert("selectedColumn".into(), json!(c));
    }
}

/// Step 2 for Fixed width: the lines in a fixed-pitch font under a ruler; a click adds a break
/// line, a double click on one removes it, dragging one moves it.
fn break_editor(ui: &mut egui::Ui, d: &mut Dialog, lines: &[String]) {
    let lang = crate::i18n::current(ui.ctx());
    let font = egui::FontId::monospace(13.0);
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);
    let row_h = 16.0;
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(10).max(10) as f32 * char_w + 40.0;
    let height = row_h * (lines.len() as f32 + 1.5);
    let mut b = breaks(d);
    preview_frame(ui, lang.tr("Data preview"), |ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(width, height), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let t = crate::theme::Tokens::get(ui.ctx());
        let color = ui.visuals().text_color();
        let left = rect.left() + 4.0;
        // Ruler: a tick every character, a number every ten.
        for i in 0..=((width - 8.0) / char_w) as usize {
            let x = left + i as f32 * char_w;
            let h = if i % 10 == 0 {
                6.0
            } else if i % 5 == 0 {
                4.0
            } else {
                2.0
            };
            painter.line_segment([egui::pos2(x, rect.top() + row_h - h), egui::pos2(x, rect.top() + row_h)], egui::Stroke::new(1.0, color));
            if i % 10 == 0 && i > 0 {
                painter.text(egui::pos2(x, rect.top()), egui::Align2::CENTER_TOP, i.to_string(), egui::FontId::proportional(9.0), color);
            }
        }
        for (k, l) in lines.iter().enumerate() {
            painter.text(egui::pos2(left, rect.top() + row_h * (k as f32 + 1.2)), egui::Align2::LEFT_TOP, l, font.clone(), color);
        }
        for p in &b {
            let x = left + *p as f32 * char_w;
            painter.line_segment([egui::pos2(x, rect.top() + row_h * 0.5), egui::pos2(x, rect.bottom())], egui::Stroke::new(1.5, t.accent));
        }
        let pos_at = |x: f32| ((x - left) / char_w).round().max(0.0) as usize;
        let near = |b: &[usize], x: f32| b.iter().position(|p| (left + *p as f32 * char_w - x).abs() <= char_w * 0.6);
        let drag_id = ui.scope_id().with("t2c_drag");
        if resp.drag_started()
            && let Some(at) = resp.interact_pointer_pos()
        {
            let hit = near(&b, at.x);
            ui.data_mut(|m| m.insert_temp(drag_id, hit));
        }
        if resp.dragged()
            && let Some(at) = resp.interact_pointer_pos()
            && let Some(Some(i)) = ui.data(|m| m.get_temp::<Option<usize>>(drag_id))
            && let Some(slot) = b.get_mut(i)
        {
            *slot = pos_at(at.x).max(1);
        }
        if resp.double_clicked() {
            if let Some(at) = resp.interact_pointer_pos()
                && let Some(i) = near(&b, at.x)
            {
                b.remove(i);
            }
        } else if resp.clicked()
            && let Some(at) = resp.interact_pointer_pos()
            && near(&b, at.x).is_none()
        {
            let p = pos_at(at.x);
            if p > 0 {
                b.push(p);
            }
        }
    });
    d.values.insert("breaks".into(), json!(b));
}
