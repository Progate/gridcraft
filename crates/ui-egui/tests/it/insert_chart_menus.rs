//! Insert ▸ Charts as in Excel 365: the chart type menus with their sections, the combo presets,
//! the More … Charts items and the dialog launcher, which open Insert Chart, and its Combo rows.

use egui_kittest::kittest::Queryable;
use gridcraft_engine::model::ChartKind;
use serde_json::json;

type Harness = crate::common::Harness;

fn sales() -> Harness {
    let mut s = crate::support::blank_session();
    s.execute(
        "range.setValues",
        json!({"range": "A1", "values": [["Month", "Sales", "Guests"], ["Apr", 100, 10], ["May", 120, 14], ["Jun", 90, 9]]}),
    )
    .unwrap();
    s.execute("selection.set", json!({"range": "A1:C4"})).unwrap();
    let mut h = crate::support::harness(s, 4);
    h.state_mut().ui.ribbon_tab = "Insert".into();
    h.run_steps(3);
    h
}

fn charts(h: &Harness) -> Vec<gridcraft_engine::model::Chart> {
    h.state().session.active().unwrap().wb.active().unwrap().charts.clone()
}

#[test]
fn the_combo_menu_inserts_clustered_column_and_a_line_on_the_secondary_axis() {
    let mut h = sales();
    h.get_by_label("Insert Combo Chart").click();
    h.run_steps(3);
    assert!(h.query_by_label("Clustered Column - Line").is_some());
    assert!(h.query_by_label("Stacked Area - Clustered Column").is_some());
    h.get_by_label("Clustered Column - Line on Secondary Axis").click();
    h.run_steps(3);
    let c = charts(&h);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, ChartKind::Combo);
    assert_eq!(c[0].series[0].kind, Some(ChartKind::ColumnClustered));
    assert_eq!(c[0].series[1].kind, Some(ChartKind::Line));
    assert!(!c[0].series[0].secondary && c[0].series[1].secondary);
}

#[test]
fn every_chart_menu_lists_only_types_gridcraft_makes_and_ends_with_more_charts() {
    let mut h = sales();
    for (button, item) in [
        ("Insert Column or Bar Chart", "More Column Charts…"),
        ("Insert Line or Area Chart", "More Line Charts…"),
        ("Insert Pie or Doughnut Chart", "More Pie Charts…"),
        ("Insert Hierarchy Chart", "More Hierarchy Charts…"),
        ("Insert Statistic Chart", "More Statistical Charts…"),
        ("Insert Scatter (X, Y) or Bubble Chart", "More Scatter Charts…"),
        ("Insert Waterfall, Funnel, Stock, Surface or Radar Chart", "More Stock Charts…"),
        ("Insert Combo Chart", "Create Custom Combo Chart…"),
    ] {
        h.get_by_label(button).click();
        h.run_steps(3);
        assert!(h.query_by_label(item).is_some(), "{button} ends with {item}");
        h.get_by_label(button).click();
        h.run_steps(2);
    }
}

#[test]
fn create_custom_combo_chart_opens_all_charts_on_combo_with_a_row_per_series() {
    let mut h = sales();
    h.get_by_label("Insert Combo Chart").click();
    h.run_steps(3);
    h.get_by_label("Create Custom Combo Chart…").click();
    h.run_steps(4);
    let d = h.state().dialog.as_ref().expect("Insert Chart");
    assert_eq!(d.name(), "recommendedCharts");
    assert_eq!(d.values["category"], "combo");
    assert_eq!(d.values["rows"].as_array().map(Vec::len), Some(2), "{:?}", d.values);
    // Secondary Axis for Guests, then OK.
    h.get_by_label("Secondary Axis Guests").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().unwrap().values["subtype"], "custom");
    h.get_by_label("   OK   ").click();
    h.run_steps(3);
    let c = charts(&h);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, ChartKind::Combo);
    assert!(c[0].series[1].secondary);
}

#[test]
fn the_dialog_launcher_opens_insert_chart_on_recommended_charts() {
    let mut h = sales();
    h.get_by_label("See All Charts").click();
    h.run_steps(3);
    let d = h.state().dialog.as_ref().expect("Insert Chart");
    assert_eq!(d.name(), "recommendedCharts");
    assert_eq!(d.tab, "Recommended Charts");
    h.get_by_label("All Charts").click();
    h.run_steps(3);
    for category in [
        "Column",
        "Line",
        "Pie",
        "Bar",
        "Area",
        "X Y (Scatter)",
        "Stock",
        "Radar",
        "Treemap",
        "Sunburst",
        "Histogram",
        "Box & Whisker",
        "Waterfall",
        "Funnel",
        "Combo",
    ] {
        assert!(h.query_by_label(category).is_some(), "All Charts lists {category}");
    }
}
