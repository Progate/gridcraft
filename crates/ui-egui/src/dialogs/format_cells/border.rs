//! Format Cells › Border, laid out as Excel's: the Line Style grid and Color on the left, the
//! None / Outline / Inside presets and the Border preview with its eight edge buttons on the
//! right. Clicking an edge button or the edge itself in the preview sets the chosen line there, or
//! removes it when it is already that line.

use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use gridcraft_engine::model::{BorderLine, BorderStyle, Borders, Color, Theme};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::grid::paint_border;
use crate::theme::{self, Tokens};

pub(super) const TOP: usize = 0;
pub(super) const INSIDE_H: usize = 1;
pub(super) const BOTTOM: usize = 2;
pub(super) const DIAG_UP: usize = 3;
pub(super) const LEFT: usize = 4;
pub(super) const INSIDE_V: usize = 5;
pub(super) const RIGHT: usize = 6;
pub(super) const DIAG_DOWN: usize = 7;

/// The edges in the order of Excel's buttons (the column left of the preview, then the row below
/// it), with the key `home.formatCells` takes and the button's label.
pub(super) const EDGES: [(&str, &str); 8] = [
    ("top", "Border Edge|Top"),
    ("insideHorizontal", "Border Edge|Inside Horizontal"),
    ("bottom", "Border Edge|Bottom"),
    ("diagonalUp", "Border Edge|Diagonal Up"),
    ("left", "Border Edge|Left"),
    ("insideVertical", "Border Edge|Inside Vertical"),
    ("right", "Border Edge|Right"),
    ("diagonalDown", "Border Edge|Diagonal Down"),
];

/// Excel's Style grid, column by column: None and the thin styles, then the medium and thick ones.
pub(super) const STYLES: [(BorderStyle, &str); 14] = [
    (BorderStyle::None, "none"),
    (BorderStyle::Hair, "hair"),
    (BorderStyle::Dotted, "dotted"),
    (BorderStyle::DashDotDot, "dashDotDot"),
    (BorderStyle::DashDot, "dashDot"),
    (BorderStyle::Dashed, "dashed"),
    (BorderStyle::Thin, "thin"),
    (BorderStyle::MediumDashDotDot, "mediumDashDotDot"),
    (BorderStyle::SlantDashDot, "slantDashDot"),
    (BorderStyle::MediumDashDot, "mediumDashDot"),
    (BorderStyle::MediumDashed, "mediumDashed"),
    (BorderStyle::Medium, "medium"),
    (BorderStyle::Thick, "thick"),
    (BorderStyle::Double, "double"),
];

/// The Border tab's draft: the chosen line, and each edge's line now and when the dialog opened
/// (`None`: the selection's cells differ there, shown grey and left as they are).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Draft {
    pub style: BorderStyle,
    pub color: Color,
    pub cur: [Option<BorderLine>; 8],
    pub orig: [Option<BorderLine>; 8],
    /// The selection has several rows (Inside Horizontal applies) / columns (Inside Vertical).
    pub rows: bool,
    pub cols: bool,
    /// A conditional format's border: outline only, no inside lines or diagonals.
    pub dxf: bool,
}

impl Draft {
    pub(super) fn line(&self) -> BorderLine {
        BorderLine { style: self.style, color: if self.style == BorderStyle::None { Color::Auto } else { self.color } }
    }

    /// Whether the edge exists for this selection (inside lines need several rows or columns).
    pub(super) fn has(&self, edge: usize) -> bool {
        match edge {
            INSIDE_H => self.rows,
            INSIDE_V => self.cols,
            DIAG_UP | DIAG_DOWN => !self.dxf,
            _ => true,
        }
    }

    /// Clicking an edge: the chosen line there, or none when it already has that line.
    pub(super) fn toggle(&mut self, edge: usize) {
        if !self.has(edge) {
            return;
        }
        let line = self.line();
        self.cur[edge] = Some(if self.cur[edge] == Some(line) { BorderLine::default() } else { line });
    }

