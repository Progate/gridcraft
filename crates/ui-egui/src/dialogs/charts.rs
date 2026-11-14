use super::show::ok_cancel;
use super::{Color32, Dialog, Icon, Json, Sense, SheetApp, Stroke, StrokeKind, icons, json, vec2};

/// The chart categories of Insert Chart ▸ All Charts (すべてのグラフ) that GridCraft can make, in
/// Excel's order: `(id, label)`. Excel's Map, Surface and Templates aren't here.
pub(crate) const CATEGORIES: [(&str, &str); 15] = [
    ("column", "ChartCategory|Column"),
    ("line", "ChartCategory|Line"),
    ("pie", "ChartCategory|Pie"),
    ("bar", "ChartCategory|Bar"),
    ("area", "ChartCategory|Area"),
    ("scatter", "ChartCategory|X Y (Scatter)"),
    ("stock", "ChartCategory|Stock"),
    ("radar", "ChartCategory|Radar"),
    ("treemap", "ChartCategory|Treemap"),
    ("sunburst", "ChartCategory|Sunburst"),
    ("histogram", "ChartCategory|Histogram"),
    ("boxWhisker", "ChartCategory|Box & Whisker"),
    ("waterfall", "ChartCategory|Waterfall"),
    ("funnel", "ChartCategory|Funnel"),
    ("combo", "ChartCategory|Combo"),
];

/// The categories a PivotChart can take: Excel's Insert Chart for a PivotChart leaves out X Y
/// (scatter), stock and the Office 2016 kinds.
fn pivot_category(id: &str) -> bool {
    matches!(id, "column" | "line" | "pie" | "bar" | "area" | "radar" | "combo")
}

/// The chart types of a category, as `(type, subtype, label)` for `insert.chart`. A combo's
/// subtype names its preset ([`combo_preset`]).
pub(crate) fn subtypes(category: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    match category {
        "column" => &[("column", "", "Clustered Column"), ("column", "stacked", "Stacked Column"), ("column", "stacked100", "100% Stacked Column")],
        "line" => &[("line", "", "Line"), ("line", "stacked", "Stacked Line"), ("line", "markers", "Line with Markers")],
        "pie" => &[("pie", "", "Pie"), ("doughnut", "", "Doughnut")],
        "bar" => &[("bar", "", "Clustered Bar"), ("bar", "stacked", "Stacked Bar"), ("bar", "stacked100", "100% Stacked Bar")],
        "area" => &[("area", "", "Area"), ("area", "stacked", "Stacked Area"), ("area", "stacked100", "100% Stacked Area")],
        "scatter" => &[("scatter", "", "Scatter"), ("scatter", "lines", "Scatter with Lines"), ("bubble", "", "Bubble")],
        "stock" => &[("stock", "", "High-Low-Close")],
        "radar" => &[("radar", "", "Radar")],
        "treemap" => &[("treemap", "", "Treemap")],
        "sunburst" => &[("sunburst", "", "Sunburst")],
        "histogram" => &[("histogram", "", "Histogram")],
        "boxWhisker" => &[("boxWhisker", "", "Box & Whisker")],
        "waterfall" => &[("waterfall", "", "Waterfall")],
        "funnel" => &[("funnel", "", "Funnel")],
        "combo" => &[
            ("combo", "columnLine", "Clustered Column - Line"),
            ("combo", "columnLineSecondary", "Clustered Column - Line on Secondary Axis"),
            ("combo", "areaColumn", "Stacked Area - Clustered Column"),
            ("combo", "custom", "Custom Combination"),
        ],
        _ => &[],
    }
}

/// The All Charts category a chart type is listed under.
fn category_of(t: &str) -> &'static str {
    match t {
        "doughnut" => "pie",
        "bubble" => "scatter",
        _ => CATEGORIES.iter().find(|(id, _)| *id == t).map_or("column", |(id, _)| id),
    }
}

