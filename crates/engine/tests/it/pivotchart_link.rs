//! #1447: a PivotChart is bound to its PivotTable behind the workbook's name.

use std::io::Read;

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params).unwrap()
}

fn source() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    run(
        &mut s,
        "range.setValues",
        json!({"range": "A1", "values": [["Region ", " Sales ", "Qty"], ["East", 10, 1], ["West", 20, 2], ["East", 5, 3]]}),
    );
    s
}

#[test]
fn pivot_chart_link_names_the_workbook_in_brackets() {
    let mut s = source();
    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "insert.pivotTable", json!({"rows": ["Region"], "values": ["Sales"]}));
    run(&mut s, "insert.pivotChart", json!({}));
    let bytes = gridcraft_engine::io::save_bytes(&s.doc().unwrap().wb, "Q1 Report.xlsx").unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut chart = String::new();
    zip.by_name("xl/charts/chart1.xml").unwrap().read_to_string(&mut chart).unwrap();
    assert!(chart.contains("<c:name>[Q1 Report.xlsx]Sheet2!PivotTable1</c:name>"), "{chart}");

    // Without a file name, a placeholder workbook is named; reading drops the brackets again.
    let saved = run(&mut s, "file.saveBytes", json!({"format": "xlsx"}));
    let mut reopened = Session::new();
    run(&mut reopened, "file.open", json!({"name": "q.xlsx", "base64": saved["base64"]}));
    let link = reopened.doc().unwrap().wb.sheets.iter().flat_map(|sh| sh.charts.iter()).find_map(|c| c.pivot.clone());
    assert_eq!(link.as_deref(), Some("Sheet2!PivotTable1"));
}

fn chart_xml(s: &Session) -> String {
    let bytes = gridcraft_engine::io::save_bytes(&s.doc().unwrap().wb, "Book1.xlsx").unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut chart = String::new();
    zip.by_name("xl/charts/chart1.xml").unwrap().read_to_string(&mut chart).unwrap();
    chart
}

fn reopen(s: &mut Session) -> Session {
    let saved = run(s, "file.saveBytes", json!({"format": "xlsx"}));
    let mut t = Session::new();
    run(&mut t, "file.open", json!({"name": "q.xlsx", "base64": saved["base64"]}));
    t
}

fn pivot_chart(s: &Session) -> gridcraft_engine::model::Chart {
    s.doc().unwrap().wb.sheets.iter().flat_map(|sh| sh.charts.iter()).find(|c| c.pivot.is_some()).cloned().unwrap()
}

#[test]
fn field_buttons_are_saved_as_pivot_options_and_read_back() {
    use gridcraft_engine::model::PivotFieldButtons;
    let mut s = source();
    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "insert.pivotTable", json!({"rows": ["Region"], "values": ["Sales"]}));
    run(&mut s, "insert.pivotChart", json!({}));
    assert_eq!(pivot_chart(&s).pivot_buttons, PivotFieldButtons::ALL, "a new PivotChart shows every field button");
    let xml = chart_xml(&s);
    assert!(
        xml.contains("<c:fmtId val=\"0\"/><c:extLst><c:ext uri=\"{781A3756-C4B2-4CAC-9D66-4F8BD8637D16}\" xmlns:c14=\"http://schemas.microsoft.com/office/drawing/2007/8/2/chart\"><c14:pivotOptions><c14:dropZoneFilter val=\"1\"/><c14:dropZoneCategories val=\"1\"/><c14:dropZoneData val=\"1\"/><c14:dropZoneSeries val=\"1\"/><c14:dropZonesVisible val=\"1\"/></c14:pivotOptions></c:ext></c:extLst></c:pivotSource>"),
        "{xml}"
    );
    assert_eq!(pivot_chart(&reopen(&mut s)).pivot_buttons, PivotFieldButtons::ALL);

    let r = run(&mut s, "chart.fieldButtons", json!({"axis": false}));
    assert_eq!(r["axis"], false, "{r}");
    let xml = chart_xml(&s);
    assert!(!xml.contains("dropZoneCategories") && xml.contains("<c14:dropZoneSeries val=\"1\"/>"), "{xml}");
    let back = pivot_chart(&reopen(&mut s)).pivot_buttons;
    assert_eq!(back, PivotFieldButtons { axis: false, ..PivotFieldButtons::ALL });

    // Hide All writes no pivot options; reading a PivotChart without them shows no buttons.
    run(&mut s, "chart.fieldButtons", json!({"all": false}));
    assert!(!chart_xml(&s).contains("pivotOptions"));
    assert_eq!(pivot_chart(&reopen(&mut s)).pivot_buttons, PivotFieldButtons::NONE);
    // The button's upper half toggles: none shown → all shown.
    run(&mut s, "chart.fieldButtons", json!({}));
    assert_eq!(pivot_chart(&s).pivot_buttons, PivotFieldButtons::ALL);
    // An ordinary chart has no field buttons.
    run(&mut s, "insert.chart", json!({"range": "Sheet1!A1:B4", "type": "line", "at": "J20"}));
    let id = s.doc().unwrap().wb.active().unwrap().charts.iter().find(|c| c.pivot.is_none()).map(|c| c.id);
    if let Some(id) = id {
        assert!(s.execute("chart.fieldButtons", json!({"chart": id, "all": false})).is_err());
    }
}

