//! Right-hand task panes: Comments, Watch Window, Selection Pane, Format Chart, Workbook Links,
//! Alt Text.

use egui::{Color32, Ui, vec2};
use serde_json::{Value as Json, json};

use gridcraft_engine::model::{ObjKind, ObjRef};

use crate::SheetApp;
use crate::theme::{self, Tokens};

pub fn show(app: &mut SheetApp, ui: &mut Ui) {
    // The Comments pane is engine state (`review.showComments`, so agents can drive it); the other
    // panes are UI state. One pane shows at a time: opening another one closes Comments.
    if app.grid.pane.as_deref() == Some("comments") {
        app.grid.pane = None;
        app.run_or_alert("review.showComments", json!({"on": true}));
    } else if app.grid.pane.is_some() && app.session.comments_pane {
        app.run_or_alert("review.showComments", json!({"on": false}));
    }
    let Some(pane) = app.grid.pane.clone().or_else(|| app.session.comments_pane.then(|| "comments".to_string())) else { return };
    let t = Tokens::get(ui.ctx());
    // Capped at half the window so a pane can never cover the sheet, whatever its content asks for.
    let max = (ui.available_width() / 2.0).max(300.0);
    let panel = egui::Panel::right("task_pane").default_size(300.0).max_size(max).resizable(true);
    panel.frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(10)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let title = match pane.as_str() {
                "comments" => "Comments",
                "workbookLinks" => "Workbook Links",
                "watch" => "Watch Window",
                "selection" => "Selection",
                "formatChart" => "Format Chart Area",
                "altText" => "Alt Text",
                _ => "",
            };
            ui.label(egui::RichText::new(app.ui.language.tr(title)).font(theme::ui_bold(15.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::widgets::icon_button(ui, crate::icons::Icon::Close, t.text_dim, "Close", vec2(22.0, 22.0)).clicked() {
                    if pane == "comments" {
                        app.run_or_alert("review.showComments", json!({"on": false}));
                    }
                    app.grid.pane = None;
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| match pane.as_str() {
            "comments" => comments(app, ui),
            "workbookLinks" => crate::workbook_links_pane::body(app, ui),
            "watch" => watch(app, ui),
            "selection" => selection(app, ui),
            "formatChart" => format_chart(app, ui),
            "altText" => alt_text(app, ui),
            _ => {}
        });
    });
}

/// Lists comments and replies, links each to its cell, and runs delete/resolve/reopen/reply commands.
fn comments(app: &mut SheetApp, ui: &mut Ui) {
    let Some(d) = app.session.active() else { return };
    let Some(sh) = d.wb.active() else { return };
    let list: Vec<(String, gridcraft_engine::model::Comment)> = sh.comments.iter().map(|(c, m)| (c.a1(), m.clone())).collect();
    if list.is_empty() {
        ui.label(egui::RichText::new(app.ui.language.tr("No comments on this sheet yet. Select a cell and choose New Comment.")).italics());
    }
    let t = crate::theme::Tokens::get(ui.ctx());
    for (cell, c) in list {
        egui::Frame::NONE
            .fill(if c.resolved { t.window } else { t.menu_bg })
            .corner_radius(6.0)
            .inner_margin(8.0)
            .stroke(egui::Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    if ui.link(egui::RichText::new(&cell).strong()).clicked() {
                        let _ = app.run("selection.set", json!({"cell": cell}));
                        app.grid.ensure_visible = true;
                    }
                    ui.label(egui::RichText::new(&c.author).color(t.text_dim));
                    if c.resolved {
                        ui.label(egui::RichText::new(app.ui.language.tr("Resolved")).small().color(t.accent));
                    }
                });
                ui.label(&c.text);
                for (a, txt) in &c.replies {
                    ui.label(egui::RichText::new(format!("↳ {a}: {txt}")).color(t.text));
                }
                let key = egui::Id::unique(("reply", &cell));
                let mut draft: String = ui.ctx().data_mut(|m| m.get_temp(key)).unwrap_or_default();
                // Buttons first, right to left, so the reply field gets exactly the width they leave:
                // a field sized by a guess wider than the row pushes the resizable pane wider every frame.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(app.ui.language.tr("Delete")).clicked() {
                            app.run_or_alert("review.deleteComment", json!({"cell": cell}));
                        }
                        if ui.small_button(app.ui.language.tr(if c.resolved { "Reopen" } else { "Resolve" })).clicked() {
                            app.run_or_alert("review.resolveComment", json!({"cell": cell, "resolved": !c.resolved}));
                        }
                        let send = ui.small_button(app.ui.language.tr("Send")).clicked();
                        let r = ui
                            .add(egui::TextEdit::singleline(&mut draft).hint_text(app.ui.language.tr("Reply…")).desired_width(ui.available_width()));
                        if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) || send) && !draft.is_empty() {
                            app.run_or_alert("review.replyComment", json!({"cell": cell, "text": draft}));
                            draft.clear();
                        }
                    });
                });
                ui.ctx().data_mut(|m| m.insert_temp(key, draft));
            });
        ui.add_space(6.0);
    }
}

