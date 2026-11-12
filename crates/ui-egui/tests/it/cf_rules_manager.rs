//! The Conditional Formatting Rules Manager: a sheet with thousands of rules scrolls inside the
//! dialog (issue #3321), and New Rule…, Edit Rule…, Delete Rule, Duplicate Rule, the move buttons
//! and Stop If True change a working copy that OK and Apply write, as in Excel.

use egui::vec2;
use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_engine::model::CfRule;
use gridcraft_ui_egui::SheetApp;
use gridcraft_ui_egui::control::{Outcome, handle};
use serde_json::{Value as Json, json};

type Harness = egui_kittest::Harness<'static, SheetApp>;

const RULES: usize = 3000;
const SCREEN_H: f32 = 800.0;

/// One rule per entry of `ranges`, added in order (so the last one is listed first), with the
/// manager opened on a selection covering them all.
fn harness_with_ranges(ranges: &[String], select: &str) -> Harness {
    let mut session = Session::new();
    session.new_workbook();
    for range in ranges {
        session
            .execute(
                "home.conditionalFormat",
                json!({"range": range, "rule": {"type": "cellIs", "operator": "greaterThan", "value": "5", "preset": "yellowFill"}}),
            )
            .unwrap();
    }
    session.execute("selection.set", json!({"range": select})).unwrap();
    let mut h = crate::common::builder().with_size(vec2(1200.0, SCREEN_H)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        crate::common::app(session),
    );
    h.run_steps(4);
    h.state_mut().open_dialog("manageRules", json!({}));
    h.run_steps(6);
    h
}

fn harness_with_rules(n: usize) -> Harness {
    let ranges: Vec<String> = (0..n).map(|i| format!("A{}", i + 1)).collect();
    harness_with_ranges(&ranges, &format!("A1:A{}", n.max(1)))
}

fn control(h: &mut Harness, method: &str, params: Json) -> Json {
    let ctx = h.ctx.clone();
    match handle(h.state_mut(), &ctx, method, &params) {
        Outcome::Done(v) => v,
        Outcome::Screenshot { .. } => json!(null),
    }
}

/// The ranges of every rule on the sheet, in priority order.
fn rule_ranges(h: &Harness) -> Vec<String> {
    let sh = h.state().session.active().unwrap().wb.active().unwrap();
    let mut rules = sh.cond_formats.clone();
    rules.sort_by_key(|c| c.priority);
    rules.iter().map(|cf| cf.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(",")).collect()
}

fn press(h: &mut Harness, label: &str) {
    let window = h.get_by_label("Conditional Formatting Rules Manager").rect();
    // OK and Cancel are padded with spaces.
    h.query_all_by(|n| n.label().is_some_and(|l| l.trim() == label))
        .find(|n| window.contains_rect(n.rect()))
        .unwrap_or_else(|| panic!("no {label} button in the dialog"))
        .click();
    h.run_steps(4);
}

#[test]
fn many_rules_scroll_inside_a_dialog_that_fits_the_window() {
    let mut h = harness_with_rules(RULES);
    let rows = h.query_all_by_label_contains("Cell Value > 5").count();
    assert!(rows > 0, "the first rules are listed");
    assert!(rows < 100, "only the visible rules are laid out, got {rows}");
    let window = h.get_by_label("Conditional Formatting Rules Manager").rect();
    assert!(window.bottom() <= SCREEN_H, "the dialog fits the window, bottom at {}", window.bottom());
    // Delete Rule removes the selected (first) rule from the list; OK writes it.
    press(&mut h, "Delete Rule");
    assert_eq!(rule_ranges(&h).len(), RULES, "nothing changes before OK");
    press(&mut h, "OK");
    let ranges = rule_ranges(&h);
    assert_eq!(ranges.len(), RULES - 1);
    // New rules go first, so the one deleted was A3000.
    assert_eq!(ranges[0], format!("A{}", RULES - 1));
    assert!(h.state().dialog.is_none(), "OK closes the manager");
}

#[test]
fn an_empty_sheet_says_so() {
    let h = harness_with_rules(0);
    h.get_by_label("There are no conditional formatting rules on this sheet.");
}

