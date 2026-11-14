use super::controls::{act, anchor_keytip, control_keytip, invoke, popup_command, sep};
use super::{
    Icon, Sense, SheetApp, Stroke, StrokeKind, Tokens, Ui, big_button, big_split_button, icons, json, small_button, small_toggle_button, vec2,
};

/// Draws the Insert tab: PivotTable, Table, Pictures, Shapes, Icons, charts, slicer, sparklines, form controls (button, slider, spin button), checkbox, links, comments, text box, header/footer and symbols.
pub(super) fn insert(app: &mut SheetApp, ui: &mut Ui) {
    let __k = big_button(ui, Icon::PivotTable, "PivotTable", "PivotTable", false);
    control_keytip(app, ui, &__k, "insert.pivotTable");
    if __k.clicked() {
        act(app, "insert.pivotTable", json!({}));
    }
    let __k = big_button(ui, Icon::Table, "Table", "Table", false);
    control_keytip(app, ui, &__k, "insert.table");
    if __k.clicked() {
        act(app, "insert.table", json!({"prompt": true}));
    }
    sep(ui);
    let pic = big_button(ui, Icon::Picture, "Pictures", "Insert a picture", true);
    anchor_keytip(app, ui, &pic, &["insert.picture"]);
    pic.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), app.ui.language.tr("Pictures")));
    egui::Popup::menu(&pic).show(|ui| {
        for (label, placement) in [("Place in Cell", "cell"), ("Place over Cells", "overCells")] {
            if ui.button(app.ui.language.tr(label)).clicked() {
                app.open_dialog("insertPicture", json!({"placement": placement}));
                ui.close();
            }
        }
    });
    let shapes = big_button(ui, Icon::Shapes, "Shapes", "Shapes", true);
    anchor_keytip(app, ui, &shapes, &["insert.shape"]);
    egui::Popup::menu(&shapes).show(|ui| {
        for (label, kind) in [
            ("Rectangle", "rectangle"),
            ("Rounded Rectangle", "roundedRectangle"),
            ("Oval", "ellipse"),
            ("Triangle", "triangle"),
            ("Line", "line"),
            ("Arrow", "arrow"),
            ("Text Box", "textBox"),
        ] {
            if ui.button(app.ui.language.tr(label)).clicked() {
                act(app, "insert.shape", json!({"kind": kind}));
            }
        }
    });
    let icn = big_button(ui, Icon::Theme, "Icons", "Insert an icon", true);
    anchor_keytip(app, ui, &icn, &["insert.icons"]);
    egui::Popup::menu(&icn).show(|ui| {
        ui.set_width(260.0);
        egui::Grid::new("icon_lib").show(ui, |ui| {
            for (i, (name, icon)) in icons::LIBRARY.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 4.0, Tokens::get(ui.ctx()).hover);
                }
                icons::paint(ui.painter(), r.shrink(6.0), *icon, Tokens::get(ui.ctx()).text);
                if resp.on_hover_text(*name).clicked() {
                    act(app, "insert.icons", json!({"name": name}));
                    ui.close();
                }
                if (i + 1) % 6 == 0 {
                    ui.end_row();
                }
            }
        });
    });
    sep(ui);
    charts_group(app, ui);
    sep(ui);
    if big_button(ui, Icon::Filter, "Slicer", "Insert a slicer to filter a table", false).clicked() {
        act(app, "insert.slicer", json!({}));
    }
    sep(ui);
    let sp = big_button(ui, Icon::Sparkline, "Sparklines", "Sparklines", true);
    anchor_keytip(app, ui, &sp, &["dialog:sparkline"]);
    egui::Popup::menu(&sp).show(|ui| {
        for (label, kind) in [("Line", "line"), ("Column", "column"), ("Win/Loss", "winLoss")] {
            if ui.button(app.ui.language.tr(label)).clicked() {
                app.open_dialog("sparkline", json!({"type": kind}));
            }
        }
    });
    sep(ui);
    // Form controls: draw a button, slider or spin button on the sheet (a click places it at its
    // default size). Their key tips run the command at the active cell.
    use gridcraft_engine::model::ControlKind;
    let placing = app.grid.placing;
    ui.vertical(|ui| {
        for (icon, kind, label, tip, id) in [
            (Icon::Button, ControlKind::Button, "Button", "Draw a button that runs a macro or a command", "insert.button"),
            (Icon::Slider, ControlKind::Slider, "Slider", "Draw a slider that sets a whole number in a linked cell", "insert.slider"),
            (Icon::Spin, ControlKind::Spin, "Spin Button", "Draw a spin button that steps a whole number in a linked cell", "insert.spinButton"),
        ] {
            let on = placing == Some(kind);
            let r = small_toggle_button(ui, icon, label, tip, on);
            control_keytip(app, ui, &r, id);
            if r.clicked() {
                app.grid.placing = if placing == Some(kind) { None } else { Some(kind) };
                app.selected_object = None;
            }
        }
    });
    sep(ui);
    // Cell Controls.
    if big_button(ui, Icon::Checkbox, "Checkbox", "Insert checkboxes in the selected cells", false).clicked() {
        act(app, "insert.checkbox", json!({}));
    }
    sep(ui);
    let __k = big_button(ui, Icon::Link, "Link", "Insert Link", false);
    control_keytip(app, ui, &__k, "dialog:insertLink");
    if __k.clicked() {
        app.open_dialog("insertLink", json!({}));
    }
    let __k = big_button(ui, Icon::Comment, "Comment", "New Comment", false);
    control_keytip(app, ui, &__k, "dialog:comment");
    if __k.clicked() {
        app.open_dialog("comment", json!({"threaded": true}));
    }
    sep(ui);
    let __k = big_button(ui, Icon::TextBox, "Text\nBox", "Text Box", false);
    control_keytip(app, ui, &__k, "insert.textBox");
    if __k.clicked() {
        act(app, "insert.textBox", json!({}));
    }
    let __k = big_button(ui, Icon::PageLayout, "Header &\nFooter", "Header & Footer", false);
    control_keytip(app, ui, &__k, "dialog:headerFooter");
    if __k.clicked() {
        // As in Excel: Page Layout view, editing the header in place (the dialog is under Page Setup).
        crate::paper_view::start_editing(app);
    }
    // Symbols: Equation, then Symbol.
    sep(ui);
    let (eq, eq_menu) = big_split_button(ui, Icon::Equation, "Equation", "Insert an equation, or pick one from the gallery");
    control_keytip(app, ui, &eq, "dialog:equation");
    if eq.clicked() {
        app.open_dialog("equation", json!({}));
    }
    egui::Popup::menu(&eq_menu).show(|ui| {
        ui.set_width(300.0);
        if ui.add(egui::Button::new(app.ui.language.tr("Insert New Equation...")).frame(false).min_size(vec2(290.0, 24.0))).clicked() {
            app.open_dialog("equation", json!({}));
            ui.close();
        }
        ui.separator();
        if let Some(latex) = crate::equation::gallery(ui) {
            act(app, "insert.equation", json!({"latex": latex}));
            ui.close();
        }
    });
    let sym = big_button(ui, Icon::Symbol, "Symbol", "Symbol", true);
    control_keytip(app, ui, &sym, "insert.symbol");
    // Opened by its key tip too (Alt+I, S): the gallery picks the symbol, nothing is inserted before.
    let forced = invoke(app, "insert.symbol");
    egui::Popup::menu(&sym).open_memory(popup_command(forced, sym.clicked())).show(|ui| {
        ui.set_width(260.0);
        ui.horizontal_wrapped(|ui| {
            for ch in [
                "€", "£", "¥", "©", "®", "™", "±", "≠", "≤", "≥", "÷", "×", "∞", "µ", "α", "β", "π", "Σ", "Ω", "°", "•", "…", "→", "←", "↑", "↓",
                "✓", "★", "½", "¼",
            ] {
                // No bundled face has ✓: the button paints a code-drawn check instead of a tofu box.
                if crate::widgets::glyph_button(ui, ch.chars().next().unwrap_or('?'), &format!("Insert {ch}")).clicked() {
                    act(app, "insert.symbol", json!({"char": ch}));
                }
            }
        });
    });
}

