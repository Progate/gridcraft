//! Table styles: how a cell inside a table looks (our own palettes, keyed by the familiar
//! `TableStyleLight1…21 / Medium1…28 / Dark1…11` names so files round-trip).

use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::{BorderLine, BorderStyle, Cell, Color, Fill, Formula, PatternType, Style, Table, Workbook};

/// (family, number) from a style name, e.g. `TableStyleMedium2` → ("Medium", 2).
pub fn parse_style_name(name: &str) -> (&'static str, u32) {
    let rest = name.strip_prefix("TableStyle").unwrap_or(name);
    for fam in ["Light", "Medium", "Dark"] {
        if let Some(n) = rest.strip_prefix(fam) {
            let k = n.parse::<u32>().unwrap_or(2).max(1); // the gallery starts at 1; 0 would underflow `n - 1`
            return (fam, k);
        }
    }
    ("Medium", 2)
}

/// Accent theme slot (4..=9) for a style number; 1 and every 7th style are neutral (dk1).
fn accent_for(n: u32) -> Option<u8> {
    let k = (n.saturating_sub(1)) % 7;
    if k == 0 { None } else { Some(3 + k as u8) }
}

/// Header/band/body colours for a table style.
pub struct TableLook {
    pub header_fill: Option<Color>,
    pub header_font: Color,
    pub header_bold: bool,
    pub band_fill: Option<Color>,
    pub body_fill: Option<Color>,
    pub line: Option<Color>,
    pub total_top: Option<Color>,
}

pub fn look(style: &str) -> TableLook {
    let (fam, n) = parse_style_name(style);
    let acc = accent_for(n);
    let base = |tint: i16| match acc {
        Some(a) => Color::Theme(a, tint),
        None => Color::Theme(1, if tint == 0 { 0 } else { tint.clamp(-1000, 1000) }),
    };
    let neutral_light = |tint: i16| match acc {
        Some(a) => Color::Theme(a, tint),
        None => Color::Theme(0, -((1000 - tint).clamp(0, 1000) / 6)),
    };
    match fam {
        "Light" => {
            let group = (n - 1) / 7; // 0: lines only, 1: banded header line, 2: grid
            TableLook {
                header_fill: None,
                header_font: if group == 0 { base(-250) } else { Color::Auto },
                header_bold: true,
                band_fill: Some(neutral_light(800)),
                body_fill: None,
                line: Some(base(0)),
                total_top: Some(base(0)),
            }
        }
        "Dark" => TableLook {
            header_fill: Some(Color::Theme(1, 0)),
            header_font: Color::Theme(0, 0),
            header_bold: true,
            band_fill: Some(base(-250)),
            body_fill: Some(base(0)),
            line: None,
            total_top: Some(Color::Theme(0, 0)),
        },
        _ => TableLook {
            header_fill: Some(base(0)),
            header_font: Color::Theme(0, 0),
            header_bold: true,
            band_fill: Some(neutral_light(800)),
            body_fill: None,
            line: if (n - 1) / 7 == 0 { None } else { Some(base(400)) },
            total_top: Some(base(0)),
        },
    }
}

