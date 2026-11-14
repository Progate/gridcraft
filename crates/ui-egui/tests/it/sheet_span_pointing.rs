//! While a formula points at references, Shift-clicking a sheet tab spans the sheets (a 3-D
//! reference, `East:West!A2`) instead of grouping them, as in Excel.

use egui::{Event, Modifiers, PointerButton, Pos2, Shape};
use serde_json::json;

type Harness = crate::common::Harness;

fn tab_at(h: &Harness, name: &str) -> Pos2 {
    h.output()
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Text(t) if t.galley.job.text == name => Some(t.visual_bounding_rect().center()),
            _ => None,
        })
        .last()
        .unwrap_or_else(|| panic!("tab {name} is drawn"))
}

fn click(h: &mut Harness, at: Pos2, modifiers: Modifiers) {
    h.input_mut().events.push(Event::ModifiersChanged(modifiers));
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerMoved(at));
        h.input_mut().events.push(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers });
        h.step();
    }
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

#[test]
fn shift_clicking_a_tab_while_pointing_spans_the_sheets_instead_of_grouping_them() {
    let mut s = crate::support::blank_session();
    s.execute("sheet.rename", json!({"sheet": 0, "name": "Summary"})).unwrap();
    for name in ["East", "Mid", "West"] {
        s.execute("home.insertSheet", json!({"name": name})).unwrap();
    }
    for i in 1..4 {
        s.execute("sheet.activate", json!({"sheet": i})).unwrap();
        s.execute("cell.set", json!({"cell": "A2", "input": format!("{i}")})).unwrap();
        s.execute("selection.set", json!({"range": "A1"})).unwrap();
    }
    s.execute("sheet.activate", json!({"sheet": 0})).unwrap();
    s.execute("selection.set", json!({"range": "B4"})).unwrap();
    let mut h = crate::support::harness(s, 6);
    h.input_mut().events.push(Event::Text("=SUM(".into()));
    h.run_steps(3);
    assert_eq!(h.state().editor.as_ref().map(|e| e.text.as_str()), Some("=SUM("));
    let east = tab_at(&h, "East");
    click(&mut h, east, Modifiers::NONE);
    let west = tab_at(&h, "West");
    click(&mut h, west, Modifiers::SHIFT);
    assert!(!h.state().session.active().unwrap().wb.is_grouped(), "the sheets are not grouped");
    assert_eq!(h.state().editor.as_ref().map(|e| e.point_span), Some(Some((1, 3))));
    // Clicking A2 names the span.
    let a2 = {
        let app = h.state();
        let sheet = app.session.active().unwrap().wb.active().unwrap();
        let geo = gridcraft_ui_egui::grid::Geo::new(sheet, app.grid.rect.unwrap(), app.view().scroll);
        egui::pos2(geo.x(sheet, 0) + sheet.col_width(0) * geo.z * 0.5, geo.y(sheet, 1) + sheet.row_height(1) * geo.z * 0.5)
    };
    click(&mut h, a2, Modifiers::NONE);
    assert_eq!(h.state().editor.as_ref().map(|e| e.text.as_str()), Some("=SUM(East:West!A2"));
    h.state_mut().editor.as_mut().unwrap().text.push(')');
    assert!(h.state_mut().commit_edit(0, 0, false, false));
    h.run_steps(2);
    let d = h.state().session.active().unwrap();
    let summary = d.wb.sheet(0).unwrap();
    let b4 = gridcraft_engine::core::CellRef::parse("B4").unwrap();
    assert_eq!(d.wb.active_sheet(), 0, "the formula went in on its own sheet");
    assert_eq!(summary.cell(b4).and_then(|c| c.formula.as_ref()).map(|f| f.text.as_str()), Some("SUM(East:West!A2)"));
    assert_eq!(summary.value(b4).display(), "6");
}
