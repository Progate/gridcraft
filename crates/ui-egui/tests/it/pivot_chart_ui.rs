//! PivotCharts and the newer charts from the ribbon and the Format pane, as Excel offers them:
//! PivotChart on Insert and PivotTable Analyze, Expand / Collapse Field, a PivotChart's field
//! buttons, Secondary Vertical axis titles and box-and-whisker quartiles.

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
    h.get_by_label("PivotChart").click();
    h.run_steps(3);
    h.get_by_label("Clustered Column").click();
    h.run_steps(3);
    let c = charts(&h);
    assert_eq!(c.len(), 1);
    assert!(c[0].pivot.is_some(), "a PivotChart");
    // Its field buttons reach the PivotTable's filter.
    h.state_mut().selected_object = Some(ObjRef::new(ObjKind::Chart, c[0].id));
    h.state_mut().ui.ribbon_tab = "Chart Design".into();
    h.run_steps(3);
    h.get_by_label("Field Buttons").click();
    h.run_steps(3);
    h.get_by_label("Item ▾").click();
    h.run_steps(3);
    h.get_by_label("Filter").click();
    h.run_steps(3);
    h.get_by_label("Cake").click();
    h.run_steps(3);
    let pt = h.state().session.active().unwrap().wb.active().unwrap().pivots[0].clone();
    let item = pt.filters.iter().find(|f| f.source_col == "Item").expect("Item is filtered");
    assert_eq!(item.selected.as_deref(), Some(&["Bread".to_string()][..]));
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
    assert!(charts(&h).first().is_some_and(|c| c.pivot.is_some() && c.kind == gridcraft_engine::model::ChartKind::Line));
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