    /// The None, Outline and Inside presets.
    pub(super) fn preset(&mut self, preset: &str) {
        let line = self.line();
        match preset {
            "none" => {
                for (edge, cur) in self.cur.iter_mut().enumerate() {
                    if edge != INSIDE_H || self.rows {
                        *cur = Some(BorderLine::default());
                    }
                }
                if !self.cols {
                    self.cur[INSIDE_V] = self.orig[INSIDE_V];
                }
                if !self.rows {
                    self.cur[INSIDE_H] = self.orig[INSIDE_H];
                }
            }
            "outline" => {
                for edge in [TOP, BOTTOM, LEFT, RIGHT] {
                    self.cur[edge] = Some(line);
                }
            }
            "inside" => {
                for edge in [INSIDE_H, INSIDE_V] {
                    if self.has(edge) {
                        self.cur[edge] = Some(line);
                    }
                }
            }
            _ => {}
        }
    }

    /// The `edges` of `home.formatCells`: the edges changed since the dialog opened.
    pub(super) fn edges(&self) -> Option<Json> {
        let mut m = serde_json::Map::new();
        for (edge, (key, _)) in EDGES.iter().enumerate() {
            if let Some(line) = self.cur[edge]
                && self.cur[edge] != self.orig[edge]
                && self.has(edge)
            {
                let style = STYLES.iter().find(|(s, _)| *s == line.style).map_or("thin", |(_, n)| *n);
                m.insert((*key).into(), if line.is_none() { json!("none") } else { json!({"style": style, "color": color_json(line.color)}) });
            }
        }
        (!m.is_empty()).then_some(Json::Object(m))
    }

    /// The outline edges written into a single style's borders (a conditional format's).
    pub(super) fn apply_to(&self, b: &mut Borders) {
        for (edge, slot) in [(TOP, &mut b.top), (BOTTOM, &mut b.bottom), (LEFT, &mut b.left), (RIGHT, &mut b.right)] {
            if let Some(line) = self.cur[edge] {
                *slot = line;
            }
        }
    }
}

fn color_json(c: Color) -> Json {
    match c {
        Color::Auto => json!("auto"),
        Color::Rgb(rgb) => json!(format!("#{rgb:06X}")),
        Color::Theme(i, tint) => json!({"theme": i, "tint": tint as f64 / 1000.0}),
    }
}

/// The draft for a conditional format's own borders.
pub(super) fn draft_for_borders(b: &Borders) -> Draft {
    let mut cur = [None; 8];
    cur[TOP] = Some(b.top);
    cur[BOTTOM] = Some(b.bottom);
    cur[LEFT] = Some(b.left);
    cur[RIGHT] = Some(b.right);
    Draft { style: BorderStyle::Thin, color: Color::Auto, cur, orig: cur, rows: false, cols: false, dxf: true }
}

