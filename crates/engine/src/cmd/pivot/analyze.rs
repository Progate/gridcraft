//! The commands behind the ▼ of 行ラベル / 列ラベル and a report filter (item checks, ラベル
//! フィルター, 値フィルター, フィルターのクリア), PivotTable Analyze › Show (＋/－ボタン, フィールドの
//! 見出し), Design › Blank Rows and Repeat All Item Labels, and the buttons a report shows.

use gridcraft_model::{PivotLabelFilter, PivotLabelOp, PivotTopBy, PivotValueFilter, PivotValueOp};

use super::fields::{canonical, update};
use super::{Json, PivotFilter, PivotTable, Result, Session, Workbook, bad, bool_param, find_pivot, json, other, pv, str_param};

/// The row or column field `field` names (a source field or a date level such as 年).
fn axis_field_mut<'a>(cmd: &str, pt: &'a mut PivotTable, field: &str) -> Result<&'a mut gridcraft_model::PivotField> {
    let (rows, i) = pv::axis_position(pt, field).ok_or_else(|| bad(cmd, format!("`{field}` isn't in the rows or columns area")))?;
    (if rows { &mut pt.rows } else { &mut pt.columns }).get_mut(i).ok_or_else(|| bad(cmd, format!("`{field}` isn't in the rows or columns area")))
}

/// The labels a field's items show (under its finest grouping on an axis), sorted as the report
/// sorts them: what the ▼ menu and a slicer list.
pub(crate) fn field_items(wb: &Workbook, pt: &PivotTable, src: &pv::Source, field: &str) -> Vec<String> {
    let Some(c) = src.col(field) else { return vec![] };
    let finest = pt
        .rows
        .iter()
        .chain(pt.columns.iter())
        .filter(|f| f.source_col.eq_ignore_ascii_case(field))
        .max_by_key(|f| if f.date_group == gridcraft_model::PivotDateGroup::None { 0 } else { f.date_group.rank() + 1 });
    let group = finest.map_or(pv::Grouping::None, |f| pv::field_grouping(wb, pt, src, c, f));
    pv::distinct_labels(wb, src, c, group, 100_000)
}

/// Sets which items of `field` show (`None`: all). A field in Rows, Columns or Filters keeps the
/// selection as its hidden items or report filter; a field in no area gets a filter of its own
/// that doesn't show above the report (a slicer's).
pub(crate) fn set_selection(pt: &mut PivotTable, field: &str, selected: Option<Vec<String>>) {
    let on_axis = placed_in_axes(&pt.rows, &pt.columns, field);
    let i = pt.filters.iter().position(|f| f.source_col.eq_ignore_ascii_case(field));
    match (i, selected) {
        (Some(i), None) => {
            // A report filter stays in the Filters area, showing (すべて).
            if !on_axis && pt.filters.get(i).is_some_and(|f| !f.hidden_only) {
                if let Some(f) = pt.filters.get_mut(i) {
                    f.selected = None;
                }
            } else {
                pt.filters.remove(i);
            }
        }
        (Some(i), Some(sel)) => {
            if let Some(f) = pt.filters.get_mut(i) {
                f.selected = Some(sel);
            }
        }
        (None, Some(sel)) => pt.filters.push(PivotFilter { source_col: field.to_string(), selected: Some(sel), hidden_only: !on_axis }),
        (None, None) => {}
    }
}

fn placed_in_axes(rows: &[gridcraft_model::PivotField], cols: &[gridcraft_model::PivotField], field: &str) -> bool {
    rows.iter().chain(cols.iter()).any(|f| f.source_col.eq_ignore_ascii_case(field))
}

/// The selected items of `field` (`None`: all).
pub(crate) fn selection(pt: &PivotTable, field: &str) -> Option<Vec<String>> {
    pt.filters.iter().find(|f| f.source_col.eq_ignore_ascii_case(field)).and_then(|f| f.selected.clone())
}

fn label_op_name(o: PivotLabelOp) -> &'static str {
    o.name()
}