/// One chart type menu of the Charts group: its button's icon, tooltip and key-tip id, the
/// sections of chart types it lists `(heading, [(label, type, subtype)])`, and its last item that
/// opens Insert Chart on All Charts (`(label, category)`).
type ChartMenu = (
    Icon,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static [(&'static str, &'static str, &'static str)])],
    (&'static str, &'static str),
);

/// The chart type menus of Insert ▸ Charts, in the order Excel 365 lays them out (three columns
/// of two or three buttons), with only the chart types GridCraft can make.
const CHART_MENUS: [ChartMenu; 8] = [
    (
        Icon::ChartBar,
        "Insert Column or Bar Chart",
        "insert.chart.column",
        &[
            (
                "2-D Column",
                &[("Clustered Column", "column", ""), ("Stacked Column", "column", "stacked"), ("100% Stacked Column", "column", "stacked100")],
            ),
            ("2-D Bar", &[("Clustered Bar", "bar", ""), ("Stacked Bar", "bar", "stacked"), ("100% Stacked Bar", "bar", "stacked100")]),
        ],
        ("More Column Charts…", "column"),
    ),
    (
        Icon::ChartLine,
        "Insert Line or Area Chart",
        "insert.chart.line",
        &[
            ("2-D Line", &[("Line", "line", ""), ("Stacked Line", "line", "stacked"), ("Line with Markers", "line", "markers")]),
            ("2-D Area", &[("Area", "area", ""), ("Stacked Area", "area", "stacked"), ("100% Stacked Area", "area", "stacked100")]),
        ],
        ("More Line Charts…", "line"),
    ),
    (
        Icon::ChartPie,
        "Insert Pie or Doughnut Chart",
        "insert.chart.pie",
        &[("2-D Pie", &[("Pie", "pie", "")]), ("ChartSection|Doughnut", &[("Doughnut", "doughnut", "")])],
        ("More Pie Charts…", "pie"),
    ),
    (
        Icon::Theme,
        "Insert Hierarchy Chart",
        "insert.chart.hierarchy",
        &[("ChartSection|Treemap", &[("Treemap", "treemap", "")]), ("ChartSection|Sunburst", &[("Sunburst", "sunburst", "")])],
        ("More Hierarchy Charts…", "treemap"),
    ),
    (
        Icon::Chart,
        "Insert Statistic Chart",
        "insert.chart.statistic",
        &[("ChartSection|Histogram", &[("Histogram", "histogram", "")]), ("ChartSection|Box & Whisker", &[("Box & Whisker", "boxWhisker", "")])],
        ("More Statistical Charts…", "histogram"),
    ),
    (
        Icon::ChartScatter,
        "Insert Scatter (X, Y) or Bubble Chart",
        "insert.chart.scatter",
        &[
            ("ChartSection|Scatter", &[("Scatter", "scatter", ""), ("Scatter with Lines", "scatter", "lines")]),
            ("ChartSection|Bubble", &[("Bubble", "bubble", "")]),
        ],
        ("More Scatter Charts…", "scatter"),
    ),
    (
        Icon::ChartBar,
        "Insert Waterfall, Funnel, Stock, Surface or Radar Chart",
        "insert.chart.waterfall",
        &[
            ("ChartSection|Waterfall", &[("Waterfall", "waterfall", "")]),
            ("ChartSection|Funnel", &[("Funnel", "funnel", "")]),
            ("ChartSection|Stock", &[("High-Low-Close", "stock", "")]),
            ("ChartSection|Radar", &[("Radar", "radar", "")]),
        ],
        ("More Stock Charts…", "stock"),
    ),
    (
        Icon::ChartLine,
        "Insert Combo Chart",
        "insert.chart.combo",
        &[(
            "",
            &[
                ("Clustered Column - Line", "combo", "columnLine"),
                ("Clustered Column - Line on Secondary Axis", "combo", "columnLineSecondary"),
                ("Stacked Area - Clustered Column", "combo", "areaColumn"),
            ],
        )],
        ("Create Custom Combo Chart…", "combo"),
    ),
];

/// The params `insert.chart` takes for a menu's chart type: a combo preset becomes its rows for
/// the series the selection makes.
fn menu_chart_params(app: &SheetApp, t: &str, sub: &str) -> serde_json::Value {
    if t != "combo" {
        return json!({"type": t, "subtype": sub});
    }
    let n = gridcraft_engine::cmd::insert::build_chart(&app.session, &json!({"type": "column"})).map_or(0, |(_, c)| c.series.len());
    let rows: Vec<serde_json::Value> = crate::dialogs::combo_preset(sub, n)
        .into_iter()
        .map(|(ty, st, secondary)| json!({"type": ty, "subtype": st, "secondary": secondary}))
        .collect();
    json!({"type": "combo", "subtype": sub, "combo": rows})
}

/// Insert ▸ Charts (グラフ) as in Excel 365: Recommended Charts, the chart type menus, PivotChart
/// and the group's dialog launcher, which opens Insert Chart.
fn charts_group(app: &mut SheetApp, ui: &mut Ui) {
    let __k = big_button(ui, Icon::Chart, "Recommended\nCharts", "Recommended Charts", false);
    control_keytip(app, ui, &__k, "insert.recommendedCharts");
    if __k.clicked() {
        // Menu-style: with no chart type, run_or_alert opens the Insert Chart dialog.
        act(app, "insert.recommendedCharts", json!({}));
    }
    // Excel lays the menus out in columns: Column or Bar, Line or Area, Pie or Doughnut; then
    // Hierarchy, Statistic, Scatter; then Waterfall…, Combo.
    for column in [&CHART_MENUS[0..3], &CHART_MENUS[3..6], &CHART_MENUS[6..8]] {
        ui.vertical(|ui| {
            for (icon, tip, cid, sections, (more, category)) in column {
                let r = small_button(ui, *icon, "", tip, true);
                control_keytip(app, ui, &r, cid);
                let forced = invoke(app, cid);
                egui::Popup::menu(&r).open_memory(popup_command(forced, r.clicked())).show(|ui| {
                    ui.set_min_width(220.0);
                    for (heading, items) in sections.iter() {
                        if !heading.is_empty() {
                            ui.label(egui::RichText::new(app.ui.language.tr(heading)).strong());
                        }
                        for (label, k, s) in items.iter() {
                            if ui.button(app.ui.language.tr(label)).clicked() {
                                let params = menu_chart_params(app, k, s);
                                act(app, "insert.chart", params);
                            }
                        }
                        ui.separator();
                    }
                    if ui.button(app.ui.language.tr(more)).clicked() {
                        let combo = *category == "combo";
                        let params = json!({"tab": "All Charts", "category": category, "type": if combo { "combo" } else { "" }, "subtype": if combo { "custom" } else { "" }});
                        app.open_dialog("recommendedCharts", params);
                        ui.close();
                    }
                });
            }
            if is_last_column(column) {
                launcher(app, ui);
            }
        });
    }
    // Excel's PivotChart, in the Charts group.
    super::contextual::pivot_chart_button(app, ui, false);
}

/// Whether `column` is the Charts group's last column, where the dialog launcher sits.
fn is_last_column(column: &[ChartMenu]) -> bool {
    column.last().is_some_and(|m| m.2 == "insert.chart.combo")
}

/// The Charts group's dialog launcher (its corner button): Insert Chart on Recommended Charts.
fn launcher(app: &mut SheetApp, ui: &mut Ui) {
    ui.add_space(4.0);
    let tip = app.ui.language.tr("See All Charts").to_string();
    let (r, l) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
    let t = Tokens::get(ui.ctx());
    if l.hovered() {
        ui.painter().rect_filled(r, 2.0, t.hover);
    }
    // Excel's launcher: a corner with an arrow pointing into it, drawn in code.
    let s = Stroke::new(1.0, t.text_dim);
    let (a, b) = (r.shrink(4.0).left_top(), r.shrink(4.0).right_bottom());
    ui.painter().line_segment([egui::pos2(a.x, a.y), egui::pos2(b.x, a.y)], s);
    ui.painter().line_segment([egui::pos2(a.x, a.y), egui::pos2(a.x, b.y)], s);
    ui.painter().line_segment([egui::pos2(a.x + 2.0, a.y + 2.0), b], s);
    ui.painter().line_segment([b, egui::pos2(b.x - 3.0, b.y)], s);
    ui.painter().line_segment([b, egui::pos2(b.x, b.y - 3.0)], s);
    l.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &tip));
    let l = l.on_hover_text(tip.as_str());
    if l.clicked() {
        app.open_dialog("recommendedCharts", json!({}));
    }
}

