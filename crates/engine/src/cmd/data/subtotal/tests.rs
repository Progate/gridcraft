use serde_json::json;

use crate::Session;
use gridcraft_core::CellRef;

/// こむぎ堂's expense list: store, item, amount (`#,##0`), sorted by store.
fn stores() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("app.region.set", json!({"region": "ja-JP"})).unwrap();
    s.execute(
        "range.setValues",
        json!({"range": "A1", "values": [
            ["店舗", "費目", "1月", "2月"],
            ["駅前店", "家賃", 1000000, 1000000],
            ["駅前店", "人件費", 402927, 400000],
            ["公園通り店", "家賃", 2000, 2500],
            ["公園通り店", "人件費", 3000, 3500],
        ]}),
    )
    .unwrap();
    s.execute("home.numberFormat", json!({"range": "C2:D5", "code": "#,##0"})).unwrap();
    s.execute("selection.set", json!({"range": "A1"})).unwrap();
    s
}

fn text(s: &Session, a1: &str) -> String {
    let sh = s.doc().unwrap().wb.active().unwrap();
    let c = CellRef::parse(a1).unwrap();
    crate::display::cell_text(&s.doc().unwrap().wb, sh, c)
}

fn input(s: &Session, a1: &str) -> String {
    s.doc().unwrap().wb.active().unwrap().input_text(CellRef::parse(a1).unwrap())
}

fn levels(s: &Session, rows: std::ops::RangeInclusive<u32>) -> Vec<u8> {
    let sh = s.doc().unwrap().wb.active().unwrap();
    rows.map(|r| sh.rows.get(&(r - 1)).map_or(0, |e| e.outline)).collect()
}

fn hidden(s: &Session, row: u32) -> bool {
    s.doc().unwrap().wb.active().unwrap().is_row_hidden(row - 1)
}

#[test]
fn header_names_choose_the_columns_and_totals_keep_the_number_format() {
    let mut s = stores();
    let r = s.execute("data.subtotal", json!({"groupBy": "店舗", "function": "sum", "columns": ["1月", "2月"]})).unwrap();
    assert_eq!(r["groups"], 2);
    assert_eq!(input(&s, "A4"), "駅前店 集計");
    assert_eq!(input(&s, "C4"), "=SUBTOTAL(9,C2:C3)");
    assert_eq!(input(&s, "D7"), "=SUBTOTAL(9,D5:D6)");
    assert_eq!(input(&s, "A8"), "総計");
    assert_eq!(input(&s, "C8"), "=SUBTOTAL(9,C2:C7)");
    assert_eq!(text(&s, "C4"), "1,402,927", "the subtotal reads like the amounts above it");
    assert_eq!(text(&s, "C8"), "1,407,927");
    let wb = &s.doc().unwrap().wb;
    let sh = wb.active().unwrap();
    let bold = |a1: &str| wb.styles.get(sh.style_id(CellRef::parse(a1).unwrap())).font.bold;
    assert!(bold("A4") && bold("A8"), "the captions are bold");
    assert!(!bold("C4"), "the amounts keep their own font");
}

#[test]
fn the_outline_has_three_levels_and_the_level_buttons_work() {
    let mut s = stores();
    s.execute("data.subtotal", json!({"groupBy": "A", "columns": ["C"]})).unwrap();
    // Header, detail 2-3, 駅前店 集計, detail 5-6, 公園通り店 集計, 総計.
    assert_eq!(levels(&s, 1..=8), [0, 2, 2, 1, 2, 2, 1, 0]);
    s.execute("data.showLevels", json!({"rows": 1})).unwrap();
    assert!((2..=7).all(|r| hidden(&s, r)), "level 1 shows the grand total only");
    assert!(!hidden(&s, 1) && !hidden(&s, 8));
    s.execute("data.showLevels", json!({"rows": 2})).unwrap();
    assert!(hidden(&s, 2) && hidden(&s, 3) && hidden(&s, 5) && hidden(&s, 6));
    assert!(!hidden(&s, 4) && !hidden(&s, 7) && !hidden(&s, 8), "level 2 shows the subtotals and the grand total");
    s.execute("data.showLevels", json!({"rows": 3})).unwrap();
    assert!((1..=8).all(|r| !hidden(&s, r)));
    assert!(s.doc().unwrap().wb.active().unwrap().show_outline_symbols, "the outline symbols show");
}