/// Adds the active cell, lists watch values and formulas, and removes watched entries.
fn watch(app: &mut SheetApp, ui: &mut Ui) {
    if ui.button(app.ui.language.tr("Add Watch (active cell)")).clicked()
        && let Some(d) = app.session.active()
        && let Some(sh) = d.wb.active()
    {
        let r = format!("{}!{}", gridcraft_engine::formula::quote_sheet(&sh.name), d.selection.active.a1());
        let _ = app.session.run("formulas.watchWindow", json!({"add": r}));
    }
    let list = app.session.run("formulas.watchWindow", json!({})).unwrap_or(Json::Null);
    egui::Grid::new("watch_grid").striped(true).num_columns(4).show(ui, |ui| {
        let lang = crate::i18n::current(ui.ctx());
        ui.strong(lang.tr("Cell"));
        ui.strong(lang.tr("Value"));
        ui.strong(lang.tr("Formula"));
        ui.label("");
        ui.end_row();
        for w in list.as_array().cloned().unwrap_or_default() {
            let cell = w["cell"].as_str().unwrap_or("").to_string();
            ui.label(&cell);
            ui.label(w["value"].as_str().unwrap_or(""));
            ui.label(w["formula"].as_str().unwrap_or(""));
            if crate::widgets::icon_button(ui, crate::icons::Icon::Close, Tokens::get(ui.ctx()).text_dim, "Delete", vec2(18.0, 18.0)).clicked() {
                let _ = app.session.run("formulas.watchWindow", json!({"remove": cell}));
            }
            ui.end_row();
        }
    });
}

/// Lists the sheet's objects and dispatches select, bring-forward, send-backward and delete.
fn selection(app: &mut SheetApp, ui: &mut Ui) {
    let list = app.session.run("arrange.selectionPane", json!({})).unwrap_or(Json::Null);
    let items = list.as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        ui.label(egui::RichText::new(app.ui.language.tr("There are no shapes, pictures or charts on this sheet.")).italics());
    }
    for it in items.iter().rev() {
        let kind = it["kind"].as_str().unwrap_or("").to_string();
        let id = it["id"].as_u64().unwrap_or(0) as u32;
        let obj = ObjKind::parse(&kind).map(|k| ObjRef::new(k, id));
        ui.horizontal(|ui| {
            let sel = obj.is_some() && app.selected_object == obj;
            if ui.selectable_label(sel, it["name"].as_str().unwrap_or("")).clicked() {
                app.selected_object = obj;
            }
            if ui.small_button("▲").on_hover_text(app.ui.language.tr("Bring Forward")).clicked() {
                app.run_or_alert("arrange.bringForward", json!({"kind": kind, "id": id}));
            }
            if ui.small_button("▼").on_hover_text(app.ui.language.tr("Send Backward")).clicked() {
                app.run_or_alert("arrange.sendBackward", json!({"kind": kind, "id": id}));
            }
            if crate::widgets::icon_button(ui, crate::icons::Icon::Close, Tokens::get(ui.ctx()).text_dim, "Delete", vec2(18.0, 18.0)).clicked() {
                app.run_or_alert("object.delete", json!({"kind": kind, "id": id}));
            }
        });
    }
}

