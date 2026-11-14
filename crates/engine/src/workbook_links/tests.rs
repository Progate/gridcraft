use std::path::PathBuf;

use gridcraft_core::{CellRef, Value};
use serde_json::json;

use crate::Session;

/// A fresh folder for one test's workbooks.
fn folder(tag: &str) -> PathBuf {
    let nonce = std::time::SystemTime::UNIX_EPOCH.elapsed().map(|d| d.as_nanos()).unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("gridcraft-links-{tag}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Saves `予算.xlsx` (sheet 予算, B3 = budget, B4 = 30) in `dir`, then closes it.
fn save_budget(dir: &std::path::Path, file: &str, budget: f64) {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("sheet.rename", json!({"name": "予算"})).unwrap();
    s.execute("range.setValues", json!({"range": "A3", "values": [["家賃", budget], ["光熱費", 30]]})).unwrap();
    s.execute("file.saveAs", json!({"path": dir.join(file)})).unwrap();
}

/// A workbook saved as `月次決算.xlsx` in `dir`.
fn monthly(dir: &std::path::Path) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("file.saveAs", json!({"path": dir.join("月次決算.xlsx")})).unwrap();
    s
}

fn value(s: &Session, a1: &str) -> Value {
    s.doc().unwrap().wb.active().unwrap().value(CellRef::parse(a1).unwrap())
}

fn input(s: &Session, a1: &str) -> String {
    s.doc().unwrap().wb.active().unwrap().input_text(CellRef::parse(a1).unwrap())
}

#[test]
fn a_typed_reference_reads_the_workbook_beside_this_one() {
    let dir = folder("typed");
    save_budget(&dir, "予算.xlsx", 120.0);
    save_budget(&dir, "予算 2025.xlsx", 99.0);
    let mut s = monthly(&dir);
    s.execute("cell.set", json!({"cell": "B2", "input": "=[予算.xlsx]予算!B3"})).unwrap();
    assert_eq!(input(&s, "B2"), "=[予算.xlsx]予算!B3");
    assert_eq!(value(&s, "B2"), Value::number(120.0));
    s.execute("cell.set", json!({"cell": "B3", "input": "='[予算 2025.xlsx]予算'!B3*2"})).unwrap();
    assert_eq!(value(&s, "B3"), Value::number(198.0));
    s.execute("cell.set", json!({"cell": "B4", "input": "=SUM([予算.xlsx]予算!B3:B4)"})).unwrap();
    assert_eq!(value(&s, "B4"), Value::number(150.0));
    // The folder written out, as Excel shows a closed workbook's reference.
    let abs = format!("='{}/[予算.xlsx]予算'!$B$4", dir.display());
    s.execute("cell.set", json!({"cell": "B5", "input": abs})).unwrap();
    assert_eq!(value(&s, "B5"), Value::number(30.0));
    let links = s.execute("data.editLinks", json!({})).unwrap();
    let names: Vec<&str> = links["links"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["予算.xlsx", "予算 2025.xlsx"], "{links}");
    assert_eq!(links["links"][0]["formulas"], 3);
    // Each entry is one undo step, the link it made included.
    for _ in 0..3 {
        s.execute("edit.undo", json!({})).unwrap();
    }
    assert_eq!(input(&s, "B3"), "");
    assert_eq!(s.doc().unwrap().wb.external_links.len(), 1);
}

#[test]
fn a_workbook_that_cannot_be_found_is_ref_and_an_open_one_is_read_live() {
    let dir = folder("open");
    let mut s = monthly(&dir);
    s.execute("cell.set", json!({"cell": "A1", "input": "=[予算.xlsx]予算!B3"})).unwrap();
    assert_eq!(value(&s, "A1"), Value::Error(gridcraft_core::CellError::Ref));
    // 予算.xlsx open in the same session (not saved anywhere): Refresh reads its window.
    let main = s.active_index();
    s.new_workbook();
    s.execute("sheet.rename", json!({"name": "予算"})).unwrap();
    s.execute("cell.set", json!({"cell": "B3", "input": "500"})).unwrap();
    s.active_mut().unwrap().title = "予算.xlsx".into();
    s.set_active(main);
    let r = s.execute("data.updateLinks", json!({})).unwrap();
    assert_eq!(r["links"][0]["status"], "OK", "{r}");
    assert_eq!(value(&s, "A1"), Value::number(500.0));
}

#[test]
fn refresh_reads_a_changed_file_and_change_source_and_break_link_work() {
    let dir = folder("update");
    save_budget(&dir, "予算.xlsx", 120.0);
    save_budget(&dir, "予算2026.xlsx", 200.0);
    let mut s = monthly(&dir);
    s.execute("cell.set", json!({"cell": "A1", "input": "=[予算.xlsx]予算!B3+1"})).unwrap();
    assert_eq!(value(&s, "A1"), Value::number(121.0));
    save_budget(&dir, "予算.xlsx", 150.0);
    assert_eq!(value(&s, "A1"), Value::number(121.0), "the cached value until Refresh");
    s.execute("data.updateLinks", json!({"link": "予算.xlsx"})).unwrap();
    assert_eq!(value(&s, "A1"), Value::number(151.0));
    let r = s.execute("data.changeLinkSource", json!({"link": 1, "source": "予算2026.xlsx"})).unwrap();
    assert_eq!(r["links"][0]["name"], "予算2026.xlsx", "{r}");
    assert_eq!(input(&s, "A1"), "=[予算2026.xlsx]予算!B3+1");
    assert_eq!(value(&s, "A1"), Value::number(201.0));
    let r = s.execute("data.breakLinks", json!({})).unwrap();
    assert_eq!(r["cells"], 1);
    assert_eq!(input(&s, "A1"), "201");
    assert!(s.doc().unwrap().wb.external_links.is_empty());
    assert!(s.execute("data.breakLinks", json!({})).is_err());
    assert!(s.execute("data.changeLinkSource", json!({"link": 3, "source": "x.xlsx"})).is_err());
}

#[test]
fn a_saved_workbook_keeps_the_link_and_its_values() {
    let dir = folder("save");
    save_budget(&dir, "予算.xlsx", 120.0);
    let mut s = monthly(&dir);
    s.execute("cell.set", json!({"cell": "A1", "input": "=[予算.xlsx]予算!B3"})).unwrap();
    s.execute("file.save", json!({})).unwrap();
    // The linked file goes away: the reopened workbook shows the saved values.
    std::fs::remove_file(dir.join("予算.xlsx")).unwrap();
    let mut t = Session::new();
    t.execute("file.open", json!({"path": dir.join("月次決算.xlsx")})).unwrap();
    assert_eq!(input(&t, "A1"), "=[予算.xlsx]予算!B3");
    assert_eq!(value(&t, "A1"), Value::number(120.0));
    t.execute("cell.set", json!({"cell": "A2", "input": "=[予算.xlsx]予算!B3*2"})).unwrap();
    assert_eq!(value(&t, "A2"), Value::number(240.0), "a new formula reads the cached value");
    let r = t.execute("data.updateLinks", json!({})).unwrap();
    assert!(r["links"][0]["status"].as_str().unwrap().starts_with("Error"), "{r}");
    assert_eq!(value(&t, "A1"), Value::number(120.0), "a link that can't be read keeps its values");
}
