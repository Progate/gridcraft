use super::{
    CellRef, Chart, Color, EngineError, Json, LegendPos, RangeRef, Result, Session, Trendline, TrendlineKind, bad, bool_param, cell_param_on, edit,
    f64_param, json, split_sheet, str_param, u32_param,
};

fn find_chart(s: &Session, p: &Json) -> Result<(usize, usize)> {
    let d = s.doc()?;
    let si = d.wb.active_sheet();
    let sh = d.wb.sheet(si).ok_or(EngineError::NoDocument)?;
    let idx = match u32_param(p, "chart") {
        Some(id) => sh.charts.iter().position(|c| c.id == id),
        None => sh.charts.len().checked_sub(1),
    };
    idx.map(|i| (si, i)).ok_or_else(|| bad("chart", "no chart on this sheet"))
}

fn with_chart(s: &mut Session, p: &Json, f: impl FnOnce(&mut Chart) -> Result<()>) -> Result<Json> {
    let (si, ci) = find_chart(s, p)?;
    edit(s, |cx| {
        let c = cx.sheet_mut(si)?.charts.get_mut(ci).ok_or_else(|| bad("chart", "no such chart"))?;
        f(c)?;
        Ok(json!({"chart": c.id}))
    })
}

pub(super) fn add_element(s: &mut Session, p: &Json) -> Result<Json> {
    let el = str_param(p, "element").unwrap_or("title").to_string();
    let default_title = s.prefs.chart_title();
    let axis_title = s.prefs.axis_title();
    let on = bool_param(p, "on").unwrap_or(true);
    let pos = str_param(p, "position").map(str::to_string);
    with_chart(s, p, |c| {
        match el.as_str() {
            // Add Chart Element ▸ Chart Title ▸ Above Chart keeps a typed title and otherwise turns
            // the automatic title back on; None removes it (`autoTitleDeleted`).
            "title" => {
                if on {
                    // Above Chart or Centered Overlay: where the title sits.
                    match pos.as_deref() {
                        None | Some("above") => c.title_overlay = false,
                        Some("overlay") => c.title_overlay = true,
                        Some(other) => return Err(bad("chart.addElement", format!("a title position is above or overlay, not `{other}`"))),
                    }
                }
                if !on {
                    c.title = Some(String::new());
                    c.auto_title = false;
                } else if c.title.as_deref().is_none_or(str::is_empty) {
                    if crate::cmd::insert::is_chartex_kind(c.kind) {
                        c.title = Some(default_title.into());
                    } else {
                        c.title = None;
                        c.auto_title = true;
                    }
                }
            }
            "legend" => {
                c.legend = if !on {
                    LegendPos::None
                } else {
                    match pos.as_deref() {
                        Some("top") => LegendPos::Top,
                        Some("left") => LegendPos::Left,
                        Some("right") => LegendPos::Right,
                        _ => LegendPos::Bottom,
                    }
                }
            }
            "dataLabels" => c.data_labels = on,
            "dataTable" => crate::cmd::insert::update_data_table(c, p, Some(on))?,
            "gridlines" => c.gridlines = on,
            "axisTitles" => {
                c.x_title = on.then(|| c.x_title.clone().unwrap_or_else(|| axis_title.into()));
                c.y_title = on.then(|| c.y_title.clone().unwrap_or_else(|| axis_title.into()));
            }
            // Axis Titles ▸ Primary Horizontal / Primary Vertical: one axis's title, Axis Title
            // (軸ラベル) until it is typed over; a title it already has is kept.
            "axisTitle" => {
                if !has_axes(c) {
                    return Err(bad("chart.addElement", "this chart type has no axes"));
                }
                let title = match str_param(p, "axis") {
                    Some("horizontal") => c.axis_title_mut(true),
                    Some("vertical") => c.axis_title_mut(false),
                    // Secondary Vertical: the secondary value axis, which a series must use.
                    Some("secondaryVertical") if c.series.iter().any(|s| s.secondary) => &mut c.y2_title,
                    Some("secondaryVertical") => return Err(bad("chart.addElement", "no series is plotted on a secondary axis")),
                    _ => return Err(bad("chart.addElement", "`axis` must be horizontal, vertical or secondaryVertical")),
                };
                *title = on.then(|| title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| axis_title.into()));
            }
            "smooth" => {
                for s in &mut c.series {
                    s.smooth = on;
                }
            }
            "trendline" => {
                let only = u32_param(p, "series").map(|i| i as usize);
                if only.is_some_and(|i| i >= c.series.len()) {
                    return Err(bad("chart.addElement", "no such series"));
                }
                let targets: Vec<usize> = (0..c.series.len()).filter(|i| only.is_none_or(|o| o == *i)).collect();
                if !on {
                    for i in targets {
                        if let Some(se) = c.series.get_mut(i) {
                            se.trendlines.clear();
                        }
                    }
                    return Ok(());
                }
                let t = trendline_param(p)?;
                let eligible: Vec<usize> =
                    targets.into_iter().filter(|i| gridcraft_chart::supports_trendline(gridcraft_chart::series_plot_kind(c, *i))).collect();
                if eligible.is_empty() {
                    return Err(bad("chart.addElement", "trendlines need a 2-D column, bar, line, area, scatter or bubble series"));
                }
                for i in eligible {
                    if let Some(se) = c.series.get_mut(i) {
                        se.trendlines.push(t.clone());
                    }
                }
            }
            other => return Err(bad("chart.addElement", format!("unknown element `{other}`"))),
        }
        Ok(())
    })
}

