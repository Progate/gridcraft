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
    Hours,
    Minutes,
    Seconds,
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
    /// Grouping › Starting at (a date serial; `None` = Auto, the earliest date). Earlier dates
    /// fall in a `<start` item. Every level of one date field carries the same range.
    pub date_start: Option<f64>,
    /// Grouping › Ending at (`None` = Auto). Later dates fall in a `>end` item.
    pub date_end: Option<f64>,
    /// Grouping › Number of days, for Days grouping (0 or 1 = one item per day of the year).
    pub date_days: u32,
    /// Label Filters (ラベル フィルター): keeps the items whose caption meets a condition.
    pub label_filter: Option<PivotLabelFilter>,
    /// Value Filters (値フィルター): keeps the items whose total of a value field meets a
    /// condition, or the top/bottom ones (トップテン).
    pub value_filter: Option<PivotValueFilter>,
}

/// The condition of a Label Filter, as Excel's Label Filter dialog lists them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotLabelOp {
    #[default]
    Equal,
    NotEqual,
    BeginsWith,
    NotBeginsWith,
    EndsWith,
    NotEndsWith,
    Contains,
    NotContains,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Between,
    NotBetween,
}

impl PivotLabelOp {
    pub const ALL: [PivotLabelOp; 14] = [
        PivotLabelOp::Equal,
        PivotLabelOp::NotEqual,
        PivotLabelOp::BeginsWith,
        PivotLabelOp::NotBeginsWith,
        PivotLabelOp::EndsWith,
        PivotLabelOp::NotEndsWith,
        PivotLabelOp::Contains,
        PivotLabelOp::NotContains,
        PivotLabelOp::GreaterThan,
        PivotLabelOp::GreaterThanOrEqual,
        PivotLabelOp::LessThan,
        PivotLabelOp::LessThanOrEqual,
        PivotLabelOp::Between,
        PivotLabelOp::NotBetween,
    ];

    /// The command name and the `filter type` of a PivotTable file (`captionEqual`).
    pub fn name(self) -> &'static str {
        match self {
            PivotLabelOp::Equal => "equal",
            PivotLabelOp::NotEqual => "notEqual",
            PivotLabelOp::BeginsWith => "beginsWith",
            PivotLabelOp::NotBeginsWith => "notBeginsWith",
            PivotLabelOp::EndsWith => "endsWith",
            PivotLabelOp::NotEndsWith => "notEndsWith",
            PivotLabelOp::Contains => "contains",
            PivotLabelOp::NotContains => "notContains",
            PivotLabelOp::GreaterThan => "greaterThan",
            PivotLabelOp::GreaterThanOrEqual => "greaterThanOrEqual",
            PivotLabelOp::LessThan => "lessThan",
            PivotLabelOp::LessThanOrEqual => "lessThanOrEqual",
            PivotLabelOp::Between => "between",
            PivotLabelOp::NotBetween => "notBetween",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix("caption").unwrap_or(s);
        Self::ALL.iter().copied().find(|o| o.name().eq_ignore_ascii_case(s))
    }

    /// Whether the condition takes a second value (Between, Not Between).
    pub fn two_values(self) -> bool {
        matches!(self, PivotLabelOp::Between | PivotLabelOp::NotBetween)
    }
}

/// A Label Filter: the condition and its one or two values, as typed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotLabelFilter {
    pub op: PivotLabelOp,
    pub value: String,
    pub value2: String,
}

/// The condition of a Value Filter, as Excel's Value Filter dialog lists them, plus Top 10.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotValueOp {
    #[default]
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Between,
    NotBetween,
    /// Top 10: the `value` largest (or, with `bottom`, smallest) items, by `top_by`.
    Top10,
}

impl PivotValueOp {
    pub const ALL: [PivotValueOp; 9] = [
        PivotValueOp::Equal,
        PivotValueOp::NotEqual,
        PivotValueOp::GreaterThan,
        PivotValueOp::GreaterThanOrEqual,
        PivotValueOp::LessThan,
        PivotValueOp::LessThanOrEqual,
        PivotValueOp::Between,
        PivotValueOp::NotBetween,
        PivotValueOp::Top10,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PivotValueOp::Equal => "equal",
            PivotValueOp::NotEqual => "notEqual",
            PivotValueOp::GreaterThan => "greaterThan",
            PivotValueOp::GreaterThanOrEqual => "greaterThanOrEqual",
            PivotValueOp::LessThan => "lessThan",
            PivotValueOp::LessThanOrEqual => "lessThanOrEqual",
            PivotValueOp::Between => "between",
            PivotValueOp::NotBetween => "notBetween",
            PivotValueOp::Top10 => "top10",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix("value").unwrap_or(s);
        Self::ALL.iter().copied().find(|o| o.name().eq_ignore_ascii_case(s))
    }

    pub fn two_values(self) -> bool {
        matches!(self, PivotValueOp::Between | PivotValueOp::NotBetween)
    }
}

