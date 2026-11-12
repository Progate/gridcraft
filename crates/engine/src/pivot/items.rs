use super::{CellError, DateGrouping, HashMap, Ordering, PivotDateGroup, PivotField, PivotFunc, PivotSort, PivotTable, Source, Value, Workbook};
use gridcraft_model::RangeBins;

// ---------------------------------------------------------------- grouping

/// How a field's values become items: as they are, by a date part, or by numeric range.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Grouping {
    #[default]
    None,
    Date(DateGrouping),
    Range(RangeBins),
}

impl From<PivotDateGroup> for Grouping {
    fn from(g: PivotDateGroup) -> Self {
        if g == PivotDateGroup::None { Grouping::None } else { Grouping::Date(g.into()) }
    }
}

/// A field's grouping over source column `col`. A numeric grouping's automatic bounds come from the
/// column's numbers; one that can't be made (no numbers, too many ranges) leaves the field ungrouped.
/// Date captions follow the table's language ([`super::is_ja`]).
pub fn field_grouping(wb: &Workbook, pt: &PivotTable, src: &Source, col: usize, f: &PivotField) -> Grouping {
    let Some(g) = &f.range_group else {
        return if f.date_group == PivotDateGroup::None { Grouping::None } else { Grouping::Date(DateGrouping::of(wb, pt, f)) };
    };
    let (mut min, mut max, mut integers, mut any) = (f64::INFINITY, f64::NEG_INFINITY, true, false);
    for v in src.rows.iter().filter_map(|r| r.get(col)) {
        if let Value::Number(n) = v.scalar() {
            any = true;
            min = min.min(n);
            max = max.max(n);
            integers &= n.fract() == 0.0;
        }
    }
    if !any {
        return Grouping::None;
    }
    g.bins(min, max, integers).map_or(Grouping::None, Grouping::Range)
}

// ---------------------------------------------------------------- items

#[derive(Clone, Debug)]
pub(super) struct Item {
    pub(super) value: Value,
    pub(super) label: String,
    class: u8,
    num: f64,
    text: String,
    first: usize,
    /// The value is a number shown with the source format.
    pub(super) formatted: bool,
}

#[derive(Default)]
pub(super) struct FieldItems {
    items: Vec<Item>,
    index: HashMap<String, u32>,
}

fn cmp_items(a: &Item, b: &Item) -> Ordering {
    a.class.cmp(&b.class).then(a.num.total_cmp(&b.num)).then_with(|| gridcraft_core::compare_text(&a.text, &b.text))
}

/// (identity key, item) for a source value under a field's grouping.
fn make_item(wb: &Workbook, v: &Value, group: Grouping, fmt: &str, first: usize) -> (String, Item) {
    let it = |value: Value, label: String, class: u8, num: f64, formatted: bool| Item {
        text: label.to_lowercase(),
        value,
        label,
        class,
        num,
        first,
        formatted,
    };
    match v {
        Value::Empty => ("b".into(), it(Value::text("(blank)"), "(blank)".into(), 4, 0.0, false)),
        Value::Number(n) => {
            if let Grouping::Range(bins) = group {
                let i = bins.index(*n);
                let label = bins.label(i);
                return (format!("r{i}"), it(Value::text(label.as_str()), label, 0, i as f64, false));
            }
            if let Grouping::Date(group) = group
                && let Some((key, label)) = date_item(wb, *n, group)
            {
                return (format!("g{key}"), it(Value::text(label.as_str()), label, 0, key, false));
            }
            let label = crate::display::format(v, fmt, wb).text;
            (format!("n{}", n.to_bits()), it(v.clone(), label, 0, *n, true))
        }
        Value::Text(t) => (format!("t{}", t.to_lowercase()), it(v.clone(), t.to_string(), 1, 0.0, false)),
        Value::Bool(b) => {
            let l = if *b { "TRUE" } else { "FALSE" };
            (format!("l{l}"), it(Value::Bool(*b), l.into(), 2, *b as u8 as f64, false))
        }
        Value::Error(e) => (format!("e{}", e.as_str()), it(v.clone(), e.as_str().into(), 3, 0.0, false)),
        Value::Array(_) => make_item(wb, &v.scalar(), group, fmt, first),
    }
}

/// (sort key, caption) of a date serial's group item: `<start` and `>end` items sort first and last.
fn date_item(wb: &Workbook, n: f64, g: DateGrouping) -> Option<(f64, String)> {
    let sys = wb.date_system;
    let whole = g.by.rank() <= PivotDateGroup::Days.rank();
    if let Some(s) = g.start
        && n < s
    {
        return Some((f64::NEG_INFINITY, format!("<{}", gridcraft_model::date_group_date(sys, s, g.ja))));
    }
    if let Some(e) = g.end
        && (if whole { n.floor() > e.floor() } else { n > e })
    {
        return Some((f64::INFINITY, format!(">{}", gridcraft_model::date_group_date(sys, g.end_caption_serial(e), g.ja))));
    }
    let (label, key) = gridcraft_model::date_group_item(sys, n, g.by, g.ja, g.start.unwrap_or(n), g.days)?;
    Some((key, label))
}