/// Whether the chart draws axes that can carry a title: not a pie, doughnut, radar or the
/// newer charts without axes (treemap, sunburst, funnel).
pub fn has_axes(c: &Chart) -> bool {
    use gridcraft_model::ChartKind::*;
    !matches!(c.kind, Pie | Doughnut | Radar | Treemap | Sunburst | Funnel)
}

/// The trendline described by `chart.addElement`'s trendline parameters (Excel's Add Chart
/// Element ▸ Trendline choices, plus the Format Trendline options).
fn trendline_param(p: &Json) -> Result<Trendline> {
    let num = |k: &str| Trendline::clamp_forecast(f64_param(p, k).unwrap_or(0.0));
    let mut t = Trendline {
        forward: num("forward"),
        backward: num("backward"),
        display_equation: bool_param(p, "displayEquation").unwrap_or(false),
        display_r_squared: bool_param(p, "displayRSquared").unwrap_or(false),
        ..Trendline::default()
    };
    t.kind = match str_param(p, "type").unwrap_or("linear") {
        "linear" => TrendlineKind::Linear,
        // Excel's Linear Forecast: a linear trendline projected two periods ahead.
        "linearForecast" => {
            if t.forward == 0.0 {
                t.forward = 2.0;
            }
            TrendlineKind::Linear
        }
        "exponential" => TrendlineKind::Exponential,
        "logarithmic" => TrendlineKind::Logarithmic,
        "power" => TrendlineKind::Power,
        "polynomial" => TrendlineKind::Polynomial(u32_param(p, "order").unwrap_or(2).clamp(2, 6) as u8),
        "movingAverage" => TrendlineKind::MovingAverage(u32_param(p, "period").unwrap_or(2).clamp(2, 255)),
        other => return Err(bad("chart.addElement", format!("unknown trendline type `{other}`"))),
    };
    Ok(t)
}

/// Data labels a Quick Layout shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutLabels {
    None,
    /// The value (column, bar, line charts).
    Value,
    /// Pie: the percentage.
    Percent,
    /// Pie: the category name and the percentage.
    CategoryPercent,
    /// Pie: the category name and the value.
    CategoryValue,
}

/// One of Excel's Quick Layouts (Chart Design ▸ Quick Layout): the chart elements it shows. A
/// layout sets every element it covers, removing the others, as Excel does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuickLayout {
    pub title: bool,
    pub legend: LegendPos,
    pub labels: LayoutLabels,
    pub x_title: bool,
    pub y_title: bool,
    pub gridlines: bool,
    /// A data table with legend keys.
    pub data_table: bool,
}

const fn ql(title: bool, legend: LegendPos, labels: LayoutLabels, x_title: bool, y_title: bool, gridlines: bool, data_table: bool) -> QuickLayout {
    QuickLayout { title, legend, labels, x_title, y_title, gridlines, data_table }
}

