//! The buttons a PivotTable report shows on its cells, and the menu of its ▼ buttons, as in Excel:
//! the ▼ of 行ラベル / 列ラベル (each field header in the Outline and Tabular layouts) and of a
//! report filter's value, and the ＋/－ of the row items that have items below them.
//!
//! The ▼ menu sorts the field, opens ラベル フィルター / 値フィルター, clears the field's filter and
//! checks its items, all through `pivot.*` commands. The buttons come from `pivot.buttons`'
//! engine function, computed once per revision of the sheet.

use std::collections::HashSet;
use std::sync::Arc;

use egui::{Painter, Pos2, Rect, Stroke, StrokeKind, Ui, pos2, vec2};
use gridcraft_engine::core::CellRef;
use gridcraft_engine::model::Sheet;
use gridcraft_engine::pivot::{ButtonArea, PivotButton};
use serde_json::{Value as Json, json};

use crate::SheetApp;
use crate::grid::Geo;
use crate::theme::Tokens;

/// A button on the active sheet: its cell, its PivotTable and what it is.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetButton {
    pub cell: CellRef,
    pub pivot: String,
    pub button: PivotButton,
    /// For a field button: whether one of its fields is filtered (the ▼ shows a funnel).
    pub filtered: bool,
}

/// The buttons of the active sheet, cached per (doc uid, revision, sheet).
pub type ButtonsCache = ((u64, u64, usize), Arc<Vec<SheetButton>>);

/// The open ▼ menu: which PivotTable and fields, where, and the field shown.
#[derive(Clone, Debug, PartialEq)]
pub struct PivotMenu {
    pub pivot: String,
    pub area: ButtonArea,
    pub fields: Vec<String>,
    pub field: String,
    pub at: Pos2,
}

/// The active sheet's PivotTable buttons.
pub fn sheet_buttons(app: &mut SheetApp) -> Arc<Vec<SheetButton>> {
    let Some(d) = app.session.active() else { return Arc::default() };
    let si = d.wb.active_sheet();
    let key = (d.uid, d.revision, si);
    if let Some((k, list)) = &app.grid.pivot_buttons
        && *k == key
    {
        return list.clone();
    }
    let mut out = Vec::new();
    if let Some(sh) = d.wb.sheet(si) {
        for (pi, pt) in sh.pivots.iter().enumerate() {
            let Some((at, list)) = gridcraft_engine::pivot::report_buttons(&d.wb, si, pi) else { continue };
            for b in list {
                let (r, c) = b.cell();
                let filtered = match &b {
                    PivotButton::Field { fields, .. } => fields.iter().any(|f| field_filtered(pt, f)),
                    PivotButton::Expand { .. } => false,
                };
                out.push(SheetButton { cell: CellRef::new(at.row + r, at.col + c), pivot: pt.name.clone(), button: b, filtered });
            }
        }
    }
    let list = Arc::new(out);
    app.grid.pivot_buttons = Some((key, list.clone()));
    list
}

/// Whether the field captioned `name` has checked-off items or a Label or Value Filter.
fn field_filtered(pt: &gridcraft_engine::model::PivotTable, name: &str) -> bool {
    let axis = pt.rows.iter().chain(pt.columns.iter()).find(|f| f.source_col.eq_ignore_ascii_case(name));
    axis.is_some_and(|f| f.label_filter.is_some() || f.value_filter.is_some())
        || pt.filters.iter().any(|f| f.source_col.eq_ignore_ascii_case(name) && f.selected.is_some())
}

/// Cells whose label makes room for a ＋/－ button, for the cell painter (`label_offset`).
#[derive(Clone, Debug, Default)]
struct ToggleCells {
    sheet: usize,
    cells: HashSet<CellRef>,
}

fn toggle_cells_id() -> egui::Id {
    egui::Id::unique("gridcraft.pivot_toggle_cells")
}

/// Tells the cell painter which cells of sheet `si` carry a ＋/－ button this frame.
pub fn publish(ctx: &egui::Context, si: usize, list: &[SheetButton]) {
    let cells: HashSet<CellRef> = list.iter().filter(|b| matches!(b.button, PivotButton::Expand { .. })).map(|b| b.cell).collect();
    ctx.data_mut(|d| d.insert_temp(toggle_cells_id(), Arc::new(ToggleCells { sheet: si, cells })));
}

/// How far (points at 100% zoom) a cell's label moves right for its ＋/－ button: Excel draws the
/// button before the caption.
pub fn label_offset(ctx: &egui::Context, si: usize, c: CellRef) -> f32 {
    let t: Option<Arc<ToggleCells>> = ctx.data(|d| d.get_temp(toggle_cells_id()));
    if t.is_some_and(|t| t.sheet == si && t.cells.contains(&c)) { TOGGLE + 3.0 } else { 0.0 }
}

