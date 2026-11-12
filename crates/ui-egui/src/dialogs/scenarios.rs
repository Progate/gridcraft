//! Data ▸ What-If Analysis ▸ Scenario Manager: the scenarios of the active sheet with their
//! changing cells and comment, and the steps behind its buttons: Add Scenario / Edit Scenario
//! (name, changing cells, comment, protection), Scenario Values (one box per changing cell) and
//! Scenario Summary (result cells). Every change runs an engine command (`data.scenarioAdd`,
//! `data.scenarioDelete`, `data.scenarioShow`, `data.scenarioSummary`).

use gridcraft_engine::core::{CellRef, RangeRef, Value, split_areas, split_sheet};

use super::data::abs;
use super::ref_edit::{RefLabel, SHEET_DELETED, end_pick, home_sheet_deleted, ref_edit};
use super::show::ok_cancel;
use super::{Dialog, Json, SheetApp, json, vec2};

/// Most changing cells a scenario has (Excel's limit; `data.scenarioAdd` enforces it too).
const MAX_CELLS: usize = 32;

/// Opens the Scenario Manager on the active sheet's scenarios.
pub(super) fn open(app: &mut SheetApp) -> Dialog {
    let mut d = Dialog::custom("scenarioManager", "Scenario Manager", json!({"step": "list"}));
    refresh(app, &mut d);
    d
}

/// Reads the scenarios again (after a change, or when the workbook changed under the dialog).
fn refresh(app: &mut SheetApp, d: &mut Dialog) {
    let list = app.session.run("data.scenarioManager", json!({})).ok().and_then(|r| r.get("scenarios").cloned()).unwrap_or(json!([]));
    let n = list.as_array().map_or(0, Vec::len);
    d.list_index = d.list_index.min(n.saturating_sub(1));
    d.result = Some(list);
    let rev = app.session.active().map_or(0, |doc| doc.revision);
    d.values.insert("revision".into(), json!(rev));
}

fn scenarios(d: &Dialog) -> Vec<Json> {
    d.result.as_ref().and_then(Json::as_array).cloned().unwrap_or_default()
}

fn text(v: &Json, key: &str) -> String {
    v.get(key).and_then(Json::as_str).unwrap_or("").to_string()
}

fn value(d: &Dialog, key: &str) -> String {
    d.values.get(key).and_then(Json::as_str).unwrap_or("").to_string()
}

/// The changing cells a reference box names (`$B$2:$B$3,$D$4`), in order, at most
/// [`MAX_CELLS`]; an error names the part that isn't a reference.
fn changing_cells(refs: &str) -> Result<Vec<CellRef>, String> {
    let mut out: Vec<CellRef> = Vec::new();
    for part in split_areas(refs).into_iter().map(str::trim).filter(|p| !p.is_empty()) {
        let (_, body) = split_sheet(part);
        let r = RangeRef::parse(body).ok_or_else(|| format!("Changing cells: `{part}` is not a cell reference."))?;
        if r.count() > MAX_CELLS as u64 {
            return Err(format!("A scenario can have at most {MAX_CELLS} changing cells."));
        }
        for c in r.iter() {
            if !out.contains(&c) {
                out.push(c);
            }
        }
        if out.len() > MAX_CELLS {
            return Err(format!("A scenario can have at most {MAX_CELLS} changing cells."));
        }
    }
    if out.is_empty() {
        return Err("Enter the changing cells.".into());
    }
    Ok(out)
}

/// A cell value as the Scenario Values box shows it: numbers in full, text as it is.
fn value_text(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::Text(t) => t.to_string(),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        _ => String::new(),
    }
}

/// A typed scenario value: a number when it reads as one, else its text.
fn typed_value(t: &str) -> Json {
    match t.trim().parse::<f64>() {
        Ok(n) if n.is_finite() => json!(n),
        _ => json!(t),
    }
}

