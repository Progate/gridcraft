//! Tables as Excel makes them: Japanese default names, Create Table's prompt, formats new rows
//! take, Convert to Range's A1 references, and Custom AutoFilter values typed in Japanese.

use gridcraft_core::CellRef;
use serde_json::{Value as Json, json};

use crate::{Session, UiRequest};

fn session(region: &str) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("app.region.set", json!({"region": region})).unwrap();
    s
}

fn formula(s: &mut Session, a: &str) -> Json {
    s.execute("cell.get", json!({"cell": a})).unwrap()["formula"].clone()
}

fn code(s: &Session, a1: &str) -> String {
    let wb = &s.doc().unwrap().wb;
    wb.styles.get(wb.active().unwrap().style_id(CellRef::parse(a1).unwrap())).num_fmt.as_str().to_string()
}

fn style(s: &Session, a1: &str) -> gridcraft_model::Style {
    let wb = &s.doc().unwrap().wb;
    wb.styles.get(wb.active().unwrap().style_id(CellRef::parse(a1).unwrap())).clone()
}

fn tables(s: &Session) -> Vec<gridcraft_model::Table> {
    s.doc().unwrap().wb.sheet(0).unwrap().tables.clone()
}

#[test]
fn japanese_tables_are_named_like_japanese_excel() {
    let mut s = session("ja-JP");
    s.execute("range.setValues", json!({"range": "A1", "values": [[1, 2], [3, 4]]})).unwrap();
    let r = s.execute("insert.table", json!({"range": "A1:B2", "header": false})).unwrap();
    assert_eq!(r["table"], "テーブル1");
    let t = &tables(&s)[0];
    assert_eq!(t.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["列1", "列2"]);
    assert_eq!(s.execute("cell.get", json!({"cell": "B1"})).unwrap()["value"], "列2");
    // A column AutoExpansion adds beside a data row gets the next generated name.
    s.execute("cell.set", json!({"cell": "C2", "input": "x"})).unwrap();
    assert_eq!(tables(&s)[0].columns.last().unwrap().name, "列3");
    s.execute("range.setValues", json!({"range": "E1", "values": [["a"], [1]]})).unwrap();
    assert_eq!(s.execute("insert.table", json!({"range": "E1:E2"})).unwrap()["table"], "テーブル2");
    // The structured reference to it works, and the file keeps the name.
    s.execute("cell.set", json!({"cell": "G1", "input": "=SUM(テーブル2[a])"})).unwrap();
    assert_eq!(s.execute("cell.get", json!({"cell": "G1"})).unwrap()["value"], json!(1.0));

    let mut en = session("en-US");
    en.execute("range.setValues", json!({"range": "A1", "values": [[1], [2]]})).unwrap();
    assert_eq!(en.execute("insert.table", json!({"range": "A1:A2", "header": false})).unwrap()["table"], "Table1");
    assert_eq!(tables(&en)[0].columns[0].name, "Column1");
}

#[test]
fn insert_table_with_prompt_asks_with_the_current_region() {
    let mut s = session("en-US");
    s.execute("range.setValues", json!({"range": "B2", "values": [["name", "qty"], ["a", 1], ["b", 2]]})).unwrap();
    s.execute("selection.set", json!({"range": "C3"})).unwrap();
    s.ui_requests.clear();
    let r = s.execute("insert.table", json!({"prompt": true})).unwrap();
    assert_eq!(r, json!({"range": "=$B$2:$C$4", "header": true}));
    assert!(tables(&s).is_empty(), "the prompt makes no table");
    assert!(
        s.ui_requests.iter().any(|q| matches!(q, UiRequest::Dialog(n, p) if n == "createTable" && p["range"] == "=$B$2:$C$4" && p["header"] == true))
    );
    // OK sends the box as Excel shows it.
    let r = s.execute("insert.table", json!({"range": "=$B$2:$C$4", "header": true})).unwrap();
    assert_eq!(r["range"], "B2:C4");
    // Numbers in the first row: no headers.
    s.execute("range.setValues", json!({"range": "F1", "values": [[1, 2], [3, 4]]})).unwrap();
    s.execute("selection.set", json!({"range": "F1"})).unwrap();
    assert_eq!(s.execute("insert.table", json!({"prompt": true})).unwrap()["header"], false);
}

