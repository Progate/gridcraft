//! Page Setup's Page and Sheet tabs and Page Layout › Scale to Fit: print titles, fitting to
//! pages and the scale, each one `pageLayout.pageSetup` (one undo step).

use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::model::PrintSettings;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

fn print(h: &Harness) -> PrintSettings {
    h.state().session.active().unwrap().wb.active().unwrap().print.clone()
}

#[test]
fn print_titles_opens_the_sheet_tab_and_ok_sets_the_rows_to_repeat() {
    let mut h = crate::support::blank_harness(4);
    h.state_mut().ui.ribbon_tab = "Page Layout".into();
    h.run_steps(2);
    h.get_by_label("Print Titles").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog.as_ref().unwrap().tab, "Sheet");
    assert!(h.query_by_label("Rows to repeat at top:").is_some());
    assert!(h.query_by_label("Columns to repeat at left:").is_some());
    h.state_mut().dialog.as_mut().unwrap().values.insert("titleRows".into(), json!("$1:$1"));
    h.get_by_label("Page").click();
    h.run_steps(2);
    assert!(h.query_by_label("Paper size:").is_some());
    h.get_by_label("Fit to a number of pages").click();
    // The window grows by a row and settles about its centre over a few frames.
    h.run_steps(5);
    assert!(h.query_by_label("Pages tall (0 = automatic):").is_some());
    let d = h.state_mut().dialog.as_mut().unwrap();
    d.values.insert("fitHeight".into(), json!(0.0));
    h.get_by_label_contains("OK").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "OK closes the dialog");
    let p = print(&h);
    assert_eq!((p.title_rows, p.fit_to), (Some((0, 0)), Some((1, 0))));
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(print(&h), PrintSettings::default(), "one undo step");
}

#[test]
fn scale_to_fit_sets_the_pages_wide_and_back_to_automatic() {
    let mut h = crate::support::blank_harness(4);
    h.state_mut().ui.ribbon_tab = "Page Layout".into();
    h.run_steps(2);
    h.get_by_label("Width:").click();
    h.run_steps(2);
    h.get_by_label("1 page").click();
    h.run_steps(2);
    assert_eq!(print(&h).fit_to, Some((1, 0)));
    h.get_by_label("Width:").click();
    h.run_steps(2);
    h.get_by_label("Automatic").click();
    h.run_steps(2);
    assert_eq!(print(&h).fit_to, None, "both automatic is no fitting");
}