fn top_by_name(b: PivotTopBy) -> &'static str {
    match b {
        PivotTopBy::Items => "items",
        PivotTopBy::Percent => "percent",
        PivotTopBy::Sum => "sum",
    }
}

/// `pivot.items`: a field's items with whether each shows, and its filters, for the ▼ menu.
pub(super) fn items(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.items";
    let field = str_param(p, "field").ok_or_else(|| bad(C, "missing `field`"))?.to_string();
    let (si, pi) = find_pivot(s, p)?;
    let d = s.doc()?;
    let pt = d.wb.sheet(si).and_then(|sh| sh.pivots.get(pi)).ok_or_else(|| other("There's no such PivotTable."))?;
    let src = pv::read_source(&d.wb, &pt.source, si).map_err(other)?;
    let axis = pv::axis_position(pt, &field).and_then(|(rows, i)| (if rows { &pt.rows } else { &pt.columns }).get(i).map(|f| (rows, f)));
    let base = match axis {
        Some((_, f)) => f.source_col.clone(),
        None => canonical(C, &src, &field)?,
    };
    // A coarser date level (年) lists its own items; a source field its finest ones.
    let labels = match axis {
        Some((_, f)) if src.col(&base).is_some() && pv::field_caption(&d.wb, pt, f) != f.source_col => {
            let c = src.col(&base).unwrap_or(0);
            pv::distinct_labels(&d.wb, &src, c, pv::field_grouping(&d.wb, pt, &src, c, f), 100_000)
        }
        _ => field_items(&d.wb, pt, &src, &base),
    };
    let sel = selection(pt, &base);
    let items: Vec<Json> =
        labels.iter().map(|l| json!({"label": l, "selected": sel.as_ref().is_none_or(|v| v.iter().any(|x| x.eq_ignore_ascii_case(l)))})).collect();
    let area = match axis {
        Some((true, _)) => "rows",
        Some((false, _)) => "columns",
        None if pt.filters.iter().any(|f| f.source_col.eq_ignore_ascii_case(&base) && !f.hidden_only) => "filters",
        None => "none",
    };
    let label_filter =
        axis.and_then(|(_, f)| f.label_filter.as_ref()).map(|l| json!({"op": label_op_name(l.op), "value": l.value, "value2": l.value2}));
    let value_filter = axis.and_then(|(_, f)| f.value_filter.as_ref()).map(|v| {
        json!({"op": v.op.name(), "valueField": v.value_field, "value": v.value, "value2": v.value2, "bottom": v.bottom, "by": top_by_name(v.top_by)})
    });
    let sort = axis.map(|(_, f)| match f.sort {
        gridcraft_model::PivotSort::Asc => "asc",
        gridcraft_model::PivotSort::Desc => "desc",
        gridcraft_model::PivotSort::None => "none",
    });
    Ok(json!({
        "field": axis.map(|(_, f)| pv::field_caption(&d.wb, pt, f)).unwrap_or_else(|| base.clone()),
        "sourceField": base,
        "area": area,
        "items": items,
        "filtered": sel.is_some() || label_filter.is_some() || value_filter.is_some(),
        "labelFilter": label_filter,
        "valueFilter": value_filter,
        "sort": sort,
        "valueFields": pt.values.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
    }))
}