/// What a Top 10 filter counts: items, a percent of the total, or a running sum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotTopBy {
    #[default]
    Items,
    Percent,
    Sum,
}

/// A Value Filter on an axis field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotValueFilter {
    pub op: PivotValueOp,
    /// The value field's caption (`合計 / 金額`).
    pub value_field: String,
    pub value: f64,
    pub value2: f64,
    /// Top 10 only: the bottom items rather than the top ones.
    pub bottom: bool,
    /// Top 10 only.
    pub top_by: PivotTopBy,
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
    /// The field is in no area (not even Filters) and only hides items, as a slicer on a field
    /// that isn't in the report filters it in Excel.
    pub hidden_only: bool,
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
    /// Language of the captions grouping generates (`"ja"`: 2025年, 第1四半期, 1月 and fields 年,
    /// 四半期; otherwise Excel's English: 2025, Qtr1, Jan, Years, Quarters), as the Excel that
    /// grouped the field names them.
    pub lang: String,
    /// Design › Blank Rows › Insert Blank Line after Each Item (空白行を挿入する).
    pub blank_rows: bool,
    /// Design › Report Layout › Repeat All Item Labels (アイテムのラベルをすべて繰り返す), in
    /// the Outline and Tabular layouts.
    pub repeat_labels: bool,
    /// PivotTable Analyze › +/- Buttons (＋/－ボタン): the expand and collapse buttons beside
    /// the row items that have items below them.
    pub show_expand_buttons: bool,
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
            lang: String::new(),
            blank_rows: false,
            repeat_labels: false,
            show_expand_buttons: true,
        }
    }
}

// ---------------------------------------------------------------- date grouping

const MONTHS_EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

impl PivotDateGroup {
    /// Coarse to fine, the order Excel places the levels of one date field in.
    pub const LEVELS: [PivotDateGroup; 7] = [
        PivotDateGroup::Years,
        PivotDateGroup::Quarters,
        PivotDateGroup::Months,
        PivotDateGroup::Days,
        PivotDateGroup::Hours,
        PivotDateGroup::Minutes,
        PivotDateGroup::Seconds,
    ];

    /// Rank in [`Self::LEVELS`] (`None` last).
    pub fn rank(self) -> usize {
        Self::LEVELS.iter().position(|g| *g == self).unwrap_or(Self::LEVELS.len())
    }

    /// The `rangePr groupBy` / command name.
    pub fn name(self) -> &'static str {
        match self {
            PivotDateGroup::None => "none",
            PivotDateGroup::Years => "years",
            PivotDateGroup::Quarters => "quarters",
            PivotDateGroup::Months => "months",
            PivotDateGroup::Days => "days",
            PivotDateGroup::Hours => "hours",
            PivotDateGroup::Minutes => "minutes",
            PivotDateGroup::Seconds => "seconds",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "years" | "year" => PivotDateGroup::Years,
            "quarters" | "quarter" => PivotDateGroup::Quarters,
            "months" | "month" => PivotDateGroup::Months,
            "days" | "day" => PivotDateGroup::Days,
            "hours" | "hour" => PivotDateGroup::Hours,
            "minutes" | "minute" => PivotDateGroup::Minutes,
            "seconds" | "second" => PivotDateGroup::Seconds,
            "none" | "ungroup" => PivotDateGroup::None,
            _ => return None,
        })
    }

    /// The name Excel gives the extra field a coarser level becomes (`Years`; ja Excel: `年`).
    pub fn caption(self, ja: bool) -> &'static str {
        match (self, ja) {
            (PivotDateGroup::Years, false) => "Years",
            (PivotDateGroup::Quarters, false) => "Quarters",
            (PivotDateGroup::Months, false) => "Months",
            (PivotDateGroup::Days, false) => "Days",
            (PivotDateGroup::Hours, false) => "Hours",
            (PivotDateGroup::Minutes, false) => "Minutes",
            (PivotDateGroup::Seconds, false) => "Seconds",
            (PivotDateGroup::None, false) => "Group",
            (PivotDateGroup::Years, true) => "年",
            (PivotDateGroup::Quarters, true) => "四半期",
            (PivotDateGroup::Months, true) => "月",
            (PivotDateGroup::Days, true) => "日",
            (PivotDateGroup::Hours, true) => "時",
            (PivotDateGroup::Minutes, true) => "分",
            (PivotDateGroup::Seconds, true) => "秒",
            (PivotDateGroup::None, true) => "グループ",
        }
    }
}

/// A date as the group boundary items show it: `4/1/2025`, ja `2025/4/1`.
pub fn date_group_date(sys: gridcraft_core::DateSystem, serial: f64, ja: bool) -> String {
    match gridcraft_core::date::datetime_from_serial(sys, serial) {
        Some(d) if ja => format!("{}/{}/{}", d.year, d.month, d.day),
        Some(d) => format!("{}/{}/{}", d.month, d.day, d.year),
        None => gridcraft_core::number_to_text(serial),
    }
}

