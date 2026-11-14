//! The PivotTable contextual tabs (PivotTable Analyze's added groups, Design), the ▼ menu of a
//! report's headers with Label and Value Filters, slicers from the ribbon, and reordering fields
//! in the PivotTable Fields pane.

use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use serde_json::json;

type Harness = crate::common::Harness;

fn pivot_session() -> Session {
    let mut s = crate::support::blank_session();
    s.execute(
        "range.setValues",
        json!({"range": "A1", "values": [["Store", "Item", "Sales"], ["North", "Bread", 10], ["South", "Bread", 7], ["North", "Cake", 4], ["South", "Cake", 9], ["East", "Cake", 1]]}),
    )
    .unwrap();
    s.execute("insert.pivotTable", json!({"source": "Sheet1!A1:C6", "rows": ["Store", "Item"], "values": ["Sales"]})).unwrap();
    s.execute("selection.set", json!({"range": "A4"})).unwrap();
    s
}

fn pt(h: &Harness) -> gridcraft_engine::model::PivotTable {
    h.state().session.active().unwrap().wb.active().unwrap().pivots[0].clone()
}

fn click(h: &mut Harness, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

#[test]
fn design_tab_sets_subtotals_layout_blank_rows_and_style() {
    let mut h = crate::support::harness(pivot_session(), 4);
    h.state_mut().ui.ribbon_tab = "PivotTable Design".into();
    // The fields pane has its own Report Layout box.
    h.state_mut().grid.pivot_list_hidden = true;
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "PivotTable Design", "a cell of the report shows Design");
    click(&mut h, "Subtotals");
    click(&mut h, "Do Not Show Subtotals");
    assert!(!pt(&h).subtotals);
    click(&mut h, "Report Layout");
    click(&mut h, "Show in Tabular Form");
    assert_eq!(pt(&h).layout, gridcraft_engine::model::PivotLayout::Tabular);
    click(&mut h, "Report Layout");
    click(&mut h, "Repeat All Item Labels");
    assert!(pt(&h).repeat_labels);
    click(&mut h, "Blank Rows");
    click(&mut h, "Insert Blank Line after Each Item");
    assert!(pt(&h).blank_rows);
    click(&mut h, "Grand Totals");
    click(&mut h, "On for Columns Only");
    assert!(!pt(&h).grand_totals_rows && pt(&h).grand_totals_cols);
}

#[test]
fn pivottable_analyze_shows_options_and_inserts_a_slicer() {
    let mut h = crate::support::harness(pivot_session(), 4);
    h.state_mut().ui.ribbon_tab = "PivotTable Analyze".into();
    h.run_steps(3);
    click(&mut h, "+/- Buttons");
    assert!(!pt(&h).show_expand_buttons);
    click(&mut h, "Field Headers");
    assert!(!pt(&h).show_headers);
    click(&mut h, "Field List");
    assert!(h.state().grid.pivot_list_hidden);
    click(&mut h, "Insert Slicer");
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name.clone()).as_deref(), Some("insertSlicer"));
    click(&mut h, "Item");
    click(&mut h, "   OK   ");
    let sh = h.state().session.active().unwrap().wb.active().unwrap().clone();
    assert_eq!(sh.slicers.len(), 1);
    assert_eq!(sh.slicers[0].pivot, Some(pt(&h).id));
}

#[test]
fn the_header_menu_filters_items_and_opens_label_filters() {
    let mut h = crate::support::harness(pivot_session(), 4);
    h.run_steps(2);
    // The ▼ of 行ラベル (Row Labels) on A3.
    let buttons = gridcraft_ui_egui::pivot_ui::sheet_buttons(h.state_mut());
    let rows = buttons.iter().find(|b| matches!(b.button, gridcraft_engine::pivot::PivotButton::Field { .. })).cloned().expect("a ▼");
    assert_eq!(rows.cell.a1(), "A3");
    assert!(buttons.iter().any(|b| matches!(b.button, gridcraft_engine::pivot::PivotButton::Expand { .. })), "＋/－ of each store");
    gridcraft_ui_egui::pivot_ui::press(h.state_mut(), &rows, egui::pos2(300.0, 200.0), false);
    h.run_steps(3);
    // The menu's checkbox, not the report's cell.
    h.get_all_by_label("East").last().unwrap().click();
    h.run_steps(3);
    click(&mut h, "   OK   ");
    let f = pt(&h).filters.iter().find(|f| f.source_col == "Store").cloned().expect("Store filtered");
    assert_eq!(f.selected, Some(vec!["North".to_string(), "South".to_string()]));
    gridcraft_ui_egui::pivot_ui::press(h.state_mut(), &rows, egui::pos2(300.0, 200.0), false);
    h.run_steps(3);
    click(&mut h, "Label Filters");
    click(&mut h, "Begins With...");
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name.clone()).as_deref(), Some("pivotLabelFilter"));
}

#[test]
fn the_fields_pane_moves_a_field_up() {
    let mut h = crate::support::harness(pivot_session(), 4);
    h.run_steps(3);
    click(&mut h, "Item ▾");
    click(&mut h, "Move Up");
    let rows: Vec<String> = pt(&h).rows.iter().map(|f| f.source_col.clone()).collect();
    assert_eq!(rows, ["Item", "Store"]);
}