/// Draws the chart format pane: title, type, legend, elements/axis titles, series colors, palettes/layouts and size.
fn format_chart(app: &mut SheetApp, ui: &mut Ui) {
    let Some(id) = app.selected_chart() else {
        ui.label(egui::RichText::new(app.ui.language.tr("Select a chart to format it.")).italics());
        return;
    };
    let Some(chart) = app.session.active().and_then(|d| d.wb.active().and_then(|s| s.charts.iter().find(|c| c.id == id).cloned())) else {
        ui.label(app.ui.language.tr("The chart no longer exists."));
        return;
    };
    let theme = app.session.active().map(|d| d.wb.theme.clone()).unwrap_or_default();
    let lang = crate::i18n::current(ui.ctx());
    ui.collapsing(lang.tr("Chart Title"), |ui| {
        let field = egui::Id::unique(("gridcraft.chart_title", id));
        // An automatic title shows what it reads now (the series name, or グラフ タイトル);
        // typing replaces it with text.
        let shown = app
            .session
            .active()
            .and_then(|d| gridcraft_chart::title_text(&chart, &crate::chartview::resolve(ui.ctx(), &d.wb, d.wb.active_sheet(), &chart)))
            .unwrap_or_default();
        if let Some(title) = crate::widgets::committed_text(ui, field, &shown, None)
            && title != shown
        {
            app.run_or_alert("chart.set", json!({"chart": id, "title": title}));
        }
    });
    ui.collapsing(lang.tr("Chart Type"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for (l, k, s) in [
                ("Column", "column", ""),
                ("Stacked Column", "column", "stacked"),
                ("100% Stacked Column", "column", "stacked100"),
                ("Bar", "bar", ""),
                ("Stacked Bar", "bar", "stacked"),
                ("100% Stacked Bar", "bar", "stacked100"),
                ("Line", "line", "markers"),
                ("Stacked Line", "line", "stacked"),
                ("Area", "area", ""),
                ("Stacked Area", "area", "stacked"),
                ("100% Stacked Area", "area", "stacked100"),
                ("Pie", "pie", ""),
                ("Doughnut", "doughnut", ""),
                ("Scatter", "scatter", ""),
                ("Radar", "radar", ""),
            ] {
                let cur = chart.kind == gridcraft_engine::cmd::insert::chart_kind(k, s);
                if ui.selectable_label(cur, lang.tr(l)).clicked() {
                    app.run_or_alert("chart.set", json!({"chart": id, "type": k, "subtype": s}));
                }
            }
        });
        if matches!(chart.kind, gridcraft_chart::ChartKind::Pie | gridcraft_chart::ChartKind::Doughnut) {
            let draft = egui::Id::unique(("gridcraft.chart_first_slice_draft", id));
            let current = u32::from(chart.first_slice_angle);
            if let Some(angle) = chart_slider(ui, draft, current, 0..=359, lang.tr("First slice angle")) {
                app.run_or_alert("chart.set", json!({"chart": id, "firstSliceAngle": angle}));
            }
        }
        if chart.kind == gridcraft_chart::ChartKind::Doughnut {
            let draft = egui::Id::unique(("gridcraft.chart_hole_size_draft", id));
            let current = u32::from(chart.hole_size);
            if let Some(hole) = chart_slider(ui, draft, current, 10..=90, lang.tr("Doughnut hole size")) {
                app.run_or_alert("chart.set", json!({"chart": id, "holeSize": hole}));
            }
        }
    });
    if chart.kind == gridcraft_chart::ChartKind::Histogram {
        // Format Axis ▸ Axis Options ▸ Bins, as Excel's: Automatic, Bin width or Number of bins,
        // and the Overflow and Underflow bins.
        ui.collapsing(lang.tr("Format Axis"), |ui| {
            let b = chart.bins.unwrap_or_default();
            ui.label(egui::RichText::new(lang.tr("Bins")).strong());
            if ui.radio(b.width.is_none() && b.count.is_none(), lang.tr("Automatic")).clicked() {
                app.run_or_alert("chart.set", json!({"chart": id, "binWidth": null, "binCount": null}));
            }
            for (label, key, on, value) in
                [("Bin width", "binWidth", b.width.is_some(), b.width), ("Number of bins", "binCount", b.count.is_some(), b.count.map(f64::from))]
            {
                ui.horizontal(|ui| {
                    // Typing a value picks this choice, as in Excel.
                    let _ = ui.radio(on, lang.tr(label));
                    let field = egui::Id::unique(("gridcraft.histogram_bins", id, key));
                    let shown = value.map(|v| v.to_string()).unwrap_or_default();
                    if let Some(t) = crate::widgets::committed_text(ui, field, &shown, Some(70.0))
                        && let Ok(v) = t.trim().parse::<f64>()
                    {
                        app.run_or_alert("chart.set", json!({"chart": id, key: v}));
                    }
                });
            }
            for (label, key, value) in [("Overflow bin", "overflow", b.overflow), ("Underflow bin", "underflow", b.underflow)] {
                ui.horizontal(|ui| {
                    let mut on = value.is_some();
                    let field = egui::Id::unique(("gridcraft.histogram_cut", id, key));
                    let shown = value.map(|v| v.to_string()).unwrap_or_default();
                    let typed = crate::widgets::committed_text(ui, field, &shown, Some(70.0));
                    if ui.checkbox(&mut on, lang.tr(label)).changed() && !on {
                        app.run_or_alert("chart.set", json!({"chart": id, key: null}));
                    } else if let Some(t) = typed
                        && let Ok(v) = t.trim().parse::<f64>()
                    {
                        app.run_or_alert("chart.set", json!({"chart": id, key: v}));
                    }
                });
            }
        });
    }
    if chart.kind == gridcraft_chart::ChartKind::BoxWhisker {
        // Format Data Series ▸ Series Options ▸ Quartile Calculation.
        ui.collapsing(lang.tr("Quartile Calculation"), |ui| {
            let exclusive = chart.series.first().is_none_or(|s| s.quartiles == gridcraft_engine::model::QuartileMethod::Exclusive);
            if ui.radio(!exclusive, lang.tr("Inclusive median")).clicked() {
                app.run_or_alert("chart.set", json!({"chart": id, "quartiles": "inclusive"}));
            }
            if ui.radio(exclusive, lang.tr("Exclusive median")).clicked() {
                app.run_or_alert("chart.set", json!({"chart": id, "quartiles": "exclusive"}));
            }
        });
    }
    ui.collapsing(lang.tr("Legend"), |ui| {
        ui.horizontal(|ui| {
            for (l, v) in [("None", "none"), ("Bottom", "bottom"), ("Top", "top"), ("Left", "left"), ("Right", "right")] {
                let cur = format!("{:?}", chart.legend).eq_ignore_ascii_case(v);
                if ui.selectable_label(cur, lang.tr(l)).clicked() {
                    app.run_or_alert("chart.set", json!({"chart": id, "legend": v}));
                }
            }
        });
    });
    ui.collapsing(lang.tr("Elements"), |ui| {
        let mut dl = chart.data_labels;
        if ui.checkbox(&mut dl, lang.tr("Data Labels")).changed() {
            app.run_or_alert("chart.set", json!({"chart": id, "dataLabels": dl}));
        }
        // Format Data Labels ▸ Label Contains.
        if chart.data_labels {
            let pie = matches!(chart.kind, gridcraft_chart::ChartKind::Pie | gridcraft_chart::ChartKind::Doughnut);
            let parts = chart.label_parts;
            ui.indent("gridcraft.label_contains", |ui| {
                ui.label(lang.tr("Label Contains"));
                let mut items = vec![
                    ("Series Name", "seriesName", parts.series_name),
                    ("Category Name", "categoryName", parts.category_name),
                    ("Value", "value", parts.value),
                ];
                if pie {
                    items.push(("Percentage", "percentage", parts.percentage));
                    items.push(("Show Leader Lines", "leaderLines", parts.leader_lines));
                }
                for (label, key, mut on) in items {
                    if ui.checkbox(&mut on, lang.tr(label)).changed() {
                        app.run_or_alert("chart.formatDataLabels", json!({"chart": id, key: on}));
                    }
                }
            });
        }
        let mut gl = chart.gridlines;
        if ui.checkbox(&mut gl, lang.tr("Gridlines")).changed() {
            app.run_or_alert("chart.set", json!({"chart": id, "gridlines": gl}));
        }
        ui.horizontal(|ui| {
            ui.label(lang.tr("Horizontal axis title:"));
            let field = egui::Id::unique(("gridcraft.chart_x_title", id));
            if let Some(xt) = crate::widgets::committed_text(ui, field, chart.x_title.as_deref().unwrap_or(""), None) {
                app.run_or_alert("chart.set", json!({"chart": id, "xTitle": if xt.is_empty() { Json::Null } else { json!(xt) }}));
            }
        });
        ui.horizontal(|ui| {
            ui.label(lang.tr("Vertical axis title:"));
            let field = egui::Id::unique(("gridcraft.chart_y_title", id));
            if let Some(yt) = crate::widgets::committed_text(ui, field, chart.y_title.as_deref().unwrap_or(""), None) {
                app.run_or_alert("chart.set", json!({"chart": id, "yTitle": if yt.is_empty() { Json::Null } else { json!(yt) }}));
            }
        });
    });
    ui.collapsing(lang.tr("Data Table"), |ui| {
        let supported = chart.supports_data_table();
        let mut on = chart.data_table.is_some();
        if ui.add_enabled(supported, egui::Checkbox::new(&mut on, lang.tr("Show Data Table"))).changed() {
            app.run_or_alert("chart.set", json!({"chart": id, "dataTable": on}));
        }
        if !supported {
            ui.label(lang.tr("Available for column, bar, line, area and compatible combo charts."));
        }
        if let Some(table) = &chart.data_table {
            for (label, key, mut value) in [
                ("Show Legend Keys", "dataTableKeys", table.show_keys),
                ("Horizontal Borders", "dataTableHorizontalBorder", table.horizontal_border),
                ("Vertical Borders", "dataTableVerticalBorder", table.vertical_border),
                ("Outline", "dataTableOutline", table.outline),
            ] {
                if ui.checkbox(&mut value, lang.tr(label)).changed() {
                    app.run_or_alert("chart.set", json!({"chart": id, key: value}));
                }
            }
        }
    });
    let axis_choice = matches!(
        chart.kind,
        gridcraft_chart::ChartKind::Combo
            | gridcraft_chart::ChartKind::ColumnClustered
            | gridcraft_chart::ChartKind::ColumnStacked
            | gridcraft_chart::ChartKind::ColumnStacked100
            | gridcraft_chart::ChartKind::Line
            | gridcraft_chart::ChartKind::LineMarkers
            | gridcraft_chart::ChartKind::LineStacked
            | gridcraft_chart::ChartKind::Area
            | gridcraft_chart::ChartKind::AreaStacked
            | gridcraft_chart::ChartKind::AreaStacked100
    );
    ui.collapsing(lang.tr("Format Chart|Series"), |ui| {
        let data = app.session.active().map(|d| gridcraft_chart::resolve(&d.wb, d.wb.active_sheet(), &chart));
        for (i, s) in chart.series.iter().enumerate() {
            let name = data.as_ref().and_then(|d| d.series.get(i)).map(|x| x.name.clone()).unwrap_or_else(|| format!("Series {}", i + 1));
            let col =
                data.as_ref().and_then(|d| d.series.get(i)).map(|x| Color32::from_rgb(x.color[0], x.color[1], x.color[2])).unwrap_or(Color32::GRAY);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 2.0, col);
                ui.label(&name);
                ui.label(egui::RichText::new(&s.values).small().color(crate::theme::Tokens::get(ui.ctx()).text_dim));
                ui.menu_button(lang.tr("Fill…"), |ui| {
                    if let Some(c) = crate::widgets::color_palette_fixed(ui, &theme.colors, lang.tr("Automatic")) {
                        app.run_or_alert(
                            "chart.formatSelection",
                            json!({"chart": id, "series": i, "color": if c == "none" { Json::Null } else { json!(c) }}),
                        );
                        ui.close();
                    }
                });
            });
            // Format Data Series ▸ Series Options ▸ Plot Series On, for charts that can take a second value axis.
            if axis_choice {
                ui.horizontal(|ui| {
                    ui.label(lang.tr("Plot Series On"));
                    let secondary = chart.kind == gridcraft_chart::ChartKind::Combo && s.secondary;
                    for (label, on) in [("Primary Axis", false), ("Secondary Axis", true)] {
                        if ui.radio(secondary == on, lang.tr(label)).clicked() && secondary != on {
                            app.run_or_alert("chart.formatSelection", json!({"chart": id, "series": i, "secondary": on}));
                        }
                    }
                });
            }
        }
        // Format Data Point ▸ Series Options ▸ Set as total, point by point (the first series:
        // a waterfall chart plots one).
        if chart.kind == gridcraft_chart::ChartKind::Waterfall
            && let Some(s) = chart.series.first()
        {
            let d = data.as_ref().and_then(|d| d.series.first().map(|x| (d, x)));
            let n = d.map_or(0, |(d, x)| d.categories.len().max(x.values.len())).min(500);
            ui.label(egui::RichText::new(lang.tr("Format Data Point")).strong());
            for j in 0..n {
                let cat = data.as_ref().and_then(|d| d.categories.get(j)).cloned().unwrap_or_else(|| (j + 1).to_string());
                let mut on = s.subtotals.contains(&(j as u32));
                if ui.checkbox(&mut on, format!("{cat}: {}", lang.tr("Set as total"))).changed() {
                    app.run_or_alert("chart.setAsTotal", json!({"chart": id, "series": 0, "point": j, "on": on}));
                }
            }
        }
        if axis_choice && ui.button(lang.tr("Change Series Chart Type…")).clicked() {
            app.open_dialog("comboChart", json!({"chart": id}));
        }
    });
    ui.collapsing(lang.tr("Alt Text"), |ui| alt_text_fields(app, ui, ObjRef::new(ObjKind::Chart, id), &chart.alt_text));
    ui.collapsing(lang.tr("Colors & Layout"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for (p, label) in crate::ribbon::contextual::color_palettes(lang) {
                if ui.button(label).clicked() {
                    app.run_or_alert("chart.changeColors", json!({"chart": id, "palette": p}));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (i, layout) in gridcraft_engine::cmd::extra::quick_layouts(chart.kind).iter().enumerate() {
                let n = i + 1;
                let tip = crate::ribbon::contextual::quick_layout_tip(lang, layout);
                if ui.button(lang.tr("Layout {n}").replace("{n}", &n.to_string())).on_hover_text(tip).clicked() {
                    app.run_or_alert("chart.quickLayout", json!({"chart": id, "layout": n}));
                }
            }
        });
    });
    ui.collapsing(lang.tr("Size"), |ui| {
        // Keep the value being typed or dragged between frames; reloading it from the chart every frame threw the edit away.
        let draft = egui::Id::unique(("gridcraft.chart_size_draft", id));
        let current = (chart.anchor.width, chart.anchor.height);
        let (mut w, mut h) = ui.data_mut(|d| d.get_temp::<(f32, f32)>(draft)).unwrap_or(current);
        // A file can make a chart larger than the 4,000 points a field otherwise reaches. The
        // range keeps such a size, so the field shows it instead of clamping it to 4,000.
        let range = |v: f32| 40.0..=v.max(4000.0);
        let a = ui.add(egui::DragValue::new(&mut w).prefix(format!("{} ", lang.tr("Width"))).range(range(current.0)));
        let b = ui.add(egui::DragValue::new(&mut h).prefix(format!("{} ", lang.tr("Height"))).range(range(current.1)));
        if a.drag_stopped() || a.lost_focus() || b.drag_stopped() || b.lost_focus() {
            ui.data_mut(|d| d.remove::<(f32, f32)>(draft));
            if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                // Only an edited dimension is sent: the other keeps what the chart has, however
                // large a file made it.
                let mut params = serde_json::Map::new();
                params.insert("chart".into(), json!(id));
                if w != current.0 {
                    params.insert("width".into(), json!(w));
                }
                if h != current.1 {
                    params.insert("height".into(), json!(h));
                }
                if params.len() > 1 {
                    app.run_or_alert("chart.set", Json::Object(params));
                }
            }
        } else if a.dragged() || a.has_focus() || b.dragged() || b.has_focus() {
            ui.data_mut(|d| d.insert_temp(draft, (w, h)));
        } else {
            ui.data_mut(|d| d.remove::<(f32, f32)>(draft));
        }
    });
}

