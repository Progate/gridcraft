//! Grouped sheets: formatting and entry act on every selected sheet.

use gridcraft_core::{CellRef, Value};
use serde_json::json;

use crate::Session;

fn three_sheets() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("home.insertSheet", json!({"name": "Feb"})).unwrap();
    s.execute("home.insertSheet", json!({"name": "Mar"})).unwrap();
    s.execute("sheet.activate", json!({"sheet": 0})).unwrap();
    s
}

fn style_on(s: &Session, sheet: &str, a1: &str) -> gridcraft_model::Style {
    let wb = &s.doc().unwrap().wb;
    let sh = wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap();
    wb.styles.get(sh.style_id(CellRef::parse(a1).unwrap())).clone()
}

fn value_on(s: &Session, sheet: &str, a1: &str) -> Value {
    let wb = &s.doc().unwrap().wb;
    wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap().value(CellRef::parse(a1).unwrap())
}

fn sheet_xml(s: &mut Session, n: usize) -> String {
    let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
    let bytes = crate::io::base64_decode(&b64).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name(&format!("xl/worksheets/sheet{n}.xml")).unwrap(), &mut xml).unwrap();
    xml
}

#[test]
fn grouped_formatting_lands_on_every_selected_sheet_in_one_undo_step() {
    let mut s = three_sheets();
    let d = s.doc_mut().unwrap();
    d.saved = d.wb.clone();
    let steps = d.undo.len();
    let r = s.execute("sheet.select", json!({"sheets": ["Sheet1", "Mar"]})).unwrap();
    assert_eq!(s.doc().unwrap().undo.len(), steps, "selecting tabs is no undo step");
    assert_eq!(r["grouped"], true);
    assert_eq!(r["sheets"], json!(["Sheet1", "Mar"]));
    assert!(!s.doc().unwrap().is_dirty(), "selecting tabs is view state");
    s.execute("selection.set", json!({"range": "B2:C3"})).unwrap();
    s.execute("home.bold", json!({})).unwrap();
    s.execute("home.fillColor", json!({"color": "#FFFF00"})).unwrap();
    s.execute("home.numberFormat", json!({"code": "0.00%"})).unwrap();
    for sheet in ["Sheet1", "Mar"] {
        let st = style_on(&s, sheet, "C3");
        assert!(st.font.bold, "{sheet}");
        assert_eq!(st.fill, gridcraft_model::Fill::solid(gridcraft_model::Color::Rgb(0xFFFF00)), "{sheet}");
        assert_eq!(st.num_fmt.as_str(), "0.00%", "{sheet}");
    }
    assert!(!style_on(&s, "Feb", "C3").font.bold, "Feb is not in the group");
    // Bold again turns it off everywhere (the active sheet's state decides, as in Excel).
    s.execute("home.bold", json!({})).unwrap();
    assert!(!style_on(&s, "Sheet1", "B2").font.bold && !style_on(&s, "Mar", "B2").font.bold);
    s.execute("edit.undo", json!({})).unwrap();
    assert!(style_on(&s, "Sheet1", "B2").font.bold && style_on(&s, "Mar", "B2").font.bold);
    // One undo step per command, across the group; the group survives Undo.
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(style_on(&s, "Mar", "B2").num_fmt.as_str(), "General");
    assert!(s.doc().unwrap().wb.is_grouped());
    assert_eq!(s.doc().unwrap().wb.active().unwrap().name, "Sheet1");
}