#[test]
fn rows_added_to_a_table_take_the_formats_of_the_last_data_row() {
    let mut s = session("en-US");
    s.execute("range.setValues", json!({"range": "A1", "values": [["item", "amount"], ["a", 1000], ["b", 2500]]})).unwrap();
    s.execute("insert.table", json!({"range": "A1:B3", "header": true})).unwrap();
    s.execute("home.numberFormat", json!({"range": "B2:B3", "code": "#,##0"})).unwrap();
    s.execute("home.fillColor", json!({"range": "A2:A3", "color": "#FFFF00"})).unwrap();
    // AutoExpansion: typing below the table.
    s.execute("cell.set", json!({"cell": "A4", "input": "c"})).unwrap();
    assert_eq!(tables(&s)[0].range.a1(), "A1:B4");
    assert_eq!(code(&s, "B4"), "#,##0", "the blank cell of the new row is formatted too");
    assert_eq!(style(&s, "A4").fill, style(&s, "A3").fill);
    s.execute("cell.set", json!({"cell": "B5", "input": "3000"})).unwrap();
    assert_eq!(code(&s, "B5"), "#,##0");
    assert_eq!(s.execute("cell.get", json!({"cell": "B5"})).unwrap()["text"], "3,000");
    // Resize Table.
    s.execute("table.resize", json!({"range": "A1:B7"})).unwrap();
    assert_eq!(code(&s, "B7"), "#,##0");
    assert_eq!(style(&s, "A7").fill, style(&s, "A3").fill);
    // Rows inserted inside the table, and above its first data row.
    s.execute("home.insertRows", json!({"rows": "4:4"})).unwrap();
    assert_eq!(code(&s, "B4"), "#,##0");
    s.execute("home.insertRows", json!({"rows": "2:2"})).unwrap();
    assert_eq!(code(&s, "B2"), "#,##0");
    // Cells shifted down across the table's columns.
    s.execute("home.insertCells", json!({"range": "A3:B3", "shift": "down"})).unwrap();
    assert_eq!(code(&s, "B3"), "#,##0");
    // Outside the table nothing changes.
    assert_eq!(code(&s, "C3"), "General");
}

#[test]
fn a_date_typed_below_a_table_keeps_its_date_format_over_the_column_look() {
    let mut s = session("en-US");
    s.execute("range.setValues", json!({"range": "A1", "values": [["when"], ["x"]]})).unwrap();
    s.execute("insert.table", json!({"range": "A1:A2", "header": true})).unwrap();
    s.execute("home.fillColor", json!({"range": "A2", "color": "#FFFF00"})).unwrap();
    s.execute("cell.set", json!({"cell": "A3", "input": "2026-10-15"})).unwrap();
    assert_eq!(style(&s, "A3").fill, style(&s, "A2").fill);
    assert_ne!(code(&s, "A3"), "General");
}

