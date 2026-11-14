//! PivotTable filters from the ▼ menus, slicers, Design options, buttons and Excel 365's defaults
//! (automatic date grouping, Japanese item order, new sheet names).

use gridcraft_core::{CellRef, Value};
use serde_json::{Value as Json, json};

use crate::Session;

/// 日付, 店舗, 商品, 金額 — eight sales over two fiscal years, in a Japanese workbook.
fn shops() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("app.region.set", json!({"region": "ja-JP"})).unwrap();
    let rows = json!([
        ["日付", "店舗", "商品", "金額"],
        ["2025/4/3", "商店街店", "食パン", 300],
        ["2025/4/9", "駅前店", "あんぱん", 150],
        ["2025/7/1", "公園通り店", "カレーパン", 200],
        ["2025/10/20", "駅前店", "食パン", 600],
        ["2026/1/5", "商店街店", "あんぱん", 100],
        ["2026/2/14", "公園通り店", "食パン", 450],
        ["2026/3/3", "駅前店", "カレーパン", 400],
        ["2026/3/30", "公園通り店", "あんぱん", 50]
    ]);
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("selection.set", json!({"range": "A1"})).unwrap();
    s
}

fn val(s: &Session, a: &str) -> Value {
    s.doc().unwrap().wb.active().unwrap().value(CellRef::parse(a).unwrap())
}
fn text(s: &Session, a: &str) -> String {
    val(s, a).display()
}
fn pt(s: &Session) -> gridcraft_model::PivotTable {
    s.doc().unwrap().wb.active().unwrap().pivots.last().unwrap().clone()
}
/// The first report column, top to bottom.
fn column_a(s: &Session) -> Vec<String> {
    let r = pt(s).last_range.unwrap();
    (r.start.row..=r.end.row).map(|row| text(s, &CellRef::new(row, r.start.col).a1())).collect()
}
/// The row labels between the header and the grand total.
fn row_labels(s: &Session) -> Vec<String> {
    let all = column_a(s);
    let start = all.iter().position(|l| l == "行ラベル").map_or(0, |i| i + 1);
    all[start..all.len() - 1].to_vec()
}
fn grand_total(s: &Session) -> Value {
    let r = pt(s).last_range.unwrap();
    val(s, &r.end.a1())
}

fn shop_pivot() -> Session {
    let mut s = shops();
    s.execute("insert.pivotTable", json!({"rows": ["店舗"], "values": ["金額"]})).unwrap();
    s
}

#[test]
fn japanese_items_sort_in_jis_order_like_japanese_excel() {
    let s = shop_pivot();
    // 駅 (JIS 1-17) before 公 (1-24) before 商 (1-30); Unicode would put 駅前店 last.
    assert_eq!(row_labels(&s), ["駅前店", "公園通り店", "商店街店"]);
    let mut s = shops();
    s.execute("insert.pivotTable", json!({"rows": ["商品"], "values": ["金額"]})).unwrap();
    // Kana by reading (あ, か), then kanji.
    assert_eq!(row_labels(&s), ["あんぱん", "カレーパン", "食パン"]);
}

#[test]
fn a_new_pivot_sheet_is_named_like_excel_names_it() {
    let mut s = shops();
    s.execute("sheet.rename", json!({"name": "売上明細"})).unwrap();
    s.execute("home.insertSheet", json!({"name": "関数の集計"})).unwrap();
    s.execute("sheet.activate", json!({"sheet": "売上明細"})).unwrap();
    s.execute("insert.pivotTable", json!({"source": "売上明細!A1:D9"})).unwrap();
    // Two sheets: Excel adds Sheet3, not the free Sheet1.
    assert_eq!(s.doc().unwrap().wb.active().unwrap().name, "Sheet3");
}

