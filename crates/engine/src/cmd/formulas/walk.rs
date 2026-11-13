//! Evaluate Formula's steps, as Excel's dialog takes them: the formula with the part it evaluates
//! next underlined, each Evaluate replacing that part with its value, left to right and innermost
//! first, until only the result is left.

use gridcraft_core::{CellRef, Value};
use gridcraft_formula::{Expr, RefKind, SheetSel};
use gridcraft_model::Workbook;
use serde_json::{Value as Json, json};

/// Most steps one walk lists: a formula past it ends with its result evaluated in one step.
const MAX_STEPS: usize = 500;

/// Deepest nesting the walk steps into; a part nested deeper is evaluated as a whole.
const MAX_DEPTH: usize = 256;

/// Most values an evaluated array shows before the step keeps the expression.
const MAX_ARRAY: usize = 1_000;

/// Stands for the part being evaluated while the formula is printed, to find where it sits.
const MARK: &str = "\u{1}GRIDCRAFT_EVALUATE\u{1}";

/// Functions whose arguments are evaluated together with the call: LET and LAMBDA bind names
/// their parts read, which mean nothing outside the call.
const OPAQUE: &[&str] = &["LET", "LAMBDA"];

/// The walk of `expr`, a formula of cell `at` on sheet `sheet`: one entry per state the dialog
/// shows, each `{text, underline: [start, end] | null, stepIn: {sheet, cell} | null}` with the
/// underline in characters of `text`, and `stepIn` set when the underlined part is a cell that
/// holds a formula. The last entry is the result, with nothing underlined.
pub(super) fn walk(wb: &Workbook, sheet: usize, at: CellRef, expr: Expr) -> Vec<Json> {
    let mut tree = expr;
    let mut states = Vec::new();
    loop {
        let path = if states.len() + 1 >= MAX_STEPS { None } else { next(&tree, false, 0) };
        let path = match path {
            Some(p) => p,
            // Nothing inside is left to evaluate: the root itself, unless it's a value already.
            None if !is_value(&tree) => Vec::new(),
            None => {
                states.push(json!({"text": format!("={}", gridcraft_formula::print(&tree)), "underline": Json::Null, "stepIn": Json::Null}));
                return states;
            }
        };
        let target = std::mem::replace(node_mut(&mut tree, &path), Expr::Name(MARK.into()));
        let printed = gridcraft_formula::print(&tree);
        let part = gridcraft_formula::print(&target);
        let (before, after) = printed.split_once(MARK).unwrap_or((printed.as_str(), ""));
        let start = 1 + before.chars().count();
        let step_in = match &target {
            Expr::Ref(r) => single_cell(r).and_then(|c| {
                let si = match &r.sheet {
                    SheetSel::Current => Some(sheet),
                    SheetSel::Named(n) => wb.sheet_index(n),
                    SheetSel::Span(..) => None,
                }?;
                let sh = wb.sheet(si)?;
                sh.cell(c)?.formula.as_ref()?;
                Some(json!({"sheet": sh.name, "cell": c.a1()}))
            }),
            _ => None,
        };
        states.push(json!({
            "text": format!("={before}{part}{after}"),
            "underline": [start, start + part.chars().count()],
            "stepIn": step_in,
        }));
        let value = gridcraft_calc::recalc::evaluate_expr(wb, sheet, at, &target);
        *node_mut(&mut tree, &path) = literal(&value).unwrap_or(target);
        strip_parens(&mut tree);
        if path.is_empty() {
            states.push(json!({"text": format!("={}", gridcraft_formula::print(&tree)), "underline": Json::Null, "stepIn": Json::Null}));
            return states;
        }
    }
}

/// A value already: what a step leaves in place of the part it evaluated.
fn is_value(e: &Expr) -> bool {
    matches!(e, Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Array(_) | Expr::Missing)
}

fn single_cell(r: &gridcraft_formula::Reference) -> Option<CellRef> {
    match (&r.kind, &r.sheet) {
        (_, SheetSel::Span(..)) => None,
        (RefKind::Cell(a), _) => Some(a.cell()),
        _ => None,
    }
}

/// The path (child indexes) of the part to evaluate next: the leftmost, innermost part whose
/// own parts are all values. A cell reference is a step of its own, except straight inside a
/// function's parentheses, where the function reads the reference itself (ROW(A1), COUNTA(A1)
/// on an empty cell) and so evaluates with it. `None` when every part is a value, or a
/// reference or name the walk leaves for the part around it.
fn next(e: &Expr, call_arg: bool, depth: usize) -> Option<Vec<usize>> {
    if depth > MAX_DEPTH {
        return (!is_value(e)).then(Vec::new);
    }
    let within = |child: &Expr, i: usize, call_arg: bool| -> Option<Vec<usize>> {
        next(child, call_arg, depth + 1).map(|mut p| {
            p.insert(0, i);
            p
        })
    };
    match e {
        Expr::Ref(r) => (!call_arg && single_cell(r).is_some()).then(Vec::new),
        Expr::Paren(x) => within(x, 0, call_arg),
        Expr::Unary(_, x) => within(x, 0, false).or_else(|| done(x).then(Vec::new)),
        Expr::Binary(_, a, b) => within(a, 0, false).or_else(|| within(b, 1, false)).or_else(|| (done(a) && done(b)).then(Vec::new)),
        Expr::Call(name, args) => {
            if OPAQUE.contains(&name.as_str()) {
                return Some(Vec::new());
            }
            args.iter().enumerate().find_map(|(i, a)| within(a, i, true)).or_else(|| Some(Vec::new()))
        }
        Expr::Invoke(..) => Some(Vec::new()),
        _ => None,
    }
}

