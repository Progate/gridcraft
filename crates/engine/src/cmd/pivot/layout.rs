use super::create::source_param;
use super::fields::{canonical, in_axes, insert_axis_field, shift_values, update};
use super::params::{group_name, group_param, layout_name};
use super::{
    Chart, Json, PivotDateGroup, PivotField, PivotFilter, PivotLayout, PivotRangeGroup, PivotSort, PivotTable, Result, Session, bad, bool_param,
    edit, find_pivot, json, ok, other, pv, str_param, u32_param,
};

pub(super) fn filter(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.filter";
    let field = str_param(p, "field").ok_or_else(|| bad(C, "missing `field`"))?.to_string();
    let selected: Option<Vec<String>> = match p.get("selected") {
        None | Some(Json::Null) => None,
        Some(Json::Array(a)) => {
            if a.len() > 100_000 {
                return Err(bad(C, "too many items"));
            }
            let v: Vec<String> = a
                .iter()
                .map(|x| match x {
                    Json::String(s) => s.clone(),
                    Json::Null => "(blank)".into(),
                    other => other.to_string(),
                })
                .collect();
            if v.is_empty() {
                return Err(other("You must select at least one item."));
            }
            Some(v)
        }
        Some(Json::String(s)) => Some(vec![s.clone()]),
        Some(_) => return Err(bad(C, "`selected` is a list of item labels or null")),
    };
    update(s, p, |pt, src, _| {
        let field = canonical(C, src, &field)?;
        let i = pt.filters.iter().position(|f| f.source_col.eq_ignore_ascii_case(&field));
        match (i, selected) {
            (Some(i), None) => {
                if in_axes(pt, &field) {
                    pt.filters.remove(i);
                } else if let Some(f) = pt.filters.get_mut(i) {
                    f.selected = None;
                }
            }
            (Some(i), Some(sel)) => {
                if let Some(f) = pt.filters.get_mut(i) {
                    f.selected = Some(sel);
                }
            }
            (None, Some(sel)) => pt.filters.push(PivotFilter { source_col: field.clone(), selected: Some(sel) }),
            (None, None) => {}
        }
        Ok(json!({"field": field}))
    })
}

fn axis_field<'a>(cmd: &str, pt: &'a mut PivotTable, field: &str) -> Result<&'a mut PivotField> {
    let (rows, i) = pv::axis_position(pt, field).ok_or_else(|| bad(cmd, format!("`{field}` isn't in the rows or columns area")))?;
    (if rows { &mut pt.rows } else { &mut pt.columns }).get_mut(i).ok_or_else(|| bad(cmd, format!("`{field}` isn't in the rows or columns area")))
}

pub(super) fn sort(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.sort";
    let field = str_param(p, "field").ok_or_else(|| bad(C, "missing `field`"))?.to_string();
    let order = match str_param(p, "order").unwrap_or("asc").to_ascii_lowercase().as_str() {
        "asc" | "ascending" | "a-z" => PivotSort::Asc,
        "desc" | "descending" | "z-a" => PivotSort::Desc,
        "none" | "manual" | "source" => PivotSort::None,
        o => return Err(bad(C, format!("unknown order `{o}` (asc|desc|none)"))),
    };
    update(s, p, |pt, _, _| {
        axis_field(C, pt, &field)?.sort = order;
        Ok(json!({}))
    })
}

/// A number param given as a JSON number or numeric text; `Ok(None)` when absent, empty or `auto`.
fn num_or_auto(cmd: &str, p: &Json, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Number(n)) => n.as_f64().map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number"))),
        Some(Json::String(t)) if t.trim().is_empty() || t.trim().eq_ignore_ascii_case("auto") => Ok(None),
        Some(Json::String(t)) => {
            t.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number")))
        }
        Some(_) => Err(bad(cmd, format!("`{key}` must be a number"))),
    }
}

/// The levels a `by` parameter names (`"months"` or `["years", "quarters", "months"]`), coarse to
/// fine; empty = ungrouped.
fn group_levels(cmd: &str, by: Option<&Json>) -> Result<Vec<PivotDateGroup>> {
    let names: Vec<&str> = match by {
        None | Some(Json::Null) => vec!["months"],
        Some(Json::String(s)) => s.split(',').collect(),
        Some(Json::Array(a)) => a.iter().filter_map(Json::as_str).collect(),
        Some(_) => return Err(bad(cmd, "`by` is a grouping or a list of groupings")),
    };
    let mut v = Vec::new();
    for n in names {
        let g = group_param(Some(n))
            .ok_or_else(|| bad(cmd, format!("unknown grouping `{n}` (seconds|minutes|hours|days|months|quarters|years|none)")))?;
        if g != PivotDateGroup::None && !v.contains(&g) {
            v.push(g);
        }
    }
    v.sort_by_key(|g: &PivotDateGroup| g.rank());
    Ok(v)
}