/// Add… : an empty scenario on the current selection, with Excel's "Created by" comment.
fn begin_add(app: &SheetApp, d: &mut Dialog) {
    let sel = app.session.active().map(|doc| doc.selection.ranges.iter().map(abs).collect::<Vec<_>>().join(",")).unwrap_or_default();
    let user = app.session.prefs.user_name.trim().to_string();
    let comment = if user.is_empty() { String::new() } else { format!("Created by {user}") };
    for (k, v) in [("step", json!("edit")), ("name", json!("")), ("changing", json!(sel)), ("comment", json!(comment))] {
        d.values.insert(k.into(), v);
    }
    d.values.insert("locked".into(), json!(true));
    d.values.insert("hidden".into(), json!(false));
    d.values.remove("old");
    d.values.remove("oldValues");
}

/// Edit… : the selected scenario's settings; its values are offered in the next step.
fn begin_edit(d: &mut Dialog, sc: &Json) {
    let changing = text(sc, "changing").split(',').filter_map(|c| CellRef::parse(c.trim())).map(|c| abs(&RangeRef::cell(c))).collect::<Vec<_>>();
    for (k, v) in [
        ("step", json!("edit")),
        ("name", json!(text(sc, "name"))),
        ("old", json!(text(sc, "name"))),
        ("changing", json!(changing.join(","))),
        ("comment", json!(text(sc, "comment"))),
        ("locked", sc.get("locked").cloned().unwrap_or(json!(true))),
        ("hidden", sc.get("hidden").cloned().unwrap_or(json!(false))),
    ] {
        d.values.insert(k.into(), v);
    }
    // Stored values by cell, so the Scenario Values step shows them rather than the current ones.
    let cells: Vec<String> = text(sc, "changing").split(',').map(|c| c.trim().to_ascii_uppercase()).collect();
    let vals = sc.get("values").and_then(Json::as_array).cloned().unwrap_or_default();
    let old: serde_json::Map<String, Json> = cells
        .into_iter()
        .zip(vals)
        .map(|(c, v)| (c, json!(v.as_f64().map(|n| n.to_string()).or_else(|| v.as_str().map(str::to_string)).unwrap_or_else(|| v.to_string()))))
        .collect();
    d.values.insert("oldValues".into(), Json::Object(old));
}

/// OK in Add/Edit Scenario: checks the name and cells, then fills one box per changing cell with
/// the scenario's stored value (Edit) or the cell's current value (Add).
fn to_values(app: &SheetApp, d: &mut Dialog) -> Result<(), String> {
    let name = value(d, "name").trim().to_string();
    if name.is_empty() {
        return Err("Enter a scenario name.".into());
    }
    let editing = d.values.get("old").and_then(Json::as_str).map(str::to_string);
    let taken = scenarios(d).iter().any(|sc| text(sc, "name").eq_ignore_ascii_case(&name))
        && editing.as_deref().is_none_or(|old| !old.eq_ignore_ascii_case(&name));
    if taken {
        return Err(format!("A scenario named '{name}' already exists on this sheet."));
    }
    let cells = changing_cells(&value(d, "changing"))?;
    let doc = app.session.active().ok_or("No workbook is open.")?;
    let sh = doc.wb.active().ok_or("No workbook is open.")?;
    let old = d.values.get("oldValues").and_then(Json::as_object).cloned().unwrap_or_default();
    let boxes: Vec<Json> = cells
        .iter()
        .map(|c| match old.get(&c.a1()).and_then(Json::as_str) {
            Some(v) => json!(v),
            None => json!(value_text(&sh.value(*c))),
        })
        .collect();
    d.values.insert("cells".into(), json!(cells.iter().map(|c| abs(&RangeRef::cell(*c))).collect::<Vec<_>>()));
    d.values.insert("boxes".into(), Json::Array(boxes));
    d.values.insert("step".into(), json!("values"));
    Ok(())
}