/// The draft for the selection: each edge's line when all the cells along it have the same one
/// (a line shared by two cells counts from either), else unknown.
pub(super) fn draft_for_selection(wb: &gridcraft_engine::model::Workbook, range: gridcraft_engine::core::RangeRef) -> Draft {
    use gridcraft_engine::core::CellRef;
    let mut cur = [None; 8];
    let (rows, cols) = (range.height() > 1, range.width() > 1);
    // A whole column or row has too many cells to compare: its edges start unknown.
    if let Some(sh) = wb.active()
        && range.count() <= 40_000
    {
        let borders = |c: CellRef| wb.styles.get(sh.style_id(c)).borders;
        // The line between `a` and the cell after it (below or right), from either side.
        let shared = |a: CellRef, down: bool| {
            let own = if down { borders(a).bottom } else { borders(a).right };
            let next = if down { a.offset(1, 0) } else { a.offset(0, 1) };
            if !own.is_none() {
                return own;
            }
            next.map(|n| if down { borders(n).top } else { borders(n).left }).unwrap_or_default()
        };
        let before = |a: CellRef, up: bool| {
            let own = if up { borders(a).top } else { borders(a).left };
            if !own.is_none() {
                return own;
            }
            let prev = if up { a.offset(-1, 0) } else { a.offset(0, -1) };
            prev.map(|p| if up { borders(p).bottom } else { borders(p).right }).unwrap_or_default()
        };
        let common = |lines: &mut dyn Iterator<Item = BorderLine>| {
            let first = lines.next()?;
            for l in lines {
                if l != first {
                    return None;
                }
            }
            Some(first)
        };
        let (r0, r1, c0, c1) = (range.start.row, range.end.row, range.start.col, range.end.col);
        cur[TOP] = common(&mut (c0..=c1).map(|c| before(CellRef::new(r0, c), true)));
        cur[BOTTOM] = common(&mut (c0..=c1).map(|c| shared(CellRef::new(r1, c), true)));
        cur[LEFT] = common(&mut (r0..=r1).map(|r| before(CellRef::new(r, c0), false)));
        cur[RIGHT] = common(&mut (r0..=r1).map(|r| shared(CellRef::new(r, c1), false)));
        cur[INSIDE_H] =
            if rows { common(&mut (r0..r1).flat_map(|r| (c0..=c1).map(move |c| CellRef::new(r, c))).map(|c| shared(c, true))) } else { None };
        cur[INSIDE_V] =
            if cols { common(&mut (r0..=r1).flat_map(|r| (c0..c1).map(move |c| CellRef::new(r, c))).map(|c| shared(c, false))) } else { None };
        cur[DIAG_UP] = common(&mut range.iter().map(|c| borders(c).diag_up));
        cur[DIAG_DOWN] = common(&mut range.iter().map(|c| borders(c).diag_down));
    }
    Draft { style: BorderStyle::Thin, color: Color::Auto, cur, orig: cur, rows, cols, dxf: false }
}

/// Draws the tab and updates `d`.
pub(super) fn tab(ui: &mut Ui, lang: crate::i18n::Language, d: &mut Draft, theme: &Theme) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            ui.label(egui::RichText::new(lang.tr("Border|Line")).strong());
            ui.label(lang.tr("Style:"));
            ui.horizontal_top(|ui| {
                for column in STYLES.chunks(7) {
                    ui.vertical(|ui| {
                        for (style, name) in column {
                            if style_cell(ui, lang, *style, name, d.style == *style).clicked() {
                                d.style = *style;
                            }
                        }
                    });
                }
            });
            ui.add_space(6.0);
            ui.label(lang.tr("Color:"));
            let swatch = d.color.resolve(theme).map(theme::color32).unwrap_or(Color32::BLACK);
            let id = ui.scope_id().with("border_color");
            let button = ui.add(egui::Button::new(egui::RichText::new("■■■■■■■").color(swatch)).min_size(vec2(120.0, 20.0)));
            button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, lang.tr("Border Color")));
            egui::Popup::menu(&button).id(id).show(|ui| {
                if let Some(c) = crate::widgets::color_palette_fixed(ui, &theme.colors, lang.tr("Automatic")) {
                    d.color = if c == "none" { Color::Auto } else { Color::from_hex(&c).unwrap_or_default() };
                    if d.style == BorderStyle::None {
                        d.style = BorderStyle::Thin;
                    }
                    ui.close();
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(lang.tr("Presets")).strong());
            ui.horizontal(|ui| {
                for (preset, label) in [("none", "Border Preset|None"), ("outline", "Border Preset|Outline"), ("inside", "Border Preset|Inside")] {
                    let enabled = preset != "inside" || d.rows || d.cols;
                    let r = ui.add_enabled_ui(enabled, |ui| preset_button(ui, lang.tr(label), preset)).inner;
                    if r.clicked() {
                        d.preset(preset);
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(lang.tr("Border")).strong());
            let edge_button = |ui: &mut Ui, d: &mut Draft, edge: usize| {
                let r = ui.add_enabled_ui(d.has(edge), |ui| edge_button(ui, lang.tr(EDGES[edge].1), edge, d.cur[edge])).inner;
                if r.clicked() {
                    d.toggle(edge);
                }
            };
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    for edge in [TOP, INSIDE_H, BOTTOM] {
                        edge_button(ui, d, edge);
                        ui.add_space(18.0);
                    }
                });
                preview(ui, d, theme);
            });
            ui.horizontal(|ui| {
                edge_button(ui, d, DIAG_UP);
                ui.add_space(14.0);
                for edge in [LEFT, INSIDE_V, RIGHT] {
                    edge_button(ui, d, edge);
                    ui.add_space(30.0);
                }
                edge_button(ui, d, DIAG_DOWN);
            });
            ui.label(
                egui::RichText::new(
                    lang.tr("The selected border style can be applied by clicking the presets, preview diagram or the buttons above."),
                )
                .small(),
            );
        });
    });
}

