//! Table slicers: the slicer, slicer cache, workbook and sheet extensions, names and drawing frame
//! Excel writes, and the round trip with the selection.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::*;

use crate::{read_xlsx, write_xlsx};

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn column(name: &str) -> TableColumn {
    TableColumn { name: name.into(), totals: TotalsFn::None, totals_label: None, formula: None, unique_name: None, xml: None }
}

fn workbook(selected: Option<Vec<String>>) -> Workbook {
    let mut wb = Workbook::new();
    let s = wb.sheet_mut(0).unwrap();
    let rows = [["Region", "Product", "Sales"], ["East", "Apples", "10"], ["West", "Pears", "20"], ["East", "Plums", "5"]];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            let value = v.parse::<f64>().map(Value::number).unwrap_or_else(|_| Value::text(*v));
            s.cells.set(CellRef::new(r as u32, c as u32), Cell::value(value));
        }
    }
    s.tables.push(Table {
        id: 1,
        name: "Sales".into(),
        range: RangeRef::new(at("A1"), at("C4")),
        header_row: true,
        totals_row: false,
        columns: vec![column("Region"), column("Product"), column("Sales")],
        style: "TableStyleMedium2".into(),
        banded_rows: true,
        banded_cols: false,
        first_col: false,
        last_col: false,
        filter_button: true,
        filter: vec![],
        table_type: None,
        connection_id: None,
    });
    if let Some(values) = &selected {
        s.autofilter = Some(AutoFilter {
            range: RangeRef::new(at("A1"), at("C4")),
            criteria: vec![(1, FilterCriterion::Values { values: values.clone(), blanks: false })],
        });
    }
    s.slicers.push(Slicer {
        id: 2,
        table: "Sales".into(),
        column: "Product".into(),
        caption: "Product".into(),
        anchor: Anchor { cell: at("E2"), dx: 4.0, dy: 4.0, width: 168.0, height: 200.0, mode: AnchorMode::MoveOnly },
        selected,
        columns: 2,
    });
    s.slicers.push(Slicer {
        id: 3,
        table: "Sales".into(),
        column: "Region".into(),
        caption: "Region".into(),
        anchor: Anchor { cell: at("H2"), dx: 4.0, dy: 4.0, width: 168.0, height: 200.0, mode: AnchorMode::MoveOnly },
        selected: None,
        columns: 1,
    });
    wb.sheets = vec![Arc::new(wb.sheet(0).unwrap().clone())];
    wb
}

fn unzip(bytes: &[u8]) -> HashMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut m = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        m.insert(f.name().to_string(), String::from_utf8_lossy(&b).into_owned());
    }
    m
}

