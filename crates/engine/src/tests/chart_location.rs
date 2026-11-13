//! Move Chart to a chart sheet and back, waterfall Set as Total, and sparklines made over a
//! location range as one group.

use serde_json::json;

use super::s;
use crate::Session;

fn names(s: &Session) -> Vec<String> {
    s.doc().unwrap().wb.sheets.iter().map(|sh| sh.name.clone()).collect()
}

fn with_chart(s: &mut Session, kind: &str) -> u32 {
    s.execute("range.setValues", json!({"range": "A1", "values": [["月", "来店数"], ["1月", 10], ["2月", -4], ["3月", 15]]})).unwrap();
    s.execute("insert.chart", json!({"range": "A1:B4", "type": kind, "at": "D2"})).unwrap()["chart"].as_u64().unwrap() as u32
}

#[test]
fn move_chart_to_a_new_sheet_makes_a_chart_sheet_before_its_sheet() {
    let mut s = s();
    let id = with_chart(&mut s, "column");
    // A name no sheet has makes a chart sheet, as Excel's New sheet does.
    let r = s.execute("chart.move", json!({"sheet": "グラフ1"})).unwrap();
    assert_eq!(r["sheet"], "グラフ1");
    assert_eq!(names(&s), ["グラフ1", "Sheet1"]);
    let wb = &s.doc().unwrap().wb;
    assert_eq!(wb.active_sheet(), 0);
    let cs = wb.sheet(0).unwrap();
    assert!(cs.chart_sheet && cs.charts.len() == 1 && cs.charts[0].id == id);
    assert!(wb.sheet(1).unwrap().charts.is_empty());
    // Undo puts the chart back on its worksheet.
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(names(&s), ["Sheet1"]);
    assert_eq!(s.doc().unwrap().wb.sheet(0).unwrap().charts.len(), 1);
}

#[test]
fn moving_a_chart_sheets_chart_onto_a_worksheet_removes_the_chart_sheet() {
    let mut s = s();
    with_chart(&mut s, "column");
    s.execute("chart.move", json!({"newSheet": "来店数グラフ"})).unwrap();
    // A second chart cannot go onto a chart sheet, and the name must be free.
    assert!(s.execute("chart.move", json!({"newSheet": "Sheet1"})).is_err());
    s.execute("chart.move", json!({"sheet": "Sheet1", "at": "F3"})).unwrap();
    assert_eq!(names(&s), ["Sheet1"]);
    let sh = s.doc().unwrap().wb.sheet(0).unwrap();
    assert!(!sh.chart_sheet);
    assert_eq!(sh.charts.len(), 1);
    assert_eq!(sh.charts[0].anchor.cell.a1(), "F3");
}

#[test]
fn a_chart_sheet_survives_a_save_and_reopen() {
    let mut s = s();
    with_chart(&mut s, "column");
    s.execute("chart.move", json!({"newSheet": "Chart1"})).unwrap();
    let wb = s.doc().unwrap().wb.clone();
    let bytes = gridcraft_xlsx::write_xlsx(&wb).unwrap();
    let (back, _) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    assert!(back.sheet(0).unwrap().chart_sheet);
    assert_eq!(back.sheet(0).unwrap().name, "Chart1");
}

#[test]
fn set_as_total_marks_waterfall_points() {
    let mut s = s();
    let id = with_chart(&mut s, "waterfall");
    for p in [3, 0, 3] {
        s.execute("chart.setAsTotal", json!({"chart": id, "point": p})).unwrap();
    }
    let totals = |s: &Session| s.doc().unwrap().wb.sheet(0).unwrap().charts[0].series[0].subtotals.clone();
    assert_eq!(totals(&s), [0, 3]);
    s.execute("chart.setAsTotal", json!({"chart": id, "point": 0, "on": false})).unwrap();
    assert_eq!(totals(&s), [3]);
    assert!(s.execute("chart.setAsTotal", json!({"chart": id})).is_err(), "a point is needed");
    assert!(s.execute("chart.setAsTotal", json!({"chart": id, "point": 1, "on": "yes"})).is_err());
    let mut s2 = super::s();
    let id = with_chart(&mut s2, "column");
    assert!(s2.execute("chart.setAsTotal", json!({"chart": id, "point": 1})).is_err(), "only waterfall charts have totals");
}