/// A date given as a serial or as typed text (`2025/4/1`, `2025-04-01`); `None` for "auto" or blank.
fn date_value(cmd: &str, p: &Json, key: &str, sys: gridcraft_core::DateSystem) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Number(n)) => Ok(n.as_f64()),
        Some(Json::String(t)) if t.trim().is_empty() || t.trim().eq_ignore_ascii_case("auto") => Ok(None),
        Some(Json::String(t)) => match gridcraft_core::parse::parse_input(t.trim(), sys).value {
            gridcraft_core::Value::Number(n) => Ok(Some(n)),
            _ => Err(bad(cmd, format!("`{key}`: `{t}` isn't a date"))),
        },
        Some(_) => Err(bad(cmd, format!("`{key}` is a date"))),
    }
}

/// Replaces every level of the axis field at (`rows`, `i`) with `levels` (coarse to fine; none =
/// ungrouped), in its place, as Excel's Grouping dialog does: Years + Quarters + Months on a date
/// field gives the fields Years, Quarters and the date field itself (by month), in that order.
pub(super) fn set_date_levels(pt: &mut PivotTable, rows: bool, i: usize, levels: &[PivotDateGroup], range: (Option<f64>, Option<f64>, u32)) {
    let base = {
        let list = if rows { &pt.rows } else { &pt.columns };
        match list.get(i) {
            Some(f) => f.source_col.clone(),
            None => return,
        }
    };
    let mut at: Option<(bool, usize)> = None;
    let mut template: Option<PivotField> = None;
    for r in [rows, !rows] {
        loop {
            let list = if r { &mut pt.rows } else { &mut pt.columns };
            let Some(j) = list.iter().position(|f| f.source_col.eq_ignore_ascii_case(&base)) else { break };
            let f = list.remove(j);
            shift_values(pt, r, j, false);
            if r == rows && at.is_none() {
                at = Some((r, j));
            }
            template.get_or_insert(f);
        }
    }
    let (r, mut j) = at.unwrap_or((rows, i));
    let template = template.unwrap_or_default();
    let (start, end, days) = range;
    let make = |by: PivotDateGroup| PivotField {
        source_col: base.clone(),
        sort: template.sort,
        date_group: by,
        collapsed_items: vec![],
        date_start: if by == PivotDateGroup::None { None } else { start },
        date_end: if by == PivotDateGroup::None { None } else { end },
        date_days: if levels == [PivotDateGroup::Days] { days } else { 0 },
        range_group: None,
    };
    let fields: Vec<PivotField> = if levels.is_empty() { vec![make(PivotDateGroup::None)] } else { levels.iter().map(|g| make(*g)).collect() };
    for f in fields {
        insert_axis_field(pt, r, f, Some(j));
        j += 1;
    }
    // Hidden items name the old items, which regrouping replaces.
    pt.filters.retain(|f| !f.source_col.eq_ignore_ascii_case(&base));
}

/// Settings `pivot.group` takes; a call with none of them (only which field) opens the Grouping dialog.
const GROUP_SETTINGS: [&str; 7] = ["by", "start", "end", "interval", "autoStart", "autoEnd", "days"];

pub(super) fn group(s: &mut Session, p: &Json) -> Result<Json> {
    let field = match str_param(p, "field") {
        Some(f) => f.to_string(),
        None => field_at_active_cell(s, p)?,
    };
    if !GROUP_SETTINGS.iter().any(|k| p.get(*k).is_some()) {
        return group_dialog(s, p, &field);
    }
    let range = str_param(p, "by").is_some_and(|b| b.eq_ignore_ascii_case("range"))
        || (p.get("by").is_none() && GROUP_SETTINGS.iter().skip(1).take(5).any(|k| p.get(*k).is_some()));
    if range {
        return group_range(s, p, field);
    }
    group_dates(s, p, field)
}

