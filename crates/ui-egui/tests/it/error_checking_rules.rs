//! Error checking rules beyond numbers stored as text: the warning button offers the rule's fix
//! (Copy Formula from Above for an inconsistent formula), and Formulas › Error Checking walks the
//! sheet's errors with Excel's buttons, Ignore Error saving the rule for the cell.

use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

fn harness(active: &str) -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1:A4", "values": [[1], [2], [3], [4]]})).unwrap();
    for (c, f) in [("B1", "=A1*2"), ("B2", "=A2*2"), ("B3", "=A3*3"), ("B4", "=A4*2"), ("D1", "=1/0")] {
        s.execute("cell.set", json!({"cell": c, "input": f})).unwrap();
    }
    s.execute("selection.set", json!({"cell": active})).unwrap();
    let mut h = crate::common::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        crate::common::app(s),
    );
    h.run_steps(4);
    h
}

fn formula(h: &mut Harness, cell: &str) -> serde_json::Value {
    h.state_mut().session.execute("cell.get", json!({"cell": cell})).unwrap()["formula"].clone()
}

#[test]
fn the_warning_button_copies_an_inconsistent_formula_from_above() {
    let mut h = harness("B3");
    h.get_by_label("Error checking options").click();
    h.run_steps(3);
    assert!(h.query_by_label("Inconsistent Formula").is_some(), "the menu names the problem");
    assert!(h.query_by_label("Ignore Error").is_some() && h.query_by_label("Edit in Formula Bar").is_some());
    h.get_by_label("Copy Formula from Above").click();
    h.run_steps(3);
    assert_eq!(formula(&mut h, "B3"), json!("=A3*2"));
    assert!(h.query_by_label("Error checking options").is_none(), "the button goes once the formula fits");
}

#[test]
fn error_checking_walks_the_sheet_and_ignore_error_is_saved() {
    let mut h = harness("A1");
    h.state_mut().open_dialog("errorChecking", json!({}));
    h.run_steps(4);
    assert!(h.query_by_label("Divide by Zero Error").is_some(), "D1 comes first, row by row");
    h.get_by_label("Next").click();
    h.run_steps(4);
    assert!(h.query_by_label("Inconsistent Formula").is_some(), "then B3");
    assert!(h.query_by_label("Copy Formula from Above").is_some(), "with its fix");
    h.get_by_label("Ignore Error").click();
    h.run_steps(4);
    assert!(h.query_by_label("Divide by Zero Error").is_some(), "Ignore Error moves on, back round to D1");
    let ignored = h.state().session.active().unwrap().wb.sheet(0).unwrap().ignored_errors.clone();
    assert_eq!(ignored.len(), 1);
    assert_eq!(ignored[0].ranges[0].a1(), "B3");
    h.get_by_label("Next").click();
    h.run_steps(4);
    assert!(h.query_by_label("The error check is complete for the entire sheet.").is_some());
}
