//! Links to other workbooks as Excel saves them: `<externalReferences>` in the workbook, one
//! `xl/externalLinks/externalLinkN.xml` per linked workbook (its sheet names and the values of the
//! cells formulas read) with the workbook's file as an external relationship, and formulas that
//! name the workbook by its number, `[1]予算!B3`. The model names it by file, `[予算.xlsx]予算!B3`,
//! as Excel shows it.

use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::{CellError, CellRef, Value, split_external};
use gridcraft_formula::{Expr, Reference, SheetSel};
use gridcraft_model::{ExternalLink, ExternalSheet, Formula, Workbook};

use crate::read::Ctx;
use crate::write::{NS_MAIN, NS_REL, Out, Rels, XML_DECL};
use crate::xml::{self, esc, esc_attr};

/// Largest externalLink part read.
const MAX_LINK_PART: u64 = 64 * 1024 * 1024;
/// Most links a workbook may have (Excel's `[n]` numbers are small; this bounds hostile files).
const MAX_LINKS: usize = 4096;

const CT_EXTERNAL_LINK: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml";

fn may_link(text: &str) -> bool {
    text.contains('[') && text.contains(']') && text.contains('!')
}

/// `e` with each reference into another workbook given the sheet name `rename` returns
/// (`None` keeps it); `None` when nothing changed.
fn renamed(e: &Expr, rename: &mut dyn FnMut(&str) -> Option<String>) -> Option<Expr> {
    let mut hit = false;
    let out = e.clone().map(&mut |x| match x {
        Expr::Ref(Reference { sheet: SheetSel::Named(n), kind }) if split_external(&n).is_some() => match rename(&n) {
            Some(m) => {
                hit = true;
                Expr::Ref(Reference { sheet: SheetSel::Named(m), kind })
            }
            None => Expr::Ref(Reference { sheet: SheetSel::Named(n), kind }),
        },
        other => other,
    });
    hit.then_some(out)
}

/// Rewrites every cell formula's references into other workbooks with `rename`.
fn rename_formulas(wb: &mut Workbook, rename: &mut dyn FnMut(&str) -> Option<String>) {
    for sh in &mut wb.sheets {
        let changed: Vec<(CellRef, Formula)> = sh
            .cells
            .iter()
            .filter_map(|(at, c)| {
                let f = c.formula.as_ref().filter(|f| may_link(&f.text))?;
                let e = f.with_expr(|e| renamed(e, rename))??;
                let mut nf = Formula::from_expr(e);
                nf.array = f.array;
                nf.dynamic = f.dynamic;
                Some((at, nf))
            })
            .collect();
        if changed.is_empty() {
            continue;
        }
        let sheet = Arc::make_mut(sh);
        for (at, f) in changed {
            if let Some(mut cell) = sheet.cells.get(at).cloned() {
                cell.formula = Some(Arc::new(f));
                sheet.cells.set(at, cell);
            }
        }
    }
}

// ---------------------------------------------------------------- writing

