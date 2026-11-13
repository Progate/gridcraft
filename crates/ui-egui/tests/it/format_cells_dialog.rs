//! Format Cells' Number and Alignment tabs through real egui controls.

use egui::vec2;
use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

fn harness() -> egui_kittest::Harness<'static, SheetApp> {
    let mut session = Session::new();
    session.new_workbook();
    session.run("range.setValues", json!({"range": "A1", "values": [[1234.5]]})).unwrap();
    session.run("selection.set", json!({"range": "A1"})).unwrap();
    let mut h = crate::common::builder().with_size(vec2(1200.0, 900.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        crate::common::app(session),
    );
    h.run_steps(6);
    h
}

fn style(h: &egui_kittest::Harness<'static, SheetApp>) -> gridcraft_engine::model::Style {
    let doc = h.state().session.active().unwrap();
    let sheet = doc.wb.active().unwrap();
    doc.wb.styles.get(sheet.style_id(CellRef::new(0, 0))).clone()
}

#[test]
fn number_tab_builds_the_category_code_and_reopens_on_it() {
    let mut h = harness();
    h.state_mut().open_dialog("formatCells", json!({"tab": "Number"}));
    h.run_steps(6);
    h.get_by_label("Percentage").click();
    h.run_steps(6);
    h.get_by_label("   OK   ").click();
    h.run_steps(6);
    assert_eq!(style(&h).num_fmt.as_str(), "0.00%");
    // Reopened, the dialog selects Percentage with two decimals; nothing changed sends nothing.
    h.state_mut().open_dialog("formatCells", json!({"tab": "Number"}));
    h.run_steps(6);
    let nf = h.state().dialog.as_ref().unwrap().values.get("nf").cloned().unwrap();
    assert_eq!((nf["category"].as_str(), nf["decimals"].as_u64()), (Some("Percentage"), Some(2)));
    h.get_by_label("   OK   ").click();
    h.run_steps(6);
    assert_eq!(style(&h).num_fmt.as_str(), "0.00%");
}

#[test]
fn alignment_tab_vertical_text_box_sets_text_rotation_255() {
    let mut h = harness();
    h.state_mut().open_dialog("formatCells", json!({"tab": "Alignment"}));
    h.run_steps(6);
    h.get_by_label("Vertical Text").click();
    h.run_steps(6);
    h.get_by_label("Wrap text").click();
    h.run_steps(6);
    h.get_by_label("   OK   ").click();
    h.run_steps(6);
    let st = style(&h);
    assert_eq!((st.align.rotation, st.align.wrap), (255, true));
}
