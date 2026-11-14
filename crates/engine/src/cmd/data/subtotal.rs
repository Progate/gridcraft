//! Data ▸ Outline ▸ Subtotal and its Remove All, as Excel's 集計の設定 dialog runs them.
//!
//! A subtotal row goes below (or above) each run of equal values in the At each change in column,
//! with `SUBTOTAL` over the group in each Add subtotal to column, and a grand total row closes the
//! list. The outline gets three levels as Excel's does: the list (level 1, summarised by the grand
//! total), each group's detail (level 2, summarised by its subtotal row), so the level buttons are
//! 1 (grand total), 2 (subtotals and grand total) and 3 (everything). Replace current subtotals
//! (on by default) first takes the earlier subtotal rows and their outline away, so running it
//! again does not stack 「駅前店 集計 集計」 rows.

use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::Sheet;
use serde_json::{Value as Json, json};

use super::{EngineError, Result, Session, bad, bool_param, data_target, edit, str_param};

/// The Use function list: `function` name, `SUBTOTAL` code, and the caption words in English and
/// in Japanese Excel (`東 集計`, `東 平均`), in the order of Excel's list.
const FUNCTIONS: [(&str, u8, &str, &str); 11] = [
    ("sum", 9, "Total", "集計"),
    ("count", 3, "Count", "個数"),
    ("average", 1, "Average", "平均"),
    ("max", 4, "Max", "最大"),
    ("min", 5, "Min", "最小"),
    ("product", 6, "Product", "積"),
    ("countNums", 2, "Count", "数値の個数"),
    ("stdDev", 7, "StdDev", "標本標準偏差"),
    ("stdDevP", 8, "StdDevp", "標準偏差"),
    ("var", 10, "Var", "標本分散"),
    ("varP", 11, "Varp", "分散"),
];

/// The `function` names `data.subtotal` takes, in the order of Excel's Use function list.
pub fn subtotal_functions() -> impl Iterator<Item = &'static str> {
    FUNCTIONS.iter().map(|f| f.0)
}

/// The header texts of the list's columns, for the At each change in and Add subtotal to lists
/// (`Column A` style captions stand in for empty headers, as Excel's do).
pub fn subtotal_fields(sh: &Sheet, r: RangeRef) -> Vec<String> {
    (r.start.col..=r.end.col)
        .map(|c| {
            let t = sh.value(CellRef::new(r.start.row, c)).display();
            if t.trim().is_empty() { format!("Column {}", gridcraft_core::col_to_letters(c)) } else { t }
        })
        .collect()
}

/// The list Subtotal works on from the selection (the current region of the active cell, as the
/// command takes it with no `range`) and its fields, for the dialog.
pub fn subtotal_list(s: &Session) -> Option<(RangeRef, Vec<String>)> {
    let r = super::data_range(s, &Json::Null).ok()?;
    let sh = s.doc().ok()?.wb.active()?;
    Some((r, subtotal_fields(sh, r)))
}

/// A column of the list named by its header text (`店舗`, matched without regard to case), by
/// its letter (`A`), or by its position from the list's first column (0). Header text wins, so a
/// header called `ID` names that column rather than column ID.
fn field(sh: &Sheet, r: RangeRef, header: bool, v: &Json) -> Option<u32> {
    match v {
        Json::String(t) => {
            let t = t.trim();
            if header
                && let Some(c) =
                    (r.start.col..=r.end.col).find(|c| sh.value(CellRef::new(r.start.row, *c)).display().trim().to_lowercase() == t.to_lowercase())
            {
                return Some(c);
            }
            // `Column C`, the caption an empty header has in the dialog.
            let letters = t.strip_prefix("Column ").unwrap_or(t);
            gridcraft_core::letters_to_col(letters.trim_start_matches('$')).filter(|c| (r.start.col..=r.end.col).contains(c))
        }
        Json::Number(n) => n.as_u64().and_then(|o| u32::try_from(o).ok()).and_then(|o| r.start.col.checked_add(o)).filter(|c| *c <= r.end.col),
        _ => None,
    }
}

/// Whether row `row` of the list is a subtotal row: one of its cells holds a `SUBTOTAL` formula.
fn is_subtotal_row(sh: &Sheet, r: RangeRef, row: u32) -> bool {
    (r.start.col..=r.end.col).any(|c| {
        sh.cell(CellRef::new(row, c))
            .and_then(|cell| cell.formula.as_ref())
            .is_some_and(|f| f.text.trim_start_matches('=').get(..9).is_some_and(|h| h.eq_ignore_ascii_case("SUBTOTAL(")))
    })
}

