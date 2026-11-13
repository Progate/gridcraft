#[cfg(test)]
use super::show::build_params;
use super::show::ok_cancel;
use super::{Dialog, Json, Key, SheetApp, json};

pub(super) fn name_manager(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let list = app.session.run("formulas.nameManager", json!({})).unwrap_or(Json::Null);
    let names = list.as_array().cloned().unwrap_or_default();
    let selected = names.get(d.list_index).cloned();
    let field = |n: &Json, k: &str| n.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    ui.horizontal(|ui| {
        if ui.button(crate::i18n::current(ui.ctx()).tr("New…")).clicked() {
            app.open_dialog("defineName", json!({}));
        }
        if ui.add_enabled(selected.is_some(), egui::Button::new(crate::i18n::current(ui.ctx()).tr("Edit…"))).clicked()
            && let Some(n) = &selected
        {
            app.open_dialog(
                "defineName",
                json!({
                    "name": field(n, "name"), "scope": field(n, "scope"), "comment": field(n, "comment"),
                    "refersTo": field(n, "refersTo"), "oldName": field(n, "name"), "oldScope": field(n, "scope"),
                }),
            );
        }
        if ui.add_enabled(selected.is_some(), egui::Button::new(crate::i18n::current(ui.ctx()).tr("Delete"))).clicked()
            && let Some(n) = &selected
        {
            app.run_or_alert("formulas.deleteName", json!({"name": field(n, "name"), "scope": field(n, "scope")}));
        }
    });
    ui.add_space(4.0);
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        egui::Grid::new("names").striped(true).num_columns(4).show(ui, |ui| {
            ui.strong(crate::i18n::current(ui.ctx()).tr("Name"));
            ui.strong(crate::i18n::current(ui.ctx()).tr("Value"));
            ui.strong(crate::i18n::current(ui.ctx()).tr("Refers To"));
            ui.strong(crate::i18n::current(ui.ctx()).tr("Scope"));
            ui.end_row();
            if names.is_empty() {
                ui.label(egui::RichText::new(crate::i18n::current(ui.ctx()).tr("No names defined")).italics());
                ui.end_row();
            }
            for (i, n) in names.iter().enumerate() {
                if ui.selectable_label(i == d.list_index, field(n, "name")).clicked() {
                    d.list_index = i;
                }
                ui.label(n.get("value").map(|v| v.to_string()).unwrap_or_default().chars().take(30).collect::<String>());
                ui.label(field(n, "refersTo"));
                ui.label(field(n, "scope"));
                ui.end_row();
            }
        });
    });
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button(" Close ").clicked() {
            *open = false;
        }
    });
}

/// The names Paste Names offers in the active workbook.
pub(super) fn pasteable_names(app: &SheetApp) -> Vec<String> {
    app.session
        .active()
        .map(|doc| gridcraft_engine::cmd::formulas::pasteable_names(&doc.wb).into_iter().map(|(n, _)| n).collect())
        .unwrap_or_default()
}

/// Puts a defined name into the formula being edited, at the caret. Without an edit under way it
/// starts one in the active cell with `=` and the name.
pub(crate) fn insert_name(app: &mut SheetApp, name: &str) {
    match app.editor.as_mut() {
        Some(ed) => ed.insert_at_caret(name),
        None => app.begin_edit(Some(format!("={name}")), false),
    }
}

/// Paste Name (F3, Formulas › Use in Formula › Paste Names…): OK, or a double-click, inserts the
/// selected name (`formulas.pasteName` with `name`); Paste List writes every name and what it
/// refers to from the active cell.
pub(super) fn paste_name(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let names: Vec<String> =
        d.result.as_ref().and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_str).map(str::to_owned).collect()).unwrap_or_default();
    // The `name` value is the selection, so `ui.dialog.set` can choose one too.
    if let Some(i) = d.values.get("name").and_then(Json::as_str).and_then(|n| names.iter().position(|x| x == n)) {
        d.list_index = i;
    }
    let (down, up) = ui.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, Key::ArrowDown), i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)));
    let last = names.len().saturating_sub(1);
    if down {
        d.list_index = (d.list_index + 1).min(last);
    }
    if up {
        d.list_index = d.list_index.saturating_sub(1);
    }
    d.list_index = d.list_index.min(last);
    ui.label(app.ui.language.tr("Paste name"));
    if names.is_empty() {
        ui.label(egui::RichText::new(app.ui.language.tr("No names defined")).italics());
    }
    // A workbook can hold thousands of names: only the rows in view are laid out.
    let row_height = ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
    let step = row_height + ui.spacing().item_spacing.y;
    egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show_rows(ui, row_height, names.len(), |ui, rows| {
        let top = ui.cursor().top() - rows.start as f32 * step;
        for (i, n) in names.iter().enumerate().skip(rows.start).take(rows.len()) {
            let r = ui.selectable_label(i == d.list_index, n);
            if r.clicked() {
                d.list_index = i;
            }
            if r.double_clicked() {
                *confirm = true;
            }
        }
        // The arrow keys can move the selection onto a row that isn't laid out.
        if down || up {
            let rect = egui::Rect::from_min_size(egui::pos2(ui.min_rect().left(), top + d.list_index as f32 * step), egui::vec2(1.0, row_height));
            if !ui.clip_rect().contains_rect(rect) {
                ui.scroll_to_rect(rect, Some(if down { egui::Align::Max } else { egui::Align::Min }));
            }
        }
    });
    ui.add_space(4.0);
    // Paste List is disabled while a cell is being edited.
    if ui.add_enabled(!names.is_empty() && app.editor.is_none(), egui::Button::new(app.ui.language.tr("Paste List"))).clicked() {
        app.run_or_alert("formulas.pasteName", json!({"list": true}));
        *open = false;
        return;
    }
    if let Some(n) = names.get(d.list_index) {
        d.values.insert("name".into(), json!(n));
    }
    ok_cancel(ui, confirm, open);
}

