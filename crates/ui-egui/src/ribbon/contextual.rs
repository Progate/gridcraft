use super::controls::{act, anchor_keytip, control_keytip, keytip, menu_items, sep};
use super::galleries::{paint_table_swatch, table_gallery};
use super::{Color32, Icon, Sense, SheetApp, Stroke, StrokeKind, Tokens, Ui, big_button, icon_button, json, keytip_table, vec2};

/// Draws the Table Design tab: rename, resize, remove duplicates, convert to range, slicer, table options and the style strip.
pub(super) fn table_design(app: &mut SheetApp, ui: &mut Ui) {
    let Some((tname, header, totals, banded, banded_c, first, last, filter, style)) = app.session.active().and_then(|d| {
        let sh = d.wb.active()?;
        let t = sh.table_at(d.selection.active)?;
        Some((t.name.clone(), t.header_row, t.totals_row, t.banded_rows, t.banded_cols, t.first_col, t.last_col, t.filter_button, t.style.clone()))
    }) else {
        return;
    };
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(app.ui.language.tr("Table Name:")).small());
        let field = egui::Id::unique(("gridcraft.table_name", &tname));
        if let Some(name) = crate::widgets::committed_text(ui, field, &tname, Some(120.0))
            && name != tname
        {
            act(app, "table.rename", json!({"table": tname, "name": name}));
        }
    });
    let __k = big_button(ui, Icon::Table, "Resize\nTable", "Resize Table", false);
    control_keytip(app, ui, &__k, "dialog:tableResize");
    if __k.clicked() {
        app.open_dialog("tableResize", json!({}));
    }
    sep(ui);
    let __k = big_button(ui, Icon::Duplicates, "Remove\nDuplicates", "Remove Duplicates", false);
    control_keytip(app, ui, &__k, "data.removeDuplicates");
    if __k.clicked() {
        act(app, "data.removeDuplicates", json!({}));
    }
    let __k = big_button(ui, Icon::Table, "Convert\nto Range", "Convert to Range", false);
    control_keytip(app, ui, &__k, "table.convertToRange");
    if __k.clicked() {
        act(app, "table.convertToRange", json!({"table": tname, "prompt": true}));
    }
    sep(ui);
    if big_button(ui, Icon::Filter, "Insert\nSlicer", "Insert Slicer", false).clicked() {
        act(app, "table.insertSlicer", json!({"table": tname}));
    }
    sep(ui);
    ui.vertical(|ui| {
        for (label, cmd, v) in [
            ("Header Row", "table.headerRow", header && filter),
            ("Total Row", "table.totalRow", totals),
            ("Banded Rows", "table.bandedRows", banded),
        ] {
            let mut on = v;
            if ui.checkbox(&mut on, app.ui.language.tr(label)).changed() {
                // The total row's label is written in the interface language, as Excel writes 集計.
                let total = app.ui.language.tr("Total").to_string();
                act(app, cmd, json!({"table": tname, "on": on, "label": total}));
            }
        }
    });
    ui.vertical(|ui| {
        for (label, cmd, v) in [
            ("First Column", "table.firstColumn", first),
            ("Last Column", "table.lastColumn", last),
            ("Banded Columns", "table.bandedColumns", banded_c),
        ] {
            let mut on = v;
            if ui.checkbox(&mut on, app.ui.language.tr(label)).changed() {
                act(app, cmd, json!({"table": tname, "on": on}));
            }
        }
    });
    sep(ui);
    table_style_strip(app, ui, &tname, &style);
}

/// The first page of Excel's Table Styles gallery, drawn in the ribbon. The More arrow opens the
/// full gallery (Light, Medium, Dark) as a popup.
const RIBBON_TABLE_STYLES: [&str; 7] =
    ["TableStyleLight1", "TableStyleLight2", "TableStyleLight3", "TableStyleLight4", "TableStyleLight5", "TableStyleLight6", "TableStyleLight7"];