#[test]
fn convert_to_range_writes_structured_references_as_a1() {
    let mut s = session("ja-JP");
    s.execute("range.setValues", json!({"range": "A1", "values": [["商品", "金額"], ["a", 100], ["b", 250]]})).unwrap();
    s.execute("insert.table", json!({"range": "A1:B3", "header": true, "name": "注文"})).unwrap();
    s.execute("cell.set", json!({"cell": "C1", "input": "倍"})).unwrap();
    s.execute("cell.set", json!({"cell": "C2", "input": "=[@金額]*2"})).unwrap();
    s.execute("table.totalRow", json!({"on": true, "table": "注文"})).unwrap();
    s.execute("cell.set", json!({"cell": "E1", "input": "=SUM(注文[金額])"})).unwrap();
    s.execute("cell.set", json!({"cell": "E2", "input": "=注文[[#Headers],[金額]]"})).unwrap();
    s.execute("cell.set", json!({"cell": "E3", "input": "=ROWS(注文)"})).unwrap();
    s.execute("cell.set", json!({"cell": "E4", "input": "=注文[[#Totals],[金額]]"})).unwrap();
    s.execute("cell.set", json!({"cell": "E5", "input": "=COUNTA(注文[#All])"})).unwrap();
    s.execute("home.insertSheet", json!({"name": "Sheet2", "before": 1})).unwrap();
    s.execute("sheet.activate", json!({"sheet": "Sheet2"})).unwrap();
    s.execute("cell.set", json!({"cell": "A1", "input": "=SUM(注文[金額])"})).unwrap();
    s.execute("formulas.defineName", json!({"name": "金額計", "refersTo": "=注文[金額]"})).unwrap();
    s.execute("sheet.activate", json!({"sheet": "Sheet1"})).unwrap();
    s.execute("table.convertToRange", json!({"table": "注文"})).unwrap();
    assert!(tables(&s).is_empty());
    assert_eq!(formula(&mut s, "E1"), "=SUM($B$2:$B$3)");
    assert_eq!(formula(&mut s, "C2"), "=B2*2");
    assert_eq!(formula(&mut s, "C3"), "=B3*2");
    assert_eq!(formula(&mut s, "E2"), "=$B$1");
    assert_eq!(formula(&mut s, "E3"), "=ROWS($A$2:$C$3)");
    assert_eq!(formula(&mut s, "E4"), "=$B$4");
    assert_eq!(formula(&mut s, "E5"), "=COUNTA($A$1:$C$4)");
    assert_eq!(formula(&mut s, "C4"), "=SUBTOTAL(109,$C$2:$C$3)");
    assert_eq!(s.execute("cell.get", json!({"cell": "E1"})).unwrap()["value"], json!(350.0));
    let name = s.doc().unwrap().wb.names.iter().find(|n| n.name == "金額計").unwrap().formula.clone();
    assert_eq!(name.trim_start_matches('='), "Sheet1!$B$2:$B$3");
    s.execute("sheet.activate", json!({"sheet": "Sheet2"})).unwrap();
    assert_eq!(formula(&mut s, "A1"), "=SUM(Sheet1!$B$2:$B$3)");
}

#[test]
fn custom_autofilter_reads_japanese_entry() {
    let mut s = session("ja-JP");
    s.execute("range.setValues", json!({"range": "A1", "values": [["日付", "金額"], ["2026/10/14", 50], ["2026/10/15", 100], ["2026/10/16", 150]]}))
        .unwrap();
    s.execute("data.filter", json!({"range": "A1:B4"})).unwrap();
    let r = s.execute("data.filterBy", json!({"column": "B", "custom": {"op": ">=", "value": "１００"}})).unwrap();
    assert_eq!(r["hiddenRows"], json!(1));
    s.execute("data.filterBy", json!({"column": "B", "clear": true})).unwrap();
    let r = s.execute("data.filterBy", json!({"column": "A", "custom": {"op": "=", "value": "２０２６/１０/１５"}})).unwrap();
    assert_eq!(r["hiddenRows"], json!(2));
    let af = s.doc().unwrap().wb.sheet(0).unwrap().autofilter.clone().unwrap();
    let saved = format!("{:?}", af.criteria);
    assert!(saved.contains("46310"), "the date is kept as its serial: {saved}");
}

#[test]
fn rows_inserted_above_the_total_row_take_formats_and_calculated_columns() {
    let mut s = session("en-US");
    s.execute("range.setValues", json!({"range": "A1", "values": [["item", "amount", "double"], ["a", 1000, null], ["b", 2500, null]]})).unwrap();
    s.execute("insert.table", json!({"range": "A1:C3", "header": true})).unwrap();
    s.execute("cell.set", json!({"cell": "C2", "input": "=[@amount]*2"})).unwrap();
    s.execute("home.numberFormat", json!({"range": "B2:C3", "code": "#,##0"})).unwrap();
    s.execute("table.totalRow", json!({"on": true, "table": "Table1"})).unwrap();
    s.execute("home.insertRows", json!({"rows": "4:5"})).unwrap();
    assert_eq!(tables(&s)[0].range.a1(), "A1:C6");
    for a in ["B4", "B5", "C4", "C5"] {
        assert_eq!(code(&s, a), "#,##0", "{a}");
    }
    assert_eq!(formula(&mut s, "C5"), "=[@amount]*2");
    assert_eq!(formula(&mut s, "C6"), "=SUBTOTAL(109,Table1[double])");
}