/// Grouping ▸ By for a date field: one level or several (Years, Quarters, Months…), Starting at /
/// Ending at dates and Number of days.
fn group_dates(s: &mut Session, p: &Json, field: String) -> Result<Json> {
    const C: &str = "pivot.group";
    let sys = s.doc()?.wb.date_system;
    let levels = group_levels(C, p.get("by"))?;
    // A ticked Auto box wins over the date beside it, as in the Grouping dialog.
    let mut start = if bool_param(p, "autoStart") == Some(true) { None } else { date_value(C, p, "start", sys)? };
    let end = if bool_param(p, "autoEnd") == Some(true) { None } else { date_value(C, p, "end", sys)? };
    let days = u32_param(p, "days").unwrap_or(0);
    if let (Some(a), Some(b)) = (start, end)
        && b < a
    {
        return Err(other("The Ending At value must be greater than the Starting At value."));
    }
    let lang = str_param(p, "lang").map(str::to_string);
    update(s, p, |pt, src, _| {
        let (rows, i) = pv::axis_position(pt, &field).ok_or_else(|| bad(C, format!("`{field}` isn't in the rows or columns area")))?;
        let base = (if rows { &pt.rows } else { &pt.columns }).get(i).map(|f| f.source_col.clone()).unwrap_or_default();
        let c = src.col(&base).ok_or_else(|| bad(C, format!("no field `{base}` in the source data")))?;
        if !levels.is_empty() && !src.numeric.get(c).copied().unwrap_or(false) {
            return Err(other("Cannot group that selection."));
        }
        // Day ranges count from Starting at: pin it to the earliest date, as Excel saves it.
        if days > 1 && levels == [PivotDateGroup::Days] && start.is_none() {
            start = pv::source_date_range(src, c).map(|(a, _)| a.floor());
        }
        if let Some(l) = &lang {
            pt.lang = l.clone();
        }
        set_date_levels(pt, rows, i, &levels, (start, end, days));
        Ok(json!({"levels": levels.iter().map(|g| g.name()).collect::<Vec<_>>()}))
    })
}

/// The row or column field whose item (or, in Outline and Tabular layouts, whose header) the active
/// cell shows: what right-click ▸ Group… and Ungroup… act on.
fn field_at_active_cell(s: &Session, p: &Json) -> Result<String> {
    const NOT_A_LABEL: &str = "Select a row or column label of the PivotTable to group.";
    let (si, pi) = find_pivot(s, p)?;
    let d = s.doc()?;
    let sh = d.wb.sheet(si).ok_or_else(|| other("There's no such sheet."))?;
    let pt = sh.pivots.get(pi).ok_or_else(|| other("There's no such PivotTable."))?;
    let at = d.selection.active;
    let origin = pt.last_range.filter(|r| r.contains(at)).map(|r| r.start).ok_or_else(|| other(NOT_A_LABEL))?;
    let out = pv::compute(&d.wb, pt, si).map_err(other)?;
    let kind = out.grid.get((at.row - origin.row) as usize).and_then(|line| line.get((at.col - origin.col) as usize)).map(|c| c.kind);
    let fields: Vec<&PivotField> = match kind {
        Some(pv::CellKind::RowLabel) => pt.rows.iter().collect(),
        Some(pv::CellKind::ColLabel) => pt.columns.iter().collect(),
        Some(pv::CellKind::Header) => pt.rows.iter().chain(pt.columns.iter()).collect(),
        _ => vec![],
    };
    let text = crate::display::cell_text(&d.wb, sh, at);
    if let Some(f) = fields.iter().find(|f| f.source_col.eq_ignore_ascii_case(text.trim())) {
        return Ok(f.source_col.clone());
    }
    let src = pv::read_source(&d.wb, &pt.source, si).map_err(other)?;
    for f in fields {
        let Some(c) = src.col(&f.source_col) else { continue };
        if pv::distinct_labels(&d.wb, &src, c, pv::field_grouping(&d.wb, pt, &src, c, f), 100_000).iter().any(|l| l.eq_ignore_ascii_case(&text)) {
            return Ok(f.source_col.clone());
        }
    }
    Err(other(NOT_A_LABEL))
}