/// The workbook as the file spells it, with its links in file order, when its formulas read other
/// workbooks: every workbook a formula names gets a link (with the values cached so far, or none),
/// links nothing reads are left out, and the formulas name the workbooks by number. `None` when no
/// formula reads another workbook, or when links kept from the file as they were (not modelled)
/// already take the numbers.
pub(crate) fn file_form(wb: &Workbook) -> Option<(Workbook, Vec<ExternalLink>)> {
    if wb.preserved.as_ref().is_some_and(|p| p.elements.iter().any(|e| e.slot == "externalReferences")) {
        return None;
    }
    let mut links: Vec<ExternalLink> = vec![];
    let mut any = false;
    for sh in &wb.sheets {
        for (_, c) in sh.cells.iter() {
            let Some(f) = c.formula.as_ref().filter(|f| may_link(&f.text)) else { continue };
            f.with_expr(|e| {
                e.walk(&mut |x| {
                    if let Expr::Ref(Reference { sheet: SheetSel::Named(n), .. }) = x
                        && let Some((dir, book, sheet)) = split_external(n)
                    {
                        any = true;
                        let i = match gridcraft_model::external::find_link(&links, dir, book) {
                            Some(i) => i,
                            None => {
                                let known = wb.external_link(dir, book).and_then(|i| wb.external_links.get(i));
                                links.push(known.cloned().unwrap_or_else(|| ExternalLink::new(dir, book)));
                                links.len() - 1
                            }
                        };
                        // sheetNames lists every sheet a formula names.
                        if let Some(l) = links.get_mut(i)
                            && l.sheet(sheet).is_none()
                        {
                            l.sheets.push(ExternalSheet { name: sheet.to_string(), cells: Default::default() });
                        }
                    }
                });
            });
        }
    }
    if !any || links.len() > MAX_LINKS {
        return None;
    }
    let mut out = wb.clone();
    rename_formulas(&mut out, &mut |n| {
        let (dir, book, sheet) = split_external(n)?;
        let i = gridcraft_model::external::find_link(&links, dir, book)?;
        Some(format!("[{}]{sheet}", i + 1))
    });
    Some((out, links))
}

/// A cached value as an externalLink `<cell>`.
fn cell_xml(at: CellRef, v: &Value) -> Option<String> {
    let r = at.a1();
    Some(match v {
        Value::Number(n) => format!("<cell r=\"{r}\"><v>{}</v></cell>", crate::xml::num(*n)),
        Value::Text(t) => format!("<cell r=\"{r}\" t=\"str\"><v>{}</v></cell>", esc(t)),
        Value::Bool(b) => format!("<cell r=\"{r}\" t=\"b\"><v>{}</v></cell>", u8::from(*b)),
        Value::Error(e) => format!("<cell r=\"{r}\" t=\"e\"><v>{}</v></cell>", esc(e.as_str())),
        _ => return None,
    })
}

/// A relationship target for the linked file: spaces as `%20`, as Excel writes them.
fn target_uri(t: &str) -> String {
    t.replace('%', "%25").replace(' ', "%20")
}

/// Writes the links' parts and returns the workbook's `<externalReferences>`.
pub(crate) fn write(links: &[ExternalLink], out: &mut Out, wb_rels: &mut Rels) -> String {
    let mut refs = String::from("<externalReferences>");
    for (i, link) in links.iter().enumerate() {
        let n = i + 1;
        let part = format!("xl/externalLinks/externalLink{n}.xml");
        let mut rels = Rels::default();
        let rid = rels.add_external("externalLinkPath", &target_uri(&link.target));
        let mut x = format!("{XML_DECL}<externalLink xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\"><externalBook r:id=\"{rid}\">");
        if !link.sheets.is_empty() {
            x.push_str("<sheetNames>");
            for s in &link.sheets {
                let _ = write!(x, "<sheetName val=\"{}\"/>", esc_attr(&s.name));
            }
            x.push_str("</sheetNames><sheetDataSet>");
            for (si, s) in link.sheets.iter().enumerate() {
                if s.cells.is_empty() {
                    let _ = write!(x, "<sheetData sheetId=\"{si}\"/>");
                    continue;
                }
                let _ = write!(x, "<sheetData sheetId=\"{si}\">");
                let mut row: Option<u32> = None;
                for (at, v) in &s.cells {
                    let Some(c) = cell_xml(*at, v) else { continue };
                    if row != Some(at.row) {
                        if row.is_some() {
                            x.push_str("</row>");
                        }
                        let _ = write!(x, "<row r=\"{}\">", at.row + 1);
                        row = Some(at.row);
                    }
                    x.push_str(&c);
                }
                if row.is_some() {
                    x.push_str("</row>");
                }
                x.push_str("</sheetData>");
            }
            x.push_str("</sheetDataSet>");
        }
        x.push_str("</externalBook></externalLink>");
        out.part(&part, Some(CT_EXTERNAL_LINK), x.into_bytes());
        out.part(&format!("xl/externalLinks/_rels/externalLink{n}.xml.rels"), None, rels.xml().into_bytes());
        let id = wb_rels.add("externalLink", &format!("externalLinks/externalLink{n}.xml"));
        let _ = write!(refs, "<externalReference r:id=\"{id}\"/>");
    }
    refs.push_str("</externalReferences>");
    refs
}