/// The style a table gives a cell (merged over the cell's own style: explicit fills/fonts win).
pub fn table_cell_style(wb: &Workbook, sheet: usize, t: &Table, c: CellRef) -> Style {
    let own = wb.sheet(sheet).map(|sh| wb.styles.get(sh.style_id(c)).clone()).unwrap_or_default();
    let mut s = own.clone();
    let lk = look(&t.style);
    let is_header = t.header_row && c.row == t.range.start.row;
    let is_total = t.totals_row && c.row == t.range.end.row;
    let data_row = c.row.saturating_sub(t.range.start.row + t.header_row as u32);
    let data_col = c.col - t.range.start.col;
    let explicit_fill = own.fill.pattern != PatternType::None;
    if is_header {
        if let Some(f) = lk.header_fill
            && !explicit_fill
        {
            s.fill = Fill::solid(f);
        }
        if own.font.color == Color::Auto {
            s.font.color = lk.header_font;
        }
        s.font.bold = s.font.bold || lk.header_bold;
        if let Some(l) = lk.line {
            s.borders.bottom = BorderLine { style: BorderStyle::Thin, color: l };
        }
    } else if is_total {
        s.font.bold = true;
        if let Some(l) = lk.total_top {
            s.borders.top = BorderLine { style: BorderStyle::Double, color: l };
        }
    } else {
        let band = (t.banded_rows && data_row.is_multiple_of(2)) || (t.banded_cols && data_col.is_multiple_of(2));
        if !explicit_fill {
            if band {
                if let Some(f) = lk.band_fill {
                    s.fill = Fill::solid(f);
                }
            } else if let Some(f) = lk.body_fill {
                s.fill = Fill::solid(f);
            }
        }
        if let Some(l) = lk.line
            && t.style.contains("Light")
            && c.row == t.range.end.row
        {
            s.borders.bottom = BorderLine { style: BorderStyle::Thin, color: l };
        }
    }
    if (t.first_col && c.col == t.range.start.col) || (t.last_col && c.col == t.range.end.col) {
        s.font.bold = true;
    }
    if t.style.starts_with("TableStyleDark") && !is_header && own.font.color == Color::Auto {
        s.font.color = Color::Theme(0, 0);
    }
    s
}

/// Gallery list of style names.
pub fn gallery() -> Vec<String> {
    let mut v = Vec::new();
    for n in 1..=21 {
        v.push(format!("TableStyleLight{n}"));
    }
    for n in 1..=28 {
        v.push(format!("TableStyleMedium{n}"));
    }
    for n in 1..=11 {
        v.push(format!("TableStyleDark{n}"));
    }
    v
}

