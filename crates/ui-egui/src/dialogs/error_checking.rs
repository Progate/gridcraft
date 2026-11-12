//! Formulas › Error Checking, laid out as Excel's: the cell and its formula, the broken rule's
//! name and description, the rule's fix, Help on This Error, Ignore Error and Edit in Formula
//! Bar beside them, and Options…, Previous and Next below. It walks the active sheet from the
//! active cell, row by row.

use gridcraft_engine::core::CellRef;

use super::{Dialog, Json, json};
use crate::SheetApp;

/// Excel's message once every cell of the sheet has been looked at.
pub(crate) const COMPLETE: &str = "The error check is complete for the entire sheet.";

/// Lists the active sheet's errors into the dialog and moves to the first at or after `from`
/// (row by row), selecting its cell. Returns false when the sheet has none left.
pub(super) fn refresh(app: &mut SheetApp, d: &mut Dialog, from: Option<CellRef>) -> bool {
    let sheet = app.session.active().map(|doc| doc.wb.active_sheet()).unwrap_or_default();
    d.result = app.session.run("formulas.errorChecking", json!({"sheet": sheet})).ok();
    let cells: Vec<CellRef> = errors(d).iter().filter_map(|e| e["cell"].as_str().and_then(CellRef::parse)).collect();
    if cells.is_empty() {
        return false;
    }
    let from = from.or_else(|| app.session.active().map(|doc| doc.selection.active)).unwrap_or_default();
    d.list_index = cells.iter().position(|c| (c.row, c.col) >= (from.row, from.col)).unwrap_or(0);
    select_current(app, d);
    true
}

fn errors(d: &Dialog) -> Vec<Json> {
    d.result.as_ref().and_then(|r| r["errors"].as_array()).cloned().unwrap_or_default()
}

fn select_current(app: &mut SheetApp, d: &Dialog) {
    if let Some(cell) = errors(d).get(d.list_index).and_then(|e| e["cell"].as_str()) {
        let _ = app.session.run("selection.set", json!({"range": cell}));
        app.grid.ensure_visible = true;
    }
}

/// Draws the dialog; `open` is cleared when it closes.
pub(super) fn error_checking(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let list = errors(d);
    let Some(e) = list.get(d.list_index).cloned() else {
        ui.label(lang.tr(COMPLETE));
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("  OK  ").clicked() {
                *open = false;
            }
        });
        return;
    };
    let cell = e["cell"].as_str().unwrap_or_default().to_string();
    let at = CellRef::parse(&cell);
    let sheet = e["sheet"].as_str().unwrap_or_default();
    let title = e["title"].as_str().unwrap_or_default().to_string();
    let description = e["description"].as_str().unwrap_or_default().to_string();
    let rule = e["rule"].as_str().unwrap_or_default().to_string();
    let fix = e["fixes"]
        .as_array()
        .and_then(|f| f.first())
        .map(|f| (f["id"].as_str().unwrap_or_default().to_string(), f["label"].as_str().unwrap_or_default().to_string()));
    let mut action: Option<&str> = None;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(300.0);
            ui.label(lang.tr("Error in cell {cell}").replace("{cell}", &format!("{}!{cell}", gridcraft_engine::formula::quote_sheet(sheet))));
            if let Some(f) = e["formula"].as_str() {
                ui.monospace(f);
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new(lang.tr(&title)).strong());
            ui.add(egui::Label::new(lang.tr(&description)).wrap());
        });
        ui.vertical(|ui| {
            let w = 190.0;
            if let Some((_, label)) = &fix
                && ui.add_sized([w, 22.0], egui::Button::new(lang.tr(label))).clicked()
            {
                action = Some("fix");
            }
            for (id, label) in [("help", "Help on This Error"), ("ignore", "Ignore Error"), ("edit", "Edit in Formula Bar")] {
                if ui.add_sized([w, 22.0], egui::Button::new(lang.tr(label))).clicked() {
                    action = Some(id);
                }
            }
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(lang.tr("Options…")).clicked() {
            action = Some("options");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(lang.tr("Next")).clicked() {
                action = Some("next");
            }
            if ui.add_enabled(d.list_index > 0, egui::Button::new(lang.tr("Previous"))).clicked() {
                action = Some("previous");
            }
        });
    });
    match action {
        Some("fix") => {
            select_current(app, d);
            if let Some((id, _)) = &fix {
                crate::grid::run_error_fix(app, id);
            }
            if !refresh(app, d, at) {
                d.result = None;
            }
        }
        Some("help") => app.message = Some((title, description)),
        Some("ignore") => {
            app.run_or_alert("formulas.ignoreError", json!({"range": cell, "rule": rule}));
            if !refresh(app, d, at) {
                d.result = None;
            }
        }
        Some("edit") => {
            select_current(app, d);
            *open = false;
            app.begin_edit(None, true);
        }
        Some("options") => {
            *open = false;
            app.open_backstage(crate::backstage::Page::Options);
        }
        Some("next") => {
            if d.list_index + 1 < list.len() {
                d.list_index += 1;
                select_current(app, d);
            } else {
                d.result = None;
            }
        }
        Some("previous") => {
            d.list_index = d.list_index.saturating_sub(1);
            select_current(app, d);
        }
        _ => {}
    }
}