/// A combo preset's rows for `n` series, as `(type, subtype, secondary)`: Clustered Column -
/// Line makes the first series a column and the others lines (on the secondary axis for
/// Clustered Column - Line on Secondary Axis); Stacked Area - Clustered Column makes the first a
/// stacked area and the others columns. Custom Combination starts as Clustered Column - Line.
pub(crate) fn combo_preset(preset: &str, n: usize) -> Vec<(&'static str, &'static str, bool)> {
    (0..n)
        .map(|i| match (preset, i) {
            ("areaColumn", 0) => ("area", "stacked", false),
            ("areaColumn", _) => ("column", "", false),
            (_, 0) => ("column", "", false),
            ("columnLineSecondary", _) => ("line", "", true),
            _ => ("line", "", false),
        })
        .collect()
}

/// The chart types listed on the Recommended Charts tab.
fn recommendations(d: &Dialog) -> Vec<(String, String, String)> {
    let field = |x: &Json, k: &str| x.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    d.result
        .as_ref()
        .and_then(Json::as_array)
        .map(|a| a.iter().map(|x| (field(x, "type"), field(x, "subtype"), field(x, "label"))).collect())
        .unwrap_or_default()
}

fn text(d: &Dialog, k: &str) -> String {
    d.values.get(k).and_then(Json::as_str).unwrap_or("").to_string()
}

/// Opens Insert Chart: chart types for the selected data, with previews. `pivot` makes a
/// PivotChart of that PivotTable (All Charts only, as Excel's PivotChart dialog); `tab: "All
/// Charts"` with `category` opens on a category (More Column Charts…). `None` (with a message)
/// when the selection has no data to chart.
pub(super) fn open_insert_chart(app: &mut SheetApp, params: &Json) -> Option<Dialog> {
    let p = |k: &str| params.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let mut d = Dialog::custom("recommendedCharts", "Insert Chart", json!({}));
    if let Some(pv) = params.get("pivot").filter(|v| !v.is_null()) {
        d.values.insert("pivot".into(), pv.clone());
        d.tab = "All Charts".into();
    } else {
        let recs = match app.session.run("insert.chartRecommendations", json!({})) {
            Ok(r) => r,
            Err(e) => {
                app.message = Some(("Insert Chart".into(), crate::clean_error(&e)));
                return None;
            }
        };
        d.tab = if p("tab") == "All Charts" { "All Charts".into() } else { "Recommended Charts".into() };
        d.result = Some(recs);
        select_chart(&mut d, 0);
    }
    if d.tab == "All Charts" {
        let (t, sub, category) = (p("type"), p("subtype"), p("category"));
        let category = if category.is_empty() { category_of(&t).to_string() } else { category };
        select_type(app, &mut d, &category, &t, &sub);
    }
    Some(d)
}

/// Selects entry `i` of the Recommended Charts tab: OK inserts it.
pub(super) fn select_chart(d: &mut Dialog, i: usize) {
    let Some((t, sub, _)) = recommendations(d).into_iter().nth(i) else { return };
    d.list_index = i;
    d.values.insert("type".into(), json!(t));
    d.values.insert("subtype".into(), json!(sub));
}

/// Selects All Charts' category `category` and its chart type `(t, sub)` (its first when `t` is
/// empty or not one of it). Choosing a combo preset sets the series rows to it.
fn select_type(app: &SheetApp, d: &mut Dialog, category: &str, t: &str, sub: &str) {
    let list = subtypes(category);
    let (t, sub, _) = list.iter().copied().find(|(a, b, _)| *a == t && *b == sub).or_else(|| list.first().copied()).unwrap_or(("column", "", ""));
    d.values.insert("category".into(), json!(category));
    d.values.insert("type".into(), json!(t));
    d.values.insert("subtype".into(), json!(sub));
    let no_rows = d.values.get("rows").and_then(Json::as_array).is_none_or(Vec::is_empty);
    if t == "combo" && (sub != "custom" || no_rows) {
        let names = series_names(app, d);
        // Custom Combination starts from Clustered Column - Line.
        let preset = if sub == "custom" { "columnLine" } else { sub };
        let rows: Vec<Json> = combo_preset(preset, names.len())
            .into_iter()
            .zip(names)
            .map(|((ty, st, secondary), name)| json!({"name": name, "type": ty, "subtype": st, "secondary": secondary}))
            .collect();
        d.values.insert("rows".into(), json!(rows));
    }
}

