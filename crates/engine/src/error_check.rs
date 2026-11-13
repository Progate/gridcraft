//! Background error checking, as Excel's File › Options › Formulas › Error checking rules: which
//! rule a cell breaks (the green corner and the warning button), with the fixes Excel offers.

use gridcraft_core::{CellError, CellRef, MAX_COLS, MAX_ROWS, RangeRef, Value};
use gridcraft_formula::{Expr, RefKind, SheetSel};
use gridcraft_model::{ErrorRule, ErrorRules, Formula, Sheet, Workbook};

/// A row and column offset from a cell.
type Offset = (i64, i64);

/// The neighbour pairs compared for an inconsistent formula, with the direction a fix copies
/// from (the first cell of the pair): the two cells on either side, then two in a row on one side.
const PAIRS: [(Offset, Offset, &str); 6] = [
    ((-1, 0), (1, 0), "above"),
    ((0, -1), (0, 1), "left"),
    ((-1, 0), (-2, 0), "above"),
    ((0, -1), (0, -2), "left"),
    ((1, 0), (2, 0), "below"),
    ((0, 1), (0, 2), "right"),
];

/// Most cells one formula's "omits adjacent cells" fix grows a reference by.
const MAX_GROW: u32 = 100_000;

/// The first rule of `rules` that cell `c` of sheet `si` breaks, skipping the ones Ignore Error
/// turned off for it. Rules are tried in the order Excel's Options page lists them.
pub fn check_cell(wb: &Workbook, si: usize, c: CellRef, rules: ErrorRules) -> Option<ErrorRule> {
    let sh = wb.sheet(si)?;
    let cell = sh.cell(c)?;
    if !cell.has_content() {
        return None;
    }
    rules.iter().find(|r| !sh.ignores_error(c, *r) && breaks(wb, si, sh, c, *r))
}

/// Whether cell `c` breaks `rule`, ignored errors aside.
pub fn breaks(wb: &Workbook, si: usize, sh: &Sheet, c: CellRef, rule: ErrorRule) -> bool {
    let Some(cell) = sh.cell(c) else { return false };
    let formula = cell.formula.as_deref();
    match rule {
        ErrorRule::EvalError => formula.is_some() && cell.value.is_error(),
        ErrorRule::NumberStoredAsText => crate::cmd::formulas::number_stored_as_text(cell).is_some(),
        ErrorRule::TwoDigitTextYear => formula.is_none() && matches!(&cell.value, Value::Text(t) if two_digit_year(t).is_some()),
        ErrorRule::UnlockedFormula => formula.is_some() && !wb.styles.get(sh.style_id(c)).protection.locked,
        ErrorRule::Formula => formula.is_some_and(|f| f.array.is_none()) && inconsistent_source(sh, c).is_some(),
        ErrorRule::FormulaRange => formula.is_some_and(|f| omitted_cells(wb, si, sh, c, f).is_some()),
        ErrorRule::EmptyCellReference => formula.is_some_and(|f| refers_to_empty(wb, si, f)),
        ErrorRule::CalculatedColumn => calculated_column_formula(sh, c).is_some_and(|col| match formula {
            Some(f) => formula_at_row(f, c, col.1).as_deref() != Some(col.0.as_str()),
            None => true,
        }),
        ErrorRule::ListDataValidation => {
            sh.tables.iter().any(|t| t.data_range().is_some_and(|d| d.contains(c)))
                && crate::cmd::data::check_validation(wb, si, c, &sh.input_text(c)).is_some()
        }
    }
}

/// The cell whose formula an inconsistent formula would copy, and its direction ("above",
/// "left", "below", "right"): two neighbours that hold the same formula (relatively, as R1C1
/// reads it) when the cell's own formula differs. A cell that itself continues a pattern in
/// another direction is consistent, as in Excel: the corner of a total row and a total column
/// (`=SUM(E4:E9)` under row totals `=SUM(B9:D9)`) matches the column totals beside it.
pub fn inconsistent_source(sh: &Sheet, c: CellRef) -> Option<(CellRef, &'static str)> {
    let mine = r1c1(sh, c)?;
    let same = |o: Offset| offset(c, o).and_then(|n| r1c1(sh, n)).as_ref() == Some(&mine);
    if PAIRS.iter().any(|(a, b, _)| same(*a) && same(*b)) {
        return None;
    }
    PAIRS.iter().find_map(|(a, b, dir)| {
        let a = offset(c, *a)?;
        let b = offset(c, *b)?;
        let ka = r1c1(sh, a)?;
        (ka != mine && r1c1(sh, b).as_ref() == Some(&ka)).then_some((a, *dir))
    })
}

