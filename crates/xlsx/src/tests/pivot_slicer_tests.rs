//! PivotTable slicers, Label and Value Filters and the Design options in the file, as Excel
//! writes them, and back.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::*;

use crate::{read_xlsx, write_xlsx};

fn unzip(bytes: &[u8]) -> HashMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut s = String::new();
        let _ = f.read_to_string(&mut s);
        out.insert(f.name().to_string(), s);
    }
    out
}

fn field(name: &str) -> PivotField {
    PivotField { source_col: name.into(), ..PivotField::default() }
}

/// 店舗, 商品, 金額 on "明細"; a PivotTable on "集計" (sheet 2) with 店舗 in Rows, and a slicer on
/// 商品 (in no area) showing あんぱん and 食パン.
fn workbook() -> Workbook {
    let mut wb = Workbook::new();
    wb.region = gridcraft_core::region::find("ja-JP").unwrap_or(wb.region);
    let data = wb.sheet_mut(0).unwrap();
    data.name = "明細".into();
    let rows = [
        ["店舗", "商品", "金額"],
        ["駅前店", "あんぱん", "150"],
        ["商店街店", "食パン", "300"],
        ["公園通り店", "カレーパン", "200"],
        ["駅前店", "食パン", "600"],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            let value = v.parse::<f64>().map(Value::number).unwrap_or_else(|_| Value::text(*v));
            data.cells.set(CellRef::new(r as u32, c as u32), Cell::value(value));
        }
    }
    let mut report = Sheet::new("集計");
    report.pivots = vec![PivotTable {
        id: 7,
        name: "ピボットテーブル1".into(),
        source: "明細!$A$1:$C$5".into(),
        anchor: CellRef::new(2, 0),
        rows: vec![PivotField {
            label_filter: Some(PivotLabelFilter { op: PivotLabelOp::EndsWith, value: "店".into(), value2: String::new() }),
            ..field("店舗")
        }],
        values: vec![PivotValue { source_col: "金額".into(), name: "合計 / 金額".into(), ..PivotValue::default() }],
        filters: vec![PivotFilter {
            source_col: "商品".into(), selected: Some(vec!["あんぱん".into(), "食パン".into()]), hidden_only: true
        }],
        blank_rows: true,
        repeat_labels: true,
        show_expand_buttons: false,
        ..PivotTable::default()
    }];
    report.slicers.push(Slicer {
        id: 20,
        table: String::new(),
        column: "商品".into(),
        caption: "商品".into(),
        anchor: Anchor { cell: CellRef::new(2, 4), dx: 4.0, dy: 4.0, width: 168.0, height: 200.0, mode: AnchorMode::MoveOnly },
        selected: None,
        columns: 1,
        pivot: Some(7),
    });
    wb.sheets.push(Arc::new(report));
    wb
}

#[test]
fn a_pivot_slicer_is_written_with_its_cache_and_pivot_connection() {
    let wb = workbook();
    let parts = unzip(&write_xlsx(&wb).unwrap());
    let cache = &parts["xl/slicerCaches/slicerCache1.xml"];
    assert!(cache.contains("name=\"スライサー_商品\" sourceName=\"商品\""), "{cache}");
    assert!(cache.contains("<pivotTables><pivotTable tabId=\"2\" name=\"ピボットテーブル1\"/></pivotTables>"), "{cache}");
    assert!(cache.contains("<tabular pivotCacheId=\"1\"><items count=\"3\">"), "{cache}");
    // Shared items in first-seen order: あんぱん, 食パン, カレーパン; カレーパン isn't selected.
    assert!(cache.contains("<i x=\"0\" s=\"1\"/><i x=\"1\" s=\"1\"/><i x=\"2\"/>"), "{cache}");
    let def = &parts["xl/pivotCache/pivotCacheDefinition1.xml"];
    assert!(def.contains("<x14:pivotCacheDefinition pivotCacheId=\"1\"/>"), "{def}");
    let book = &parts["xl/workbook.xml"];
    assert!(book.contains("{BBE1A952-AA13-448e-AADC-164F8A28A991}"), "{book}");
    assert!(book.contains("<definedName name=\"スライサー_商品\">#N/A</definedName>"), "{book}");
    let sheet = &parts["xl/worksheets/sheet2.xml"];
    assert!(sheet.contains("{A8765BA9-456A-4dab-B4F3-ACF838C121DE}"), "{sheet}");
    let slicers = &parts["xl/slicers/slicer1.xml"];
    assert!(slicers.contains("<slicer name=\"商品\" cache=\"スライサー_商品\" caption=\"商品\""), "{slicers}");
    // The field is in no area: its unselected item is hidden, and it isn't a page field.
    let table = &parts["xl/pivotTables/pivotTable1.xml"];
    assert!(!table.contains("<pageFields"), "{table}");
    assert!(table.contains("<item x=\"2\" h=\"1\"/>"), "{table}");
}