/// Editing a table's header row renames the column, as in Excel: an empty or repeated header
/// gets a unique name written back into the cell, and structured references follow the rename.
pub(crate) fn sync_headers(cx: &mut crate::cmd::Ctx) {
    if cx.wb.sheets.iter().all(|s| s.tables.is_empty()) {
        return;
    }
    // (sheet, table name, table range, old column name, new column name)
    let mut renames: Vec<(usize, String, RangeRef, String, String)> = Vec::new();
    let heads: Vec<(usize, RangeRef)> = (0..cx.wb.sheets.len())
        .flat_map(|si| cx.wb.sheet(si).into_iter().flat_map(move |sh| sh.tables.iter().filter(|t| t.header_row).map(move |t| (si, t.range))))
        .map(|(si, r)| (si, RangeRef::new(r.start, CellRef::new(r.start.row, r.end.col))))
        .collect();
    // A structural edit can move anything into a header row: then every header is checked.
    let changed: std::collections::BTreeSet<(usize, CellRef)> = if cx.structural {
        heads.iter().flat_map(|(s, r)| r.iter().map(move |c| (*s, c))).collect()
    } else {
        cx.changed.iter().filter(|(si, c)| heads.iter().any(|(s, r)| s == si && r.contains(*c))).copied().collect()
    };
    // Per table: lower-case column name → column, and the next number to try for a repeated name
    // (both kept up to date, so a wide header row stays linear).
    type Names = (std::collections::HashMap<String, usize>, std::collections::HashMap<String, usize>);
    let mut names: std::collections::HashMap<(usize, usize), Names> = std::collections::HashMap::new();
    for (si, c) in changed {
        let Some(sh) = cx.wb.sheet(si) else { continue };
        let Some(ti) = sh.tables.iter().position(|t| t.header_row && c.row == t.range.start.row && t.range.contains(c)) else { continue };
        let Some(t) = sh.tables.get(ti) else { continue };
        let col = (c.col - t.range.start.col) as usize;
        let Some(old) = t.columns.get(col).map(|x| x.name.clone()) else { continue };
        // A text header names its column as it is: a number format only changes how it looks (an
        // accounting format pads text with spaces), and the file writes the name into the cell.
        let value = sh.value(c);
        let text = match value.as_text() {
            Some(t) => t.to_string(),
            None => crate::display::cell_text(&cx.wb, sh, c),
        };
        // Headers hold constant text: a formula (or a spilled value) there becomes its text.
        let is_text = value.is_text() && sh.cell(c).is_some_and(|x| x.formula.is_none());
        if text == old && is_text {
            continue;
        }
        let (taken, next) = names
            .entry((si, ti))
            .or_insert_with(|| (t.columns.iter().enumerate().map(|(i, x)| (x.name.to_lowercase(), i)).collect(), Default::default()));
        if taken.get(&old.to_lowercase()) == Some(&col) {
            taken.remove(&old.to_lowercase());
        }
        let base = if text.trim().is_empty() { format!("Column{}", col + 1) } else { text.clone() };
        let k = next.entry(base.to_lowercase()).or_insert(2);
        let mut name = base.clone();
        while taken.contains_key(&name.to_lowercase()) {
            name = format!("{base}{k}");
            *k += 1;
        }
        taken.insert(name.to_lowercase(), col);
        let (table, range) = (t.name.clone(), t.range);
        let Some(sh) = cx.wb.sheet_mut(si) else { continue };
        if let Some(x) = sh.tables.get_mut(ti).and_then(|t| t.columns.get_mut(col)) {
            x.name = name.clone();
        }
        // Headers are text: a number typed there, or a generated name, is written back as text.
        if name != text || !is_text {
            let style = sh.style_id(c);
            sh.cells.set(c, Cell { value: gridcraft_core::Value::text(name.as_str()), style, ..Default::default() });
        }
        if !old.eq_ignore_ascii_case(&name) {
            renames.push((si, table, range, old, name));
        }
    }
    if renames.is_empty() {
        return;
    }
    // Structured references follow the new names.
    for i in 0..cx.wb.sheets.len() {
        let formulas: Vec<(CellRef, std::sync::Arc<Formula>)> =
            cx.wb.sheets.get(i).map(|s| s.cells.iter().filter_map(|(c, x)| x.formula.clone().map(|f| (c, f))).collect()).unwrap_or_default();
        for (c, f) in formulas {
            let Some(e) = f.expr() else { continue };
            let mut hit = false;
            let ne = e.map(&mut |x| match x {
                gridcraft_formula::Expr::Struct(mut st) => {
                    for (si, table, range, old, name) in &renames {
                        let this = if st.table.is_empty() { i == *si && range.contains(c) } else { st.table.eq_ignore_ascii_case(table) };
                        if this {
                            for end in [&mut st.col_start, &mut st.col_end].into_iter().flatten() {
                                if end.eq_ignore_ascii_case(old) {
                                    *end = name.clone();
                                    hit = true;
                                }
                            }
                        }
                    }
                    gridcraft_formula::Expr::Struct(st)
                }
                o => o,
            });
            if hit && let Some(cell) = cx.wb.sheet_mut(i).and_then(|s| s.cells.get_mut(c)) {
                cell.formula = Some(std::sync::Arc::new(Formula::from_expr(ne)));
            }
        }
    }
    cx.structural = true;
}

/// The first name `base`, `base2`, `base3`… no column of `t` has (names compare without case).
fn unique_column_name(t: &Table, base: &str) -> String {
    let taken = |n: &str| t.columns.iter().any(|c| c.name.eq_ignore_ascii_case(n));
    if !taken(base) {
        return base.to_string();
    }
    (2u32..).map(|k| format!("{base}{k}")).find(|n| !taken(n)).unwrap_or_else(|| base.to_string())
}