#[test]
fn rules_are_moved_duplicated_and_stopped_then_applied() {
    let mut h = harness_with_ranges(&["A1".into(), "A2".into(), "A3".into()], "A1:A3");
    assert_eq!(rule_ranges(&h), ["A3", "A2", "A1"]);
    // The first rule is selected: move it down, then copy it.
    press(&mut h, "▼");
    press(&mut h, "Duplicate Rule");
    press(&mut h, "Apply");
    assert_eq!(rule_ranges(&h), ["A2", "A3", "A3", "A1"], "Apply writes the list and the dialog stays");
    assert!(h.state().dialog.is_some());
    // Stop If True on the selected copy, written by OK.
    let window = h.get_by_label("Conditional Formatting Rules Manager").rect();
    let boxes: Vec<_> = h.query_all_by_label("Stop If True").filter(|n| window.contains_rect(n.rect())).map(|n| n.rect()).collect();
    assert_eq!(boxes.len(), 5, "a header and one box per rule");
    h.query_all_by_label("Stop If True").filter(|n| window.contains_rect(n.rect())).nth(2).unwrap().click();
    h.run_steps(4);
    press(&mut h, "OK");
    let sh = h.state().session.active().unwrap().wb.active().unwrap();
    let mut rules = sh.cond_formats.clone();
    rules.sort_by_key(|c| c.priority);
    assert_eq!(rules.iter().map(|c| c.stop_if_true).collect::<Vec<_>>(), [false, true, false, false]);
}

#[test]
fn cancel_leaves_the_rules_alone() {
    let mut h = harness_with_ranges(&["A1".into(), "A2".into()], "A1:A2");
    press(&mut h, "Delete Rule");
    press(&mut h, "Cancel");
    assert_eq!(rule_ranges(&h), ["A2", "A1"]);
}

#[test]
fn current_selection_lists_only_its_rules() {
    let h = harness_with_ranges(&["A1".into(), "C5".into()], "C5");
    assert_eq!(h.query_all_by_label_contains("Cell Value > 5").count(), 1);
}

#[test]
fn edit_rule_opens_the_editor_filled_in_and_puts_the_rule_back() {
    let mut h = harness_with_ranges(&["A1:A9".into()], "A1");
    press(&mut h, "Edit Rule…");
    let d = h.state().dialog.as_ref().expect("the rule editor is open");
    assert_eq!(d.name(), "newFormattingRule");
    assert_eq!(d.title, "Edit Formatting Rule");
    assert_eq!(
        (d.values["ruleType"].as_str(), d.values["operator"].as_str(), d.values["value"].as_str()),
        (Some("contain"), Some("greaterThan"), Some("5"))
    );
    control(&mut h, "ui.dialog.set", json!({"field": "operator", "value": "between"}));
    control(&mut h, "ui.dialog.set", json!({"field": "value", "value": "100"}));
    control(&mut h, "ui.dialog.set", json!({"field": "value2", "value": "200"}));
    let window = h.get_by_label("Edit Formatting Rule").rect();
    h.query_all_by_label_contains("OK").find(|n| window.contains_rect(n.rect())).unwrap().click();
    h.run_steps(6);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name().to_string()).as_deref(), Some("manageRules"), "back in the manager");
    h.get_by_label_contains("Cell Value between 100 and 200");
    press(&mut h, "OK");
    let sh = h.state().session.active().unwrap().wb.active().unwrap();
    assert!(matches!(&sh.cond_formats[0].rule, CfRule::CellIs { a, b: Some(b), .. } if a == "100" && b == "200"), "{:?}", sh.cond_formats);
    assert_eq!(sh.cond_formats[0].ranges[0].a1(), "A1:A9", "Applies To kept");
}

#[test]
fn applies_to_is_edited_in_its_reference_box() {
    let mut h = harness_with_ranges(&["A1:A9".into()], "A1");
    control(&mut h, "ui.dialog.set", json!({"field": "appliesTo", "value": "=$B$2:$B$20,$D$1"}));
    h.run_steps(4);
    press(&mut h, "OK");
    assert_eq!(rule_ranges(&h), ["B2:B20,D1"]);
}
