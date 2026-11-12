//! The regional format (`app.region.set`): Japanese entry, dates shown in the region's short
//! date, formulas that follow a date's format, yen formats and Japanese report captions.

use gridcraft_core::{CellRef, Value};
use serde_json::json;

use crate::Session;

fn ja() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("app.region.set", json!({"region": "ja_JP.UTF-8"})).unwrap();
    s
}

fn value(s: &Session, a1: &str) -> Value {
    s.doc().unwrap().wb.active().unwrap().value(CellRef::parse(a1).unwrap())
}

fn code(s: &Session, a1: &str) -> String {
    let wb = &s.doc().unwrap().wb;
    wb.styles.get(wb.active().unwrap().style_id(CellRef::parse(a1).unwrap())).num_fmt.as_str().to_string()
}

fn shown(s: &Session, a1: &str) -> String {
    let wb = &s.doc().unwrap().wb;
    let c = CellRef::parse(a1).unwrap();
    crate::display::format(&wb.active().unwrap().value(c), &code(s, a1), wb).text
}

fn set(s: &mut Session, a1: &str, input: &str) {
    s.execute("cell.set", json!({"cell": a1, "input": input})).unwrap();
}

#[test]
fn the_region_is_a_session_setting_on_every_workbook() {
    let mut s = Session::new();
    s.new_workbook();
    assert_eq!(s.execute("app.region.set", json!({})).unwrap()["region"], "en-US");
    let r = s.execute("app.region.set", json!({"region": "ja-JP"})).unwrap();
    assert_eq!(r["region"], "ja-JP");
    assert_eq!(s.prefs.region, "ja-JP");
    assert!(!s.doc().unwrap().is_dirty(), "the region is not an edit");
    s.new_workbook();
    assert!(s.doc().unwrap().wb.region.is_japanese(), "a new workbook takes the session's region");
    assert!(s.documents().iter().all(|d| d.wb.region.is_japanese()));
    assert!(s.execute("app.region.set", json!({"region": " "})).is_err());
}

#[test]
fn japanese_entry_reads_dates_full_width_numbers_and_yen() {
    let mut s = ja();
    set(&mut s, "A1", "2026/11/10");
    assert_eq!(code(&s, "A1"), "m/d/yyyy");
    assert_eq!(shown(&s, "A1"), "2026/11/10");
    set(&mut s, "A2", "11/10");
    assert_eq!(code(&s, "A2"), "m\"月\"d\"日\"");
    assert_eq!(shown(&s, "A2"), "11月10日");
    set(&mut s, "A3", "2026年11月10日");
    assert_eq!(shown(&s, "A3"), "2026年11月10日");
    assert_eq!(value(&s, "A3"), value(&s, "A1"));
    set(&mut s, "A4", "３８０");
    assert_eq!(value(&s, "A4"), Value::Number(380.0));
    set(&mut s, "A5", "￥1280");
    assert_eq!(value(&s, "A5"), Value::Number(1280.0));
    assert_eq!(shown(&s, "A5"), "¥1,280");
    set(&mut s, "A6", "＝ＳＵＭ（Ａ４：Ａ５）");
    let d = s.doc().unwrap();
    assert_eq!(d.wb.active().unwrap().cell(CellRef::parse("A6").unwrap()).unwrap().formula.as_ref().unwrap().text, "SUM(A4:A5)");
    assert_eq!(value(&s, "A6"), Value::Number(1660.0));
    // Quoted text keeps its full-width characters.
    set(&mut s, "A7", "＝ＩＦ（Ａ４＞１，\"ＯＫ\"，\"\"）");
    assert_eq!(value(&s, "A7"), Value::text("ＯＫ"));
    // The formula bar shows a date in the short date, whatever the cell's format.
    let wb = &s.doc().unwrap().wb;
    let edit = crate::display::edit_text(wb, wb.active().unwrap(), CellRef::parse("A2").unwrap());
    assert_eq!(edit, format!("{}/11/10", gridcraft_core::parse::current_year()));
    // English (United States) reads them as before.
    let mut en = Session::new();
    en.new_workbook();
    set(&mut en, "A1", "３８０");
    assert_eq!(value(&en, "A1"), Value::text("３８０"));
    set(&mut en, "A2", "＝1+1");
    assert_eq!(value(&en, "A2"), Value::text("＝1+1"));
}

