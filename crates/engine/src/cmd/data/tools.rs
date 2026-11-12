use super::{
    Cell, CellRef, EngineError, Json, RangeRef, Result, Session, bad, bool_param, cell_param_on, col_param, data_target, edit, guess_header, json,
    ok, str_param, target_range, target_sheet,
};

// ---------------------------------------------------------------- tools

/// Most cells (rows times compared columns) one Remove Duplicates call reads.
const MAX_COMPARED_CELLS: u64 = 50_000_000;

pub(super) fn remove_duplicates(s: &mut Session, p: &Json) -> Result<Json> {
    let (sheet, r) = data_target(s, p, "data.removeDuplicates")?;
    let d = s.doc()?;
    let sh = d.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
    // Rows and columns past the data are blank: only the part with data is compared (whole
    // columns compared a million blank rows and reported them as removed duplicates).
    let r = sh.used_range().and_then(|u| r.intersection(&u)).unwrap_or(RangeRef::cell(r.start));
    let header = bool_param(p, "header").unwrap_or_else(|| guess_header(&d.wb, sh, r));
    let cols: Vec<u32> = p
        .get("columns")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|c| col_param(Some(c), r.start.col)).collect())
        .unwrap_or_else(|| (r.start.col..=r.end.col).collect());
    // With no column to compare, every row key is empty: the first row would survive and every
    // other row would be deleted as a duplicate. A column outside the range compares data the
    // user didn't select. Excel refuses both.
    if cols.is_empty() || cols.iter().any(|c| !(r.start.col..=r.end.col).contains(c)) {
        return Err(bad("data.removeDuplicates", "`columns` must name at least one column of the range"));
    }
    // Rows are compared one by one, so a sparse sheet's enclosing rectangle is bounded like a list.
    if r.count() > MAX_COMPARED_CELLS {
        return Err(bad("data.removeDuplicates", "the range is too large"));
    }
    let start = r.start.row + header as u32;
    let mut removed = 0;
    edit(s, |cx| {
        let Some(sh) = cx.wb.sheet(sheet) else { return Ok(()) };
        let mut seen = std::collections::HashSet::new();
        let mut keep: Vec<u32> = Vec::new();
        // Cells are compared by the text they show, as in Excel: the same date in two formats
        // isn't a duplicate, while 1.01 and 1.02 shown as 1.0 are.
        let shown = |c: CellRef| crate::display::format(&sh.value(c), cx.wb.styles.get(sh.style_id(c)).num_fmt.as_str(), &cx.wb).text;
        for row in start..=r.end.row {
            let key: Vec<String> = cols.iter().map(|c| shown(CellRef::new(row, *c)).to_lowercase()).collect();
            if seen.insert(key) {
                keep.push(row);
            } else {
                removed += 1;
            }
        }
        let pictures: Vec<_> = keep
            .iter()
            .enumerate()
            .flat_map(|(i, row)| {
                (r.start.col..=r.end.col)
                    .filter_map(move |col| sh.cell_pictures.get(&CellRef::new(*row, col)).map(|p| (CellRef::new(start + i as u32, col), p.clone())))
            })
            .collect();
        // Rows a filter keeps are compacted to `start + i`, so the notes and hyperlinks that
        // describe them move with them; the ones on removed rows are dropped with the row.
        let notes: Vec<_> = keep
            .iter()
            .enumerate()
            .flat_map(|(i, row)| {
                (r.start.col..=r.end.col)
                    .filter_map(move |col| sh.comments.get(&CellRef::new(*row, col)).map(|cm| (CellRef::new(start + i as u32, col), cm.clone())))
            })
            .collect();
        let links: Vec<_> = keep
            .iter()
            .enumerate()
            .flat_map(|(i, row)| {
                (r.start.col..=r.end.col)
                    .filter_map(move |col| sh.hyperlinks.get(&CellRef::new(*row, col)).map(|h| (CellRef::new(start + i as u32, col), h.clone())))
            })
            .collect();
        let rows: Vec<Vec<Option<Cell>>> =
            keep.iter().map(|row| (r.start.col..=r.end.col).map(|c| sh.cell(CellRef::new(*row, c)).cloned()).collect()).collect();
        let shm = cx.sheet_mut(sheet)?;
        for row in start..=r.end.row {
            for c in r.start.col..=r.end.col {
                shm.remove_cell(CellRef::new(row, c));
            }
        }
        for row in start..=r.end.row {
            for c in r.start.col..=r.end.col {
                let at = CellRef::new(row, c);
                shm.comments.remove(&at);
                shm.hyperlinks.remove(&at);
            }
        }
        for (i, cells) in rows.into_iter().enumerate() {
            for (j, cell) in cells.into_iter().enumerate() {
                if let Some(cell) = cell {
                    shm.set_cell(CellRef::new(start + i as u32, r.start.col + j as u32), cell);
                }
            }
        }
        shm.cell_pictures.extend(pictures);
        for (at, cm) in notes {
            shm.comments.insert(at, cm);
        }
        for (at, h) in links {
            shm.hyperlinks.insert(at, h);
        }
        for c in RangeRef::new(CellRef::new(start, r.start.col), r.end).iter() {
            cx.changed.push((sheet, c));
        }
        if removed > 0 {
            shrink_after_removal(cx, sheet, r, removed as u32)?;
        }
        Ok(())
    })?;
    let remaining = (r.end.row + 1 - start) as usize - removed;
    Ok(json!({"removed": removed, "remaining": remaining}))
}

