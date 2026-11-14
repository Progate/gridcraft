//! Links to other workbooks (Excel's Data ▸ Queries & Connections ▸ Workbook Links).
//!
//! A formula that reads a sheet of another workbook, `=[予算.xlsx]予算!B3`, shows a value cached in
//! this workbook ([`gridcraft_model::ExternalLink`]); Excel keeps the same cache in the file, so a
//! saved workbook opens with its values. The cache is filled from the linked workbook: from its
//! window when it is open in this session, else from its file, found beside this workbook (or in
//! the folder the formula names). Entering such a formula fills what it reads; Refresh
//! (`data.updateLinks`) reads the links again; Change Source points the formulas at another
//! workbook; Break Link turns them into their values.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef, Value, split_external};
use gridcraft_formula::{Expr, Reference, SheetSel};
use gridcraft_model::{ExternalLink, ExternalSheet, Formula, Workbook};
use serde_json::{Value as Json, json};

use crate::cmd::{bad, edit, str_param};
use crate::{EngineError, Result, Session};

/// Most cells one update caches from all links together.
const MAX_CACHED_CELLS: u64 = 1_048_576;

/// The references into other workbooks in `e`: each one's sheet name (`[予算.xlsx]予算`) and range.
pub fn external_refs(e: &Expr) -> Vec<(String, RangeRef)> {
    let mut out = vec![];
    e.walk(&mut |x| {
        if let Expr::Ref(r @ Reference { sheet: SheetSel::Named(n), .. }) = x
            && split_external(n).is_some()
        {
            out.push((n.clone(), r.range()));
        }
    });
    out
}

/// Whether formula text may read another workbook (a cheap test before parsing).
fn may_link(text: &str) -> bool {
    text.contains('[') && text.contains(']') && text.contains('!')
}

/// Every reference into another workbook the workbook's formulas make: (sheet, cell, sheet name, range).
fn all_refs(wb: &Workbook) -> Vec<(usize, CellRef, String, RangeRef)> {
    let mut out = vec![];
    for (si, sh) in wb.sheets.iter().enumerate() {
        for (at, cell) in sh.cells.iter() {
            let Some(f) = cell.formula.as_ref().filter(|f| may_link(&f.text)) else { continue };
            if let Some(refs) = f.with_expr(external_refs) {
                out.extend(refs.into_iter().map(|(n, r)| (si, at, n, r)));
            }
        }
    }
    out
}

/// The link of `links` a formula's `dir[book]` names (see [`Workbook::external_link`]).
fn find(links: &[ExternalLink], dir: &str, book: &str) -> Option<usize> {
    gridcraft_model::external::find_link(links, dir, book)
}

/// Whether sheet name `n` (`[予算.xlsx]予算`) reads `link`.
fn reads(n: &str, link: &ExternalLink) -> bool {
    split_external(n).is_some_and(|(dir, book, _)| link.matches(dir, book))
}

/// What an update found for one link, for the Workbook Links list.
#[derive(Clone, Debug, PartialEq)]
enum Status {
    /// Read from the open workbook or its file.
    Updated,
    /// Not read this time (still showing the values cached before).
    Unchanged,
    /// The workbook could not be found or read: its cached values stay.
    Error(String),
}

impl Status {
    fn json(&self) -> Json {
        match self {
            Status::Updated => json!("OK"),
            Status::Unchanged => json!("Unknown"),
            Status::Error(e) => json!(format!("Error: {e}")),
        }
    }
}

/// The folder relative links are found from: the folder this workbook was read from, else the
/// folder of the file it is saved as, else (a new workbook) the current folder, where Excel's
/// Update Values asks first.
fn base_dir(s: &Session, uid: u64) -> Option<PathBuf> {
    let d = s.documents().iter().find(|d| d.uid == uid)?;
    d.wb.source_dir.clone().or_else(|| d.path.as_deref().and_then(crate::io::source_dir)).or_else(|| std::env::current_dir().ok())
}

/// Whether `dir` is an absolute folder (`/home/me/`, `C:\会計\`).
fn absolute(dir: &str) -> bool {
    Path::new(dir).is_absolute() || dir.as_bytes().get(1) == Some(&b':') || dir.starts_with('\\')
}

