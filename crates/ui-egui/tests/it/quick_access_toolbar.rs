//! The Quick Access Toolbar as Excel's: a ribbon button's right-click menu adds its command, the
//! added button runs it, and Customize Quick Access Toolbar moves the toolbar below the ribbon.

use egui::{Event, Modifiers, PointerButton};
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn right_click(h: &mut crate::common::Harness, label: &str) {
    let pos = h.get_all_by_label(label).next().unwrap().rect().center();
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerMoved(pos));
        h.input_mut().events.push(Event::PointerButton { pos, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(2);
}

fn bold(h: &crate::common::Harness) -> bool {
    let d = h.state().session.active().unwrap();
    let sh = d.wb.active().unwrap();
    d.wb.styles.get(sh.style_id(d.selection.active)).font.bold
}

#[test]
fn a_ribbon_command_added_to_the_toolbar_runs_from_it_and_the_toolbar_moves_below_the_ribbon() {
    let mut s = crate::support::blank_session();
    s.execute("cell.set", json!({"cell": "A1", "input": "x"})).unwrap();
    let mut h = crate::support::harness(s, 4);
    let ribbon_bolds = h.get_all_by_label("Bold").count();
    right_click(&mut h, "Bold");
    h.get_by_label("Add to Quick Access Toolbar").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.qat, ["home.bold"]);
    // The ribbon's Bold and the toolbar's.
    assert_eq!(h.get_all_by_label("Bold").count(), ribbon_bolds + 1);
    // The toolbar's button is the one in the title bar, above the ribbon.
    let top = h.get_all_by_label("Bold").map(|n| n.rect().min.y).fold(f32::MAX, f32::min);
    assert!(top < 40.0, "a Bold button in the title bar: {top}");
    h.get_all_by_label("Bold").find(|n| n.rect().min.y == top).unwrap().click();
    h.run_steps(3);
    assert!(bold(&h), "the toolbar button runs Bold");

    h.get_by_label("Customize Quick Access Toolbar").click();
    h.run_steps(3);
    h.get_by_label("Show Below the Ribbon").click();
    h.run_steps(3);
    assert!(h.state().ui.qat_below);
    assert_eq!(h.get_all_by_label("Bold").count(), ribbon_bolds + 1, "the toolbar shows under the ribbon");
    assert!(h.get_all_by_label("Bold").all(|n| n.rect().min.y > 40.0), "nothing left in the title bar");
    h.get_by_label("Customize Quick Access Toolbar").click();
    h.run_steps(3);
    assert!(h.query_by_label("Show Above the Ribbon").is_some());

    // Removed from the ribbon button's menu.
    h.state_mut().ui.qat_below = false;
    h.run_steps(2);
    right_click(&mut h, "Bold");
    h.get_by_label("Remove from Quick Access Toolbar").click();
    h.run_steps(3);
    assert!(h.state().ui.qat.is_empty());

    let ja = gridcraft_ui_egui::i18n::Language::Ja;
    assert_eq!(ja.tr("Add to Quick Access Toolbar"), "クイック アクセス ツール バーに追加");
    assert_eq!(ja.tr("Show Below the Ribbon"), "リボンの下に表示");
}

#[test]
fn the_toolbar_is_a_preference_that_ignores_unknown_commands() {
    let state: gridcraft_ui_egui::UiState = serde_json::from_value(json!({"qat": ["home.bold"], "qatBelow": true})).unwrap();
    assert_eq!((state.qat.as_slice(), state.qat_below), (["home.bold".to_string()].as_slice(), true));
    let mut h = crate::support::harness(crate::support::blank_session(), 4);
    gridcraft_ui_egui::ribbon::qat::set_command(h.state_mut(), "no.such.command", true);
    assert!(h.state().ui.qat.is_empty());
}