#[test]
fn a_date_field_added_to_rows_groups_by_year_quarter_and_month() {
    let mut s = shops();
    s.execute("insert.pivotTable", json!({"values": ["金額"]})).unwrap();
    let r = s.execute("pivot.addField", json!({"field": "日付", "area": "rows"})).unwrap();
    assert_eq!(r["grouped"], json!(["years", "quarters", "months"]));
    let info = s.execute("pivot.fields", json!({})).unwrap();
    let names: Vec<&str> = info["rows"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["年", "四半期", "日付"]);
    // The years show collapsed, as Excel first shows them.
    assert_eq!(row_labels(&s), ["2025年", "2026年"]);
    assert_eq!(grand_total(&s), Value::number(2250.0));
    s.execute("pivot.collapse", json!({"field": "年", "collapse": false})).unwrap();
    s.execute("pivot.collapse", json!({"field": "四半期", "collapse": false})).unwrap();
    assert_eq!(row_labels(&s)[..3], ["2025年", "第2四半期", "4月"]);
    // autoGroup: false keeps the dates.
    let mut s = shops();
    s.execute("insert.pivotTable", json!({"values": ["金額"]})).unwrap();
    s.execute("pivot.addField", json!({"field": "日付", "area": "rows", "autoGroup": false})).unwrap();
    assert_eq!(pt(&s).rows.len(), 1);
    assert_eq!(row_labels(&s).len(), 8);
}

#[test]
fn label_filters_keep_the_items_whose_caption_matches() {
    let mut s = shop_pivot();
    s.execute("pivot.labelFilter", json!({"field": "店舗", "op": "endsWith", "value": "通り店"})).unwrap();
    assert_eq!(row_labels(&s), ["公園通り店"]);
    assert_eq!(grand_total(&s), Value::number(700.0));
    s.execute("pivot.labelFilter", json!({"field": "店舗", "op": "equal", "value": "*店"})).unwrap();
    assert_eq!(row_labels(&s).len(), 3);
    s.execute("pivot.labelFilter", json!({"field": "店舗", "op": "notContains", "value": "前"})).unwrap();
    assert_eq!(row_labels(&s), ["公園通り店", "商店街店"]);
    let items = s.execute("pivot.items", json!({"field": "店舗"})).unwrap();
    assert_eq!(items["filtered"], true);
    assert_eq!(items["labelFilter"]["op"], "notContains");
    s.execute("pivot.clearFilter", json!({"field": "店舗"})).unwrap();
    assert_eq!(row_labels(&s).len(), 3);
    assert!(s.execute("pivot.labelFilter", json!({"field": "店舗", "op": "near", "value": "x"})).is_err());
    assert!(s.execute("pivot.labelFilter", json!({"field": "金額", "op": "equal", "value": "x"})).is_err());
}

#[test]
fn value_filters_and_top_ten_keep_items_by_their_totals() {
    let mut s = shop_pivot();
    // 駅前店 1150, 公園通り店 700, 商店街店 400.
    s.execute("pivot.valueFilter", json!({"field": "店舗", "op": "greaterThan", "value": 500})).unwrap();
    assert_eq!(row_labels(&s), ["駅前店", "公園通り店"]);
    assert_eq!(grand_total(&s), Value::number(1850.0));
    s.execute("pivot.valueFilter", json!({"field": "店舗", "op": "between", "value": 300, "value2": 800})).unwrap();
    assert_eq!(row_labels(&s), ["公園通り店", "商店街店"]);
    s.execute("pivot.valueFilter", json!({"field": "店舗", "op": "top10", "value": 1})).unwrap();
    assert_eq!(row_labels(&s), ["駅前店"]);
    s.execute("pivot.valueFilter", json!({"field": "店舗", "op": "top10", "value": 1, "order": "bottom"})).unwrap();
    assert_eq!(row_labels(&s), ["商店街店"]);
    // A Label Filter replaces the Value Filter (one filter per field, as in Excel).
    s.execute("pivot.labelFilter", json!({"field": "店舗", "op": "beginsWith", "value": "駅"})).unwrap();
    assert!(pt(&s).rows[0].value_filter.is_none());
    assert_eq!(row_labels(&s), ["駅前店"]);
    s.execute("pivot.clearFilter", json!({})).unwrap();
    assert_eq!(row_labels(&s).len(), 3);
    assert!(s.execute("pivot.valueFilter", json!({"field": "店舗", "op": "greaterThan"})).is_err());
    assert!(s.execute("pivot.valueFilter", json!({"field": "店舗", "op": "greaterThan", "value": 1, "valueField": "nope"})).is_err());
}