/// The names of the series the chart would have, for the combo rows.
fn series_names(app: &SheetApp, d: &Dialog) -> Vec<String> {
    let mut p = json!({"type": "column"});
    if let Some(pv) = d.values.get("pivot").filter(|v| !v.is_null()) {
        p["pivot"] = pv.clone();
    }
    let Some(doc) = app.session.active() else { return Vec::new() };
    let built = if p.get("pivot").is_some() {
        gridcraft_engine::cmd::pivot::build_pivot_chart(&app.session, &p)
    } else {
        gridcraft_engine::cmd::insert::build_chart(&app.session, &p)
    };
    let Ok((sheet, chart)) = built else { return Vec::new() };
    gridcraft_chart::resolve(&doc.wb, sheet, &chart).series.into_iter().map(|s| s.name).collect()
}

/// The params of `insert.chart` (or `insert.pivotChart`) for the dialog's choice.
fn insert_params(d: &Dialog) -> Json {
    let mut p = json!({"type": text(d, "type"), "subtype": text(d, "subtype")});
    if text(d, "type") == "combo" {
        let rows: Vec<Json> = d
            .values
            .get("rows")
            .and_then(Json::as_array)
            .map(|a| a.iter().map(|r| json!({"type": r["type"], "subtype": r["subtype"], "secondary": r["secondary"]})).collect())
            .unwrap_or_default();
        p["combo"] = json!(rows);
    }
    if let Some(pv) = d.values.get("pivot").filter(|v| !v.is_null()) {
        p["pivot"] = pv.clone();
    }
    p
}

fn chart_icon(t: &str) -> Icon {
    match t {
        "column" | "bar" | "histogram" | "waterfall" | "funnel" | "boxWhisker" | "stock" => Icon::ChartBar,
        "line" | "area" | "radar" | "combo" => Icon::ChartLine,
        "pie" | "doughnut" | "sunburst" => Icon::ChartPie,
        "scatter" | "bubble" => Icon::ChartScatter,
        "treemap" => Icon::Theme,
        _ => Icon::Chart,
    }
}

