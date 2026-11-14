//! Data tab: sort, filter, remove duplicates, text to columns, data validation, outline.

use std::cmp::Ordering;
use std::sync::Arc;

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{
    Anchor, AnchorMode, AutoFilter, Cell, CfOperator, Comment, ErrorStyle, FilterCriterion, Formula, Hyperlink, LineInfo, Sheet, Validation,
    ValidationKind, Workbook,
};
use serde_json::{Value as Json, json};

use super::{
    CommandSpec, EngineError, Result, Session, bad, bool_param, cell_param, cell_param_on, cmd, edit, has_doc, ok, parse_lines, split_sheet,
    str_param, target_range, target_ranges, target_sheet,
};
use crate::outline;
use crate::selection::current_region;

mod filter;
mod grouping;
mod sort;
mod subtotal;
mod tools;
mod validation;

pub use filter::{FilterColumnKind, filter_column_kind, filter_criterion_at, filter_values};
pub(crate) use filter::{apply_filter, grow_autofilter};
use filter::{clear_filter, filter_by, reapply, toggle_filter};
use grouping::{detail, group, outline_settings, show_levels};
use sort::{quick_sort, sort};
use subtotal::{remove_subtotals, subtotal};
pub use subtotal::{subtotal_fields, subtotal_functions, subtotal_list};
/// Text to Columns' field splitting, for the wizard's preview.
pub use tools::split_delimited;
use tools::{remove_duplicates, text_to_columns};
pub use validation::{check_validation, list_items};
use validation::{circle_invalid, validate_cmd, validation};
pub(super) use validation::{move_rule_formula, remove_validation_cells};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("data.sortAscending", "Sort A to Z", ["Data", "Sort & Filter"], None, "{range?, column?: \"B\"}", has_doc, |s, p| quick_sort(
            s, p, true
        )),
        cmd!("data.sortDescending", "Sort Z to A", ["Data", "Sort & Filter"], None, "{range?, column?}", has_doc, |s, p| quick_sort(s, p, false)),
        cmd!(
            "data.sort",
            "Sort…",
            ["Data", "Sort & Filter"],
            None,
            "{range?, header?: bool, keys: [{column: \"B\", order: asc|desc, by?: values|cellColor|fontColor, customList?: [..]}], orientation?: rows|columns, matchCase?, phonetic?: bool (Japanese collation: sort cells with furigana by their reading, Excel's \"Use phonetic\", default true)}",
            has_doc,
            sort
        ),
        cmd!(
            "data.filter",
            "Filter",
            ["Data", "Sort & Filter"],
            Some("Ctrl+Shift+L"),
            "{range?} toggles AutoFilter on the current region",
            has_doc,
            toggle_filter
        ),
        cmd!(
            "data.filterBy",
            "Filter Column",
            [],
            None,
            "{column: \"B\" (or 0-based offset), values?: [..], blanks?: bool, custom?: {op: = | <> | > | >= | < | <= | beginsWith | notBeginsWith | endsWith | notEndsWith | contains | notContains, value: \"10\" (wildcards ? and *; a date is stored as its serial), op2?, value2?, and?: bool (default true; false = Or)}, top?: {count: 1-500 items or 1-100 percent, bottom?, percent?}, dynamic?: aboveAverage | belowAverage | tomorrow | today | yesterday | nextWeek | thisWeek | lastWeek | nextMonth | thisMonth | lastMonth | nextQuarter | thisQuarter | lastQuarter | nextYear | thisYear | lastYear | yearToDate | Q1-Q4 | M1-M12, aboveAverage?: bool, clear?: bool}",
            has_doc,
            filter_by
        ),
        cmd!("data.clearFilter", "Clear", ["Data", "Sort & Filter"], None, "{}", has_doc, clear_filter),
        cmd!("data.reapply", "Reapply", ["Data", "Sort & Filter"], None, "{}", has_doc, reapply),
        cmd!(
            "data.removeDuplicates",
            "Remove Duplicates",
            ["Data", "Data Tools"],
            None,
            "{range?, columns?: [\"A\",\"C\"], header?: bool}",
            has_doc,
            remove_duplicates
        ),
        cmd!(
            "data.textToColumns",
            "Text to Columns",
            ["Data", "Data Tools"],
            None,
            "{range?, delimiters?: [\",\", \"\\t\", \" \", \";\"], other?: \"|\", fixedWidths?: [5, 10], treatConsecutive?: bool, textQualifier?: \"\\\"\", destination?: \"B1\", columns?: [general|text|ymd|mdy|dmy|ydm|myd|dym|skip] (step 3 Column data format, per column)}: fixedWidths are the break positions (characters from the start)",
            has_doc,
            text_to_columns
        ),
        cmd!(
            "data.validation",
            "Data Validation…",
            ["Data", "Data Tools"],
            None,
            "{range?, type: any|whole|decimal|list|date|time|textLength|custom, operator?: between|notBetween|equal|notEqual|greater|less|greaterOrEqual|lessOrEqual, formula1, formula2?, ignoreBlank?, dropdown?, showInputMessage?: bool (default true), inputTitle?, inputMessage?, showErrorMessage?: bool (default true), errorStyle?: stop|warning|information, errorTitle?, errorMessage?, clear?: bool}",
            has_doc,
            validation
        ),
        cmd!(query "data.validate", "Check Value Against Validation", [], None, "{cell?, input} → {ok, message?}", has_doc, validate_cmd),
        cmd!(noundo "data.circleInvalid", "Circle Invalid Data", ["Data", "Data Tools", "Data Validation"], None, "{} → {invalid: [cell], truncated: bool (true when the 100,000-cell scan allowance ended before every range was checked)}", has_doc, circle_invalid),
        CommandSpec {
            shortcut_mac: Some("Cmd+Shift+K"),
            ..cmd!(
                "data.group",
                "Group",
                ["Data", "Outline"],
                Some("Alt+Shift+Right"),
                "{rows?: \"2:5\" | cols?: \"B:C\" | by?: rows|cols (the selection's rows or columns; a block of cells groups rows by default), sheet?}",
                has_doc,
                |s, p| group(s, p, true)
            )
        },
        CommandSpec {
            shortcut_mac: Some("Cmd+Shift+J"),
            ..cmd!(
                "data.ungroup",
                "Ungroup",
                ["Data", "Outline"],
                Some("Alt+Shift+Left"),
                "{rows? | cols? | by?: rows|cols, sheet?}",
                has_doc,
                |s, p| { group(s, p, false) }
            )
        },
        cmd!(
            "data.hideDetail",
            "Hide Detail",
            ["Data", "Outline"],
            None,
            "{rows? | cols? | by?, level?: 1-7} (the innermost group, or the one at `level`, holding the lines or summarised by them)",
            has_doc,
            |s, p| detail(s, p, true)
        ),
        cmd!("data.showDetail", "Show Detail", ["Data", "Outline"], None, "{rows? | cols? | by?, level?: 1-7}", has_doc, |s, p| detail(s, p, false)),
        cmd!(
            "data.showLevels",
            "Show Outline Levels",
            [],
            None,
            "{rows?: 1-8, cols?: 1-8} (1 shows only the top-level summaries, as the outline's level buttons)",
            has_doc,
            show_levels
        ),
        cmd!(
            "data.outlineSettings",
            "Outline Settings",
            ["Data", "Outline"],
            None,
            "{sheet?, summaryBelow?: bool, summaryRight?: bool} (summary rows below their detail, summary columns right of it; off puts them above / left) → {summaryBelow, summaryRight}",
            has_doc,
            outline_settings
        ),
        cmd!(
            "data.subtotal",
            "Subtotal",
            ["Data", "Outline"],
            None,
            "{range?, groupBy: \"店舗\" | \"A\" (At each change in: a header name, a column letter, or a position in the list), function?: sum|count|average|max|min|product|countNums|stdDev|stdDevP|var|varP, columns: [\"金額\", \"D\"] (Add subtotal to: header names or letters), replace?: bool (Replace current subtotals, default true), pageBreaks?: bool (Page break between groups, default false), summaryBelow?: bool (Summary below data, default true), header?: true} → {groups, removed, grandTotal}. The rows get a three-level outline: 1 grand total, 2 subtotals, 3 detail",
            has_doc,
            subtotal
        ),
        cmd!(
            "data.removeSubtotals",
            "Remove All Subtotals",
            ["Data", "Outline"],
            None,
            "{range?, header?: true} (the Subtotal dialog's Remove All: deletes the list's SUBTOTAL rows, its row outline and page breaks) → {removed, range}",
            has_doc,
            remove_subtotals
        ),
        cmd!(
            query "data.editLinks",
            "Workbook Links",
            ["Data", "Queries & Connections"],
            None,
            "{} → {links: [{link: 1, name: \"予算.xlsx\", folder, target, sheets, formulas, status}]}: the other workbooks this one's formulas read (=[予算.xlsx]予算!B3), as Excel's Workbook Links (Edit Links) lists them",
            has_doc,
            crate::workbook_links::list
        ),
        cmd!(
            "data.updateLinks",
            "Refresh Workbook Links",
            ["Data", "Queries & Connections", "Workbook Links"],
            None,
            "{link?: 1 | \"予算.xlsx\"} (default: every link) reads the values again from the linked workbook, open in this session or its file beside this one (Excel's Update Values / Refresh) → {links}",
            has_doc,
            crate::workbook_links::update
        ),
        cmd!(
            "data.changeLinkSource",
            "Change Link Source",
            ["Data", "Queries & Connections", "Workbook Links"],
            None,
            "{link: 1 | \"予算.xlsx\", source: \"予算2026.xlsx\" | \"C:\\会計\\予算.xlsx\"} points the formulas that read the link at another workbook and reads its values (Excel's Change Source) → {links}",
            has_doc,
            crate::workbook_links::change_source
        ),
        cmd!(
            "data.breakLinks",
            "Break Workbook Links",
            ["Data", "Queries & Connections", "Workbook Links"],
            None,
            "{link?: 1 | \"予算.xlsx\"} (default: every link) turns the formulas that read the link into their values and removes the link (Excel's Break Link) → {cells, links}",
            has_doc,
            crate::workbook_links::break_links
        ),
        cmd!("data.flashFill", "Flash Fill", ["Data", "Data Tools"], None, "{range?}", has_doc, |s, p| s.execute("edit.flashFill", p.clone())),
    ]
}

