//! The PivotTable contextual tabs' groups: PivotTable Analyze (ピボットテーブル分析, whose tab and
//! PivotChart, Expand/Collapse and Refresh buttons live in `contextual.rs`) and Design (デザイン):
//! Subtotals, Grand Totals, Report Layout, Blank Rows and the PivotTable Styles gallery.

use super::controls::{act, sep};
use super::{Color32, Icon, Sense, SheetApp, Stroke, StrokeKind, Tokens, Ui, big_button, json, vec2};
use crate::widgets::{small_button, small_toggle_button};

/// The active PivotTable's name and on/off options for the ribbon.
struct Active {
    name: String,
    headers: bool,
    buttons: bool,
    style: String,
}

fn active(app: &SheetApp) -> Option<Active> {
    let name = crate::pivot_pane::active_pivot(app)?;
    let d = app.session.active()?;
    let pt = d.wb.active()?.pivots.iter().find(|p| p.name == name)?;
    Some(Active { name, headers: pt.show_headers, buttons: pt.show_expand_buttons, style: pt.style.clone() })
}

/// PivotTable Analyze › PivotTable and Active Field: the PivotTable's name, the active field and
/// Field Settings.
pub(super) fn analyze_head(app: &mut SheetApp, ui: &mut Ui) {
    let Some(a) = active(app) else { return };
    let lang = app.ui.language;
    let field = app.session.run("pivot.activeField", json!({"pivot": a.name})).ok().and_then(|v| v["field"].as_str().map(str::to_string));
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(lang.tr("PivotTable Name:")).small());
        ui.label(egui::RichText::new(&a.name).strong());
    });
    sep(ui);
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(lang.tr("Active Field:")).small());
        ui.label(egui::RichText::new(field.clone().unwrap_or_default()).strong());
        if small_button(ui, Icon::Settings, "Field Settings", "Field Settings", false).clicked() {
            act(app, "pivot.fieldSettings", json!({"pivot": a.name}));
        }
    });
}

/// PivotTable Analyze › Group and Filter: Group Selection, Ungroup, Insert Slicer.
pub(super) fn analyze_group_filter(app: &mut SheetApp, ui: &mut Ui) {
    let Some(a) = active(app) else { return };
    ui.vertical(|ui| {
        if small_button(ui, Icon::Group, "Group Selection", "Group Selection", false).clicked() {
            act(app, "pivot.group", json!({"pivot": a.name}));
        }
        if small_button(ui, Icon::Ungroup, "PivotTable|Ungroup", "PivotTable|Ungroup", false).clicked() {
            act(app, "pivot.group", json!({"pivot": a.name, "by": "none"}));
        }
    });
    sep(ui);
    if big_button(ui, Icon::Filter, "Insert\nSlicer", "Insert Slicer", false).clicked() {
        act(app, "pivot.insertSlicer", json!({"pivot": a.name}));
    }
    sep(ui);
}

/// PivotTable Analyze › Data (beside Refresh), Actions, Calculations.
pub(super) fn analyze_data_actions(app: &mut SheetApp, ui: &mut Ui) {
    let Some(a) = active(app) else { return };
    let lang = app.ui.language;
    if big_button(ui, Icon::Table, "Change Data\nSource", "Change Data Source", false).clicked() {
        app.open_dialog("pivotChangeSource", json!({"pivot": a.name}));
    }
    sep(ui);
    let clear = big_button(ui, Icon::Eraser, "Clear", "Clear", true);
    egui::Popup::menu(&clear).show(|ui| {
        if ui.button(lang.tr("Clear All")).clicked() {
            act(app, "pivot.clearAll", json!({"pivot": a.name}));
        }
        if ui.button(lang.tr("Clear Filters")).clicked() {
            act(app, "pivot.clearFilter", json!({"pivot": a.name}));
        }
    });
    sep(ui);
    let calc = big_button(ui, Icon::Function, "Fields, Items,\n& Sets", "Fields, Items, & Sets", true);
    egui::Popup::menu(&calc).show(|ui| {
        if ui.button(lang.tr("Calculated Field...")).clicked() {
            act(app, "pivot.calculatedField", json!({"pivot": a.name}));
        }
    });
    sep(ui);
}