#[test]
fn checked_items_from_the_menu_hide_the_others() {
    let mut s = shop_pivot();
    s.execute("pivot.filter", json!({"field": "店舗", "selected": ["駅前店", "商店街店"]})).unwrap();
    assert_eq!(row_labels(&s), ["駅前店", "商店街店"]);
    let items = s.execute("pivot.items", json!({"field": "店舗"})).unwrap();
    let states: Vec<(String, bool)> =
        items["items"].as_array().unwrap().iter().map(|i| (i["label"].as_str().unwrap().to_string(), i["selected"].as_bool().unwrap())).collect();
    assert_eq!(states, [("駅前店".to_string(), true), ("公園通り店".to_string(), false), ("商店街店".to_string(), true)]);
    assert_eq!(items["area"], "rows");
    assert_eq!(items["valueFields"], json!(["合計 / 金額"]));
}

#[test]
fn report_filter_caption_is_japanese_in_a_japanese_workbook() {
    let mut s = shops();
    s.execute("insert.pivotTable", json!({"rows": ["商品"], "values": ["金額"], "filters": ["店舗"]})).unwrap();
    assert_eq!(text(&s, "B1"), "(すべて)");
    s.execute("pivot.filter", json!({"field": "店舗", "selected": ["駅前店", "商店街店"]})).unwrap();
    assert_eq!(text(&s, "B1"), "(複数のアイテム)");
}

#[test]
fn design_blank_rows_and_repeated_labels() {
    let mut s = shops();
    s.execute("insert.pivotTable", json!({"rows": ["店舗", "商品"], "values": ["金額"]})).unwrap();
    let before = column_a(&s).len();
    s.execute("pivot.blankRows", json!({"on": true})).unwrap();
    assert!(pt(&s).blank_rows);
    // One blank line after each of the three shops.
    assert_eq!(column_a(&s).len(), before + 3);
    s.execute("pivot.blankRows", json!({})).unwrap();
    assert!(!pt(&s).blank_rows);
    s.execute("pivot.layout", json!({"layout": "tabular", "subtotals": "none"})).unwrap();
    s.execute("pivot.repeatLabels", json!({"on": true})).unwrap();
    // Every line names its shop: 駅前店 has あんぱん, カレーパン and 食パン.
    let r = pt(&s).last_range.unwrap();
    let shops: Vec<String> = (r.start.row + 1..r.end.row).map(|row| text(&s, &CellRef::new(row, 0).a1())).collect();
    assert_eq!(shops[..3], ["駅前店", "駅前店", "駅前店"]);
    s.execute("pivot.repeatLabels", json!({"on": false})).unwrap();
    let shops: Vec<String> = (r.start.row + 1..r.end.row).map(|row| text(&s, &CellRef::new(row, 0).a1())).collect();
    assert_eq!(shops[..3], ["駅前店", "", ""]);
}

fn buttons(s: &mut Session) -> Vec<Json> {
    s.execute("pivot.buttons", json!({})).unwrap().as_array().cloned().unwrap()
}

#[test]
fn the_report_shows_field_and_expand_buttons() {
    let mut s = shops();
    s.execute("insert.pivotTable", json!({"rows": ["店舗", "商品"], "values": ["金額"], "filters": ["日付"]})).unwrap();
    let b = buttons(&mut s);
    let field = |area: &str| b.iter().find(|x| x["kind"] == "field" && x["area"] == area).cloned();
    assert_eq!(field("filters").unwrap()["cell"], "B1");
    assert_eq!(field("rows").unwrap()["cell"], "A3");
    assert_eq!(field("rows").unwrap()["fields"], json!(["店舗", "商品"]));
    let expands: Vec<&Json> = b.iter().filter(|x| x["kind"] == "expand").collect();
    assert_eq!(expands.len(), 3);
    assert_eq!(expands[0]["item"], "駅前店");
    assert_eq!(expands[0]["collapsed"], false);
    s.execute("pivot.expandButtons", json!({"on": false})).unwrap();
    assert!(buttons(&mut s).iter().all(|x| x["kind"] != "expand"));
    s.execute("pivot.fieldHeaders", json!({"on": false})).unwrap();
    assert!(buttons(&mut s).iter().all(|x| x["area"] != "rows"));
    assert_eq!(text(&s, "A3"), "");
}