/// Remove All: deletes the list's subtotal rows, clears its row outline and the page breaks
/// inside it. Returns the list's range afterwards.
fn remove_all(s: &mut Session, sheet: usize, r: RangeRef, header: bool) -> Result<(RangeRef, u32)> {
    let first = r.start.row + u32::from(header);
    let sh = s.doc()?.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
    let rows: Vec<u32> = (first..=r.end.row).filter(|row| is_subtotal_row(sh, r, *row)).collect();
    for row in rows.iter().rev() {
        s.execute("home.deleteRows", json!({"sheet": sheet, "rows": format!("{}:{}", row + 1, row + 1)}))?;
    }
    let removed = rows.len() as u32;
    let r = RangeRef::new(r.start, CellRef::new(r.end.row.saturating_sub(removed).max(r.start.row), r.end.col));
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        for row in first..=r.end.row {
            if let Some(e) = sh.rows.get_mut(&row) {
                e.outline = 0;
                e.hidden = false;
                e.collapsed = false;
            }
        }
        // The grand total's mark (+) sat on the row just below the list.
        if let Some(e) = sh.rows.get_mut(&(r.end.row + 1)) {
            e.collapsed = false;
        }
        sh.print.row_breaks.retain(|b| !(first + 1..=r.end.row + 1).contains(b));
        cx.structural = true;
        Ok(())
    })?;
    Ok((r, removed))
}

/// `data.removeSubtotals`: the dialog's Remove All button.
pub(super) fn remove_subtotals(s: &mut Session, p: &Json) -> Result<Json> {
    let (sheet, r) = data_target(s, p, "data.removeSubtotals")?;
    let header = bool_param(p, "header").unwrap_or(true);
    let (r, removed) = remove_all(s, sheet, r, header)?;
    Ok(json!({"removed": removed, "range": r.a1()}))
}

/// Copies each list cell's formatting from row `from` onto the new row `to`, as an inserted row
/// takes the formatting of its neighbour, so a subtotal of `#,##0` amounts reads 1,402,927.
fn copy_row_style(s: &mut Session, sheet: usize, r: RangeRef, from: u32, to: u32) -> Result<()> {
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        for c in r.start.col..=r.end.col {
            let style = sh.style_id(CellRef::new(from, c));
            sh.set_style(CellRef::new(to, c), style);
        }
        Ok(())
    })
}

