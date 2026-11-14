use super::create::{insert_pivot, source_param};
use super::{
    Anchor, CellRef, Chart, ChartKind, EngineError, Json, LegendPos, Result, Session, Workbook, anchor_mode_param, bad, bool_param, cell_param, edit,
    f64_param, find_pivot, json, other, pv, str_param, u32_param,
};
use gridcraft_model::PivotFieldButtons;

// ---------------------------------------------------------------- PivotChart

/// The PivotTable a PivotChart call targets: `pivot` may also be a cell of its report.
fn chart_pivot(s: &Session, p: &Json) -> Result<(usize, usize)> {
    if let Some(t) = str_param(p, "pivot") {
        let d = s.doc()?;
        let (sheet, addr) = match t.rsplit_once('!') {
            Some((sh, a)) => {
                let sh = sh.trim();
                let sh = gridcraft_core::unquote_sheet(sh);
                (d.wb.sheet_index(&sh), a)
            }
            None => (Some(d.wb.active_sheet()), t),
        };
        if let (Some(si), Some(c)) = (sheet, CellRef::parse(addr.trim().replace('$', "").as_str())) {
            let hit = d.wb.sheet(si).and_then(|sh| sh.pivots.iter().position(|pt| pt.last_range.is_some_and(|r| r.contains(c)) || pt.anchor == c));
            return hit.map(|pi| (si, pi)).ok_or_else(|| bad("insert.pivotChart", format!("{t} is not in a PivotTable")));
        }
    }
    find_pivot(s, p)
}

pub(crate) fn insert_pivot_chart(s: &mut Session, p: &Json) -> Result<Json> {
    // Insert ▸ PivotChart outside a PivotTable (Create PivotChart): a new PivotTable of `source`
    // at `destination`, then its PivotChart, as one step.
    if p.get("source").is_some_and(|v| !v.is_null()) {
        let mut q = serde_json::Map::new();
        for k in ["source", "destination", "name", "rows", "columns", "values", "filters", "lang"] {
            if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
                q.insert(k.into(), v.clone());
            }
        }
        let made = insert_pivot(s, &Json::Object(q))?;
        let name = made.get("name").and_then(Json::as_str).unwrap_or_default().to_string();
        let mut rest = p.clone();
        if let Some(m) = rest.as_object_mut() {
            m.remove("source");
            m.insert("pivot".into(), json!(name));
        }
        return insert_pivot_chart(s, &rest);
    }
    let (si, chart) = build_pivot_chart(s, p)?;
    let (id, n) = (chart.id, chart.series.len());
    let pivot_name = chart.pivot.as_deref().and_then(pv::parse_chart_link).map(|(_, name)| name).unwrap_or_default();
    edit(s, |cx| {
        cx.sheet_mut(si)?.charts.push(chart);
        Ok(json!({"chart": id, "pivot": pivot_name, "series": n}))
    })
}