fn style_cell(ui: &mut Ui, lang: crate::i18n::Language, style: BorderStyle, name: &str, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(76.0, 20.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, format!("{} {name}", lang.tr("Line style"))));
    let t = Tokens::get(ui.ctx());
    if selected {
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.5, t.accent), StrokeKind::Inside);
    } else if response.hovered() {
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, t.input_border), StrokeKind::Inside);
    }
    let y = rect.center().y;
    if style == BorderStyle::None {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, lang.tr("Border Preset|None"), theme::ui_font(12.0), t.text);
    } else {
        paint_border(ui.painter(), [pos2(rect.left() + 8.0, y), pos2(rect.right() - 8.0, y)], style, t.text);
    }
    response
}

fn preset_button(ui: &mut Ui, label: &str, preset: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(58.0, 54.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let t = Tokens::get(ui.ctx());
    let color = if ui.is_enabled() { t.text } else { t.text_disabled };
    let icon = Rect::from_center_size(pos2(rect.center().x, rect.top() + 17.0), vec2(30.0, 28.0));
    ui.painter().rect_stroke(icon.expand(2.0), 2.0, Stroke::new(1.0, if response.hovered() { t.accent } else { t.input_border }), StrokeKind::Inside);
    let guide = Color32::from_gray(170);
    let lines = [
        [icon.left_top(), icon.right_top()],
        [icon.left_bottom(), icon.right_bottom()],
        [icon.left_top(), icon.left_bottom()],
        [icon.right_top(), icon.right_bottom()],
    ];
    let inside = [[icon.left_center(), icon.right_center()], [icon.center_top(), icon.center_bottom()]];
    for pts in lines.iter().chain(&inside) {
        ui.painter().extend(egui::Shape::dotted_line(pts, guide, 3.0, 0.4));
    }
    match preset {
        "outline" => lines.iter().for_each(|pts| paint_border(ui.painter(), *pts, BorderStyle::Thin, color)),
        "inside" => inside.iter().for_each(|pts| paint_border(ui.painter(), *pts, BorderStyle::Thin, color)),
        _ => {}
    }
    ui.painter().text(pos2(rect.center().x, rect.bottom() - 6.0), Align2::CENTER_CENTER, label, theme::ui_font(11.5), color);
    response
}

/// One of the eight edge buttons: a small diagram with its edge drawn, pressed while the edge has
/// a line.
fn edge_button(ui: &mut Ui, label: &str, edge: usize, line: Option<BorderLine>) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let on = line.is_some_and(|l| !l.is_none());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), on, label));
    let t = Tokens::get(ui.ctx());
    if on {
        ui.painter().rect_filled(rect, 2.0, t.accent.gamma_multiply(0.25));
    }
    ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, if response.hovered() { t.accent } else { t.input_border }), StrokeKind::Inside);
    let r = rect.shrink(6.0);
    for pts in
        [[r.left_top(), r.right_top()], [r.left_bottom(), r.right_bottom()], [r.left_top(), r.left_bottom()], [r.right_top(), r.right_bottom()]]
    {
        ui.painter().extend(egui::Shape::dotted_line(&pts, Color32::from_gray(170), 2.5, 0.4));
    }
    let color = if ui.is_enabled() { t.text } else { t.text_disabled };
    paint_border(ui.painter(), edge_points(r, edge), BorderStyle::Thin, color);
    response
}