#[test]
fn table_slicers_are_written_as_excel_writes_them() {
    let parts = unzip(&write_xlsx(&workbook(Some(vec!["Apples".into(), "Plums".into()]))).unwrap());
    let slicers = &parts["xl/slicers/slicer1.xml"];
    assert!(slicers.contains(r#"<slicer name="Product" cache="Slicer_Product" caption="Product" columnCount="2" rowHeight="241300"/>"#), "{slicers}");
    assert!(slicers.contains(r#"<slicer name="Region" cache="Slicer_Region" caption="Region" rowHeight="241300"/>"#), "{slicers}");
    let cache = &parts["xl/slicerCaches/slicerCache1.xml"];
    assert!(cache.contains(r#"name="Slicer_Product" sourceName="Product""#), "{cache}");
    assert!(cache.contains(r#"<x15:tableSlicerCache tableId="1" column="2"/>"#), "{cache}");
    assert!(parts["xl/slicerCaches/slicerCache2.xml"].contains(r#"<x15:tableSlicerCache tableId="1" column="1"/>"#));
    let ct = &parts["[Content_Types].xml"];
    assert!(ct.contains(r#"<Override PartName="/xl/slicers/slicer1.xml" ContentType="application/vnd.ms-excel.slicer+xml"/>"#), "{ct}");
    assert!(
        ct.contains(r#"<Override PartName="/xl/slicerCaches/slicerCache1.xml" ContentType="application/vnd.ms-excel.slicerCache+xml"/>"#),
        "{ct}"
    );
    let book = &parts["xl/workbook.xml"];
    assert!(book.contains(r#"<definedName name="Slicer_Product">#N/A</definedName>"#), "{book}");
    assert!(
        book.contains(r#"<x15:slicerCaches xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:slicerCache r:id="#),
        "{book}"
    );
    let book_rels = &parts["xl/_rels/workbook.xml.rels"];
    assert!(
        book_rels.contains(r#"Type="http://schemas.microsoft.com/office/2007/relationships/slicerCache" Target="slicerCaches/slicerCache1.xml""#)
    );
    let sheet = &parts["xl/worksheets/sheet1.xml"];
    assert!(sheet.contains(r#"<ext uri="{3A4CF648-6AED-40f4-86FF-DC5316D8AED3}""#), "{sheet}");
    assert!(sheet.contains("<x14:slicerList"), "{sheet}");
    assert!(parts["xl/worksheets/_rels/sheet1.xml.rels"].contains(r#"relationships/slicer" Target="../slicers/slicer1.xml""#));
    let drawing = &parts["xl/drawings/drawing1.xml"];
    assert!(drawing.contains(r#"<sle:slicer xmlns:sle="http://schemas.microsoft.com/office/drawing/2010/slicer" name="Product"/>"#), "{drawing}");
    assert!(drawing.contains(r#"Requires="sle15""#));
    // The selection is the table's filter on the column.
    let table = &parts["xl/tables/table1.xml"];
    assert!(table.contains(r#"<filterColumn colId="1"><filters><filter val="Apples"/><filter val="Plums"/></filters></filterColumn>"#), "{table}");
}

#[test]
fn table_slicers_round_trip_with_their_selection() {
    let wb = workbook(Some(vec!["Apples".into(), "Plums".into()]));
    let (back, report) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert!(report.warnings.iter().all(|w| !w.contains("licer")), "{:?}", report.warnings);
    let s = back.sheet(0).unwrap();
    assert_eq!(s.slicers.len(), 2);
    let (a, b) = (&s.slicers[0], &wb.sheet(0).unwrap().slicers[0]);
    assert_eq!((&a.table, &a.column, &a.caption, a.columns, &a.selected), (&b.table, &b.column, &b.caption, b.columns, &b.selected));
    assert_eq!((a.anchor.cell, a.anchor.mode), (b.anchor.cell, b.anchor.mode));
    assert!((a.anchor.width - b.anchor.width).abs() < 1.0 && (a.anchor.height - b.anchor.height).abs() < 1.0);
    assert_eq!(s.slicers[1].selected, None);
    // The selection is the live sheet filter again, and the #N/A cache names are not user names.
    assert!(s.autofilter.as_ref().is_some_and(|af| af.criteria.len() == 1));
    assert!(back.names.is_empty(), "{:?}", back.names);
    // A second trip writes the same parts.
    let again = unzip(&write_xlsx(&back).unwrap());
    let first = unzip(&write_xlsx(&wb).unwrap());
    for part in ["xl/slicers/slicer1.xml", "xl/slicerCaches/slicerCache1.xml", "xl/slicerCaches/slicerCache2.xml", "xl/tables/table1.xml"] {
        assert_eq!(again[part], first[part], "{part}");
    }
    assert!(!again.contains_key("xl/slicerCaches/slicerCache3.xml"));
}

#[test]
fn a_slicer_on_a_missing_table_is_not_written() {
    let mut wb = workbook(None);
    let s = Arc::make_mut(&mut wb.sheets[0]);
    s.slicers[0].table = "Nope".into();
    s.slicers.truncate(1);
    let parts = unzip(&write_xlsx(&wb).unwrap());
    assert!(!parts.keys().any(|k| k.contains("slicer")));
    assert!(!parts["xl/workbook.xml"].contains("Slicer_"));
}
