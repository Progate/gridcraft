//! Links to other workbooks: formulas `[1]予算!B3` with `xl/externalLinks/externalLink1.xml` read
//! into the model as `[予算.xlsx]予算!B3` with the cached values, and written back as Excel does.

use std::collections::BTreeMap;
use std::io::Read;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{ExternalLink, ExternalSheet, Formula, Workbook};

use super::minimal;
use crate::{read_xlsx, write_xlsx};

fn part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap_or_else(|_| panic!("no {name}")).read_to_string(&mut s).unwrap();
    s
}

/// A file as Excel writes one whose A1 reads another workbook at `target`.
fn linked_file(formula: &str, target: &str) -> Vec<u8> {
    let link = r#"<?xml version="1.0"?><externalLink xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><externalBook r:id="rId1"><sheetNames><sheetName val="予算"/><sheetName val="メモ"/></sheetNames><sheetDataSet><sheetData sheetId="0"><row r="3"><cell r="B3"><v>120</v></cell><cell r="C3" t="str"><v>家賃</v></cell></row></sheetData><sheetData sheetId="1"/></sheetDataSet></externalBook></externalLink>"#;
    let link_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath" Target="{target}" TargetMode="External"/></Relationships>"#
    );
    minimal(
        &format!(r#"<sheetData><row r="1"><c r="A1"><f>{formula}</f><v>120</v></c></row></sheetData>"#),
        &[("xl/externalLinks/externalLink1.xml", link), ("xl/externalLinks/_rels/externalLink1.xml.rels", &link_rels)],
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="externalLinks/externalLink1.xml"/>"#,
        r#"<externalReferences><externalReference r:id="rId9"/></externalReferences>"#,
    )
}

#[test]
fn excels_links_read_into_the_model_with_their_values() {
    let (wb, _) = read_xlsx(&linked_file("[1]予算!B3", "予算.xlsx")).unwrap();
    let sh = wb.sheet(0).unwrap();
    assert_eq!(sh.input_text(CellRef::new(0, 0)), "=[予算.xlsx]予算!B3");
    assert_eq!(wb.external_links.len(), 1);
    let l = &wb.external_links[0];
    assert_eq!(l.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["予算", "メモ"]);
    assert_eq!(l.sheet("予算").unwrap().cells.get(&CellRef::new(2, 1)), Some(&Value::number(120.0)));
    assert_eq!(l.sheet("予算").unwrap().cells.get(&CellRef::new(2, 2)), Some(&Value::Text("家賃".into())));
    assert!(wb.preserved.as_ref().is_none_or(|p| p.elements.iter().all(|e| e.slot != "externalReferences")), "the link is not kept twice");
    // A link with a folder (Windows, percent-encoded) shows it, quoted, as Excel does.
    let (wb, _) = read_xlsx(&linked_file("SUM([1]予算!$B$3:$B$4)", "file:///C:\\会計\\予算%202025.xlsx")).unwrap();
    assert_eq!(wb.sheet(0).unwrap().input_text(CellRef::new(0, 0)), "=SUM('C:\\会計\\[予算 2025.xlsx]予算'!$B$3:$B$4)");
}

#[test]
fn links_are_written_as_excel_writes_them_and_read_back() {
    let mut wb = Workbook::new();
    let mut cells = BTreeMap::new();
    cells.insert(CellRef::new(2, 1), Value::number(120.0));
    cells.insert(CellRef::new(3, 1), Value::Text("A&B".into()));
    wb.external_links.push(ExternalLink { target: "予算 2025.xlsx".into(), sheets: vec![ExternalSheet { name: "予算".into(), cells }] });
    let sh = wb.sheet_mut(0).unwrap();
    for (row, f) in ["='[予算 2025.xlsx]予算'!B3", "=[実績.xlsx]実績!$C$5*2", "=SUM(1,2)"].iter().enumerate() {
        let cell = gridcraft_model::Cell { formula: Some(std::sync::Arc::new(Formula::new(f))), ..Default::default() };
        sh.set_cell(CellRef::new(row as u32, 0), cell);
    }
    let bytes = write_xlsx(&wb).unwrap();
    let wbx = part(&bytes, "xl/workbook.xml");
    assert!(wbx.find("</sheets>").unwrap() < wbx.find("<externalReferences>").unwrap());
    assert_eq!(wbx.matches("<externalReference ").count(), 2, "{wbx}");
    let sx = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sx.contains("<f>[1]予算!B3</f>") && sx.contains("<f>[2]実績!$C$5*2</f>"), "{sx}");
    let l1 = part(&bytes, "xl/externalLinks/externalLink1.xml");
    assert!(l1.contains(r#"<sheetName val="予算"/>"#) && l1.contains(r#"<row r="3"><cell r="B3"><v>120</v></cell></row>"#), "{l1}");
    assert!(l1.contains(r#"<cell r="B4" t="str"><v>A&amp;B</v></cell>"#), "{l1}");
    assert!(part(&bytes, "xl/externalLinks/_rels/externalLink1.xml.rels").contains(r#"Target="予算%202025.xlsx" TargetMode="External""#));
    let l2 = part(&bytes, "xl/externalLinks/externalLink2.xml");
    assert!(l2.contains(r#"<sheetName val="実績"/>"#), "a workbook not read yet still lists the sheet: {l2}");
    assert!(part(&bytes, "[Content_Types].xml").contains("/xl/externalLinks/externalLink1.xml"));
    // Read back: the same formulas and values.
    let (back, _) = read_xlsx(&bytes).unwrap();
    let sh = back.sheet(0).unwrap();
    assert_eq!(sh.input_text(CellRef::new(0, 0)), "='[予算 2025.xlsx]予算'!B3");
    assert_eq!(sh.input_text(CellRef::new(1, 0)), "=[実績.xlsx]実績!$C$5*2");
    assert_eq!(back.external_value("[予算 2025.xlsx]予算", gridcraft_core::RangeRef::cell(CellRef::new(2, 1))), Some(Value::number(120.0)));
    assert_eq!(back.external_links[0].target, "予算 2025.xlsx");
}

#[test]
fn a_workbook_without_links_writes_no_link_parts() {
    let bytes = write_xlsx(&Workbook::new()).unwrap();
    assert!(!part(&bytes, "xl/workbook.xml").contains("externalReferences"));
}

#[test]
#[cfg(unix)]
fn saving_into_another_folder_rebases_a_relative_link() {
    let mut wb = Workbook::new();
    wb.source_dir = Some("/books/2026".into());
    wb.external_links.push(ExternalLink { target: "予算 2025.xlsx".into(), sheets: vec![] });
    wb.external_links.push(ExternalLink { target: "/abs/実績.xlsx".into(), sheets: vec![] });
    let out = crate::rebase_links(&wb, std::path::Path::new("/books")).unwrap();
    assert_eq!(out.external_links[0].target, "2026/予算 2025.xlsx");
    assert_eq!(out.external_links[1].target, "/abs/実績.xlsx", "an absolute path stays");
    assert!(crate::rebase_links(&wb, std::path::Path::new("/books/2026")).is_none());
}
