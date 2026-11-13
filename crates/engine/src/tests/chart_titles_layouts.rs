//! Select Data keeping Switch Row/Column, Excel's automatic chart titles, and Quick Layouts.

use serde_json::json;

use super::{reopen, s};
use crate::Session;
use gridcraft_model::{Chart, LegendPos};

fn chart(s: &Session) -> Chart {
    s.doc().unwrap().wb.active().unwrap().charts.last().unwrap().clone()
}

fn shown(s: &Session) -> Option<String> {
    crate::cmd::insert::shown_chart_title(&s.doc().unwrap().wb, 0, &chart(s), s.prefs.chart_title())
}

fn monthly(s: &mut Session) {
    let mut rows = vec![json!(["月", "2025年度", "2024年度"])];
    for m in 1..=12 {
        rows.push(json!([format!("{m}月"), m * 10, m * 7]));
    }
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
}

#[test]
fn select_data_keeps_series_in_rows_after_switch_row_column() {
    let mut s = s();
    monthly(&mut s);
    let id = s.execute("insert.chart", json!({"range": "A1:B13", "type": "column"})).unwrap()["chart"].clone();
    s.execute("chart.switchRowColumn", json!({"chart": id})).unwrap();
    assert!(chart(&s).by_rows);
    s.execute("chart.selectData", json!({"chart": id, "range": "Sheet1!A1:C13"})).unwrap();
    let c = chart(&s);
    assert!(c.by_rows, "a new range keeps the series in rows");
    assert_eq!(c.series.len(), 12);
    assert_eq!(c.series[0].values, "Sheet1!$B$2:$C$2");
    // `byRows` sets the orientation, with a range or alone (the dialog's Switch Row/Column).
    s.execute("chart.selectData", json!({"chart": id, "byRows": false})).unwrap();
    assert_eq!((chart(&s).by_rows, chart(&s).series.len()), (false, 2));
    s.execute("chart.selectData", json!({"chart": id, "range": "A1:C13", "byRows": true})).unwrap();
    assert_eq!(chart(&s).series.len(), 12);
    assert!(s.execute("chart.selectData", json!({"chart": id, "byRows": true})).is_ok(), "no change is fine");
}

#[test]
fn select_data_preview_does_not_change_the_chart() {
    let mut s = s();
    monthly(&mut s);
    s.execute("insert.chart", json!({"range": "A1:C13", "type": "column"})).unwrap();
    let (_, _, c) = crate::cmd::extra::select_data_preview(&s, &json!({"byRows": true})).unwrap();
    assert_eq!(c.series.len(), 12);
    assert_eq!(chart(&s).series.len(), 2);
}

#[test]
fn the_automatic_title_follows_the_series() {
    let mut s = s();
    s.prefs.language = "ja".into();
    monthly(&mut s);
    s.execute("insert.chart", json!({"range": "A1:B13", "type": "column"})).unwrap();
    let c = chart(&s);
    assert!(c.auto_title && c.title.is_none());
    assert_eq!(shown(&s).as_deref(), Some("2025年度"));
    s.execute("chart.selectData", json!({"range": "A1:C13"})).unwrap();
    assert_eq!(shown(&s).as_deref(), Some("グラフ タイトル"), "two series show the placeholder");
    s.execute("chart.selectData", json!({"range": "A1:A13,C1:C13"})).unwrap();
    assert_eq!(shown(&s).as_deref(), Some("2024年度"));
    // Saved as Excel's automatic title and read back as one.
    reopen(&mut s);
    assert!(chart(&s).auto_title);
    assert_eq!(shown(&s).as_deref(), Some("2024年度"));
    // Typed text is literal and stays when the series change.
    s.execute("chart.set", json!({"title": "売上推移"})).unwrap();
    s.execute("chart.selectData", json!({"range": "A1:C13"})).unwrap();
    assert_eq!((chart(&s).auto_title, shown(&s).as_deref()), (false, Some("売上推移")));
    // Chart Title ▸ None deletes it; Above Chart brings the automatic title back.
    s.execute("chart.addElement", json!({"element": "title", "on": false})).unwrap();
    assert_eq!(shown(&s), None);
    reopen(&mut s);
    assert_eq!(shown(&s), None, "autoTitleDeleted");
    s.execute("chart.addElement", json!({"element": "title"})).unwrap();
    assert!(chart(&s).auto_title);
    assert_eq!(shown(&s).as_deref(), Some("グラフ タイトル"));
    s.execute("chart.set", json!({"title": "x"})).unwrap();
    s.execute("chart.set", json!({"autoTitle": true})).unwrap();
    assert_eq!(shown(&s).as_deref(), Some("グラフ タイトル"));
}

#[test]
fn quick_layouts_set_excels_elements() {
    let mut s = s();
    s.prefs.language = "ja".into();
    monthly(&mut s);
    s.execute("insert.chart", json!({"range": "A1:C13", "type": "column"})).unwrap();
    s.execute("chart.quickLayout", json!({"layout": 5})).unwrap();
    let c = chart(&s);
    assert!(c.auto_title && c.data_table.as_ref().is_some_and(|t| t.show_keys) && c.gridlines);
    assert_eq!((c.legend, c.x_title.as_deref(), c.y_title.as_deref()), (LegendPos::None, None, Some("軸ラベル")));
    s.execute("chart.quickLayout", json!({"layout": 2})).unwrap();
    let c = chart(&s);
    assert!(c.data_labels && !c.gridlines && c.data_table.is_none() && c.y_title.is_none());
    assert_eq!(c.legend, LegendPos::Top);
    s.execute("chart.quickLayout", json!({"layout": 7})).unwrap();
    let c = chart(&s);
    assert_eq!(shown(&s), None);
    assert_eq!((c.legend, c.x_title.is_some(), c.y_title.is_some()), (LegendPos::Right, true, true));
    s.execute("chart.quickLayout", json!({"layout": 11})).unwrap();
    assert!(s.execute("chart.quickLayout", json!({"layout": 12})).is_err());
    assert!(s.execute("chart.quickLayout", json!({"layout": 0})).is_err());
    s.execute("chart.set", json!({"type": "pie"})).unwrap();
    s.execute("chart.quickLayout", json!({"layout": 1})).unwrap();
    let c = chart(&s);
    assert!(c.data_labels && c.label_parts.category_name && c.label_parts.percentage && !c.label_parts.value);
    assert_eq!(c.legend, LegendPos::None);
    assert!(s.execute("chart.quickLayout", json!({"layout": 8})).is_err(), "pies have 7 layouts");
}
