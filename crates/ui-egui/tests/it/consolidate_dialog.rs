//! Data ▸ Consolidate: the ribbon button opens the dialog, Add fills All references, and OK
//! consolidates them at the active cell with the chosen function and labels.

use egui::vec2;
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use gridcraft_ui_egui::control::{Outcome, handle};
use serde_json::{Value as Json, json};

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// Two regional sheets with the same layout in different orders, Sheet1 active at C3.
fn harness() -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    for (name, rows) in [("East", json!([["", "Sales"], ["Pens", 10], ["Ink", 4]])), ("West", json!([["", "Sales"], ["Ink", 6], ["Pens", 2]]))] {
        s.execute("home.insertSheet", json!({})).unwrap();
        s.execute("sheet.rename", json!({"name": name})).unwrap();
        s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    }
    s.execute("sheet.activate", json!({"sheet": "Sheet1"})).unwrap();
    s.execute("selection.set", json!({"range": "C3"})).unwrap();
    let mut app = crate::common::app(s);
    app.ui.ribbon_tab = "Data".into();
    let mut h = crate::common::builder().with_size(vec2(1900.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn control(h: &mut Harness, method: &str, params: Json) -> Json {
    let ctx = h.ctx.clone();
    match handle(h.state_mut(), &ctx, method, &params) {
        Outcome::Done(v) => v,
        Outcome::Screenshot { .. } => json!(null),
    }
}

fn cell(h: &Harness, a1: &str) -> String {
    let doc = h.state().session.active().unwrap();
    let sheet = doc.wb.sheets.iter().find(|s| s.name == "Sheet1").unwrap();
    sheet.value(CellRef::parse(a1).unwrap()).display()
}

#[test]
fn the_ribbon_button_opens_the_dialog_and_ok_consolidates_the_added_references() {
    let mut h = harness();
    h.get_by_label("Consolidate").click();
    h.run_steps(4);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("consolidate"));
    for r in ["East!$A$1:$B$3", "West!$A$1:$B$3"] {
        control(&mut h, "ui.dialog.set", json!({"field": "reference", "value": r}));
        h.run_steps(2);
        h.get_by_label("Add").click();
        h.run_steps(3);
    }
    assert_eq!(h.state().dialog.as_ref().unwrap().values["sources"], json!(["East!$A$1:$B$3", "West!$A$1:$B$3"]));
    // Adding a reference already listed keeps one copy.
    h.get_by_label("Add").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().unwrap().values["sources"].as_array().unwrap().len(), 2);
    h.get_by_label("Top row").click();
    h.get_by_label("Left column").click();
    h.run_steps(3);
    h.get_by_label("   OK   ").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none(), "{:?}", h.state().message);
    assert!(h.state().message.is_none(), "{:?}", h.state().message);
    assert_eq!(cell(&h, "D3"), "Sales");
    assert_eq!(cell(&h, "C4"), "Pens");
    assert_eq!(cell(&h, "D4"), "12");
    assert_eq!(cell(&h, "C5"), "Ink");
    assert_eq!(cell(&h, "D5"), "10");
}

#[test]
fn delete_removes_the_selected_reference_and_the_function_is_sent() {
    let mut h = harness();
    control(&mut h, "ui.dialog", json!({"name": "consolidate"}));
    h.run_steps(3);
    control(&mut h, "ui.dialog.set", json!({"field": "sources", "value": ["East!$B$2:$B$3", "West!$B$2:$B$3", "Sheet1!$Z$1"]}));
    h.run_steps(2);
    h.get_by_label("Sheet1!$Z$1").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().unwrap().values["reference"], "Sheet1!$Z$1", "choosing a reference puts it in the box");
    h.get_by_label("Delete").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().unwrap().values["sources"], json!(["East!$B$2:$B$3", "West!$B$2:$B$3"]));
    control(&mut h, "ui.dialog.set", json!({"field": "function", "value": "max"}));
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    // By position: the first rows (10 and 6) and the second rows (4 and 2).
    assert_eq!(cell(&h, "C3"), "10");
    assert_eq!(cell(&h, "C4"), "4");
}

#[test]
fn the_command_without_sources_opens_the_dialog() {
    let mut h = harness();
    h.state_mut().run_or_alert("data.consolidate", json!({}));
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("consolidate"));
    assert!(h.state().message.is_none());
}