/// OK in Scenario Values: adds the scenario, or replaces the one being edited.
fn save_scenario(app: &mut SheetApp, d: &mut Dialog) -> Result<(), String> {
    let cells = d.values.get("cells").and_then(Json::as_array).cloned().unwrap_or_default();
    let boxes = d.values.get("boxes").and_then(Json::as_array).cloned().unwrap_or_default();
    let changing: Vec<String> = cells.iter().filter_map(Json::as_str).map(|c| c.replace('$', "")).collect();
    let values: Vec<Json> = boxes.iter().map(|b| typed_value(b.as_str().unwrap_or(""))).collect();
    let name = value(d, "name").trim().to_string();
    let mut params = json!({
        "name": name,
        "changing": changing.join(","),
        "values": values,
        "comment": value(d, "comment"),
        "locked": d.values.get("locked").and_then(Json::as_bool).unwrap_or(true),
        "hidden": d.values.get("hidden").and_then(Json::as_bool).unwrap_or(false),
    });
    if let Some(old) = d.values.get("old").and_then(Json::as_str) {
        params["old"] = json!(old);
        params["replace"] = json!(old.eq_ignore_ascii_case(&name));
    }
    app.run("data.scenarioAdd", params)?;
    refresh(app, d);
    d.list_index = scenarios(d).iter().position(|sc| text(sc, "name").eq_ignore_ascii_case(&name)).unwrap_or(0);
    d.values.insert("step".into(), json!("list"));
    Ok(())
}

/// The Scenario Manager's body, by step.
pub(super) fn scenario_manager(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let rev = app.session.active().map_or(0, |doc| doc.revision);
    if d.values.get("revision").and_then(Json::as_u64) != Some(rev) {
        refresh(app, d);
    }
    let step = value(d, "step");
    match step.as_str() {
        "edit" => edit_step(app, ui, d, confirm),
        "values" => values_step(app, ui, d, confirm),
        "summary" => summary_step(app, ui, d, confirm, open),
        _ => list_step(app, ui, d, open),
    }
    // OK inside a step moves between steps; only Close (or the window's own close) ends the dialog.
    *confirm = false;
}

fn back_to_list(d: &mut Dialog) {
    d.values.insert("step".into(), json!("list"));
}

fn list_step(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let list = scenarios(d);
    let selected = list.get(d.list_index).cloned();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(lang.tr("Scenarios:"));
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_size(vec2(220.0, 150.0));
                egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                    if list.is_empty() {
                        ui.weak(lang.tr("No Scenarios defined. Choose Add to add scenarios."));
                    }
                    for (i, sc) in list.iter().enumerate() {
                        let r = ui.selectable_label(i == d.list_index, text(sc, "name"));
                        if r.clicked() {
                            d.list_index = i;
                        }
                        if r.double_clicked() {
                            app.run_or_alert("data.scenarioShow", json!({"name": text(sc, "name")}));
                        }
                    }
                });
            });
        });
        ui.vertical(|ui| {
            ui.add_space(18.0);
            let w = vec2(100.0, 0.0);
            if ui.add(egui::Button::new(lang.tr("Add…")).min_size(w)).clicked() {
                begin_add(app, d);
            }
            if ui.add_enabled(selected.is_some(), egui::Button::new(lang.tr("Delete")).min_size(w)).clicked()
                && let Some(sc) = &selected
            {
                if let Err(e) = app.run("data.scenarioDelete", json!({"name": text(sc, "name")})) {
                    app.message = Some((d.title.clone(), crate::clean_error(&e)));
                }
                refresh(app, d);
            }
            if ui.add_enabled(selected.is_some(), egui::Button::new(lang.tr("Edit…")).min_size(w)).clicked()
                && let Some(sc) = &selected
            {
                begin_edit(d, sc);
            }
            if ui.add_enabled(!list.is_empty(), egui::Button::new(lang.tr("Summary…")).min_size(w)).clicked() {
                let current = app.session.active().map(|doc| abs(&doc.selection.current())).unwrap_or_default();
                d.values.insert("resultCells".into(), json!(current));
                d.values.insert("step".into(), json!("summary"));
            }
        });
    });
    ui.add_space(6.0);
    egui::Grid::new("scenario_details").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
        ui.label(lang.tr("Changing cells:"));
        let changing = selected.as_ref().map(|sc| {
            text(sc, "changing").split(',').filter_map(|c| CellRef::parse(c.trim())).map(|c| abs(&RangeRef::cell(c))).collect::<Vec<_>>().join(",")
        });
        ui.label(changing.unwrap_or_default());
        ui.end_row();
        ui.label(lang.tr("Comment:"));
        ui.label(selected.as_ref().map(|sc| text(sc, "comment")).unwrap_or_default());
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!(" {} ", lang.tr("Close"))).clicked() {
                *open = false;
            }
            if ui.add_enabled(selected.is_some(), egui::Button::new(format!(" {} ", lang.tr("Show")))).clicked()
                && let Some(sc) = &selected
            {
                app.run_or_alert("data.scenarioShow", json!({"name": text(sc, "name")}));
            }
        });
    });
}

