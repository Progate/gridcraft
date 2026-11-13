//! Formulas › Evaluate Formula, as Excel's dialog: the reference and the formula with the part it
//! evaluates next underlined; Evaluate replaces that part with its value, Step In opens the
//! formula of the cell the underlined reference names in a box below, Step Out closes that box
//! and puts its value in place, and once the result shows Evaluate reads Restart.

use egui::text::{LayoutJob, TextFormat};
use gridcraft_engine::core::{CellRef, col_to_letters, quote_sheet};

use super::{Dialog, Json, json};
use crate::SheetApp;

/// Most nested boxes Step In opens, as a cell can refer back to itself through others.
const MAX_LEVELS: usize = 16;

/// Starts the dialog on the active cell's formula.
pub(super) fn open(app: &mut SheetApp) -> Dialog {
    let mut d = Dialog::custom("evaluateFormula", "Evaluate Formula", json!({}));
    d.result = app.session.run("formulas.evaluateFormula", json!({})).ok();
    let levels: Vec<Json> = d.result.as_ref().map(level).into_iter().collect();
    d.values.insert("levels".into(), Json::Array(levels));
    d
}

/// A box of the dialog: an `evaluateFormula` reply's states and the one showing.
fn level(r: &Json) -> Json {
    let cell = r["cell"].as_str().and_then(CellRef::parse).unwrap_or_default();
    let sheet = r["sheet"].as_str().map(quote_sheet).unwrap_or_default();
    json!({"walk": r["walk"], "pos": 0, "ref": format!("{sheet}!${}${}", col_to_letters(cell.col), cell.row + 1)})
}

fn levels(d: &Dialog) -> Vec<Json> {
    d.values.get("levels").and_then(Json::as_array).cloned().unwrap_or_default()
}

fn set_levels(d: &mut Dialog, levels: Vec<Json>) {
    d.values.insert("levels".into(), Json::Array(levels));
}

/// The state a box shows.
fn state(level: &Json) -> Json {
    let pos = level["pos"].as_u64().unwrap_or(0) as usize;
    level["walk"].get(pos).cloned().unwrap_or(Json::Null)
}

fn at_end(level: &Json) -> bool {
    state(level)["underline"].is_null()
}

/// Evaluate (Restart once the outer box shows its result) on the innermost box.
pub(super) fn evaluate(d: &mut Dialog) {
    let mut ls = levels(d);
    let Some(top) = ls.last_mut() else { return };
    if at_end(top) {
        if ls.len() == 1 {
            ls[0]["pos"] = json!(0);
        }
    } else {
        top["pos"] = json!(top["pos"].as_u64().unwrap_or(0) + 1);
    }
    set_levels(d, ls);
}

/// Step In: whether the underlined part of the innermost box is a cell that holds a formula.
pub(super) fn can_step_in(d: &Dialog) -> bool {
    let ls = levels(d);
    ls.len() < MAX_LEVELS && ls.last().is_some_and(|l| state(l)["stepIn"].is_object())
}

pub(super) fn step_in(app: &mut SheetApp, d: &mut Dialog) {
    if !can_step_in(d) {
        return;
    }
    let mut ls = levels(d);
    let target = ls.last().map(|l| state(l)["stepIn"].clone()).unwrap_or_default();
    if let Ok(r) = app.session.run("formulas.evaluateFormula", json!({"sheet": target["sheet"], "cell": target["cell"]})) {
        ls.push(level(&r));
        set_levels(d, ls);
    }
}

/// Step Out: closes the innermost box, and the reference it was opened from shows its value.
pub(super) fn step_out(d: &mut Dialog) {
    let mut ls = levels(d);
    if ls.len() < 2 {
        return;
    }
    ls.pop();
    set_levels(d, ls);
    evaluate(d);
}