/// Menu-style Group…: the Grouping dialog on `field`, with Starting at / Ending at / By for numbers
/// (filled in with the current grouping, else the smallest and largest values by 10) and By for dates.
fn group_dialog(s: &mut Session, p: &Json, field: &str) -> Result<Json> {
    const C: &str = "pivot.group";
    let (si, pi) = find_pivot(s, p)?;
    let d = s.doc()?;
    let pt = d.wb.sheet(si).and_then(|sh| sh.pivots.get(pi)).ok_or_else(|| other("There's no such PivotTable."))?;
    let src = pv::read_source(&d.wb, &pt.source, si).map_err(other)?;
    let (rows, i) = pv::axis_position(pt, field).ok_or_else(|| bad(C, format!("`{field}` isn't in the rows or columns area")))?;
    let f = (if rows { &pt.rows } else { &pt.columns }).get(i).ok_or_else(|| bad(C, format!("`{field}` isn't in the rows or columns area")))?;
    let c = src.col(&f.source_col).ok_or_else(|| bad(C, format!("no field `{}` in the source data", f.source_col)))?;
    if !src.numeric.get(c).copied().unwrap_or(false) {
        return Err(other("Cannot group that selection."));
    }
    let mut params = json!({"pivot": pt.name, "field": f.source_col});
    if src.dates.get(c).copied().unwrap_or(false) && f.range_group.is_none() {
        // Every level the date field is on (Years, Quarters, Months…); Months when ungrouped.
        let levels: Vec<&str> = PivotDateGroup::LEVELS
            .iter()
            .filter(|g| pt.rows.iter().chain(pt.columns.iter()).any(|x| x.source_col.eq_ignore_ascii_case(&f.source_col) && x.date_group == **g))
            .map(|g| group_name(*g))
            .collect();
        let (min, max) = pv::source_date_range(&src, c).unwrap_or((0.0, 0.0));
        let ja = pv::is_ja(&d.wb, pt);
        let date = |n: f64| gridcraft_model::date_group_date(d.wb.date_system, n, ja);
        params["kind"] = json!("date");
        params["by"] = json!(if levels.is_empty() { vec!["months"] } else { levels });
        params["autoStart"] = json!(f.date_start.is_none());
        params["autoEnd"] = json!(f.date_end.is_none());
        params["start"] = json!(date(f.date_start.unwrap_or(min.floor())));
        params["end"] = json!(date(f.date_end.unwrap_or(max.floor() + 1.0)));
        params["days"] = json!(f.date_days.max(1));
    } else {
        let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in src.rows.iter().filter_map(|r| r.get(c)) {
            if let gridcraft_core::Value::Number(n) = v.scalar() {
                min = min.min(n);
                max = max.max(n);
            }
        }
        if !min.is_finite() {
            return Err(other("Cannot group that selection."));
        }
        let g = f.range_group.unwrap_or(PivotRangeGroup { start: min, end: max, ..PivotRangeGroup::default() });
        let (start, end) = (if g.auto_start { min } else { g.start }, if g.auto_end { max } else { g.end });
        let t = gridcraft_core::number_to_text;
        params["kind"] = json!("number");
        params["autoStart"] = json!(g.auto_start);
        params["autoEnd"] = json!(g.auto_end);
        params["start"] = json!(t(start));
        params["end"] = json!(t(end));
        params["originalStart"] = json!(t(start));
        params["originalEnd"] = json!(t(end));
        params["interval"] = json!(t(g.interval));
    }
    s.ui_requests.push(crate::UiRequest::Dialog("pivotGroup".into(), params));
    ok()
}

/// Grouping ▸ Starting at / Ending at / By for a numeric field: ranges such as `0-9`, `10-19`.
fn group_range(s: &mut Session, p: &Json, field: String) -> Result<Json> {
    const C: &str = "pivot.group";
    // A ticked Auto box wins over the number beside it, as in the Grouping dialog.
    let start = if bool_param(p, "autoStart") == Some(true) { None } else { num_or_auto(C, p, "start")? };
    let end = if bool_param(p, "autoEnd") == Some(true) { None } else { num_or_auto(C, p, "end")? };
    let interval = num_or_auto(C, p, "interval")?.unwrap_or(10.0);
    if interval <= 0.0 {
        return Err(other("The By value must be greater than zero."));
    }
    update(s, p, |pt, src, _| {
        let c = src.col(&field).ok_or_else(|| bad(C, format!("no field `{field}` in the source data")))?;
        if !src.numeric.get(c).copied().unwrap_or(false) {
            return Err(other("Cannot group that selection."));
        }
        let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in src.rows.iter().filter_map(|r| r.get(c)) {
            if let gridcraft_core::Value::Number(n) = v.scalar() {
                min = min.min(n);
                max = max.max(n);
            }
        }
        if !min.is_finite() {
            return Err(other("Cannot group that selection."));
        }
        let g =
            PivotRangeGroup { start: start.unwrap_or(min), end: end.unwrap_or(max), interval, auto_start: start.is_none(), auto_end: end.is_none() };
        if g.bins(min, max, false).is_none() {
            let (a, b) = (if g.auto_start { min } else { g.start }, if g.auto_end { max } else { g.end });
            return Err(other(if a > b {
                "The ending number must be greater than the starting number."
            } else {
                "Too many groups: increase the By value."
            }));
        }
        // A date field on several levels becomes one field again.
        let (rows, i) = pv::axis_position(pt, &field).ok_or_else(|| bad(C, format!("`{field}` isn't in the rows or columns area")))?;
        set_date_levels(pt, rows, i, &[], (None, None, 0));
        let f = axis_field(C, pt, &field)?;
        f.range_group = Some(g);
        Ok(json!({"start": g.start, "end": g.end, "interval": g.interval, "autoStart": g.auto_start, "autoEnd": g.auto_end}))
    })
}