fn edge_points(r: Rect, edge: usize) -> [Pos2; 2] {
    match edge {
        TOP => [r.left_top(), r.right_top()],
        INSIDE_H => [r.left_center(), r.right_center()],
        BOTTOM => [r.left_bottom(), r.right_bottom()],
        DIAG_UP => [r.left_bottom(), r.right_top()],
        LEFT => [r.left_top(), r.left_bottom()],
        INSIDE_V => [r.center_top(), r.center_bottom()],
        RIGHT => [r.right_top(), r.right_bottom()],
        _ => [r.left_top(), r.right_bottom()],
    }
}

/// The segments of the preview: each edge's, the diagonals once per cell shown.
pub(super) fn preview_segments(d: &Draft, r: Rect) -> Vec<(usize, [Pos2; 2])> {
    let mut out = Vec::new();
    for edge in [TOP, BOTTOM, LEFT, RIGHT, INSIDE_H, INSIDE_V] {
        if d.has(edge) {
            out.push((edge, edge_points(r, edge)));
        }
    }
    if !d.dxf {
        let xs: Vec<f32> = if d.cols { vec![r.left(), r.center().x, r.right()] } else { vec![r.left(), r.right()] };
        let ys: Vec<f32> = if d.rows { vec![r.top(), r.center().y, r.bottom()] } else { vec![r.top(), r.bottom()] };
        for y in ys.windows(2) {
            for x in xs.windows(2) {
                let cell = Rect::from_min_max(pos2(x[0], y[0]), pos2(x[1], y[1]));
                out.push((DIAG_UP, edge_points(cell, DIAG_UP)));
                out.push((DIAG_DOWN, edge_points(cell, DIAG_DOWN)));
            }
        }
    }
    out
}

/// The edge a click at `p` in the preview is on: the nearest segment within a few pixels.
pub(super) fn edge_at(d: &Draft, r: Rect, p: Pos2) -> Option<usize> {
    let dist = |[a, b]: [Pos2; 2]| {
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
        (a + ab * t).distance(p)
    };
    preview_segments(d, r)
        .into_iter()
        .map(|(edge, seg)| (edge, dist(seg)))
        .filter(|(_, x)| *x <= 7.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e)
}