#[test]
fn sparklines_over_a_location_range_are_one_group_one_per_row() {
    let mut s = s();
    let rows: Vec<Vec<f64>> = (0..4).map(|r| (0..12).map(|c| f64::from(r * 12 + c)).collect()).collect();
    s.execute("range.setValues", json!({"range": "B4", "values": rows})).unwrap();
    let r = s.execute("insert.sparkline", json!({"range": "B4:M7", "location": "N4:N7", "type": "column"})).unwrap();
    assert_eq!(r["count"], 4);
    let sh = s.doc().unwrap().wb.sheet(0).unwrap();
    let got: Vec<(String, String, u32)> = sh.sparklines.iter().map(|x| (x.cell.a1(), x.source.clone(), x.group)).collect();
    assert_eq!(got.len(), 4);
    for (i, (cell, src, group)) in got.iter().enumerate() {
        assert_eq!(cell, &format!("N{}", i + 4));
        assert_eq!(src, &format!("B{0}:M{0}", i + 4));
        assert!(*group != 0 && *group == got[0].2);
    }
    // A row of location cells takes the data's columns, with the sheet the data names.
    s.execute("insert.sparkline", json!({"range": "Sheet1!B4:C7", "location": "B9:C9"})).unwrap();
    let sh = s.doc().unwrap().wb.sheet(0).unwrap();
    let last: Vec<&str> = sh.sparklines.iter().skip(4).map(|x| x.source.as_str()).collect();
    assert_eq!(last, ["Sheet1!B4:B7", "Sheet1!C4:C7"]);
    assert!(sh.sparklines[4].group != sh.sparklines[0].group);
    // Sizes that don't match, and a block of cells, are refused.
    assert!(s.execute("insert.sparkline", json!({"range": "B4:M7", "location": "N4:N6"})).is_err());
    assert!(s.execute("insert.sparkline", json!({"range": "B4:M7", "location": "N4:O7"})).is_err());
    // One cell keeps the whole range.
    s.execute("insert.sparkline", json!({"range": "B4:M4", "location": "P4"})).unwrap();
    let sh = s.doc().unwrap().wb.sheet(0).unwrap();
    assert_eq!(sh.sparklines.last().map(|x| (x.source.as_str(), x.group)), Some(("B4:M4", 0)));
}

fn series_refs(s: &Session) -> Vec<(Option<String>, Option<String>, String)> {
    let sh = s.doc().unwrap().wb.active().unwrap();
    sh.charts.last().unwrap().series.iter().map(|x| (x.name.clone(), x.categories.clone(), x.values.clone())).collect()
}

#[test]
fn a_date_column_under_a_header_labels_the_categories() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A1", "values": [["月", "売上", "客数"], [45292, 100, 10], [45323, 120, 12], [45352, 90, 9]]}))
        .unwrap();
    s.execute("home.numberFormat", json!({"range": "A2:A4", "format": "yyyy/m/d"})).unwrap();
    s.execute("insert.chart", json!({"range": "A1:C4", "type": "column"})).unwrap();
    let got = series_refs(&s);
    assert_eq!(got.len(), 2, "the dates are not a series: {got:?}");
    assert_eq!(got[0], (Some("Sheet1!$B$1".into()), Some("Sheet1!$A$2:$A$4".into()), "Sheet1!$B$2:$B$4".into()));
}

#[test]
fn a_header_row_and_rows_further_down_make_one_chart() {
    let mut s = s();
    s.execute(
        "range.setValues",
        json!({"range": "B3", "values": [["", "東京", "大阪"], ["skip", 0, 0], ["skip", 0, 0], ["skip", 0, 0], ["skip", 0, 0], ["skip", 0, 0], ["1月", 1, 2], ["2月", 3, 4], ["3月", 5, 6]]}),
    )
    .unwrap();
    s.execute("insert.chart", json!({"range": "B3:D3,B9:D11", "type": "column"})).unwrap();
    let got = series_refs(&s);
    assert_eq!(
        got,
        [
            (Some("Sheet1!$C$3".into()), Some("Sheet1!$B$9:$B$11".into()), "Sheet1!$C$9:$C$11".into()),
            (Some("Sheet1!$D$3".into()), Some("Sheet1!$B$9:$B$11".into()), "Sheet1!$D$9:$D$11".into()),
        ]
    );
    // Select Data takes the same areas, sheet names and all.
    let id = s.doc().unwrap().wb.active().unwrap().charts[0].id;
    s.execute("chart.selectData", json!({"chart": id, "range": "=Sheet1!$B$3:$C$3,Sheet1!$B$9:$C$11"})).unwrap();
    assert_eq!(series_refs(&s), [(Some("Sheet1!$C$3".into()), Some("Sheet1!$B$9:$B$11".into()), "Sheet1!$C$9:$C$11".into())]);
    // Columns side by side over the same rows: labels, then series further right.
    s.execute("insert.chart", json!({"range": "B9:B11,D9:D11", "type": "column"})).unwrap();
    assert_eq!(series_refs(&s), [(None, Some("Sheet1!$B$9:$B$11".into()), "Sheet1!$D$9:$D$11".into())]);
    // Areas that don't line up are refused.
    assert!(s.execute("insert.chart", json!({"range": "B3:C3,D9:D11"})).is_err());
}

