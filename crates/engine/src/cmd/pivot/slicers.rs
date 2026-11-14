//! PivotTable slicers: PivotTable Analyze › Insert Slicer (スライサーの挿入), and Insert ›
//! Slicer with a cell of a PivotTable selected.
//!
//! A pivot slicer filters one source field of its PivotTable. Its selection is the PivotTable's
//! own filter on that field ([`super::analyze::set_selection`]): the field's hidden items when it
//! is in Rows, Columns or Filters, else a filter that doesn't show above the report, as Excel
//! keeps a slicer's selection in the pivot field's hidden items. So the slicer and the ▼ menu of
//! the field always agree.

use gridcraft_core::CellRef;
use gridcraft_model::{Anchor, AnchorMode, Slicer, Workbook};

use super::analyze::{field_items, selection, set_selection};
use super::{Json, Result, Session, bad, edit, f64_param, find_pivot, json, other, pv, str_param, u32_param};

/// (sheet, index) of the PivotTable with id `id`.
pub(crate) fn pivot_by_id(wb: &Workbook, id: u32) -> Option<(usize, usize)> {
    wb.sheets.iter().enumerate().find_map(|(si, sh)| sh.pivots.iter().position(|p| p.id == id).map(|pi| (si, pi)))
}

/// A pivot slicer's tiles as `(label, selected)`: the field's items, every one selected when the
/// PivotTable shows them all.
pub fn pivot_slicer_items(wb: &Workbook, sl: &Slicer) -> Vec<(String, bool)> {
    let Some((si, pi)) = sl.pivot.and_then(|id| pivot_by_id(wb, id)) else { return vec![] };
    let Some(pt) = wb.sheet(si).and_then(|sh| sh.pivots.get(pi)) else { return vec![] };
    let Ok(src) = pv::read_source(wb, &pt.source, si) else { return vec![] };
    let sel = selection(pt, &sl.column);
    field_items(wb, pt, &src, &sl.column)
        .into_iter()
        .map(|l| {
            let on = sel.as_ref().is_none_or(|v| v.iter().any(|x| x.eq_ignore_ascii_case(&l)));
            (l, on)
        })
        .collect()
}

/// `pivot.insertSlicer` (and Insert › Slicer in a PivotTable): one slicer per field, cascading
/// right of the report. With no field, the Insert Slicers dialog lists the fields.
pub(crate) fn insert_slicer(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "pivot.insertSlicer";
    let (si, pi) = find_pivot(s, p)?;
    let mut names: Vec<String> = match p.get("fields") {
        Some(Json::Array(a)) => a.iter().take(64).filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => vec![],
    };
    if let Some(f) = str_param(p, "field").or_else(|| str_param(p, "column")) {
        names.insert(0, f.to_string());
    }
    let d = s.doc()?;
    let pt = d.wb.sheet(si).and_then(|sh| sh.pivots.get(pi)).ok_or_else(|| other("There's no such PivotTable."))?;
    let src = pv::read_source(&d.wb, &pt.source, si).map_err(other)?;
    if names.is_empty() {
        s.ui_requests.push(crate::UiRequest::Dialog("insertSlicer".into(), json!({"pivot": pt.name, "fields": src.headers})));
        return Ok(json!({"slicers": []}));
    }
    let mut fields = Vec::new();
    for n in &names {
        let c = src.col(n).ok_or_else(|| bad(C, format!("no field `{n}` in the source data")))?;
        let name = src.headers.get(c).cloned().unwrap_or_default();
        if !fields.contains(&name) {
            fields.push(name);
        }
    }
    let at = match str_param(p, "at") {
        Some(a) => Some(crate::cmd::cell_param_on(s, p, "at", si)?.ok_or_else(|| bad(C, format!("not a cell: `{a}`")))?),
        None => None,
    };
    let d = s.doc()?;
    let pt = d.wb.sheet(si).and_then(|sh| sh.pivots.get(pi)).ok_or_else(|| other("There's no such PivotTable."))?;
    let pid = pt.id;
    let right = pt.last_range.map_or(pt.anchor.col, |r| r.end.col);
    let top = pt.last_range.map_or(pt.anchor.row, |r| r.start.row);
    let at = at.unwrap_or_else(|| CellRef::new(top, right.saturating_add(2).min(gridcraft_core::MAX_COLS - 1)));
    let columns = u32_param(p, "columns").unwrap_or(1).clamp(1, 10);
    // Excel's default slicer is 2.54 cm × 4.57 cm (144 × 259 pt)… GridCraft keeps its table
    // slicers' size so both kinds look alike.
    let width = f64_param(p, "width").unwrap_or(168.0).clamp(60.0, 10000.0) as f32;
    let height = f64_param(p, "height").unwrap_or(200.0).clamp(60.0, 10000.0) as f32;
    let first = d.wb.next_object_id();
    let slicers: Vec<Slicer> = fields
        .iter()
        .enumerate()
        .map(|(k, f)| {
            // Several slicers inserted at once cascade down and to the right, as in Excel.
            let off = k as f32 * 24.0;
            Slicer {
                id: first + k as u32,
                table: String::new(),
                column: f.clone(),
                caption: f.clone(),
                anchor: Anchor { cell: at, dx: 4.0 + off, dy: 4.0 + off, width, height, mode: AnchorMode::MoveOnly },
                selected: None,
                columns,
                pivot: Some(pid),
            }
        })
        .collect();
    let ids: Vec<u32> = slicers.iter().map(|x| x.id).collect();
    edit(s, |cx| {
        cx.sheet_mut(si)?.slicers.extend(slicers);
        cx.structural = true;
        Ok(json!({"slicers": ids, "slicer": ids.first()}))
    })
}

/// Sets a pivot slicer's selection (`None`: all) on its PivotTable and refreshes the report.
pub(crate) fn set_pivot_slicer(s: &mut Session, sl: &Slicer, selected: Option<Vec<String>>) -> Result<Json> {
    let pid = sl.pivot.ok_or_else(|| other("The slicer isn't connected to a PivotTable."))?;
    let (si, pi) = pivot_by_id(&s.doc()?.wb, pid).ok_or_else(|| other("The slicer's PivotTable no longer exists."))?;
    let field = sl.column.clone();
    edit(s, |cx| {
        let mut pt = cx.wb.sheet(si).and_then(|sh| sh.pivots.get(pi)).cloned().ok_or_else(|| other("There's no such PivotTable."))?;
        set_selection(&mut pt, &field, selected.clone());
        if let Some(slot) = cx.sheet_mut(si)?.pivots.get_mut(pi) {
            *slot = pt;
        }
        pv::refresh(&mut cx.wb, si, pi, false).map_err(other)?;
        cx.structural = true;
        Ok(json!({"selected": selected}))
    })
}