/// Table AutoExpansion, as Excel does it when a value is typed or pasted next to a table: content
/// in the row directly below it (within its columns) adds that row, and content in the column
/// directly right of it (within its header and data rows) adds that column. A table with a total
/// row does not grow down, and nothing grows into another table. The new column is named by its
/// header cell, or `Column<n>` (n its position) when that cell is blank, which is written into it.
/// New rows get the table's calculated column formulas. `area` is what the command wrote.
pub(crate) fn auto_expand(cx: &mut crate::cmd::Ctx, sheet: usize, area: RangeRef) {
    let Some(sh) = cx.wb.sheet(sheet) else { return };
    if sh.tables.is_empty() {
        return;
    }
    let filled = |sh: &gridcraft_model::Sheet, c: CellRef| sh.cell(c).is_some_and(Cell::has_content);
    let clear_of_others =
        |sh: &gridcraft_model::Sheet, ti: usize, r: &RangeRef| sh.tables.iter().enumerate().all(|(i, o)| i == ti || !o.range.intersects(r));
    let mut grown = false;
    for ti in 0..sh.tables.len() {
        // Down: one row at a time, so a paste of several rows below the table adds them all.
        loop {
            let Some(sh) = cx.wb.sheet(sheet) else { return };
            let Some(t) = sh.tables.get(ti) else { break };
            let Some(row) = t.range.end.row.checked_add(1).filter(|r| *r < gridcraft_core::MAX_ROWS) else { break };
            let (c0, c1) = (t.range.start.col.max(area.start.col), t.range.end.col.min(area.end.col));
            if t.totals_row || row < area.start.row || row > area.end.row || c0 > c1 || !(c0..=c1).any(|c| filled(sh, CellRef::new(row, c))) {
                break;
            }
            let grown_range = RangeRef::new(t.range.start, CellRef::new(row, t.range.end.col));
            if !clear_of_others(sh, ti, &grown_range) {
                break;
            }
            let Some(sh) = cx.wb.sheet_mut(sheet) else { return };
            if let Some(t) = sh.tables.get_mut(ti) {
                t.range = grown_range;
            }
            for c in fill_calculated(sh, ti, row, row) {
                cx.changed.push((sheet, c));
            }
            grown = true;
        }
        // Right: beside the header and data rows (not the total row).
        loop {
            let Some(sh) = cx.wb.sheet(sheet) else { return };
            let Some(t) = sh.tables.get(ti) else { break };
            let Some(col) = t.range.end.col.checked_add(1).filter(|c| *c < gridcraft_core::MAX_COLS) else { break };
            let last = t.range.end.row.saturating_sub(t.totals_row as u32);
            let (r0, r1) = (t.range.start.row.max(area.start.row), last.min(area.end.row));
            if col < area.start.col || col > area.end.col || r0 > r1 || !(r0..=r1).any(|r| filled(sh, CellRef::new(r, col))) {
                break;
            }
            let grown_range = RangeRef::new(t.range.start, CellRef::new(t.range.end.row, col));
            if !clear_of_others(sh, ti, &grown_range) {
                break;
            }
            let header = CellRef::new(t.range.start.row, col);
            let typed = if t.header_row { sh.value(header).display() } else { String::new() };
            let base = if typed.trim().is_empty() { format!("Column{}", t.columns.len() + 1) } else { typed.clone() };
            let name = unique_column_name(t, &base);
            let header_row = t.header_row;
            let Some(sh) = cx.wb.sheet_mut(sheet) else { return };
            if let Some(t) = sh.tables.get_mut(ti) {
                t.range = grown_range;
                t.columns.push(gridcraft_model::TableColumn {
                    name: name.clone(),
                    totals: Default::default(),
                    totals_label: None,
                    formula: None,
                    unique_name: None,
                    xml: None,
                });
            }
            // The header holds the column's name as text (a generated one, or a repeat made unique).
            if header_row && (name != typed || !sh.value(header).is_text()) {
                let style = sh.style_id(header);
                sh.set_cell(header, Cell { value: gridcraft_core::Value::text(name.as_str()), style, ..Default::default() });
                cx.changed.push((sheet, header));
            }
            grown = true;
        }
    }
    if grown {
        cx.structural = true;
    }
}

/// Writes the calculated column formulas of table `ti` into its blank cells of rows `r0..=r1`
/// (clipped to the data rows), each formula moved from the first data row as a fill would move it.
/// Returns the cells written.
pub(crate) fn fill_calculated(sh: &mut gridcraft_model::Sheet, ti: usize, r0: u32, r1: u32) -> Vec<CellRef> {
    let Some(t) = sh.tables.get(ti) else { return Vec::new() };
    let Some(data) = t.data_range() else { return Vec::new() };
    let (r0, r1) = (r0.max(data.start.row), r1.min(data.end.row));
    if r0 > r1 {
        return Vec::new();
    }
    let calc: Vec<(u32, gridcraft_formula::Expr)> = t
        .columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| Some((data.start.col + i as u32, gridcraft_formula::parse(c.formula.as_deref()?).ok()?)))
        .collect();
    let mut out = Vec::new();
    for (col, e) in calc {
        for row in r0..=r1 {
            let c = CellRef::new(row, col);
            if sh.cell(c).is_some_and(Cell::has_content) {
                continue;
            }
            let moved = gridcraft_formula::adjust::shift_relative(e.clone(), i64::from(row) - i64::from(data.start.row), 0);
            let mut cell = Cell::formula(Formula::from_expr(moved));
            cell.style = sh.style_id(c);
            sh.set_cell(c, cell);
            out.push(c);
        }
    }
    out
}

