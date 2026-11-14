//! The chart and sparkline interface as Excel's: the Chart Styles gallery, Add Chart Element's
//! Axis Titles and Chart Title submenus, titles selected and edited on the chart, the chart's
//! Format tab and right-click menu, and the Sparkline tab.

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_engine::model::{Chart, ObjKind, ObjRef, Sparkline};
use gridcraft_ui_egui::{chartview, grid::Geo};
use serde_json::json;

type Harness = crate::common::Harness;

fn chart_session() -> (Session, u32) {
    let mut s = crate::support::blank_session();
    s.execute("range.setValues", json!({"range": "A1", "values": [["Month", "Visits", "Last"], ["Apr", 10, 7], ["May", 20, 14]]})).unwrap();
    let id = s.execute("insert.chart", json!({"range": "A1:C3", "type": "column", "at": "E3", "width": 420, "height": 260})).unwrap()["chart"]
        .as_u64()
        .unwrap() as u32;
    (s, id)
}

fn selected_chart_harness() -> (Harness, u32) {
    let (s, id) = chart_session();
    let mut h = crate::support::harness(s, 4);
    h.state_mut().selected_object = Some(ObjRef::new(ObjKind::Chart, id));
    h.state_mut().ui.ribbon_tab = "Chart Design".into();
    h.run_steps(3);
    (h, id)
}

fn chart(h: &Harness) -> Chart {
    h.state().session.active().unwrap().wb.active().unwrap().charts[0].clone()
}

fn click(h: &mut Harness, p: Pos2) {
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerMoved(p));
        h.input_mut().events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(2);
}

/// The centre of `element` of chart `id` on screen.
fn element_centre(h: &Harness, id: u32, element: gridcraft_chart::ChartElement) -> Pos2 {
    let app = h.state();
    let d = app.session.active().unwrap();
    let sh = d.wb.active().unwrap();
    let geo = Geo::new(sh, app.grid.rect.unwrap(), app.view().scroll);
    let painter = egui::Painter::new(h.ctx.clone(), egui::LayerId::background(), egui::Rect::EVERYTHING);
    chartview::chart_element_rect(&painter, &geo, &d.wb, d.wb.active_sheet(), sh, id, element).unwrap().center()
}

#[test]
fn a_chart_style_from_the_gallery_is_saved_as_the_charts_style() {
    let (mut h, _) = selected_chart_harness();
    assert_eq!(chart(&h).gallery_style(), Some(1));
    h.get_by_label("Style 3").click();
    h.run_steps(3);
    assert_eq!((chart(&h).chart_style, chart(&h).gallery_style()), (Some(203), Some(3)));
    // More Chart Styles opens the whole gallery.
    h.get_by_label("More Chart Styles").click();
    h.run_steps(3);
    h.get_by_label("Style 12").click();
    h.run_steps(3);
    assert_eq!(chart(&h).gallery_style(), Some(12));
}

#[test]
fn add_chart_element_adds_one_axis_title_and_places_the_chart_title() {
    let (mut h, _) = selected_chart_harness();
    h.get_by_label("Add Chart Element").click();
    h.run_steps(3);
    h.get_by_label("Axis Titles").click();
    h.run_steps(3);
    h.get_by_label("Primary Vertical").click();
    h.run_steps(3);
    let c = chart(&h);
    assert_eq!((c.y_title.as_deref(), c.x_title.as_deref()), (Some("Axis Title"), None), "the vertical axis only");
    h.get_by_label("Add Chart Element").click();
    h.run_steps(3);
    h.get_all_by_label("Chart Title").next().unwrap().click();
    h.run_steps(3);
    h.get_by_label("Centered Overlay").click();
    h.run_steps(3);
    assert!(chart(&h).title_overlay);
    h.get_by_label("Add Chart Element").click();
    h.run_steps(3);
    h.get_all_by_label("Chart Title").next().unwrap().click();
    h.run_steps(3);
    h.get_by_label("None").click();
    h.run_steps(3);
    assert_eq!(chart(&h).title.as_deref(), Some(""), "Chart Title ▸ None deletes it");
}