fn offset(c: CellRef, (dr, dc): (i64, i64)) -> Option<CellRef> {
    let r = u32::try_from(i64::from(c.row) + dr).ok().filter(|r| *r < MAX_ROWS)?;
    let col = u32::try_from(i64::from(c.col) + dc).ok().filter(|x| *x < MAX_COLS)?;
    Some(CellRef::new(r, col))
}

/// A cell's formula in R1C1 form (the same for formulas a fill copied), `None` for no formula.
fn r1c1(sh: &Sheet, c: CellRef) -> Option<String> {
    let f = sh.cell(c)?.formula.as_deref()?;
    if f.array.is_some() {
        return None;
    }
    f.with_expr(|e| gridcraft_formula::print_r1c1(e, c))
}

/// The table column formula over cell `c` and the table's first data row, when `c` is a data
/// cell of a calculated column.
fn calculated_column_formula(sh: &Sheet, c: CellRef) -> Option<(String, u32)> {
    sh.tables.iter().find_map(|t| {
        let d = t.data_range().filter(|d| d.contains(c))?;
        let col = t.columns.get(usize::try_from(c.col.checked_sub(d.start.col)?).ok()?)?;
        Some((col.formula.clone()?, d.start.row))
    })
}

/// The formula of `f` at `at` as it would read in row `first`, as a fill would move it.
fn formula_at_row(f: &Formula, at: CellRef, first: u32) -> Option<String> {
    if f.array.is_some() {
        return None;
    }
    let e = f.expr()?;
    Some(gridcraft_formula::print(&gridcraft_formula::adjust::shift_relative(e, i64::from(first) - i64::from(at.row), 0)))
}

/// Whether a reference names cells of sheet `si`.
fn on_sheet(wb: &Workbook, si: usize, sel: &SheetSel) -> bool {
    match sel {
        SheetSel::Current => true,
        SheetSel::Named(n) => wb.sheet_index(n) == Some(si),
        SheetSel::Span(..) => false,
    }
}

/// For "formula omits adjacent cells": the formula with its first one-column (or one-row) range
/// grown over the numbers right next to it, when there are any.
pub fn omitted_cells(wb: &Workbook, si: usize, sh: &Sheet, at: CellRef, f: &Formula) -> Option<String> {
    let e = f.expr()?;
    let mut found: Option<(RangeRef, RangeRef)> = None;
    e.walk(&mut |x| {
        if found.is_some() {
            return;
        }
        let Expr::Ref(r) = x else { return };
        if !on_sheet(wb, si, &r.sheet) || !matches!(r.kind, RefKind::Range(..)) {
            return;
        }
        let range = r.range();
        if let Some(grown) = grow_range(sh, range, at) {
            found = Some((range, grown));
        }
    });
    let (from, to) = found?;
    let mut done = false;
    let grown = e.map(&mut |x| match x {
        Expr::Ref(mut r) if !done && r.range() == from => {
            if let RefKind::Range(a, b) = &mut r.kind {
                // Each end keeps its `$` anchors; the ends move to the grown range's corners.
                let (top_left_a, bottom_right_b) = (a.row <= b.row, a.col <= b.col);
                let (r0, r1) = if top_left_a { (&mut a.row, &mut b.row) } else { (&mut b.row, &mut a.row) };
                *r0 = to.start.row;
                *r1 = to.end.row;
                let (c0, c1) = if bottom_right_b { (&mut a.col, &mut b.col) } else { (&mut b.col, &mut a.col) };
                *c0 = to.start.col;
                *c1 = to.end.col;
            }
            done = true;
            Expr::Ref(r)
        }
        other => other,
    });
    Some(format!("={}", gridcraft_formula::print(&grown)))
}