#[test]
fn filters_and_design_options_are_written_as_excel_writes_them() {
    let mut wb = workbook();
    let report = Arc::make_mut(&mut wb.sheets[1]);
    report.pivots[0].columns = vec![PivotField {
        value_filter: Some(PivotValueFilter {
            op: PivotValueOp::Top10,
            value_field: "合計 / 金額".into(),
            value: 2.0,
            bottom: true,
            ..PivotValueFilter::default()
        }),
        ..field("商品")
    }];
    report.pivots[0].filters.clear();
    report.slicers.clear();
    let parts = unzip(&write_xlsx(&wb).unwrap());
    let table = &parts["xl/pivotTables/pivotTable1.xml"];
    assert!(table.contains("showDrill=\"0\""), "{table}");
    assert!(table.contains("insertBlankRow=\"1\""), "{table}");
    assert!(table.contains("<x14:pivotField fillDownLabels=\"1\"/>"), "{table}");
    assert!(table.contains("<filters count=\"2\">"), "{table}");
    assert!(
        table.contains("<filter fld=\"0\" type=\"captionEndsWith\" evalOrder=\"-1\" id=\"1\" stringValue1=\"店\"><autoFilter ref=\"A1\"><filterColumn colId=\"0\"><customFilters><customFilter val=\"*店\"/></customFilters></filterColumn></autoFilter></filter>"),
        "{table}"
    );
    assert!(table.contains("type=\"count\" evalOrder=\"-1\" id=\"2\" iMeasureFld=\"0\""), "{table}");
    assert!(table.contains("<top10 top=\"0\" val=\"2\" filterVal=\"2\"/>"), "{table}");
    // The filters come after the style info, as the schema orders them.
    assert!(table.find("<pivotTableStyleInfo").unwrap() < table.find("<filters").unwrap());

    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let pt = &back.sheet(1).unwrap().pivots[0];
    assert_eq!(pt.rows[0].label_filter, Some(PivotLabelFilter { op: PivotLabelOp::EndsWith, value: "店".into(), value2: String::new() }));
    let vf = pt.columns[0].value_filter.clone().unwrap();
    assert_eq!((vf.op, vf.value, vf.bottom, vf.top_by, vf.value_field.as_str()), (PivotValueOp::Top10, 2.0, true, PivotTopBy::Items, "合計 / 金額"));
    assert!(pt.blank_rows && pt.repeat_labels && !pt.show_expand_buttons);
}

#[test]
fn a_pivot_slicer_reads_back_connected_to_its_pivot() {
    let wb = workbook();
    let (back, report) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert!(report.warnings.iter().all(|w| !w.contains("slicer")), "{:?}", report.warnings);
    let sh = back.sheet(1).unwrap();
    let pt = &sh.pivots[0];
    let sl = sh.slicers.first().expect("the slicer reads back");
    assert_eq!(sl.pivot, Some(pt.id));
    assert_eq!((sl.column.as_str(), sl.caption.as_str()), ("商品", "商品"));
    let f = pt.filters.iter().find(|f| f.source_col == "商品").unwrap();
    assert!(f.hidden_only);
    assert_eq!(f.selected, Some(vec!["あんぱん".to_string(), "食パン".to_string()]));
    // And writes again the same way.
    let parts = unzip(&write_xlsx(&back).unwrap());
    assert!(parts["xl/slicerCaches/slicerCache1.xml"].contains("<i x=\"2\"/>"));
}

#[test]
fn a_slicer_whose_pivot_is_gone_is_not_written() {
    let mut wb = workbook();
    Arc::make_mut(&mut wb.sheets[1]).pivots.clear();
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert!(!parts.keys().any(|k| k.starts_with("xl/slicer")), "{:?}", parts.keys().collect::<Vec<_>>());
    assert!(!parts["xl/workbook.xml"].contains("スライサー_"));
}