// ---------------------------------------------------------------- reading

/// `%XX` escapes decoded (a relationship target is a URI); text that doesn't decode to UTF-8 is
/// kept as it was.
fn percent_decode(t: &str) -> String {
    if !t.contains('%') {
        return t.to_string();
    }
    let b = t.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(h) = t.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(h);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| t.to_string())
}

fn cached_value(c: &xml::El) -> Option<Value> {
    let v = c.child("v")?.text.as_str();
    Some(match c.attr("t") {
        Some("str" | "s" | "inlineStr") => Value::Text(v.into()),
        Some("b") => Value::Bool(v.trim() == "1" || v.trim().eq_ignore_ascii_case("true")),
        Some("e") => Value::Error(CellError::ALL.iter().copied().find(|e| e.as_str().eq_ignore_ascii_case(v.trim())).unwrap_or(CellError::Ref)),
        _ => Value::number(v.trim().parse::<f64>().ok()?),
    })
}

/// The workbook's links to other workbooks, when every one is a workbook link (`externalBook`).
/// `None` when it has none, or one is another kind (DDE, OLE) or can't be read: those links are
/// then kept as they were, unmodelled.
pub(crate) fn read(cx: &mut Ctx<'_>, wb_part: &str, wb_xml: &xml::El) -> Option<Vec<ExternalLink>> {
    let ids: Vec<String> = wb_xml.child("externalReferences")?.kids("externalReference").filter_map(|x| x.attr("id").map(str::to_string)).collect();
    if ids.is_empty() || ids.len() > MAX_LINKS {
        return None;
    }
    let rels = cx.pkg.rels(wb_part).ok()?;
    let mut links = Vec::with_capacity(ids.len());
    for id in ids {
        let rel = rels.iter().find(|r| r.id == id && r.kind.eq_ignore_ascii_case("externalLink") && !r.external)?;
        let part = rel.target.clone();
        let bytes = cx.pkg.read_limited(&part, MAX_LINK_PART).ok()??;
        let root = xml::parse(&bytes).ok()?;
        let book = root.child("externalBook")?;
        let path_id = book.attr("id")?;
        let target = cx.pkg.rels(&part).ok()?.into_iter().find(|r| r.id == path_id && r.external)?.target;
        let mut link = ExternalLink { target: percent_decode(&target), sheets: vec![] };
        for name in book.child("sheetNames").into_iter().flat_map(|s| s.kids("sheetName")) {
            link.sheets.push(ExternalSheet { name: name.attr("val").unwrap_or("").to_string(), cells: Default::default() });
        }
        for data in book.child("sheetDataSet").into_iter().flat_map(|s| s.kids("sheetData")) {
            let Some(sheet) = data.attr_u32("sheetId").and_then(|i| link.sheets.get_mut(i as usize)) else { continue };
            for c in data.kids("row").flat_map(|r| r.kids("cell")) {
                if let (Some(at), Some(v)) = (c.attr("r").and_then(CellRef::parse), cached_value(c)) {
                    sheet.cells.insert(at, v);
                }
            }
        }
        links.push(link);
    }
    Some(links)
}

/// The formulas read from the file name linked workbooks by number (`[1]予算!B3`): they name them
/// by file as Excel shows them (`[予算.xlsx]予算!B3`, with the folder when the link has one).
pub(crate) fn name_books(wb: &mut Workbook) {
    if wb.external_links.is_empty() {
        return;
    }
    let links = wb.external_links.clone();
    rename_formulas(wb, &mut |n| {
        let (dir, book, sheet) = split_external(n)?;
        if !dir.is_empty() || !book.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let i = book.parse::<usize>().ok()?.checked_sub(1)?;
        Some(links.get(i)?.sheet_ref(sheet))
    });
}