pub(super) fn collapse(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.collapse";
    let field = str_param(p, "field").ok_or_else(|| bad(C, "missing `field`"))?.to_string();
    let on = bool_param(p, "collapse").unwrap_or(true);
    let items: Option<Vec<String>> =
        p.get("items").and_then(Json::as_array).map(|a| a.iter().take(100_000).filter_map(|x| x.as_str().map(str::to_string)).collect());
    update(s, p, |pt, src, wb| {
        let all = match &items {
            Some(v) => v.clone(),
            None => {
                let f = pv::axis_position(pt, &field).and_then(|(rows, i)| (if rows { &pt.rows } else { &pt.columns }).get(i));
                let col = f.map(|f| f.source_col.clone()).unwrap_or_else(|| field.clone());
                match src.col(&col) {
                    Some(c) => pv::distinct_labels(wb, src, c, f.map_or(pv::Grouping::None, |f| pv::field_grouping(wb, pt, src, c, f)), 100_000),
                    None => vec![],
                }
            }
        };
        let f = axis_field(C, pt, &field)?;
        if on {
            for it in all {
                if !f.collapsed_items.iter().any(|x| x.eq_ignore_ascii_case(&it)) {
                    f.collapsed_items.push(it);
                }
            }
        } else if items.is_none() {
            f.collapsed_items.clear();
        } else {
            f.collapsed_items.retain(|x| !all.iter().any(|a| a.eq_ignore_ascii_case(x)));
        }
        Ok(json!({"collapsed": f.collapsed_items}))
    })
}

pub(super) fn layout(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.layout";
    let lay = match str_param(p, "layout").map(|x| x.to_ascii_lowercase()) {
        Some(l) if l == "compact" => Some(PivotLayout::Compact),
        Some(l) if l == "outline" => Some(PivotLayout::Outline),
        Some(l) if l == "tabular" => Some(PivotLayout::Tabular),
        Some(l) => return Err(bad(C, format!("unknown layout `{l}` (compact|outline|tabular)"))),
        None => None,
    };
    let subtotals: Option<(bool, Option<bool>)> = match p.get("subtotals") {
        Some(Json::Bool(b)) => Some((*b, None)),
        Some(Json::String(m)) => match m.to_ascii_lowercase().as_str() {
            "top" => Some((true, Some(true))),
            "bottom" => Some((true, Some(false))),
            "none" | "off" => Some((false, None)),
            o => return Err(bad(C, format!("unknown subtotals `{o}` (top|bottom|none)"))),
        },
        _ => None,
    };
    let style = str_param(p, "style").map(str::to_string);
    if style.as_ref().is_some_and(|s| s.len() > 64) {
        return Err(bad(C, "style name too long"));
    }
    let gr = bool_param(p, "grandTotalsRows");
    let gc = bool_param(p, "grandTotalsCols");
    let hd = bool_param(p, "showHeaders");
    update(s, p, |pt, _, _| {
        if let Some(l) = lay {
            pt.layout = l;
            if l == PivotLayout::Tabular {
                pt.subtotals_top = false;
            }
        }
        if let Some((on, top)) = subtotals {
            pt.subtotals = on;
            if let Some(t) = top {
                pt.subtotals_top = t;
            }
        }
        if let Some(st) = &style {
            pt.style = st.clone();
        }
        if let Some(v) = gr {
            pt.grand_totals_rows = v;
        }
        if let Some(v) = gc {
            pt.grand_totals_cols = v;
        }
        if let Some(v) = hd {
            pt.show_headers = v;
        }
        Ok(json!({"layout": layout_name(pt.layout)}))
    })
}