/// After Remove Duplicates compacted the rows of `r`, `removed` rows at its bottom are empty. A
/// table whose data ends with `r` loses those rows, as in Excel: its range ends at the last kept
/// row and its total row moves up under it. A sheet AutoFilter over exactly those rows shrinks
/// the same way. Nothing outside the range moves.
fn shrink_after_removal(cx: &mut crate::cmd::Ctx, sheet: usize, r: RangeRef, removed: u32) -> Result<()> {
    let sh = cx.sheet_mut(sheet)?;
    let table = sh.tables.iter().position(|t| {
        t.range.start.col == r.start.col
            && t.range.end.col == r.end.col
            && t.range.start.row <= r.start.row
            && t.data_range().is_some_and(|d| d.start.row <= r.start.row.saturating_add(1) && d.end.row == r.end.row)
    });
    let new_end = r.end.row - removed;
    let mut moved_total = None;
    if let Some(i) = table
        && let Some(t) = sh.tables.get(i).cloned()
    {
        if t.totals_row {
            let from = t.range.end.row;
            let to = new_end + 1;
            for col in t.range.start.col..=t.range.end.col {
                let (a, b) = (CellRef::new(from, col), CellRef::new(to, col));
                if let Some(cell) = sh.cell(a).cloned() {
                    sh.remove_cell(a);
                    sh.set_cell(b, cell);
                }
                if let Some(cm) = sh.comments.remove(&a) {
                    sh.comments.insert(b, cm);
                }
                if let Some(h) = sh.hyperlinks.remove(&a) {
                    sh.hyperlinks.insert(b, h);
                }
            }
            moved_total = Some((from, to, t.range.start.col, t.range.end.col));
        }
        if let Some(t) = sh.tables.get_mut(i) {
            t.range.end.row -= removed;
        }
    }
    if let Some(af) = sh.autofilter.as_mut()
        && af.range.start.col == r.start.col
        && af.range.end.col == r.end.col
        && af.range.start.row <= r.start.row
        && af.range.end.row == r.end.row
    {
        af.range.end.row = new_end;
    }
    if let Some((from, to, c0, c1)) = moved_total {
        for col in c0..=c1 {
            cx.changed.push((sheet, CellRef::new(from, col)));
            cx.changed.push((sheet, CellRef::new(to, col)));
        }
    }
    if table.is_some() {
        // Structured references (`Table1[Sales]`) now cover fewer rows.
        cx.recalc_all = true;
    }
    Ok(())
}