/// Quick Layouts 1–11 of column, bar, line and area charts (and the other charts with axes), from
/// observing Excel's gallery. Excel's layout 2 also hides the value axis, 8 and 10 narrow the gap
/// between columns, and line charts have a twelfth layout (high-low lines); those parts are not
/// modelled, so 10 matches 1 here.
const AXIS_LAYOUTS: [QuickLayout; 11] = {
    use LayoutLabels::{None as NoLabels, Value};
    use LegendPos::{Bottom, None as NoLegend, Right, Top};
    [
        ql(true, Right, NoLabels, false, false, true, false),
        ql(true, Top, Value, false, false, false, false),
        ql(true, Bottom, NoLabels, false, false, true, false),
        ql(false, Bottom, Value, false, false, true, false),
        ql(true, NoLegend, NoLabels, false, true, true, true),
        ql(true, NoLegend, NoLabels, false, true, true, false),
        ql(false, Right, NoLabels, true, true, true, false),
        ql(true, NoLegend, NoLabels, true, true, true, false),
        ql(true, Right, NoLabels, true, true, true, false),
        ql(true, Right, NoLabels, false, false, true, false),
        ql(false, Right, NoLabels, false, false, true, false),
    ]
};

/// Quick Layouts 1–7 of pie and doughnut charts.
const PIE_LAYOUTS: [QuickLayout; 7] = {
    use LayoutLabels::{CategoryPercent, CategoryValue, None as NoLabels, Percent, Value};
    use LegendPos::{Bottom, None as NoLegend, Right, Top};
    [
        ql(true, NoLegend, CategoryPercent, false, false, false, false),
        ql(true, Top, Percent, false, false, false, false),
        ql(false, Bottom, NoLabels, false, false, false, false),
        ql(false, Bottom, Value, false, false, false, false),
        ql(true, NoLegend, CategoryValue, false, false, false, false),
        ql(true, Right, Percent, false, false, false, false),
        ql(false, Right, NoLabels, false, false, false, false),
    ]
};

/// The Quick Layouts Excel offers for a chart of this kind (layout `n` is entry `n - 1`).
pub fn quick_layouts(kind: gridcraft_model::ChartKind) -> &'static [QuickLayout] {
    use gridcraft_model::ChartKind as K;
    match kind {
        K::Pie | K::Doughnut => &PIE_LAYOUTS,
        K::Histogram | K::BoxWhisker | K::Waterfall | K::Funnel | K::Treemap | K::Sunburst => &AXIS_LAYOUTS[..0],
        _ => &AXIS_LAYOUTS,
    }
}

pub(super) fn quick_layout(s: &mut Session, p: &Json) -> Result<Json> {
    let n = u32_param(p, "layout").unwrap_or(1);
    let axis_title = s.prefs.axis_title();
    let (si, ci) = find_chart(s, p)?;
    let kind = s.doc()?.wb.sheet(si).and_then(|sh| sh.charts.get(ci)).map(|c| c.kind).ok_or_else(|| bad("chart", "no such chart"))?;
    let layouts = quick_layouts(kind);
    let Some(layout) = n.checked_sub(1).and_then(|i| layouts.get(i as usize)).copied() else {
        return Err(bad("chart.quickLayout", format!("this chart has layouts 1 to {}", layouts.len())));
    };
    with_chart(s, p, |c| {
        if !layout.title {
            c.title = Some(String::new());
            c.auto_title = false;
        } else if c.title.as_deref().is_none_or(str::is_empty) {
            // A typed title stays; otherwise the automatic title (series name or Chart Title).
            c.title = None;
            c.auto_title = true;
        }
        c.legend = layout.legend;
        c.data_labels = layout.labels != LayoutLabels::None;
        let parts = &mut c.label_parts;
        match layout.labels {
            LayoutLabels::None => {}
            LayoutLabels::Value => *parts = gridcraft_model::LabelParts::default(),
            LayoutLabels::Percent => *parts = gridcraft_model::LabelParts { value: false, percentage: true, ..Default::default() },
            LayoutLabels::CategoryPercent => {
                *parts = gridcraft_model::LabelParts { value: false, category_name: true, percentage: true, ..Default::default() }
            }
            LayoutLabels::CategoryValue => *parts = gridcraft_model::LabelParts { category_name: true, ..Default::default() },
        }
        c.gridlines = layout.gridlines;
        c.x_title = if layout.x_title { c.x_title.clone().filter(|t| !t.is_empty()).or_else(|| Some(axis_title.into())) } else { None };
        c.y_title = if layout.y_title { c.y_title.clone().filter(|t| !t.is_empty()).or_else(|| Some(axis_title.into())) } else { None };
        c.data_table = (layout.data_table && c.supports_data_table()).then(Default::default);
        Ok(())
    })
}