/// A one-column or one-row range grown over the numbers that continue it (above and below, or
/// left and right), stopping at the formula's own cell. `None` when nothing continues it.
fn grow_range(sh: &Sheet, r: RangeRef, at: CellRef) -> Option<RangeRef> {
    let number = |c: CellRef| c != at && matches!(sh.value(c), Value::Number(_));
    let (mut start, mut end) = (r.start, r.end);
    if r.start.col == r.end.col && r.end.row > r.start.row {
        let mut n = 0;
        while start.row > 0 && n < MAX_GROW && number(CellRef::new(start.row - 1, start.col)) {
            start.row -= 1;
            n += 1;
        }
        while end.row + 1 < MAX_ROWS && n < MAX_GROW && number(CellRef::new(end.row + 1, end.col)) {
            end.row += 1;
            n += 1;
        }
    } else if r.start.row == r.end.row && r.end.col > r.start.col {
        let mut n = 0;
        while start.col > 0 && n < MAX_GROW && number(CellRef::new(start.row, start.col - 1)) {
            start.col -= 1;
            n += 1;
        }
        while end.col + 1 < MAX_COLS && n < MAX_GROW && number(CellRef::new(end.row, end.col + 1)) {
            end.col += 1;
            n += 1;
        }
    } else {
        return None;
    }
    let grown = RangeRef::new(start, end);
    (grown != r).then_some(grown)
}

/// Whether a formula refers to a single cell that is empty.
fn refers_to_empty(wb: &Workbook, si: usize, f: &Formula) -> bool {
    f.with_expr(|e| {
        let mut empty = false;
        e.walk(&mut |x| {
            let Expr::Ref(r) = x else { return };
            let RefKind::Cell(a) = &r.kind else { return };
            let sheet = match &r.sheet {
                SheetSel::Current => Some(si),
                SheetSel::Named(n) => wb.sheet_index(n),
                SheetSel::Span(..) => None,
            };
            if let Some(sh) = sheet.and_then(|i| wb.sheet(i))
                && sh.cell(a.cell()).is_none_or(|c| !c.has_content())
            {
                empty = true;
            }
        });
        empty
    })
    .unwrap_or(false)
}

/// A text date with a two-digit year (`1/5/25`, `25-1-5`, `5-Jan-25`): the parts as
/// (year, month, day) with the year still two digits.
pub fn two_digit_year(t: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = t.trim().split(['/', '-', '.']).collect();
    let [a, b, c] = parts.as_slice() else { return None };
    let digits = |s: &str| !s.is_empty() && s.len() <= 4 && s.bytes().all(|b| b.is_ascii_digit());
    let num = |s: &str| s.parse::<u32>().ok();
    let month_name = |s: &str| {
        const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
        let l = s.to_ascii_lowercase();
        let head = l.get(..3)?;
        MONTHS.iter().position(|m| *m == head).map(|i| i as u32 + 1)
    };
    let valid = |m: u32, d: u32| (1..=12).contains(&m) && (1..=31).contains(&d);
    // d-Mon-yy
    if digits(a)
        && digits(c)
        && c.len() == 2
        && a.len() <= 2
        && let Some(m) = month_name(b)
        && let Some(d) = num(a)
        && valid(m, d)
    {
        return Some((num(c)?, m, d));
    }
    if !(digits(a) && digits(b) && digits(c)) || a.len() == 4 || c.len() == 4 {
        return None;
    }
    let (x, y, z) = (num(a)?, num(b)?, num(c)?);
    if c.len() == 2 && a.len() <= 2 && b.len() <= 2 && valid(x, y) {
        return Some((z, x, y));
    }
    if a.len() == 2 && b.len() <= 2 && c.len() <= 2 && valid(y, z) {
        return Some((x, y, z));
    }
    None
}