/// The preview: the selection's cells with sample text, each edge's line (grey where the cells
/// differ), clickable on the edges themselves.
fn preview(ui: &mut Ui, d: &mut Draft, theme: &Theme) {
    let (area, response) = ui.allocate_exact_size(vec2(190.0, 130.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Border preview"));
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(area, 2.0, Color32::WHITE);
    ui.painter().rect_stroke(area, 2.0, Stroke::new(1.0, t.input_border), StrokeKind::Inside);
    let r = area.shrink2(vec2(18.0, 14.0));
    // Corner marks, as Excel's.
    for (corner, dx, dy) in [(r.left_top(), -1.0, -1.0), (r.right_top(), 1.0, -1.0), (r.left_bottom(), -1.0, 1.0), (r.right_bottom(), 1.0, 1.0)] {
        let mark = Stroke::new(1.0, Color32::from_gray(150));
        ui.painter().line_segment([corner + vec2(dx * 3.0, 0.0), corner + vec2(dx * 10.0, 0.0)], mark);
        ui.painter().line_segment([corner + vec2(0.0, dy * 3.0), corner + vec2(0.0, dy * 10.0)], mark);
    }
    let lang = crate::i18n::current(ui.ctx());
    let xs: Vec<f32> = if d.cols { vec![r.left(), r.center().x, r.right()] } else { vec![r.left(), r.right()] };
    let ys: Vec<f32> = if d.rows { vec![r.top(), r.center().y, r.bottom()] } else { vec![r.top(), r.bottom()] };
    for y in ys.windows(2) {
        for x in xs.windows(2) {
            let c = pos2((x[0] + x[1]) / 2.0, (y[0] + y[1]) / 2.0);
            ui.painter().text(c, Align2::CENTER_CENTER, lang.tr("Border preview|Text"), theme::ui_font(12.0), Color32::from_gray(60));
        }
    }
    for (edge, seg) in preview_segments(d, r) {
        match d.cur[edge] {
            Some(line) if !line.is_none() => {
                let color = line.color.resolve(theme).map(theme::color32).unwrap_or(Color32::BLACK);
                paint_border(ui.painter(), seg, line.style, color);
            }
            Some(_) => {}
            None => paint_border(ui.painter(), seg, BorderStyle::Thin, Color32::from_gray(185)),
        }
    }
    if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
        && let Some(edge) = edge_at(d, r, p)
    {
        d.toggle(edge);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(rows: bool, cols: bool) -> Draft {
        let none = Some(BorderLine::default());
        Draft { style: BorderStyle::Thin, color: Color::Auto, cur: [none; 8], orig: [none; 8], rows, cols, dxf: false }
    }

    #[test]
    fn presets_and_edge_clicks_build_the_edges_sent() {
        let mut d = draft(true, true);
        assert_eq!(d.edges(), None);
        d.style = BorderStyle::Thick;
        d.color = Color::Rgb(0xFF0000);
        d.preset("outline");
        d.style = BorderStyle::Dotted;
        d.color = Color::Auto;
        d.preset("inside");
        assert_eq!(
            d.edges(),
            Some(json!({
                "top": {"style": "thick", "color": "#FF0000"}, "bottom": {"style": "thick", "color": "#FF0000"},
                "left": {"style": "thick", "color": "#FF0000"}, "right": {"style": "thick", "color": "#FF0000"},
                "insideHorizontal": {"style": "dotted", "color": "auto"}, "insideVertical": {"style": "dotted", "color": "auto"},
            }))
        );
        // Clicking an edge with the chosen line removes it; another click sets it again.
        d.toggle(INSIDE_V);
        assert_eq!(d.cur[INSIDE_V], Some(BorderLine::default()));
        d.toggle(DIAG_DOWN);
        assert_eq!(d.cur[DIAG_DOWN].unwrap().style, BorderStyle::Dotted);
        d.preset("none");
        assert_eq!(d.edges(), None, "back to how the cells were");
    }

    #[test]
    fn a_single_cell_has_no_inside_lines() {
        let mut d = draft(false, false);
        d.preset("inside");
        d.toggle(INSIDE_H);
        assert_eq!(d.edges(), None);
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 60.0));
        assert_eq!(edge_at(&d, r, pos2(50.0, 1.0)), Some(TOP));
        assert_eq!(edge_at(&d, r, pos2(99.0, 30.0)), Some(RIGHT));
        assert_eq!(edge_at(&d, r, pos2(25.0, 15.0)), Some(DIAG_DOWN));
        assert_eq!(edge_at(&d, r, pos2(25.0, 45.0)), Some(DIAG_UP));
        let d = draft(true, true);
        assert_eq!(edge_at(&d, r, pos2(20.0, 30.0)), Some(INSIDE_H));
        assert_eq!(edge_at(&d, r, pos2(50.0, 50.0)), Some(INSIDE_V));
    }

    #[test]
    fn the_selection_draft_reads_lines_shared_with_neighbours() {
        let mut s = gridcraft_engine::Session::new();
        s.new_workbook();
        s.run("home.borders", json!({"range": "B2:C3", "preset": "outside", "style": "medium"})).unwrap();
        s.run("home.borders", json!({"range": "B2", "preset": "top", "style": "double"})).unwrap();
        let doc = s.active().unwrap();
        let d = draft_for_selection(&doc.wb, gridcraft_engine::core::RangeRef::parse("B2:C3").unwrap());
        let medium = Some(BorderLine { style: BorderStyle::Medium, color: Color::Auto });
        assert_eq!(d.cur[LEFT], medium);
        assert_eq!(d.cur[BOTTOM], medium);
        assert_eq!(d.cur[TOP], None, "B2's top is double, C2's medium");
        assert_eq!(d.cur[INSIDE_H], Some(BorderLine::default()));
        assert!(d.rows && d.cols);
    }
}
