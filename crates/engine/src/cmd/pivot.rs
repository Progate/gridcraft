//! PivotTable commands (Insert › PivotTable, PivotTable Analyze, PivotTable Design, Data ›
//! Refresh All). Every change re-renders the report as one undo step.
//!
//! `pivot?` parameters name a PivotTable (name or numeric id); by default it is the pivot whose
//! report contains the active cell, else the last pivot on the active sheet.

use std::sync::Arc;

use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::{
    Anchor, Cell, Chart, ChartKind, LegendPos, NumFmt, PivotCalcField, PivotDateGroup, PivotField, PivotFilter, PivotFunc, PivotLayout,
    PivotRangeGroup, PivotShowAs, PivotSort, PivotTable, PivotValue, Sheet, StyleId, Workbook,
};
use serde_json::{Value as Json, json};

use super::{
    CommandSpec, Ctx, EngineError, Result, Session, anchor_mode_param, bad, bool_param, cell_param, cmd, edit, f64_param, has_doc, ok, str_param,
    u32_param,
};
use crate::pivot as pv;
use crate::selection::current_region;

mod analyze;
mod charts;
mod create;
mod fields;
mod layout;
mod params;
mod slicers;

use analyze::{blank_rows, buttons, clear_filter, expand_buttons, field_headers, field_settings, items, label_filter, repeat_labels, value_filter};
pub(crate) use charts::insert_pivot_chart;
use charts::recommended;
use create::{insert_pivot, options, show_details, summarize_table};
use fields::{add_field, calculated_field, fields, list, move_field, remove_field, value_settings};
use layout::{change_source, collapse, delete, filter, grand_totals, group, layout, refresh, refresh_all, sort};
pub use slicers::pivot_slicer_items;
pub(crate) use slicers::{insert_slicer as insert_pivot_slicer, set_pivot_slicer};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "insert.pivotTable",
            "PivotTable",
            ["Insert", "Tables"],
            None,
            "{source?: \"Sheet1!$A$1:$E$200\" | table name (default: the table or current region of the selection), destination?: \"new\" (default: new sheet before the current one, report at A3) | \"Sheet1!H3\", name?, rows?: [field | {field, group?: years|quarters|months|days | [years, quarters, months]}], columns?: [..], values?: [field | {field, func?, name?}], filters?: [field], lang?: \"ja\" (Japanese group captions), replace?: bool} → {id, name, sheet, anchor, fields}",
            has_doc,
            insert_pivot
        ),
        cmd!(
            "insert.recommendedPivotTables",
            "Recommended PivotTables",
            ["Insert", "Tables"],
            None,
            "{source?, apply?: index, destination?} → suggestions [{index, title, rows, columns, values}]; with apply, creates that one",
            has_doc,
            recommended
        ),
        cmd!(
            "table.summarizePivot",
            "Summarize with PivotTable",
            ["Table Design", "Tools"],
            None,
            "{table?, destination?} creates a PivotTable from the table",
            has_doc,
            summarize_table
        ),
        cmd!(
            "pivot.calculatedField",
            "Calculated Field",
            ["PivotTable Analyze", "Calculations"],
            None,
            "{pivot?, name, formula?: \"=Sales*0.1\" (fields bare or 'quoted'; evaluated on each cell's sums), delete?: bool, values?: bool (add a new field to Values, default true)} adds, changes or deletes a calculated field; without `name` opens the dialog → {name, formula, calculatedFields}",
            has_doc,
            calculated_field
        ),
        cmd!(
            "insert.pivotChart",
            "PivotChart",
            ["Insert", "Charts"],
            None,
            "{pivot?: name | id | a cell of the report (\"Sheet2!B5\"; default: the PivotTable at the active cell), type?: column|bar|line|pie|area|… (default column), subtype?, title?, at?: \"H2\", width?, height?} adds a chart bound to the PivotTable's report: categories are the innermost row items, series the column items × value fields; it follows refreshes and layout changes → {chart, pivot, series}",
            has_doc,
            insert_pivot_chart
        ),
        cmd!(query "pivot.fields", "PivotTable Fields", [], None, "{pivot?} → source fields (name, numeric, date, samples, areas) and the current layout", has_doc, fields),
        cmd!(query "pivot.fieldList", "Field List", ["PivotTable Analyze", "Show"], None, "{pivot?} (same as pivot.fields)", has_doc, fields),
        cmd!(query "pivot.list", "PivotTables", [], None, "{} → every PivotTable in the workbook", has_doc, list),
        cmd!(
            "pivot.addField",
            "Add Field",
            [],
            None,
            "{pivot?, field, area: rows|columns|values|filters, func?: sum|count|average|max|min|product|countNumbers|stdDev|stdDevP|var|varP, name?, position?: index, autoGroup?: true (a date field new to Rows/Columns groups as Excel 365 does: over several years Years › Quarters › months with the years collapsed, within a year by months; false keeps the dates), replace?}",
            has_doc,
            add_field
        ),
        cmd!(
            "pivot.removeField",
            "Remove Field",
            [],
            None,
            "{pivot?, field (or value caption), area?: rows|columns|values|filters, replace?}",
            has_doc,
            remove_field
        ),
        cmd!(
            "pivot.moveField",
            "Move Field",
            [],
            None,
            "{pivot?, field (\"Σ Values\" moves the value fields between rows and columns; in columns it stays after the last field), from: rows|columns|values|filters, to: rows|columns|values|filters, position?: index, replace?}",
            has_doc,
            move_field
        ),
        cmd!(
            "pivot.valueSettings",
            "Value Field Settings",
            ["PivotTable Analyze", "Active Field"],
            None,
            "{pivot?, field: caption or source field | index: n, func?, name?, showAs?: normal|percentOfGrandTotal|percentOfColumnTotal|percentOfRowTotal|percentOfParentRowTotal|percentOfParentColumnTotal|runningTotal|percentOfRunningTotal|rank|rankAscending|index, numberFormat?: \"#,##0\" | null, replace?}",
            has_doc,
            value_settings
        ),
        cmd!("pivot.filter", "Filter Items", [], None, "{pivot?, field, selected: [\"East\", ..] | null (all), replace?}", has_doc, filter),
        cmd!("pivot.sort", "Sort Field", [], None, "{pivot?, field, order: asc|desc|none, replace?}", has_doc, sort),
        cmd!(
            query "pivot.items",
            "Field Items",
            [],
            None,
            "{pivot?, field (a row, column or filter field, or a date level such as 年)} → {field, area, items: [{label, selected}], filtered, labelFilter, valueFilter, sort, valueFields}: what the ▼ of 行ラベル / 列ラベル lists",
            has_doc,
            items
        ),
        cmd!(
            "pivot.labelFilter",
            "Label Filters",
            [],
            None,
            "{pivot?, field, op: equal|notEqual|beginsWith|notBeginsWith|endsWith|notEndsWith|contains|notContains|greaterThan|greaterThanOrEqual|lessThan|lessThanOrEqual|between|notBetween, value, value2? (between), clear?: true} keeps the row or column items whose caption meets the condition (* and ? wildcards); it replaces the field's Value Filter",
            has_doc,
            label_filter
        ),
        cmd!(
            "pivot.valueFilter",
            "Value Filters",
            [],
            None,
            "{pivot?, field, op: equal|notEqual|greaterThan|greaterThanOrEqual|lessThan|lessThanOrEqual|between|notBetween|top10, valueField?: caption (default: the first value field), value, value2? (between), order?: top|bottom (top10), by?: items|percent|sum (top10), clear?: true} keeps the row or column items whose total meets the condition; it replaces the field's Label Filter",
            has_doc,
            value_filter
        ),
        cmd!(
            "pivot.clearFilter",
            "Clear Filter",
            ["PivotTable Analyze", "Actions"],
            None,
            "{pivot?, field?} clears a field's checked items, Label and Value Filters (all fields when `field` is left out)",
            has_doc,
            clear_filter
        ),
        cmd!(
            "pivot.insertSlicer",
            "Insert Slicer",
            ["PivotTable Analyze", "Filter"],
            None,
            "{pivot?, field: \"店舗\" | fields: [..], at?: \"H3\" (default: right of the report), columns?: 1, width?, height?} adds a slicer per field that filters the PivotTable (its selection is the field's filter; slicer.toggle / slicer.clear click it); without a field opens Insert Slicers → {slicers, slicer}",
            has_doc,
            insert_pivot_slicer
        ),
        cmd!(
            "pivot.fieldSettings",
            "Field Settings",
            ["PivotTable Analyze", "Active Field"],
            None,
            "{pivot?, field? (default: the field of the active cell)} opens Value Field Settings for a value field, else Field Settings",
            has_doc,
            field_settings
        ),
        cmd!(
            "pivot.expandButtons",
            "+/- Buttons",
            ["PivotTable Analyze", "Show"],
            None,
            "{pivot?, on?: bool (default: switch)} shows or hides the expand/collapse buttons of the row items",
            has_doc,
            expand_buttons
        ),
        cmd!(
            "pivot.fieldHeaders",
            "Field Headers",
            ["PivotTable Analyze", "Show"],
            None,
            "{pivot?, on?: bool (default: switch)} shows or hides the 行ラベル / 列ラベル headers and their ▼ buttons",
            has_doc,
            field_headers
        ),
        cmd!(
            "pivot.blankRows",
            "Blank Rows",
            ["PivotTable Design", "Layout"],
            None,
            "{pivot?, on?: bool (default: switch)} Insert Blank Line after Each Item (on) / Remove Blank Line after Each Item (off)",
            has_doc,
            blank_rows
        ),
        cmd!(
            "pivot.repeatLabels",
            "Repeat All Item Labels",
            ["PivotTable Design", "Layout"],
            None,
            "{pivot?, on?: bool (default: switch)} Report Layout › Repeat All Item Labels (on) / Do Not Repeat Item Labels (off), in the Outline and Tabular layouts",
            has_doc,
            repeat_labels
        ),
        cmd!(query "pivot.buttons", "PivotTable Buttons", [], None, "{pivot?} → the report's buttons: [{kind: field|expand, cell, area, fields | field, item, collapsed}]", has_doc, buttons),
        cmd!(
            "pivot.group",
            "Group Field",
            ["PivotTable Analyze", "Group"],
            None,
            "{pivot?, field (default: the field whose label the active cell shows; a date level such as Years / 年 by its name), by: years|quarters|months|days|hours|minutes|seconds|range|none or a list of date levels (coarse ones become fields Years, Quarters…; none = Ungroup), start?: number or date | \"auto\", end?: number or date | \"auto\", autoStart?, autoEnd?, interval?: number (range By, default 10), days?: n (Days only: Number of days), lang?: \"ja\" (captions; default: the workbook region), replace?} — with only the field: the Grouping dialog. → {start, end, interval, autoStart, autoEnd} for ranges, {levels} for dates",
            has_doc,
            group
        ),
        cmd!(
            "pivot.collapse",
            "Expand/Collapse Field",
            ["PivotTable Analyze", "Active Field"],
            None,
            "{pivot?, field? (default: the field whose label the active cell shows: Expand Field / Collapse Field), items?: [labels] (default: all), collapse?: true, replace?}",
            has_doc,
            collapse
        ),
        cmd!(
            "pivot.layout",
            "PivotTable Layout",
            ["PivotTable Design", "Layout"],
            None,
            "{pivot?, layout?: compact|outline|tabular, grandTotalsRows?: bool, grandTotalsCols?: bool, subtotals?: bool|top|bottom|none, style?: \"PivotStyleLight16\", showHeaders?: bool, replace?}",
            has_doc,
            layout
        ),
        cmd!(
            "pivot.grandTotals",
            "Grand Totals",
            ["PivotTable Design", "Layout"],
            None,
            "{pivot?, mode?: off|on|rows|columns, rows?: bool, cols?: bool}",
            has_doc,
            grand_totals
        ),
        cmd!("pivot.subtotals", "Subtotals", ["PivotTable Design", "Layout"], None, "{pivot?, mode: none|top|bottom}", has_doc, |s, p| {
            let mode = str_param(p, "mode").unwrap_or("top");
            layout(s, &with(p, "subtotals", json!(mode)))
        }),
        cmd!(
            "pivot.reportLayout",
            "Report Layout",
            ["PivotTable Design", "Layout"],
            None,
            "{pivot?, layout: compact|outline|tabular}",
            has_doc,
            layout
        ),
        cmd!(
            "pivot.style",
            "PivotTable Styles",
            ["PivotTable Design", "PivotTable Styles"],
            None,
            "{pivot?, style: \"PivotStyleMedium9\"}",
            has_doc,
            layout
        ),
        cmd!("pivot.refresh", "Refresh", ["PivotTable Analyze", "Data"], Some("Alt+F5"), "{pivot?, replace?}", has_doc, refresh),
        cmd!(
            "data.refreshAll",
            "Refresh All",
            ["Data", "Queries & Connections"],
            Some("Cmd+Alt+F5"),
            "{replace?} refreshes every PivotTable → {refreshed, errors}",
            has_doc,
            refresh_all
        ),
        cmd!(
            "pivot.changeSource",
            "Change Data Source",
            ["PivotTable Analyze", "Data"],
            None,
            "{pivot?, source: \"Sheet1!$A$1:$E$300\" | table name, replace?} (fields missing from the new source are removed)",
            has_doc,
            change_source
        ),
        cmd!(
            "pivot.showDetails",
            "Show Details",
            [],
            None,
            "{cell?: \"B5\" (a value, subtotal or grand total cell of a PivotTable on the active sheet; default: the active cell)} puts the source records behind that value as a table (Table1 / テーブル1) on a new sheet before the current one → {sheet, records, table}. An error when the PivotTable's Enable show details option is off (`pivot.options` `{enableShowDetails: true}` turns it on).",
            has_doc,
            show_details
        ),
        cmd!(
            "pivot.options",
            "PivotTable Options",
            ["PivotTable Analyze", "PivotTable"],
            None,
            "{pivot?, enableShowDetails?: bool (Data › Enable show details: off, Show Details and double-clicking a value cell list no records)} → {enableShowDetails}",
            has_doc,
            options
        ),
        cmd!(
            "pivot.delete",
            "Delete PivotTable",
            ["PivotTable Analyze", "Actions"],
            None,
            "{pivot?} clears the report and removes the PivotTable",
            has_doc,
            delete
        ),
    ]
}

