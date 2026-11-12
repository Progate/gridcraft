//! Repeat (`edit.repeat`): the last formatting, insert or delete action again, on the selection.

use gridcraft_core::CellRef;
use gridcraft_model::Style;
use serde_json::json;

use super::s;
use crate::{EngineError, Session};

fn style(s: &Session, a1: &str) -> Style {
    let d = s.doc().unwrap();
    d.wb.styles.get(d.wb.active().unwrap().style_id(CellRef::parse(a1).unwrap())).clone()
}

fn select(s: &mut Session, range: &str) {
    s.execute("selection.set", json!({"range": range})).unwrap();
}

#[test]
fn repeat_applies_the_last_format_to_the_selection() {
    let mut s = s();
    s.execute("home.bold", json!({"range": "A1"})).unwrap();
    select(&mut s, "B2");
    let r = s.execute("edit.repeat", json!({})).unwrap();
    assert_eq!(r["repeated"], "home.bold");
    assert!(style(&s, "B2").font.bold);
    // The step is the action's own, so Undo says what it undoes.
    assert_eq!(s.doc().unwrap().undo.last().unwrap().label, "Bold");
    assert_eq!(crate::cmd::edit::repeat_label(&s).as_deref(), Some("Bold"));
}

#[test]
fn repeat_keeps_the_action_params_and_repeats_again() {
    let mut s = s();
    select(&mut s, "A1:B2");
    s.execute("home.fillColor", json!({"color": "#FFFF00"})).unwrap();
    let fill = style(&s, "A1").fill;
    select(&mut s, "D4");
    s.execute("edit.repeat", json!({})).unwrap();
    assert_eq!(style(&s, "D4").fill, fill);
    select(&mut s, "F6");
    s.execute("edit.repeat", json!({})).unwrap();
    assert_eq!(style(&s, "F6").fill, fill);
    assert_eq!(style(&s, "E5").fill, Style::default().fill);
}

#[test]
fn repeat_inserts_rows_at_the_new_selection() {
    let mut s = s();
    s.execute("cell.set", json!({"cell": "A5", "input": "x"})).unwrap();
    s.execute("home.insertRows", json!({"rows": "1:1"})).unwrap();
    select(&mut s, "3:3");
    s.execute("edit.repeat", json!({})).unwrap();
    assert_eq!(super::v(&s, "A7"), gridcraft_core::Value::Text("x".into()));
}

#[test]
fn repeat_is_disabled_after_an_action_it_does_not_repeat() {
    let mut s = s();
    assert!(matches!(s.execute("edit.repeat", json!({})), Err(EngineError::Disabled(..))));
    s.execute("home.bold", json!({})).unwrap();
    // Selecting cells doesn't change what Repeat repeats; typing in a cell does.
    select(&mut s, "C3");
    assert!(crate::cmd::edit::can_repeat(&s).is_ok());
    s.execute("cell.set", json!({"cell": "C3", "input": "1"})).unwrap();
    assert!(crate::cmd::edit::can_repeat(&s).is_err());
}

#[test]
fn a_recorded_repeat_is_the_command_it_ran() {
    let mut s = s();
    s.execute("home.italic", json!({"range": "A1"})).unwrap();
    select(&mut s, "B2");
    s.start_recording();
    s.execute("edit.repeat", json!({})).unwrap();
    let rec = s.stop_recording().unwrap();
    assert_eq!(rec.steps.len(), 1);
    assert_eq!(rec.steps[0].id, "home.italic");
    assert_eq!(rec.steps[0].params, json!({"on": true}));
}
