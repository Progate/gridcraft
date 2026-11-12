//! Ignore Error (`<ignoredErrors>`) and the Watch Window (`<cellWatches>`): written where
//! CT_Worksheet puts them, as Excel writes them, and read back.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::*;

use crate::{read_xlsx, write_xlsx};

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

fn workbook() -> Workbook {
    let mut wb = Workbook::new();
    let mut sh = wb.sheet(0).unwrap().clone();
    sh.set_value(CellRef::parse("B5").unwrap(), gridcraft_core::Value::number(1.0));
    let mut rules = ErrorRules::NONE;
    rules.insert(ErrorRule::Formula);
    sh.ignored_errors.push(IgnoredError { ranges: vec![RangeRef::parse("B5").unwrap()], rules });
    let mut two = ErrorRules::NONE;
    two.insert(ErrorRule::NumberStoredAsText);
    two.insert(ErrorRule::EvalError);
    sh.ignored_errors.push(IgnoredError { ranges: vec![RangeRef::parse("C1:C3").unwrap(), RangeRef::parse("E2").unwrap()], rules: two });
    sh.cell_watches = vec![CellRef::parse("B5").unwrap(), CellRef::parse("A1").unwrap()];
    sh.print.row_breaks = vec![4];
    wb.sheets = vec![Arc::new(sh)];
    wb
}

#[test]
fn ignored_errors_and_cell_watches_are_written_in_schema_order() {
    let parts = unzip(&write_xlsx(&workbook()).unwrap());
    let sheet = &parts["xl/worksheets/sheet1.xml"];
    assert!(
        sheet.contains(r#"<cellWatches><cellWatch r="B5"/><cellWatch r="A1"/></cellWatches><ignoredErrors><ignoredError sqref="B5" formula="1"/><ignoredError sqref="C1:C3 E2" evalError="1" numberStoredAsText="1"/></ignoredErrors>"#),
        "{sheet}"
    );
    // CT_Worksheet: … pageSetup, headerFooter, rowBreaks, colBreaks, customProperties,
    // cellWatches, ignoredErrors, smartTags, drawing …
    let at = |tag: &str| sheet.find(tag).unwrap_or_else(|| panic!("{tag} missing"));
    assert!(at("<pageMargins") < at("<rowBreaks") && at("<rowBreaks") < at("<cellWatches") && at("<cellWatches") < at("<ignoredErrors"));
}

#[test]
fn ignored_errors_and_cell_watches_read_back() {
    let (back, _) = read_xlsx(&write_xlsx(&workbook()).unwrap()).unwrap();
    let sh = back.sheet(0).unwrap();
    assert_eq!(sh.ignored_errors, workbook().sheet(0).unwrap().ignored_errors);
    assert_eq!(sh.cell_watches, workbook().sheet(0).unwrap().cell_watches);
    assert!(sh.ignores_error(CellRef::parse("C2").unwrap(), ErrorRule::EvalError));
    assert!(!sh.ignores_error(CellRef::parse("C2").unwrap(), ErrorRule::Formula));
}