/// `pivot.labelFilter`: ラベル フィルター on a row or column field.
pub(super) fn label_filter(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.labelFilter";
    let field = str_param(p, "field").ok_or_else(|| bad(C, "missing `field`"))?.to_string();
    let clear = bool_param(p, "clear").unwrap_or(false);
    let filter = if clear {
        None
    } else {
        let op = str_param(p, "op").ok_or_else(|| bad(C, "missing `op`"))?;
        let op = PivotLabelOp::from_name(op).ok_or_else(|| {
            bad(C, format!("unknown op `{op}` (equal|notEqual|beginsWith|notBeginsWith|endsWith|notEndsWith|contains|notContains|greaterThan|greaterThanOrEqual|lessThan|lessThanOrEqual|between|notBetween)"))
        })?;
        let text = |k: &str| match p.get(k) {
            Some(Json::String(s)) => s.chars().take(255).collect(),
            Some(Json::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        Some(PivotLabelFilter { op, value: text("value"), value2: text("value2") })
    };
    update(s, p, |pt, _, _| {
        let f = axis_field_mut(C, pt, &field)?;
        f.label_filter = filter.clone();
        // Excel keeps one filter per field: a Label Filter replaces a Value Filter.
        if filter.is_some() {
            f.value_filter = None;
        }
        Ok(json!({}))
    })
}

fn number(cmd: &str, p: &Json, k: &str) -> Result<Option<f64>> {
    match p.get(k) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Number(n)) => Ok(n.as_f64()),
        Some(Json::String(t)) if t.trim().is_empty() => Ok(None),
        Some(Json::String(t)) => {
            t.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| bad(cmd, format!("`{k}` must be a number")))
        }
        Some(_) => Err(bad(cmd, format!("`{k}` must be a number"))),
    }
}

/// `pivot.valueFilter`: 値フィルター (and トップテン) on a row or column field.
pub(super) fn value_filter(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.valueFilter";
    let field = str_param(p, "field").ok_or_else(|| bad(C, "missing `field`"))?.to_string();
    let clear = bool_param(p, "clear").unwrap_or(false);
    let op = match (clear, str_param(p, "op")) {
        (true, _) => None,
        (false, Some(o)) => Some(PivotValueOp::from_name(o).ok_or_else(|| {
            bad(C, format!("unknown op `{o}` (equal|notEqual|greaterThan|greaterThanOrEqual|lessThan|lessThanOrEqual|between|notBetween|top10)"))
        })?),
        (false, None) => return Err(bad(C, "missing `op`")),
    };
    let value = number(C, p, "value")?;
    let value2 = number(C, p, "value2")?.unwrap_or(0.0);
    let bottom = matches!(str_param(p, "order").map(str::to_ascii_lowercase).as_deref(), Some("bottom")) || bool_param(p, "bottom") == Some(true);
    let top_by = match str_param(p, "by").map(str::to_ascii_lowercase).as_deref() {
        None | Some("items") | Some("item") => PivotTopBy::Items,
        Some("percent") => PivotTopBy::Percent,
        Some("sum") => PivotTopBy::Sum,
        Some(o) => return Err(bad(C, format!("unknown `by` `{o}` (items|percent|sum)"))),
    };
    let value_field = str_param(p, "valueField").map(str::to_string);
    if op.is_some_and(|o| o != PivotValueOp::Top10) && value.is_none() {
        return Err(bad(C, "missing `value`"));
    }
    update(s, p, |pt, _, _| {
        let vf = match op {
            None => None,
            Some(op) => {
                let name = match &value_field {
                    Some(n) => pt
                        .values
                        .iter()
                        .find(|v| v.name.eq_ignore_ascii_case(n) || v.source_col.eq_ignore_ascii_case(n))
                        .map(|v| v.name.clone())
                        .ok_or_else(|| bad(C, format!("no value field `{n}`")))?,
                    None => pt.values.first().map(|v| v.name.clone()).ok_or_else(|| other("Add a field to the Values area first."))?,
                };
                let n = value.unwrap_or(if op == PivotValueOp::Top10 { 10.0 } else { 0.0 });
                if op == PivotValueOp::Top10 && !(n > 0.0 && n <= 2_147_483_647.0) {
                    return Err(bad(C, "the number of items must be greater than 0"));
                }
                Some(PivotValueFilter { op, value_field: name, value: n, value2, bottom, top_by })
            }
        };
        let f = axis_field_mut(C, pt, &field)?;
        if vf.is_some() {
            f.label_filter = None;
        }
        f.value_filter = vf;
        Ok(json!({}))
    })
}

