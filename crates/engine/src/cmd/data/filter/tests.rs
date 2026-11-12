use serde_json::json;

use super::*;

fn session(values: Json) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": values})).unwrap();
    s
}

fn hidden(s: &Session) -> Vec<u32> {
    s.doc().unwrap().wb.sheet(0).unwrap().rows.iter().filter(|(_, r)| r.hidden).map(|(k, _)| *k).collect()
}

fn criterion(s: &Session, col: u32) -> FilterCriterion {
    filter_criterion_at(&s.doc().unwrap().wb, 0, col).unwrap()
}

#[test]
fn text_operators_are_stored_as_excel_wildcards_and_match_case_insensitively() {
    let mut s = session(json!([["Name"], ["Apple"], ["apricot"], ["Banana"], ["grape"], ["Pineapple"]]));
    s.execute("data.filter", json!({"range": "A1:A6"})).unwrap();
    let cases = [
        ("beginsWith", "ap", "=", "ap*", vec![3, 4, 5]),
        ("endsWith", "APPLE", "=", "*APPLE", vec![2, 3, 4]),
        ("contains", "an", "=", "*an*", vec![1, 2, 4, 5]),
        ("notContains", "an", "<>", "*an*", vec![3]),
        ("notBeginsWith", "a", "<>", "a*", vec![1, 2]),
        ("=", "banana", "=", "banana", vec![1, 2, 4, 5]),
        ("<>", "banana", "<>", "banana", vec![3]),
        ("=", "?rape", "=", "?rape", vec![1, 2, 3, 5]),
    ];
    for (op, value, stored_op, stored, want) in cases {
        s.execute("data.filterBy", json!({"column": "A", "custom": {"op": op, "value": value}})).unwrap();
        let FilterCriterion::Custom { a, b: None, .. } = criterion(&s, 0) else { panic!("{op}: not a custom filter") };
        assert_eq!(a, (stored_op.to_string(), stored.to_string()), "{op}");
        assert_eq!(hidden(&s), want, "{op} {value}");
    }
}

#[test]
fn number_conditions_join_with_and_or_or() {
    let mut s = session(json!([["N"], [50], [100], [150], [200], ["text"], [250]]));
    s.execute("data.filter", json!({"range": "A1:A7"})).unwrap();
    s.execute("data.filterBy", json!({"column": "A", "custom": {"op": ">=", "value": 100, "op2": "<", "value2": 200, "and": true}})).unwrap();
    assert_eq!(hidden(&s), vec![1, 4, 5, 6]);
    s.execute("data.filterBy", json!({"column": "A", "custom": {"op": "lessThan", "value": 100, "op2": "greaterThan", "value2": 200, "and": false}}))
        .unwrap();
    // Text never passes a numeric ordering comparison.
    assert_eq!(hidden(&s), vec![2, 3, 4, 5]);
}

#[test]
fn date_conditions_are_stored_as_serial_numbers() {
    let mut s = session(json!([["Day"], ["2026-03-01"], ["2026-03-04"], ["2026-03-09"]]));
    s.execute("data.filter", json!({"range": "A1:A4"})).unwrap();
    s.execute("data.filterBy", json!({"column": "A", "custom": {"op": ">", "value": "2026-03-04"}})).unwrap();
    let FilterCriterion::Custom { a, .. } = criterion(&s, 0) else { panic!("not a custom filter") };
    assert_eq!(a, (">".to_string(), "46085".to_string()));
    assert_eq!(hidden(&s), vec![1, 2]);
    assert_eq!(filter_column_kind(&s.doc().unwrap().wb, 0, 0), FilterColumnKind::Date);
}