#[test]
fn data_labels_lists_excels_positions_for_the_chart_type() {
    let (mut h, _) = selected_chart_harness();
    h.get_by_label("Add Chart Element").click();
    h.run_steps(3);
    h.get_by_label("Data Labels").click();
    h.run_steps(3);
    for label in ["None", "Center", "Inside End", "Inside Base", "Outside End", "Data Callout"] {
        assert!(h.query_by_label(label).is_some(), "{label} is offered on a clustered column");
    }
    assert!(h.query_by_label("Show").is_none() && h.query_by_label("Best Fit").is_none());
    h.get_by_label("Outside End").click();
    h.run_steps(3);
    let c = chart(&h);
    assert!(c.data_labels);
    assert_eq!(c.label_position(), Some(gridcraft_engine::model::LabelPos::OutsideEnd));
    let ja = gridcraft_ui_egui::i18n::Language::Ja;
    assert_eq!(
        [
            "Data Labels|Center",
            "Inside End",
            "Inside Base",
            "Outside End",
            "Data Callout",
            "Pie Labels|Inside End",
            "Pie Labels|Outside End",
            "Best Fit"
        ]
        .map(|l| ja.tr(l)),
        ["中央", "内側終点", "内側軸寄り", "外側終点", "データの吹き出し", "内部外側", "外部", "自動調整"]
    );
}

#[test]
fn clicking_the_title_selects_it_and_a_second_click_edits_it_in_place() {
    let (s, id) = chart_session();
    let mut h = crate::support::harness(s, 4);
    let title = element_centre(&h, id, gridcraft_chart::ChartElement::Title);
    click(&mut h, title);
    assert_eq!(h.state().selected_chart(), Some(id));
    assert_eq!(h.state().selected_chart_element(), Some((id, gridcraft_chart::ChartElement::Title)));
    h.run_steps(40);
    click(&mut h, title);
    assert!(h.state().editing_chart_text(), "the second click edits the title");
    h.input_mut().events.push(Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.step();
    h.input_mut().events.push(Event::Text("Monthly visits".into()));
    h.step();
    h.input_mut().events.push(Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    assert!(!h.state().editing_chart_text());
    assert_eq!(chart(&h).title.as_deref(), Some("Monthly visits"));
}

#[test]
fn typing_over_a_selected_axis_title_replaces_it_and_delete_removes_it() {
    let (mut s, id) = chart_session();
    s.execute("chart.set", json!({"yTitle": "Axis Title"})).unwrap();
    let mut h = crate::support::harness(s, 4);
    let at = element_centre(&h, id, gridcraft_chart::ChartElement::VerticalAxisTitle);
    click(&mut h, at);
    assert_eq!(h.state().selected_chart_element(), Some((id, gridcraft_chart::ChartElement::VerticalAxisTitle)));
    h.input_mut().events.push(Event::Text("Visitors".into()));
    h.run_steps(2);
    h.input_mut().events.push(Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    assert_eq!(chart(&h).y_title.as_deref(), Some("Visitors"));
    assert_eq!(h.state().selected_chart_element(), Some((id, gridcraft_chart::ChartElement::VerticalAxisTitle)), "the title stays selected");
    h.input_mut().events.push(Event::Key { key: egui::Key::Delete, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    assert_eq!(chart(&h).y_title, None, "Delete removes the title");
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().charts.len(), 1, "not the chart");
}

#[test]
fn the_charts_format_tab_has_alt_text_and_its_menu_edit_alt_text() {
    let (mut h, id) = selected_chart_harness();
    h.state_mut().ui.ribbon_tab = "Format".into();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Format", "a chart has a Format tab");
    h.get_by_label("Alt Text").click();
    h.run_steps(3);
    assert_eq!(h.state().grid.pane.as_deref(), Some("altText"), "Format ▸ Alt Text opens the Alt Text pane");
    h.state_mut().grid.pane = None;
    h.state_mut().ui.ribbon_tab = "Chart Design".into();
    h.run_steps(2);
    assert!(h.query_by_label("Alt Text").is_none(), "Alt Text is on the Format tab, not Chart Design");
    // The right-click menu.
    h.state_mut().grid.object_menu = Some((egui::pos2(600.0, 400.0), "chart", id));
    h.run_steps(2);
    for label in ["Change Chart Type…", "Select Data…", "Edit Alt Text…", "Format Chart Area…"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    h.get_by_label("Edit Alt Text…").click();
    h.run_steps(3);
    assert_eq!(h.state().grid.pane.as_deref(), Some("altText"));
}

#[test]
fn the_alt_text_pane_has_a_description_and_mark_as_decorative_but_no_title() {
    let (mut s, id) = chart_session();
    // A title read from an older file stays as it is.
    s.execute("object.altText", json!({"kind": "chart", "id": id, "title": "Old title"})).unwrap();
    let mut h = crate::support::harness(s, 4);
    h.state_mut().selected_object = Some(ObjRef::new(ObjKind::Chart, id));
    h.state_mut().open_dialog("altText", json!({"kind": "chart", "id": id}));
    h.run_steps(3);
    assert!(h.state().dialog.is_none() && h.state().grid.pane.as_deref() == Some("altText"));
    assert!(h.query_by_label("Title:").is_none(), "Microsoft 365's pane has no title field");
    assert!(h.query_by_label("Mark as decorative").is_some());
    h.get_by_label("Description:").click();
    h.run_steps(2);
    h.input_mut().events.push(Event::Text("Visits rose in May.".into()));
    h.step();
    // Clicking the checkbox moves focus away, which commits the description.
    h.get_by_label("Mark as decorative").click();
    h.run_steps(3);
    let alt = chart(&h).alt_text;
    assert_eq!((alt.title.as_str(), alt.description.as_str(), alt.decorative), ("Old title", "Visits rose in May.", true));
}

#[test]
fn the_sparkline_tab_sets_the_groups_points_and_type() {
    let mut s = crate::support::blank_session();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1, 5, 2, 8], [3, 1, 4, 2]]})).unwrap();
    s.execute("insert.sparkline", json!({"range": "A1:D2", "location": "F1:F2"})).unwrap();
    s.execute("selection.set", json!({"range": "F1"})).unwrap();
    let mut h = crate::support::harness(s, 4);
    let sparks = |h: &Harness| -> Vec<Sparkline> { h.state().session.active().unwrap().wb.active().unwrap().sparklines.clone() };
    h.state_mut().ui.ribbon_tab = "Sparkline".into();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Sparkline");
    for label in ["High Point", "Low Point", "Markers"] {
        h.get_by_label(label).click();
        h.run_steps(2);
    }
    assert!(sparks(&h).iter().all(|sp| sp.high && sp.low && sp.markers), "the whole group: {:?}", sparks(&h));
    h.get_by_label("Column").click();
    h.run_steps(2);
    assert!(sparks(&h).iter().all(|sp| sp.kind == gridcraft_engine::model::SparklineKind::Column));
    // Leaving the sparkline cell takes the tab away.
    h.state_mut().session.execute("selection.set", json!({"range": "H5"})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().ui.ribbon_tab, "Home");
}

