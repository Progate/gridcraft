//! Chart sheets, grouped sparklines and waterfall totals: the parts Excel writes for them, and a
//! save and reopen.

use std::sync::Arc;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Anchor, AnchorMode, Chart, ChartKind, Color, LegendPos, Series, Sheet, Sparkline, SparklineKind, Workbook};

use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

fn part(bytes: &[u8], name: &str) -> String {
    let mut pkg = Package::open(bytes).unwrap();
    String::from_utf8(pkg.read(name).unwrap().unwrap_or_else(|| panic!("no part {name}"))).unwrap()
}

fn data_sheet() -> Sheet {
    let mut s = Sheet::new("Data");
    for (r, (label, v)) in [("Start", 100.0), ("Up", 50.0), ("Down", -30.0), ("Subtotal", 120.0)].into_iter().enumerate() {
        s.set_value(CellRef::new(r as u32, 0), Value::text(label));
        s.set_value(CellRef::new(r as u32, 1), Value::number(v));
    }
    s
}

fn chart(kind: ChartKind) -> Chart {
    Chart {
        id: 1,
        kind,
        anchor: Anchor { width: 910.0, height: 661.0, mode: AnchorMode::Absolute, ..Anchor::default() },
        title: None,
        series: vec![Series { categories: Some("Data!$A$1:$A$4".into()), values: "Data!$B$1:$B$4".into(), ..Default::default() }],
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
        auto_title: false,
        title_overlay: false,
        chart_style: None,
    }
}

fn chart_sheet(name: &str, kind: ChartKind) -> Sheet {
    let mut s = Sheet::new(name);
    s.chart_sheet = true;
    s.charts.push(chart(kind));
    s
}

