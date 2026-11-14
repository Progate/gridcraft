//! Label Filters and Value Filters of the row and column fields (the ▼ of 行ラベル / 列ラベル ›
//! ラベル フィルター / 値フィルター).
//!
//! A Label Filter keeps the items whose caption meets its condition, record by record. A Value
//! Filter needs the items' totals first: [`value_exclusions`] sums the value field per item (within
//! its parent items, over the whole column axis, as Excel compares an item's grand total) before
//! the report is aggregated, and the report then leaves out the records of the items it drops.

use std::collections::{HashMap, HashSet};

use gridcraft_core::Collation;
use gridcraft_model::{PivotField, PivotLabelFilter, PivotLabelOp, PivotTopBy, PivotValueFilter, PivotValueOp};

use super::items::{Acc, Grouping, item_label};
use super::output::ValSrc;
use super::{PivotValue, Source, Value, Workbook};

/// Whether an item's caption (and value, for numeric items) meets a Label Filter. Text compares
/// without case; Equals, Begins With, Ends With and Contains take `*` and `?` wildcards; Greater
/// Than and the rest compare numbers as numbers and text in the workbook's collation.
pub fn label_matches(f: &PivotLabelFilter, label: &str, value: &Value, coll: Collation) -> bool {
    let low = label.to_lowercase();
    let pat = f.value.to_lowercase();
    let wild = |p: &str| crate::cmd::edit::find::wildcard(&low, p);
    let cmp = |other: &str| -> std::cmp::Ordering {
        match (value, other.trim().parse::<f64>()) {
            (Value::Number(n), Ok(x)) => n.total_cmp(&x),
            _ => coll.compare_text(label, other),
        }
    };
    use std::cmp::Ordering::{Greater, Less};
    match f.op {
        PivotLabelOp::Equal => wild(&pat),
        PivotLabelOp::NotEqual => !wild(&pat),
        PivotLabelOp::BeginsWith => wild(&format!("{pat}*")),
        PivotLabelOp::NotBeginsWith => !wild(&format!("{pat}*")),
        PivotLabelOp::EndsWith => wild(&format!("*{pat}")),
        PivotLabelOp::NotEndsWith => !wild(&format!("*{pat}")),
        PivotLabelOp::Contains => wild(&format!("*{pat}*")),
        PivotLabelOp::NotContains => !wild(&format!("*{pat}*")),
        PivotLabelOp::GreaterThan => cmp(&f.value) == Greater,
        PivotLabelOp::GreaterThanOrEqual => cmp(&f.value) != Less,
        PivotLabelOp::LessThan => cmp(&f.value) == Less,
        PivotLabelOp::LessThanOrEqual => cmp(&f.value) != Greater,
        PivotLabelOp::Between => cmp(&f.value) != Less && cmp(&f.value2) != Greater,
        PivotLabelOp::NotBetween => !(cmp(&f.value) != Less && cmp(&f.value2) != Greater),
    }
}

/// Whether a total meets a Value Filter's comparison (Top 10 is decided among siblings instead).
fn value_matches(f: &PivotValueFilter, v: f64) -> bool {
    let (a, b) = (f.value, f.value2);
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    match f.op {
        PivotValueOp::Equal => v == a,
        PivotValueOp::NotEqual => v != a,
        PivotValueOp::GreaterThan => v > a,
        PivotValueOp::GreaterThanOrEqual => v >= a,
        PivotValueOp::LessThan => v < a,
        PivotValueOp::LessThanOrEqual => v <= a,
        PivotValueOp::Between => v >= lo && v <= hi,
        PivotValueOp::NotBetween => v < lo || v > hi,
        PivotValueOp::Top10 => true,
    }
}

/// The items a Top 10 filter keeps among siblings `(key, total)`.
fn top_kept(f: &PivotValueFilter, mut sibs: Vec<(Vec<String>, f64)>) -> Vec<Vec<String>> {
    sibs.sort_by(|x, y| if f.bottom { x.1.total_cmp(&y.1) } else { y.1.total_cmp(&x.1) });
    let n = f.value.max(0.0);
    match f.top_by {
        PivotTopBy::Items => sibs.into_iter().take(n.floor() as usize).map(|(k, _)| k).collect(),
        PivotTopBy::Percent | PivotTopBy::Sum => {
            let total: f64 = sibs.iter().map(|(_, v)| v).sum();
            let goal = if f.top_by == PivotTopBy::Percent { total * n / 100.0 } else { n };
            let mut acc = 0.0;
            let mut kept = Vec::new();
            for (k, v) in sibs {
                if acc >= goal && !kept.is_empty() {
                    break;
                }
                acc += v;
                kept.push(k);
            }
            kept
        }
    }
}

/// Items a Value Filter drops: on Rows (`rows`) or Columns, at field `level`, the lower-cased
/// captions of the item and its parents.
pub struct Exclusion {
    pub rows: bool,
    pub level: usize,
    pub dropped: HashSet<Vec<String>>,
}

/// What [`value_exclusions`] reads of the report being computed.
pub struct ExclusionInput<'a> {
    pub wb: &'a Workbook,
    pub src: &'a Source,
    pub rows: &'a [(usize, &'a PivotField)],
    pub cols: &'a [(usize, &'a PivotField)],
    pub row_groups: &'a [Grouping],
    pub col_groups: &'a [Grouping],
    pub vals: &'a [(ValSrc, &'a PivotValue)],
}