fn edit_step(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let editing = d.values.contains_key("old");
    ui.strong(lang.tr(if editing { "Edit Scenario" } else { "Add Scenario" }));
    ui.add_space(4.0);
    let l = ui.label(lang.tr("Scenario name:"));
    let mut name = value(d, "name");
    if ui.add(egui::TextEdit::singleline(&mut name).desired_width(320.0)).labelled_by(l.id).changed() {
        d.values.insert("name".into(), json!(name));
    }
    let l = ui.label(lang.tr("Changing cells:"));
    ref_edit(app, ui, d, "changing", RefLabel::By(l.id), 320.0);
    ui.weak(lang.tr("Ctrl+click cells to select non-adjacent changing cells."));
    let l = ui.label(lang.tr("Comment:"));
    let mut comment = value(d, "comment");
    if ui.add(egui::TextEdit::multiline(&mut comment).desired_rows(3).desired_width(320.0)).labelled_by(l.id).changed() {
        d.values.insert("comment".into(), json!(comment));
    }
    ui.label(lang.tr("Protection"));
    for (key, label) in [("locked", "Prevent changes"), ("hidden", "Hide")] {
        let mut on = d.values.get(key).and_then(Json::as_bool).unwrap_or(false);
        if ui.checkbox(&mut on, lang.tr(label)).changed() {
            d.values.insert(key.into(), json!(on));
        }
    }
    let mut cancel = true;
    ok_cancel(ui, confirm, &mut cancel);
    if !cancel {
        let _ = end_pick(app, d);
        back_to_list(d);
        return;
    }
    if !*confirm {
        return;
    }
    if home_sheet_deleted(app, d) {
        app.message = Some((d.title.clone(), SHEET_DELETED.into()));
        return;
    }
    // The changing cells are on the dialog's own sheet: show it again before reading its values.
    if let Err(e) = end_pick(app, d) {
        app.message = Some((d.title.clone(), crate::clean_error(&e)));
        return;
    }
    if let Err(e) = to_values(app, d) {
        app.message = Some((lang.tr(if editing { "Edit Scenario" } else { "Add Scenario" }).to_string(), e));
    }
}

fn values_step(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    ui.strong(lang.tr("Scenario Values"));
    ui.label(lang.tr("Enter values for each of the changing cells."));
    ui.add_space(4.0);
    let cells: Vec<String> =
        d.values.get("cells").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_str).map(str::to_string).collect()).unwrap_or_default();
    let mut boxes: Vec<String> =
        d.values.get("boxes").and_then(Json::as_array).map(|a| a.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
    boxes.resize(cells.len(), String::new());
    let mut changed = false;
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        egui::Grid::new("scenario_values").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
            for (i, (cell, b)) in cells.iter().zip(boxes.iter_mut()).enumerate() {
                let l = ui.label(format!("{}:  {cell}", i + 1));
                changed |= ui.add(egui::TextEdit::singleline(b).desired_width(180.0)).labelled_by(l.id).changed();
                ui.end_row();
            }
        });
    });
    if changed {
        d.values.insert("boxes".into(), json!(boxes));
    }
    let mut cancel = true;
    ok_cancel(ui, confirm, &mut cancel);
    if !cancel {
        back_to_list(d);
        return;
    }
    if *confirm && let Err(e) = save_scenario(app, d) {
        app.message = Some((lang.tr("Scenario Values").to_string(), crate::clean_error(&e)));
    }
}