#[test]
fn top10_records_its_cut_off_value() {
    let mut s = session(json!([["N"], [10], [40], [20], [50], [30]]));
    s.execute("data.filter", json!({"range": "A1:A6"})).unwrap();
    s.execute("data.filterBy", json!({"column": "A", "top": {"count": 2}})).unwrap();
    assert!(matches!(criterion(&s, 0), FilterCriterion::Top10 { filter_val: Some(v), .. } if v == 40.0));
    assert_eq!(hidden(&s), vec![1, 3, 5]);
    s.execute("data.filterBy", json!({"column": "A", "top": {"count": 40, "bottom": true, "percent": true}})).unwrap();
    assert!(matches!(criterion(&s, 0), FilterCriterion::Top10 { filter_val: Some(v), .. } if v == 20.0));
    assert_eq!(hidden(&s), vec![2, 4, 5]);
    assert!(s.execute("data.filterBy", json!({"column": "A", "top": {"count": 101, "percent": true}})).is_err());
    assert!(s.execute("data.filterBy", json!({"column": "A", "top": {"count": 0}})).is_err());
}

#[test]
fn average_filters_record_the_average() {
    let mut s = session(json!([["N"], [10], [20], [30], [40]]));
    s.execute("data.filter", json!({"range": "A1:A5"})).unwrap();
    s.execute("data.filterBy", json!({"column": "A", "dynamic": "aboveAverage"})).unwrap();
    assert!(matches!(criterion(&s, 0), FilterCriterion::Dynamic { ref kind, val: Some(v), max_val: None } if kind == "aboveAverage" && v == 25.0));
    assert_eq!(hidden(&s), vec![1, 2]);
    s.execute("data.filterBy", json!({"column": "A", "aboveAverage": false})).unwrap();
    assert_eq!(hidden(&s), vec![3, 4]);
    assert!(s.execute("data.filterBy", json!({"column": "A", "dynamic": "nextDecade"})).is_err());
    assert!(s.execute("data.filterBy", json!({"column": "A", "custom": {"op": "like", "value": "x"}})).is_err());
}

#[test]
fn relative_date_spans_follow_excel_weeks_quarters_and_years() {
    let sys = DateSystem::D1900;
    let ymd = |y, m, d| gridcraft_core::date::serial_from_ymd(sys, y, m, d).unwrap();
    // Thursday 12 November 2026.
    let today = ymd(2026, 11, 12);
    let span = |k| dynamic_span(k, today, sys).unwrap();
    assert_eq!(span("today"), (today, today + 1.0));
    assert_eq!(span("tomorrow"), (today + 1.0, today + 2.0));
    assert_eq!(span("yesterday"), (today - 1.0, today));
    assert_eq!(span("thisWeek"), (ymd(2026, 11, 8), ymd(2026, 11, 15)));
    assert_eq!(span("lastWeek"), (ymd(2026, 11, 1), ymd(2026, 11, 8)));
    assert_eq!(span("nextWeek"), (ymd(2026, 11, 15), ymd(2026, 11, 22)));
    assert_eq!(span("thisMonth"), (ymd(2026, 11, 1), ymd(2026, 12, 1)));
    assert_eq!(span("nextMonth"), (ymd(2026, 12, 1), ymd(2027, 1, 1)));
    assert_eq!(span("lastMonth"), (ymd(2026, 10, 1), ymd(2026, 11, 1)));
    assert_eq!(span("thisQuarter"), (ymd(2026, 10, 1), ymd(2027, 1, 1)));
    assert_eq!(span("nextQuarter"), (ymd(2027, 1, 1), ymd(2027, 4, 1)));
    assert_eq!(span("lastQuarter"), (ymd(2026, 7, 1), ymd(2026, 10, 1)));
    assert_eq!(span("thisYear"), (ymd(2026, 1, 1), ymd(2027, 1, 1)));
    assert_eq!(span("nextYear"), (ymd(2027, 1, 1), ymd(2028, 1, 1)));
    assert_eq!(span("lastYear"), (ymd(2025, 1, 1), ymd(2026, 1, 1)));
    assert_eq!(span("yearToDate"), (ymd(2026, 1, 1), today + 1.0));
    assert_eq!(dynamic_span("M3", today, sys), None);
}