#[test]
fn filling_and_pasting_a_sparkline_cell_carries_its_sparkline() {
    let mut s = s();
    let rows: Vec<Vec<f64>> = (0..4).map(|r| (0..3).map(|c| f64::from(r * 3 + c)).collect()).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("insert.sparkline", json!({"range": "A1:C1", "location": "D1"})).unwrap();
    s.execute("edit.autoFill", json!({"source": "D1", "target": "D1:D3"})).unwrap();
    let got = |s: &Session| -> Vec<(String, String, u32)> {
        let mut v: Vec<_> = s.doc().unwrap().wb.sheet(0).unwrap().sparklines.iter().map(|x| (x.cell.a1(), x.source.clone(), x.group)).collect();
        v.sort();
        v
    };
    let filled = got(&s);
    assert_eq!(filled.iter().map(|x| (x.0.as_str(), x.1.as_str())).collect::<Vec<_>>(), [("D1", "A1:C1"), ("D2", "A2:C2"), ("D3", "A3:C3")]);
    assert!(filled.iter().all(|x| x.2 != 0 && x.2 == filled[0].2), "one group: {filled:?}");
    s.execute("selection.set", json!({"range": "D3"})).unwrap();
    s.execute("edit.copy", json!({})).unwrap();
    s.execute("edit.paste", json!({"at": "D4"})).unwrap();
    assert!(got(&s).iter().any(|x| x.0 == "D4" && x.1 == "A4:C4" && x.2 == filled[0].2));
}

#[test]
fn a_single_series_chart_has_no_legend_and_the_default_title_follows_the_language() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A1", "values": [["月", "売上", "客数"], ["1月", 1, 2], ["2月", 3, 4]]})).unwrap();
    s.execute("insert.chart", json!({"range": "A1:B3", "type": "column"})).unwrap();
    let last = |s: &Session| s.doc().unwrap().wb.active().unwrap().charts.last().unwrap().clone();
    let shown = |s: &Session| crate::cmd::insert::shown_chart_title(&s.doc().unwrap().wb, 0, &last(s), s.prefs.chart_title());
    assert_eq!(last(&s).legend, gridcraft_model::LegendPos::None);
    assert_eq!(shown(&s).as_deref(), Some("売上"));
    s.execute("insert.chart", json!({"range": "A1:B3", "type": "pie"})).unwrap();
    assert_eq!(last(&s).legend, gridcraft_model::LegendPos::Right, "a pie names its slices");
    s.prefs.language = "ja".into();
    s.execute("insert.chart", json!({"range": "A1:C3", "type": "column"})).unwrap();
    assert_eq!(last(&s).legend, gridcraft_model::LegendPos::Bottom);
    assert_eq!(shown(&s).as_deref(), Some("グラフ タイトル"));
    s.prefs.language = "en".into();
    s.execute("insert.chart", json!({"range": "A1:C3", "type": "column"})).unwrap();
    assert_eq!(shown(&s).as_deref(), Some("Chart Title"));
}

#[test]
fn format_data_labels_picks_what_a_label_shows() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A1", "values": [["地域", "売上"], ["東", 30], ["西", 70]]})).unwrap();
    let id = s.execute("insert.chart", json!({"range": "A1:B3", "type": "pie"})).unwrap()["chart"].as_u64().unwrap();
    s.execute("chart.formatDataLabels", json!({"chart": id, "categoryName": true, "percentage": true, "value": false})).unwrap();
    let ch = s.doc().unwrap().wb.active().unwrap().charts[0].clone();
    assert!(ch.data_labels);
    assert!(ch.label_parts.category_name && ch.label_parts.percentage && !ch.label_parts.value && ch.label_parts.leader_lines);
    assert!(
        s.execute("chart.formatDataLabels", json!({"chart": id, "categoryName": false, "percentage": false})).is_err(),
        "a label shows something"
    );
    assert!(s.execute("chart.formatDataLabels", json!({"chart": id, "value": "yes"})).is_err());
}