/// Excel's name and description of a broken rule, in English (the interface translates them).
/// An error value names its own kind.
pub fn describe(rule: ErrorRule, value: &Value) -> (&'static str, &'static str) {
    match rule {
        ErrorRule::EvalError => match value.as_error() {
            Some(CellError::Div0) => ("Divide by Zero Error", "The formula or function used is dividing by zero or empty cells."),
            Some(CellError::Value) => ("Error in Value", "A value used in the formula is of the wrong data type."),
            Some(CellError::Name) => ("Invalid Name Error", "The formula contains unrecognized text."),
            Some(CellError::Ref) => ("Invalid Cell Reference Error", "The formula or function refers to cells that are not valid."),
            Some(CellError::NA) => ("Value Not Available Error", "A value is not available to the formula or function."),
            Some(CellError::Num) => ("Invalid Number Error", "The formula or function contains invalid numeric values."),
            Some(CellError::Null) => ("Null Error", "The formula refers to an intersection of ranges that do not intersect."),
            _ => ("Error in Formula", "The formula in this cell results in an error."),
        },
        ErrorRule::CalculatedColumn => ("Inconsistent Calculated Column Formula", "This cell is inconsistent with the column formula."),
        ErrorRule::TwoDigitTextYear => ("Text Date with 2-Digit Year", "The cell contains a text date with a 2-digit year."),
        ErrorRule::NumberStoredAsText => ("Number Stored as Text", "The number in this cell is formatted as text or preceded by an apostrophe."),
        ErrorRule::Formula => ("Inconsistent Formula", "The formula in this cell differs from the formulas in this area of the spreadsheet."),
        ErrorRule::FormulaRange => {
            ("Formula Omits Adjacent Cells", "The formula in this cell refers to a range that has additional numbers adjacent to it.")
        }
        ErrorRule::UnlockedFormula => {
            ("Unprotected Formula", "The cell containing this formula is not locked to protect it from being changed inadvertently.")
        }
        ErrorRule::EmptyCellReference => ("Formula Refers to Empty Cells", "The formula in this cell refers to empty cells."),
        ErrorRule::ListDataValidation => {
            ("Data Validation Error", "The value in this cell is not valid according to the data validation rules for this cell.")
        }
    }
}