/// Size of a ＋/－ button at 100% zoom.
const TOGGLE: f32 = 9.0;

/// Where a button is drawn: a ▼ at the cell's right, a ＋/－ after the cell's indent.
pub fn button_rect(geo: &Geo, sh: &Sheet, b: &SheetButton, indent: u8) -> Rect {
    let r = geo.cell_rect(sh, b.cell);
    match b.button {
        PivotButton::Field { .. } => {
            let s = (r.height() - 4.0).clamp(10.0, 18.0).min(r.width() - 2.0).max(0.0);
            Rect::from_min_size(pos2(r.right() - s - 2.0, r.bottom() - s - 2.0), vec2(s, s))
        }
        PivotButton::Expand { .. } => {
            let s = (TOGGLE * geo.z).min(r.height() - 2.0).max(0.0);
            let x = r.left() + 2.5 * geo.z + indent as f32 * 9.0 * geo.z;
            Rect::from_min_size(pos2(x, r.center().y - s / 2.0), vec2(s, s))
        }
    }
}

fn indent_of(app: &SheetApp, sh: &Sheet, c: CellRef) -> u8 {
    app.session.active().map_or(0, |d| d.wb.styles.get(sh.style_id(c)).align.indent)
}

/// Paints the buttons of the active sheet over its cells.
pub fn paint(app: &mut SheetApp, p: &Painter, geo: &Geo, sh: &Sheet, si: usize) {
    let list = sheet_buttons(app);
    publish(p.ctx(), si, &list);
    let t = Tokens::get(p.ctx());
    for b in list.iter() {
        if geo.hidden_under_frozen(sh, b.cell) || sh.row_height(b.cell.row) <= 0.0 || sh.col_width(b.cell.col) <= 0.0 {
            continue;
        }
        let r = button_rect(geo, sh, b, indent_of(app, sh, b.cell));
        if !r.intersects(geo.cells) || r.width() <= 0.0 {
            continue;
        }
        p.rect_filled(r, 2.0, t.sheet_control_bg);
        p.rect_stroke(r, 2.0, Stroke::new(1.0, t.sheet_control_border), StrokeKind::Inside);
        match &b.button {
            PivotButton::Field { .. } => {
                let icon = if b.filtered { crate::icons::Icon::Filter } else { crate::icons::Icon::Chevron };
                crate::icons::paint_on_sheet(p, r.shrink(1.5), icon, t.sheet_control_text);
            }
            PivotButton::Expand { collapsed, .. } => {
                let c = r.center();
                let h = r.width() * 0.3;
                let stroke = Stroke::new(1.0, t.sheet_control_text);
                p.line_segment([pos2(c.x - h, c.y), pos2(c.x + h, c.y)], stroke);
                if *collapsed {
                    p.line_segment([pos2(c.x, c.y - h), pos2(c.x, c.y + h)], stroke);
                }
            }
        }
    }
}

/// The button under `p` (logical coordinates), if any.
pub fn hit(app: &mut SheetApp, geo: &Geo, sh: &Sheet, p: Pos2) -> Option<SheetButton> {
    let list = sheet_buttons(app);
    list.iter().find(|b| !geo.hidden_under_frozen(sh, b.cell) && button_rect(geo, sh, b, indent_of(app, sh, b.cell)).contains(p)).cloned()
}

/// A press on a button: a ▼ opens its menu at once; a ＋/－ acts when the press is a click.
pub fn press(app: &mut SheetApp, b: &SheetButton, at: Pos2, clicked: bool) {
    match &b.button {
        PivotButton::Field { area, fields, .. } => {
            let field = fields.first().cloned().unwrap_or_default();
            app.grid.pivot_menu = Some(PivotMenu { pivot: b.pivot.clone(), area: *area, fields: fields.clone(), field, at });
        }
        PivotButton::Expand { field, item, collapsed, .. } => {
            if clicked {
                app.run_or_alert("pivot.collapse", json!({"pivot": b.pivot, "field": field, "items": [item], "collapse": !collapsed}));
            }
        }
    }
}

/// The condition items of the Label Filters submenu: (label, op); "-" is a separator.
const LABEL_OPS: &[(&str, &str)] = &[
    ("PivotFilter|Equals...", "equal"),
    ("PivotFilter|Does Not Equal...", "notEqual"),
    ("-", ""),
    ("PivotFilter|Begins With...", "beginsWith"),
    ("PivotFilter|Does Not Begin With...", "notBeginsWith"),
    ("PivotFilter|Ends With...", "endsWith"),
    ("PivotFilter|Does Not End With...", "notEndsWith"),
    ("-", ""),
    ("PivotFilter|Contains...", "contains"),
    ("PivotFilter|Does Not Contain...", "notContains"),
    ("-", ""),
    ("PivotFilter|Greater Than...", "greaterThan"),
    ("PivotFilter|Greater Than Or Equal To...", "greaterThanOrEqual"),
    ("PivotFilter|Less Than...", "lessThan"),
    ("PivotFilter|Less Than Or Equal To...", "lessThanOrEqual"),
    ("PivotFilter|Between...", "between"),
    ("PivotFilter|Not Between...", "notBetween"),
];

