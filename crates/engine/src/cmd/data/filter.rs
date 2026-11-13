use super::sort::sort_key;
use gridcraft_core::date::DateSystem;
use gridcraft_model::{DYNAMIC_FILTER_TYPES, custom_filter_condition};

use super::{
    AutoFilter, CellRef, EngineError, FilterCriterion, Json, Ordering, RangeRef, Result, Session, Sheet, Value, Workbook, bad, bool_param, col_param,
    current_region, data_target, edit, json,
};

// ---------------------------------------------------------------- filter

pub(super) fn toggle_filter(s: &mut Session, p: &Json) -> Result<Json> {
    let (sheet, r) = data_target(s, p, "data.filter")?;
    crate::cmd::refuse_unless_allowed(s, sheet, "Filtering", |p| p.autofilter)?;
    // The active cell belongs to the active sheet: on another sheet the table is the one at `range`.
    let at = if sheet == s.doc()?.wb.active_sheet() { s.doc()?.selection.active } else { r.start };
    let has = s.doc()?.wb.sheet(sheet).is_some_and(|sh| sh.autofilter.is_some());
    let in_table = s.doc()?.wb.sheet(sheet).and_then(|sh| sh.table_at(at).map(|t| t.name.clone()));
    if let Some(name) = in_table {
        return edit(s, |cx| {
            let sh = cx.sheet_mut(sheet)?;
            if let Some(t) = sh.tables.iter_mut().find(|t| t.name == name) {
                t.filter_button = !t.filter_button;
                if !t.filter_button {
                    t.filter.clear();
                }
            }
            Ok(Json::Null)
        });
    }
    if has {
        return edit(s, |cx| {
            let sh = cx.sheet_mut(sheet)?;
            // Reveal only rows the filter hid; rows hidden by hand or by a collapsed group stay hidden.
            let filtered: Vec<u32> = sh.rows.keys().copied().filter(|r| sh.is_row_filtered(*r)).collect();
            sh.autofilter = None;
            for r in filtered {
                if let Some(info) = sh.rows.get_mut(&r) {
                    info.hidden = false;
                }
            }
            cx.structural = true;
            Ok(json!({"filter": false}))
        });
    }
    install_filter(s, p)
}

/// Turn a sheet AutoFilter on over `p.range`, or over the data range of the selection.
fn install_filter(s: &mut Session, p: &Json) -> Result<Json> {
    let (sheet, mut r) = data_target(s, p, "data.filter")?;
    // A selected header row filters the data below it (#201).
    if p.get("range").is_none() && r.height() == 1 {
        let d = s.doc()?;
        let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
        let below = current_region(sh, r.start).end.row;
        if below > r.end.row {
            r = RangeRef::new(r.start, CellRef::new(below, r.end.col));
        }
    }
    edit(s, |cx| {
        cx.sheet_mut(sheet)?.autofilter = Some(AutoFilter { range: r, criteria: vec![] });
        Ok(json!({"filter": true, "range": r.a1()}))
    })
}

pub(super) fn filter_by(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = s.doc()?.wb.active_sheet();
    crate::cmd::refuse_unless_allowed(s, sheet, "Filtering", |p| p.autofilter)?;
    let Some(sh) = s.doc()?.wb.sheet(sheet) else { return Err(EngineError::NoDocument) };
    let af = match &sh.autofilter {
        Some(a) => a.clone(),
        None => {
            install_filter(s, &json!({}))?;
            s.doc()?.wb.sheet(sheet).and_then(|sh| sh.autofilter.clone()).ok_or_else(|| EngineError::Other("no filter".into()))?
        }
    };
    let col = match col_param(p.get("column"), af.range.start.col) {
        Some(c) => c,
        // A number too large to be a column is outside the filter range, not a missing parameter.
        None if p.get("column").is_some_and(|v| v.as_u64().is_some()) => return Err(bad("data.filterBy", "column outside the filter range")),
        None => return Err(bad("data.filterBy", "missing `column`")),
    };
    if col < af.range.start.col || col > af.range.end.col {
        return Err(bad("data.filterBy", "column outside the filter range"));
    }
    let off = col - af.range.start.col;
    let (sys, region) = (s.doc()?.wb.date_system, s.doc()?.wb.region);
    let crit = filter_criterion(p, sys, region)?;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        if let Some(a) = sh.autofilter.as_mut() {
            a.criteria.retain(|(o, _)| *o != off);
            if let Some(c) = crit.clone() {
                a.criteria.push((off, c));
            }
        }
        apply_filter(&mut cx.wb, sheet);
        cx.structural = true;
        Ok(Json::Null)
    })?;
    let hidden = s.doc()?.wb.sheet(sheet).map(|sh| sh.rows.values().filter(|r| r.hidden).count()).unwrap_or(0);
    Ok(json!({"hiddenRows": hidden}))
}