/// The fixes Excel's warning menu offers first for a broken rule, as `(id, label)`: the id is
/// what `formulas.errorFix` takes.
pub fn fixes(sh: &Sheet, c: CellRef, rule: ErrorRule) -> Vec<(&'static str, &'static str)> {
    match rule {
        ErrorRule::EvalError => vec![("evaluate", "Show Calculation Steps…"), ("traceError", "Trace Error")],
        ErrorRule::NumberStoredAsText => vec![("convertToNumber", "Convert to Number")],
        ErrorRule::TwoDigitTextYear => vec![("convert19", "Convert XX to 19XX"), ("convert20", "Convert XX to 20XX")],
        ErrorRule::UnlockedFormula => vec![("lockCell", "Lock Cell")],
        ErrorRule::Formula => match inconsistent_source(sh, c).map(|(_, d)| d) {
            Some("left") => vec![("copyFromSource", "Copy Formula from Left")],
            Some("below") => vec![("copyFromSource", "Copy Formula from Below")],
            Some("right") => vec![("copyFromSource", "Copy Formula from Right")],
            _ => vec![("copyFromSource", "Copy Formula from Above")],
        },
        ErrorRule::FormulaRange => vec![("includeCells", "Update Formula to Include Cells")],
        ErrorRule::EmptyCellReference => vec![("traceEmpty", "Trace Empty Cell")],
        ErrorRule::CalculatedColumn => vec![("restoreColumnFormula", "Restore to Calculated Column Formula")],
        ErrorRule::ListDataValidation => vec![("validation", "Data Validation…")],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Session;

    fn session(cells: &[(&str, &str)]) -> Session {
        let mut s = Session::new();
        s.new_workbook();
        for (c, input) in cells {
            s.execute("cell.set", json!({"cell": c, "input": input})).unwrap();
        }
        s
    }

    /// `(cell, rule)` of every error the rules find.
    fn found(s: &mut Session, rules: &[&str]) -> Vec<(String, String)> {
        let r = s.execute("formulas.errorChecking", json!({"rules": rules})).unwrap();
        r["errors"].as_array().unwrap().iter().map(|e| (e["cell"].as_str().unwrap().into(), e["rule"].as_str().unwrap().into())).collect()
    }

    fn text(s: &Session, a1: &str) -> String {
        let d = s.doc().unwrap();
        d.wb.active().unwrap().cell(CellRef::parse(a1).unwrap()).and_then(|c| c.formula.as_ref()).map(|f| format!("={}", f.text)).unwrap_or_default()
    }

    #[test]
    fn an_inconsistent_formula_is_found_and_copied_from_above() {
        let mut s =
            session(&[("A1", "1"), ("A2", "2"), ("A3", "3"), ("A4", "4"), ("B1", "=A1*2"), ("B2", "=A2*2"), ("B3", "=A3*3"), ("B4", "=A4*2")]);
        assert_eq!(found(&mut s, &["formula"]), [("B3".to_string(), "formula".to_string())]);
        let r = s.execute("formulas.errorChecking", json!({"rules": ["formula"]})).unwrap();
        assert_eq!(r["errors"][0]["title"], "Inconsistent Formula");
        assert_eq!(r["errors"][0]["fixes"][0], json!({"id": "copyFromSource", "label": "Copy Formula from Above"}));
        s.execute("formulas.errorFix", json!({"cell": "B3", "fix": "copyFromSource"})).unwrap();
        assert_eq!(text(&s, "B3"), "=A3*2");
        assert!(found(&mut s, &["formula"]).is_empty());
    }

    /// A budget's grand total sits under row totals and beside column totals: it follows the
    /// column totals, so it isn't flagged although the row totals above it differ.
    #[test]
    fn a_grand_total_in_a_total_row_is_consistent() {
        let mut cells: Vec<(String, String)> = Vec::new();
        for row in 1..=3 {
            for col in ["A", "B"] {
                cells.push((format!("{col}{row}"), "1".into()));
            }
            cells.push((format!("C{row}"), format!("=SUM(A{row}:B{row})")));
        }
        for col in ["A", "B", "C"] {
            cells.push((format!("{col}4"), format!("=SUM({col}1:{col}3)")));
        }
        let cells: Vec<(&str, &str)> = cells.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let mut s = session(&cells);
        assert!(found(&mut s, &["formula"]).is_empty(), "{:?}", found(&mut s, &["formula"]));
    }

    #[test]
    fn a_formula_omitting_adjacent_numbers_is_found_and_extended() {
        let mut s = session(&[("A1", "1"), ("A2", "2"), ("A3", "3"), ("A4", "4"), ("A5", "=SUM(A1:A3)")]);
        assert_eq!(found(&mut s, &["formulaRange"]), [("A5".to_string(), "formulaRange".to_string())]);
        s.execute("formulas.errorFix", json!({"cell": "A5", "fix": "includeCells"})).unwrap();
        assert_eq!(text(&s, "A5"), "=SUM(A1:A4)");
        assert!(found(&mut s, &["formulaRange"]).is_empty());
    }

    #[test]
    fn each_rule_flags_its_own_cells() {
        let mut s = session(&[("A1", "=1/0"), ("A2", "'123"), ("A3", "=D9+1"), ("A4", "'1/5/25"), ("A5", "=A2")]);
        s.execute("home.lockCell", json!({"range": "A5", "on": false})).unwrap();
        let rows = |s: &mut Session, r: &str| found(s, &[r]).into_iter().map(|(c, _)| c).collect::<Vec<_>>();
        assert_eq!(rows(&mut s, "evalError"), ["A1"]);
        assert_eq!(rows(&mut s, "numberStoredAsText"), ["A2"]);
        assert_eq!(rows(&mut s, "emptyCellReference"), ["A3"]);
        assert_eq!(rows(&mut s, "twoDigitTextYear"), ["A4"]);
        assert_eq!(rows(&mut s, "unlockedFormula"), ["A5"]);
        // Formulas referring to empty cells is off unless asked for, as in Excel.
        let all = found(&mut s, &[]);
        assert!(all.is_empty(), "no rules, no errors: {all:?}");
        let r = s.execute("formulas.errorChecking", json!({})).unwrap();
        assert!(!r["errors"].as_array().unwrap().iter().any(|e| e["rule"] == "emptyCellReference"));
        assert_eq!(r["errors"][0]["error"], "#DIV/0!");
    }

    #[test]
    fn ignore_error_hides_the_rule_for_the_cell_and_undo_brings_it_back() {
        let mut s = session(&[("A1", "'123"), ("A2", "'456")]);
        s.execute("formulas.ignoreError", json!({"range": "A1"})).unwrap();
        assert_eq!(found(&mut s, &["numberStoredAsText"]), [("A2".to_string(), "numberStoredAsText".to_string())]);
        let sh = s.doc().unwrap().wb.active().unwrap().clone();
        assert!(sh.ignores_error(CellRef::parse("A1").unwrap(), ErrorRule::NumberStoredAsText));
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(found(&mut s, &["numberStoredAsText"]).len(), 2);
        s.execute("formulas.ignoreError", json!({"range": "A1:A2", "rule": "numberStoredAsText"})).unwrap();
        assert!(found(&mut s, &["numberStoredAsText"]).is_empty());
        let r = s.execute("formulas.resetIgnoredErrors", json!({})).unwrap();
        assert_eq!(r["cleared"], 1);
        assert_eq!(found(&mut s, &["numberStoredAsText"]).len(), 2);
    }

    #[test]
    fn a_two_digit_year_converts_to_the_chosen_century() {
        let mut s = session(&[("A1", "'1/5/25")]);
        s.execute("formulas.errorFix", json!({"cell": "A1", "fix": "convert20"})).unwrap();
        let v = s.doc().unwrap().wb.active().unwrap().value(CellRef::parse("A1").unwrap());
        // 2025-01-05 as a date serial.
        assert_eq!(v, Value::Number(45662.0));
    }

    #[test]
    fn options_turn_rules_and_background_checking_on_and_off() {
        let mut s = session(&[("A1", "=D1")]);
        let r = s.execute("options.errorChecking", json!({"rules": {"emptyCellReference": true}, "background": false})).unwrap();
        assert_eq!((r["background"].clone(), r["rules"]["emptyCellReference"].clone()), (json!(false), json!(true)));
        let r = s.execute("formulas.errorChecking", json!({})).unwrap();
        assert_eq!(r["errors"][0]["rule"], "emptyCellReference");
        assert!(s.execute("options.errorChecking", json!({"rules": {"nope": true}})).is_err());
    }

    #[test]
    fn a_calculated_column_exception_is_found_and_restored() {
        let mut s = session(&[("A1", "Qty"), ("B1", "Total"), ("A2", "1"), ("A3", "2"), ("A4", "3")]);
        s.execute("insert.table", json!({"range": "A1:B4"})).unwrap();
        s.execute("cell.set", json!({"cell": "B2", "input": "=A2*10"})).unwrap();
        assert_eq!(text(&s, "B4"), "=A4*10", "the column is calculated");
        s.execute("cell.set", json!({"cell": "B3", "input": "7"})).unwrap();
        assert_eq!(found(&mut s, &["calculatedColumn"]), [("B3".to_string(), "calculatedColumn".to_string())]);
        s.execute("formulas.errorFix", json!({"cell": "B3", "fix": "restoreColumnFormula"})).unwrap();
        assert_eq!(text(&s, "B3"), "=A3*10");
        assert!(found(&mut s, &["calculatedColumn"]).is_empty());
    }

    #[test]
    fn two_digit_years_are_recognised() {
        assert_eq!(two_digit_year("1/5/25"), Some((25, 1, 5)));
        assert_eq!(two_digit_year("25/1/5"), Some((25, 1, 5)));
        assert_eq!(two_digit_year("5-Jan-99"), Some((99, 1, 5)));
        for no in ["1/5/2025", "2025/1/5", "abc", "1/5", "13/40/25", "1.5"] {
            assert_eq!(two_digit_year(no), None, "{no}");
        }
    }
}
