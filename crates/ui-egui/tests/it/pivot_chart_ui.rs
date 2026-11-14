//! PivotCharts and the newer charts from the ribbon and the Format pane, as Excel offers them:
//! PivotChart on Insert and PivotTable Analyze (Insert Chart, or Create PivotChart outside a
//! PivotTable), Expand / Collapse Field, a PivotChart's field buttons on the chart and PivotChart
//! Analyze ▸ Field Buttons, Secondary Vertical axis titles and box-and-whisker quartiles.

use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_engine::model::{ObjKind, ObjRef};
use serde_json::json;

type Harness = crate::common::Harness;

fn pivot_session() -> Session {
    let mut s = crate::support::blank_session();
    s.execute(
        "range.setValues",
        json!({"range": "A1", "values": [["Store", "Item", "Sales"], ["North", "Bread", 10], ["South", "Bread", 7], ["North", "Cake", 4], ["South", "Cake", 9]]}),
    )
    .unwrap();
    s.execute("insert.pivotTable", json!({"source": "Sheet1!A1:C5", "rows": ["Store", "Item"], "values": ["Sales"]})).unwrap();
    s.execute("selection.set", json!({"range": "A4"})).unwrap();
    s
}

fn charts(h: &Harness) -> Vec<gridcraft_engine::model::Chart> {
    h.state().session.active().unwrap().wb.active().unwrap().charts.clone()
}

/// Where a point of chart `id` given in chart coordinates is on the screen.
fn on_chart(h: &Harness, id: u32, x: f32, y: f32) -> egui::Pos2 {
    let app = h.state();
    let sh = app.session.active().unwrap().wb.active().unwrap();
    let geo = gridcraft_ui_egui::grid::Geo::new(sh, app.grid.rect.unwrap(), app.view().scroll);
    let a = sh.charts.iter().find(|c| c.id == id).unwrap().anchor;
    geo.cell_rect(sh, a.cell).min + egui::vec2(a.dx + x, a.dy + y) * geo.z
}

fn click(h: &mut Harness, pos: egui::Pos2) {
    h.input_mut().events.push(egui::Event::PointerMoved(pos));
    h.run_steps(2);
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(2);
}

#[test]
fn pivottable_analyze_makes_a_pivot_chart_and_expands_and_collapses_the_field() {
    let mut h = crate::support::harness(pivot_session(), 4);
    h.state_mut().ui.ribbon_tab = "PivotTable Analyze".into();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "PivotTable Analyze", "a cell of the report shows the tab");
    h.get_by_label("Collapse Field").click();
    h.run_steps(3);
    let collapsed = |h: &Harness| h.state().session.active().unwrap().wb.active().unwrap().pivots[0].rows[0].collapsed_items.len();
    assert_eq!(collapsed(&h), 2, "every Store item");
    h.get_by_label("Expand Field").click();
    h.run_steps(3);
    assert_eq!(collapsed(&h), 0);
    // PivotChart opens Insert Chart on All Charts, as Excel's: Clustered Column is chosen.
    h.get_by_label("PivotChart").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("recommendedCharts"));
    assert!(h.query_by_label("Recommended Charts").is_none(), "a PivotChart's Insert Chart has only All Charts");
    assert!(h.query_by_label("X Y (Scatter)").is_none(), "nor the types a PivotChart can't be");
    assert_eq!(h.state().dialog.as_ref().unwrap().values["type"], "column", "Clustered Column is chosen");
    h.get_by_label("   OK   ").click();
    h.run_steps(3);
    let c = charts(&h);
    assert_eq!(c.len(), 1);
    assert!(c[0].pivot.is_some(), "a PivotChart");
    assert_eq!(c[0].kind, gridcraft_engine::model::ChartKind::ColumnClustered);
    assert_eq!(h.state().selected_chart(), Some(c[0].id), "the new PivotChart is selected");
    // Its axis field button, at the bottom left of the chart, opens the PivotTable's ▼ menu.
    let id = c[0].id;
    let at = on_chart(&h, id, 14.0, c[0].anchor.height - 18.0);
    click(&mut h, at);
    assert_eq!(h.state().grid.pivot_menu.as_ref().map(|m| m.field.as_str()), Some("Store"), "the axis field button of the outer row field");
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "North").click();
    h.run_steps(2);
    h.get_by_label("   OK   ").click();
    h.run_steps(3);
    let pt = h.state().session.active().unwrap().wb.active().unwrap().pivots[0].clone();
    let store = pt.filters.iter().find(|f| f.source_col == "Store").expect("Store is filtered");
    assert_eq!(store.selected.as_deref(), Some(&["South".to_string()][..]));
    // PivotChart Analyze ▸ Field Buttons hides and shows them.
    h.state_mut().selected_object = Some(ObjRef::new(ObjKind::Chart, id));
    h.state_mut().ui.ribbon_tab = "PivotChart Analyze".into();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "PivotChart Analyze", "a selected PivotChart shows the tab");
    h.get_by_label("Field Buttons options").click();
    h.run_steps(3);
    h.get_by_label("Show Axis Field Buttons").click();
    h.run_steps(3);
    assert!(!charts(&h)[0].pivot_buttons.axis);
    assert!(charts(&h)[0].pivot_buttons.legend);
    h.get_by_label("Field Buttons").click();
    h.run_steps(3);
    assert!(!charts(&h)[0].pivot_buttons.any(), "the upper half hides them all");
}