pub(super) fn change_colors(s: &mut Session, p: &Json) -> Result<Json> {
    let explicit: Option<Vec<Color>> =
        p.get("colors").and_then(Json::as_array).map(|a| a.iter().filter_map(|c| c.as_str().and_then(Color::from_hex)).collect());
    let palette = str_param(p, "palette").unwrap_or("colorful").to_string();
    with_chart(s, p, |c| {
        for (i, se) in c.series.iter_mut().enumerate() {
            se.color = match &explicit {
                Some(cols) if !cols.is_empty() => cols.get(i % cols.len()).copied(),
                _ => match palette.as_str() {
                    "monochrome" => Some(Color::Theme(4, (-400 + (i as i16 * 200)).clamp(-800, 800))),
                    p if p.starts_with("accent") => {
                        let k: u8 = p.trim_start_matches("accent").parse().unwrap_or(1);
                        Some(Color::Theme(3 + k.clamp(1, 6), (-500 + i as i16 * 250).clamp(-800, 800)))
                    }
                    _ => None,
                },
            };
        }
        Ok(())
    })
}

pub(super) fn select_data(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ci, chart) = select_data_preview(s, p)?;
    edit(s, |cx| {
        let c = cx.sheet_mut(si)?.charts.get_mut(ci).ok_or_else(|| bad("chart", "no such chart"))?;
        *c = chart;
        Ok(json!({"chart": c.id}))
    })
}