fn col_param(v: Option<&Json>, base: u32) -> Option<u32> {
    match v? {
        Json::String(s) => gridcraft_core::letters_to_col(s.trim_start_matches('$')),
        Json::Number(n) => n.as_u64().and_then(|o| u32::try_from(o).ok()).and_then(|o| base.checked_add(o)),
        _ => None,
    }
}

/// The range a data command works on: the selection when it's a block, else the current
/// region (trimmed to the used range for whole columns).
fn data_range(s: &Session, p: &Json) -> Result<RangeRef> {
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    if p.get("range").is_some() {
        return target_range(s, p);
    }
    if let Some(t) = sh.table_at(d.selection.active) {
        return Ok(t.data_range().map(|dr| RangeRef::new(t.range.start, dr.end)).unwrap_or(t.range));
    }
    let sel = d.selection.current();
    if !sel.is_single() {
        // An empty sheet has no data to work on: just the active cell.
        let Some(used) = sh.used_range() else { return Ok(RangeRef::cell(d.selection.active)) };
        return Ok(sel.intersection(&used).unwrap_or(sel));
    }
    Ok(current_region(sh, d.selection.active))
}

/// The sheet and range a data command works on: the sheet `sheet` or a `Sheet!` prefix on `range`
/// names, else the active one. The selection belongs to the active sheet, so another sheet needs `range`.
fn data_target(s: &Session, p: &Json, cmd: &str) -> Result<(usize, RangeRef)> {
    let sheet = target_sheet(s, p)?;
    let d = s.doc()?;
    if let Some(t) = str_param(p, "range")
        && let (Some(name), _) = split_sheet(t)
        && d.wb.sheet_index(&name) != Some(sheet)
    {
        return Err(bad(cmd, format!("`{t}` names a sheet other than `sheet`")));
    }
    if sheet != d.wb.active_sheet() && p.get("range").is_none() {
        return Err(bad(cmd, "missing `range`: the selection is on the active sheet"));
    }
    Ok((sheet, data_range(s, p)?))
}

/// Excel's header guess: the first row is text over a column of numbers, or is bold.
fn guess_header(wb: &Workbook, sh: &Sheet, r: RangeRef) -> bool {
    if let Some(t) = sh.table_at(r.start) {
        return t.header_row;
    }
    if r.height() < 2 {
        return false;
    }
    let mut votes = 0i32;
    for c in r.start.col..=r.end.col {
        let a = sh.value(CellRef::new(r.start.row, c));
        let b = sh.value(CellRef::new(r.start.row + 1, c));
        if a.is_text() && !b.is_text() && !b.is_empty() {
            votes += 1;
        }
        if wb.styles.get(sh.style_id(CellRef::new(r.start.row, c))).font.bold
            != wb.styles.get(sh.style_id(CellRef::new(r.start.row + 1, c))).font.bold
        {
            votes += 1;
        }
        if a.is_text() && b.is_text() {
            votes -= 0;
        }
    }
    votes > 0
}

#[cfg(test)]
mod tests;
