//! Data ▸ Subtotal opens Excel's 集計の設定 dialog: the list's headers in At each change in and
//! Add subtotal to, Replace current subtotals on, and Remove All. Group on a block of cells asks
//! for rows or columns.

use egui::vec2;
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use gridcraft_ui_egui::control::{Outcome, handle};
use serde_json::{Value as Json, json};

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A store expense list sorted by store, `#,##0` amounts, the active cell inside it.
fn harness() -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "range.setValues",
        json!({"range": "A1", "values": [["Store", "Item", "Jan"], ["East", "Rent", 1000000], ["East", "Pay", 402927], ["West", "Rent", 2000]]}),
    )
    .unwrap();
    s.execute("home.numberFormat", json!({"range": "C2:C4", "code": "#,##0"})).unwrap();
    s.execute("selection.set", json!({"range": "B2"})).unwrap();
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

fn shown(h: &Harness, a1: &str) -> String {
    let doc = h.state().session.active().unwrap();
    gridcraft_engine::display::cell_text(&doc.wb, doc.wb.active().unwrap(), CellRef::parse(a1).unwrap())
}

#[test]
fn the_dialog_lists_the_headers_and_ok_subtotals_by_them() {
    let mut h = harness();
    h.get_by_label("Subtotal").click();
    h.run_steps(4);
    let d = h.state().dialog.as_ref().expect("the Subtotal dialog");
    assert_eq!(d.name(), "subtotal");
    assert_eq!(d.values["fields"], json!(["Store", "Item", "Jan"]));
    assert_eq!(d.values["columns"], json!([2]), "the last column is ticked, as in Excel");
    assert_eq!((d.values["replace"].as_bool(), d.values["summaryBelow"].as_bool()), (Some(true), Some(true)));
    // The headers are the choices.
    h.get_by_label("Replace current subtotals");
    h.get_by_label("Page break between groups");
    h.get_by_label("Summary below data");
    h.get_by_label("Remove All");
    h.get_by_label("   OK   ").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none(), "{:?}", h.state().message);
    assert_eq!(input(&h, "A4"), "East Total");
    assert_eq!(input(&h, "C4"), "=SUBTOTAL(9,C2:C3)");
    assert_eq!(shown(&h, "C4"), "1,402,927");
    assert_eq!(input(&h, "A7"), "Grand Total");
    // Again with the same settings: the subtotals are replaced, not stacked.
    control(&mut h, "ui.dialog", json!({"name": "subtotal"}));
    h.run_steps(3);
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(input(&h, "A4"), "East Total");
    assert_eq!(input(&h, "A7"), "Grand Total");
    assert_eq!(input(&h, "A8"), "");
}

#[test]
fn remove_all_takes_the_subtotals_away() {
    let mut h = harness();
    h.state_mut().run_or_alert("data.subtotal", json!({"groupBy": "Store", "columns": ["Jan"]}));
    h.run_steps(2);
    assert_eq!(input(&h, "A7"), "Grand Total");
    control(&mut h, "ui.dialog", json!({"name": "subtotal"}));
    h.run_steps(3);
    h.get_by_label("Remove All").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    assert_eq!(input(&h, "A4"), "West");
    assert_eq!(input(&h, "A5"), "");
}

#[test]
fn grouping_a_block_of_cells_asks_rows_or_columns() {
    let mut h = harness();
    control(&mut h, "engine.execute", json!({"command": "selection.set", "params": {"range": "B2:C3"}}));
    h.get_by_label("Group").click();
    h.run_steps(4);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("groupLines"));
    h.get_by_label("Columns").click();
    h.run_steps(2);
    h.get_by_label("   OK   ").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    let sh = h.state().session.active().unwrap().wb.active().unwrap().clone();
    assert_eq!(sh.cols.get(&1).map(|c| c.outline), Some(1), "column B is grouped");
    assert_eq!(sh.cols.get(&2).map(|c| c.outline), Some(1));
    assert!(sh.rows.values().all(|r| r.outline == 0), "no rows are grouped");
    // Whole rows group straight away.
    control(&mut h, "engine.execute", json!({"command": "selection.set", "params": {"range": "2:3"}}));
    h.state_mut().run_or_alert("data.group", json!({}));
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().rows.get(&1).map(|r| r.outline), Some(1));
}
