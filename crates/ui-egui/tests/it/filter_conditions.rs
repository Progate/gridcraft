//! The Text, Number and Date Filters submenus of the AutoFilter dropdown and the dialogs they open.

use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A1:B5 holds names and amounts under an AutoFilter, with the menu of `cell` opened by Alt+Down.
fn menu_open(cell: &str) -> Harness {
    let mut session = Session::new();
    session.new_workbook();
    session
        .execute(
            "range.setValues",
            json!({"range": "A1", "values": [["Name", "Amount"], ["Apple", 10], ["Banana", 40], ["Cherry", 20], ["Apricot", 30]]}),
        )
        .unwrap();
    session.execute("data.filter", json!({"range": "A1:B5"})).unwrap();
    session.execute("selection.set", json!({"cell": cell})).unwrap();
    let mut h = crate::common::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        crate::common::app(session),
    );
    h.run_steps(4);
    h.input_mut().events.push(Event::Key { key: Key::ArrowDown, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::ALT });
    h.input_mut().events.push(Event::Key { key: Key::ArrowDown, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::ALT });
    h.run_steps(2);
    assert!(h.state().grid.filter_menu.is_some(), "Alt+Down opens the filter menu");
    h
}

fn click(h: &mut Harness, label: &str) {
    // The OK button pads its label with spaces.
    let at = if label == "OK" { h.get_by_label_contains(label).rect().center() } else { h.get_by_label(label).rect().center() };
    h.input_mut().events.push(Event::PointerMoved(at));
    h.run_steps(2);
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(3);
}

fn hidden(h: &Harness) -> Vec<bool> {
    let sh = h.state().session.active().unwrap().wb.active().unwrap();
    (1..=4).map(|r| sh.is_row_hidden(r)).collect()
}

fn dialog(h: &Harness) -> Option<String> {
    h.state().dialog.as_ref().map(|d| d.name.clone())
}

#[test]
fn number_filters_above_average_filters_at_once() {
    let mut h = menu_open("B1");
    click(&mut h, "Number Filters");
    assert!(h.state().grid.filter_menu.is_some(), "opening the submenu keeps the dropdown open");
    click(&mut h, "Above Average");
    assert_eq!(hidden(&h), [true, false, true, false], "40 and 30 are above the average of 25");
    assert!(h.state().grid.filter_menu.is_none(), "choosing an item closes the dropdown");
}

#[test]
fn number_filters_top_10_opens_its_dialog() {
    let mut h = menu_open("B1");
    click(&mut h, "Number Filters");
    click(&mut h, "Top 10…");
    assert_eq!(dialog(&h).as_deref(), Some("top10AutoFilter"));
    click(&mut h, "OK");
    assert_eq!(dialog(&h), None);
    assert_eq!(hidden(&h), [false, false, false, false], "Top 10 of four items keeps them all");
}

#[test]
fn text_filters_begins_with_opens_custom_autofilter_with_the_operator() {
    let mut h = menu_open("A1");
    click(&mut h, "Text Filters");
    click(&mut h, "Begins With…");
    assert_eq!(dialog(&h).as_deref(), Some("customAutoFilter"));
    let d = h.state_mut().dialog.as_mut().unwrap();
    assert_eq!(d.values["op"], json!("beginsWith"));
    d.values.insert("value".into(), json!("ap"));
    h.run_steps(2);
    click(&mut h, "OK");
    assert_eq!(dialog(&h), None);
    assert_eq!(hidden(&h), [false, true, true, false], "Apple and Apricot begin with ap");
}

#[test]
fn the_japanese_menu_uses_excels_wording() {
    // Without the pinned craft-fonts input there is no Japanese face to draw these labels with
    // (the gap `interaction.rs` documents); the catalog itself is checked by the unit tests.
    if !gridcraft_ui_egui::theme::craft_fonts_loaded() {
        return;
    }
    let mut h = menu_open("B1");
    h.state_mut().ui.language = gridcraft_ui_egui::i18n::Language::Ja;
    h.run_steps(2);
    click(&mut h, "数値フィルター");
    for label in ["指定の値以上...", "指定の範囲内...", "トップテン...", "平均より上", "ユーザー設定フィルター..."] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    click(&mut h, "指定の範囲内...");
    assert_eq!(dialog(&h).as_deref(), Some("customAutoFilter"));
    for label in ["抽出条件の指定:", "AND", "OR", "以上", "以下"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
}