/// The linked workbook: the one open in this session with that file name (other than the
/// workbook `self_uid` itself), else its file.
fn source(s: &Session, self_uid: u64, link: &ExternalLink) -> std::result::Result<Arc<Workbook>, String> {
    let (dir, book) = link.path();
    let name_of = |p: &str| Path::new(p).file_name().map(|n| n.to_string_lossy().to_lowercase());
    let open = s.documents().iter().filter(|d| d.uid != self_uid).find(|d| {
        let name = d.path.as_deref().and_then(name_of).unwrap_or_else(|| d.title.to_lowercase());
        name == book.to_lowercase()
    });
    if let Some(d) = open {
        return Ok(d.wb.clone());
    }
    let path = if absolute(&dir) {
        PathBuf::from(format!("{dir}{book}"))
    } else {
        let base = base_dir(s, self_uid).ok_or_else(|| format!("{book} was not found (this workbook has no folder yet)"))?;
        base.join(format!("{dir}{book}"))
    };
    let p = path.to_string_lossy().to_string();
    let bytes = crate::io::read_file(&p).map_err(|_| format!("{book} was not found"))?;
    let (wb, _) = crate::io::open_bytes(&book, &bytes).map_err(|e| format!("{book} could not be read: {e}"))?;
    Ok(Arc::new(wb))
}

/// Which links an update reads.
#[derive(Clone, PartialEq)]
enum Which {
    /// The links of these sheet names (`[予算.xlsx]予算`): the ones formulas just entered read.
    Read(Vec<String>),
    /// Every link (Refresh All).
    All,
    /// The link at this position of the workbook's links.
    One(usize),
}

/// The workbook's links after an update: every workbook the formulas name has a link (a link
/// nothing reads any more goes), and the chosen ones hold the values of the cells the formulas
/// read, from the linked workbook. The statuses go with the links of the result.
fn updated(s: &Session, uid: u64, wb: &Workbook, which: Which) -> (Vec<ExternalLink>, Vec<Status>) {
    let refs = all_refs(wb);
    // The links keep their order (a file's `[1]`, `[2]`); new ones follow.
    let mut used: BTreeMap<usize, Vec<(String, RangeRef)>> = BTreeMap::new();
    let mut extra: Vec<(ExternalLink, Vec<(String, RangeRef)>)> = vec![];
    for (_, _, name, range) in &refs {
        let Some((dir, book, sheet)) = split_external(name) else { continue };
        let want = (sheet.to_string(), *range);
        match find(&wb.external_links, dir, book) {
            Some(i) => used.entry(i).or_default().push(want),
            None => match extra.iter_mut().find(|(l, _)| l.matches(dir, book)) {
                Some((_, w)) => w.push(want),
                None => extra.push((ExternalLink::new(dir, book), vec![want])),
            },
        }
    }
    let mut work: Vec<(Option<usize>, ExternalLink, Vec<(String, RangeRef)>)> =
        used.into_iter().filter_map(|(i, w)| wb.external_links.get(i).map(|l| (Some(i), l.clone(), w))).collect();
    work.extend(extra.into_iter().map(|(l, w)| (None, l, w)));
    let mut budget = MAX_CACHED_CELLS;
    let mut statuses = Vec::with_capacity(work.len());
    let mut links = Vec::with_capacity(work.len());
    for (old, mut link, want) in work {
        let go = match &which {
            Which::All => true,
            Which::Read(names) => names.iter().any(|n| reads(n, &link)),
            Which::One(k) => old == Some(*k),
        };
        let status = if go {
            match source(s, uid, &link) {
                Ok(src) => {
                    link.sheets = fill(&src, &want, &mut budget);
                    Status::Updated
                }
                Err(e) => Status::Error(e),
            }
        } else {
            Status::Unchanged
        };
        links.push(link);
        statuses.push(status);
    }
    (links, statuses)
}

/// The linked workbook's sheets with the values of the cells `want` reads (at most `budget` cells).
fn fill(src: &Workbook, want: &[(String, RangeRef)], budget: &mut u64) -> Vec<ExternalSheet> {
    let mut sheets: Vec<ExternalSheet> = src.sheets.iter().map(|sh| ExternalSheet { name: sh.name.clone(), cells: BTreeMap::new() }).collect();
    for (name, range) in want {
        let Some(i) = src.sheet_index(name) else { continue };
        let (Some(sh), Some(out)) = (src.sheet(i), sheets.get_mut(i)) else { continue };
        // A reference to the sheet's edge (`A:A`) reads only as far as the sheet's data.
        let mut r = *range;
        if r.end.row == MAX_ROWS - 1 || r.end.col == MAX_COLS - 1 {
            let Some(used) = sh.used_range() else { continue };
            r = RangeRef::new(r.start, CellRef::new(r.end.row.min(used.end.row.max(r.start.row)), r.end.col.min(used.end.col.max(r.start.col))));
        }
        let cells = u64::from(r.height()) * u64::from(r.width());
        if cells > *budget {
            continue;
        }
        *budget -= cells;
        for c in r.iter() {
            let v = sh.value(c);
            if !matches!(v, Value::Empty) {
                out.cells.insert(c, v);
            }
        }
    }
    sheets
}

