//! Data ▸ Workbook Links opens Excel's Workbook Links (ブックのリンク) pane: the linked workbooks with Refresh,
//! Change source and Break links.

use egui::vec2;
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use gridcraft_engine::model::{ExternalLink, ExternalSheet};
use gridcraft_ui_egui::SheetApp;
use gridcraft_ui_egui::control::{Outcome, handle};
use serde_json::{Value as Json, json};

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A workbook whose A1 reads Budget.xlsx (not on disk; its value cached as 120).
fn harness() -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    let mut cells = std::collections::BTreeMap::new();
    cells.insert(CellRef::new(2, 1), Value::number(120.0));
    let doc = s.active_mut().unwrap();
    std::sync::Arc::make_mut(&mut doc.wb)
        .external_links
        .push(ExternalLink { target: "Budget.xlsx".into(), sheets: vec![ExternalSheet { name: "Plan".into(), cells }] });
    s.execute("cell.set", json!({"cell": "A1", "input": "=[Budget.xlsx]Plan!B3+1"})).unwrap();
    let mut app = crate::common::app(s);
    app.ui.ribbon_tab = "Data".into();
    let mut h = crate::common::builder().with_size(vec2(1900.0, 900.0)).build_ui_state(
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

fn input(h: &Harness, a1: &str) -> String {
    h.state().session.active().unwrap().wb.active().unwrap().input_text(CellRef::parse(a1).unwrap())
}

#[test]
fn the_pane_lists_the_link_and_break_links_keeps_the_values() {
    let mut h = harness();
    assert_eq!(input(&h, "A1"), "=[Budget.xlsx]Plan!B3+1");
    h.get_by_label("Workbook Links").click();
    h.run_steps(4);
    assert_eq!(h.state().grid.pane.as_deref(), Some("workbookLinks"));
    h.get_by_label("Budget.xlsx");
    h.get_by_label("Refresh All");
    // The file isn't there: Refresh reports it and keeps the cached value.
    h.get_by_label("Refresh").click();
    h.run_steps(4);
    h.get_by_label("Error: Source not found");
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::new(0, 0)), Value::number(121.0));
    h.get_by_label("Break links").click();
    h.run_steps(4);
    assert_eq!(input(&h, "A1"), "121");
    h.get_by_label("This workbook has no links to other workbooks.");
}

#[test]
fn change_source_points_the_formulas_at_another_workbook() {
    let mut h = harness();
    h.state_mut().run_or_alert("data.editLinks", json!({}));
    h.run_steps(3);
    h.get_by_label("Change source").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("changeLinkSource"));
    control(&mut h, "ui.dialog.set", json!({"field": "source", "value": "Budget2026.xlsx"}));
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(input(&h, "A1"), "=[Budget2026.xlsx]Plan!B3+1");
}