/// The edge boxes of Create Names from Selection, in Excel's order: (value key, label).
const LABEL_EDGES: [(&str, &str); 4] = [("top", "Top row"), ("left", "Left column"), ("bottom", "Bottom row"), ("right", "Right column")];

/// Formulas › Create from Selection (Ctrl+Shift+F3) opens Excel's Create Names from Selection on
/// the selection (the current region around a single cell, as Excel takes it), the top row and
/// the left column ticked when they hold text (`guess_label_edges`).
pub(super) fn create_names_open(app: &SheetApp) -> Dialog {
    use gridcraft_engine::cmd::formulas::guess_label_edges;
    let found = app.session.active().and_then(|doc| {
        let sh = doc.wb.active()?;
        let r = doc.selection.current();
        let r = if r.is_single() { gridcraft_engine::selection::current_region(sh, doc.selection.active) } else { r };
        Some((r, guess_label_edges(&doc.wb, doc.wb.active_sheet(), r)))
    });
    let (range, e) = found.map(|(r, e)| (r.a1(), e)).unwrap_or_default();
    let mut d = Dialog::custom(
        "createNames",
        "Create Names from Selection",
        json!({"range": range, "top": e.top, "left": e.left, "bottom": e.bottom, "right": e.right, "replace": []}),
    );
    d.command = Some("formulas.createFromSelection");
    d
}

/// The names OK would make that already exist and haven't been answered yet, in order.
fn unanswered_existing(app: &SheetApp, d: &Dialog) -> Vec<String> {
    use gridcraft_engine::cmd::formulas::{LabelEdges, selection_names, workbook_name_exists};
    let Some(doc) = app.session.active() else { return Vec::new() };
    let Some(r) = d.values.get("range").and_then(Json::as_str).and_then(gridcraft_engine::core::RangeRef::parse) else { return Vec::new() };
    let on = |k: &str| d.values.get(k).and_then(Json::as_bool) == Some(true);
    let edges = LabelEdges { top: on("top"), left: on("left"), bottom: on("bottom"), right: on("right") };
    let answered = |n: &str| {
        ["replace", "keep"].iter().any(|k| {
            d.values.get(*k).and_then(Json::as_array).is_some_and(|a| a.iter().any(|x| x.as_str().is_some_and(|x| x.eq_ignore_ascii_case(n))))
        })
    };
    let mut out: Vec<String> = Vec::new();
    for (n, _) in selection_names(&doc.wb, doc.wb.active_sheet(), r, edges) {
        if workbook_name_exists(&doc.wb, &n) && !answered(&n) && !out.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
            out.push(n);
        }
    }
    out
}

/// Records the answer to "Replace existing definition of 'X'?" in the `replace` or `keep` list.
fn answer(d: &mut Dialog, name: &str, replace: bool) {
    let key = if replace { "replace" } else { "keep" };
    let mut list = d.values.get(key).and_then(Json::as_array).cloned().unwrap_or_default();
    list.push(json!(name));
    d.values.insert(key.into(), Json::Array(list));
}