/// A chart slider that keeps the value being dragged in a per-chart draft and submits it once
/// through `chart.set` when the interaction ends, like the Size controls. Escape cancels, and a
/// value the chart already has is not sent again.
fn chart_slider(ui: &mut Ui, draft: egui::Id, value: u32, range: std::ops::RangeInclusive<u32>, label: &str) -> Option<u32> {
    let mut current = ui.data_mut(|d| d.get_temp::<u32>(draft)).unwrap_or(value);
    let r = ui.add(egui::Slider::new(&mut current, range).text(label));
    if r.drag_stopped() || r.clicked() || r.lost_focus() {
        ui.data_mut(|d| d.remove::<u32>(draft));
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) || current == value { None } else { Some(current) }
    } else if r.dragged() || r.has_focus() {
        ui.data_mut(|d| d.insert_temp(draft, current));
        None
    } else {
        ui.data_mut(|d| d.remove::<u32>(draft));
        None
    }
}

/// Review ▸ Alt Text, Format ▸ Alt Text and Edit Alt Text…: Microsoft 365's Alt Text pane on the
/// selected chart or picture, following the selection as Excel's does.
fn alt_text(app: &mut SheetApp, ui: &mut Ui) {
    let lang = app.ui.language;
    let target = app.selected_object.filter(|o| matches!(o.kind, ObjKind::Chart | ObjKind::Image));
    let current = target.and_then(|o| app.session.run("object.altText", json!({"kind": o.kind.name(), "id": o.id})).ok());
    let (Some(o), Some(current)) = (target, current) else {
        ui.label(egui::RichText::new(lang.tr("Select a chart or picture first.")).italics());
        return;
    };
    let alt = gridcraft_engine::model::AltText {
        title: current["title"].as_str().unwrap_or_default().to_string(),
        description: current["description"].as_str().unwrap_or_default().to_string(),
        decorative: current["decorative"].as_bool().unwrap_or(false),
    };
    alt_text_fields(app, ui, o, &alt);
}