/// (caption, sort key) of a date serial's group item, as Excel captions it (en / ja Excel).
/// Days with `days` > 1 count `days`-day ranges from `start`. Boundaries are not handled here.
pub fn date_group_item(sys: gridcraft_core::DateSystem, serial: f64, by: PivotDateGroup, ja: bool, start: f64, days: u32) -> Option<(String, f64)> {
    let dt = gridcraft_core::date::datetime_from_serial(sys, serial)?;
    let mi = dt.month.saturating_sub(1) as usize;
    Some(match by {
        PivotDateGroup::None => return None,
        PivotDateGroup::Years => (if ja { format!("{}年", dt.year) } else { dt.year.to_string() }, dt.year as f64),
        PivotDateGroup::Quarters => {
            let q = mi / 3 + 1;
            (if ja { format!("第{q}四半期") } else { format!("Qtr{q}") }, q as f64)
        }
        PivotDateGroup::Months => {
            (if ja { format!("{}月", dt.month) } else { MONTHS_EN.get(mi).copied().unwrap_or("Jan").to_string() }, dt.month as f64)
        }
        PivotDateGroup::Days if days > 1 => {
            let k = ((serial.floor() - start.floor()) / days as f64).floor().max(0.0);
            let a = start.floor() + k * days as f64;
            (format!("{} - {}", date_group_date(sys, a, ja), date_group_date(sys, a + days as f64 - 1.0, ja)), k)
        }
        PivotDateGroup::Days => {
            let label =
                if ja { format!("{}月{}日", dt.month, dt.day) } else { format!("{}-{}", dt.day, MONTHS_EN.get(mi).copied().unwrap_or("Jan")) };
            (label, (dt.month * 100 + dt.day) as f64)
        }
        PivotDateGroup::Hours => (hour_caption(dt.hour, ja), dt.hour as f64),
        PivotDateGroup::Minutes => (if ja { format!("{:02}分", dt.minute) } else { format!(":{:02}", dt.minute) }, dt.minute as f64),
        PivotDateGroup::Seconds => (if ja { format!("{:02}秒", dt.second) } else { format!(":{:02}", dt.second) }, dt.second as f64),
    })
}

fn hour_caption(h: u32, ja: bool) -> String {
    if ja {
        return format!("{h}時");
    }
    let h12 = if h.is_multiple_of(12) { 12 } else { h % 12 };
    format!("{h12} {}", if h < 12 { "AM" } else { "PM" })
}

/// Every group item of a level, as Excel lists them in `groupItems`: `<start`, the items, `>end`.
pub fn date_group_items(sys: gridcraft_core::DateSystem, by: PivotDateGroup, ja: bool, start: f64, end: f64, days: u32) -> Vec<String> {
    let mut v = vec![format!("<{}", date_group_date(sys, start, ja))];
    match by {
        PivotDateGroup::Years => {
            let y = |s: f64| gridcraft_core::date::datetime_from_serial(sys, s).map(|d| d.year);
            let y0 = y(start).unwrap_or(1900);
            let y1 = y(end - 1.0).unwrap_or(y0).max(y0);
            v.extend((y0..=y1.min(y0 + 10_000)).map(|y| if ja { format!("{y}年") } else { y.to_string() }));
        }
        PivotDateGroup::Quarters => v.extend((1..=4).map(|q| if ja { format!("第{q}四半期") } else { format!("Qtr{q}") })),
        PivotDateGroup::Months => v.extend((1..=12).map(|m| if ja { format!("{m}月") } else { MONTHS_EN[m - 1].to_string() })),
        PivotDateGroup::Days if days > 1 => {
            let n = ((end.floor() - start.floor()) / days as f64).ceil().clamp(1.0, 100_000.0) as usize;
            for k in 0..n {
                let a = start.floor() + (k as u32 * days) as f64;
                v.push(format!("{} - {}", date_group_date(sys, a, ja), date_group_date(sys, a + days as f64 - 1.0, ja)));
            }
        }
        PivotDateGroup::Days | PivotDateGroup::None => {
            const DAYS: [u32; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
            for (m, n) in DAYS.iter().enumerate() {
                for d in 1..=*n {
                    v.push(if ja { format!("{}月{d}日", m + 1) } else { format!("{d}-{}", MONTHS_EN[m]) });
                }
            }
        }
        PivotDateGroup::Hours => v.extend((0..24).map(|h| hour_caption(h, ja))),
        PivotDateGroup::Minutes => v.extend((0..60).map(|m| if ja { format!("{m:02}分") } else { format!(":{m:02}") })),
        PivotDateGroup::Seconds => v.extend((0..60).map(|m| if ja { format!("{m:02}秒") } else { format!(":{m:02}") })),
    }
    v.push(format!(">{}", date_group_date(sys, end, ja)));
    v
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