/// The criterion `data.filterBy` sets from its parameters; `None` clears the column.
fn filter_criterion(p: &Json, sys: DateSystem, region: &gridcraft_core::Region) -> Result<Option<FilterCriterion>> {
    const C: &str = "data.filterBy";
    if bool_param(p, "clear").unwrap_or(false) {
        return Ok(None);
    }
    if let Some(vals) = p.get("values").and_then(Json::as_array) {
        return Ok(Some(FilterCriterion::Values {
            values: vals.iter().map(crate::cmd::edit::json_to_input).collect(),
            blanks: bool_param(p, "blanks").unwrap_or(false),
        }));
    }
    if let Some(c) = p.get("custom") {
        let cond = |op: &str, value: Option<&Json>| {
            let value = value.map(crate::cmd::edit::json_to_input).unwrap_or_default();
            custom_filter_condition(op, &date_criterion(&value, sys, region)).ok_or_else(|| bad(C, format!("unknown custom filter operator `{op}`")))
        };
        let a = cond(c.get("op").and_then(Json::as_str).unwrap_or("="), c.get("value"))?;
        let b = match c.get("op2").and_then(Json::as_str) {
            Some(op) => Some(cond(op, c.get("value2"))?),
            None => None,
        };
        // `and` only means something for two conditions; a single one keeps it off, as the file does.
        let and = b.is_some() && c.get("and").and_then(Json::as_bool).unwrap_or(true);
        return Ok(Some(FilterCriterion::Custom { a, b, and }));
    }
    if let Some(t) = p.get("top") {
        let percent = t.get("percent").and_then(Json::as_bool).unwrap_or(false);
        let count = t.get("count").and_then(Json::as_u64).unwrap_or(10);
        // Excel's Top 10 dialog takes 1–500 items or 1–100 percent.
        let max = if percent { 100 } else { 500 };
        if !(1..=max).contains(&count) {
            return Err(bad(C, format!("`top.count` must be between 1 and {max}")));
        }
        return Ok(Some(FilterCriterion::Top10 {
            bottom: t.get("bottom").and_then(Json::as_bool).unwrap_or(false),
            percent,
            count: count as u32,
            filter_val: None,
        }));
    }
    if let Some(kind) = p.get("dynamic").and_then(Json::as_str) {
        let Some(kind) = DYNAMIC_FILTER_TYPES.iter().find(|k| k.eq_ignore_ascii_case(kind)) else {
            return Err(bad(C, format!("unknown dynamic filter `{kind}`")));
        };
        return Ok(Some(FilterCriterion::Dynamic { kind: (*kind).to_string(), val: None, max_val: None }));
    }
    Ok(bool_param(p, "aboveAverage").map(|above| FilterCriterion::Dynamic {
        kind: if above { "aboveAverage" } else { "belowAverage" }.to_string(),
        val: None,
        max_val: None,
    }))
}