/// Create Names from Selection: the four edge boxes, then for each name that already exists
/// Excel's question 「既存の名前 'X' を置き換えますか?」 (Yes replaces it, No keeps it, Cancel stops
/// without making any name). OK makes the names in one undo step.
pub(super) fn create_names(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    if let Some(name) = d.values.get("ask").and_then(Json::as_str).map(str::to_string) {
        ui.label(lang.tr("Replace existing definition of '{name}'?").replace("{name}", &name));
        ui.add_space(8.0);
        let mut choice = None;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!(" {} ", lang.tr("Cancel"))).clicked() {
                    *open = false;
                }
                if ui.button(format!(" {} ", lang.tr("No"))).clicked() {
                    choice = Some(false);
                }
                let t = crate::theme::Tokens::get(ui.ctx());
                let yes = ui.add(egui::Button::new(egui::RichText::new(format!("   {}   ", lang.tr("Yes"))).color(t.on_accent)).fill(t.accent));
                if yes.clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    choice = Some(true);
                }
            })
        });
        if let Some(replace) = choice {
            d.values.remove("ask");
            answer(d, &name, replace);
            next_question_or_ok(app, d, confirm);
        }
        return;
    }
    ui.label(lang.tr("Create names from values in the:"));
    for (key, label) in LABEL_EDGES {
        let mut on = d.values.get(key).and_then(Json::as_bool).unwrap_or(false);
        if ui.checkbox(&mut on, lang.tr(label)).changed() {
            d.values.insert(key.into(), json!(on));
        }
    }
    let mut ok = false;
    ok_cancel(ui, &mut ok, open);
    if ok {
        // A new OK asks again: answers belong to the edges they were given for.
        d.values.insert("replace".into(), json!([]));
        d.values.insert("keep".into(), json!([]));
        next_question_or_ok(app, d, confirm);
    }
}

/// Asks about the next existing name, or confirms the dialog when none is left.
fn next_question_or_ok(app: &SheetApp, d: &mut Dialog, confirm: &mut bool) {
    match unanswered_existing(app, d).into_iter().next() {
        Some(n) => {
            d.values.insert("ask".into(), json!(n));
        }
        None => *confirm = true,
    }
}

/// `formulas.createFromSelection` params from the dialog: the range, the four edges and the names
/// answered Yes for (`replace`).
pub(super) fn create_names_params(v: &mut Json) {
    if let Some(m) = v.as_object_mut() {
        m.remove("ask");
        m.remove("keep");
        for (key, _) in LABEL_EDGES {
            let on = m.get(key).and_then(Json::as_bool).unwrap_or(false);
            m.insert(key.into(), json!(on));
        }
    }
}

#[cfg(test)]
mod create_names_tests {
    use super::*;

    fn app(values: Json, existing: &[&str], range: &str) -> SheetApp {
        let mut s = gridcraft_engine::Session::default();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": values})).unwrap();
        for n in existing {
            s.execute("formulas.defineName", json!({"name": n, "refersTo": "=1"})).unwrap();
        }
        s.execute("selection.set", json!({"range": range})).unwrap();
        SheetApp::new(s, Default::default())
    }

    fn names(app: &SheetApp) -> Vec<(String, String)> {
        let mut v: Vec<_> = app.session.active().unwrap().wb.names.iter().map(|n| (n.name.clone(), n.formula.clone())).collect();
        v.sort();
        v
    }

    #[test]
    fn opens_on_the_current_region_with_text_edges_ticked() {
        let mut app = app(json!([["", "Jan", "Feb"], ["North", 1, 2], ["South", 3, 4]]), &[], "B2");
        let d = Dialog::open(&mut app, "createNames", json!({})).unwrap();
        assert_eq!(d.values["range"], "A1:C3");
        let edges: Vec<&Json> = LABEL_EDGES.iter().map(|(k, _)| &d.values[*k]).collect();
        assert_eq!(edges, [&json!(true), &json!(true), &json!(false), &json!(false)]);
    }

    #[test]
    fn ok_asks_about_each_existing_name_and_replaces_the_yes_ones() {
        let mut app = app(json!([["Jan", "Feb", "Mar"], [1, 2, 3]]), &["Jan", "feb"], "A1:C2");
        let mut d = Dialog::open(&mut app, "createNames", json!({})).unwrap();
        let mut confirm = false;
        next_question_or_ok(&app, &mut d, &mut confirm);
        assert_eq!((d.values["ask"].clone(), confirm), (json!("Jan"), false));
        d.values.remove("ask");
        answer(&mut d, "Jan", true);
        next_question_or_ok(&app, &mut d, &mut confirm);
        assert_eq!((d.values["ask"].clone(), confirm), (json!("Feb"), false));
        d.values.remove("ask");
        answer(&mut d, "Feb", false);
        next_question_or_ok(&app, &mut d, &mut confirm);
        assert!(confirm);
        let params = build_params(&d);
        assert_eq!(params, json!({"range": "A1:C2", "top": true, "left": false, "bottom": false, "right": false, "replace": ["Jan"]}));
        app.run(d.command.unwrap(), params).unwrap();
        assert_eq!(names(&app), [("Jan".into(), "Sheet1!$A$2:$A$2".into()), ("Mar".into(), "Sheet1!$C$2:$C$2".into()), ("feb".into(), "1".into())]);
    }
}