/// The text of a state with its underlined part underlined.
fn underlined(ui: &egui::Ui, st: &Json) -> LayoutJob {
    let text = st["text"].as_str().unwrap_or_default();
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let color = ui.visuals().text_color();
    let plain = TextFormat { font_id: font.clone(), color, ..Default::default() };
    let marked = TextFormat { font_id: font, color, underline: egui::Stroke::new(1.5, color), ..Default::default() };
    let (start, end) = match st["underline"].as_array() {
        Some(u) => (u[0].as_u64().unwrap_or(0) as usize, u[1].as_u64().unwrap_or(0) as usize),
        None => (0, 0),
    };
    let byte = |chars: usize| text.char_indices().nth(chars).map_or(text.len(), |(i, _)| i);
    let (a, b) = (byte(start), byte(end));
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    job.append(&text[..a], 0.0, plain.clone());
    job.append(&text[a..b], 0.0, marked);
    job.append(&text[b..], 0.0, plain);
    job
}

/// Draws the dialog; `open` is cleared when it closes.
pub(super) fn evaluate_formula(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let ls = levels(d);
    if ls.is_empty() {
        ui.label(lang.tr("The cell has no formula."));
    }
    for (i, l) in ls.iter().enumerate() {
        egui::Grid::new(("evaluate_level", i)).num_columns(2).spacing(egui::vec2(12.0, 4.0)).show(ui, |ui| {
            ui.label(lang.tr("Reference:"));
            ui.label(lang.tr("Evaluation:"));
            ui.end_row();
            ui.monospace(l["ref"].as_str().unwrap_or_default());
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(320.0);
                ui.label(underlined(ui, &state(l)));
            });
            ui.end_row();
        });
        ui.add_space(4.0);
    }
    ui.add_space(4.0);
    let restart = ls.len() == 1 && ls.first().is_some_and(at_end);
    let nested = ls.len() > 1;
    let mut close = false;
    ui.horizontal(|ui| {
        let label = if restart { "Restart" } else { "Evaluate" };
        if ui.add_enabled(!ls.is_empty() && !(nested && ls.last().is_some_and(at_end)), egui::Button::new(lang.tr(label))).clicked() {
            evaluate(d);
        }
        if ui.add_enabled(can_step_in(d), egui::Button::new(lang.tr("Step In"))).clicked() {
            step_in(app, d);
        }
        if ui.add_enabled(nested, egui::Button::new(lang.tr("Step Out"))).clicked() {
            step_out(d);
        }
        if ui.button(lang.tr("Close")).clicked() {
            close = true;
        }
    });
    if close {
        *open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> SheetApp {
        let mut session = gridcraft_engine::Session::new();
        session.new_workbook();
        session.execute("cell.set", json!({"cell": "A1", "input": "=2+3"})).unwrap();
        session.execute("cell.set", json!({"cell": "A2", "input": "4"})).unwrap();
        session.execute("cell.set", json!({"cell": "B1", "input": "=A1*A2"})).unwrap();
        session.execute("selection.set", json!({"cell": "B1"})).unwrap();
        SheetApp::new(session, crate::Services::default())
    }

    fn shown(d: &Dialog) -> Vec<String> {
        levels(d).iter().map(|l| state(l)["text"].as_str().unwrap_or_default().to_string()).collect()
    }

    #[test]
    fn evaluate_steps_one_part_at_a_time_then_restarts() {
        let mut app = app();
        let mut d = open(&mut app);
        assert_eq!(levels(&d)[0]["ref"], "Sheet1!$B$1");
        assert_eq!(shown(&d), ["=A1*A2"]);
        evaluate(&mut d);
        assert_eq!(shown(&d), ["=5*A2"]);
        evaluate(&mut d);
        evaluate(&mut d);
        assert_eq!(shown(&d), ["=20"]);
        evaluate(&mut d);
        assert_eq!(shown(&d), ["=A1*A2"], "Restart");
    }

    #[test]
    fn step_in_opens_the_referenced_formula_and_step_out_puts_its_value() {
        let mut app = app();
        let mut d = open(&mut app);
        assert!(can_step_in(&d), "A1 holds a formula");
        step_in(&mut app, &mut d);
        assert_eq!(shown(&d), ["=A1*A2", "=2+3"]);
        assert_eq!(levels(&d)[1]["ref"], "Sheet1!$A$1");
        evaluate(&mut d);
        assert_eq!(shown(&d), ["=A1*A2", "=5"]);
        step_out(&mut d);
        assert_eq!(shown(&d), ["=5*A2"]);
        assert!(!can_step_in(&d), "A2 is a constant");
    }
}