pub(super) fn subtotal(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "data.subtotal";
    let (sheet, r) = data_target(s, p, CMD)?;
    let header = bool_param(p, "header").unwrap_or(true);
    let replace = bool_param(p, "replace").unwrap_or(true);
    let below = bool_param(p, "summaryBelow").unwrap_or(true);
    let page_breaks = bool_param(p, "pageBreaks").unwrap_or(false);
    let given = str_param(p, "function").unwrap_or("sum");
    let Some(&(_, code, en, ja)) = FUNCTIONS.iter().find(|f| f.0.eq_ignore_ascii_case(given)) else {
        return Err(bad(CMD, format!("unknown function `{given}`")));
    };
    let (group_col, cols) = {
        let sh = s.doc()?.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
        let group_col = match p.get("groupBy") {
            None | Some(Json::Null) => r.start.col,
            Some(v) => field(sh, r, header, v).ok_or_else(|| bad(CMD, format!("`groupBy` {v} is not a column of the list {}", r.a1())))?,
        };
        let cols: Vec<u32> = match p.get("columns").and_then(Json::as_array) {
            Some(a) => a
                .iter()
                .map(|v| field(sh, r, header, v).ok_or_else(|| bad(CMD, format!("`columns` {v} is not a column of the list {}", r.a1()))))
                .collect::<Result<_>>()?,
            None => vec![r.end.col],
        };
        (group_col, cols)
    };
    if cols.is_empty() {
        return Err(bad(CMD, "choose at least one column in `columns` (Add subtotal to)"));
    }
    let (r, removed) = if replace { remove_all(s, sheet, r, header)? } else { (r, 0) };
    // The total rows' captions: `East Total` and `Grand Total`, or as Japanese Excel writes them,
    // `東 集計` and `総計` (`東 平均` and `全体の平均`).
    let (label, grand) = if s.doc()?.wb.region.is_japanese() {
        (ja, if code == 9 { "総計".to_string() } else { format!("全体の{ja}") })
    } else {
        (en, format!("Grand {en}"))
    };
    let first = r.start.row + u32::from(header);
    if first > r.end.row {
        return Err(bad(CMD, format!("the list {} has no rows to subtotal", r.a1())));
    }
    // Group boundaries: runs of equal values in the At each change in column.
    let groups: Vec<(u32, u32, String)> = {
        let sh = s.doc()?.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
        let mut groups = Vec::new();
        let mut start = first;
        let mut prev = sh.value(CellRef::new(first, group_col)).display();
        for row in first + 1..=r.end.row + 1 {
            let cur = if row > r.end.row { String::new() } else { sh.value(CellRef::new(row, group_col)).display() };
            if row > r.end.row || cur != prev {
                groups.push((start, row - 1, std::mem::replace(&mut prev, cur)));
                start = row;
            }
        }
        groups
    };
    if s.doc()?.wb.sheet(sheet).is_some_and(|sh| sh.summary_below != below) {
        s.execute("data.outlineSettings", json!({"sheet": sheet, "summaryBelow": below}))?;
    }
    let set = |s: &mut Session, row: u32, col: u32, input: String| {
        s.execute("cell.set", json!({"sheet": sheet, "cell": CellRef::new(row, col).a1(), "input": input})).map(|_| ())
    };
    let bold = |s: &mut Session, row: u32| {
        s.execute("home.bold", json!({"sheet": sheet, "range": CellRef::new(row, group_col).a1(), "on": true})).map(|_| ())
    };
    // Bottom-up, so the rows still to come keep their indices. Each group's rows shift by the
    // rows inserted above it: `k` subtotal rows before group k, plus the grand total row when the
    // summaries sit above.
    let n = groups.len() as u32;
    let mut summary_rows = Vec::with_capacity(groups.len());
    for (k, (g0, g1, name)) in groups.iter().enumerate().rev() {
        let k = k as u32;
        let shift = k + u32::from(!below);
        let at = if below { g1 + 1 } else { *g0 };
        s.execute("home.insertRows", json!({"sheet": sheet, "rows": format!("{}:{}", at + 1, at + 1)}))?;
        // The detail rows after this insertion (only this group's own row moved them, if above).
        let (d0, d1) = if below { (*g0, *g1) } else { (g0 + 1, g1 + 1) };
        copy_row_style(s, sheet, r, if below { *g1 } else { g0 + 1 }, at)?;
        set(s, at, group_col, format!("{name} {label}"))?;
        for c in &cols {
            let range = RangeRef::new(CellRef::new(d0, *c), CellRef::new(d1, *c)).a1();
            set(s, at, *c, format!("=SUBTOTAL({code},{range})"))?;
        }
        bold(s, at)?;
        summary_rows.push((at + shift, d0 + shift, d1 + shift));
    }
    summary_rows.reverse();
    // The list's rows now: the data and subtotal rows, then (or before them) the grand total.
    let last = r.end.row + n;
    let grand_row = if below { last + 1 } else { first };
    s.execute("home.insertRows", json!({"sheet": sheet, "rows": format!("{}:{}", grand_row + 1, grand_row + 1)}))?;
    let (span0, span1) = if below { (first, last) } else { (first + 1, last + 1) };
    copy_row_style(s, sheet, r, if below { last } else { first + 1 }, grand_row)?;
    set(s, grand_row, group_col, grand)?;
    for c in &cols {
        let range = RangeRef::new(CellRef::new(span0, *c), CellRef::new(span1, *c)).a1();
        set(s, grand_row, *c, format!("=SUBTOTAL({code},{range})"))?;
    }
    bold(s, grand_row)?;
    // The outline: the list at level 1 (its summary the grand total), each group's detail at
    // level 2 (its summary the subtotal row).
    s.execute("data.group", json!({"sheet": sheet, "rows": format!("{}:{}", span0 + 1, span1 + 1)}))?;
    for (_, d0, d1) in &summary_rows {
        s.execute("data.group", json!({"sheet": sheet, "rows": format!("{}:{}", d0 + 1, d1 + 1)}))?;
    }
    if page_breaks && summary_rows.len() > 1 {
        // A page break after each group's subtotal row (before it, with summaries above), so each
        // group prints on its own page.
        edit(s, |cx| {
            let pr = &mut cx.sheet_mut(sheet)?.print;
            let breaks: Vec<u32> = if below {
                summary_rows.iter().take(summary_rows.len() - 1).map(|(row, _, _)| row + 1).collect()
            } else {
                summary_rows.iter().skip(1).map(|(row, _, _)| *row).collect()
            };
            for b in breaks {
                if !pr.row_breaks.contains(&b) {
                    pr.row_breaks.push(b);
                }
            }
            pr.row_breaks.sort_unstable();
            Ok(())
        })?;
    }
    Ok(json!({"groups": n, "removed": removed, "grandTotal": CellRef::new(grand_row, group_col).a1()}))
}

#[cfg(test)]
mod tests;