/// The Alt Text pane's fields as Microsoft 365 has them: the question, one description box and
/// Mark as decorative, which greys the box out. There is no Title field (Excel 365 dropped it);
/// a title read from an older file is kept as it is.
fn alt_text_fields(app: &mut SheetApp, ui: &mut Ui, o: ObjRef, alt: &gridcraft_engine::model::AltText) {
    let lang = app.ui.language;
    ui.label(lang.tr("How would you describe this object and its context to someone who is blind or has low vision?"));
    ui.add_space(4.0);
    let field = egui::Id::unique(("gridcraft.alt_text_description", o.kind.name(), o.id));
    let draft = field.with("draft");
    let mut text = ui.data_mut(|d| d.get_temp::<String>(draft)).unwrap_or_else(|| alt.description.clone());
    let edit = egui::TextEdit::multiline(&mut text).id(field).desired_rows(5).desired_width(f32::INFINITY);
    let r = ui.add_enabled(!alt.decorative, edit);
    let r = r.on_hover_text(lang.tr("Description:"));
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(draft));
        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) && text != alt.description {
            app.run_or_alert("object.altText", json!({"kind": o.kind.name(), "id": o.id, "description": text}));
        }
    } else if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(draft, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(draft));
    }
    let mut decorative = alt.decorative;
    if ui.checkbox(&mut decorative, lang.tr("Mark as decorative")).changed() {
        app.run_or_alert("object.altText", json!({"kind": o.kind.name(), "id": o.id, "decorative": decorative}));
    }
}