/// A custom filter value typed as a date (`2026/3/4`, `2026-03-04`) becomes its serial number,
/// which is what Excel stores and compares for date columns; any other value stays as typed.
/// In the Japanese region the value is read as a cell entry is there: full-width digits and
/// symbols from an input method (`１００`, `２０２６/１０/１５`), `¥1,280` and `2026年10月15日`
/// become the number (or the date's serial) they type in a cell.
fn date_criterion(value: &str, sys: DateSystem, region: &gridcraft_core::Region) -> String {
    if region.is_japanese() && !value.contains(['*', '?']) {
        let parsed = gridcraft_core::parse::parse_input_in(value, sys, region);
        if let Some(n) = parsed.value.as_f64()
            && matches!(parsed.value, gridcraft_core::Value::Number(_))
        {
            return n.to_string();
        }
        return value.to_string();
    }
    let parsed = gridcraft_core::parse::parse_input(value, sys);
    match (parsed.value.as_f64(), parsed.format) {
        (Some(n), Some(f)) if gridcraft_numfmt::NumberFormat::parse(f).kind() == gridcraft_numfmt::FormatKind::Date => n.to_string(),
        _ => value.to_string(),
    }
}

/// The half-open serial span `[start, end)` a relative date filter covers on `today`, or `None`
/// for a filter that is not a span of dates (the averages, `Q1`–`Q4`, `M1`–`M12`). Weeks run
/// Sunday to Saturday and quarters are calendar quarters, as in Excel.
pub(crate) fn dynamic_span(kind: &str, today: f64, sys: DateSystem) -> Option<(f64, f64)> {
    let t = today.floor();
    let dt = gridcraft_core::date::datetime_from_serial(sys, t)?;
    let (y, m) = (i64::from(dt.year), i64::from(dt.month));
    let ymd = |y: i64, m: i64| gridcraft_core::date::serial_from_ymd(sys, y, m, 1);
    let week = t - f64::from(dt.weekday);
    let quarter = (m - 1) / 3 * 3 + 1;
    let months = |from: i64, len: i64| Some((ymd(y, from)?, ymd(y, from + len)?));
    match kind {
        "tomorrow" => Some((t + 1.0, t + 2.0)),
        "today" => Some((t, t + 1.0)),
        "yesterday" => Some((t - 1.0, t)),
        "nextWeek" => Some((week + 7.0, week + 14.0)),
        "thisWeek" => Some((week, week + 7.0)),
        "lastWeek" => Some((week - 7.0, week)),
        "nextMonth" => months(m + 1, 1),
        "thisMonth" => months(m, 1),
        "lastMonth" => months(m - 1, 1),
        "nextQuarter" => months(quarter + 3, 3),
        "thisQuarter" => months(quarter, 3),
        "lastQuarter" => months(quarter - 3, 3),
        "nextYear" => months(13, 12),
        "thisYear" => months(1, 12),
        "lastYear" => months(-11, 12),
        "yearToDate" => Some((ymd(y, 1)?, t + 1.0)),
        _ => None,
    }
}

/// Whether a number passes a dynamic filter whose span (or average) was resolved to `val`/`max_val`.
fn dynamic_ok(kind: &str, n: f64, val: Option<f64>, max_val: Option<f64>, sys: DateSystem) -> bool {
    let month = || gridcraft_core::date::datetime_from_serial(sys, n).map(|d| d.month);
    match kind {
        "aboveAverage" => val.is_some_and(|avg| n > avg),
        "belowAverage" => val.is_some_and(|avg| n < avg),
        _ => {
            if let Some(q) = kind.strip_prefix('Q').and_then(|q| q.parse::<u32>().ok()) {
                month().is_some_and(|m| (m - 1) / 3 + 1 == q)
            } else if let Some(want) = kind.strip_prefix('M').and_then(|m| m.parse::<u32>().ok()) {
                month() == Some(want)
            } else {
                matches!((val, max_val), (Some(a), Some(b)) if n >= a && n < b)
            }
        }
    }
}