#[test]
fn formulas_take_the_date_format_of_the_cell_they_read() {
    let mut s = ja();
    set(&mut s, "C2", "11/10");
    set(&mut s, "H2", "7");
    set(&mut s, "D2", "=C2+$H$2");
    assert_eq!(code(&s, "D2"), "m\"月\"d\"日\"");
    assert_eq!(shown(&s, "D2"), "11月17日");
    set(&mut s, "D3", "=MAX(C2:C2)");
    assert_eq!(code(&s, "D3"), "m\"月\"d\"日\"");
    set(&mut s, "D4", "=$H$2+C2");
    assert_eq!(code(&s, "D4"), "m\"月\"d\"日\"");
    // A result that is not a date stays General.
    set(&mut s, "D5", "=YEAR(C2)");
    assert_eq!(code(&s, "D5"), "General");
    set(&mut s, "D6", "=H2*2");
    assert_eq!(code(&s, "D6"), "General");
    // TODAY and NOW show in the region's short date.
    set(&mut s, "E1", "=TODAY()");
    assert_eq!(code(&s, "E1"), "m/d/yyyy");
    assert!(shown(&s, "E1").starts_with("20"), "{}", shown(&s, "E1"));
    set(&mut s, "E2", "=NOW()");
    assert_eq!(code(&s, "E2"), "m/d/yyyy h:mm");
    assert!(shown(&s, "E2").starts_with("20"), "{}", shown(&s, "E2"));
    // TEXT and number formats read the Japanese weekday codes.
    set(&mut s, "F1", "2026/11/14");
    set(&mut s, "F2", "=TEXT(F1,\"aaa\")");
    assert_eq!(value(&s, "F2"), Value::text("土"));
    s.execute("home.numberFormat", json!({"range": "F1", "code": "m/d(aaa)"})).unwrap();
    assert_eq!(shown(&s, "F1"), "11/14(土)");
}

#[test]
fn japanese_currency_accounting_and_comma_style() {
    let mut s = ja();
    set(&mut s, "A1", "-1234");
    s.execute("home.accounting", json!({"range": "A1"})).unwrap();
    assert_eq!(code(&s, "A1"), crate::cmd::format::ACCOUNTING_JA);
    assert_eq!(shown(&s, "A1").trim(), "¥-1,234");
    s.execute("home.comma", json!({"range": "A1"})).unwrap();
    assert_eq!(code(&s, "A1"), crate::cmd::format::COMMA_JA);
    s.execute("home.numberFormat", json!({"range": "A1", "format": "Currency"})).unwrap();
    assert_eq!(code(&s, "A1"), "\"¥\"#,##0;\"¥\"\\-#,##0");
    assert_eq!(shown(&s, "A1"), "¥-1,234");
    // An explicit symbol still picks it.
    s.execute("home.accounting", json!({"range": "A1", "symbol": "$"})).unwrap();
    assert!(code(&s, "A1").contains("\"$\""));
}

#[test]
fn japanese_pivot_and_subtotal_captions() {
    let mut s = ja();
    s.execute("range.setValues", json!({"range": "A1", "values": [["地域", "売上金額"], ["東", 10], ["西", 20], ["東", 5]]})).unwrap();
    s.execute("insert.pivotTable", json!({"source": "Sheet1!$A$1:$B$4", "destination": "Sheet1!D1", "rows": ["地域"], "values": ["売上金額"]}))
        .unwrap();
    let texts: Vec<String> = (0..5).map(|r| value(&s, &format!("D{}", r + 1)).display().to_string()).collect();
    assert_eq!(texts[0], "行ラベル");
    assert!(texts.iter().any(|t| t == "総計"), "{texts:?}");
    assert_eq!(value(&s, "E1"), Value::text("合計 / 売上金額"));
    let pt = &s.doc().unwrap().wb.active().unwrap().pivots[0];
    assert_eq!(pt.values[0].name, "合計 / 売上金額");
    // Another function renames an automatic name the Japanese way.
    s.execute("pivot.valueSettings", json!({"pivot": pt.name.clone(), "index": 0, "func": "average"})).unwrap();
    assert_eq!(s.doc().unwrap().wb.active().unwrap().pivots[0].values[0].name, "平均 / 売上金額");
    // A name equal to a source field is refused, as in Excel; any other is kept.
    let name = s.doc().unwrap().wb.active().unwrap().pivots[0].name.clone();
    assert!(s.execute("pivot.valueSettings", json!({"pivot": name, "index": 0, "name": "売上金額"})).is_err());
    s.execute("pivot.valueSettings", json!({"pivot": name, "index": 0, "name": "売上の平均"})).unwrap();
    assert_eq!(s.doc().unwrap().wb.active().unwrap().pivots[0].values[0].name, "売上の平均");

    let mut s = ja();
    s.execute("range.setValues", json!({"range": "A1", "values": [["地域", "売上"], ["東", 10], ["東", 5], ["西", 20]]})).unwrap();
    s.execute("data.subtotal", json!({"range": "A1:B4", "groupBy": "A", "function": "sum", "columns": ["B"]})).unwrap();
    assert_eq!(value(&s, "A4"), Value::text("東 集計"));
    assert_eq!(value(&s, "A6"), Value::text("西 集計"));
    assert_eq!(value(&s, "A7"), Value::text("総計"));
}
