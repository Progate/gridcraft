//! Original border diagrams, painted with the same strokes as worksheet cells.

use egui::{Align2, Rect, Response, Ui, pos2, vec2};
use gridcraft_engine::model::BorderStyle;

use crate::grid::paint_border;
use crate::theme::{self, Tokens};

/// A labelled menu button with a two-by-two diagram to distinguish inside edges
/// from the selection outline. Labels stay available to accessibility tools.
pub(crate) fn preset_button(ui: &mut Ui, label: &str, preset: &str) -> Response {
    let response = ui.add(egui::Button::new("").frame(false).min_size(vec2(254.0, 26.0)));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let t = Tokens::get(ui.ctx());
    let rect = Rect::from_center_size(pos2(response.rect.left() + 16.0, response.rect.center().y), vec2(20.0, 16.0));
    let edges = [
        [rect.left_top(), rect.right_top()],
        [rect.left_bottom(), rect.right_bottom()],
        [rect.left_top(), rect.left_bottom()],
        [rect.right_top(), rect.right_bottom()],
        [rect.left_center(), rect.right_center()],
        [rect.center_top(), rect.center_bottom()],
    ];
    use BorderStyle::{Double, None, Thick, Thin};
    let styles = match preset {
        "bottom" => [None, Thin, None, None, None, None],
        "top" => [Thin, None, None, None, None, None],
        "left" => [None, None, Thin, None, None, None],
        "right" => [None, None, None, Thin, None, None],
        "all" => [Thin; 6],
        "outside" => [Thin, Thin, Thin, Thin, None, None],
        "thickOutside" => [Thick, Thick, Thick, Thick, None, None],
        "doubleBottom" => [None, Double, None, None, None, None],
        "thickBottom" => [None, Thick, None, None, None, None],
        "topBottom" => [Thin, Thin, None, None, None, None],
        "topThickBottom" => [Thin, Thick, None, None, None, None],
        "topDoubleBottom" => [Thin, Double, None, None, None, None],
        "inside" => [None, None, None, None, Thin, Thin],
        _ => [None; 6],
    };
    for (points, style) in edges.into_iter().zip(styles) {
        if style == None {
            ui.painter().extend(egui::Shape::dotted_line(&points, t.text_disabled, 3.0, 0.45));
        } else {
            paint_border(ui.painter(), points, style, t.text);
        }
    }
    ui.painter().text(pos2(response.rect.left() + 36.0, response.rect.center().y), Align2::LEFT_CENTER, label, theme::ui_font(12.5), t.text);
    response
}