/// Does a value pass a custom comparison (`>`, `=*abc*`, `<>x`…)?
fn custom_ok(v: &Value, text: &str, op: &str, crit: &str) -> bool {
    let num = gridcraft_core::parse::parse_number_text(crit);
    let cmp = match (v.as_f64(), num) {
        (Some(a), Some(b)) => a.partial_cmp(&b),
        // A numeric ordering comparison only matches numbers; blanks and text never do.
        (None, Some(_)) if matches!(op, "<" | "<=" | ">" | ">=") => None,
        _ => Some(gridcraft_core::compare_text(text, crit)),
    };
    let wild = || crate::cmd::edit::wildcard(&text.to_lowercase(), &crit.to_lowercase());
    match op {
        "=" => {
            if crit.contains(['*', '?']) {
                wild()
            } else {
                cmp == Some(Ordering::Equal)
            }
        }
        "<>" => {
            if crit.contains(['*', '?']) {
                !wild()
            } else {
                cmp != Some(Ordering::Equal)
            }
        }
        ">" => cmp == Some(Ordering::Greater),
        "<" => cmp == Some(Ordering::Less),
        ">=" => matches!(cmp, Some(Ordering::Greater | Ordering::Equal)),
        "<=" => matches!(cmp, Some(Ordering::Less | Ordering::Equal)),
        "beginsWith" => text.to_lowercase().starts_with(&crit.to_lowercase()),
        "endsWith" => text.to_lowercase().ends_with(&crit.to_lowercase()),
        "contains" => text.to_lowercase().contains(&crit.to_lowercase()),
        "notContains" => !text.to_lowercase().contains(&crit.to_lowercase()),
        _ => true,
    }
}

/// The filter's "(Blanks)" group: truly empty cells and zero-length text such as a `=""` result.
fn filter_blank(v: &Value) -> bool {
    v.is_empty() || matches!(v, Value::Text(t) if t.is_empty())
}

/// Excel grows an AutoFilter range when a value is entered in the row right below it, in one of its columns (#2342).
/// A table's own filter is left to the table, and a row typed after a blank row does not count.
/// A formula counts before it is calculated: this runs inside the edit, ahead of the recalc.
pub(crate) fn grow_autofilter(sh: &mut Sheet, at: CellRef) {
    if !sh.cells.get(at).is_some_and(|c| !filter_blank(&c.value) || c.formula.is_some()) {
        return;
    }
    let Some(af) = sh.autofilter.as_ref() else { return };
    let r = af.range;
    if r.end.row.checked_add(1) != Some(at.row) || at.col < r.start.col || at.col > r.end.col || sh.tables.iter().any(|t| t.range == r) {
        return;
    }
    let grown = RangeRef::new(r.start, CellRef::new(at.row, r.end.col));
    if let Some(af) = sh.autofilter.as_mut() {
        af.range = grown;
    }
}

/// Hides rows of the autofilter range that fail any criterion, and records what the Top 10,
/// average and relative date criteria resolved to (Excel's `filterVal`, `val` and `maxVal`).
pub(crate) fn apply_filter(wb: &mut Workbook, sheet: usize) {
    let today = gridcraft_core::date::serial_in_system(wb.date_system, gridcraft_calc::now_serial()).floor();
    apply_filter_on(wb, sheet, today);
}