/// The earliest and latest date serials of a source column (numbers only).
pub(crate) fn source_date_range(src: &Source, col: usize) -> Option<(f64, f64)> {
    let mut r: Option<(f64, f64)> = None;
    for row in &src.rows {
        if let Some(Value::Number(n)) = row.get(col).map(Value::scalar).as_ref() {
            r = Some(match r {
                Some((a, b)) => (a.min(*n), b.max(*n)),
                None => (*n, *n),
            });
        }
    }
    r
}

impl FieldItems {
    pub(super) fn intern(&mut self, wb: &Workbook, v: &Value, group: Grouping, fmt: &str, first: usize) -> u32 {
        let (key, item) = make_item(wb, v, group, fmt, first);
        if let Some(i) = self.index.get(&key) {
            return *i;
        }
        let i = self.items.len() as u32;
        self.items.push(item);
        self.index.insert(key, i);
        i
    }
    pub(super) fn get(&self, i: u32) -> Option<&Item> {
        self.items.get(i as usize)
    }
    pub(super) fn label(&self, i: u32) -> String {
        self.get(i).map(|x| x.label.clone()).unwrap_or_default()
    }
}

/// The label of a source value under a grouping (for filters and field lists).
pub fn item_label(wb: &Workbook, v: &Value, group: Grouping, fmt: &str) -> String {
    make_item(wb, v, group, fmt, 0).1.label
}

/// Distinct item labels of a source column, sorted like the field list shows them.
pub fn distinct_labels(wb: &Workbook, src: &Source, col: usize, group: Grouping, limit: usize) -> Vec<String> {
    let mut fi = FieldItems::default();
    let fmt = src.formats.get(col).map(String::as_str).unwrap_or("General");
    for (i, r) in src.rows.iter().enumerate() {
        if let Some(v) = r.get(col) {
            fi.intern(wb, v, group, fmt, i);
        }
        if fi.items.len() > 100_000 {
            break;
        }
    }
    let mut items = fi.items;
    items.sort_by(cmp_items);
    items.into_iter().take(limit).map(|i| i.label).collect()
}

// ---------------------------------------------------------------- aggregation

#[derive(Clone, Debug, Default)]
pub(super) struct Acc {
    count: u64,
    nums: u64,
    sum: f64,
    prod: f64,
    min: f64,
    max: f64,
    mean: f64,
    m2: f64,
    err: Option<CellError>,
}

impl Acc {
    pub(super) fn add(&mut self, v: &Value) {
        match v {
            Value::Empty => {}
            Value::Number(n) => {
                let n = *n;
                self.count += 1;
                self.nums += 1;
                self.sum += n;
                if self.nums == 1 {
                    self.prod = n;
                    self.min = n;
                    self.max = n;
                } else {
                    self.prod *= n;
                    self.min = self.min.min(n);
                    self.max = self.max.max(n);
                }
                let d = n - self.mean;
                self.mean += d / self.nums as f64;
                self.m2 += d * (n - self.mean);
            }
            Value::Error(e) => {
                self.count += 1;
                if self.err.is_none() {
                    self.err = Some(*e);
                }
            }
            _ => self.count += 1,
        }
    }
    pub(super) fn result(&self, f: PivotFunc) -> Value {
        let n = self.nums as f64;
        match f {
            PivotFunc::Count => return Value::number(self.count as f64),
            PivotFunc::CountNumbers => return Value::number(n),
            _ => {}
        }
        if let Some(e) = self.err {
            return Value::Error(e);
        }
        let div0 = Value::Error(CellError::Div0);
        match f {
            PivotFunc::Sum => Value::number(self.sum),
            PivotFunc::Average => {
                if self.nums == 0 {
                    div0
                } else {
                    Value::number(self.sum / n)
                }
            }
            PivotFunc::Max => Value::number(if self.nums == 0 { 0.0 } else { self.max }),
            PivotFunc::Min => Value::number(if self.nums == 0 { 0.0 } else { self.min }),
            PivotFunc::Product => Value::number(if self.nums == 0 { 0.0 } else { self.prod }),
            PivotFunc::StdDev | PivotFunc::Var => {
                if self.nums < 2 {
                    div0
                } else {
                    let var = (self.m2 / (n - 1.0)).max(0.0);
                    Value::number(if f == PivotFunc::Var { var } else { var.sqrt() })
                }
            }
            PivotFunc::StdDevP | PivotFunc::VarP => {
                if self.nums < 1 {
                    div0
                } else {
                    let var = (self.m2 / n).max(0.0);
                    Value::number(if f == PivotFunc::VarP { var } else { var.sqrt() })
                }
            }
            PivotFunc::Count | PivotFunc::CountNumbers => Value::Empty,
        }
    }
}

