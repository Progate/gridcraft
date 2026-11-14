//! Review's protection dialogs as Excel's: Protect Sheet's permissions, Protect Workbook's
//! Structure box, Allow Users to Edit Ranges with New… and its New Range dialog, Confirm
//! Password, and the Unlock Range prompt a protected range's cell opens when it is typed into.

use egui::Event;
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable as _;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

fn type_into_password(h: &mut Harness, text: &str) {
    h.run_steps(3);
    h.get_by_role(Role::PasswordInput).click();
    h.run_steps(2);
    h.input_mut().events.push(Event::Text(text.into()));
    h.run_steps(2);
}

fn ok(h: &mut Harness) {
    h.get_by_label("   OK   ").click();
    h.run_steps(3);
}

fn dialog(h: &Harness) -> Option<String> {
    h.state().dialog.as_ref().map(|d| d.name.clone())
}

#[test]
fn protect_sheet_lists_excels_permissions_and_saves_the_new_ones() {
    let mut h = crate::support::blank_harness(4);
    h.state_mut().open_dialog("protectSheet", json!({}));
    h.run_steps(3);
    for label in [
        "Protect worksheet and contents of locked cells",
        "Select locked cells",
        "Insert hyperlinks",
        "Use AutoFilter",
        "Use PivotTable & PivotChart",
        "Edit objects",
        "Edit scenarios",
    ] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    h.get_by_label("Insert hyperlinks").click();
    h.get_by_label("Edit objects").click();
    h.run_steps(2);
    ok(&mut h);
    let p = h.state().session.active().unwrap().wb.active().unwrap().protection.clone().unwrap();
    assert!(p.insert_hyperlinks && p.edit_objects && !p.edit_scenarios && !p.pivot_tables);
    // Without the first box, OK protects nothing.
    h.state_mut().run("review.unprotectSheet", json!({})).unwrap();
    h.state_mut().open_dialog("protectSheet", json!({}));
    h.run_steps(3);
    h.get_by_label("Protect worksheet and contents of locked cells").click();
    h.run_steps(2);
    ok(&mut h);
    assert!(dialog(&h).is_none());
    assert!(!h.state().active_sheet_protected());
}

#[test]
fn protect_workbook_protects_the_structure_with_a_confirmed_password() {
    let mut h = crate::support::blank_harness(4);
    h.state_mut().open_dialog("protectWorkbook", json!({}));
    h.run_steps(3);
    assert!(h.query_by_label("Structure").is_some());
    type_into_password(&mut h, "honbu");
    ok(&mut h);
    assert_eq!(dialog(&h).as_deref(), Some("confirmPassword"));
    // A different password is refused, and nothing is protected yet.
    type_into_password(&mut h, "other");
    ok(&mut h);
    assert!(h.state().message.as_ref().is_some_and(|(_, m)| m.contains("not identical")));
    h.state_mut().message = None;
    assert!(!h.state().session.active().unwrap().wb.protected_structure);
    type_into_password(&mut h, "honbu");
    ok(&mut h);
    assert!(dialog(&h).is_none());
    assert!(h.state().session.active().unwrap().wb.protected_structure);
    let ja = gridcraft_ui_egui::i18n::Language::Ja;
    assert_eq!([ja.tr("Structure"), ja.tr("Protect workbook for")], ["シート構成", "保護対象"]);
}

#[test]
fn new_range_adds_an_edit_range_and_typing_into_it_asks_for_its_password() {
    let mut s = crate::support::blank_session();
    s.execute("selection.set", json!({"range": "B4:B9"})).unwrap();
    let mut h = crate::support::harness(s, 4);
    h.state_mut().open_dialog("allowEditRanges", json!({}));
    h.run_steps(3);
    assert_eq!(dialog(&h).as_deref(), Some("allowEditRanges"));
    h.get_by_label("New…").click();
    h.run_steps(3);
    assert_eq!(dialog(&h).as_deref(), Some("newEditRange"));
    let values = &h.state().dialog.as_ref().unwrap().values;
    assert_eq!((values["title"].as_str(), values["range"].as_str()), (Some("Range1"), Some("=$B$4:$B$9")));
    type_into_password(&mut h, "ekimae");
    ok(&mut h);
    assert_eq!(dialog(&h).as_deref(), Some("confirmPassword"), "{:?} {:?}", h.state().message, h.state().dialog.as_ref().map(|d| d.values.clone()));
    type_into_password(&mut h, "ekimae");
    ok(&mut h);
    assert_eq!(dialog(&h).as_deref(), Some("allowEditRanges"), "back to the list");
    assert!(h.query_by_label("Range1").is_some());
    let ranges = h.state().session.active().unwrap().wb.active().unwrap().edit_ranges.clone();
    assert_eq!((ranges.len(), ranges[0].ranges[0].a1()), (1, "B4:B9".to_string()));
    ok(&mut h);
    assert!(dialog(&h).is_none());

    // Protected, B4 is the range's: typing asks for its password there and then.
    h.state_mut().run("review.protectSheet", json!({})).unwrap();
    h.state_mut().run("selection.set", json!({"range": "B4"})).unwrap();
    h.run_steps(2);
    h.state_mut().begin_edit(Some("5".into()), false);
    h.run_steps(3);
    assert_eq!(dialog(&h).as_deref(), Some("unlockRange"));
    assert!(h.state().editor.is_none());
    type_into_password(&mut h, "wrong");
    ok(&mut h);
    assert!(h.state().message.is_some(), "a wrong password is refused");
    h.state_mut().message = None;
    h.state_mut().dialog.as_mut().unwrap().values.insert("password".into(), json!(""));
    type_into_password(&mut h, "ekimae");
    ok(&mut h);
    assert!(dialog(&h).is_none());
    h.state_mut().begin_edit(Some("5".into()), false);
    h.run_steps(2);
    assert!(h.state().editor.is_some(), "unlocked: the cell takes the edit");
    let ja = gridcraft_ui_egui::i18n::Language::Ja;
    assert_eq!(
        ["Allow Users to Edit Ranges", "New Range", "Unlock Range", "Confirm Password", "Range{n}"].map(|l| ja.tr(l)),
        ["範囲の編集の許可", "新しい範囲", "ロック解除範囲", "パスワードの確認", "範囲{n}"]
    );
}