/// [`apply_filter`] with "today" given, so the relative date filters can be tested.
pub(crate) fn apply_filter_on(wb: &mut Workbook, sheet: usize, today: f64) {
    let sys = wb.date_system;
    let Some(sh) = wb.sheet(sheet) else { return };
    let Some(mut af) = sh.autofilter.clone() else { return };
    // An imported range can name rows past the grid; none of those exist to hide.
    let r = RangeRef::new(af.range.start, CellRef::new(af.range.end.row.min(gridcraft_core::MAX_ROWS - 1), af.range.end.col));
    let mut hide = Vec::new();
    // Resolve each Top 10 threshold, average and date span once, not per row.
    for (off, c) in af.criteria.iter_mut() {
        let col = r.start.col + *off;
        let nums = || -> Vec<f64> { (r.start.row + 1..=r.end.row).filter_map(|row| sh.value(CellRef::new(row, col)).as_f64()).collect() };
        match c {
            FilterCriterion::Top10 { bottom, percent, count, filter_val } => {
                let mut nums = nums();
                nums.sort_by(|a, b| if *bottom { a.partial_cmp(b) } else { b.partial_cmp(a) }.unwrap_or(Ordering::Equal));
                let k = if *percent { ((nums.len() as f64) * f64::from(*count) / 100.0).ceil() as usize } else { *count as usize };
                let k = k.clamp(1, nums.len().max(1));
                *filter_val = nums.get(k - 1).copied();
            }
            FilterCriterion::Dynamic { kind, val, max_val } if kind == "aboveAverage" || kind == "belowAverage" => {
                let nums = nums();
                *val = (!nums.is_empty()).then(|| nums.iter().sum::<f64>() / nums.len() as f64);
                *max_val = None;
            }
            FilterCriterion::Dynamic { kind, val, max_val } => {
                let span = dynamic_span(kind, today, sys);
                *val = span.map(|s| s.0);
                *max_val = span.map(|s| s.1);
            }
            _ => {}
        }
    }
    for row in r.start.row + 1..=r.end.row {
        let mut visible = true;
        for (off, crit) in &af.criteria {
            let c = CellRef::new(row, r.start.col + off);
            let v = sh.value(c);
            let text = crate::display::cell_text(wb, sh, c);
            let ok = match crit {
                FilterCriterion::Values { values, blanks } => {
                    if filter_blank(&v) {
                        *blanks
                    } else {
                        values.iter().any(|x| x.eq_ignore_ascii_case(&text) || x.eq_ignore_ascii_case(&v.display()))
                    }
                }
                FilterCriterion::Custom { a, b, and } => {
                    let x = custom_ok(&v, &text, &a.0, &a.1);
                    match b {
                        Some(b) => {
                            let y = custom_ok(&v, &text, &b.0, &b.1);
                            if *and { x && y } else { x || y }
                        }
                        None => x,
                    }
                }
                FilterCriterion::Top10 { bottom, filter_val, .. } => match (v.as_f64(), filter_val) {
                    (Some(n), Some(thr)) => {
                        if *bottom {
                            n <= *thr
                        } else {
                            n >= *thr
                        }
                    }
                    _ => false,
                },
                FilterCriterion::Dynamic { kind, val, max_val } => v.as_f64().is_some_and(|n| dynamic_ok(kind, n, *val, *max_val, sys)),
                FilterCriterion::FillColor(col) => wb.styles.get(sh.style_id(c)).fill.fg == *col,
                FilterCriterion::FontColor(col) => wb.styles.get(sh.style_id(c)).font.color == *col,
                // Icons are not evaluated here: the row keeps its current visibility.
                FilterCriterion::Icon { .. } => !sh.is_row_hidden(row),
            };
            if !ok {
                visible = false;
                break;
            }
        }
        hide.push((row, !visible));
    }
    let Some(shm) = wb.sheet_mut(sheet) else { return };
    for (row, h) in hide {
        if h {
            shm.rows.entry(row).or_default().hidden = true;
        } else if let Some(e) = shm.rows.get_mut(&row) {
            e.hidden = false;
        }
    }
    if let Some(live) = shm.autofilter.as_mut() {
        live.criteria = std::mem::take(&mut af.criteria);
    }
}

pub(super) fn clear_filter(s: &mut Session, _: &Json) -> Result<Json> {
    let sheet = s.doc()?.wb.active_sheet();
    crate::cmd::refuse_unless_allowed(s, sheet, "Filtering", |p| p.autofilter)?;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        if let Some(a) = sh.autofilter.as_mut() {
            a.criteria.clear();
        }
        apply_filter(&mut cx.wb, sheet);
        cx.structural = true;
        Ok(Json::Null)
    })
}

pub(super) fn reapply(s: &mut Session, _: &Json) -> Result<Json> {
    let sheet = s.doc()?.wb.active_sheet();
    crate::cmd::refuse_unless_allowed(s, sheet, "Filtering", |p| p.autofilter)?;
    edit(s, |cx| {
        apply_filter(&mut cx.wb, sheet);
        cx.structural = true;
        Ok(Json::Null)
    })
}