#[test]
fn a_chart_sheet_is_written_as_excel_writes_one_and_read_back() {
    let wb = Workbook { sheets: vec![Arc::new(chart_sheet("グラフ1", ChartKind::ColumnClustered)), Arc::new(data_sheet())], ..Default::default() };
    let bytes = write_xlsx(&wb).unwrap();

    let book = part(&bytes, "xl/workbook.xml");
    assert!(book.contains(r#"<sheet name="グラフ1" sheetId="1" r:id="rId1"/><sheet name="Data" sheetId="2" r:id="rId2"/>"#), "{book}");
    let rels = part(&bytes, "xl/_rels/workbook.xml.rels");
    assert!(
        rels.contains(
            r#"Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet" Target="chartsheets/sheet1.xml""#
        ),
        "{rels}"
    );
    assert!(rels.contains(r#"Target="worksheets/sheet1.xml""#), "worksheets are numbered on their own: {rels}");
    let types = part(&bytes, "[Content_Types].xml");
    assert!(
        types.contains(r#"<Override PartName="/xl/chartsheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml"/>"#),
        "{types}"
    );
    let cs = part(&bytes, "xl/chartsheets/sheet1.xml");
    assert!(
        cs.contains(r#"<sheetPr/><sheetViews><sheetView tabSelected="1" zoomToFit="1" workbookViewId="0"/></sheetViews><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><drawing r:id="rId1"/></chartsheet>"#),
        "{cs}"
    );
    assert!(part(&bytes, "xl/chartsheets/_rels/sheet1.xml.rels").contains(r#"Target="../drawings/drawing1.xml""#));
    let drawing = part(&bytes, "xl/drawings/drawing1.xml");
    assert!(drawing.contains(r#"<xdr:absoluteAnchor><xdr:pos x="0" y="0"/><xdr:ext cx="8667750" cy="6296025"/><xdr:graphicFrame"#), "{drawing}");
    assert!(drawing.contains("<c:chart r:id=\"rId1\"/>") && drawing.ends_with("</xdr:absoluteAnchor></xdr:wsDr>"), "{drawing}");
    let app = part(&bytes, "docProps/app.xml");
    assert!(
        app.contains("<HeadingPairs><vt:vector size=\"4\" baseType=\"variant\"><vt:variant><vt:lpstr>Worksheets</vt:lpstr></vt:variant><vt:variant><vt:i4>1</vt:i4></vt:variant><vt:variant><vt:lpstr>Charts</vt:lpstr></vt:variant><vt:variant><vt:i4>1</vt:i4></vt:variant></vt:vector></HeadingPairs><TitlesOfParts><vt:vector size=\"2\" baseType=\"lpstr\"><vt:lpstr>Data</vt:lpstr><vt:lpstr>グラフ1</vt:lpstr></vt:vector></TitlesOfParts>"),
        "{app}"
    );

    let (back, report) = read_xlsx(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let names: Vec<&str> = back.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["グラフ1", "Data"]);
    let cs = back.sheet(0).unwrap();
    assert!(cs.chart_sheet && !back.sheet(1).unwrap().chart_sheet);
    assert_eq!(cs.charts.len(), 1);
    assert_eq!(cs.charts[0].series[0].values, "Data!$B$1:$B$4");
    // A second save writes the same chart sheet again.
    let again = write_xlsx(&back).unwrap();
    assert_eq!(part(&again, "xl/chartsheets/sheet1.xml"), part(&bytes, "xl/chartsheets/sheet1.xml"));
}

#[test]
fn waterfall_totals_are_written_as_subtotals_and_read_back() {
    let mut ch = chart(ChartKind::Waterfall);
    ch.series[0].subtotals = vec![0, 3];
    let mut data = data_sheet();
    ch.anchor = Anchor { cell: CellRef::new(0, 3), width: 480.0, height: 288.0, ..Anchor::default() };
    data.charts.push(ch);
    let wb = Workbook { sheets: vec![Arc::new(data)], ..Default::default() };
    let bytes = write_xlsx(&wb).unwrap();
    let x = part(&bytes, "xl/charts/chartEx1.xml");
    assert!(
        x.contains(r#"<cx:dataId val="0"/><cx:layoutPr><cx:subtotals><cx:idx val="0"/><cx:idx val="3"/></cx:subtotals></cx:layoutPr></cx:series>"#),
        "{x}"
    );
    // Totals take the third accent, a decrease the second.
    assert!(x.contains(r#"<cx:dataPt idx="0"><cx:spPr><a:solidFill><a:schemeClr val="accent3"/></a:solidFill></cx:spPr></cx:dataPt>"#), "{x}");
    assert!(x.contains(r#"<cx:dataPt idx="2"><cx:spPr><a:solidFill><a:schemeClr val="accent2"/></a:solidFill></cx:spPr></cx:dataPt>"#), "{x}");
    let (back, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(back.sheet(0).unwrap().charts[0].series[0].subtotals, [0, 3]);
}

#[test]
fn sparklines_of_one_group_are_saved_in_one_sparkline_group() {
    let mut s = Sheet::new("Sheet1");
    let sp = |row: u32, group: u32| Sparkline {
        cell: CellRef::new(row, 13),
        source: format!("Sheet1!B{0}:M{0}", row + 1),
        kind: SparklineKind::Column,
        color: Color::Theme(4, 0),
        markers: false,
        high: false,
        low: false,
        group,
    };
    s.sparklines = (3..7).map(|r| sp(r, 1)).collect();
    s.sparklines.push(Sparkline { cell: CellRef::new(3, 14), source: "B4:M4".into(), ..sp(3, 0) });
    let wb = Workbook { sheets: vec![Arc::new(s)], ..Default::default() };
    let bytes = write_xlsx(&wb).unwrap();
    let x = part(&bytes, "xl/worksheets/sheet1.xml");
    assert_eq!(x.matches("<x14:sparklineGroup ").count(), 2, "{x}");
    assert!(
        x.contains("<x14:sparklines><x14:sparkline><xm:f>Sheet1!B4:M4</xm:f><xm:sqref>N4</xm:sqref></x14:sparkline><x14:sparkline><xm:f>Sheet1!B5:M5</xm:f><xm:sqref>N5</xm:sqref></x14:sparkline><x14:sparkline><xm:f>Sheet1!B6:M6</xm:f><xm:sqref>N6</xm:sqref></x14:sparkline><x14:sparkline><xm:f>Sheet1!B7:M7</xm:f><xm:sqref>N7</xm:sqref></x14:sparkline></x14:sparklines>"),
        "{x}"
    );
    let (back, _) = read_xlsx(&bytes).unwrap();
    let sh = back.sheet(0).unwrap();
    assert_eq!(sh.sparklines.len(), 5);
    let groups: Vec<u32> = sh.sparklines.iter().map(|s| s.group).collect();
    assert!(groups[..4].iter().all(|g| *g != 0 && *g == groups[0]) && groups[4] == 0, "{groups:?}");
    // Saved again, still two groups.
    let again = write_xlsx(&back).unwrap();
    assert_eq!(part(&again, "xl/worksheets/sheet1.xml").matches("<x14:sparklineGroup ").count(), 2);
}

#[test]
fn pie_data_label_parts_are_written_and_read_back() {
    let mut ch = chart(ChartKind::Pie);
    ch.anchor = Anchor { cell: CellRef::new(0, 3), width: 480.0, height: 288.0, ..Anchor::default() };
    ch.data_labels = true;
    ch.label_parts = gridcraft_model::LabelParts { value: false, category_name: true, percentage: true, ..Default::default() };
    let mut data = data_sheet();
    data.charts.push(ch);
    let bytes = write_xlsx(&Workbook { sheets: vec![Arc::new(data)], ..Default::default() }).unwrap();
    let x = part(&bytes, "xl/charts/chart1.xml");
    assert!(
        x.contains(r#"<c:dLbls><c:showLegendKey val="0"/><c:showVal val="0"/><c:showCatName val="1"/><c:showSerName val="0"/><c:showPercent val="1"/><c:showBubbleSize val="0"/><c:showLeaderLines val="1"/></c:dLbls>"#),
        "{x}"
    );
    let (back, _) = read_xlsx(&bytes).unwrap();
    let ch = &back.sheet(0).unwrap().charts[0];
    assert!(ch.data_labels);
    assert_eq!(ch.label_parts, gridcraft_model::LabelParts { value: false, category_name: true, percentage: true, ..Default::default() });
}

#[test]
fn high_and_low_points_are_saved_with_their_colours() {
    let mut s = Sheet::new("Sheet1");
    s.sparklines.push(Sparkline {
        cell: CellRef::new(0, 5),
        source: "Sheet1!A1:D1".into(),
        kind: SparklineKind::Line,
        color: Color::Theme(4, 0),
        markers: true,
        high: true,
        low: true,
        group: 0,
    });
    let wb = Workbook { sheets: vec![Arc::new(s)], ..Default::default() };
    let bytes = crate::write_xlsx(&wb).unwrap();
    let mut pkg = crate::package::Package::open(&bytes).unwrap();
    let xml = String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap();
    assert!(xml.contains(r#"<x14:sparklineGroup displayEmptyCellsAs="gap" markers="1" high="1" low="1"><x14:colorSeries theme="4"/><x14:colorMarkers theme="4"/><x14:colorHigh theme="5"/><x14:colorLow theme="5"/><x14:sparklines>"#), "{xml}");
    let (back, _) = crate::read_xlsx(&bytes).unwrap();
    let sp = &back.sheet(0).unwrap().sparklines[0];
    assert!(sp.high && sp.low && sp.markers);
}
