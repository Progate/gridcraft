//! Combo charts with a series on the secondary axis, and Alt Text on charts and pictures: the
//! parts Excel writes for them, and a save and reopen.

use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{AltText, Anchor, Chart, ChartKind, Image, LegendPos, Series, Sheet, Workbook};

use crate::chart::read_chart;
use crate::package::Package;
use crate::xml::parse;
use crate::{read_xlsx, write_xlsx};

fn series(col: &str, kind: ChartKind, secondary: bool) -> Series {
    Series {
        name: Some(format!("Data!${col}$1")),
        categories: Some("Data!$A$2:$A$5".into()),
        values: format!("Data!${col}$2:${col}$5"),
        kind: Some(kind),
        secondary,
        ..Default::default()
    }
}

fn chart(id: u32, kind: ChartKind, series: Vec<Series>) -> Chart {
    Chart {
        id,
        kind,
        anchor: Anchor { cell: CellRef::new(0, 4), width: 480.0, height: 288.0, ..Anchor::default() },
        title: Some("Sales".into()),
        series,
        legend: LegendPos::Bottom,
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
        alt_text: AltText::default(),
        label_parts: Default::default(),
        auto_title: false,
        title_overlay: false,
        chart_style: None,
        y2_title: None,
        bins: None,
    }
}

fn book(sheet: Sheet) -> Vec<u8> {
    let wb = Workbook { sheets: vec![Arc::new(sheet)], ..Default::default() };
    write_xlsx(&wb).unwrap()
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut pkg = Package::open(bytes).unwrap();
    String::from_utf8(pkg.read(name).unwrap().unwrap()).unwrap()
}

#[test]
fn a_line_on_the_secondary_axis_is_written_as_excel_writes_it() {
    let mut s = Sheet::new("Data");
    s.charts.push(chart(1, ChartKind::Combo, vec![series("B", ChartKind::ColumnClustered, false), series("C", ChartKind::Line, true)]));
    let bytes = book(s);
    let xml = part(&bytes, "xl/charts/chart1.xml");
    let bar = xml.find("<c:barChart>").unwrap();
    let line = xml.find("<c:lineChart>").unwrap();
    assert!(bar < line, "the column group comes first");
    assert!(xml[bar..line].contains("<c:axId val=\"1\"/><c:axId val=\"2\"/></c:barChart>"));
    assert!(xml[line..].contains("<c:axId val=\"3\"/><c:axId val=\"4\"/></c:lineChart>"));
    let secondary_val = xml.find("<c:valAx><c:axId val=\"4\"/>").unwrap();
    let secondary_cat = xml.find("<c:catAx><c:axId val=\"3\"/>").unwrap();
    assert!(secondary_val < secondary_cat, "the secondary value axis comes before its hidden category axis");
    assert!(xml[secondary_val..secondary_cat].contains("<c:axPos val=\"r\"/>"));
    assert!(xml[secondary_val..secondary_cat].contains("<c:crossAx val=\"3\"/><c:crosses val=\"max\"/>"));
    assert!(xml[secondary_cat..].contains("<c:delete val=\"1\"/>"));

    let (back, _) = read_xlsx(&bytes).unwrap();
    let ch = &back.sheets[0].charts[0];
    assert_eq!(ch.kind, ChartKind::Combo);
    let got: Vec<(Option<ChartKind>, bool)> = ch.series.iter().map(|x| (x.kind, x.secondary)).collect();
    assert_eq!(got, [(Some(ChartKind::ColumnClustered), false), (Some(ChartKind::Line), true)]);
}

#[test]
fn combo_groups_follow_excels_order_per_axis() {
    let mut s = Sheet::new("Data");
    s.charts.push(chart(
        1,
        ChartKind::Combo,
        vec![series("B", ChartKind::Line, false), series("C", ChartKind::ColumnClustered, false), series("D", ChartKind::AreaStacked, true)],
    ));
    let bytes = book(s);
    let xml = part(&bytes, "xl/charts/chart1.xml");
    let order: Vec<usize> = ["<c:barChart>", "<c:lineChart>", "<c:areaChart>"].iter().map(|t| xml.find(t).unwrap()).collect();
    assert!(order[0] < order[1] && order[1] < order[2], "primary column, primary line, then the secondary area");
    assert!(xml.contains("<c:areaChart><c:grouping val=\"stacked\"/>"));
    let (back, _) = read_xlsx(&bytes).unwrap();
    let got: Vec<(Option<ChartKind>, bool)> = back.sheets[0].charts[0].series.iter().map(|x| (x.kind, x.secondary)).collect();
    // Series come back in the order of their groups.
    assert_eq!(got, [(Some(ChartKind::ColumnClustered), false), (Some(ChartKind::Line), false), (Some(ChartKind::AreaStacked), true)]);
}

