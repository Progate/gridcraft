//! Formulas ▸ Create from Selection opens Excel's Create Names from Selection (選択範囲から名前を
//! 作成) with its Top row, Left column, Bottom row and Right column boxes; OK makes the names.

use egui_kittest::kittest::Queryable;
use serde_json::json;

#[test]
fn create_from_selection_asks_for_the_edges_and_makes_names_from_the_top_row() {
    let mut s = crate::support::blank_session();
    let rows = json!([["Fruit", "Veg", "Meat"], ["Apple", "Carrot", "Beef"], ["Orange", "Onion", "Pork"]]);
    s.execute("range.setValues", json!({"range": "F1", "values": rows})).unwrap();
    s.execute("selection.set", json!({"range": "F1:H3"})).unwrap();
    let mut h = crate::support::harness(s, 4);
    h.state_mut().ui.ribbon_tab = "Formulas".into();
    h.run_steps(3);
    h.get_by_label("Create from Selection").click();
    h.run_steps(4);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name.as_str()), Some("createNames"));
    for label in ["Top row", "Left column", "Bottom row", "Right column"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    // Excel ticks the top row and the left column when both hold text: leave just the top row.
    let values = &h.state().dialog.as_ref().unwrap().values;
    assert_eq!((values["top"].as_bool(), values["left"].as_bool()), (Some(true), Some(true)));
    h.get_by_label("Left column").click();
    h.run_steps(2);
    h.get_by_label("   OK   ").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none(), "{:?}", h.state().message);
    let wb = &h.state().session.active().unwrap().wb;
    let mut names: Vec<(&str, &str)> = wb.names.iter().map(|n| (n.name.as_str(), n.formula.as_str())).collect();
    names.sort();
    assert_eq!(names, [("Fruit", "Sheet1!$F$2:$F$3"), ("Meat", "Sheet1!$H$2:$H$3"), ("Veg", "Sheet1!$G$2:$G$3")]);
    let ja = gridcraft_ui_egui::i18n::Language::Ja;
    assert_eq!(
        ["Create Names from Selection", "Top row", "Left column", "Bottom row", "Right column"].map(|l| ja.tr(l)),
        ["選択範囲から名前を作成", "上端行", "左端列", "最下行", "右端列"]
    );
}