/// The condition items of the Value Filters submenu.
const VALUE_OPS: &[(&str, &str)] = &[
    ("PivotFilter|Equals...", "equal"),
    ("PivotFilter|Does Not Equal...", "notEqual"),
    ("-", ""),
    ("PivotFilter|Greater Than...", "greaterThan"),
    ("PivotFilter|Greater Than Or Equal To...", "greaterThanOrEqual"),
    ("PivotFilter|Less Than...", "lessThan"),
    ("PivotFilter|Less Than Or Equal To...", "lessThanOrEqual"),
    ("PivotFilter|Between...", "between"),
    ("PivotFilter|Not Between...", "notBetween"),
    ("-", ""),
    ("PivotFilter|Top 10...", "top10"),
];

/// What a click in the menu asks for once the popup has drawn.
enum Pick {
    Run(&'static str, Json),
    Dialog(&'static str, Json),
}

/// Draws the open ▼ menu: field choice (compact layout), sort, Clear Filter From, Label and
/// Value Filters, and the item list with search, Select All, and OK / Cancel.
pub fn menu(app: &mut SheetApp, ui: &mut Ui, was_open: bool) {
    let Some(m) = app.grid.pivot_menu.clone() else { return };
    let info = match app.session.run("pivot.items", json!({"pivot": m.pivot, "field": m.field})) {
        Ok(v) => v,
        Err(_) => {
            app.grid.pivot_menu = None;
            return;
        }
    };
    let lang = app.ui.language;
    let key = egui::Id::unique(("pivot_menu_checks", m.pivot.as_str(), m.field.as_str()));
    let search_key = egui::Id::unique(("pivot_menu_search", m.pivot.as_str(), m.field.as_str()));
    let items: Vec<(String, bool)> = info["items"]
        .as_array()
        .map(|a| a.iter().map(|i| (i["label"].as_str().unwrap_or_default().to_string(), i["selected"].as_bool().unwrap_or(true))).collect())
        .unwrap_or_default();
    let report_filter = m.area == ButtonArea::Filters;
    let mut close = false;
    let mut pick: Option<Pick> = None;
    let mut field_change: Option<String> = None;
    let area = egui::Area::new(egui::Id::unique("pivot_menu")).fixed_pos(m.at + vec2(-200.0, 10.0)).order(egui::Order::Foreground);
    let response = area.show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(250.0);
            if m.fields.len() > 1 {
                ui.horizontal(|ui| {
                    ui.label(lang.tr("Select field:"));
                    craft_ui::ComboBox::from_id_salt("pivot_menu_field").selected_text(m.field.clone()).show_ui(ui, |ui| {
                        for f in &m.fields {
                            if ui.selectable_label(*f == m.field, f).clicked() {
                                field_change = Some(f.clone());
                            }
                        }
                    });
                });
                ui.separator();
            }
            if !report_filter {
                if ui.button(format!("↑  {}", lang.tr("Sort A to Z"))).clicked() {
                    pick = Some(Pick::Run("pivot.sort", json!({"pivot": m.pivot, "field": m.field, "order": "asc"})));
                }
                if ui.button(format!("↓  {}", lang.tr("Sort Z to A"))).clicked() {
                    pick = Some(Pick::Run("pivot.sort", json!({"pivot": m.pivot, "field": m.field, "order": "desc"})));
                }
                ui.separator();
            }
            let clear_label = lang.tr("Clear Filter From \"{field}\"").replace("{field}", &m.field);
            if ui.add_enabled(info["filtered"].as_bool().unwrap_or(false), egui::Button::new(clear_label)).clicked() {
                pick = Some(Pick::Run("pivot.clearFilter", json!({"pivot": m.pivot, "field": m.field})));
            }
            if !report_filter {
                let base = json!({"pivot": m.pivot, "field": m.field});
                ui.menu_button(lang.tr("Label Filters"), |ui| {
                    if ui.add_enabled(!info["labelFilter"].is_null(), egui::Button::new(lang.tr("Clear Filter"))).clicked() {
                        pick = Some(Pick::Run("pivot.labelFilter", json!({"pivot": m.pivot, "field": m.field, "clear": true})));
                    }
                    ui.separator();
                    for (label, op) in LABEL_OPS {
                        if *label == "-" {
                            ui.separator();
                        } else if ui.button(lang.tr(label)).clicked() {
                            let mut p = base.clone();
                            p["op"] = json!(op);
                            p["current"] = info["labelFilter"].clone();
                            pick = Some(Pick::Dialog("pivotLabelFilter", p));
                        }
                    }
                });
                ui.menu_button(lang.tr("Value Filters"), |ui| {
                    if ui.add_enabled(!info["valueFilter"].is_null(), egui::Button::new(lang.tr("Clear Filter"))).clicked() {
                        pick = Some(Pick::Run("pivot.valueFilter", json!({"pivot": m.pivot, "field": m.field, "clear": true})));
                    }
                    ui.separator();
                    for (label, op) in VALUE_OPS {
                        if *label == "-" {
                            ui.separator();
                        } else if ui
                            .add_enabled(info["valueFields"].as_array().is_some_and(|a| !a.is_empty()), egui::Button::new(lang.tr(label)))
                            .clicked()
                        {
                            let mut p = base.clone();
                            p["op"] = json!(op);
                            p["valueFields"] = info["valueFields"].clone();
                            p["current"] = info["valueFilter"].clone();
                            pick = Some(Pick::Dialog(if *op == "top10" { "pivotTop10" } else { "pivotValueFilter" }, p));
                        }
                    }
                });
            }
            ui.separator();
            let mut checks: Vec<(String, bool)> = ui.ctx().data_mut(|d| d.get_temp::<Vec<(String, bool)>>(key)).unwrap_or_else(|| items.clone());
            let mut search: String = ui.ctx().data_mut(|d| d.get_temp(search_key)).unwrap_or_default();
            let field = ui.add(egui::TextEdit::singleline(&mut search).hint_text(lang.tr("Search")).desired_width(f32::INFINITY));
            if !was_open {
                field.request_focus();
            }
            let needle = search.trim().to_lowercase();
            let matches = |v: &str| needle.is_empty() || v.to_lowercase().contains(&needle);
            let all = checks.iter().filter(|(v, _)| matches(v)).all(|(_, b)| *b);
            let mut all_new = all;
            let label = if needle.is_empty() { "(Select All)" } else { "(Select All Search Results)" };
            if ui.checkbox(&mut all_new, lang.tr(label)).changed() {
                for c in checks.iter_mut().filter(|(v, _)| matches(v)) {
                    c.1 = all_new;
                }
            }
            egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                for (v, on) in checks.iter_mut().filter(|(v, _)| matches(v)) {
                    // The blank item reads in the interface language, as Excel's (空白).
                    let shown = if v == "(blank)" { lang.tr("(blank)") } else { v.as_str() };
                    ui.checkbox(on, shown);
                }
            });
            ui.ctx().data_mut(|d| {
                d.insert_temp(key, checks.clone());
                d.insert_temp(search_key, search.clone());
            });
            let shown: Vec<(String, bool)> = checks.iter().map(|(v, on)| (v.clone(), *on && matches(v))).collect();
            ui.separator();
            ui.horizontal(|ui| {
                let any = shown.iter().any(|(_, on)| *on);
                if ui.add_enabled(any, egui::Button::new(format!("   {}   ", lang.tr("OK")))).clicked() {
                    let selected: Json = if shown.iter().all(|(_, on)| *on) {
                        Json::Null
                    } else {
                        json!(shown.iter().filter(|(_, on)| *on).map(|(v, _)| v.clone()).collect::<Vec<_>>())
                    };
                    pick = Some(Pick::Run("pivot.filter", json!({"pivot": m.pivot, "field": m.field, "selected": selected})));
                }
                if ui.button(lang.tr("Cancel")).clicked() {
                    close = true;
                }
            });
        });
    });
    if let Some(f) = field_change {
        ui.ctx().data_mut(|d| {
            d.remove::<Vec<(String, bool)>>(key);
            d.remove::<String>(search_key);
        });
        if let Some(menu) = app.grid.pivot_menu.as_mut() {
            menu.field = f;
        }
        return;
    }
    let layer = response.response.layer_id;
    let in_submenu = ui
        .input(|i| i.pointer.interact_pos())
        .and_then(|p| ui.ctx().layer_id_at(p))
        .is_some_and(|l| l != layer && l.order >= egui::Order::Foreground);
    let dismissed = crate::grid::popup_dismissed(&response.response, was_open) && !in_submenu;
    if let Some(p) = pick {
        close = true;
        match p {
            Pick::Run(id, params) => app.run_or_alert(id, params),
            Pick::Dialog(name, params) => app.open_dialog(name, params),
        }
    }
    if close || dismissed {
        ui.ctx().data_mut(|d| {
            d.remove::<Vec<(String, bool)>>(key);
            d.remove::<String>(search_key);
        });
        app.grid.pivot_menu = None;
    }
}