pub(super) fn text_to_columns(s: &mut Session, p: &Json) -> Result<Json> {
    let r = target_range(s, p)?;
    let sheet = target_sheet(s, p)?;
    let mut delims: Vec<char> = p
        .get("delimiters")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|d| d.as_str().and_then(|x| x.chars().next())).collect())
        .unwrap_or_else(|| vec!['\t']);
    if let Some(o) = str_param(p, "other").and_then(|o| o.chars().next()) {
        delims.push(o);
    }
    let fixed: Option<Vec<usize>> =
        p.get("fixedWidths").and_then(Json::as_array).map(|a| a.iter().filter_map(|x| x.as_u64().map(|v| v as usize)).collect());
    let consecutive = bool_param(p, "treatConsecutive").unwrap_or(false);
    let quote = str_param(p, "textQualifier").and_then(|q| q.chars().next()).unwrap_or('"');
    let dest = cell_param_on(s, p, "destination", sheet)?.unwrap_or(r.start);
    let formats: Vec<ColumnFormat> = match p.get("columns").and_then(Json::as_array) {
        Some(a) => a
            .iter()
            .take(gridcraft_core::MAX_COLS as usize)
            .map(|f| {
                let f = f.as_str().unwrap_or("general");
                ColumnFormat::parse(f)
                    .ok_or_else(|| bad("data.textToColumns", format!("unknown column format `{f}` (general|text|ymd|mdy|dmy|ydm|myd|dym|skip)")))
            })
            .collect::<Result<_>>()?,
        None => vec![],
    };
    edit(s, |cx| {
        let Some(sh) = cx.wb.sheet(sheet) else { return Ok(()) };
        let sys = cx.wb.date_system;
        let mut out: Vec<(CellRef, String, ColumnFormat)> = Vec::new();
        for (i, row) in (r.start.row..=r.end.row).enumerate() {
            let text = sh.value(CellRef::new(row, r.start.col)).display();
            let parts: Vec<String> = match &fixed {
                Some(w) => {
                    let chars: Vec<char> = text.chars().collect();
                    let mut cuts = vec![0];
                    let mut breaks: Vec<usize> = w.iter().copied().filter(|x| *x > 0 && *x < chars.len()).collect();
                    breaks.sort_unstable();
                    breaks.dedup();
                    cuts.extend(breaks);
                    cuts.push(chars.len());
                    cuts.windows(2)
                        .map(|p| chars.get(p[0]..p[1]).map(|s| s.iter().collect::<String>().trim().to_string()).unwrap_or_default())
                        .collect()
                }
                None => split_delimited(&text, &delims, consecutive, quote),
            };
            // Skipped columns aren't imported: the next column takes their place.
            let mut j = 0u32;
            for (k, part) in parts.into_iter().enumerate() {
                let format = formats.get(k).copied().unwrap_or(ColumnFormat::General);
                if format == ColumnFormat::Skip {
                    continue;
                }
                out.push((CellRef::new(dest.row + i as u32, dest.col + j), part, format));
                j += 1;
            }
        }
        let mut text_style = None;
        let mut date_style = None;
        for (c, t, format) in out {
            let typed = match format {
                ColumnFormat::Text if !t.is_empty() => {
                    let st = *text_style.get_or_insert_with(|| {
                        cx.wb.styles.derive(gridcraft_model::StyleId::DEFAULT, |x| x.num_fmt = gridcraft_model::NumFmt::new("@"))
                    });
                    Some(Cell { value: gridcraft_core::Value::text(t.as_str()), formula: None, style: st, py: None })
                }
                ColumnFormat::Date(order) => parse_date(&t, order, sys).map(|serial| {
                    let st = *date_style.get_or_insert_with(|| {
                        cx.wb.styles.derive(gridcraft_model::StyleId::DEFAULT, |x| x.num_fmt = gridcraft_model::NumFmt::new("yyyy/m/d"))
                    });
                    Cell { value: gridcraft_core::Value::Number(serial), formula: None, style: st, py: None }
                }),
                _ => None,
            };
            if let Some(cell) = typed {
                cx.sheet_mut(sheet)?.set_cell(c, cell);
                cx.changed.push((sheet, c));
                continue;
            }
            let cell = match crate::cmd::edit::input_to_cell(&t, sheet, c, &mut cx.wb) {
                Ok(cell) => cell,
                // `=PY(…)` doesn't make a Python cell (that takes python.set): the piece is kept
                // as text rather than lost.
                Err(_) if gridcraft_model::python::is_py_formula(&t) => {
                    let style = cx.wb.sheet(sheet).map(|sh| sh.style_id(c)).unwrap_or_default();
                    Some(Cell { value: gridcraft_core::Value::text(t.as_str()), formula: None, style, py: None })
                }
                Err(_) => None,
            };
            let shm = cx.sheet_mut(sheet)?;
            match cell {
                Some(x) => shm.set_cell(c, x),
                None => {
                    shm.remove_cell(c);
                }
            }
            cx.changed.push((sheet, c));
        }
        Ok(())
    })?;
    ok()
}

