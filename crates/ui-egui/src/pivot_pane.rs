//! The PivotTable Fields task pane: shown on the right when the active cell is in a pivot
//! report. Ticking a field adds it (text → Rows, numbers → Values); each area lists its fields
//! with move/remove menus. Everything runs `pivot.*` commands.

use egui::{Ui, vec2};
use serde_json::{Value as Json, json};

use crate::SheetApp;
use crate::i18n;
use crate::theme::{self, Tokens};

/// Whether the active cell is inside a pivot report (so the pane should show).
pub fn active_pivot(app: &SheetApp) -> Option<String> {
    let d = app.session.active()?;
    let sh = d.wb.active()?;
    let a = d.selection.active;
    sh.pivots.iter().find(|p| p.last_range.is_some_and(|r| r.contains(a)) || p.anchor == a).map(|p| p.name.clone())
}

/// Draws the PivotTable Fields pane: add/remove fields, move them between areas, sort/group/value settings, refresh, calculated field and report layout.
pub fn show(app: &mut SheetApp, ui: &mut Ui) {
    let Some(name) = active_pivot(app) else { return };
    // PivotTable Analyze › Field List is off.
    if app.grid.pivot_list_hidden {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let lang = i18n::current(ui.ctx());
    let Some(key) = app.session.active().map(|d| (d.uid, d.revision, name.clone())) else { return };
    if app.grid.pivot_fields.as_ref().is_none_or(|(k, _)| *k != key) {
        let info = match app.session.run("pivot.fields", json!({"pivot": name})) {
            Ok(v) => v,
            Err(_) => return,
        };
        app.grid.pivot_fields = Some((key, info));
    }
    let Some((_, info)) = app.grid.pivot_fields.clone() else { return };
    egui::Panel::right("pivot_pane").default_size(280.0).resizable(true).frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(10)).show(ui, |ui| {
        ui.label(egui::RichText::new(lang.tr("PivotTable Fields")).font(theme::ui_bold(15.0)));
        ui.label(egui::RichText::new(lang.tr("Choose fields to add to report:")).small().color(t.text_dim));
        ui.add_space(4.0);
        let fields = info["fields"].as_array().cloned().unwrap_or_default();
        egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).id_salt("pv_fields").show(ui, |ui| {
            for f in &fields {
                let fname = f["name"].as_str().unwrap_or("").to_string();
                let areas: Vec<String> =
                    f["areas"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                let mut on = !areas.is_empty();
                // Excel puts a ticked number field in Values, and text or dates in Rows.
                let numeric = f["numeric"].as_bool().unwrap_or(false) && !f["date"].as_bool().unwrap_or(false);
                let r = ui.checkbox(&mut on, egui::RichText::new(&fname).font(theme::ui_font(13.0)));
                if r.changed() {
                    let res = if on {
                        app.run("pivot.addField", json!({"pivot": name, "field": fname, "area": if numeric { "values" } else { "rows" }}))
                    } else {
                        app.run("pivot.removeField", json!({"pivot": name, "field": fname}))
                    };
                    if let Err(e) = res {
                        app.message = Some(("PivotTable".into(), crate::clean_error(&e)));
                    }
                }
                let samples: Vec<String> =
                    f["samples"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                r.on_hover_text(samples.join(", "));
            }
        });
        ui.separator();
        ui.label(egui::RichText::new(lang.tr("Use a field's menu to move it between areas:")).small().color(t.text_dim));
        let area = |key: &str| -> Vec<String> {
            info["pivot"][key]
                .as_array()
                .or(info[key].as_array())
                .map(|a| {
                    a.iter()
                        // The filters list also holds hidden items of Rows and Columns fields and
                        // slicers' filters: only report filters are in the Filters area.
                        .filter(|x| key != "filters" || x["report"].as_bool().unwrap_or(true))
                        .filter_map(|x| x["name"].as_str().or(x["field"].as_str()).or(x["sourceCol"].as_str()).or(x.as_str()).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        // Σ Values (two or more value fields) sits in Rows or Columns; it moves only between them.
        const VALUES_FIELD: &str = "Σ Values";
        let values_key = if info["valuesOnRows"].as_bool().unwrap_or(false) { "rows" } else { "columns" };
        let values_at = info["valuesPosition"].as_u64().map(|p| p as usize);
        let area_with_values = |key: &str| -> Vec<String> {
            let mut v = area(key);
            if key == values_key && area("values").len() > 1 {
                let at = values_at.unwrap_or(v.len()).min(v.len());
                v.insert(at, VALUES_FIELD.to_string());
            }
            v
        };
        // Dragging a field moves it within its area or to another, at the place it is dropped.
        let drag_id = egui::Id::unique("pivot_pane_drag");
        let dragging: Option<(String, String)> = ui.ctx().data(|d| d.get_temp(drag_id));
        let mut drop_areas: Vec<(&'static str, egui::Rect, Vec<egui::Rect>)> = Vec::new();
        ui.columns(2, |cols| {
            for (i, (key, label)) in [("filters", "Filters"), ("columns", "Columns"), ("rows", "Rows"), ("values", "Values")].iter().enumerate() {
                let ui = &mut cols[i % 2];
                ui.label(egui::RichText::new(lang.tr(label)).strong());
                let mut rects = Vec::new();
                let frame = egui::Frame::NONE.fill(t.window).corner_radius(4.0).inner_margin(4.0).show(ui, |ui| {
                    ui.set_min_size(vec2(ui.available_width(), 60.0));
                    let plain = area(key);
                    for f in area_with_values(key) {
                        let is_values = f == VALUES_FIELD;
                        let b = ui
                            .add(
                                egui::Button::new(egui::RichText::new(format!("{f} ▾")).font(theme::ui_font(12.0)))
                                    .fill(t.ribbon)
                                    .min_size(vec2(ui.available_width(), 20.0)),
                            )
                            .interact(egui::Sense::click_and_drag());
                        rects.push(b.rect);
                        if b.drag_started() {
                            ui.ctx().data_mut(|d| d.insert_temp(drag_id, (key.to_string(), f.clone())));
                        }
                        let at = plain.iter().position(|x| *x == f);
                        egui::Popup::menu(&b).show(|ui| {
                            if let Some(i) = at.filter(|_| !is_values) {
                                let last = plain.len().saturating_sub(1);
                                for (l, to, on) in [
                                    ("Move Up", i.saturating_sub(1), i > 0),
                                    ("Move Down", i + 1, i < last),
                                    ("Move to Beginning", 0, i > 0),
                                    ("Move to End", last, i < last),
                                ] {
                                    if ui.add_enabled(on, egui::Button::new(lang.tr(l))).clicked() {
                                        app.run_or_alert(
                                            "pivot.moveField",
                                            json!({"pivot": name, "field": f, "from": key, "to": key, "position": to}),
                                        );
                                    }
                                }
                                ui.separator();
                            }
                            for (to, l) in [
                                ("rows", "Move to Row Labels"),
                                ("columns", "Move to Column Labels"),
                                ("filters", "Move to Report Filter"),
                                ("values", "Move to Values"),
                            ] {
                                if to != *key && (!is_values || matches!(to, "rows" | "columns")) && ui.button(lang.tr(l)).clicked() {
                                    app.run_or_alert("pivot.moveField", json!({"pivot": name, "field": f, "from": key, "to": to}));
                                }
                            }
                            if is_values {
                                return;
                            }
                            if *key != "values" {
                                filter_menu(app, ui, &name, &f, &info);
                            }
                            if *key == "values" {
                                ui.separator();
                                for (func, l) in [
                                    ("sum", "Summarize by Sum"),
                                    ("count", "Summarize by Count"),
                                    ("average", "Summarize by Average"),
                                    ("max", "Summarize by Max"),
                                    ("min", "Summarize by Min"),
                                ] {
                                    if ui.button(lang.tr(l)).clicked() {
                                        app.run_or_alert("pivot.valueSettings", json!({"pivot": name, "field": f, "func": func}));
                                    }
                                }
                                for (sa, l) in [
                                    ("normal", "Show as No Calculation"),
                                    ("percentOfGrandTotal", "Show as % of Grand Total"),
                                    ("percentOfColumnTotal", "Show as % of Column Total"),
                                    ("percentOfRowTotal", "Show as % of Row Total"),
                                    ("runningTotal", "Show as Running Total"),
                                    ("rank", "Show as Rank"),
                                ] {
                                    if ui.button(lang.tr(l)).clicked() {
                                        app.run_or_alert("pivot.valueSettings", json!({"pivot": name, "field": f, "showAs": sa}));
                                    }
                                }
                                ui.separator();
                                // Only the field: the command opens the Value Field Settings dialog on it.
                                if ui.button(lang.tr("Value Field Settings…")).clicked() {
                                    app.run_or_alert("pivot.valueSettings", json!({"pivot": name, "field": f}));
                                }
                            } else {
                                ui.separator();
                                for (o, l) in [("asc", "Sort A to Z"), ("desc", "Sort Z to A")] {
                                    if ui.button(lang.tr(l)).clicked() {
                                        app.run_or_alert("pivot.sort", json!({"pivot": name, "field": f, "order": o}));
                                    }
                                }
                                // Only the field: the command opens the Grouping dialog on it.
                                if ui.button(lang.tr("PivotTable|Group…")).clicked() {
                                    app.run_or_alert("pivot.group", json!({"pivot": name, "field": f}));
                                }
                                for (g, l) in [
                                    ("years", "Group by Years"),
                                    ("quarters", "Group by Quarters"),
                                    ("months", "Group by Months"),
                                    ("none", "Ungroup"),
                                ] {
                                    if ui.button(lang.tr(l)).clicked() {
                                        app.run_or_alert("pivot.group", json!({"pivot": name, "field": f, "by": g}));
                                    }
                                }
                            }
                            ui.separator();
                            if ui.button(lang.tr("Remove Field")).clicked() {
                                app.run_or_alert("pivot.removeField", json!({"pivot": name, "field": f, "area": key}));
                            }
                        });
                    }
                });
                drop_areas.push((key, frame.response.rect, rects));
            }
        });
        if let Some((from, f)) = dragging {
            let pointer = ui.ctx().pointer_latest_pos();
            if let Some(p) = pointer {
                egui::Area::new(egui::Id::unique("pivot_pane_ghost")).fixed_pos(p + vec2(8.0, 4.0)).order(egui::Order::Tooltip).show(
                    ui.ctx(),
                    |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(f.as_str()));
                    },
                );
            }
            if ui.ctx().input(|i| i.pointer.any_released()) {
                ui.ctx().data_mut(|d| d.remove::<(String, String)>(drag_id));
                let target = pointer.and_then(|p| {
                    drop_areas.iter().find(|(_, r, _)| r.contains(p)).map(|(k, _, rects)| (*k, rects.iter().filter(|r| r.center().y < p.y).count()))
                });
                if let Some((to, mut position)) = target {
                    // The slots counted include the field itself while it is still in place.
                    if to == from
                        && drop_areas.iter().find(|(k, _, _)| *k == from).is_some_and(|(_, _, rects)| {
                            rects.iter().filter(|r| pointer.is_some_and(|p| r.center().y < p.y)).count()
                                > area(&from).iter().position(|x| *x == f).unwrap_or(usize::MAX)
                        })
                    {
                        position = position.saturating_sub(1);
                    }
                    app.run_or_alert("pivot.moveField", json!({"pivot": name, "field": f, "from": from, "to": to, "position": position}));
                }
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(lang.tr("Refresh")).clicked() {
                app.run_or_alert("pivot.refresh", json!({"pivot": name}));
            }
            if ui.button(lang.tr("Calculated Field…")).clicked() {
                app.run_or_alert("pivot.calculatedField", json!({"pivot": name}));
            }
            craft_ui::ComboBox::from_id_salt("pv_layout")
                .selected_text(lang.tr("Report Layout"))
                .show_ui(ui, |ui| {
                    for (l, v) in [("Compact", "compact"), ("Outline Form", "outline"), ("Tabular", "tabular")] {
                        if ui.button(lang.tr(l)).clicked() {
                            app.run_or_alert("pivot.layout", json!({"pivot": name, "layout": v}));
                        }
                    }
                })
                .response
                .accessible_name(lang.tr("Report Layout"));
        });
        let _: Option<Json> = None;
    });
}

/// A row, column or filter field's item list, as Excel's field dropdown: (Select All) and a
/// checkbox per item, applied with `pivot.filter` at once.
pub(crate) fn filter_menu(app: &mut SheetApp, ui: &mut Ui, pivot: &str, field: &str, info: &Json) {
    let lang = i18n::current(ui.ctx());
    let items: Vec<String> = info["fields"]
        .as_array()
        .and_then(|fs| fs.iter().find(|f| f["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(field))))
        .and_then(|f| f["items"].as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    if items.is_empty() {
        return;
    }
    let selected: Option<Vec<String>> = info["pivot"]["filters"]
        .as_array()
        .and_then(|fs| fs.iter().find(|f| f["field"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(field))))
        .and_then(|f| f["selected"].as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect());
    let shown = |it: &str| selected.as_ref().is_none_or(|s| s.iter().any(|x| x.eq_ignore_ascii_case(it)));
    ui.separator();
    ui.menu_button(lang.tr("Filter"), |ui| {
        let mut all = selected.is_none();
        if ui.checkbox(&mut all, lang.tr("(Select All)")).changed() && all {
            app.run_or_alert("pivot.filter", json!({"pivot": pivot, "field": field, "selected": null}));
        }
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for it in &items {
                let mut on = shown(it);
                if ui.checkbox(&mut on, it.as_str()).changed() {
                    let mut keep: Vec<&String> = items.iter().filter(|x| shown(x) && *x != it).collect();
                    if on {
                        keep.push(it);
                    }
                    // Every item ticked again is no filter; none ticked is refused, as Excel's OK greys out.
                    if keep.len() == items.len() {
                        app.run_or_alert("pivot.filter", json!({"pivot": pivot, "field": field, "selected": null}));
                    } else if !keep.is_empty() {
                        app.run_or_alert("pivot.filter", json!({"pivot": pivot, "field": field, "selected": keep}));
                    }
                }
            }
        });
    });
}