/// Nothing left to evaluate inside `e`: a value, a reference or a name the part around it reads.
fn done(e: &Expr) -> bool {
    match e {
        Expr::Paren(x) => done(x),
        Expr::Ref(_) | Expr::Name(_) | Expr::Struct(_) => true,
        e => is_value(e),
    }
}

fn node_mut<'a>(e: &'a mut Expr, path: &[usize]) -> &'a mut Expr {
    let Some((&i, rest)) = path.split_first() else { return e };
    let child = match e {
        Expr::Paren(x) | Expr::Unary(_, x) => x.as_mut(),
        Expr::Binary(_, a, b) => {
            if i == 0 {
                a.as_mut()
            } else {
                b.as_mut()
            }
        }
        Expr::Call(_, args) => &mut args[i],
        _ => return e,
    };
    node_mut(child, rest)
}

/// The value a step shows: a number, text, logical or error, an array of them; `None` for an
/// array too large to show, which stays as it was written.
fn literal(v: &Value) -> Option<Expr> {
    Some(match v {
        Value::Empty => Expr::Number(0.0),
        Value::Number(n) => Expr::Number(*n),
        Value::Text(t) => Expr::Text(t.clone()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Error(e) => Expr::Error(*e),
        Value::Array(a) => {
            if a.rows.saturating_mul(a.cols) > MAX_ARRAY {
                return None;
            }
            if a.rows == 1 && a.cols == 1 {
                return literal(&a.get(0, 0).cloned().unwrap_or_default());
            }
            let rows = (0..a.rows).map(|r| (0..a.cols).map(|c| literal(&a.get(r, c).cloned().unwrap_or_default())).collect::<Option<Vec<_>>>());
            Expr::Array(rows.collect::<Option<Vec<_>>>()?)
        }
    })
}

/// Drops the parentheses around a value, as Excel shows `(3)*2` as `3*2` once the inside is done.
fn strip_parens(e: &mut Expr) {
    let mut stack = vec![e];
    while let Some(e) = stack.pop() {
        if let Expr::Paren(x) = e
            && is_value(x)
        {
            let inner = std::mem::replace(x.as_mut(), Expr::Missing);
            *e = inner;
            continue;
        }
        match e {
            Expr::Paren(x) | Expr::Unary(_, x) => stack.push(x.as_mut()),
            Expr::Binary(_, a, b) => {
                stack.push(a.as_mut());
                stack.push(b.as_mut());
            }
            Expr::Call(_, args) => stack.extend(args.iter_mut()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    fn walk_of(s: &Session, formula: &str) -> Vec<(String, Option<(usize, usize)>)> {
        let d = s.doc().unwrap();
        let e = gridcraft_formula::parse(formula).unwrap();
        walk(&d.wb, 0, CellRef::new(9, 9), e)
            .into_iter()
            .map(|j| {
                let u = j["underline"].as_array().map(|u| (u[0].as_u64().unwrap() as usize, u[1].as_u64().unwrap() as usize));
                (j["text"].as_str().unwrap().to_string(), u)
            })
            .collect()
    }

    fn underlined(state: &(String, Option<(usize, usize)>)) -> String {
        let (s, e) = state.1.unwrap();
        state.0.chars().skip(s).take(e - s).collect()
    }

    #[test]
    fn steps_go_left_to_right_and_innermost_first() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [[2], [3], [4]]})).unwrap();
        let w = walk_of(&s, "ROUND(A1*A2/(A3+1),0)");
        let parts: Vec<String> = w.iter().filter(|st| st.1.is_some()).map(underlined).collect();
        assert_eq!(parts, ["A1", "A2", "2*3", "A3", "4+1", "6/5", "ROUND(1.2,0)"]);
        assert_eq!(w[0].0, "=ROUND(A1*A2/(A3+1),0)");
        assert_eq!(w[4].0, "=ROUND(6/(4+1),0)");
        assert_eq!(w[5].0, "=ROUND(6/5,0)", "the parentheses go once the inside is a value");
        assert_eq!(w.last().unwrap(), &("=1".to_string(), None));
    }

    #[test]
    fn a_reference_inside_a_function_is_read_with_the_function() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [[5], [6]]})).unwrap();
        let w = walk_of(&s, "SUM(A1:A2)+COUNTA(B1)");
        let parts: Vec<String> = w.iter().filter(|st| st.1.is_some()).map(underlined).collect();
        assert_eq!(parts, ["SUM(A1:A2)", "COUNTA(B1)", "11+0"]);
        assert_eq!(w.last().unwrap().0, "=11");
    }

    #[test]
    fn a_cell_holding_a_formula_can_be_stepped_into() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("cell.set", json!({"cell": "A1", "input": "=1+1"})).unwrap();
        s.execute("cell.set", json!({"cell": "A2", "input": "7"})).unwrap();
        let d = s.doc().unwrap();
        let w = walk(&d.wb, 0, CellRef::new(9, 9), gridcraft_formula::parse("A1*A2").unwrap());
        assert_eq!(w[0]["stepIn"], json!({"sheet": "Sheet1", "cell": "A1"}));
        assert_eq!(w[1]["stepIn"], Json::Null, "A2 is a constant");
        assert_eq!(w.last().unwrap()["text"], "=14");
    }
}