pub(super) fn draw(app: &mut SheetApp, ui: &mut Ui) {
    let tool = app.session.draw_tool.clone();
    let __k = big_button(ui, Icon::Shapes, "Select\nObjects", "Select and move ink and shapes", false);
    control_keytip(app, ui, &__k, "draw.lasso");
    if __k.clicked() {
        act(app, "draw.lasso", json!({}));
    }
    sep(ui);
    for (label, color) in [("Pen\nBlue", "#1F5FC9"), ("Pen\nBlack", "#000000"), ("Pen\nRed", "#D13B2F"), ("Highlighter", "#F2E33A")] {
        let on = tool == "pen" && app.session.draw_color.eq_ignore_ascii_case(color);
        let r = big_button(ui, Icon::Pen, label, "Draw with ink", false);
        if on {
            ui.painter().rect_stroke(r.rect, 5.0, Stroke::new(2.0, Tokens::get(ui.ctx()).accent), StrokeKind::Inside);
        }
        if r.clicked() {
            let width = if label == "Highlighter" { 12.0 } else { 2.0 };
            act(app, "draw.pen", json!({"on": !on, "color": color, "width": width}));
        }
    }
    let er = big_button(ui, Icon::Eraser, "Eraser", "Erase ink strokes", false);
    if tool == "eraser" {
        ui.painter().rect_stroke(er.rect, 5.0, Stroke::new(2.0, Tokens::get(ui.ctx()).accent), StrokeKind::Inside);
    }
    if er.clicked() {
        act(app, "draw.eraser", json!({}));
    }
    sep(ui);
    let __k = big_button(ui, Icon::Shapes, "Ink to\nShape", "Convert the last ink stroke to a shape", false);
    control_keytip(app, ui, &__k, "draw.inkToShape");
    if __k.clicked() {
        act(app, "draw.inkToShape", json!({}));
    }
}