/// `pivot.clearFilter`: フィルターのクリア of one field (its checked items, Label and Value
/// Filters), or of every field.
pub(super) fn clear_filter(s: &mut Session, p: &Json) -> Result<Json> {
    let field = str_param(p, "field").map(str::to_string);
    update(s, p, |pt, _, _| {
        let all = field.is_none();
        let name = field.clone().unwrap_or_default();
        let base = pv::axis_position(pt, &name)
            .and_then(|(rows, i)| (if rows { &pt.rows } else { &pt.columns }).get(i))
            .map(|f| f.source_col.clone())
            .unwrap_or(name.clone());
        for f in pt.rows.iter_mut().chain(pt.columns.iter_mut()) {
            if all || f.source_col.eq_ignore_ascii_case(&base) {
                f.label_filter = None;
                f.value_filter = None;
            }
        }
        let axes: Vec<String> = pt.rows.iter().chain(pt.columns.iter()).map(|f| f.source_col.clone()).collect();
        pt.filters.retain_mut(|f| {
            if !(all || f.source_col.eq_ignore_ascii_case(&base)) {
                return true;
            }
            let on_axis = axes.iter().any(|a| a.eq_ignore_ascii_case(&f.source_col));
            if on_axis || f.hidden_only {
                return false;
            }
            f.selected = None;
            true
        });
        Ok(json!({}))
    })
}

/// A pivot-wide on/off option: `on` (default: the opposite of now).
fn toggle(s: &mut Session, p: &Json, get: fn(&PivotTable) -> bool, set: fn(&mut PivotTable, bool)) -> Result<Json> {
    let on = bool_param(p, "on");
    update(s, p, |pt, _, _| {
        let v = on.unwrap_or(!get(pt));
        set(pt, v);
        Ok(json!({"on": v}))
    })
}

pub(super) fn expand_buttons(s: &mut Session, p: &Json) -> Result<Json> {
    toggle(s, p, |pt| pt.show_expand_buttons, |pt, v| pt.show_expand_buttons = v)
}

pub(super) fn field_headers(s: &mut Session, p: &Json) -> Result<Json> {
    toggle(s, p, |pt| pt.show_headers, |pt, v| pt.show_headers = v)
}

pub(super) fn blank_rows(s: &mut Session, p: &Json) -> Result<Json> {
    toggle(s, p, |pt| pt.blank_rows, |pt, v| pt.blank_rows = v)
}

pub(super) fn repeat_labels(s: &mut Session, p: &Json) -> Result<Json> {
    toggle(s, p, |pt| pt.repeat_labels, |pt, v| pt.repeat_labels = v)
}

/// `pivot.buttons`: the ▼ and +/- buttons of the report, at their cells.
pub(super) fn buttons(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, pi) = find_pivot(s, p)?;
    let d = s.doc()?;
    let Some((at, list)) = pv::report_buttons(&d.wb, si, pi) else { return Ok(json!([])) };
    let v: Vec<Json> = list
        .iter()
        .map(|b| {
            let (r, c) = b.cell();
            let mut j = serde_json::to_value(b).unwrap_or(Json::Null);
            if let Some(m) = j.as_object_mut() {
                m.insert("cell".into(), json!(gridcraft_core::CellRef::new(at.row + r, at.col + c).a1()));
            }
            j
        })
        .collect();
    Ok(Json::Array(v))
}

/// The value field whose number the active cell shows: the one whose caption heads its column
/// (or, with Σ Values in Rows, labels its row), else the only one.
fn value_at_active_cell(s: &Session, p: &Json) -> Option<String> {
    let (si, pi) = find_pivot(s, p).ok()?;
    let d = s.doc().ok()?;
    let pt = d.wb.sheet(si)?.pivots.get(pi)?;
    let at = d.selection.active;
    let origin = pt.last_range.filter(|r| r.contains(at))?.start;
    let out = pv::compute(&d.wb, pt, si).ok()?;
    let (r, c) = ((at.row - origin.row) as usize, (at.col - origin.col) as usize);
    let kind = out.grid.get(r)?.get(c)?.kind;
    if !matches!(kind, pv::CellKind::Data | pv::CellKind::Subtotal | pv::CellKind::GrandTotal) || pt.values.is_empty() {
        return None;
    }
    if pt.values.len() == 1 {
        return pt.values.first().map(|v| v.name.clone());
    }
    let text = |r: usize, c: usize| out.grid.get(r).and_then(|l| l.get(c)).and_then(|x| x.value.to_text().ok()).unwrap_or_default();
    let named = |t: String| pt.values.iter().find(|v| t == v.name || t.ends_with(&format!(" {}", v.name))).map(|v| v.name.clone());
    let headers = out.row_kinds.iter().enumerate().filter(|(_, k)| **k == pv::RowKind::Header).map(|(i, _)| i);
    headers.map(|h| text(h, c)).chain((0..c).map(|cc| text(r, cc))).find_map(named).or_else(|| pt.values.first().map(|v| v.name.clone()))
}