/// Text to Columns step 3: a column's data format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColumnFormat {
    /// General: numbers and dates as typed input, the rest text.
    General,
    /// Text: kept as it is (`0101` keeps its zero).
    Text,
    /// Date in this order of year, month and day (`b"ymd"`).
    Date([u8; 3]),
    /// Do not import column (skip).
    Skip,
}

impl ColumnFormat {
    fn parse(s: &str) -> Option<ColumnFormat> {
        let s = s.to_ascii_lowercase();
        Some(match s.as_str() {
            "general" | "" => ColumnFormat::General,
            "text" => ColumnFormat::Text,
            "skip" => ColumnFormat::Skip,
            "ymd" | "mdy" | "dmy" | "ydm" | "myd" | "dym" => {
                let b = s.as_bytes();
                ColumnFormat::Date([*b.first()?, *b.get(1)?, *b.get(2)?])
            }
            _ => return None,
        })
    }
}

/// A date written in `order` (`2025/4/1`, `01-04-2025`, `20250401` for YMD): its serial, or
/// `None` when it isn't one (the text is then read as General, as in Excel).
fn parse_date(text: &str, order: [u8; 3], sys: gridcraft_core::DateSystem) -> Option<f64> {
    let t = text.trim();
    let parts: Vec<&str> = t.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).collect();
    let nums: Vec<i64> = if parts.len() == 3 {
        parts.iter().map(|x| x.parse::<i64>().ok()).collect::<Option<_>>()?
    } else if parts.len() == 1 && t.len() == 8 && t.bytes().all(|b| b.is_ascii_digit()) {
        let at = |a: usize, b: usize| t.get(a..b).and_then(|x| x.parse::<i64>().ok());
        match order {
            [b'y', ..] => vec![at(0, 4)?, at(4, 6)?, at(6, 8)?],
            [_, _, b'y'] => vec![at(0, 2)?, at(2, 4)?, at(4, 8)?],
            _ => vec![at(0, 2)?, at(2, 6)?, at(6, 8)?],
        }
    } else {
        return None;
    };
    let (mut y, mut m, mut d) = (0, 0, 0);
    for (k, n) in order.iter().zip(nums) {
        match k {
            b'y' => y = n,
            b'm' => m = n,
            _ => d = n,
        }
    }
    if y < 100 {
        y += if y < 30 { 2000 } else { 1900 };
    }
    gridcraft_core::date::serial_from_ymd(sys, y, m, d)
}

pub fn split_delimited(text: &str, delims: &[char], consecutive: bool, quote: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut last_delim = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == quote {
            if in_q && chars.peek() == Some(&quote) {
                cur.push(quote);
                chars.next();
            } else {
                in_q = !in_q;
            }
            continue;
        }
        if !in_q && delims.contains(&c) {
            if consecutive && last_delim {
                continue;
            }
            out.push(std::mem::take(&mut cur));
            last_delim = true;
            continue;
        }
        last_delim = false;
        cur.push(c);
    }
    out.push(cur);
    out
}