/// The chart as `chart.selectData` with these params would leave it, without changing it, and
/// where it is (sheet, index): the Select Data Source dialog lists the series it would have.
pub fn select_data_preview(s: &Session, p: &Json) -> Result<(usize, usize, Chart)> {
    let (si, ci) = find_chart(s, p)?;
    let d = s.doc()?;
    let mut c = d.wb.sheet(si).and_then(|sh| sh.charts.get(ci)).cloned().ok_or_else(|| bad("chart", "no such chart"))?;
    // The series keep running the way they do (Switch Row/Column, in this dialog in Excel) when
    // the data range changes; `byRows` sets it.
    let current_by_rows = crate::cmd::insert::series_in_rows(&c);
    // A new data range rebuilds the series, as in Excel's Select Data › Chart data range.
    let rebuilt = match str_param(p, "range") {
        Some(range) => {
            let range = range.trim().trim_start_matches('=');
            // One area, or several (Ctrl-selected), all on one sheet.
            let mut src_si = None;
            let mut areas = vec![];
            for part in gridcraft_core::split_areas(range) {
                let (sheet_name, body) = split_sheet(part.trim());
                let si = match sheet_name {
                    Some(n) => d.wb.sheet_index(&n).ok_or_else(|| bad("chart.selectData", "no such sheet"))?,
                    None => src_si.unwrap_or(d.wb.active_sheet()),
                };
                if src_si.is_some_and(|s| s != si) {
                    return Err(bad("chart.selectData", "the data must be on one sheet"));
                }
                src_si = Some(si);
                let r = RangeRef::parse(body.trim_start_matches('(').trim_end_matches(')')).ok_or_else(|| bad("chart.selectData", "not a range"))?;
                areas.push(r);
            }
            let src_si = src_si.ok_or_else(|| bad("chart.selectData", "not a range"))?;
            if areas.iter().fold(0_u64, |n, r| n.saturating_add(r.count())) > 1_000_000 {
                return Err(bad("chart.selectData", "range too large for a chart"));
            }
            let src = d.wb.sheet(src_si).ok_or(EngineError::NoDocument)?;
            let q = gridcraft_formula::quote_sheet(&src.name);
            let source = areas.iter().map(|r| format!("{q}!{}", r.a1())).collect::<Vec<_>>().join(",");
            let (series, by_rows) = match (areas.as_slice(), bool_param(p, "byRows")) {
                ([r], by_rows) => {
                    let by_rows = by_rows.unwrap_or(current_by_rows);
                    (crate::cmd::insert::series_from_range(&d.wb, src, *r, by_rows), by_rows)
                }
                _ => {
                    let (_, by_rows, series) = crate::cmd::insert::series_from_areas(&d.wb, src, &areas)?;
                    (series, by_rows)
                }
            };
            Some((series, source, by_rows))
        }
        None => None,
    };
    // Horizontal (Category) Axis Labels › Edit: one labels range for every series ("" clears).
    let categories = match str_param(p, "categories") {
        None => None,
        Some(t) if t.trim().trim_start_matches('=').is_empty() => Some(None),
        Some(t) => {
            let t = t.trim().trim_start_matches('=');
            let (sheet_name, body) = split_sheet(t);
            let si = match sheet_name {
                Some(n) => d.wb.sheet_index(&n).ok_or_else(|| bad("chart.selectData", "no such sheet"))?,
                None => d.wb.active_sheet(),
            };
            let r = RangeRef::parse(body).ok_or_else(|| bad("chart.selectData", "`categories` is not a range"))?;
            let name = d.wb.sheet(si).map(|sh| sh.name.clone()).ok_or(EngineError::NoDocument)?;
            let abs = |c: CellRef| format!("${}${}", gridcraft_core::col_to_letters(c.col), c.row + 1);
            Some(Some(format!("{}!{}:{}", gridcraft_formula::quote_sheet(&name), abs(r.start), abs(r.end))))
        }
    };
    let mut remove: Vec<usize> = p
        .get("removeSeries")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(Json::as_u64).filter_map(|i| usize::try_from(i).ok()).collect())
        .unwrap_or_default();
    remove.sort_unstable();
    remove.dedup();
    if rebuilt.is_none() && categories.is_none() && remove.is_empty() && p.get("byRows").is_none() {
        return Err(bad("chart.selectData", "missing `range`, `byRows`, `categories` or `removeSeries`"));
    }
    if let Some((series, source, by_rows)) = rebuilt {
        c.series = series;
        crate::cmd::insert::scatter_x_from_first(c.kind, &mut c.series);
        c.source = Some(source);
        c.by_rows = by_rows;
    } else if let Some(by_rows) = bool_param(p, "byRows").filter(|b| *b != current_by_rows) {
        // Switch Row/Column on the chart's own data range.
        let src = c.source.clone().or_else(|| crate::cmd::insert::series_block(&c.series).map(|(src, _)| src));
        let Some(src) = src else { return Err(bad("chart.selectData", "the chart's data isn't one block of cells")) };
        let (sheet_name, body) = split_sheet(&src);
        let src_si = sheet_name.and_then(|n| d.wb.sheet_index(&n)).unwrap_or(si);
        let r = RangeRef::parse(body).ok_or_else(|| bad("chart.selectData", "not a range"))?;
        if r.count() > 1_000_000 {
            return Err(bad("chart.selectData", "range too large for a chart"));
        }
        let src_sheet = d.wb.sheet(src_si).ok_or(EngineError::NoDocument)?;
        c.series = crate::cmd::insert::series_from_range(&d.wb, src_sheet, r, by_rows);
        crate::cmd::insert::scatter_x_from_first(c.kind, &mut c.series);
        c.source = Some(src);
        c.by_rows = by_rows;
    }
    for i in remove.iter().rev() {
        if *i < c.series.len() && c.series.len() > 1 {
            c.series.remove(*i);
        }
    }
    if let Some(cats) = categories {
        for se in &mut c.series {
            se.categories = cats.clone();
        }
    }
    Ok((si, ci, c))
}

/// Size of a chart on a new chart sheet (Excel's default chart sheet, in pixels at 96 dpi); the
/// chart sheet shows it zoomed to fit the window.
const CHART_SHEET_SIZE: (f32, f32) = (910.0, 661.0);
/// Size of a chart moved from a chart sheet onto a worksheet: a new chart's.
const EMBEDDED_SIZE: (f32, f32) = (480.0, 288.0);