#[test]
fn japanese_names_the_tabs_as_excel_does() {
    let ja = gridcraft_ui_egui::i18n::Language::Ja;
    assert_eq!(ja.tr("Review"), "校閲");
    assert_eq!(ja.tr("Chart Design"), "グラフのデザイン");
    assert_eq!(ja.tr("Format"), "書式");
    assert_eq!(ja.tr("Sparkline"), "スパークライン");
    assert_eq!(ja.tr("Axis Titles"), "軸ラベル");
    assert_eq!(ja.tr("Primary Vertical"), "第 1 縦軸");
    assert_eq!(ja.tr("Centered Overlay"), "中央揃えで重ねて配置");
    assert_eq!(ja.tr("Edit Alt Text…"), "代替テキストを編集…");
    // Create Sparklines, Fill Series and the Home tab's number format box as Excel names them.
    assert_eq!(ja.tr("Location Range:"), "場所の範囲:");
    assert_eq!(ja.tr("Weekday"), "週日");
    assert_eq!(
        ["Series|Series in", "Series|Linear", "Series|Growth", "Series|Date unit"].map(|l| ja.tr(l)),
        ["範囲", "加算", "乗算", "増加単位"]
    );
    assert_eq!((ja.tr("Number Format"), ja.tr("Number")), ("数値の書式", "数値"));
    assert_eq!(ja.tr("Format Cells|Number"), "表示形式", "Format Cells' tab keeps its name");
}
