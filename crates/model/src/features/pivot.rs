use super::{CellRef, RangeRef};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- pivot tables

/// How a PivotTable lays out its row fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotLayout {
    /// All row fields in one column, inner items indented ("Row Labels" header).
    #[default]
    Compact,
    /// One column per row field; each item on its own row.
    Outline,
    /// One column per row field; inner items start on their parent's row, totals below.
    Tabular,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotSort {
    #[default]
    Asc,
    Desc,
    /// Source order (first appearance).
    None,
}

/// Grouping of a date field (values are date serials).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotDateGroup {
    #[default]
    None,
    Years,
    Quarters,
    Months,
    Days,
}

/// Grouping of a numeric field into equal ranges, as Excel's Grouping dialog sets it (Starting at,
/// Ending at, By). An automatic bound follows the field's smallest or largest value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotRangeGroup {
    pub start: f64,
    pub end: f64,
    pub interval: f64,
    pub auto_start: bool,
    pub auto_end: bool,
}

impl Default for PivotRangeGroup {
    fn default() -> Self {
        PivotRangeGroup { start: 0.0, end: 0.0, interval: 10.0, auto_start: true, auto_end: true }
    }
}

/// Most ranges one numeric grouping may produce (besides the `<start` and `>end` items).
pub const MAX_RANGE_BINS: usize = 100_000;

impl PivotRangeGroup {
    /// The ranges for a field whose numbers run from `min` to `max`; `integers` when they are all
    /// whole numbers. `None` when the grouping can't be made (a bad interval, start after end, or
    /// more than [`MAX_RANGE_BINS`] ranges).
    pub fn bins(&self, min: f64, max: f64, integers: bool) -> Option<RangeBins> {
        let start = if self.auto_start { min } else { self.start };
        let end = if self.auto_end { max } else { self.end };
        let interval = self.interval;
        if !(start.is_finite() && end.is_finite() && interval.is_finite()) || interval <= 0.0 || start > end {
            return None;
        }
        let n = ((end - start) / interval).floor() + 1.0;
        if !(n >= 1.0 && n <= MAX_RANGE_BINS as f64) {
            return None;
        }
        // Whole-number data in whole-number steps shows `0-9`; otherwise each range shows its
        // upper bound, which belongs to the next range (`0-0.5`, `0.5-1`).
        let whole = integers && start.fract() == 0.0 && interval.fract() == 0.0;
        Some(RangeBins { start, end, interval, count: n as usize, whole })
    }
}

/// A resolved numeric grouping: item 0 is `<start`, items 1..=count the ranges, the last `>end`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeBins {
    pub start: f64,
    pub end: f64,
    pub interval: f64,
    /// Number of ranges.
    pub count: usize,
    /// Labels show inclusive whole-number ranges (`0-9`).
    pub whole: bool,
}

impl RangeBins {
    /// The item a number falls in.
    pub fn index(&self, v: f64) -> usize {
        if v < self.start {
            return 0;
        }
        if v > self.end {
            return self.count + 1;
        }
        let k = ((v - self.start) / self.interval).floor();
        (k.max(0.0) as usize).min(self.count.saturating_sub(1)) + 1
    }
    /// The caption of item `i`.
    pub fn label(&self, i: usize) -> String {
        use gridcraft_core::number_to_text as t;
        if i == 0 {
            return format!("<{}", t(self.start));
        }
        if i > self.count {
            return format!(">{}", t(self.end));
        }
        let a = self.start + (i - 1) as f64 * self.interval;
        let b = if self.whole { a + self.interval - 1.0 } else { a + self.interval };
        format!("{}-{}", t(a), t(b))
    }
    /// Every item caption, `<start` first and `>end` last.
    pub fn labels(&self) -> Vec<String> {
        (0..self.count + 2).map(|i| self.label(i)).collect()
    }
}

/// A field placed in the Rows or Columns area.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotField {
    /// Source column header name.
    pub source_col: String,
    pub sort: PivotSort,
    pub date_group: PivotDateGroup,
    /// Grouping of a numeric field into equal ranges (`0-9`, `10-19`…); exclusive with `date_group`.
    pub range_group: Option<PivotRangeGroup>,
    /// Item labels whose detail is hidden (collapsed).
    pub collapsed_items: Vec<String>,
}

/// Summarize-values-by function.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotFunc {
    #[default]
    Sum,
    Count,
    Average,
    Max,
    Min,
    Product,
    CountNumbers,
    StdDev,
    StdDevP,
    Var,
    VarP,
}

/// Show-values-as transform.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotShowAs {
    #[default]
    Normal,
    PercentOfGrandTotal,
    PercentOfColumnTotal,
    PercentOfRowTotal,
    /// Running total in the first row field.
    RunningTotal,
    /// Rank largest to smallest among sibling row items.
    Rank,
    /// Each value over its parent row item's value (the grand total for outermost items).
    PercentOfParentRowTotal,
    /// Each value over its parent column item's value (the grand total for outermost items).
    PercentOfParentColumnTotal,
    /// The running total in the first row field over that field's total.
    PercentOfRunningTotal,
    /// Rank smallest to largest among sibling row items.
    RankAscending,
    /// (value × grand total) ÷ (row total × column total).
    Index,
}