/// The PivotChart `insert.pivotChart` would add for these params (`pivot`, `type`, `combo`…),
/// without adding it: the sheet it goes on and the chart (previews in the Insert Chart dialog).
pub fn build_pivot_chart(s: &Session, p: &Json) -> Result<(usize, Chart)> {
    let (si, pi) = chart_pivot(s, p)?;
    let d = s.doc()?;
    let sh = d.wb.sheet(si).ok_or(EngineError::NoDocument)?;
    let pt = sh.pivots.get(pi).ok_or_else(|| other("There's no such PivotTable."))?;
    // An empty PivotTable gets an empty PivotChart, as in Excel: its series come with the fields.
    let area = pt.last_range.unwrap_or(gridcraft_core::RangeRef::cell(pt.anchor));
    let out = pv::compute(&d.wb, pt, si).map_err(other)?;
    let series = if out.chart_rows.is_empty() || out.chart_cols.is_empty() { Vec::new() } else { pv::chart_series(&sh.name, area.start, &out) };
    let t = str_param(p, "type").unwrap_or("column");
    let kind = crate::cmd::insert::chart_kind(t, str_param(p, "subtype").unwrap_or(""));
    if crate::cmd::insert::is_chartex_kind(kind)
        || matches!(kind, ChartKind::Scatter | ChartKind::ScatterLines | ChartKind::Bubble | ChartKind::Stock)
    {
        // Excel offers none of these for a PivotChart: their data isn't a report's rows and columns.
        return Err(bad(
            "insert.pivotChart",
            "a PivotChart can't be an X Y (scatter), bubble, stock, treemap, sunburst, histogram, box and whisker, waterfall or funnel chart",
        ));
    }
    let at =
        cell_param(s, p, "at")?.unwrap_or_else(|| CellRef::new(area.start.row, area.end.col.saturating_add(2).min(gridcraft_core::MAX_COLS - 1)));
    let title = match p.get("title") {
        Some(Json::String(t)) => Some(t.clone()),
        Some(Json::Null) => None,
        // Excel's default PivotChart title, in the interface language: Total (集計) over one series,
        // else Chart Title (グラフ タイトル).
        _ => Some(if series.len() == 1 { s.prefs.pivot_total().to_string() } else { s.prefs.chart_title().to_string() }),
    };
    let id = d.wb.next_object_id();
    let link = pv::chart_link(&sh.name, &pt.name);
    let mut chart = Chart {
        id,
        kind,
        anchor: Anchor {
            cell: at,
            dx: 0.0,
            dy: 0.0,
            width: f64_param(p, "width").unwrap_or(480.0).clamp(10.0, 10_000.0) as f32,
            height: f64_param(p, "height").unwrap_or(288.0).clamp(10.0, 10_000.0) as f32,
            mode: anchor_mode_param(p).unwrap_or_default(),
        },
        title,
        series,
        // A PivotChart's legend is at the right, under its legend field buttons, as in Excel.
        legend: LegendPos::Right,
        data_labels: false,
        data_table: None,
        gridlines: !matches!(kind, ChartKind::Pie | ChartKind::Doughnut),
        hole_size: 75,
        first_slice_angle: 0,
        style: 1,
        x_title: None,
        y_title: None,
        source: None,
        by_rows: false,
        pivot: Some(link),
        alt_text: Default::default(),
        label_parts: Default::default(),
        auto_title: false,
        title_overlay: false,
        chart_style: None,
        y2_title: None,
        bins: None,
        pivot_buttons: Default::default(),
    };
    if let Some(rows) = p.get("combo").filter(|v| !v.is_null()) {
        crate::cmd::insert::apply_combo(&mut chart, rows)?;
    }
    Ok((si, chart))
}

/// `chart.fieldButtons`: the field buttons a PivotChart draws (PivotChart Analyze ▸ Field Buttons).
/// `all` shows (true) or hides (false, Hide All) every kind; `reportFilter`, `legend`, `axis` and
/// `value` set one kind. With none of them it toggles, as the button's upper half does: a chart
/// showing any button hides them all, one showing none shows them all.
pub(super) fn field_buttons(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "chart.fieldButtons";
    let d = s.doc()?;
    let si = d.wb.active_sheet();
    let sh = d.wb.sheet(si).ok_or(EngineError::NoDocument)?;
    let ci = match u32_param(p, "chart") {
        Some(id) => sh.charts.iter().position(|c| c.id == id).ok_or_else(|| bad(CMD, "no such chart"))?,
        None => sh.charts.iter().rposition(|c| c.pivot.is_some()).ok_or_else(|| bad(CMD, "this sheet has no PivotChart"))?,
    };
    let Some(ch) = sh.charts.get(ci) else { return Err(bad(CMD, "no such chart")) };
    if ch.pivot.is_none() {
        return Err(other("Field buttons are only on PivotCharts."));
    }
    let mut b = ch.pivot_buttons;
    let kinds = ["reportFilter", "legend", "axis", "value"];
    match bool_param(p, "all") {
        Some(true) => b = PivotFieldButtons::ALL,
        Some(false) => b = PivotFieldButtons::NONE,
        None if kinds.iter().all(|k| p.get(*k).is_none_or(Json::is_null)) => {
            b = if b.any() { PivotFieldButtons::NONE } else { PivotFieldButtons::ALL }
        }
        None => {}
    }
    for (k, slot) in kinds.into_iter().zip([&mut b.report_filter, &mut b.legend, &mut b.axis, &mut b.value]) {
        if let Some(v) = bool_param(p, k) {
            *slot = v;
        }
    }
    let id = ch.id;
    edit(s, |cx| {
        if let Some(c) = cx.sheet_mut(si)?.charts.get_mut(ci) {
            c.pivot_buttons = b;
        }
        Ok(json!({"chart": id, "reportFilter": b.report_filter, "legend": b.legend, "axis": b.axis, "value": b.value}))
    })
}

// ---------------------------------------------------------------- recommended