#[test]
fn create_pivot_chart_makes_an_empty_pivot_table_and_chart_that_fill_in_with_fields() {
    let mut s = source();
    let r = run(&mut s, "insert.pivotChart", json!({"source": "Sheet1!A1:C4", "destination": "new"}));
    assert_eq!(r["series"], 0, "{r}");
    let name = r["pivot"].as_str().unwrap().to_string();
    let ch = pivot_chart(&s);
    assert!(ch.series.is_empty());
    assert_eq!(ch.legend, gridcraft_engine::model::LegendPos::Right);
    run(&mut s, "pivot.addField", json!({"pivot": name, "field": "Region", "area": "rows"}));
    run(&mut s, "pivot.addField", json!({"pivot": name, "field": "Sales", "area": "values"}));
    let ch = pivot_chart(&s);
    assert_eq!(ch.series.len(), 1, "the chart follows the fields added");
    // One Undo takes the PivotTable and its chart away together.
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc().unwrap().wb.sheets.iter().all(|sh| sh.pivots.is_empty() && sh.charts.is_empty()));
    // On an existing sheet.
    run(
        &mut s,
        "insert.pivotChart",
        json!({"source": "Sheet1!A1:C4", "destination": "Sheet1!H3", "rows": ["Region"], "values": ["Sales", "Qty"], "type": "combo", "combo": [{"type": "column"}, {"type": "line", "secondary": true}]}),
    );
    let ch = pivot_chart(&s);
    assert_eq!(ch.series.len(), 2);
    assert!(ch.series[1].secondary, "a combo PivotChart keeps its secondary axis");
    run(&mut s, "pivot.refresh", json!({}));
    assert!(pivot_chart(&s).series[1].secondary, "through a refresh");
    assert!(s.execute("insert.pivotChart", json!({"type": "scatter"})).is_err(), "Excel has no X Y PivotChart");
}

#[test]
fn field_buttons_are_drawn_where_excel_draws_them_and_can_be_clicked() {
    use gridcraft_chart::{FieldButtonKind, field_button_at, pivot_field_buttons};
    let mut s = Session::new();
    s.new_workbook();
    run(
        &mut s,
        "range.setValues",
        json!({"range": "A1", "values": [["Item", "Shop", "Month", "Sales"], ["Bun", "East", "Apr", 10], ["Tart", "West", "Apr", 20], ["Bun", "West", "May", 5]]}),
    );
    run(
        &mut s,
        "insert.pivotTable",
        json!({"source": "Sheet1!A1:D4", "rows": ["Item"], "columns": ["Shop"], "values": ["Sales"], "filters": ["Month"]}),
    );
    run(&mut s, "insert.pivotChart", json!({}));
    run(&mut s, "pivot.filter", json!({"field": "Item", "selected": ["Bun"]}));
    let wb = s.doc().unwrap().wb.clone();
    let chart = pivot_chart(&s);
    let b = pivot_field_buttons(&wb, &chart).unwrap();
    let names = |k: FieldButtonKind| b.of(k).iter().map(|f| (f.name.as_str(), f.filtered)).collect::<Vec<_>>();
    assert_eq!(names(FieldButtonKind::ReportFilter), [("Month", false)]);
    assert_eq!(names(FieldButtonKind::Axis), [("Item", true)], "a filtered field's button shows a funnel");
    assert_eq!(names(FieldButtonKind::Legend), [("Shop", false)]);
    assert_eq!(b.of(FieldButtonKind::Value).len(), 1);
    let si = wb.sheets.iter().position(|sh| sh.charts.iter().any(|c| c.id == chart.id)).unwrap();
    let mut data = gridcraft_chart::resolve(&wb, si, &chart);
    assert_eq!(data.pivot_buttons, None, "export and print draw no field buttons");
    data.pivot_buttons = Some(b);
    let (w, h) = (chart.anchor.width, chart.anchor.height);
    let m = gridcraft_chart::ApproxMeasure;
    // Report filters at the top left, axis fields at the bottom left, legend fields at the right.
    assert_eq!(field_button_at(&chart, &data, w, h, &m, 16.0, 16.0), Some((FieldButtonKind::ReportFilter, 0)));
    assert_eq!(field_button_at(&chart, &data, w, h, &m, 16.0, h - 16.0), Some((FieldButtonKind::Axis, 0)));
    let legend = (0..200).map(|i| i as f32).find_map(|y| field_button_at(&chart, &data, w, h, &m, w - 16.0, y));
    assert_eq!(legend, Some((FieldButtonKind::Legend, 0)));
    // Hidden kinds aren't drawn.
    run(&mut s, "chart.fieldButtons", json!({"reportFilter": false, "axis": false}));
    let chart = pivot_chart(&s);
    data.pivot_buttons = pivot_field_buttons(&s.doc().unwrap().wb, &chart);
    assert_ne!(field_button_at(&chart, &data, w, h, &m, 16.0, 16.0).map(|x| x.0), Some(FieldButtonKind::ReportFilter));
    assert_ne!(field_button_at(&chart, &data, w, h, &m, 16.0, h - 16.0).map(|x| x.0), Some(FieldButtonKind::Axis));
    run(&mut s, "chart.fieldButtons", json!({"all": false}));
    assert_eq!(pivot_field_buttons(&s.doc().unwrap().wb, &pivot_chart(&s)), None);
}