/// Distinct display values of a filter column (for the dropdown list).
pub fn filter_values(wb: &Workbook, sheet: usize, col: u32) -> Vec<(String, bool)> {
    let Some(sh) = wb.sheet(sheet) else { return vec![] };
    let Some(af) = &sh.autofilter else { return vec![] };
    let off = col.saturating_sub(af.range.start.col);
    let selected: Option<&FilterCriterion> = af.criteria.iter().find(|(o, _)| *o == off).map(|(_, c)| c);
    let mut seen = std::collections::BTreeMap::new();
    for row in af.range.start.row + 1..=af.range.end.row.min(af.range.start.row + 1_000_000) {
        let c = CellRef::new(row, col);
        let v = sh.value(c);
        let blank = filter_blank(&v);
        let t = if blank { "(Blanks)".to_string() } else { crate::display::cell_text(wb, sh, c) };
        let on = match selected {
            Some(FilterCriterion::Values { values, blanks }) => {
                if blank {
                    *blanks
                } else {
                    values.iter().any(|x| x.eq_ignore_ascii_case(&t))
                }
            }
            _ => true,
        };
        let key = if blank { sort_key(&Value::Empty) } else { sort_key(&v) };
        seen.entry((key, t)).or_insert(on);
    }
    let mut values: Vec<_> = seen.into_iter().collect();
    values.sort_by(|((a, ta), _), ((b, tb), _)| {
        a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then_with(|| gridcraft_core::compare_text(&a.2, &b.2)).then_with(|| ta.cmp(tb))
    });
    values.into_iter().map(|((_, t), on)| (t, on)).collect()
}

/// Which condition submenu a filter column's dropdown offers, as in Excel: the type most of the
/// column's values have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterColumnKind {
    /// Text Filters.
    Text,
    /// Number Filters.
    Number,
    /// Date Filters: numbers shown in a date format.
    Date,
}

/// The kind of values in column `col` of the sheet's AutoFilter (`Text` when it has none).
pub fn filter_column_kind(wb: &Workbook, sheet: usize, col: u32) -> FilterColumnKind {
    let Some(sh) = wb.sheet(sheet) else { return FilterColumnKind::Text };
    let Some(af) = &sh.autofilter else { return FilterColumnKind::Text };
    let (mut text, mut num, mut date) = (0usize, 0usize, 0usize);
    let mut date_style: std::collections::HashMap<gridcraft_model::StyleId, bool> = std::collections::HashMap::new();
    for row in af.range.start.row + 1..=af.range.end.row.min(af.range.start.row + 1_000_000) {
        let c = CellRef::new(row, col);
        let v = sh.value(c);
        if filter_blank(&v) {
            continue;
        }
        if v.is_number() {
            let id = sh.style_id(c);
            let is_date = *date_style.entry(id).or_insert_with(|| {
                gridcraft_numfmt::NumberFormat::parse(wb.styles.get(id).num_fmt.as_str()).kind() == gridcraft_numfmt::FormatKind::Date
            });
            if is_date {
                date += 1;
            } else {
                num += 1;
            }
        } else {
            text += 1;
        }
    }
    if date > num && date > text {
        FilterColumnKind::Date
    } else if num > text || (date > 0 && num + date > text) {
        FilterColumnKind::Number
    } else {
        FilterColumnKind::Text
    }
}

/// The criterion the sheet's AutoFilter applies to column `col`, if any.
pub fn filter_criterion_at(wb: &Workbook, sheet: usize, col: u32) -> Option<FilterCriterion> {
    let af = wb.sheet(sheet)?.autofilter.as_ref()?;
    let off = col.checked_sub(af.range.start.col)?;
    af.criteria.iter().find(|(o, _)| *o == off).map(|(_, c)| c.clone())
}

#[cfg(test)]
mod tests;