/// Move Chart: to a new chart sheet (`newSheet`, or a `sheet` name no sheet has yet) or as an
/// object onto a worksheet (`sheet`). A chart sheet the chart leaves goes away, as in Excel.
pub(super) fn move_chart(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ci) = find_chart(s, p)?;
    let wb = &s.doc()?.wb;
    let from_chart_sheet = wb.sheet(si).is_some_and(|sh| sh.chart_sheet);
    let named = |k: &str| str_param(p, k).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
    // Some(name): a new chart sheet; None: onto worksheet `dest`.
    let (new_sheet, dest) = match (named("newSheet"), named("sheet")) {
        (Some(n), _) => (Some(n), si),
        (None, Some(n)) => match wb.sheet_index(&n) {
            Some(i) => (None, i),
            None => (Some(n), si),
        },
        (None, None) => (None, si),
    };
    if let Some(n) = &new_sheet {
        if wb.protected_structure {
            return Err(EngineError::Other("The workbook is protected and cannot be changed.".into()));
        }
        wb.check_sheet_name(n, None).map_err(EngineError::Other)?;
    } else if wb.sheet(dest).is_some_and(|sh| sh.chart_sheet) {
        if dest == si {
            return Ok(json!({"chart": wb.sheet(si).and_then(|sh| sh.charts.get(ci)).map(|c| c.id)}));
        }
        return Err(bad("chart.move", "a chart sheet holds one chart; choose a worksheet"));
    }
    if from_chart_sheet && new_sheet.is_some() {
        // Excel offers New sheet for a chart sheet's chart as a rename of the sheet.
        let n = new_sheet.unwrap_or_default();
        return s.execute("sheet.rename", json!({"sheet": si, "name": n}));
    }
    let at = if new_sheet.is_some() { None } else { cell_param_on(s, p, "at", dest)? };
    let d = s.doc_mut()?;
    let (active, inserted, removed) = crate::cmd::commit(d, |cx| {
        let charts = &mut cx.sheet_mut(si)?.charts;
        if ci >= charts.len() {
            return Err(bad("chart", "no such chart"));
        }
        let mut chart = charts.remove(ci);
        if let Some(name) = new_sheet {
            // The new chart sheet goes before the sheet the chart was on, as in Excel.
            chart.anchor = gridcraft_model::Anchor {
                cell: CellRef::new(0, 0),
                dx: 0.0,
                dy: 0.0,
                width: CHART_SHEET_SIZE.0,
                height: CHART_SHEET_SIZE.1,
                mode: gridcraft_model::AnchorMode::Absolute,
            };
            let mut sheet = gridcraft_model::Sheet::new(name);
            sheet.chart_sheet = true;
            sheet.show_gridlines = false;
            sheet.show_headings = false;
            sheet.charts.push(chart);
            cx.wb.sheets.insert(si, std::sync::Arc::new(sheet));
            for n in cx.wb.names.iter_mut() {
                if let Some(sc) = n.scope.as_mut()
                    && *sc >= si
                {
                    *sc += 1;
                }
            }
            cx.wb.set_active_sheet(si);
            *cx.sel = crate::Selection::default();
            cx.structural = true;
            return Ok((si, Some(si), None));
        }
        if from_chart_sheet {
            chart.anchor = gridcraft_model::Anchor {
                cell: at.unwrap_or_else(|| CellRef::new(1, 1)),
                width: EMBEDDED_SIZE.0,
                height: EMBEDDED_SIZE.1,
                ..Default::default()
            };
        } else if let Some(a) = at {
            chart.anchor.cell = a;
        }
        cx.sheet_mut(dest)?.charts.push(chart);
        if !from_chart_sheet {
            return Ok((cx.wb.active_sheet(), None, None));
        }
        // The emptied chart sheet goes, with the names scoped to it.
        cx.wb.sheets.remove(si);
        cx.wb.names.retain(|n| n.scope != Some(si));
        for n in cx.wb.names.iter_mut() {
            if let Some(sc) = n.scope.as_mut()
                && *sc > si
            {
                *sc -= 1;
            }
        }
        let dest = if dest > si { dest - 1 } else { dest };
        cx.wb.set_active_sheet(dest);
        *cx.sel = crate::Selection::default();
        cx.structural = true;
        Ok((dest, None, Some(si)))
    })?;
    if let Some(i) = inserted {
        d.sheet_selections.insert(i.min(d.sheet_selections.len()), crate::Selection::default());
    }
    if let Some(i) = removed
        && i < d.sheet_selections.len()
    {
        d.sheet_selections.remove(i);
    }
    let name = d.wb.sheet(active).map(|sh| sh.name.clone()).unwrap_or_default();
    Ok(json!({"sheet": name, "index": active}))
}