fn suggestions(wb: &Workbook, src: &pv::Source) -> Vec<Json> {
    let n = src.headers.len();
    let h = |i: usize| src.headers.get(i).cloned().unwrap_or_default();
    let distinct = |i: usize| pv::distinct_labels(wb, src, i, pv::Grouping::None, 1001).len();
    let records = src.rows.len().max(1);
    let is_num = |i: usize| src.numeric.get(i).copied().unwrap_or(false);
    let is_date = |i: usize| src.dates.get(i).copied().unwrap_or(false);
    // Category columns: text with repeats.
    let cats: Vec<usize> = (0..n)
        .filter(|&i| {
            !is_num(i) && {
                let d = distinct(i);
                d >= 1 && (d < records || records <= 2) && d <= 1000
            }
        })
        .collect();
    let nums: Vec<usize> = (0..n).filter(|&i| is_num(i) && !is_date(i)).collect();
    let dates: Vec<usize> = (0..n).filter(|&i| is_date(i)).collect();
    let mut out = vec![];
    let mut push = |title: String, rows: Vec<Json>, cols: Vec<Json>, vals: Vec<Json>| {
        if out.len() < 5 {
            let idx = out.len();
            out.push(json!({"index": idx, "title": title, "rows": rows, "columns": cols, "values": vals}));
        }
    };
    if let (Some(&c), Some(&v)) = (cats.first(), nums.first()) {
        push(format!("Sum of {} by {}", h(v), h(c)), vec![json!(h(c))], vec![], vec![json!({"field": h(v), "func": "sum"})]);
    }
    if let (Some(&c), Some(&c2), Some(&v)) = (cats.first(), cats.get(1), nums.first()) {
        push(
            format!("Sum of {} by {} and {}", h(v), h(c), h(c2)),
            vec![json!(h(c))],
            vec![json!(h(c2))],
            vec![json!({"field": h(v), "func": "sum"})],
        );
    }
    if let (Some(&d), Some(&v)) = (dates.first(), nums.first()) {
        push(
            format!("Sum of {} by {} (quarters)", h(v), h(d)),
            vec![json!({"field": h(d), "group": "quarters"})],
            vec![],
            vec![json!({"field": h(v), "func": "sum"})],
        );
    }
    if let (Some(&c), Some(&v), Some(&v2)) = (cats.first(), nums.first(), nums.get(1)) {
        push(
            format!("Sum of {} and {} by {}", h(v), h(v2), h(c)),
            vec![json!(h(c))],
            vec![],
            vec![json!({"field": h(v), "func": "sum"}), json!({"field": h(v2), "func": "sum"})],
        );
    }
    if let Some(&c) = cats.first() {
        push(format!("Count of {} by {}", h(c), h(c)), vec![json!(h(c))], vec![], vec![json!({"field": h(c), "func": "count"})]);
    }
    if let (Some(&c), Some(&v)) = (cats.get(1), nums.first()) {
        push(format!("Average of {} by {}", h(v), h(c)), vec![json!(h(c))], vec![], vec![json!({"field": h(v), "func": "average"})]);
    }
    if cats.is_empty()
        && dates.is_empty()
        && let Some(&v) = nums.first()
    {
        push(format!("Sum of {}", h(v)), vec![], vec![], vec![json!({"field": h(v), "func": "sum"})]);
    }
    out
}

pub(super) fn recommended(s: &mut Session, p: &Json) -> Result<Json> {
    let source = source_param(s, p)?;
    let active = s.doc()?.wb.active_sheet();
    let src = pv::read_source(&s.doc()?.wb, &source, active).map_err(other)?;
    let list = suggestions(&s.doc()?.wb, &src);
    let Some(apply) = p.get("apply").and_then(Json::as_u64) else {
        return Ok(json!({"source": source, "suggestions": list}));
    };
    let pick = list.get(apply as usize).cloned().ok_or_else(|| bad("insert.recommendedPivotTables", "no suggestion with that index"))?;
    let mut q = json!({
        "source": source,
        "rows": pick.get("rows").cloned().unwrap_or(json!([])),
        "columns": pick.get("columns").cloned().unwrap_or(json!([])),
        "values": pick.get("values").cloned().unwrap_or(json!([])),
    });
    if let (Some(m), Some(d)) = (q.as_object_mut(), p.get("destination")) {
        m.insert("destination".into(), d.clone());
    }
    if let (Some(m), Some(r)) = (q.as_object_mut(), p.get("replace")) {
        m.insert("replace".into(), r.clone());
    }
    let mut res = insert_pivot(s, &q)?;
    if let Some(m) = res.as_object_mut() {
        m.insert("suggestion".into(), pick);
    }
    Ok(res)
}