/// A field in the Values area.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotValue {
    pub source_col: String,
    pub func: PivotFunc,
    /// Caption, e.g. "Sum of Sales".
    pub name: String,
    pub show_as: PivotShowAs,
    pub number_format: Option<String>,
}

/// A report filter (or, for a field also in Rows/Columns, its hidden-items filter).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotFilter {
    pub source_col: String,
    /// Item labels shown; `None` = all.
    pub selected: Option<Vec<String>>,
}

/// A PivotTable: its definition plus where its output was last written. The output itself is
/// static cell values and styles on the sheet, rewritten on every refresh.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotTable {
    pub id: u32,
    pub name: String,
    /// Source: range formula text (`Sheet1!$A$1:$E$200`), a table name or a defined name.
    pub source: String,
    /// Top-left of the output body on the pivot's sheet.
    pub anchor: CellRef,
    pub rows: Vec<PivotField>,
    pub columns: Vec<PivotField>,
    pub values: Vec<PivotValue>,
    pub filters: Vec<PivotFilter>,
    pub layout: PivotLayout,
    /// PivotTable Options › Data › Enable show details: Show Details lists a value cell's
    /// source records only while this is on.
    pub enable_show_details: bool,
    /// Grand total for each row (the "Grand Total" column), like Excel's "On for Rows".
    pub grand_totals_rows: bool,
    /// Grand total for each column (the "Grand Total" row), like Excel's "On for Columns".
    pub grand_totals_cols: bool,
    pub subtotals: bool,
    /// Subtotals at the top of each group (Compact/Outline); otherwise below it.
    pub subtotals_top: bool,
    pub style: String,
    pub show_headers: bool,
    /// Area written by the last refresh.
    pub last_range: Option<RangeRef>,
    /// Calculated fields, usable in the Values area by name.
    pub calculated_fields: Vec<PivotCalcField>,
    /// The Σ Values pseudo-field (it exists with two or more value fields) sits in Rows rather
    /// than Columns (Excel's "values on rows").
    pub values_on_rows: bool,
    /// Position of Σ Values among the row fields (`None` = after the last). Only used with
    /// `values_on_rows`: in Columns Σ Values always comes after the last column field.
    pub values_position: Option<usize>,
}

/// A calculated field: a formula over other fields (`=Sales*0.1`, `='Unit Price'*Units`). As in
/// Excel, it is evaluated on the sums of the fields it names for each report cell, not per record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotCalcField {
    pub name: String,
    /// Formula text, starting with `=`.
    pub formula: String,
}

impl Default for PivotTable {
    fn default() -> Self {
        PivotTable {
            id: 0,
            name: "PivotTable1".into(),
            source: String::new(),
            anchor: CellRef::default(),
            rows: vec![],
            columns: vec![],
            values: vec![],
            filters: vec![],
            layout: PivotLayout::Compact,
            enable_show_details: true,
            grand_totals_rows: true,
            grand_totals_cols: true,
            subtotals: true,
            subtotals_top: true,
            style: "PivotStyleLight16".into(),
            show_headers: true,
            last_range: None,
            calculated_fields: vec![],
            values_on_rows: false,
            values_position: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_bins_label_whole_number_ranges() {
        let g = PivotRangeGroup { start: 0.0, end: 100.0, interval: 10.0, auto_start: false, auto_end: false };
        let b = g.bins(3.0, 98.0, true).unwrap();
        assert_eq!(b.count, 11);
        let l = b.labels();
        assert_eq!(l.first().map(String::as_str), Some("<0"));
        assert_eq!(l.get(1).map(String::as_str), Some("0-9"));
        assert_eq!(l.get(11).map(String::as_str), Some("100-109"));
        assert_eq!(l.last().map(String::as_str), Some(">100"));
        assert_eq!(b.index(-1.0), 0);
        assert_eq!(b.index(0.0), 1);
        assert_eq!(b.index(19.0), 2);
        assert_eq!(b.index(100.0), 11);
        assert_eq!(b.index(100.5), 12);
    }

    #[test]
    fn range_bins_auto_bounds_and_fractions() {
        let g = PivotRangeGroup { interval: 0.5, ..PivotRangeGroup::default() };
        let b = g.bins(1.0, 2.2, false).unwrap();
        assert_eq!(b.labels(), vec!["<1", "1-1.5", "1.5-2", "2-2.5", ">2.2"]);
        assert!(PivotRangeGroup { interval: 0.0, ..g }.bins(1.0, 2.0, true).is_none());
        assert!(PivotRangeGroup { interval: 1e-9, ..g }.bins(0.0, 1.0, true).is_none());
    }
}