/// PivotTable Analyze › Show: Field List, +/- Buttons, Field Headers.
pub(super) fn analyze_show(app: &mut SheetApp, ui: &mut Ui) {
    let Some(a) = active(app) else { return };
    sep(ui);
    ui.vertical(|ui| {
        let list = !app.grid.pivot_list_hidden;
        if small_toggle_button(ui, Icon::Table, "Field List", "Field List", list).clicked() {
            app.grid.pivot_list_hidden = list;
        }
        if small_toggle_button(ui, Icon::Plus, "+/- Buttons", "+/- Buttons", a.buttons).clicked() {
            act(app, "pivot.expandButtons", json!({"pivot": a.name, "on": !a.buttons}));
        }
        if small_toggle_button(ui, Icon::Filter, "Field Headers", "Field Headers", a.headers).clicked() {
            act(app, "pivot.fieldHeaders", json!({"pivot": a.name, "on": !a.headers}));
        }
    });
}

/// The Design tab (デザイン) of a PivotTable.
pub(crate) fn design(app: &mut SheetApp, ui: &mut Ui) {
    let Some(a) = active(app) else { return };
    let name = a.name.clone();
    let menu = |app: &mut SheetApp, ui: &mut Ui, icon: Icon, label: &str, items: &[(&str, &str, serde_json::Value)]| {
        let b = big_button(ui, icon, label, &label.replace('\n', " "), true);
        egui::Popup::menu(&b).show(|ui| super::controls::menu_items(app, ui, items));
    };
    menu(
        app,
        ui,
        Icon::Subtotal,
        "Subtotals",
        &[
            ("Do Not Show Subtotals", "pivot.subtotals", json!({"pivot": name, "mode": "none"})),
            ("Show all Subtotals at Bottom of Group", "pivot.subtotals", json!({"pivot": name, "mode": "bottom"})),
            ("Show all Subtotals at Top of Group", "pivot.subtotals", json!({"pivot": name, "mode": "top"})),
        ],
    );
    menu(
        app,
        ui,
        Icon::Sum,
        "Grand\nTotals",
        &[
            ("Off for Rows and Columns", "pivot.grandTotals", json!({"pivot": name, "mode": "off"})),
            ("On for Rows and Columns", "pivot.grandTotals", json!({"pivot": name, "mode": "on"})),
            ("On for Rows Only", "pivot.grandTotals", json!({"pivot": name, "mode": "rows"})),
            ("On for Columns Only", "pivot.grandTotals", json!({"pivot": name, "mode": "columns"})),
        ],
    );
    menu(
        app,
        ui,
        Icon::Table,
        "Report\nLayout",
        &[
            ("Show in Compact Form", "pivot.layout", json!({"pivot": name, "layout": "compact"})),
            ("Show in Outline Form", "pivot.layout", json!({"pivot": name, "layout": "outline"})),
            ("Show in Tabular Form", "pivot.layout", json!({"pivot": name, "layout": "tabular"})),
            ("-", "", json!(null)),
            ("Repeat All Item Labels", "pivot.repeatLabels", json!({"pivot": name, "on": true})),
            ("Do Not Repeat Item Labels", "pivot.repeatLabels", json!({"pivot": name, "on": false})),
        ],
    );
    menu(
        app,
        ui,
        Icon::Insert,
        "Blank\nRows",
        &[
            ("Insert Blank Line after Each Item", "pivot.blankRows", json!({"pivot": name, "on": true})),
            ("Remove Blank Line after Each Item", "pivot.blankRows", json!({"pivot": name, "on": false})),
        ],
    );
    sep(ui);
    style_strip(app, ui, &name, &a.style);
}