fn table_style_strip(app: &mut SheetApp, ui: &mut Ui, tname: &str, current: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        // Borrow the workbook only while painting; the session must be free for `act` afterwards.
        let clicked = {
            let Some(d) = app.session.active() else { return };
            let wb = &d.wb;
            let mut clicked = None;
            for name in RIBBON_TABLE_STYLES {
                let (r, resp) = ui.allocate_exact_size(vec2(52.0, 40.0), Sense::click());
                paint_table_swatch(ui, r, wb, name);
                if name == current {
                    ui.painter().rect_stroke(r.expand(2.5), 2.0, Stroke::new(2.0, t.accent), StrokeKind::Outside);
                }
                if resp.hovered() {
                    ui.painter().rect_stroke(r.expand(1.0), 2.0, Stroke::new(2.0, Color32::from_rgb(0xE8, 0x7D, 0x2B)), StrokeKind::Outside);
                }
                if resp.on_hover_text(app.ui.language.style_name(&wb.theme, name)).clicked() {
                    clicked = Some(name);
                }
            }
            clicked
        };
        if let Some(name) = clicked {
            act(app, "table.style", json!({"table": tname, "style": name}));
        }
        let more = icon_button(ui, Icon::Chevron, t.text_dim, "More Table Styles", vec2(18.0, 40.0));
        anchor_keytip(app, ui, &more, &["table.style"]);
        egui::Popup::menu(&more).show(|ui| table_gallery(app, ui, "table.style"));
    });
}

/// What Chart Design reads of the selected chart: its type, Chart Styles entry and elements.
struct ChartState {
    id: u32,
    kind: gridcraft_engine::model::ChartKind,
    gallery: Option<u32>,
    styles: u32,
    axes: bool,
    across: bool,
    horizontal_title: bool,
    vertical_title: bool,
    title: bool,
    overlay: bool,
    data_table: bool,
}

fn chart_state(app: &SheetApp) -> Option<ChartState> {
    let id = app.selected_chart()?;
    let d = app.session.active()?;
    let si = d.wb.active_sheet();
    let c = d.wb.sheet(si)?.charts.iter().find(|c| c.id == id)?;
    let shown = gridcraft_engine::cmd::insert::shown_chart_title(&d.wb, si, c, app.session.prefs.chart_title());
    Some(ChartState {
        id,
        kind: c.kind,
        gallery: c.gallery_style(),
        styles: gridcraft_engine::model::chart_style_family(c.kind).map_or(0, |(_, n)| n),
        axes: gridcraft_engine::cmd::extra::has_axes(c),
        across: c.value_axis_across(),
        horizontal_title: c.axis_title(true).is_some(),
        vertical_title: c.axis_title(false).is_some(),
        title: shown.is_some(),
        overlay: c.title_overlay,
        data_table: c.supports_data_table(),
    })
}

/// A menu item shown selected while its choice is the chart's, as Excel highlights it.
fn check_item(app: &mut SheetApp, ui: &mut Ui, label: &str, on: bool) -> bool {
    let text = app.ui.language.tr(label);
    ui.add(egui::Button::selectable(on, text).min_size(vec2(200.0, 22.0))).clicked()
}

/// Add Chart Element, as Excel's menu: Axis Titles, Chart Title, Data Labels, Data Table,
/// Gridlines and Legend, each a submenu of its choices.
fn add_chart_element_menu(app: &mut SheetApp, ui: &mut Ui, c: &ChartState) {
    let lang = app.ui.language;
    let id = c.id;
    if c.axes {
        ui.menu_button(lang.tr("Axis Titles"), |ui| {
            for (label, axis, on) in [("Primary Horizontal", "horizontal", c.horizontal_title), ("Primary Vertical", "vertical", c.vertical_title)] {
                if check_item(app, ui, label, on) {
                    act(app, "chart.addElement", json!({"chart": id, "element": "axisTitle", "axis": axis, "on": !on}));
                    ui.close();
                }
            }
        });
    }
    ui.menu_button(lang.tr("Chart Title"), |ui| {
        for (label, on, params) in [
            ("None", !c.title, json!({"chart": id, "element": "title", "on": false})),
            ("Above Chart", c.title && !c.overlay, json!({"chart": id, "element": "title", "position": "above"})),
            ("Centered Overlay", c.title && c.overlay, json!({"chart": id, "element": "title", "position": "overlay"})),
        ] {
            if check_item(app, ui, label, on) {
                act(app, "chart.addElement", params);
                ui.close();
            }
        }
    });
    ui.menu_button(lang.tr("Data Labels"), |ui| {
        menu_items(
            app,
            ui,
            &[("None", "chart.set", json!({"chart": id, "dataLabels": false})), ("Show", "chart.set", json!({"chart": id, "dataLabels": true}))],
        );
    });
    if c.data_table {
        ui.menu_button(lang.tr("Data Table"), |ui| {
            menu_items(
                app,
                ui,
                &[
                    ("None", "chart.addElement", json!({"chart": id, "element": "dataTable", "on": false})),
                    ("With Legend Keys", "chart.addElement", json!({"chart": id, "element": "dataTable", "dataTableKeys": true})),
                    ("No Legend Keys", "chart.addElement", json!({"chart": id, "element": "dataTable", "dataTableKeys": false})),
                ],
            );
        });
    }
    if c.axes {
        ui.menu_button(lang.tr("Gridlines"), |ui| {
            let on = app.session.active().and_then(|d| d.wb.active()?.charts.iter().find(|x| x.id == id).map(|x| x.gridlines)).unwrap_or(false);
            // GridCraft's gridlines are the value axis's major ones: horizontal lines, or vertical on a bar chart.
            let label = if c.across { "Primary Major Vertical" } else { "Primary Major Horizontal" };
            if check_item(app, ui, label, on) {
                act(app, "chart.set", json!({"chart": id, "gridlines": !on}));
                ui.close();
            }
        });
    }
    ui.menu_button(lang.tr("Legend"), |ui| {
        menu_items(
            app,
            ui,
            &[
                ("None", "chart.set", json!({"chart": id, "legend": "none"})),
                ("Right", "chart.set", json!({"chart": id, "legend": "right"})),
                ("Top", "chart.set", json!({"chart": id, "legend": "top"})),
                ("Left", "chart.set", json!({"chart": id, "legend": "left"})),
                ("Bottom", "chart.set", json!({"chart": id, "legend": "bottom"})),
            ],
        );
    });
}