#[test]
fn pivot_slicers_filter_their_pivot_table() {
    let mut s = shop_pivot();
    s.execute("selection.set", json!({"range": "A4"})).unwrap();
    // Insert › Slicer in a PivotTable makes a pivot slicer.
    let r = s.execute("insert.slicer", json!({"field": "商品"})).unwrap();
    let id = r["slicer"].as_u64().unwrap() as u32;
    let sl = s.doc().unwrap().wb.active().unwrap().slicers.iter().find(|x| x.id == id).cloned().unwrap();
    assert_eq!(sl.pivot, Some(pt(&s).id));
    assert_eq!(sl.column, "商品");
    let items = crate::cmd::slicer::slicer_items(&s.doc().unwrap().wb, 0, &sl);
    assert_eq!(items, [("あんぱん".to_string(), true), ("カレーパン".to_string(), true), ("食パン".to_string(), true)]);
    // One click shows only that item; 商品 is in no area, so it filters without showing above.
    s.execute("slicer.toggle", json!({"slicer": id, "value": "食パン"})).unwrap();
    assert_eq!(grand_total(&s), Value::number(1350.0));
    assert!(pt(&s).filters.iter().all(|f| f.hidden_only));
    let info = s.execute("pivot.fields", json!({})).unwrap();
    assert!(info["fields"].as_array().unwrap().iter().all(|f| f["areas"].as_array().unwrap().iter().all(|a| a != "filters")));
    // Multi-select adds an item; Clear Filter shows all again.
    s.execute("slicer.toggle", json!({"slicer": id, "value": "あんぱん", "multi": true})).unwrap();
    assert_eq!(grand_total(&s), Value::number(1650.0));
    let sl = s.doc().unwrap().wb.active().unwrap().slicers.iter().find(|x| x.id == id).cloned().unwrap();
    let items = crate::cmd::slicer::slicer_items(&s.doc().unwrap().wb, 0, &sl);
    assert_eq!(items.iter().filter(|(_, on)| *on).count(), 2);
    s.execute("slicer.clear", json!({"slicer": id})).unwrap();
    assert_eq!(grand_total(&s), Value::number(2250.0));
    // A slicer on a field in Rows sets its hidden items, which the ▼ menu shows.
    let r = s.execute("pivot.insertSlicer", json!({"fields": ["店舗"]})).unwrap();
    let shop = r["slicer"].as_u64().unwrap();
    s.execute("slicer.toggle", json!({"slicer": shop, "value": "駅前店"})).unwrap();
    assert_eq!(row_labels(&s), ["駅前店"]);
    let items = s.execute("pivot.items", json!({"field": "店舗"})).unwrap();
    assert_eq!(items["items"][1]["selected"], false);
    // Deleting the slicer keeps the filter, as Excel does; undo brings the slicer back.
    s.execute("slicer.delete", json!({"slicer": shop})).unwrap();
    assert_eq!(row_labels(&s), ["駅前店"]);
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.doc().unwrap().wb.active().unwrap().slicers.len(), 2);
    assert!(s.execute("slicer.toggle", json!({"slicer": shop, "value": "no such shop"})).is_err());
}

#[test]
fn insert_slicer_without_a_field_opens_the_dialog() {
    let mut s = shop_pivot();
    s.ui_requests.clear();
    s.execute("pivot.insertSlicer", json!({})).unwrap();
    let Some(crate::UiRequest::Dialog(name, params)) = s.ui_requests.last() else { panic!("no dialog") };
    assert_eq!(name, "insertSlicer");
    assert_eq!(params["fields"], json!(["日付", "店舗", "商品", "金額"]));
    assert!(s.execute("pivot.insertSlicer", json!({"field": "nope"})).is_err());
}

#[test]
fn field_settings_opens_the_dialog_for_the_active_field() {
    let mut s = shop_pivot();
    s.ui_requests.clear();
    s.execute("selection.set", json!({"range": "B4"})).unwrap();
    s.execute("pivot.fieldSettings", json!({})).unwrap();
    let Some(crate::UiRequest::Dialog(name, _)) = s.ui_requests.last() else { panic!("no dialog") };
    assert_eq!(name, "pivotValueSettings");
    s.execute("selection.set", json!({"range": "A4"})).unwrap();
    s.execute("pivot.fieldSettings", json!({})).unwrap();
    let Some(crate::UiRequest::Dialog(name, params)) = s.ui_requests.last() else { panic!("no dialog") };
    assert_eq!(name, "pivotFieldSettings");
    assert_eq!(params["sourceName"], "店舗");
    s.execute("pivot.fieldSettings", json!({"field": "店舗", "subtotals": "none", "layout": "tabular", "repeatLabels": true})).unwrap();
    assert!(!pt(&s).subtotals);
    assert!(pt(&s).repeat_labels);
}