/// The formula of `f` at `at` as it would read in row `first` (a fill's move), for comparing the
/// formulas of a column.
fn formula_at_row(f: &Formula, at: CellRef, first: u32) -> Option<String> {
    if f.array.is_some() {
        return None;
    }
    let e = f.expr()?;
    Some(gridcraft_formula::print(&gridcraft_formula::adjust::shift_relative(e, i64::from(first) - i64::from(at.row), 0)))
}

/// Calculated columns, as Excel makes them: a formula entered in a data cell of a table column
/// whose other data cells are blank, or all hold one formula, fills the column's data cells (each
/// moved as a fill moves it) and becomes the column's formula, which rows added later get. A value
/// or a different formula entered beside other contents is an exception and leaves the column's
/// formula as it is.
pub(crate) fn calculated_entry(cx: &mut crate::cmd::Ctx, sheet: usize, at: CellRef) {
    let Some(sh) = cx.wb.sheet(sheet) else { return };
    let Some(f) = sh.cell(at).and_then(|c| c.formula.clone()) else { return };
    let Some((ti, data, col)) = sh.tables.iter().enumerate().find_map(|(i, t)| {
        let d = t.data_range().filter(|d| d.contains(at))?;
        Some((i, d, (at.col - d.start.col) as usize))
    }) else {
        return;
    };
    let first = data.start.row;
    let Some(mine) = formula_at_row(&f, at, first) else { return };
    let column = RangeRef::new(CellRef::new(first, at.col), CellRef::new(data.end.row, at.col));
    let mut common: Option<String> = None;
    for (c, cell) in sh.cells.iter_range(column) {
        if c == at || !cell.has_content() {
            continue;
        }
        let Some(other) = cell.formula.as_ref().and_then(|g| formula_at_row(g, c, first)) else { return };
        if common.get_or_insert_with(|| other.clone()) != &other {
            return;
        }
    }
    let Ok(e) = gridcraft_formula::parse(&mine) else { return };
    let Some(sh) = cx.wb.sheet_mut(sheet) else { return };
    for row in data.start.row..=data.end.row {
        let c = CellRef::new(row, at.col);
        if c == at {
            continue;
        }
        let moved = gridcraft_formula::adjust::shift_relative(e.clone(), i64::from(row) - i64::from(first), 0);
        let mut cell = Cell::formula(Formula::from_expr(moved));
        cell.style = sh.style_id(c);
        sh.set_cell(c, cell);
        cx.changed.push((sheet, c));
    }
    if let Some(tc) = sh.tables.get_mut(ti).and_then(|t| t.columns.get_mut(col)) {
        tc.formula = Some(mine);
    }
}