/// How many Chart Styles the ribbon shows before the More arrow, as Excel's gallery row.
const RIBBON_CHART_STYLES: u32 = 6;

/// One Chart Styles thumbnail: the chart drawn small, its number, and the chosen one framed.
/// Returns whether it was clicked.
fn chart_style_swatch(app: &SheetApp, ui: &mut Ui, n: u32, current: Option<u32>) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(52.0, 40.0), Sense::click());
    ui.painter().rect_filled(r, 2.0, Color32::WHITE);
    if let Some(d) = app.session.active()
        && let Some(id) = app.selected_chart()
        && let Some(c) = d.wb.active().and_then(|sh| sh.charts.iter().find(|c| c.id == id))
    {
        crate::chartview::paint_chart(ui.painter(), r.shrink(2.0), &d.wb, d.wb.active_sheet(), c);
    }
    ui.painter().text(r.left_top() + vec2(3.0, 2.0), egui::Align2::LEFT_TOP, n.to_string(), crate::theme::ui_font(9.0), Color32::from_gray(0x40));
    if current == Some(n) {
        ui.painter().rect_stroke(r.expand(2.5), 2.0, Stroke::new(2.0, t.accent), StrokeKind::Outside);
    }
    if resp.hovered() {
        ui.painter().rect_stroke(r.expand(1.0), 2.0, Stroke::new(2.0, Color32::from_rgb(0xE8, 0x7D, 0x2B)), StrokeKind::Outside);
    }
    let name = app.ui.language.tr("Style {n}").replace("{n}", &n.to_string());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name.clone()));
    resp.on_hover_text(name).clicked()
}