/// Puts `links` on the active workbook and recalculates the formulas that read other workbooks.
fn apply(s: &mut Session, links: Vec<ExternalLink>) -> Result<()> {
    if s.doc()?.wb.external_links == links {
        return Ok(());
    }
    edit(s, |cx| {
        cx.wb.external_links = links;
        let cells: BTreeSet<(usize, CellRef)> = all_refs(&cx.wb).into_iter().map(|(si, at, _, _)| (si, at)).collect();
        for (si, at) in cells {
            cx.touch(si, at);
        }
        Ok(())
    })
}

/// After formulas were entered in `range` of `sheet`: when one reads another workbook, makes its
/// link and reads the values it needs, as Excel reads the workbook a typed reference names.
pub(crate) fn after_entry(s: &mut Session, sheet: usize, range: RangeRef) -> Result<()> {
    let d = s.doc()?;
    let Some(sh) = d.wb.sheet(sheet) else { return Ok(()) };
    let mut names: Vec<String> = vec![];
    for (_, c) in sh.cells.iter_range(range) {
        if let Some(f) = c.formula.as_ref().filter(|f| may_link(&f.text))
            && let Some(refs) = f.with_expr(external_refs)
        {
            for (n, _) in refs {
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
    }
    if names.is_empty() {
        return Ok(());
    }
    let (uid, wb) = (d.uid, d.wb.clone());
    let (links, _) = updated(s, uid, &wb, Which::Read(names));
    apply(s, links)
}

/// One link as the Workbook Links list shows it.
fn describe(wb: &Workbook, i: usize, link: &ExternalLink, status: Option<&Status>) -> Json {
    let (dir, book) = link.path();
    let formulas: BTreeSet<(usize, CellRef)> =
        all_refs(wb).into_iter().filter(|(_, _, n, _)| reads(n, link)).map(|(si, at, _, _)| (si, at)).collect();
    json!({
        "link": i + 1,
        "name": book,
        "folder": dir,
        "target": link.target,
        "sheets": link.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "formulas": formulas.len(),
        "status": status.map_or(json!("Unknown"), Status::json),
    })
}

fn describe_all(s: &Session, statuses: &[Status]) -> Result<Json> {
    let wb = &s.doc()?.wb;
    Ok(json!({"links": wb.external_links.iter().enumerate().map(|(i, l)| describe(wb, i, l, statuses.get(i))).collect::<Vec<_>>()}))
}

/// The link a command's `link` param names: its number (1 = first) or its file name.
fn link_param(wb: &Workbook, p: &Json, cmd: &str) -> Result<Option<usize>> {
    let n = wb.external_links.len();
    match p.get("link") {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Number(k)) => match k.as_u64().and_then(|k| usize::try_from(k).ok()) {
            Some(k) if (1..=n).contains(&k) => Ok(Some(k - 1)),
            _ => Err(bad(cmd, format!("`link` {k}: this workbook has {n} links"))),
        },
        Some(Json::String(t)) => {
            let (dir, book) = ExternalLink { target: t.clone(), sheets: vec![] }.path();
            wb.external_link(&dir, &book).map(Some).ok_or_else(|| bad(cmd, format!("no link to `{t}`")))
        }
        Some(other) => Err(bad(cmd, format!("`link` {other}: a number or a file name"))),
    }
}

/// `data.editLinks`: the links, as the Workbook Links pane lists them.
pub(crate) fn list(s: &mut Session, _p: &Json) -> Result<Json> {
    describe_all(s, &[])
}

/// `data.updateLinks`: reads the values again from the linked workbooks (all, or `link`).
pub(crate) fn update(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let (uid, wb) = (d.uid, d.wb.clone());
    let which = match link_param(&wb, p, "data.updateLinks")? {
        Some(i) => Which::One(i),
        None => Which::All,
    };
    let (links, statuses) = updated(s, uid, &wb, which);
    apply(s, links)?;
    describe_all(s, &statuses)
}

/// Gives every formula reference that reads `link` the sheet name `rename` returns for its
/// sheet; returns the cells changed.
fn rewrite(wb: &mut Workbook, link: &ExternalLink, rename: &dyn Fn(&str) -> String) -> Vec<(usize, CellRef)> {
    let mut changed = vec![];
    for (si, sh) in wb.sheets.iter_mut().enumerate() {
        let targets: Vec<(CellRef, Arc<Formula>)> = sh
            .cells
            .iter()
            .filter_map(|(at, c)| {
                c.formula
                    .as_ref()
                    .filter(|f| may_link(&f.text) && f.with_expr(|e| external_refs(e).iter().any(|(n, _)| reads(n, link))) == Some(true))
                    .map(|f| (at, f.clone()))
            })
            .collect();
        if targets.is_empty() {
            continue;
        }
        let sheet = Arc::make_mut(sh);
        for (at, formula) in targets {
            let Some(e) = formula.expr() else { continue };
            let e = e.map(&mut |x| match x {
                Expr::Ref(Reference { sheet: SheetSel::Named(n), kind }) if reads(&n, link) => {
                    let name = split_external(&n).map_or_else(|| n.clone(), |(_, _, sheet)| rename(sheet));
                    Expr::Ref(Reference { sheet: SheetSel::Named(name), kind })
                }
                other => other,
            });
            let Some(mut cell) = sheet.cells.get(at).cloned() else { continue };
            let mut nf = Formula::from_expr(e);
            nf.array = formula.array;
            nf.dynamic = formula.dynamic;
            cell.formula = Some(Arc::new(nf));
            sheet.cells.set(at, cell);
            changed.push((si, at));
        }
    }
    changed
}

/// `data.changeLinkSource`: points the formulas reading `link` at the workbook `source`
/// (`予算2026.xlsx`, or with its folder) and reads its values.
pub(crate) fn change_source(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "data.changeLinkSource";
    let wb = s.doc()?.wb.clone();
    let Some(i) = link_param(&wb, p, CMD)? else { return Err(bad(CMD, "missing `link`: the link's number or file name")) };
    let to = str_param(p, "source").map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad(CMD, "missing `source`: the workbook to link to"))?;
    let Some(old) = wb.external_links.get(i).cloned() else { return Err(bad(CMD, "no such link")) };
    let (dir, book) = match to.rfind(['/', '\\']) {
        Some(k) => (to.get(..=k).unwrap_or(""), to.get(k + 1..).unwrap_or("")),
        None => ("", to),
    };
    if book.is_empty() || book.contains(['[', ']']) {
        return Err(bad(CMD, format!("`source` {to} names no workbook file")));
    }
    let new = ExternalLink::new(dir, book);
    edit(s, |cx| {
        let changed = rewrite(&mut cx.wb, &old, &|sheet| new.sheet_ref(sheet));
        if let Some(l) = cx.wb.external_links.get_mut(i) {
            *l = new.clone();
        }
        for (si, at) in changed {
            cx.touch(si, at);
        }
        Ok(())
    })?;
    let d = s.doc()?;
    let (uid, wb) = (d.uid, d.wb.clone());
    let (links, statuses) = updated(s, uid, &wb, Which::One(i));
    apply(s, links)?;
    describe_all(s, &statuses)
}