/// The first row of the PivotTable Styles gallery in the ribbon; the More arrow opens all of it
/// (Light, Medium, Dark).
fn style_strip(app: &mut SheetApp, ui: &mut Ui, pivot: &str, current: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let mut clicked = None;
        for n in 1..=7 {
            let name = format!("PivotStyleLight{n}");
            if swatch(app, ui, &name, current, vec2(52.0, 40.0)) {
                clicked = Some(name);
            }
        }
        let more = super::icon_button(ui, Icon::Chevron, t.text_dim, "More PivotTable Styles", vec2(18.0, 40.0));
        egui::Popup::menu(&more).show(|ui| {
            ui.set_width(430.0);
            let lang = app.ui.language;
            for (fam, n) in [("Light", 28u32), ("Medium", 28), ("Dark", 28)] {
                ui.label(egui::RichText::new(lang.tr(&format!("Table Styles|{fam}"))).strong());
                egui::Grid::new(("pivot_styles", fam)).spacing(vec2(4.0, 4.0)).show(ui, |ui| {
                    for i in 1..=n {
                        let name = format!("PivotStyle{fam}{i}");
                        if swatch(app, ui, &name, current, vec2(54.0, 38.0)) {
                            clicked = Some(name);
                            ui.close();
                        }
                        if i % 7 == 0 {
                            ui.end_row();
                        }
                    }
                });
            }
        });
        if let Some(style) = clicked {
            act(app, "pivot.style", json!({"pivot": pivot, "style": style}));
        }
    });
}

/// One PivotTable style drawn as a small report (header row, rows, total row); true when clicked.
fn swatch(app: &SheetApp, ui: &mut Ui, name: &str, current: &str, size: egui::Vec2) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let Some(d) = app.session.active() else { return false };
    let lk = gridcraft_engine::pivot::look(name);
    let res = |c: gridcraft_engine::model::Color| c.resolve(&d.wb.theme).map(crate::theme::color32);
    let p = ui.painter();
    p.rect_filled(r, 0.0, t.grid_bg);
    let rows = 5;
    let rh = r.height() / rows as f32;
    for i in 0..rows {
        let rr = egui::Rect::from_min_size(egui::pos2(r.left(), r.top() + i as f32 * rh), vec2(r.width(), rh));
        if i == 0
            && let Some(f) = res(lk.header_fill)
        {
            p.rect_filled(rr, 0.0, f);
        }
        if (i == 0 || i == rows - 1)
            && let Some(l) = res(lk.line)
        {
            let ends = if i == 0 { [rr.left_bottom(), rr.right_bottom()] } else { [rr.left_top(), rr.right_top()] };
            p.line_segment(ends, Stroke::new(1.0, l));
        }
        let text = if i == 0 { res(lk.header_font).unwrap_or(t.cell_text) } else { t.sheet_control_border };
        p.line_segment([egui::pos2(rr.left() + 4.0, rr.center().y), egui::pos2(rr.left() + 18.0, rr.center().y)], Stroke::new(1.0, text));
        p.line_segment([egui::pos2(rr.right() - 14.0, rr.center().y), egui::pos2(rr.right() - 4.0, rr.center().y)], Stroke::new(1.0, text));
    }
    p.rect_stroke(r, 0.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
    if name.eq_ignore_ascii_case(current) {
        p.rect_stroke(r.expand(2.5), 2.0, Stroke::new(2.0, t.accent), StrokeKind::Outside);
    }
    if resp.hovered() {
        p.rect_stroke(r.expand(1.0), 2.0, Stroke::new(2.0, Color32::from_rgb(0xE8, 0x7D, 0x2B)), StrokeKind::Outside);
    }
    resp.on_hover_text(app.ui.language.style_name(&d.wb.theme, name)).clicked()
}