#[test]
fn every_series_on_the_secondary_axis_is_written_on_the_primary_one() {
    let mut s = Sheet::new("Data");
    s.charts.push(chart(1, ChartKind::Combo, vec![series("B", ChartKind::ColumnClustered, true), series("C", ChartKind::Line, true)]));
    let xml = part(&book(s), "xl/charts/chart1.xml");
    assert!(!xml.contains("<c:crosses val=\"max\"/>"));
    assert!(!xml.contains("<c:axId val=\"3\"/>"));
}

#[test]
fn the_axis_crossing_at_the_maximum_marks_the_secondary_group_wherever_it_sits() {
    // An area group on the secondary axis written before the primary columns.
    let ser =
        |i: usize| format!("<c:ser><c:idx val=\"{i}\"/><c:order val=\"{i}\"/><c:val><c:numRef><c:f>Data!$B$2:$B$5</c:f></c:numRef></c:val></c:ser>");
    let xml = format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea>\
         <c:areaChart><c:grouping val=\"standard\"/>{}<c:axId val=\"30\"/><c:axId val=\"40\"/></c:areaChart>\
         <c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/>{}<c:axId val=\"10\"/><c:axId val=\"20\"/></c:barChart>\
         <c:catAx><c:axId val=\"10\"/><c:crossAx val=\"20\"/></c:catAx><c:valAx><c:axId val=\"20\"/><c:crossAx val=\"10\"/><c:crosses val=\"autoZero\"/></c:valAx>\
         <c:valAx><c:axId val=\"40\"/><c:crossAx val=\"30\"/><c:crosses val=\"max\"/></c:valAx><c:catAx><c:axId val=\"30\"/><c:delete val=\"1\"/></c:catAx>\
         </c:plotArea></c:chart></c:chartSpace>",
        ser(0),
        ser(1)
    );
    let ch = read_chart(&parse(xml.as_bytes()).unwrap()).unwrap();
    let got: Vec<bool> = ch.series.iter().map(|x| x.secondary).collect();
    assert_eq!(got, [true, false]);
}

#[test]
fn alt_text_of_charts_and_pictures_is_written_on_cnvpr_and_read_back() {
    let mut s = Sheet::new("Data");
    let mut ch = chart(1, ChartKind::ColumnClustered, vec![series("B", ChartKind::ColumnClustered, false)]);
    ch.alt_text = AltText { title: "Sales chart".into(), description: "Sales by month & \"rate\"".into(), decorative: false };
    s.charts.push(ch);
    let mut deco = chart(2, ChartKind::Line, vec![series("C", ChartKind::Line, false)]);
    deco.alt_text.decorative = true;
    s.charts.push(deco);
    s.images.push(Image {
        id: 3,
        anchor: Anchor { cell: CellRef::new(20, 0), width: 40.0, height: 40.0, ..Anchor::default() },
        data: b"\x89PNG\r\n\x1a\n0000IHDRfake".to_vec().into(),
        mime: "image/png".into(),
        alt: "Company logo".into(),
        alt_title: "Logo".into(),
        decorative: false,
    });
    let bytes = book(s);
    let xml = part(&bytes, "xl/drawings/drawing1.xml");
    assert!(xml.contains("name=\"Picture 1\" descr=\"Company logo\" title=\"Logo\"/>"), "{xml}");
    assert!(xml.contains("name=\"Chart 2\" descr=\"Sales by month &amp; &quot;rate&quot;\" title=\"Sales chart\"/>"), "{xml}");
    assert!(xml.contains(
        "name=\"Chart 3\"><a:extLst><a:ext uri=\"{C183D7F6-B498-43B3-948B-1728B52AA6E4}\"><adec:decorative xmlns:adec=\"http://schemas.microsoft.com/office/drawing/2017/decorative\" val=\"1\"/></a:ext></a:extLst></xdr:cNvPr>"
    ));

    let (back, _) = read_xlsx(&bytes).unwrap();
    let sh = &back.sheets[0];
    assert_eq!(sh.charts[0].alt_text, AltText { title: "Sales chart".into(), description: "Sales by month & \"rate\"".into(), decorative: false });
    assert_eq!(sh.charts[1].alt_text, AltText { decorative: true, ..AltText::default() });
    assert_eq!((sh.images[0].alt.as_str(), sh.images[0].alt_title.as_str(), sh.images[0].decorative), ("Company logo", "Logo", false));
}

#[test]
fn a_decorative_box_and_whisker_chart_keeps_its_mark() {
    let mut s = Sheet::new("Data");
    let mut ch = chart(1, ChartKind::BoxWhisker, vec![Series { values: "Data!$B$2:$B$9".into(), ..Default::default() }]);
    ch.alt_text = AltText { title: String::new(), description: "Spread of sales".into(), decorative: true };
    s.charts.push(ch);
    let bytes = book(s);
    let (back, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(back.sheets[0].charts[0].kind, ChartKind::BoxWhisker);
    assert_eq!(back.sheets[0].charts[0].alt_text, AltText { title: String::new(), description: "Spread of sales".into(), decorative: true });
}
