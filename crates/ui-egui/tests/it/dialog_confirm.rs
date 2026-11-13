//! `ui.dialog.confirm` runs a dialog's command with the parameters its OK button sends (#1459).

use egui_kittest::kittest::Queryable;
use gridcraft_engine::model::CfRule;
use gridcraft_ui_egui::SheetApp;
use gridcraft_ui_egui::control::{Outcome, handle};
use serde_json::{Value as Json, json};

fn harness() -> egui_kittest::Harness<'static, SheetApp> {
    crate::support::blank_harness(6)
}

fn control(h: &mut egui_kittest::Harness<'static, SheetApp>, method: &str, params: Json) -> Json {
    let ctx = h.ctx.clone();
    match handle(h.state_mut(), &ctx, method, &params) {
        Outcome::Done(v) => v,
        Outcome::Screenshot { .. } => json!(null),
    }
}

#[test]
fn confirming_a_highlight_cells_rule_adds_the_rule_like_ok() {
    let mut h = harness();
    h.state_mut().run("range.setValues", json!({"range": "A1", "values": [[1], [3]]})).unwrap();
    h.state_mut().run("selection.set", json!({"range": "A1:A2"})).unwrap();
    // Home › Conditional Formatting › Highlight Cells Rules › Greater Than…
    control(&mut h, "ui.dialog", json!({"name": "cfQuick", "type": "cellIs", "operator": "greaterThan", "title": "Greater Than"}));
    control(&mut h, "ui.dialog.set", json!({"field": "value", "value": "2"}));
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], json!(true), "{r}");
    assert!(h.state().dialog.is_none(), "the dialog closed");
    let sheet = h.state().session.active().unwrap().wb.active().unwrap();
    assert_eq!(sheet.cond_formats.len(), 1, "{:?}", sheet.cond_formats);
    let rule = &sheet.cond_formats[0].rule;
    assert!(matches!(rule, CfRule::CellIs { a, .. } if a == "2"), "{rule:?}");
}

#[test]
fn confirming_text_to_columns_splits_on_the_ticked_delimiter() {
    let mut h = harness();
    h.state_mut().run("range.setValues", json!({"range": "A1", "values": [["a;b"], ["c;d"]]})).unwrap();
    h.state_mut().run("selection.set", json!({"range": "A1:A2"})).unwrap();
    control(&mut h, "ui.dialog", json!({"name": "textToColumns"}));
    control(&mut h, "ui.dialog.set", json!({"field": "semicolon", "value": true}));
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], json!(true), "{r}");
    let read = h.state_mut().run("sheet.read", json!({"range": "A1:B2"})).unwrap();
    assert_eq!(read["values"], json!([["a", "b"], ["c", "d"]]), "{read}");
}

#[test]
fn text_to_columns_wizard_splits_fixed_width_with_column_formats() {
    let mut h = harness();
    h.state_mut().run("range.setValues", json!({"range": "A1", "values": [["'0101 2025/04/01 x"], ["'0102 2025/04/02 y"]]})).unwrap();
    h.state_mut().run("selection.set", json!({"range": "A1:A2"})).unwrap();
    control(&mut h, "ui.dialog", json!({"name": "textToColumns"}));
    h.run_steps(2);
    control(&mut h, "ui.dialog.set", json!({"field": "type", "value": "fixed"}));
    control(&mut h, "ui.dialog.set", json!({"field": "breaks", "value": [5, 16]}));
    control(&mut h, "ui.dialog.set", json!({"field": "step", "value": 3}));
    control(&mut h, "ui.dialog.set", json!({"field": "columns", "value": ["text", "ymd", "skip"]}));
    h.run_steps(2);
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], json!(true), "{r}");
    let app = h.state();
    let d = app.session.active().unwrap();
    let sh = d.wb.active().unwrap();
    let cell = |a: &str| sh.value(gridcraft_engine::core::CellRef::parse(a).unwrap());
    assert_eq!(cell("A1").display(), "0101");
    assert!(matches!(cell("B2"), gridcraft_engine::core::Value::Number(_)), "a date");
    assert_eq!(cell("C1").display(), "", "the skipped column isn't imported");
}

