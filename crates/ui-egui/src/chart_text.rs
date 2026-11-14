//! Chart titles and axis titles selected and edited on the chart, as in Excel: a click on a title
//! selects it (a frame around it), a second click edits its text in place, and typing while it is
//! selected replaces the text. The draft becomes one undoable `chart.set`.

use egui::{Color32, FontId, Key, Modifiers, Rect, Stroke, StrokeKind, TextEdit, vec2};
use gridcraft_chart::ChartElement;
use gridcraft_engine::model::{Sheet, Workbook};
use serde_json::{Value as Json, json};

use crate::{SheetApp, grid::Geo, theme};

/// A title being edited in place.
#[derive(Clone, Debug)]
pub struct ChartTextEdit {
    pub chart: u32,
    pub element: ChartElement,
    pub text: String,
    original: String,
    uid: u64,
    sheet: usize,
    request_focus: bool,
    rect: Option<Rect>,
}

/// The stable id of the in-place title editor.
pub(crate) fn editor_id() -> egui::Id {
    egui::Id::unique("gridcraft.chart_text_editor")
}

/// The text `element` of chart `id` shows now: a title's own text or what its automatic title
/// reads (グラフ タイトル), an axis title's text.
fn shown_text(app: &SheetApp, id: u32, element: ChartElement) -> Option<String> {
    let d = app.session.active()?;
    let si = d.wb.active_sheet();
    let c = d.wb.sheet(si)?.charts.iter().find(|c| c.id == id)?;
    match element {
        ChartElement::Title => gridcraft_engine::cmd::insert::shown_chart_title(&d.wb, si, c, app.session.prefs.chart_title()),
        ChartElement::HorizontalAxisTitle => c.axis_title(true).map(str::to_string),
        ChartElement::VerticalAxisTitle => c.axis_title(false).map(str::to_string),
    }
}

/// The `chart.set` parameters that give `element` of chart `id` the text `text`.
pub fn set_params(app: &SheetApp, id: u32, element: ChartElement, text: &str) -> Json {
    let across = app.session.active().and_then(|d| d.wb.active()?.charts.iter().find(|c| c.id == id).map(|c| c.value_axis_across())).unwrap_or(false);
    let key = match element {
        ChartElement::Title => return json!({"chart": id, "title": text}),
        ChartElement::HorizontalAxisTitle if across => "yTitle",
        ChartElement::HorizontalAxisTitle => "xTitle",
        ChartElement::VerticalAxisTitle if across => "xTitle",
        ChartElement::VerticalAxisTitle => "yTitle",
    };
    json!({"chart": id, key: text})
}

impl SheetApp {
    /// The selected chart's selected title, when its chart is still the selected object.
    pub fn selected_chart_element(&self) -> Option<(u32, ChartElement)> {
        self.grid.chart_element.filter(|(id, _)| self.selected_chart() == Some(*id))
    }

    /// Starts editing the selected title in place, with `typed` replacing its text when given.
    pub fn begin_chart_text_edit(&mut self, id: u32, element: ChartElement, typed: Option<String>) -> bool {
        let Some(original) = shown_text(self, id, element) else { return false };
        let Some(d) = self.session.active() else { return false };
        if d.wb.active().is_some_and(Sheet::is_protected) {
            return false;
        }
        let text = typed.unwrap_or_else(|| original.clone());
        self.grid.chart_text =
            Some(ChartTextEdit { chart: id, element, text, original, uid: d.uid, sheet: d.wb.active_sheet(), request_focus: true, rect: None });
        self.grid.chart_element = Some((id, element));
        true
    }

    /// Saves the title being edited (nothing when unchanged).
    pub fn commit_chart_text_edit(&mut self) {
        let Some(ed) = self.grid.chart_text.take() else { return };
        if self.view_key() != Some((ed.uid, ed.sheet)) || ed.text == ed.original {
            return;
        }
        let params = set_params(self, ed.chart, ed.element, &ed.text);
        self.run_or_alert("chart.set", params);
    }

    pub fn editing_chart_text(&self) -> bool {
        self.grid.chart_text.is_some()
    }
}

/// Before the frame: Escape drops the draft, a press outside the editor saves it.
pub(crate) fn before_ui(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(ed) = &app.grid.chart_text else { return };
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        app.grid.chart_text = None;
        ctx.memory_mut(|m| m.surrender_focus(editor_id()));
        return;
    }
    let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| ed.rect.is_none_or(|r| !r.contains(p))));
    if outside {
        app.commit_chart_text_edit();
        ctx.memory_mut(|m| m.surrender_focus(editor_id()));
    }
}

/// Frames the selected title and draws the in-place editor over the title being edited.
pub(crate) fn show(app: &mut SheetApp, ui: &mut egui::Ui, geo: &Geo, wb: &Workbook, sh: &Sheet) {
    let si = wb.active_sheet();
    if let Some((id, element)) = app.selected_chart_element()
        && app.grid.chart_text.is_none()
    {
        match crate::chartview::chart_element_rect(ui.painter(), geo, wb, si, sh, id, element) {
            Some(r) => {
                let r = geo.mirror_rect(r);
                let t = theme::Tokens::get(ui.ctx());
                ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, t.sheet_control_border), StrokeKind::Outside);
                for c in [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()] {
                    ui.painter().circle_filled(c, 3.0, t.sheet_control_bg);
                    ui.painter().circle_stroke(c, 3.0, Stroke::new(1.0, t.sheet_control_border));
                }
            }
            // The title is gone (deleted, or the chart type has no axes): nothing stays selected.
            None => app.grid.chart_element = None,
        }
    }
    let Some(mut ed) = app.grid.chart_text.take() else { return };
    if app.view_key() != Some((ed.uid, ed.sheet)) {
        app.grid.chart_text = Some(ed);
        return;
    }
    let Some(r) = crate::chartview::chart_element_rect(ui.painter(), geo, wb, si, sh, ed.chart, ed.element)
        .or_else(|| sh.charts.iter().find(|c| c.id == ed.chart).map(|c| crate::chartview::anchor_rect(geo, sh, &c.anchor)))
    else {
        return;
    };
    // At least wide enough to type in, centred where the title is.
    let size = vec2(r.width().max(160.0 * geo.z.max(0.5)), (16.0 * geo.z).max(20.0));
    let rect = geo.mirror_rect(Rect::from_center_size(r.center(), size)).intersect(geo.cells);
    if !rect.is_positive() {
        app.grid.chart_text = Some(ed);
        return;
    }
    ed.rect = Some(rect);
    let id = editor_id();
    if ed.request_focus {
        egui::text_edit::TextEditState::default().store(ui.ctx(), id);
    }
    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, theme::Tokens::get(ui.ctx()).accent), StrokeKind::Outside);
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(rect));
    let response = child
        .add(
            TextEdit::singleline(&mut ed.text)
                .id(id)
                .font(FontId::proportional((12.0 * geo.z).max(9.0)))
                .text_color(Color32::from_gray(0x40))
                .align(egui::Align2::CENTER_CENTER)
                .frame(egui::Frame::NONE)
                .desired_width(rect.width())
                .char_limit(255),
        )
        .accessible_name(app.ui.language.tr("Chart Title"));
    if ed.request_focus {
        response.request_focus();
        // Typing over the title leaves the caret after what was typed.
        if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
            let end = egui::text::CCursor::new(ed.text.chars().count());
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
            state.store(ui.ctx(), id);
        }
        ed.request_focus = false;
    }
    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    app.grid.chart_text = Some(ed);
    if enter {
        app.commit_chart_text_edit();
    }
}
