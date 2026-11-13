//! Format Cells' Border tab on a selection (outline, inside lines, diagonals) and Home ›
//! Orientation's menu items.

use gridcraft_core::CellRef;
use gridcraft_model::{BorderLine, BorderStyle, Borders, Color};
use serde_json::json;

use super::s;
use crate::Session;

fn borders(s: &Session, cell: &str) -> Borders {
    let d = s.doc().unwrap();
    let sh = d.wb.active().unwrap();
    d.wb.styles.get(sh.style_id(CellRef::parse(cell).unwrap())).borders
}

fn rotation(s: &Session, cell: &str) -> i16 {
    let d = s.doc().unwrap();
    let sh = d.wb.active().unwrap();
    d.wb.styles.get(sh.style_id(CellRef::parse(cell).unwrap())).align.rotation
}

#[test]
fn border_tab_outline_and_inside_lines_go_on_the_selection_like_excel() {
    let mut s = s();
    let thick = json!({"style": "thick", "color": "#FF0000"});
    s.execute(
        "home.formatCells",
        json!({"range": "B2:C3", "edges": {"top": thick, "bottom": thick, "left": thick, "right": thick, "insideHorizontal": "thin", "insideVertical": "dotted"}}),
    )
    .unwrap();
    let red = BorderLine { style: BorderStyle::Thick, color: Color::Rgb(0xFF0000) };
    let thin = BorderLine { style: BorderStyle::Thin, color: Color::Auto };
    let dotted = BorderLine { style: BorderStyle::Dotted, color: Color::Auto };
    assert_eq!(borders(&s, "B2"), Borders { top: red, left: red, bottom: thin, right: dotted, ..Borders::default() });
    assert_eq!(borders(&s, "C3"), Borders { top: thin, left: dotted, bottom: red, right: red, ..Borders::default() });
    assert_eq!(borders(&s, "D2"), Borders::default(), "nothing outside the selection");
    // Lines left out of `edges` stay; `none` removes one, from the facing cell outside too.
    s.execute("home.borders", json!({"range": "B1", "preset": "bottom", "style": "double"})).unwrap();
    s.execute("home.formatCells", json!({"range": "B2:C3", "edges": {"top": "none", "diagonalDown": {"style": "mediumDashDotDot"}}})).unwrap();
    assert!(borders(&s, "B2").top.is_none());
    assert!(borders(&s, "B1").bottom.is_none());
    assert_eq!(borders(&s, "B2").left, red);
    assert_eq!(borders(&s, "C3").diag_down.style, BorderStyle::MediumDashDotDot);
    assert!(borders(&s, "C3").diag_up.is_none());
    // One edit: a single undo takes the whole tab back.
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(borders(&s, "B2").top, red);
}

#[test]
fn format_cells_sets_style_and_edges_in_one_undo_step() {
    let mut s = s();
    s.execute("home.formatCells", json!({"range": "A1:B1", "style": {"num_fmt": "0.0"}, "edges": {"bottom": "double"}})).unwrap();
    assert_eq!(borders(&s, "B1").bottom.style, BorderStyle::Double);
    s.execute("edit.undo", json!({})).unwrap();
    assert!(borders(&s, "B1").bottom.is_none());
    let d = s.doc().unwrap();
    assert_eq!(d.wb.styles.get(d.wb.active().unwrap().style_id(CellRef::new(0, 0))).num_fmt.as_str(), "General");
    assert!(s.execute("home.formatCells", json!({"range": "A1", "edges": {"middle": "thin"}})).is_err());
}

#[test]
fn a_colour_patch_replaces_the_colour_of_another_kind() {
    let mut s = s();
    s.execute("home.fontColor", json!({"range": "A1", "color": "#C00000"})).unwrap();
    s.execute("home.formatCells", json!({"range": "A1", "style": {"font": {"color": {"Theme": [4, 0]}}}})).unwrap();
    let d = s.doc().unwrap();
    assert_eq!(d.wb.styles.get(d.wb.active().unwrap().style_id(CellRef::new(0, 0))).font.color, Color::Theme(4, 0));
}

#[test]
fn orientation_menu_angles_and_the_checked_item_returns_to_horizontal() {
    let mut s = s();
    for (angle, want) in [("counterclockwise", 45), ("clockwise", -45), ("vertical", 255), ("up", 90), ("down", -90)] {
        s.execute("home.orientation", json!({"range": "A1", "angle": angle, "toggle": true})).unwrap();
        assert_eq!(rotation(&s, "A1"), want, "{angle}");
    }
    s.execute("home.orientation", json!({"range": "A1", "angle": "down", "toggle": true})).unwrap();
    assert_eq!(rotation(&s, "A1"), 0);
}
