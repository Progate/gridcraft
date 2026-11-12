//! Right-clicking a number item of a PivotTable report offers Group… (Excel's right-click › Group…),
//! which opens the Grouping dialog on that field; OK groups it into ranges and Ungroup… undoes it.

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::{SheetApp, grid::Geo};
use serde_json::json;

fn harness() -> egui_kittest::Harness<'static, SheetApp> {
    let mut session = Session::new();
    session.new_workbook();
    let rows = json!([["Score", "N"], [3, 1], [18, 1], [42, 1], [98, 1]]);
    session.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    // Report at D3: D4..D7 the scores.
    session.execute("insert.pivotTable", json!({"source": "Sheet1!A1:B5", "destination": "Sheet1!D3", "rows": ["Score"], "values": ["N"]})).unwrap();
    session.execute("selection.set", json!({"cell": "A1"})).unwrap();
    crate::support::stepped_harness(session)
}

fn center(h: &egui_kittest::Harness<'_, SheetApp>, cell: &str) -> Pos2 {
    let app = h.state();
    let sheet = app.session.active().unwrap().wb.active().unwrap();
    let geo = Geo::new(sheet, app.grid.rect.unwrap(), app.view().scroll);
    geo.cell_rect(sheet, CellRef::parse(cell).unwrap()).center()
}

fn right_click(h: &mut egui_kittest::Harness<'_, SheetApp>, cell: &str) {
    let pos = center(h, cell);
    h.input_mut().events.push(Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton { pos, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
    assert!(h.state().grid.context_menu.is_some(), "the cell menu opens on {cell}");
}

#[test]
fn group_in_the_cell_menu_opens_grouping_and_ungroup_restores_the_items() {
    let mut h = harness();
    right_click(&mut h, "D5");
    h.get_by_label("Group…").click();
    h.run_steps(2);
    {
        let app = h.state();
        let d = app.dialog.as_ref().expect("the Grouping dialog opens");
        assert_eq!(d.name(), "pivotGroup");
        assert_eq!(d.values["field"], "Score");
        assert_eq!((d.values["start"].as_str(), d.values["end"].as_str()), (Some("3"), Some("98")));
    }
    h.get_by_label_contains("OK").click();
    h.run_steps(2);
    let text = |h: &egui_kittest::Harness<'_, SheetApp>, a: &str| {
        h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::parse(a).unwrap()).display()
    };
    assert_eq!(text(&h, "D4"), "3-12");
    assert_eq!(text(&h, "D7"), "93-102");

    right_click(&mut h, "D4");
    h.get_by_label("Ungroup…").click();
    h.run_steps(2);
    assert_eq!(text(&h, "D4"), "3");
    assert!(h.state().session.active().unwrap().wb.active().unwrap().pivots[0].rows[0].range_group.is_none());
}