#[test]
fn date_dynamic_filters_hide_rows_outside_the_period() {
    let mut s = session(json!([["Day"], ["2026-11-12"], ["2026-11-13"], ["2026-03-04"], ["2025-03-20"], ["2026-10-31"], ["text"]]));
    s.execute("data.filter", json!({"range": "A1:A7"})).unwrap();
    let today = gridcraft_core::date::serial_from_ymd(DateSystem::D1900, 2026, 11, 12).unwrap();
    let run = |s: &mut Session, kind: &str| {
        s.execute("data.filterBy", json!({"column": "A", "dynamic": kind})).unwrap();
        let d = s.doc_mut().unwrap();
        apply_filter_on(std::sync::Arc::make_mut(&mut d.wb), 0, today);
        hidden(s)
    };
    assert_eq!(run(&mut s, "today"), vec![2, 3, 4, 5, 6]);
    assert!(matches!(criterion(&s, 0), FilterCriterion::Dynamic { val: Some(a), max_val: Some(b), .. } if a == today && b == today + 1.0));
    assert_eq!(run(&mut s, "thisMonth"), vec![3, 4, 5, 6]);
    assert_eq!(run(&mut s, "M3"), vec![1, 2, 5, 6]);
    assert_eq!(run(&mut s, "Q4"), vec![3, 4, 6]);
    assert_eq!(run(&mut s, "lastYear"), vec![1, 2, 3, 5, 6]);
    assert!(matches!(criterion(&s, 0), FilterCriterion::Dynamic { val: Some(_), .. }));
    assert_eq!(run(&mut s, "Q1"), vec![1, 2, 5, 6]);
    assert!(matches!(criterion(&s, 0), FilterCriterion::Dynamic { val: None, max_val: None, .. }));
}

#[test]
fn the_column_kind_follows_most_of_its_values() {
    let mut s = session(json!([["A", "B", "C"], ["x", 1, "2026-01-01"], ["y", 2, "2026-01-02"], [3, "z", "w"]]));
    s.execute("data.filter", json!({"range": "A1:C4"})).unwrap();
    let wb = &s.doc().unwrap().wb;
    assert_eq!(filter_column_kind(wb, 0, 0), FilterColumnKind::Text);
    assert_eq!(filter_column_kind(wb, 0, 1), FilterColumnKind::Number);
    assert_eq!(filter_column_kind(wb, 0, 2), FilterColumnKind::Date);
}

#[test]
fn a_table_column_filter_is_saved_in_the_table_part_and_read_back() {
    use std::io::Read;
    let mut s = session(json!([["Item", "Qty"], ["a", 5], ["b", 150], ["c", 120], ["d", 300]]));
    s.execute("insert.table", json!({"range": "A1:B5", "header": true})).unwrap();
    s.execute("selection.set", json!({"cell": "B2"})).unwrap();
    s.execute("data.filterBy", json!({"column": "B", "custom": {"op": ">=", "value": 100, "op2": "<", "value2": 200}})).unwrap();
    assert_eq!(hidden(&s), vec![1, 4]);
    let bytes = gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let mut table = String::new();
    zip.by_name("xl/tables/table1.xml").unwrap().read_to_string(&mut table).unwrap();
    assert!(
        table.contains(r#"<autoFilter ref="A1:B5"><filterColumn colId="1"><customFilters and="1"><customFilter operator="greaterThanOrEqual" val="100"/><customFilter operator="lessThan" val="200"/></customFilters></filterColumn></autoFilter>"#),
        "{table}"
    );
    let (back, _) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    let sh = back.sheet(0).unwrap();
    assert!(matches!(sh.tables[0].filter.as_slice(), [(1, FilterCriterion::Custom { b: Some(_), and: true, .. })]), "{:?}", sh.tables[0].filter);
    assert!(sh.is_row_hidden(1) && sh.is_row_hidden(4) && !sh.is_row_hidden(2));
}