fn summary_step(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    ui.strong(lang.tr("Scenario Summary"));
    ui.add_space(4.0);
    ui.label(lang.tr("Report type"));
    let _ = ui.radio(true, lang.tr("Scenario summary"));
    let l = ui.label(lang.tr("Result cells:"));
    ref_edit(app, ui, d, "resultCells", RefLabel::By(l.id), 320.0);
    let mut cancel = true;
    ok_cancel(ui, confirm, &mut cancel);
    if !cancel {
        let _ = end_pick(app, d);
        back_to_list(d);
        return;
    }
    if !*confirm {
        return;
    }
    if home_sheet_deleted(app, d) {
        app.message = Some((d.title.clone(), SHEET_DELETED.into()));
        return;
    }
    if let Err(e) = end_pick(app, d) {
        app.message = Some((d.title.clone(), crate::clean_error(&e)));
        return;
    }
    // Excel closes the Scenario Manager once the summary sheet is made.
    // The report is written in the interface language, as Excel writes it in its own.
    match app.run("data.scenarioSummary", json!({"resultCells": value(d, "resultCells"), "language": lang.code()})) {
        Ok(_) => *open = false,
        Err(e) => app.message = Some((lang.tr("Scenario Summary").to_string(), crate::clean_error(&e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_inputs() -> SheetApp {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        session.prefs.user_name = "Ana".into();
        let mut app = SheetApp::new(session, Default::default());
        app.run("cell.set", json!({"cell": "B2", "input": "100"})).unwrap();
        app.run("cell.set", json!({"cell": "B3", "input": "0.25"})).unwrap();
        app.run("cell.set", json!({"cell": "B5", "input": "=B2*B3"})).unwrap();
        app.run("selection.set", json!({"range": "B2:B3"})).unwrap();
        app
    }

    #[test]
    fn changing_cells_accept_absolute_areas_and_refuse_more_than_32() {
        assert_eq!(changing_cells("$B$2:$B$3,$D$4").unwrap(), vec![CellRef::new(1, 1), CellRef::new(2, 1), CellRef::new(3, 3)]);
        assert_eq!(changing_cells("Sheet1!B2").unwrap(), vec![CellRef::new(1, 1)]);
        assert!(changing_cells("A1:A33").is_err());
        assert!(changing_cells("nope").is_err());
        assert!(changing_cells("").is_err());
    }

    #[test]
    fn add_then_edit_a_scenario_through_its_steps() {
        let mut app = app_with_inputs();
        let mut d = open(&mut app);
        assert!(scenarios(&d).is_empty());
        begin_add(&app, &mut d);
        assert_eq!(value(&d, "changing"), "$B$2:$B$3");
        assert_eq!(value(&d, "comment"), "Created by Ana");
        d.values.insert("name".into(), json!("Best"));
        to_values(&app, &mut d).unwrap();
        assert_eq!(d.values["boxes"], json!(["100", "0.25"]), "current values prefill the boxes");
        d.values.insert("boxes".into(), json!(["150", "0.5"]));
        save_scenario(&mut app, &mut d).unwrap();
        assert_eq!(value(&d, "step"), "list");
        let list = scenarios(&d);
        assert_eq!(text(&list[0], "name"), "Best");
        assert_eq!(list[0]["values"], json!([150.0, 0.5]));
        assert_eq!(list[0]["locked"], true);
        // A second Add with the same name is refused before the values step.
        begin_add(&app, &mut d);
        d.values.insert("name".into(), json!("best"));
        assert!(to_values(&app, &mut d).is_err());
        // Edit keeps the stored values in the boxes and can rename.
        begin_edit(&mut d, &list[0]);
        assert_eq!(value(&d, "changing"), "$B$2,$B$3");
        d.values.insert("name".into(), json!("Best case"));
        to_values(&app, &mut d).unwrap();
        assert_eq!(d.values["boxes"], json!(["150", "0.5"]));
        save_scenario(&mut app, &mut d).unwrap();
        let list = scenarios(&d);
        assert_eq!(list.len(), 1);
        assert_eq!(text(&list[0], "name"), "Best case");
        app.run("data.scenarioShow", json!({"name": "Best case"})).unwrap();
        let sh = app.session.active().unwrap().wb.active().unwrap().value(CellRef::new(4, 1));
        assert_eq!(sh, Value::Number(75.0));
    }
}