/// Every Value Filter's dropped items. `keep` says whether a record gets past the report filters
/// and the Label Filters.
pub fn value_exclusions(input: &ExclusionInput<'_>, keep: &dyn Fn(&[Value]) -> bool) -> Vec<Exclusion> {
    let fmt_of = |c: usize| input.src.formats.get(c).cloned().unwrap_or_else(|| "General".into());
    let mut out = Vec::new();
    for rows in [true, false] {
        let (axis, groups) = if rows { (input.rows, input.row_groups) } else { (input.cols, input.col_groups) };
        for (level, (_, field)) in axis.iter().enumerate() {
            let Some(vf) = &field.value_filter else { continue };
            // A calculated field has no per-record numbers to total; its filter is left out.
            let Some((ValSrc::Col(vc), value)) = input.vals.iter().find(|(_, v)| v.name.eq_ignore_ascii_case(&vf.value_field)) else {
                continue;
            };
            let mut totals: HashMap<Vec<String>, Acc> = HashMap::new();
            for rec in &input.src.rows {
                if !keep(rec) {
                    continue;
                }
                let key: Vec<String> = axis
                    .iter()
                    .zip(groups)
                    .take(level + 1)
                    .map(|((c, _), g)| item_label(input.wb, rec.get(*c).unwrap_or(&Value::Empty), *g, &fmt_of(*c)).to_lowercase())
                    .collect();
                totals.entry(key).or_default().add(rec.get(*vc).unwrap_or(&Value::Empty));
            }
            let totals: Vec<(Vec<String>, f64)> = totals.into_iter().map(|(k, acc)| (k, acc.result(value.func).as_f64().unwrap_or(0.0))).collect();
            let dropped: HashSet<Vec<String>> = if vf.op == PivotValueOp::Top10 {
                let mut by_parent: HashMap<Vec<String>, Vec<(Vec<String>, f64)>> = HashMap::new();
                for (k, v) in &totals {
                    by_parent.entry(k[..level].to_vec()).or_default().push((k.clone(), *v));
                }
                let kept: HashSet<Vec<String>> = by_parent.into_values().flat_map(|s| top_kept(vf, s)).collect();
                totals.into_iter().map(|(k, _)| k).filter(|k| !kept.contains(k)).collect()
            } else {
                totals.into_iter().filter(|(_, v)| !value_matches(vf, *v)).map(|(k, _)| k).collect()
            };
            if !dropped.is_empty() {
                out.push(Exclusion { rows, level, dropped });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lf(op: PivotLabelOp, v: &str) -> PivotLabelFilter {
        PivotLabelFilter { op, value: v.into(), value2: String::new() }
    }

    #[test]
    fn label_conditions_match_without_case_and_with_wildcards() {
        let t = |op, v: &str, label: &str| label_matches(&lf(op, v), label, &Value::text(label), Collation::Japanese);
        assert!(t(PivotLabelOp::Equal, "東", "東"));
        assert!(t(PivotLabelOp::Equal, "e*", "East"));
        assert!(!t(PivotLabelOp::NotEqual, "east", "East"));
        assert!(t(PivotLabelOp::BeginsWith, "カレ", "カレーパン"));
        assert!(t(PivotLabelOp::EndsWith, "パン", "メロンパン"));
        assert!(!t(PivotLabelOp::NotContains, "ロン", "メロンパン"));
        assert!(t(PivotLabelOp::GreaterThan, "B", "c"));
        let between = PivotLabelFilter { op: PivotLabelOp::Between, value: "b".into(), value2: "d".into() };
        assert!(label_matches(&between, "c", &Value::text("c"), Collation::Default));
        assert!(!label_matches(&between, "e", &Value::text("e"), Collation::Default));
        // Numbers compare as numbers: 10 is greater than 9.
        assert!(label_matches(&lf(PivotLabelOp::GreaterThan, "9"), "10", &Value::number(10.0), Collation::Default));
    }

    #[test]
    fn top_ten_keeps_items_percent_or_sum() {
        let s = |v: &[f64]| v.iter().enumerate().map(|(i, x)| (vec![i.to_string()], *x)).collect::<Vec<_>>();
        let f = PivotValueFilter { op: PivotValueOp::Top10, value: 2.0, ..PivotValueFilter::default() };
        assert_eq!(top_kept(&f, s(&[5.0, 9.0, 1.0, 7.0])), vec![vec!["1".to_string()], vec!["3".to_string()]]);
        let b = PivotValueFilter { bottom: true, ..f.clone() };
        assert_eq!(top_kept(&b, s(&[5.0, 9.0, 1.0, 7.0])), vec![vec!["2".to_string()], vec!["0".to_string()]]);
        let p = PivotValueFilter { top_by: PivotTopBy::Percent, value: 50.0, ..f.clone() };
        assert_eq!(top_kept(&p, s(&[10.0, 60.0, 30.0])).len(), 1);
        let sum = PivotValueFilter { top_by: PivotTopBy::Sum, value: 80.0, ..f };
        assert_eq!(top_kept(&sum, s(&[10.0, 60.0, 30.0])).len(), 2);
    }
}