fn with(p: &Json, k: &str, v: Json) -> Json {
    let mut m = p.as_object().cloned().unwrap_or_default();
    m.insert(k.into(), v);
    Json::Object(m)
}

fn other(msg: impl Into<String>) -> EngineError {
    EngineError::Other(msg.into())
}

// ---------------------------------------------------------------- lookup

/// (sheet, index) of the pivot a call targets.
fn find_pivot(s: &Session, p: &Json) -> Result<(usize, usize)> {
    let d = s.doc()?;
    let active = d.wb.active_sheet();
    if let Some(v) = p.get("pivot").filter(|v| !v.is_null()) {
        let mut order: Vec<usize> = vec![active];
        order.extend((0..d.wb.sheets.len()).filter(|i| *i != active));
        for si in order {
            let Some(sh) = d.wb.sheet(si) else { continue };
            let hit = sh.pivots.iter().position(|pt| match v {
                Json::String(n) => pt.name.eq_ignore_ascii_case(n),
                Json::Number(n) => n.as_u64() == Some(pt.id as u64),
                _ => false,
            });
            if let Some(i) = hit {
                return Ok((si, i));
            }
        }
        return Err(bad("pivot", format!("no PivotTable `{}`", v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))));
    }
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    let at = d.selection.active;
    if let Some(i) = sh.pivots.iter().position(|pt| pt.last_range.is_some_and(|r| r.contains(at))) {
        return Ok((active, i));
    }
    if !sh.pivots.is_empty() {
        return Ok((active, sh.pivots.len() - 1));
    }
    Err(EngineError::Disabled("pivot".into(), "select a cell in a PivotTable (or pass `pivot`)".into()))
}

#[cfg(test)]
mod analyze_tests;
#[cfg(test)]
mod tests;