/// Rows inserted inside a table (whole rows, or cells shifted down across its columns) get its
/// calculated column formulas, as in Excel.
pub(crate) fn fill_inserted_rows(cx: &mut crate::cmd::Ctx) {
    use gridcraft_formula::adjust::{Axis, Edit};
    if cx.edits.is_empty() {
        return;
    }
    // (sheet, first row, last row, the columns when only cells were inserted)
    type Inserted = (String, u32, u32, Option<(u32, u32)>);
    let inserted: Vec<Inserted> = cx
        .edits
        .iter()
        .filter_map(|(sheet, e)| match *e {
            Edit::Insert { axis: Axis::Rows, at, count } if count > 0 => Some((sheet.clone(), at, at.saturating_add(count - 1), None)),
            Edit::InsertCells { axis: Axis::Rows, range } => {
                Some((sheet.clone(), range.start.row, range.end.row, Some((range.start.col, range.end.col))))
            }
            _ => None,
        })
        .collect();
    for (name, r0, r1, cols) in inserted {
        let Some(si) = cx.wb.sheet_index(&name) else { continue };
        let Some(sh) = cx.wb.sheet_mut(si) else { continue };
        for ti in 0..sh.tables.len() {
            let Some(t) = sh.tables.get(ti) else { continue };
            // Cells shifted down within part of a table's columns don't make table rows.
            if cols.is_some_and(|(c0, c1)| c0 > t.range.start.col || c1 < t.range.end.col) || t.columns.iter().all(|c| c.formula.is_none()) {
                continue;
            }
            for c in fill_calculated(sh, ti, r0, r1) {
                cx.changed.push((si, c));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gridcraft_core::Value;
    use serde_json::json;

    use crate::Session;

    #[test]
    fn editing_a_header_renames_the_column() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [["a", "b", "c"], [1, 2, 3]]})).unwrap();
        s.execute("insert.table", json!({"range": "A1:C2", "header": true})).unwrap();
        s.execute("cell.set", json!({"cell": "E1", "input": "=SUM(Table1[a])"})).unwrap();
        s.execute("cell.set", json!({"cell": "A1", "input": "zz"})).unwrap();
        s.execute("edit.clearContents", json!({"range": "B1"})).unwrap();
        s.execute("cell.set", json!({"cell": "C1", "input": "ZZ"})).unwrap();
        let get = |s: &mut Session, a: &str| s.execute("cell.get", json!({"cell": a})).unwrap();
        assert_eq!(get(&mut s, "E1")["formula"], "=SUM(Table1[zz])");
        assert_eq!(get(&mut s, "E1")["value"], json!(1.0));
        let d = s.doc().unwrap();
        let sh = d.wb.sheet(0).unwrap();
        let names: Vec<&str> = sh.tables[0].columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["zz", "Column2", "ZZ2"]);
        assert_eq!(sh.value(gridcraft_core::CellRef::new(0, 1)), Value::from("Column2"));
        assert_eq!(sh.value(gridcraft_core::CellRef::new(0, 2)), Value::from("ZZ2"));
        s.execute("range.setValues", json!({"range": "A1", "values": [["x", "x", 5]]})).unwrap();
        let sh = s.doc().unwrap().wb.sheet(0).unwrap();
        let names: Vec<&str> = sh.tables[0].columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["x", "x2", "5"]);
        assert_eq!(sh.value(gridcraft_core::CellRef::new(0, 2)), Value::from("5"));
    }

    fn orders() -> Session {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [["商品", "単価", "数量"], ["a", 100, 3], ["b", 50, 4]]})).unwrap();
        s.execute("insert.table", json!({"range": "A1:C3", "header": true})).unwrap();
        s.execute("table.rename", json!({"table": "Table1", "name": "注文"})).unwrap();
        s
    }

    fn table(s: &Session) -> gridcraft_model::Table {
        s.doc().unwrap().wb.sheet(0).unwrap().tables[0].clone()
    }

    fn formula(s: &mut Session, a: &str) -> Json {
        s.execute("cell.get", json!({"cell": a})).unwrap()["formula"].clone()
    }

    use serde_json::Value as Json;

    #[test]
    fn typing_next_to_a_table_extends_it() {
        let mut s = orders();
        s.execute("cell.set", json!({"cell": "A4", "input": "c"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:C4");
        // Beside the header: a column named by what was typed.
        s.execute("cell.set", json!({"cell": "D1", "input": "備考"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:D4");
        // Beside a data row: a column with a generated name, written into its header.
        s.execute("cell.set", json!({"cell": "E3", "input": "x"})).unwrap();
        let t = table(&s);
        assert_eq!(t.range.a1(), "A1:E4");
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["商品", "単価", "数量", "備考", "Column5"]);
        assert_eq!(s.doc().unwrap().wb.sheet(0).unwrap().value(gridcraft_core::CellRef::new(0, 4)), Value::from("Column5"));
        // The entry and the expansion are one undo step.
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:D4");
        assert_eq!(s.doc().unwrap().wb.sheet(0).unwrap().value(gridcraft_core::CellRef::new(0, 4)), Value::Empty);
        // A header typed beside the table that repeats a name is made unique.
        s.execute("cell.set", json!({"cell": "E1", "input": "単価"})).unwrap();
        assert_eq!(table(&s).columns[4].name, "単価2");
        // Not two cells away, nor below the header only.
        s.execute("cell.set", json!({"cell": "A6", "input": "far"})).unwrap();
        s.execute("cell.set", json!({"cell": "G1", "input": "far"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:E4");
    }

    #[test]
    fn values_and_pastes_next_to_a_table_extend_it() {
        let mut s = orders();
        s.execute("range.setValues", json!({"range": "A4", "values": [["c", 1, 2], ["d", 3, 4]]})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:C5");
        s.execute("edit.paste", json!({"at": "A6", "text": "e\t5\t6\n"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:C6");
        s.execute("edit.copy", json!({"range": "A6:C6"})).unwrap();
        s.execute("edit.paste", json!({"at": "A7"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:C7");
        // Formats alone don't.
        s.execute("edit.copy", json!({"range": "A7:C7"})).unwrap();
        s.execute("cell.set", json!({"cell": "A9", "input": "keep"})).unwrap();
        s.execute("edit.pasteSpecial", json!({"at": "A8", "what": "formats"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:C7");
    }

    #[test]
    fn a_total_row_stops_expansion_downwards() {
        let mut s = orders();
        s.execute("table.totalRow", json!({"table": "注文", "on": true})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:C4");
        s.execute("cell.set", json!({"cell": "A5", "input": "c"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:C4");
        // Beside the total row neither; beside a data row the table still grows right.
        s.execute("cell.set", json!({"cell": "D4", "input": "x"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:C4");
        s.execute("cell.set", json!({"cell": "D2", "input": "y"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:D4");
    }

    #[test]
    fn a_formula_in_an_empty_column_makes_a_calculated_column() {
        let mut s = orders();
        s.execute("cell.set", json!({"cell": "D1", "input": "金額"})).unwrap();
        s.execute("cell.set", json!({"cell": "D2", "input": "=[@単価]*[@数量]"})).unwrap();
        assert_eq!(formula(&mut s, "D3"), "=[@単価]*[@数量]");
        assert_eq!(table(&s).columns[3].formula.as_deref(), Some("[@単価]*[@数量]"));
        assert_eq!(s.execute("cell.get", json!({"cell": "D3"})).unwrap()["value"], json!(200.0));
        // Relative references move as a fill moves them; the column formula is the first row's.
        s.execute("cell.set", json!({"cell": "E1", "input": "税"})).unwrap();
        s.execute("cell.set", json!({"cell": "E3", "input": "=D3*0.1"})).unwrap();
        assert_eq!(formula(&mut s, "E2"), "=D2*0.1");
        assert_eq!(table(&s).columns[4].formula.as_deref(), Some("D2*0.1"));
        // Rows added later get the formulas: AutoExpansion, an inserted row, Resize Table.
        s.execute("cell.set", json!({"cell": "A4", "input": "c"})).unwrap();
        assert_eq!(formula(&mut s, "D4"), "=[@単価]*[@数量]");
        assert_eq!(formula(&mut s, "E4"), "=D4*0.1");
        s.execute("home.insertRows", json!({"rows": "3:3"})).unwrap();
        assert_eq!(formula(&mut s, "D3"), "=[@単価]*[@数量]");
        assert_eq!(formula(&mut s, "E3"), "=D3*0.1");
        s.execute("table.resize", json!({"table": "注文", "range": "A1:E7"})).unwrap();
        assert_eq!(formula(&mut s, "D7"), "=[@単価]*[@数量]");
        // A value typed over one cell is an exception: the column stays calculated.
        s.execute("cell.set", json!({"cell": "D2", "input": "1"})).unwrap();
        assert_eq!(formula(&mut s, "D4"), "=[@単価]*[@数量]");
        assert!(table(&s).columns[3].formula.is_some());
        // With an exception there, a new formula is entered in its cell only.
        s.execute("cell.set", json!({"cell": "D3", "input": "=[@単価]"})).unwrap();
        assert_eq!(formula(&mut s, "D4"), "=[@単価]*[@数量]");
        // Without one, a new formula replaces the column's.
        s.execute("cell.set", json!({"cell": "E5", "input": "=D5*0.08"})).unwrap();
        assert_eq!(formula(&mut s, "E2"), "=D2*0.08");
        assert_eq!(table(&s).columns[4].formula.as_deref(), Some("D2*0.08"));
    }

    #[test]
    fn a_formula_beside_data_or_from_a_script_column() {
        let mut s = orders();
        // A column holding values isn't filled.
        s.execute("cell.set", json!({"cell": "C2", "input": "=1+1"})).unwrap();
        assert_eq!(formula(&mut s, "C3"), Json::Null);
        assert!(table(&s).columns[2].formula.is_none());
        // A formula typed beside the table makes a new calculated column.
        s.execute("cell.set", json!({"cell": "D2", "input": "=[@単価]*2"})).unwrap();
        assert_eq!(table(&s).range.a1(), "A1:D3");
        assert_eq!(formula(&mut s, "D3"), "=[@単価]*2");
        // range.setValues writing the same formula down a column makes it calculated too.
        s.execute("cell.set", json!({"cell": "E1", "input": "x"})).unwrap();
        s.execute("range.setValues", json!({"range": "E2", "values": [["=B2+1"], ["=B3+1"]]})).unwrap();
        assert_eq!(table(&s).columns[4].formula.as_deref(), Some("B2+1"));
    }

    #[test]
    fn total_row_label_and_functions() {
        let mut s = orders();
        s.execute("table.totalRow", json!({"table": "注文", "on": true, "label": "集計"})).unwrap();
        assert_eq!(s.execute("cell.get", json!({"cell": "A4"})).unwrap()["value"], json!("集計"));
        assert_eq!(formula(&mut s, "C4"), "=SUBTOTAL(109,注文[数量])");
        s.execute("table.totalFunction", json!({"table": "注文", "column": "単価", "function": "average"})).unwrap();
        assert_eq!(formula(&mut s, "B4"), "=SUBTOTAL(101,注文[単価])");
        // A function chosen for the first column replaces its label; None clears it.
        s.execute("table.totalFunction", json!({"table": "注文", "column": "商品", "function": "count"})).unwrap();
        assert_eq!(formula(&mut s, "A4"), "=SUBTOTAL(103,注文[商品])");
        s.execute("table.totalFunction", json!({"table": "注文", "column": "商品", "function": "none"})).unwrap();
        assert_eq!(s.execute("cell.get", json!({"cell": "A4"})).unwrap()["value"], Json::Null);
        // A text last column is counted.
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [["n", "name"], [1, "a"]]})).unwrap();
        s.execute("insert.table", json!({"range": "A1:B2", "header": true})).unwrap();
        s.execute("table.totalRow", json!({"on": true, "table": "Table1"})).unwrap();
        assert_eq!(formula(&mut s, "B3"), "=SUBTOTAL(103,Table1[name])");
        assert_eq!(s.execute("cell.get", json!({"cell": "A3"})).unwrap()["value"], json!("Total"));
    }

    #[test]
    fn go_to_a_table_selects_its_data() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [["a", "b"], [1, 2], [3, 4]]})).unwrap();
        s.execute("insert.table", json!({"range": "A1:B3", "header": true})).unwrap();
        assert_eq!(s.execute("edit.goTo", json!({"reference": "Table1"})).unwrap()["selection"], "A2:B3");
    }
}

#[cfg(test)]
mod style_tests {
    use super::*;

    #[test]
    fn zero_numbered_styles_do_not_underflow() {
        assert_eq!(parse_style_name("TableStyleLight0"), ("Light", 1));
        assert_eq!(parse_style_name("TableStyleMedium0"), ("Medium", 1));
        for name in ["TableStyleLight0", "TableStyleMedium0", "TableStyleDark0"] {
            let _ = look(name);
        }
    }
}