#[test]
fn running_it_again_replaces_the_subtotals_unless_told_not_to() {
    let mut s = stores();
    s.execute("data.subtotal", json!({"groupBy": "店舗", "columns": ["1月"]})).unwrap();
    s.execute("selection.set", json!({"range": "B3"})).unwrap();
    let r = s.execute("data.subtotal", json!({"groupBy": "店舗", "function": "average", "columns": ["2月"]})).unwrap();
    assert_eq!(r["removed"], 3);
    assert_eq!(input(&s, "A4"), "駅前店 平均");
    assert_eq!(input(&s, "C4"), "", "the old sums are gone");
    assert_eq!(input(&s, "D4"), "=SUBTOTAL(1,D2:D3)");
    assert_eq!(input(&s, "A8"), "全体の平均");
    assert_eq!(input(&s, "A9"), "");
    assert_eq!(levels(&s, 1..=8), [0, 2, 2, 1, 2, 2, 1, 0], "the outline does not deepen");
    // Without Replace the new subtotals nest inside the old ones.
    s.execute("data.subtotal", json!({"groupBy": "店舗", "columns": ["1月"], "replace": false})).unwrap();
    assert!((1..=14).any(|r| input(&s, &format!("A{r}")) == "駅前店 集計"));
    assert!(levels(&s, 1..=14).contains(&3));
}

#[test]
fn remove_all_takes_the_subtotals_and_the_outline_away() {
    let mut s = stores();
    s.execute("data.subtotal", json!({"groupBy": "店舗", "columns": ["1月"], "pageBreaks": true})).unwrap();
    assert_eq!(s.doc().unwrap().wb.active().unwrap().print.row_breaks, vec![4], "a page break after 駅前店 集計");
    let r = s.execute("data.removeSubtotals", json!({})).unwrap();
    assert_eq!(r["removed"], 3);
    assert_eq!(r["range"], "A1:D5");
    assert_eq!(input(&s, "A4"), "公園通り店");
    assert_eq!(input(&s, "A6"), "");
    assert_eq!(levels(&s, 1..=6), [0; 6]);
    assert!(s.doc().unwrap().wb.active().unwrap().print.row_breaks.is_empty());
    // One undo puts the subtotals back.
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(input(&s, "A8"), "総計");
}

#[test]
fn summaries_above_the_data_put_the_totals_first() {
    let mut s = stores();
    s.execute("data.subtotal", json!({"groupBy": "店舗", "columns": ["1月"], "summaryBelow": false})).unwrap();
    assert!(!s.doc().unwrap().wb.active().unwrap().summary_below);
    assert_eq!(input(&s, "A2"), "総計");
    assert_eq!(input(&s, "C2"), "=SUBTOTAL(9,C3:C8)");
    assert_eq!(input(&s, "A3"), "駅前店 集計");
    assert_eq!(input(&s, "C3"), "=SUBTOTAL(9,C4:C5)");
    assert_eq!(input(&s, "A6"), "公園通り店 集計");
    assert_eq!(input(&s, "C6"), "=SUBTOTAL(9,C7:C8)");
    assert_eq!(text(&s, "C2"), "1,407,927");
    assert_eq!(levels(&s, 1..=8), [0, 0, 1, 2, 2, 1, 2, 2]);
    s.execute("data.showLevels", json!({"rows": 2})).unwrap();
    assert!(!hidden(&s, 3) && hidden(&s, 4) && !hidden(&s, 6) && hidden(&s, 8));
}

#[test]
fn unknown_fields_and_functions_are_refused() {
    let mut s = stores();
    let e = s.execute("data.subtotal", json!({"groupBy": "地域", "columns": ["1月"]})).unwrap_err();
    assert!(e.to_string().contains("groupBy"), "{e}");
    let e = s.execute("data.subtotal", json!({"groupBy": "A", "columns": ["Z"]})).unwrap_err();
    assert!(e.to_string().contains("columns"), "{e}");
    let e = s.execute("data.subtotal", json!({"groupBy": "A", "columns": ["C"], "function": "median"})).unwrap_err();
    assert!(e.to_string().contains("median"), "{e}");
    assert_eq!(input(&s, "A4"), "公園通り店", "nothing changed");
}

#[test]
fn every_function_has_its_code_and_japanese_caption() {
    for (f, code, cap) in [("count", 3, "個数"), ("countNums", 2, "数値の個数"), ("stdDevP", 8, "標準偏差"), ("varP", 11, "分散")] {
        let mut s = stores();
        s.execute("data.subtotal", json!({"groupBy": "A", "columns": ["C"], "function": f})).unwrap();
        assert_eq!(input(&s, "A4"), format!("駅前店 {cap}"));
        assert_eq!(input(&s, "C4"), format!("=SUBTOTAL({code},C2:C3)"));
        assert_eq!(input(&s, "A8"), format!("全体の{cap}"));
    }
}