#[test]
fn from_text_csv_preview_loads_into_this_workbook() {
    let mut h = harness();
    let csv = "code,name\n0101,bread\n0102,milk\n";
    control(
        &mut h,
        "ui.dialog",
        json!({"name": "textImport", "name_": "", "text": csv, "encoding": "auto", "delimiter": "auto", "detect": "first200"}),
    );
    h.run_steps(3);
    {
        let d = h.state().dialog.as_ref().expect("the preview is up");
        assert_eq!(d.name(), "textImport");
        assert_eq!(d.values["encoding"], "65001", "the detected origin shows");
        assert_eq!(d.values["delimiter"], "comma");
    }
    control(&mut h, "ui.dialog.set", json!({"field": "detect", "value": "none"}));
    h.run_steps(2);
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], json!(true), "{r}");
    let d = h.state().session.active().unwrap();
    assert_eq!(d.wb.sheets.len(), 2, "loaded on a new sheet of this workbook");
    let sh = d.wb.active().unwrap();
    assert_eq!(sh.value(gridcraft_engine::core::CellRef::parse("A2").unwrap()).display(), "0101");
    assert_eq!(sh.tables.len(), 1);
}

#[test]
fn from_text_csv_transform_data_chooses_a_columns_type_in_the_power_query_editor() {
    let mut h = harness();
    let csv = "date,code,qty\n2025/04/01,0101,3\n2025/04/02,0102,4\n";
    control(
        &mut h,
        "ui.dialog",
        json!({"name": "textImport", "text": csv, "encoding": "auto", "delimiter": "auto", "detect": "first200", "types": {"qty": "skip"}}),
    );
    h.run_steps(3);
    {
        let d = h.state().dialog.as_ref().expect("the preview is up");
        let r = d.result.as_ref().expect("a preview");
        assert_eq!(r["types"], json!(["date", "integer", "skip"]), "{r}");
        assert_eq!(d.values["columns"], json!(["auto", "auto", "skip"]), "named types become the headers' choices");
    }
    // The plain preview names its columns only; types are chosen in the Power Query Editor.
    assert!(h.query_by_label("1²3 Whole Number ▾").is_none(), "the preview has no type buttons");
    h.get_by_label(" Transform Data ").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.title.as_str()), Some("Power Query Editor"));
    // The code column's header icon (1²3 Whole Number, detected) opens the data type menu.
    h.get_by_label("1²3 Whole Number ▾").click();
    h.run_steps(2);
    h.get_by_label("Text").click();
    h.run_steps(3);
    // The column has a detected type, so Power Query asks first; the type waits for the answer.
    assert_eq!(h.state().dialog.as_ref().unwrap().values["columns"][1], "auto");
    h.get_by_label("Change Column Type");
    h.get_by_label("Replace current").click();
    h.run_steps(3);
    {
        let d = h.state().dialog.as_ref().expect("the editor is up");
        let r = d.result.as_ref().expect("a preview");
        assert_eq!(r["types"][1], "text", "{r}");
        assert_eq!(r["rows"][1][1], "0101", "the preview follows the choice");
    }
    h.get_by_label("   Close & Load   ").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none(), "{:?}", h.state().message);
    let d = h.state().session.active().unwrap();
    let sh = d.wb.active().unwrap();
    let cell = |a: &str| sh.value(gridcraft_engine::core::CellRef::parse(a).unwrap());
    assert_eq!(cell("B2").display(), "0101");
    assert!(matches!(cell("A2"), gridcraft_engine::core::Value::Number(_)), "the date still converts");
    assert_eq!(cell("C1").display(), "", "the removed column isn't loaded");
    assert_eq!(sh.tables[0].range.a1(), "A1:B3");
}

#[test]
fn change_column_type_escape_keeps_the_type() {
    let mut h = harness();
    let csv = "name,code\nPens,0101\nInk,0102\n";
    control(&mut h, "ui.dialog", json!({"name": "textImport", "text": csv, "encoding": "auto", "delimiter": "auto", "detect": "first200"}));
    h.run_steps(3);
    h.get_by_label(" Transform Data ").click();
    h.run_steps(3);
    h.get_by_label("1²3 Whole Number ▾").click();
    h.run_steps(2);
    h.get_by_label("Text").click();
    h.run_steps(3);
    // Escape answers Change Column Type (as its Cancel does) and leaves the editor open.
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let d = h.state().dialog.as_ref().expect("the editor stays");
    assert_ne!(d.values.get("columns").and_then(|c| c.get(1)), Some(&json!("text")));
    assert!(d.values.get("pendingType").is_none());
    assert_eq!(d.result.as_ref().unwrap()["types"][1], "integer");
}