/// Insert Chart (グラフの挿入): Recommended Charts (おすすめグラフ) and All Charts (すべてのグラフ),
/// whose category list, chart types and, for Combo (組み合わせ), each series' chart type and
/// Secondary Axis (第 2 軸) box are Excel's. With `pivot` it makes a PivotChart, and like Excel's
/// PivotChart dialog it has only All Charts, without the types a PivotChart can't be.
pub(super) fn recommended_charts(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let pivot = d.values.get("pivot").is_some_and(|v| !v.is_null());
    ui.horizontal(|ui| {
        let tabs: &[&str] = if pivot { &["All Charts"] } else { &["Recommended Charts", "All Charts"] };
        for tab in tabs {
            if ui.selectable_label(d.tab == *tab, lang.tr(tab)).clicked() && d.tab != *tab {
                d.tab = tab.to_string();
                if d.tab == "All Charts" {
                    let c = text(d, "category");
                    let c = if c.is_empty() { category_of(&text(d, "type")).to_string() } else { c };
                    let (t, sub) = (text(d, "type"), text(d, "subtype"));
                    select_type(app, d, &c, &t, &sub);
                } else {
                    select_chart(d, d.list_index);
                }
            }
        }
    });
    ui.separator();
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            egui::ScrollArea::vertical().id_salt("chart_choices").max_height(330.0).show(ui, |ui| {
                if d.tab == "All Charts" {
                    let current = text(d, "category");
                    for (id, label) in CATEGORIES.iter().filter(|(id, _)| !pivot || pivot_category(id)) {
                        let resp = ui
                            .horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                                icons::paint(ui.painter(), r.shrink(2.0), chart_icon(id), t.text);
                                ui.selectable_label(current == *id, lang.tr(label))
                            })
                            .inner;
                        if resp.clicked() && current != *id {
                            select_type(app, d, id, "", "");
                        }
                    }
                } else {
                    for (i, (kind, _, label)) in recommendations(d).iter().enumerate() {
                        let resp = ui
                            .horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                                icons::paint(ui.painter(), r.shrink(2.0), chart_icon(kind), t.text);
                                ui.selectable_label(i == d.list_index, lang.tr(label))
                            })
                            .inner;
                        if resp.clicked() {
                            select_chart(d, i);
                        }
                        if resp.double_clicked() {
                            select_chart(d, i);
                            *confirm = true;
                        }
                    }
                }
            });
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            let (ty, sub) = (text(d, "type"), text(d, "subtype"));
            let mut label = String::new();
            if d.tab == "All Charts" {
                // The category's chart types, as a row of buttons above the preview.
                let category = text(d, "category");
                ui.horizontal_wrapped(|ui| {
                    for (a, b, l) in subtypes(&category) {
                        let on = *a == ty && *b == sub;
                        let resp = ui.selectable_label(on, lang.tr(l));
                        if resp.clicked() && !on {
                            select_type(app, d, &category, a, b);
                        }
                        if resp.double_clicked() {
                            *confirm = true;
                        }
                    }
                });
                label = subtypes(&category).iter().find(|(a, b, _)| *a == ty && *b == sub).map(|c| lang.tr(c.2).to_string()).unwrap_or_default();
            } else if let Some(c) = recommendations(d).get(d.list_index) {
                label = lang.tr(&c.2).to_string();
            }
            ui.strong(label);
            let combo = ty == "combo";
            let (rect, _) = ui.allocate_exact_size(vec2(380.0, if combo { 170.0 } else { 228.0 }), Sense::hover());
            ui.painter().rect_filled(rect, 2.0, Color32::WHITE);
            ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
            let params = insert_params(d);
            if let Some(doc) = app.session.active() {
                let built = if pivot {
                    gridcraft_engine::cmd::pivot::build_pivot_chart(&app.session, &params)
                } else {
                    gridcraft_engine::cmd::insert::build_chart(&app.session, &params)
                };
                if let Ok((sheet, chart)) = built {
                    crate::chartview::paint_chart(ui.painter(), rect, &doc.wb, sheet, &chart);
                }
            }
            if combo {
                combo_rows(ui, d);
            }
        });
    });
    ok_cancel(ui, confirm, open);
    if *confirm {
        *confirm = false;
        let params = insert_params(d);
        let id = if pivot { "insert.pivotChart" } else { "insert.chart" };
        match app.run(id, params) {
            Ok(r) => {
                // The new chart is selected, as after inserting any object.
                app.select_object_id(r.get("chart").and_then(Json::as_u64).and_then(|id| u32::try_from(id).ok()));
                *open = false;
            }
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}

/// All Charts ▸ Combo: "Choose the chart type and axis for your data series:", a row per series
/// with its chart type and Secondary Axis box. Changing a row makes the choice a Custom
/// Combination, as in Excel.
fn combo_rows(ui: &mut egui::Ui, d: &mut Dialog) {
    let lang = crate::i18n::current(ui.ctx());
    let mut rows: Vec<Json> = d.values.get("rows").and_then(Json::as_array).cloned().unwrap_or_default();
    let mut changed = false;
    ui.add_space(4.0);
    ui.label(lang.tr("Choose the chart type and axis for your data series:"));
    egui::ScrollArea::vertical().id_salt("combo_rows").max_height(120.0).show(ui, |ui| {
        egui::Grid::new("gridcraft.insert_combo_rows").num_columns(3).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
            ui.strong(lang.tr("Series Name"));
            ui.strong(lang.tr("Chart Type"));
            ui.strong(lang.tr("Secondary Axis"));
            ui.end_row();
            for (i, row) in rows.iter_mut().enumerate() {
                let name = row["name"].as_str().unwrap_or("").to_string();
                ui.label(&name);
                let (t, sub) = (row["type"].as_str().unwrap_or("").to_string(), row["subtype"].as_str().unwrap_or("").to_string());
                let current = COMBO_TYPES.iter().find(|(k, s, _)| *k == t && *s == sub).map_or("", |c| c.2);
                craft_ui::ComboBox::from_id_salt(("gridcraft.insert_combo_type", i)).width(160.0).selected_text(lang.tr(current)).show_ui(ui, |ui| {
                    for (k, s, label) in COMBO_TYPES {
                        if ui.selectable_label(current == label, lang.tr(label)).clicked() && current != label {
                            row["type"] = json!(k);
                            row["subtype"] = json!(s);
                            changed = true;
                        }
                    }
                });
                let mut secondary = row["secondary"].as_bool().unwrap_or(false);
                let r = ui.checkbox(&mut secondary, "");
                r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, secondary, format!("{} {name}", lang.tr("Secondary Axis"))));
                if r.changed() {
                    row["secondary"] = json!(secondary);
                    changed = true;
                }
                ui.end_row();
            }
        });
    });
    if changed {
        d.values.insert("subtype".into(), json!("custom"));
    }
    d.values.insert("rows".into(), json!(rows));
}

