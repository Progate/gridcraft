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
    let on = bool_param(p, "on").unwrap_or(true);
    let pos = str_param(p, "position").map(str::to_string);
    with_chart(s, p, |c| {
        match el.as_str() {
            "title" => c.title = if on { c.title.clone().filter(|t| !t.is_empty()).or(Some("Chart Title".into())) } else { Some(String::new()) },
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
                c.x_title = on.then(|| c.x_title.clone().unwrap_or_else(|| "Axis Title".into()));
                c.y_title = on.then(|| c.y_title.clone().unwrap_or_else(|| "Axis Title".into()));
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

pub(super) fn quick_layout(s: &mut Session, p: &Json) -> Result<Json> {
    let n = u32_param(p, "layout").unwrap_or(1);
    with_chart(s, p, |c| {
        let (legend, labels, grid, axes) = match n {
            2 => (LegendPos::Top, true, false, false),
            3 => (LegendPos::Bottom, false, true, false),
            4 => (LegendPos::Bottom, true, false, false),
            5 => (LegendPos::None, false, true, true),
            6 => (LegendPos::Right, false, true, true),
            _ => (LegendPos::Right, false, true, false),
        };
        c.legend = legend;
        c.data_labels = labels;
        c.gridlines = grid;
        if axes {
            c.x_title.get_or_insert_with(|| "Axis Title".into());
            c.y_title.get_or_insert_with(|| "Axis Title".into());
        } else {
            c.x_title = None;
            c.y_title = None;
        }
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
    let d = s.doc()?;
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
                    let by_rows = by_rows.unwrap_or(r.width() > r.height() + 1);
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
    if rebuilt.is_none() && categories.is_none() && remove.is_empty() {
        return Err(bad("chart.selectData", "missing `range`, `categories` or `removeSeries`"));
    }
    with_chart(s, p, move |c| {
        if let Some((series, source, by_rows)) = rebuilt {
            c.series = series;
            crate::cmd::insert::scatter_x_from_first(c.kind, &mut c.series);
            c.source = Some(source);
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
        Ok(())
    })
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
