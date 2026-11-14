//! Chart Design ▸ Chart Styles and the Add Chart Element choices for titles: Style n saved as
//! Excel's chart style id, Chart Title ▸ Above Chart / Centered Overlay, and Axis Titles ▸
//! Primary Horizontal / Primary Vertical.

use serde_json::json;

use super::{reopen, s};
use crate::Session;
use gridcraft_model::Chart;

fn chart(s: &Session) -> Chart {
    s.doc().unwrap().wb.active().unwrap().charts.last().unwrap().clone()
}

fn column_chart(s: &mut Session) {
    s.execute("range.setValues", json!({"range": "A1", "values": [["月", "来店数", "前年"], ["4月", 10, 7], ["5月", 20, 14]]})).unwrap();
    s.execute("insert.chart", json!({"range": "A1:C3", "type": "column"})).unwrap();
}

#[test]
fn chart_styles_pick_a_gallery_style_saved_as_its_style_id() {
    let mut s = s();
    column_chart(&mut s);
    assert_eq!((chart(&s).chart_style, chart(&s).gallery_style()), (None, Some(1)));
    s.execute("chart.set", json!({"style": 3})).unwrap();
    assert_eq!((chart(&s).chart_style, chart(&s).gallery_style()), (Some(203), Some(3)));
    reopen(&mut s);
    assert_eq!(chart(&s).gallery_style(), Some(3), "the style part keeps it");
    assert!(s.execute("chart.set", json!({"style": 0})).is_err());
    assert!(s.execute("chart.set", json!({"style": 15})).is_err(), "a column chart has 14 styles");
    assert!(s.execute("chart.set", json!({"style": "2"})).is_err());
    // Another chart type starts its own gallery at Style 1.
    s.execute("chart.set", json!({"type": "line"})).unwrap();
    assert_eq!((chart(&s).chart_style, chart(&s).chart_style_id()), (None, Some(227)));
    s.execute("chart.set", json!({"type": "radar"})).unwrap();
    assert!(s.execute("chart.set", json!({"style": 1})).is_err(), "no gallery for a radar chart");
}

#[test]
fn chart_title_above_or_centered_overlay() {
    let mut s = s();
    column_chart(&mut s);
    s.execute("chart.addElement", json!({"element": "title", "position": "overlay"})).unwrap();
    assert!(chart(&s).title_overlay && chart(&s).auto_title);
    reopen(&mut s);
    assert!(chart(&s).title_overlay);
    s.execute("chart.addElement", json!({"element": "title", "position": "above"})).unwrap();
    assert!(!chart(&s).title_overlay);
    s.execute("chart.set", json!({"titlePosition": "overlay"})).unwrap();
    assert!(chart(&s).title_overlay);
    assert!(s.execute("chart.addElement", json!({"element": "title", "position": "left"})).is_err());
    assert!(s.execute("chart.set", json!({"titlePosition": "below"})).is_err());
}

#[test]
fn axis_titles_are_added_one_axis_at_a_time() {
    let mut s = s();
    s.prefs.language = "ja".into();
    column_chart(&mut s);
    s.execute("chart.addElement", json!({"element": "axisTitle", "axis": "vertical"})).unwrap();
    let c = chart(&s);
    assert_eq!((c.axis_title(false), c.axis_title(true)), (Some("軸ラベル"), None));
    assert_eq!(c.y_title.as_deref(), Some("軸ラベル"), "a column chart's vertical axis is its value axis");
    s.execute("chart.set", json!({"yTitle": "来店数（人）"})).unwrap();
    s.execute("chart.addElement", json!({"element": "axisTitle", "axis": "vertical"})).unwrap();
    assert_eq!(chart(&s).y_title.as_deref(), Some("来店数（人）"), "adding it again keeps the typed title");
    s.execute("chart.addElement", json!({"element": "axisTitle", "axis": "horizontal"})).unwrap();
    assert_eq!(chart(&s).x_title.as_deref(), Some("軸ラベル"));
    s.execute("chart.addElement", json!({"element": "axisTitle", "axis": "horizontal", "on": false})).unwrap();
    assert_eq!((chart(&s).x_title, chart(&s).y_title.as_deref()), (None, Some("来店数（人）")));
    // A bar chart's value axis runs across: its horizontal axis title is the value axis's.
    s.execute("chart.set", json!({"type": "bar", "yTitle": null})).unwrap();
    s.execute("chart.addElement", json!({"element": "axisTitle", "axis": "horizontal"})).unwrap();
    assert_eq!(chart(&s).y_title.as_deref(), Some("軸ラベル"));
    assert!(s.execute("chart.addElement", json!({"element": "axisTitle", "axis": "depth"})).is_err());
    s.execute("chart.set", json!({"type": "pie"})).unwrap();
    assert!(s.execute("chart.addElement", json!({"element": "axisTitle", "axis": "vertical"})).is_err(), "a pie chart has no axes");
}

#[test]
fn sparkline_set_changes_the_whole_group_and_saves_high_and_low_points() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1, 5, 2], [3, 1, 4]]})).unwrap();
    s.execute("insert.sparkline", json!({"range": "A1:C2", "location": "E1:E2"})).unwrap();
    s.execute("sparkline.set", json!({"range": "E1", "high": true, "low": true, "markers": true})).unwrap();
    let sparks = |s: &Session| s.doc().unwrap().wb.active().unwrap().sparklines.clone();
    assert!(sparks(&s).iter().all(|sp| sp.high && sp.low && sp.markers), "E2 is in E1's group");
    s.execute("sparkline.set", json!({"range": "E2", "type": "column"})).unwrap();
    assert!(sparks(&s).iter().all(|sp| sp.kind == gridcraft_model::SparklineKind::Column));
    reopen(&mut s);
    assert!(sparks(&s).iter().all(|sp| sp.high && sp.low && sp.markers && sp.kind == gridcraft_model::SparklineKind::Column));
    assert!(s.execute("sparkline.set", json!({"range": "A1", "high": true})).is_err(), "no sparkline there");
    assert!(s.execute("sparkline.set", json!({"range": "E1", "type": "area"})).is_err());
    assert!(s.execute("sparkline.set", json!({"range": "E1", "high": "yes"})).is_err());
}