/// Draws chart range, series keep/remove and category-range controls and runs `chart.selectData` when something changed.
pub(super) fn select_data(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let text = |d: &Dialog, k: &str| d.values.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let edit = |ui: &mut egui::Ui, d: &mut Dialog, key: &str, label: &str| {
        let label = ui.label(label);
        let mut v = text(d, key);
        if ui.add(egui::TextEdit::singleline(&mut v).desired_width(420.0)).labelled_by(label.id).changed() {
            d.values.insert(key.into(), json!(v));
        }
    };
    edit(ui, d, "range", "Chart data range:");
    ui.add_space(6.0);
    let by_rows = |d: &Dialog, k: &str| d.values.get(k).and_then(Json::as_bool).unwrap_or(false);
    // Excel's Switch Row/Column sits in this dialog: the series list shows the other way round
    // at once, and the chart changes on OK. A new data range keeps the orientation.
    if ui.button(crate::i18n::current(ui.ctx()).tr("Switch Row/Column")).clicked() {
        let flipped = !by_rows(d, "byRows");
        d.values.insert("byRows".into(), json!(flipped));
        d.values.insert("remove".into(), json!([]));
        let mut params = serde_json::Map::new();
        params.insert("chart".into(), d.values.get("chart").cloned().unwrap_or(Json::Null));
        params.insert("byRows".into(), json!(flipped));
        if text(d, "range") != text(d, "range0") {
            params.insert("range".into(), json!(text(d, "range")));
        }
        if let Ok((si, _, chart)) = gridcraft_engine::cmd::extra::select_data_preview(&app.session, &Json::Object(params))
            && let Some(doc) = app.session.active()
        {
            let names: Vec<String> = gridcraft_chart::resolve(&doc.wb, si, &chart).series.into_iter().map(|x| x.name).collect();
            d.values.insert("names".into(), json!(names));
        }
    }
    ui.add_space(6.0);
    ui.strong(crate::i18n::current(ui.ctx()).tr("Legend Entries (Series)"));
    let names: Vec<String> =
        d.values.get("names").and_then(Json::as_array).map(|a| a.iter().map(|n| n.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
    let mut remove: Vec<u64> =
        d.values.get("remove").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_u64).collect()).unwrap_or_default();
    let kept = names.len().saturating_sub(remove.len());
    egui::ScrollArea::vertical().id_salt("series").max_height(160.0).show(ui, |ui| {
        for (i, name) in names.iter().enumerate() {
            let i = i as u64;
            let removed = remove.contains(&i);
            ui.horizontal(|ui| {
                let label = if name.is_empty() { format!("Series{}", i + 1) } else { name.clone() };
                ui.label(if removed { egui::RichText::new(label).strikethrough().weak() } else { egui::RichText::new(label) });
                if removed {
                    if ui.small_button(crate::i18n::current(ui.ctx()).tr("Keep")).clicked() {
                        remove.retain(|x| *x != i);
                    }
                } else if ui.add_enabled(kept > 1, egui::Button::new(crate::i18n::current(ui.ctx()).tr("Remove")).small()).clicked() {
                    remove.push(i);
                }
            });
        }
    });
    d.values.insert("remove".into(), json!(remove));
    ui.add_space(6.0);
    edit(ui, d, "categories", "Horizontal (Category) Axis Labels:");
    ok_cancel(ui, confirm, open);
    if *confirm {
        *confirm = false;
        let mut params = serde_json::Map::new();
        params.insert("chart".into(), d.values.get("chart").cloned().unwrap_or(Json::Null));
        let range_changed = text(d, "range") != text(d, "range0");
        let switched = by_rows(d, "byRows") != by_rows(d, "byRows0");
        if range_changed || switched {
            params.insert("byRows".into(), json!(by_rows(d, "byRows")));
        }
        if range_changed {
            params.insert("range".into(), json!(text(d, "range")));
        } else if !remove.is_empty() {
            // A new range rebuilds the series; removals only apply to the current ones.
            params.insert("removeSeries".into(), json!(remove));
        }
        if text(d, "categories") != text(d, "categories0") || (range_changed && !text(d, "categories").is_empty()) {
            params.insert("categories".into(), json!(text(d, "categories")));
        }
        if params.len() == 1 {
            *open = false;
            return;
        }
        match app.run("chart.selectData", Json::Object(params)) {
            Ok(_) => *open = false,
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}

/// The chart types a combo chart's series can take in Change Chart Type ▸ Combo: `(type, subtype, label)`.
const COMBO_TYPES: [(&str, &str, &str); 8] = [
    ("column", "", "Clustered Column"),
    ("column", "stacked", "Stacked Column"),
    ("column", "stacked100", "100% Stacked Column"),
    ("line", "", "Line"),
    ("line", "markers", "Line with Markers"),
    ("line", "stacked", "Stacked Line"),
    ("area", "", "Area"),
    ("area", "stacked", "Stacked Area"),
];

/// The Combo row `(type, subtype)` for a series drawn as `kind`: its own when a combo can take
/// it, else Excel's default combination (the first series a column, the others lines).
pub(super) fn combo_row_type(kind: gridcraft_chart::ChartKind, i: usize) -> (&'static str, &'static str) {
    COMBO_TYPES
        .iter()
        .find(|(t, s, _)| gridcraft_engine::cmd::insert::combo_series_kind(t, s) == Some(kind))
        .map_or(if i == 0 { ("column", "") } else { ("line", "") }, |(t, s, _)| (*t, *s))
}

/// Change Chart Type ▸ Combo: each series' chart type and Secondary Axis box; OK runs
/// `chart.changeType` with the rows as `combo`.
pub(super) fn combo_chart(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let mut rows: Vec<Json> = d.values.get("rows").and_then(Json::as_array).cloned().unwrap_or_default();
    ui.label(lang.tr("Choose the chart type and axis for your data series:"));
    ui.add_space(4.0);
    egui::Grid::new("gridcraft.combo_rows").num_columns(3).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
        ui.strong(lang.tr("Series Name"));
        ui.strong(lang.tr("Chart Type"));
        ui.strong(lang.tr("Secondary Axis"));
        ui.end_row();
        for (i, row) in rows.iter_mut().enumerate() {
            let name = row["name"].as_str().unwrap_or("").to_string();
            ui.label(&name);
            let (t, sub) = (row["type"].as_str().unwrap_or("").to_string(), row["subtype"].as_str().unwrap_or("").to_string());
            let current = COMBO_TYPES.iter().find(|(k, s, _)| *k == t && *s == sub).map_or("", |c| c.2);
            craft_ui::ComboBox::from_id_salt(("gridcraft.combo_type", i)).width(170.0).selected_text(lang.tr(current)).show_ui(ui, |ui| {
                for (k, s, label) in COMBO_TYPES {
                    if ui.selectable_label(current == label, lang.tr(label)).clicked() {
                        row["type"] = json!(k);
                        row["subtype"] = json!(s);
                    }
                }
            });
            let mut secondary = row["secondary"].as_bool().unwrap_or(false);
            let r = ui.checkbox(&mut secondary, "");
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, secondary, format!("{} {name}", lang.tr("Secondary Axis"))));
            if r.changed() {
                row["secondary"] = json!(secondary);
            }
            ui.end_row();
        }
    });
    d.values.insert("rows".into(), json!(rows));
    ok_cancel(ui, confirm, open);
    if *confirm {
        *confirm = false;
        let combo: Vec<Json> = rows.iter().map(|r| json!({"type": r["type"], "subtype": r["subtype"], "secondary": r["secondary"]})).collect();
        let chart = d.values.get("chart").cloned().unwrap_or(Json::Null);
        match app.run("chart.changeType", json!({"chart": chart, "type": "combo", "combo": combo})) {
            Ok(_) => *open = false,
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}

/// Move Chart (グラフの移動): "Choose where you want the chart to be placed:" New sheet, with
/// the chart sheet's name, or Object in, with a worksheet.
pub(super) fn move_chart(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let text = |d: &Dialog, k: &str| d.values.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let mut place = text(d, "where");
    let mut name = text(d, "newSheet");
    let mut sheet = text(d, "sheet");
    let sheets: Vec<String> = d
        .values
        .get("sheets")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    ui.label(lang.tr("Choose where you want the chart to be placed:"));
    ui.add_space(4.0);
    egui::Grid::new("gridcraft.move_chart").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        if ui.radio(place == "newSheet", lang.tr("New sheet:")).clicked() {
            place = "newSheet".into();
        }
        let r = ui.add_enabled(place == "newSheet", egui::TextEdit::singleline(&mut name).desired_width(200.0));
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, lang.tr("New sheet:")));
        ui.end_row();
        if ui.radio(place == "object", lang.tr("Object in:")).clicked() {
            place = "object".into();
        }
        ui.add_enabled_ui(place == "object", |ui| {
            craft_ui::ComboBox::from_id_salt("gridcraft.move_chart_sheet").width(200.0).selected_text(sheet.clone()).show_ui(ui, |ui| {
                for n in &sheets {
                    if ui.selectable_label(*n == sheet, n).clicked() {
                        sheet = n.clone();
                    }
                }
            });
        });
        ui.end_row();
    });
    d.values.insert("where".into(), json!(place));
    d.values.insert("newSheet".into(), json!(name));
    d.values.insert("sheet".into(), json!(sheet));
    ok_cancel(ui, confirm, open);
    if *confirm {
        *confirm = false;
        let chart = d.values.get("chart").cloned().unwrap_or(Json::Null);
        let params = if place == "newSheet" { json!({"chart": chart, "newSheet": name}) } else { json!({"chart": chart, "sheet": sheet}) };
        match app.run("chart.move", params) {
            Ok(_) => {
                *open = false;
                app.selected_object = None;
            }
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}
