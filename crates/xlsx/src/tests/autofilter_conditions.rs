//! AutoFilter condition filters (custom, Top 10, dynamic): the `filterColumn` children Excel
//! writes for them, and reading them back.

use std::sync::Arc;

use gridcraft_core::RangeRef;
use gridcraft_model::{AutoFilter, FilterCriterion, LineInfo, Sheet, Workbook};

use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

fn custom(a: (&str, &str), b: Option<(&str, &str)>, and: bool) -> FilterCriterion {
    FilterCriterion::Custom { a: (a.0.into(), a.1.into()), b: b.map(|b| (b.0.into(), b.1.into())), and }
}

fn dynamic(kind: &str, val: Option<f64>, max_val: Option<f64>) -> FilterCriterion {
    FilterCriterion::Dynamic { kind: kind.into(), val, max_val }
}

fn sheet_xml(criterion: FilterCriterion) -> (String, Workbook) {
    let mut s = Sheet::new("Sheet1");
    s.autofilter = Some(AutoFilter { range: RangeRef::parse("A1:A10").unwrap(), criteria: vec![(0, criterion)] });
    s.rows.insert(2, LineInfo { hidden: true, ..Default::default() });
    let wb = Workbook { sheets: vec![Arc::new(s)], ..Default::default() };
    let bytes = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&bytes).unwrap();
    let xml = String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap();
    let (back, _) = read_xlsx(&bytes).unwrap();
    (xml, back)
}

fn filter_column(xml: &str) -> &str {
    let start = xml.find("<filterColumn colId=\"0\">").unwrap() + "<filterColumn colId=\"0\">".len();
    let end = xml.find("</filterColumn>").unwrap();
    &xml[start..end]
}

#[test]
fn every_condition_filter_is_written_as_excel_writes_it_and_read_back() {
    let cases = [
        (
            custom((">=", "100"), Some(("<", "200")), true),
            r#"<customFilters and="1"><customFilter operator="greaterThanOrEqual" val="100"/><customFilter operator="lessThan" val="200"/></customFilters>"#,
        ),
        (custom(("=", "abc"), None, true), r#"<customFilters and="1"><customFilter val="abc"/></customFilters>"#),
        (custom(("=", "abc*"), Some(("=", "*xyz")), false), r#"<customFilters><customFilter val="abc*"/><customFilter val="*xyz"/></customFilters>"#),
        (custom(("<>", "*abc*"), None, true), r#"<customFilters and="1"><customFilter operator="notEqual" val="*abc*"/></customFilters>"#),
        (custom((">", "46085"), None, true), r#"<customFilters and="1"><customFilter operator="greaterThan" val="46085"/></customFilters>"#),
        (FilterCriterion::Top10 { bottom: false, percent: false, count: 10, filter_val: Some(42.0) }, r#"<top10 val="10" filterVal="42"/>"#),
        (
            FilterCriterion::Top10 { bottom: true, percent: true, count: 10, filter_val: Some(3.5) },
            r#"<top10 top="0" percent="1" val="10" filterVal="3.5"/>"#,
        ),
        (dynamic("aboveAverage", Some(55.5), None), r#"<dynamicFilter type="aboveAverage" val="55.5"/>"#),
        (dynamic("belowAverage", Some(55.5), None), r#"<dynamicFilter type="belowAverage" val="55.5"/>"#),
        (
            dynamic("today", Some(46338.0), Some(46339.0)),
            r#"<dynamicFilter type="today" val="46338" valIso="2026-11-12T00:00:00" maxVal="46339" maxValIso="2026-11-13T00:00:00"/>"#,
        ),
        (
            dynamic("thisMonth", Some(46327.0), Some(46357.0)),
            r#"<dynamicFilter type="thisMonth" val="46327" valIso="2026-11-01T00:00:00" maxVal="46357" maxValIso="2026-12-01T00:00:00"/>"#,
        ),
        (dynamic("M3", None, None), r#"<dynamicFilter type="M3"/>"#),
        (dynamic("Q1", None, None), r#"<dynamicFilter type="Q1"/>"#),
    ];
    for (criterion, want) in cases {
        let (xml, back) = sheet_xml(criterion.clone());
        assert_eq!(filter_column(&xml), want);
        let row = xml.find(r#"<row r="3""#).map(|i| &xml[i..i + xml[i..].find('>').unwrap()]).unwrap_or("");
        assert!(row.contains(r#"hidden="1""#), "the filtered row is written hidden: {row:?}");
        let read = back.sheet(0).unwrap().autofilter.clone().unwrap();
        assert_eq!(read.criteria, vec![(0, criterion)]);
    }
}

#[test]
fn text_operators_from_older_models_are_written_as_wildcards() {
    let (xml, _) = sheet_xml(custom(("contains", "abc"), Some(("notContains", "x")), true));
    assert_eq!(
        filter_column(&xml),
        r#"<customFilters and="1"><customFilter val="*abc*"/><customFilter operator="notEqual" val="*x*"/></customFilters>"#
    );
}