#[test]
fn insert_has_pivotchart_and_a_chart_from_the_report_is_a_pivot_chart() {
    let mut h = crate::support::harness(pivot_session(), 4);
    h.state_mut().ui.ribbon_tab = "Insert".into();
    h.run_steps(3);
    h.get_by_label("PivotChart").click();
    h.run_steps(3);
    h.get_by_label("Line").click();
    h.run_steps(3);
    h.get_by_label("   OK   ").click();
    h.run_steps(3);
    assert!(charts(&h).first().is_some_and(|c| c.pivot.is_some() && c.kind == gridcraft_engine::model::ChartKind::Line));
}

#[test]
fn pivotchart_outside_a_pivot_table_creates_one_with_its_chart() {
    let mut s = crate::support::blank_session();
    s.execute("range.setValues", json!({"range": "A1", "values": [["Store", "Item", "Sales"], ["North", "Bread", 10], ["South", "Cake", 9]]}))
        .unwrap();
    s.execute("selection.set", json!({"range": "B2"})).unwrap();
    let mut h = crate::support::harness(s, 4);
    h.state_mut().ui.ribbon_tab = "Insert".into();
    h.run_steps(3);
    h.get_by_label("PivotChart").click();
    h.run_steps(3);
    let d = h.state().dialog.as_ref().expect("Create PivotChart");
    assert_eq!(d.name(), "createPivotChart");
    assert_eq!(d.values["source"], "Sheet1!$A$1:$C$3", "the block at the active cell");
    h.get_by_label("Existing Worksheet").click();
    h.run_steps(2);
    h.state_mut().dialog.as_mut().unwrap().values.insert("location".into(), json!("Sheet1!$F$2"));
    h.run_steps(2);
    h.get_by_label("   OK   ").click();
    h.run_steps(3);
    let wb = &h.state().session.active().unwrap().wb;
    let sh = wb.active().unwrap();
    assert_eq!(sh.pivots.len(), 1, "a PivotTable on this sheet");
    assert_eq!(sh.pivots[0].anchor, gridcraft_engine::core::CellRef::parse("F2").unwrap());
    assert!(sh.charts.first().is_some_and(|c| c.pivot.is_some() && c.series.is_empty()), "an empty PivotChart of it");
    assert_eq!(h.state().selected_chart(), sh.charts.first().map(|c| c.id));
}

#[test]
fn the_secondary_axis_title_and_box_quartiles_are_in_the_ui() {
    let mut s = crate::support::blank_session();
    s.execute("range.setValues", json!({"range": "A1", "values": [["M", "Sales", "Guests"], ["Apr", 100, 10], ["May", 120, 14]]})).unwrap();
    let id = s.execute("insert.chart", json!({"range": "A1:C3", "type": "column"})).unwrap()["chart"].as_u64().unwrap() as u32;
    s.execute("chart.set", json!({"combo": [{}, {"type": "line", "secondary": true}]})).unwrap();
    let mut h = crate::support::harness(s, 4);
    h.state_mut().selected_object = Some(ObjRef::new(ObjKind::Chart, id));
    h.state_mut().ui.ribbon_tab = "Chart Design".into();
    h.run_steps(3);
    h.get_by_label("Add Chart Element").click();
    h.run_steps(3);
    h.get_by_label("Axis Titles").click();
    h.run_steps(3);
    h.get_by_label("Secondary Vertical").click();
    h.run_steps(3);
    assert_eq!(charts(&h)[0].y2_title.as_deref(), Some("Axis Title"));
    // Box and whisker: Quartile Calculation in the Format pane.
    h.state_mut().run("chart.set", json!({"chart": id, "type": "boxWhisker"})).unwrap();
    h.state_mut().grid.pane = Some("formatChart".into());
    h.run_steps(3);
    h.get_by_label("Quartile Calculation").click();
    h.run_steps(3);
    h.get_by_label("Inclusive median").click();
    h.run_steps(3);
    assert!(charts(&h)[0].series.iter().all(|s| s.quartiles == gridcraft_engine::model::QuartileMethod::Inclusive));
}