pub(super) fn grand_totals(s: &mut Session, p: &Json) -> Result<Json> {
    let (r, c) = match str_param(p, "mode").map(|m| m.to_ascii_lowercase()) {
        Some(m) if m == "off" => (Some(false), Some(false)),
        Some(m) if m == "on" => (Some(true), Some(true)),
        Some(m) if m == "rows" => (Some(true), Some(false)),
        Some(m) if m == "columns" || m == "cols" => (Some(false), Some(true)),
        Some(m) => return Err(bad("pivot.grandTotals", format!("unknown mode `{m}` (off|on|rows|columns)"))),
        None => (bool_param(p, "rows"), bool_param(p, "cols")),
    };
    let mut q = p.as_object().cloned().unwrap_or_default();
    if let Some(v) = r {
        q.insert("grandTotalsRows".into(), json!(v));
    }
    if let Some(v) = c {
        q.insert("grandTotalsCols".into(), json!(v));
    }
    layout(s, &Json::Object(q))
}

pub(super) fn refresh(s: &mut Session, p: &Json) -> Result<Json> {
    update(s, p, |_, src, _| Ok(json!({"records": src.rows.len()})))
}

pub(super) fn refresh_all(s: &mut Session, p: &Json) -> Result<Json> {
    let replace = bool_param(p, "replace").unwrap_or(false);
    edit(s, |cx| {
        let mut n = 0;
        let mut errors = vec![];
        for si in 0..cx.wb.sheets.len() {
            let count = cx.wb.sheet(si).map(|sh| sh.pivots.len()).unwrap_or(0);
            for pi in 0..count {
                let name = cx.wb.sheet(si).and_then(|sh| sh.pivots.get(pi)).map(|p| p.name.clone()).unwrap_or_default();
                match pv::refresh(&mut cx.wb, si, pi, replace) {
                    Ok(_) => n += 1,
                    Err(e) => errors.push(json!({"pivot": name, "error": e})),
                }
            }
        }
        cx.structural = n > 0;
        Ok(json!({"refreshed": n, "errors": errors}))
    })
}

pub(super) fn change_source(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.changeSource";
    if str_param(p, "source").or_else(|| str_param(p, "range")).is_none() {
        return Err(bad(C, "missing `source`"));
    }
    let source = source_param(s, p)?;
    let (si, _) = find_pivot(s, p)?;
    let src = pv::read_source(&s.doc()?.wb, &source, si).map_err(other)?;
    update(s, p, move |pt, _, _| {
        pt.source = source.clone();
        let mut dropped: Vec<String> = vec![];
        let keep = |name: &str, dropped: &mut Vec<String>| {
            let ok = src.col(name).is_some();
            if !ok {
                dropped.push(name.to_string());
            }
            ok
        };
        // Axis fields leave one by one so Σ Values keeps its place among the rest.
        for rows in [true, false] {
            let mut i = 0;
            loop {
                let list = if rows { &mut pt.rows } else { &mut pt.columns };
                let Some(f) = list.get(i) else { break };
                if keep(&f.source_col, &mut dropped) {
                    i += 1;
                } else {
                    list.remove(i);
                    shift_values(pt, rows, i, false);
                }
            }
        }
        pt.values.retain(|f| keep(&f.source_col, &mut dropped));
        pt.filters.retain(|f| keep(&f.source_col, &mut dropped));
        Ok(json!({"source": source, "dropped": dropped}))
    })
}

pub(super) fn delete(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, pi) = find_pivot(s, p)?;
    edit(s, |cx| {
        // Its PivotCharts become ordinary charts.
        let link = cx.wb.sheet(si).and_then(|sh| sh.pivots.get(pi).map(|pt| (sh.name.clone(), pt.name.clone())));
        if let Some((sheet, name)) = link {
            for i in 0..cx.wb.sheets.len() {
                let linked = |c: &Chart| c.pivot.as_deref().is_some_and(|l| pv::links_to(l, &sheet, &name));
                if cx.wb.sheet(i).is_some_and(|sh| sh.charts.iter().any(linked)) {
                    for c in cx.sheet_mut(i)?.charts.iter_mut().filter(|c| linked(c)) {
                        c.pivot = None;
                    }
                }
            }
        }
        pv::clear_output(&mut cx.wb, si, pi);
        let sh = cx.sheet_mut(si)?;
        if pi < sh.pivots.len() {
            sh.pivots.remove(pi);
        }
        cx.structural = true;
        Ok(json!({}))
    })
}