/// Settings `pivot.fieldSettings` applies; without them it opens a dialog.
const FIELD_SETTINGS: [&str; 4] = ["subtotals", "repeatLabels", "blankRows", "layout"];

/// `pivot.fieldSettings`: フィールドの設定. A value field opens Value Field Settings; a row or
/// column field opens Field Settings (its subtotals and layout options, which GridCraft keeps for
/// the whole PivotTable), or applies them when they're given.
pub(super) fn field_settings(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.fieldSettings";
    if FIELD_SETTINGS.iter().any(|k| p.get(*k).is_some()) {
        let subtotals = match str_param(p, "subtotals").map(str::to_ascii_lowercase).as_deref() {
            None => None,
            Some("automatic") | Some("auto") | Some("on") => Some(true),
            Some("none") | Some("off") => Some(false),
            Some(o) => return Err(bad(C, format!("unknown subtotals `{o}` (automatic|none)"))),
        };
        let layout = match str_param(p, "layout").map(str::to_ascii_lowercase).as_deref() {
            None => None,
            Some("compact") => Some(gridcraft_model::PivotLayout::Compact),
            Some("outline") => Some(gridcraft_model::PivotLayout::Outline),
            Some("tabular") => Some(gridcraft_model::PivotLayout::Tabular),
            Some(o) => return Err(bad(C, format!("unknown layout `{o}` (compact|outline|tabular)"))),
        };
        let repeat = bool_param(p, "repeatLabels");
        let blank = bool_param(p, "blankRows");
        return update(s, p, |pt, _, _| {
            if let Some(v) = subtotals {
                pt.subtotals = v;
            }
            if let Some(l) = layout {
                pt.layout = l;
            }
            if let Some(v) = repeat {
                pt.repeat_labels = v;
            }
            if let Some(v) = blank {
                pt.blank_rows = v;
            }
            Ok(json!({}))
        });
    }
    let field = match str_param(p, "field") {
        Some(f) => Some(f.to_string()),
        None => value_at_active_cell(s, p),
    };
    let (si, pi) = find_pivot(s, p)?;
    let pt = s.doc()?.wb.sheet(si).and_then(|sh| sh.pivots.get(pi)).cloned().ok_or_else(|| other("There's no such PivotTable."))?;
    if let Some(f) = &field
        && pt.values.iter().any(|v| v.name.eq_ignore_ascii_case(f))
    {
        return super::fields::value_settings(s, &json!({"pivot": pt.name, "field": f}));
    }
    let field = match field {
        Some(f) => f,
        None => super::layout::field_at_active_cell(s, p).map_err(|_| other("Select a field of the PivotTable first."))?,
    };
    let wb = &s.doc()?.wb;
    let (rows, i) = pv::axis_position(&pt, &field).ok_or_else(|| bad(C, format!("`{field}` isn't in the rows or columns area")))?;
    let f = (if rows { &pt.rows } else { &pt.columns }).get(i).ok_or_else(|| other("There's no such field."))?;
    let params = json!({
        "pivot": pt.name,
        "field": pv::field_caption(wb, &pt, f),
        "sourceName": f.source_col,
        "subtotals": if pt.subtotals { "automatic" } else { "none" },
        "layout": super::params::layout_name(pt.layout),
        "repeatLabels": pt.repeat_labels,
        "blankRows": pt.blank_rows,
    });
    s.ui_requests.push(crate::UiRequest::Dialog("pivotFieldSettings".into(), params));
    Ok(json!({}))
}