pub(super) fn chart_design(app: &mut SheetApp, ui: &mut Ui) {
    let Some(c) = chart_state(app) else { return };
    let id = c.id;
    let ae = big_button(ui, Icon::Chart, "Add Chart\nElement", "Add Chart Element", true);
    for sequence in keytip_table::group_extension(app.keytips.prefix().unwrap_or_default(), &["chart.set"]) {
        keytip(app, ui, sequence, ae.rect.center_bottom());
    }
    egui::Popup::menu(&ae).show(|ui| add_chart_element_menu(app, ui, &c));
    let layouts = gridcraft_engine::cmd::extra::quick_layouts(c.kind);
    if !layouts.is_empty() {
        let ql = big_button(ui, Icon::Chart, "Quick\nLayout", "Quick Layout", true);
        control_keytip(app, ui, &ql, "chart.quickLayout");
        egui::Popup::menu(&ql).show(|ui| {
            let lang = app.ui.language;
            for (i, layout) in layouts.iter().enumerate() {
                let n = i + 1;
                let label = lang.tr("Layout {n}").replace("{n}", &n.to_string());
                if ui.button(label).on_hover_text(quick_layout_tip(lang, layout)).clicked() {
                    act(app, "chart.quickLayout", json!({"chart": id, "layout": n}));
                }
            }
        });
    }
    sep(ui);
    chart_style_strip(app, ui, &c);
    sep(ui);
    let __k = big_button(ui, Icon::Redo, "Switch\nRow/Column", "Switch Row/Column", false);
    control_keytip(app, ui, &__k, "chart.switchRowColumn");
    if __k.clicked() {
        act(app, "chart.switchRowColumn", json!({"chart": id}));
    }
    if big_button(ui, Icon::Table, "Select\nData", "Select Data", false).clicked() {
        app.open_dialog("selectData", json!({"chart": id}));
    }
    sep(ui);
    let ct = big_button(ui, Icon::ChartBar, "Change\nChart Type", "Change Chart Type", true);
    for sequence in keytip_table::group_extension(
        app.keytips.prefix().unwrap_or_default(),
        &["column", "bar", "line", "area", "pie", "doughnut", "scatter", "radar", "waterfall", "funnel", "treemap"],
    ) {
        keytip(app, ui, sequence, ct.rect.center_bottom());
    }
    egui::Popup::menu(&ct).show(|ui| {
        for (label, k, s) in [
            ("Clustered Column", "column", ""),
            ("Stacked Column", "column", "stacked"),
            ("Bar", "bar", ""),
            ("Line", "line", "markers"),
            ("Area", "area", ""),
            ("Pie", "pie", ""),
            ("Doughnut", "doughnut", ""),
            ("Scatter", "scatter", ""),
            ("Radar", "radar", ""),
            ("Waterfall", "waterfall", ""),
            ("Funnel", "funnel", ""),
            ("Treemap", "treemap", ""),
        ] {
            if ui.button(app.ui.language.tr(label)).clicked() {
                act(app, "chart.set", json!({"chart": id, "type": k, "subtype": s}));
            }
        }
        ui.separator();
        if ui.button(app.ui.language.tr("Combo…")).clicked() {
            app.open_dialog("comboChart", json!({"chart": id}));
        }
    });
    sep(ui);
    let __k = big_button(ui, Icon::Chart, "Move\nChart", "Move Chart", false);
    control_keytip(app, ui, &__k, "chart.move");
    if __k.clicked() {
        app.open_dialog("moveChart", json!({"chart": id}));
    }
    sep(ui);
    if big_button(ui, Icon::Settings, "Format\nPane", "Format Chart pane", false).clicked() {
        app.grid.pane = Some("formatChart".into());
    }
    let __k = big_button(ui, Icon::TextBox, "Chart\nTitle", "Edit chart title", false);
    control_keytip(app, ui, &__k, "chartTitle");
    if __k.clicked() {
        app.open_dialog("chartTitle", json!({"chart": id}));
    }
    let __k = big_button(ui, Icon::Delete, "Delete\nChart", "Delete chart", false);
    control_keytip(app, ui, &__k, "chart.delete");
    if __k.clicked() {
        act(app, "chart.delete", json!({"chart": id}));
        app.selected_object = None;
    }
}

/// Chart Design ▸ Chart Styles: the first styles of the chart type's gallery, and the More arrow
/// that opens all of them. Picking one saves it as the chart's style (`chart.set {style}`).
fn chart_style_strip(app: &mut SheetApp, ui: &mut Ui, c: &ChartState) {
    if c.styles == 0 {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.add_space(4.0);
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for n in 1..=c.styles.min(RIBBON_CHART_STYLES) {
            if chart_style_swatch(app, ui, n, c.gallery) {
                picked = Some(n);
            }
        }
        let more = icon_button(ui, Icon::Chevron, t.text_dim, "More Chart Styles", vec2(18.0, 40.0));
        control_keytip(app, ui, &more, "chart.set");
        egui::Popup::menu(&more).show(|ui| {
            egui::Grid::new("gridcraft.chart_styles").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
                for n in 1..=c.styles {
                    if chart_style_swatch(app, ui, n, c.gallery) {
                        picked = Some(n);
                        ui.close();
                    }
                    if n % 4 == 0 {
                        ui.end_row();
                    }
                }
            });
        });
    });
    if let Some(n) = picked {
        act(app, "chart.set", json!({"chart": c.id, "style": n}));
    }
}

/// The chart's Format tab (書式), with what GridCraft can do there: Format Selection, Alt Text
/// (Excel's Accessibility group) and Bring Forward / Send Backward.
pub(super) fn chart_format(app: &mut SheetApp, ui: &mut Ui) {
    let Some(id) = app.selected_chart() else { return };
    let __k = big_button(ui, Icon::Settings, "Format\nSelection", "Format Selection", false);
    control_keytip(app, ui, &__k, "chart.formatSelection");
    if __k.clicked() {
        app.grid.pane = Some("formatChart".into());
    }
    sep(ui);
    let __k = big_button(ui, Icon::TextBox, "Alt\nText", "Alt Text", false);
    control_keytip(app, ui, &__k, "object.altText");
    if __k.clicked() {
        app.open_dialog("altText", json!({"kind": "chart", "id": id}));
    }
    sep(ui);
    for (label, tip, cmd) in
        [("Bring\nForward", "Bring Forward", "arrange.bringForward"), ("Send\nBackward", "Send Backward", "arrange.sendBackward")]
    {
        let __k = big_button(ui, Icon::Shapes, label, tip, false);
        control_keytip(app, ui, &__k, cmd);
        if __k.clicked() {
            act(app, cmd, json!({"kind": "chart", "id": id}));
        }
    }
}