/// `data.breakLinks`: the formulas reading `link` (or every link) become their values, as
/// Excel's Break Link does, and the link goes.
pub(crate) fn break_links(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "data.breakLinks";
    let wb = s.doc()?.wb.clone();
    let chosen: Vec<ExternalLink> = match link_param(&wb, p, CMD)? {
        Some(i) => wb.external_links.get(i).cloned().into_iter().collect(),
        None => wb.external_links.clone(),
    };
    if chosen.is_empty() {
        return Err(EngineError::Other("This workbook has no links to other workbooks.".into()));
    }
    let mut cells = BTreeSet::new();
    for (si, at, name, _) in all_refs(&wb) {
        if chosen.iter().any(|l| reads(&name, l)) {
            cells.insert((si, at));
        }
    }
    let count = cells.len();
    edit(s, |cx| {
        for (si, at) in cells {
            let sh = cx.sheet_mut(si)?;
            if let Some(mut cell) = sh.cells.get(at).cloned() {
                cell.formula = None;
                sh.set_cell(at, cell);
            }
        }
        cx.wb.external_links.retain(|l| !chosen.contains(l));
        Ok(())
    })?;
    Ok(json!({"cells": count, "links": s.doc()?.wb.external_links.len()}))
}

#[cfg(test)]
mod tests;