/// The words a PivotTable writes into its report: English, or Japanese as Japanese Excel
/// writes them (行ラベル, 総計, 東 集計, 合計 / 売上).
pub struct Captions {
    pub row_labels: &'static str,
    pub column_labels: &'static str,
    pub grand_total: &'static str,
    /// After an item's label in its subtotal (`East Total`, `東 集計`), and the header of the
    /// one data column when Σ Values is in Rows.
    pub total: &'static str,
    /// The Σ Values field's header in a tabular or outline layout.
    pub values: &'static str,
    /// Before a value field's name in its grand total row (`Total Sum of Sales`).
    pub total_of: &'static str,
    pub japanese: bool,
}

pub static CAPTIONS_EN: Captions = Captions {
    row_labels: "Row Labels",
    column_labels: "Column Labels",
    grand_total: "Grand Total",
    total: "Total",
    values: "Values",
    total_of: "Total",
    japanese: false,
};
pub static CAPTIONS_JA: Captions = Captions {
    row_labels: "行ラベル",
    column_labels: "列ラベル",
    grand_total: "総計",
    total: "集計",
    values: "値",
    total_of: "全体の",
    japanese: true,
};

/// The captions for a workbook in `region`.
pub fn captions(region: &gridcraft_core::Region) -> &'static Captions {
    if region.is_japanese() { &CAPTIONS_JA } else { &CAPTIONS_EN }
}

/// A value field's automatic name: `Sum of Sales`, or in Japanese `合計 / 売上`.
pub fn value_name(f: PivotFunc, field: &str, japanese: bool) -> String {
    if japanese { format!("{} / {field}", func_caption_ja(f)) } else { format!("{} of {field}", func_caption(f)) }
}

/// Japanese Excel's name for a summary function, as in a value field's name (`合計 / 売上`).
pub fn func_caption_ja(f: PivotFunc) -> &'static str {
    match f {
        PivotFunc::Sum => "合計",
        PivotFunc::Count => "データの個数",
        PivotFunc::Average => "平均",
        PivotFunc::Max => "最大",
        PivotFunc::Min => "最小",
        PivotFunc::Product => "積",
        PivotFunc::CountNumbers => "数値の個数",
        PivotFunc::StdDev => "標本標準偏差",
        PivotFunc::StdDevP => "標準偏差",
        PivotFunc::Var => "標本分散",
        PivotFunc::VarP => "分散",
    }
}

/// Excel's caption verb for a function ("Sum of Sales"). Count Numbers is captioned "Count of
/// Sales", as Excel names it and as the XLSX reader names a value field saved without a name.
pub fn func_caption(f: PivotFunc) -> &'static str {
    match f {
        PivotFunc::Sum => "Sum",
        PivotFunc::Count => "Count",
        PivotFunc::Average => "Average",
        PivotFunc::Max => "Max",
        PivotFunc::Min => "Min",
        PivotFunc::Product => "Product",
        PivotFunc::CountNumbers => "Count",
        PivotFunc::StdDev => "StdDev",
        PivotFunc::StdDevP => "StdDevp",
        PivotFunc::Var => "Var",
        PivotFunc::VarP => "Varp",
    }
}

// ---------------------------------------------------------------- trees

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub(super) item: u32,
    pub(super) children: Vec<Node>,
}

pub(super) fn build_tree(paths: &[&[u32]], depth: usize, fields: &[&PivotField], items: &[FieldItems]) -> Vec<Node> {
    let (Some(field), Some(fi)) = (fields.get(depth), items.get(depth)) else { return vec![] };
    let mut order: Vec<u32> = Vec::new();
    let mut groups: HashMap<u32, Vec<&[u32]>> = HashMap::new();
    for p in paths {
        if let Some(&it) = p.get(depth) {
            let g = groups.entry(it).or_default();
            if g.is_empty() {
                order.push(it);
            }
            g.push(p);
        }
    }
    let mut nodes: Vec<Node> = order
        .into_iter()
        .map(|it| {
            let ps = groups.remove(&it).unwrap_or_default();
            Node { item: it, children: build_tree(&ps, depth + 1, fields, items) }
        })
        .collect();
    let by = |a: &Node, b: &Node| match (fi.get(a.item), fi.get(b.item)) {
        (Some(x), Some(y)) => match field.sort {
            PivotSort::Asc => cmp_items(x, y),
            PivotSort::Desc => cmp_items(y, x),
            PivotSort::None => x.first.cmp(&y.first),
        },
        _ => Ordering::Equal,
    };
    nodes.sort_by(by);
    nodes
}

pub(super) fn is_collapsed(field: &PivotField, label: &str) -> bool {
    field.collapsed_items.iter().any(|c| c.eq_ignore_ascii_case(label))
}

pub(super) fn collect_siblings(nodes: &[Node], prefix: &mut Vec<u32>, out: &mut HashMap<Vec<u32>, Vec<u32>>) {
    out.insert(prefix.clone(), nodes.iter().map(|n| n.item).collect());
    for n in nodes {
        prefix.push(n.item);
        collect_siblings(&n.children, prefix, out);
        prefix.pop();
    }
}