#[test]
fn grouped_entry_widths_and_merges_reach_every_sheet() {
    let mut s = three_sheets();
    s.execute("sheet.selectAll", json!({})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": "Total"})).unwrap();
    s.execute("home.columnWidth", json!({"cols": "B", "chars": 20})).unwrap();
    s.execute("home.mergeCenter", json!({"range": "A3:C3"})).unwrap();
    for sheet in ["Sheet1", "Feb", "Mar"] {
        assert_eq!(value_on(&s, sheet, "A1"), Value::from("Total"), "{sheet}");
        let wb = &s.doc().unwrap().wb;
        let sh = wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap();
        assert!(sh.cols.get(&1).and_then(|c| c.size).is_some(), "{sheet}");
        assert_eq!(sh.merges.len(), 1, "{sheet}");
    }
    // A call that names its sheet acts on that sheet only.
    s.execute("cell.set", json!({"cell": "Feb!A2", "input": "only"})).unwrap();
    assert_eq!(value_on(&s, "Mar", "A2"), Value::Empty);
}

#[test]
fn switching_to_a_sheet_outside_the_group_ungroups() {
    let mut s = three_sheets();
    s.execute("sheet.select", json!({"sheets": [0, 1], "active": 1})).unwrap();
    assert_eq!(s.doc().unwrap().wb.active_sheet(), 1);
    // Within the group the active sheet moves and the group stays.
    s.execute("sheet.activate", json!({"sheet": 0})).unwrap();
    assert!(s.doc().unwrap().wb.is_grouped());
    s.execute("sheet.activate", json!({"sheet": "Mar"})).unwrap();
    assert!(!s.doc().unwrap().wb.is_grouped());
    s.execute("home.bold", json!({"range": "A1"})).unwrap();
    assert!(!style_on(&s, "Sheet1", "A1").font.bold);
    s.execute("sheet.selectAll", json!({})).unwrap();
    assert_eq!(s.execute("sheet.ungroup", json!({})).unwrap()["grouped"], false);
    assert_eq!(s.execute("sheet.selected", json!({})).unwrap()["sheets"], json!(["Mar"]));
}

#[test]
fn a_protected_sheet_in_the_group_refuses_the_whole_edit() {
    let mut s = three_sheets();
    s.execute("sheet.activate", json!({"sheet": "Feb"})).unwrap();
    s.execute("review.protectSheet", json!({})).unwrap();
    s.execute("sheet.activate", json!({"sheet": "Sheet1"})).unwrap();
    s.execute("sheet.selectAll", json!({})).unwrap();
    assert!(s.execute("cell.set", json!({"cell": "A1", "input": "x"})).is_err());
    assert_eq!(value_on(&s, "Sheet1", "A1"), Value::Empty, "the active sheet is put back too");
    assert!(s.doc().unwrap().undo.is_empty() || s.execute("edit.undo", json!({})).is_ok());
}

#[test]
fn grouped_tabs_and_their_styles_are_saved_and_read_back() {
    let mut s = three_sheets();
    s.execute("sheet.select", json!({"sheets": ["Sheet1", "Feb"]})).unwrap();
    s.execute("home.bold", json!({"range": "A1"})).unwrap();
    s.execute("home.fillColor", json!({"range": "A1", "color": "#FF0000"})).unwrap();
    s.execute("home.numberFormat", json!({"range": "A1", "code": "#,##0.00"})).unwrap();
    let x1 = sheet_xml(&mut s, 1);
    let x2 = sheet_xml(&mut s, 2);
    let x3 = sheet_xml(&mut s, 3);
    assert!(x1.contains("<sheetView tabSelected=\"1\""), "{x1}");
    assert!(x2.contains("<sheetView tabSelected=\"1\""), "{x2}");
    assert!(!x3.contains("tabSelected"), "{x3}");
    // The same styled cell on both grouped sheets, none on the third.
    let a1 = |x: &str| x.split("<c r=\"A1\"").nth(1).map(|t| t.split('>').next().unwrap_or("").to_string());
    let s1 = a1(&x1).unwrap();
    assert!(s1.contains(" s=\""), "{x1}");
    assert_eq!(a1(&x2), Some(s1));
    assert_eq!(a1(&x3), None);
    let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
    s.execute("file.open", json!({"name": "g.xlsx", "base64": b64})).unwrap();
    assert_eq!(s.execute("sheet.selected", json!({})).unwrap()["sheets"], json!(["Sheet1", "Feb"]));
    let st = style_on(&s, "Feb", "A1");
    assert!(st.font.bold);
    assert_eq!(st.num_fmt.as_str(), "#,##0.00");
}
