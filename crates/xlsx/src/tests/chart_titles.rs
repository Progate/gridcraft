//! Chart titles as Excel writes them: automatic (`c:title` without text), typed text, and
//! deleted (`autoTitleDeleted`).

use std::sync::Arc;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Anchor, Chart, ChartKind, LegendPos, Series, Sheet, Workbook};

use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

fn chart(title: Option<&str>, auto_title: bool, series: usize) -> Chart {
    Chart {
        id: 1,
        kind: ChartKind::ColumnClustered,
        anchor: Anchor { width: 480.0, height: 288.0, ..Anchor::default() },
        title: title.map(str::to_string),
        series: (0..series)
            .map(|i| {
                let col = ["B", "C"][i % 2];
                Series {
                    name: Some(format!("Data!${col}$1")),
                    categories: Some("Data!$A$2:$A$3".into()),
                    values: format!("Data!${col}$2:${col}$3"),
                    ..Default::default()
                }
            })
            .collect(),
        legend: LegendPos::None,
        data_labels: false,
        data_table: None,
        gridlines: true,
        style: 2,
        hole_size: 75,
        first_slice_angle: 0,
        x_title: None,
        y_title: None,
        source: None,
        by_rows: false,
        pivot: None,
        alt_text: Default::default(),
        label_parts: Default::default(),
        auto_title,
        title_overlay: false,
        chart_style: None,
    }
}

/// The chart part and the chart read back.
fn save(c: Chart) -> (String, Chart) {
    let mut s = Sheet::new("Data");
    for (r, row) in [["", "2025年度", "2024年度"], ["1月", "10", "7"], ["2月", "20", "14"]].iter().enumerate() {
        for (col, v) in row.iter().enumerate() {
            let value = v.parse::<f64>().map(Value::number).unwrap_or_else(|_| Value::text(*v));
            s.set_value(CellRef::new(r as u32, col as u32), value);
        }
    }
    s.charts.push(c);
    let wb = Workbook { sheets: vec![Arc::new(s)], ..Default::default() };
    let bytes = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&bytes).unwrap();
    let xml = String::from_utf8(pkg.read("xl/charts/chart1.xml").unwrap().unwrap()).unwrap();
    let (back, _) = read_xlsx(&bytes).unwrap();
    (xml, back.sheet(0).unwrap().charts[0].clone())
}

#[test]
fn an_automatic_title_is_a_title_without_text() {
    for n in [1, 2] {
        let (xml, back) = save(chart(None, true, n));
        assert!(xml.contains("<c:chart><c:title><c:overlay val=\"0\"/></c:title><c:autoTitleDeleted val=\"0\"/>"), "{xml}");
        assert!(back.auto_title && back.title.is_none(), "{n} series");
    }
}

#[test]
fn typed_and_deleted_titles_round_trip() {
    let (xml, back) = save(chart(Some("売上"), false, 2));
    assert!(xml.contains("<a:t>売上</a:t>") && xml.contains("<c:autoTitleDeleted val=\"0\"/>"), "{xml}");
    assert_eq!((back.title.as_deref(), back.auto_title), (Some("売上"), false));
    let (xml, back) = save(chart(Some(""), false, 1));
    assert!(xml.contains("<c:autoTitleDeleted val=\"1\"/>") && !xml.contains("<c:title>"), "{xml}");
    assert_eq!((back.title.as_deref(), back.auto_title), (Some(""), false), "a deleted title stays deleted");
}

#[test]
fn a_file_without_a_title_shows_the_automatic_one_for_one_series() {
    // Files from other tools often leave out both `c:title` and `autoTitleDeleted`: Excel then
    // titles a chart of one series with its name.
    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:ser><c:idx val="0"/><c:val><c:numRef><c:f>Data!$B$2:$B$3</c:f></c:numRef></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
    let root = crate::xml::parse(xml.as_bytes()).unwrap();
    let c = crate::chart::read_chart(&root).unwrap();
    assert!(c.auto_title && c.title.is_none());
}

#[test]
fn a_centered_overlay_title_round_trips() {
    let mut c = chart(Some("売上"), false, 2);
    c.title_overlay = true;
    let (xml, back) = save(c);
    assert!(xml.contains("<a:t>売上</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"1\"/></c:title>"), "{xml}");
    assert!(back.title_overlay);
    let mut c = chart(None, true, 2);
    c.title_overlay = true;
    let (xml, back) = save(c);
    assert!(xml.contains("<c:title><c:overlay val=\"1\"/></c:title>"), "{xml}");
    assert!(back.title_overlay && back.auto_title);
}

#[test]
fn axis_titles_go_on_the_axis_excel_names() {
    let mut c = chart(Some("来店数"), false, 2);
    c.y_title = Some("来店数（人）".into());
    let (xml, back) = save(c);
    let val = &xml[xml.find("<c:valAx>").unwrap()..];
    assert!(val.contains("<a:t>来店数（人）</a:t>"), "the vertical axis of a column chart is its value axis: {xml}");
    assert!(!xml[..xml.find("<c:valAx>").unwrap()].contains("来店数（人）"));
    assert_eq!((back.axis_title(false), back.axis_title(true)), (Some("来店数（人）"), None));
}

/// The style part a chart is saved with, as Excel 2013 and later write one: related from the
/// chart, with the Chart Styles gallery entry's id.
fn style_part(c: Chart) -> (Option<String>, Chart) {
    let mut s = Sheet::new("Data");
    s.set_value(CellRef::new(1, 1), Value::number(1.0));
    s.charts.push(c);
    let wb = Workbook { sheets: vec![Arc::new(s)], ..Default::default() };
    let bytes = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&bytes).unwrap();
    let rels = pkg.read("xl/charts/_rels/chart1.xml.rels").unwrap().map(|b| String::from_utf8(b).unwrap());
    let style = pkg.read("xl/charts/style1.xml").unwrap().map(|b| String::from_utf8(b).unwrap());
    if let Some(r) = &rels {
        assert!(r.contains("relationships/chartStyle") && r.contains("Target=\"style1.xml\""), "{r}");
        assert!(r.contains("relationships/chartColorStyle"), "{r}");
    }
    let (back, _) = read_xlsx(&bytes).unwrap();
    (style, back.sheet(0).unwrap().charts[0].clone())
}

#[test]
fn the_chart_style_is_saved_as_excels_style_part() {
    let (style, back) = style_part(chart(None, true, 1));
    assert!(style.unwrap().contains("<cs:chartStyle xmlns:cs=\"http://schemas.microsoft.com/office/drawing/2012/chartStyle\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" id=\"201\">"));
    assert_eq!((back.chart_style, back.gallery_style()), (Some(201), Some(1)));
    let mut c = chart(None, true, 1);
    c.chart_style = Some(203);
    let (style, back) = style_part(c);
    assert!(style.unwrap().contains(" id=\"203\">"));
    assert_eq!(back.gallery_style(), Some(3));
    let mut c = chart(None, true, 1);
    c.kind = ChartKind::Line;
    let (style, back) = style_part(c);
    assert!(style.unwrap().contains(" id=\"227\">"), "a line chart's Style 1");
    assert_eq!(back.gallery_style(), Some(1));
    // A type without a gallery keeps no style part.
    let mut c = chart(None, true, 1);
    c.kind = ChartKind::Radar;
    let (style, back) = style_part(c);
    assert_eq!((style, back.chart_style), (None, None));
}