pub(super) fn format_series(s: &mut Session, p: &Json) -> Result<Json> {
    let idx = u32_param(p, "series").unwrap_or(0) as usize;
    let color = str_param(p, "color").and_then(Color::from_hex);
    if p.get("secondary").is_some_and(|v| !v.is_boolean()) {
        return Err(bad("chart.formatSelection", "`secondary` must be a boolean"));
    }
    let secondary = bool_param(p, "secondary");
    // Without `color` the fill goes back to Automatic, unless the call only moves the series to another axis.
    let set_color = p.get("color").is_some() || secondary.is_none();
    with_chart(s, p, |c| {
        if secondary.is_some() && idx >= c.series.len() {
            return Err(bad("chart.formatSelection", "no such series"));
        }
        if let Some(on) = secondary {
            crate::cmd::insert::make_combo(c)?;
            if let Some(se) = c.series.get_mut(idx) {
                se.secondary = on;
            }
        }
        if set_color && let Some(se) = c.series.get_mut(idx) {
            se.color = color;
        }
        Ok(())
    })
}

/// Most points a waterfall series can have set as totals.
const MAX_SUBTOTALS: usize = 10_000;

/// Format Data Point ▸ Set as total on a waterfall chart (also the data point's right-click menu).
pub(super) fn set_as_total(s: &mut Session, p: &Json) -> Result<Json> {
    let idx = u32_param(p, "series").unwrap_or(0) as usize;
    let point = u32_param(p, "point").ok_or_else(|| bad("chart.setAsTotal", "missing `point`"))?;
    if p.get("on").is_some_and(|v| !v.is_boolean()) {
        return Err(bad("chart.setAsTotal", "`on` must be a boolean"));
    }
    let on = bool_param(p, "on").unwrap_or(true);
    with_chart(s, p, |c| {
        if c.kind != gridcraft_model::ChartKind::Waterfall {
            return Err(bad("chart.setAsTotal", "only a waterfall chart has totals"));
        }
        let se = c.series.get_mut(idx).ok_or_else(|| bad("chart.setAsTotal", "no such series"))?;
        match (on, se.subtotals.binary_search(&point)) {
            (true, Err(at)) if se.subtotals.len() < MAX_SUBTOTALS => se.subtotals.insert(at, point),
            (true, Err(_)) => return Err(bad("chart.setAsTotal", "too many totals")),
            (false, Ok(at)) => {
                se.subtotals.remove(at);
            }
            _ => {}
        }
        Ok(())
    })
}

/// Format Data Labels ▸ Label Contains: the parts each data label shows. At least one of series
/// name, category name, value and percentage stays on, as in Excel.
pub(super) fn format_data_labels(s: &mut Session, p: &Json) -> Result<Json> {
    for k in ["seriesName", "categoryName", "value", "percentage", "leaderLines"] {
        if p.get(k).is_some_and(|v| !v.is_boolean()) {
            return Err(bad("chart.formatDataLabels", format!("`{k}` must be a boolean")));
        }
    }
    with_chart(s, p, |c| {
        let mut parts = c.label_parts;
        if !c.data_labels {
            parts = Default::default();
        }
        for (k, field) in [
            ("seriesName", &mut parts.series_name),
            ("categoryName", &mut parts.category_name),
            ("value", &mut parts.value),
            ("percentage", &mut parts.percentage),
            ("leaderLines", &mut parts.leader_lines),
        ] {
            if let Some(on) = bool_param(p, k) {
                *field = on;
            }
        }
        if !(parts.series_name || parts.category_name || parts.value || parts.percentage) {
            return Err(bad("chart.formatDataLabels", "a data label shows at least one of series name, category name, value or percentage"));
        }
        c.label_parts = parts;
        c.data_labels = true;
        Ok(())
    })
}