/// The Sparkline tab (スパークライン) for the sparkline group of the active cell: its type (Line,
/// Column, Win/Loss) and the points it shows (High Point, Low Point, Markers), as Excel's Type and
/// Show groups. A change applies to the whole group.
pub(super) fn sparkline_tab(app: &mut SheetApp, ui: &mut Ui) {
    use gridcraft_engine::model::SparklineKind;
    let Some((cell, kind, markers, high, low)) = app.session.active().and_then(|d| {
        let sp = d.wb.active()?.sparklines.iter().find(|sp| sp.cell == d.selection.active)?;
        Some((sp.cell.a1(), sp.kind, sp.markers, sp.high, sp.low))
    }) else {
        return;
    };
    for (label, tip, k, icon) in [
        ("Line", "Line", SparklineKind::Line, Icon::ChartLine),
        ("Column", "Column", SparklineKind::Column, Icon::ChartBar),
        ("Win/Loss", "Win/Loss", SparklineKind::WinLoss, Icon::Sparkline),
    ] {
        let resp = big_button(ui, icon, label, tip, false);
        if kind == k {
            ui.painter().rect_stroke(resp.rect, 4.0, Stroke::new(1.5, Tokens::get(ui.ctx()).accent), StrokeKind::Inside);
        }
        if resp.clicked() && kind != k {
            let ty = match k {
                SparklineKind::Line => "line",
                SparklineKind::Column => "column",
                SparklineKind::WinLoss => "winLoss",
            };
            act(app, "sparkline.set", json!({"cell": cell, "type": ty}));
        }
    }
    sep(ui);
    ui.vertical(|ui| {
        for (label, key, v) in [("High Point", "high", high), ("Low Point", "low", low)] {
            let mut on = v;
            if ui.checkbox(&mut on, app.ui.language.tr(label)).changed() {
                act(app, "sparkline.set", json!({"cell": cell, key: on}));
            }
        }
    });
    ui.vertical(|ui| {
        // Excel greys Markers out for column and win/loss sparklines, which have no line to mark.
        let mut on = markers;
        if ui.add_enabled(kind == SparklineKind::Line, egui::Checkbox::new(&mut on, app.ui.language.tr("Markers"))).changed() {
            act(app, "sparkline.set", json!({"cell": cell, "markers": on}));
        }
    });
}

/// A Quick Layout's tooltip: the chart elements it shows, in the interface language.
pub(crate) fn quick_layout_tip(lang: crate::i18n::Language, l: &gridcraft_engine::cmd::extra::QuickLayout) -> String {
    use gridcraft_engine::cmd::extra::LayoutLabels;
    use gridcraft_engine::model::LegendPos;
    let mut parts: Vec<String> = Vec::new();
    if l.title {
        parts.push(lang.tr("Chart Title").into());
    }
    parts.push(
        lang.tr(match l.legend {
            LegendPos::None => "Legend: None",
            LegendPos::Top => "Legend: Top",
            LegendPos::Bottom => "Legend: Bottom",
            LegendPos::Left => "Legend: Left",
            LegendPos::Right => "Legend: Right",
        })
        .into(),
    );
    let label_parts: &[&str] = match l.labels {
        LayoutLabels::None => &[],
        LayoutLabels::Value => &["Value"],
        LayoutLabels::Percent => &["Percentage"],
        LayoutLabels::CategoryPercent => &["Category Name", "Percentage"],
        LayoutLabels::CategoryValue => &["Category Name", "Value"],
    };
    if !label_parts.is_empty() {
        let shown: Vec<&str> = label_parts.iter().map(|p| lang.tr(p)).collect();
        parts.push(format!("{} ({})", lang.tr("Data Labels"), shown.join(", ")));
    }
    if l.x_title {
        parts.push(lang.tr("Horizontal Axis Title").into());
    }
    if l.y_title {
        parts.push(lang.tr("Vertical Axis Title").into());
    }
    if l.gridlines {
        parts.push(lang.tr("Gridlines").into());
    }
    if l.data_table {
        parts.push(lang.tr("Data Table").into());
    }
    parts.join(", ")
}
