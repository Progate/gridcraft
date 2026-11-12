use super::{
    Cell, CellRef, Deserialize, EngineError, HashMap, Json, Result, Serialize, Session, Sheet, StyleId, Trial, Value, Workbook, bad, bool_param,
    cell_list, edit, json, json_value, str_param, value_json,
};

// ---------------------------------------------------------------- scenarios

/// Hidden workbook-scope name that held the scenarios as a quoted JSON string before they were
/// saved as Excel's own `<scenarios>` element. Still read, so a workbook saved that way keeps its
/// scenarios; the next change moves them to their sheets and drops the name.
pub(crate) const SCENARIO_NAME: &str = "_sc_scenarios";

const MAX_SCENARIO_CELLS: usize = 32;

const MAX_SCENARIOS: usize = 1000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Scenario {
    pub name: String,
    pub sheet: String,
    pub changing: String,
    pub values: Vec<Json>,
    #[serde(default)]
    pub comment: String,
    /// Prevent changes (Excel's default for a new scenario).
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub user: String,
}

/// Every scenario in the workbook, sheet by sheet: those in the sheets' `<scenarios>`, then any
/// in the legacy hidden name for a sheet that has none of its own.
pub(crate) fn load_scenarios(wb: &Workbook) -> Vec<Scenario> {
    let mut out = Vec::new();
    for sh in &wb.sheets {
        for d in sh.scenarios.iter().flat_map(|x| x.list.iter()) {
            let inputs: Vec<&gridcraft_model::ScenarioInput> = d.inputs.iter().filter(|i| !i.deleted).collect();
            out.push(Scenario {
                name: d.name.clone(),
                sheet: sh.name.clone(),
                changing: inputs.iter().map(|i| i.cell.a1()).collect::<Vec<_>>().join(","),
                values: inputs.iter().map(|i| val_json(&i.value)).collect(),
                comment: d.comment.clone(),
                locked: d.locked,
                hidden: d.hidden,
                user: d.user.clone(),
            });
        }
    }
    let has_own = |sheet: &str| wb.sheet_index(sheet).and_then(|i| wb.sheet(i)).is_some_and(|sh| sh.scenarios.is_some());
    out.extend(legacy_scenarios(wb).into_iter().filter(|sc| !has_own(&sc.sheet)));
    out
}

fn legacy_scenarios(wb: &Workbook) -> Vec<Scenario> {
    let Some(n) = wb.names.iter().find(|n| n.scope.is_none() && n.name.eq_ignore_ascii_case(SCENARIO_NAME)) else { return vec![] };
    serde_json::from_str(&string_literals(&n.formula)).unwrap_or_default()
}

/// A scenario value as Excel saves it in `inputCells val`: numbers in invariant notation, text as
/// typed, booleans as `TRUE`/`FALSE`.
fn json_val(v: &Json) -> String {
    match v {
        Json::Null => String::new(),
        Json::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        Json::Number(n) => n.as_f64().map(|f| f.to_string()).unwrap_or_else(|| n.to_string()),
        Json::String(t) => t.clone(),
        other => other.to_string(),
    }
}

/// The value of an `inputCells val`: a number when it reads as one, else its text (which Show
/// enters as typed input).
fn val_json(v: &str) -> Json {
    match v.trim().parse::<f64>() {
        Ok(n) if n.is_finite() && !v.trim().is_empty() => json!(n),
        _ => json!(v),
    }
}

/// Concatenates the string literals of a `"a"&"b""c"` formula.
fn string_literals(f: &str) -> String {
    let mut out = String::new();
    let mut it = f.chars().peekable();
    let mut in_str = false;
    while let Some(c) = it.next() {
        if !in_str {
            if c == '"' {
                in_str = true;
            }
            continue;
        }
        if c == '"' {
            if it.peek() == Some(&'"') {
                it.next();
                out.push('"');
            } else {
                in_str = false;
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Saves `list` as each sheet's `<scenarios>` (Excel's own form, so Excel's Scenario Manager
/// lists them) and drops the legacy hidden name. A sheet keeps its shown and selected scenario
/// while that index still exists.
fn store_scenarios(wb: &mut Workbook, list: &[Scenario]) {
    wb.names.retain(|n| !(n.scope.is_none() && n.name.eq_ignore_ascii_case(SCENARIO_NAME)));
    for si in 0..wb.sheets.len() {
        let Some(sh) = wb.sheet(si) else { continue };
        let mine: Vec<&Scenario> = list.iter().filter(|x| x.sheet.eq_ignore_ascii_case(&sh.name)).collect();
        let set = (!mine.is_empty()).then(|| {
            let defs: Vec<gridcraft_model::ScenarioDef> = mine
                .iter()
                .map(|x| gridcraft_model::ScenarioDef {
                    name: x.name.clone(),
                    locked: x.locked,
                    hidden: x.hidden,
                    user: x.user.clone(),
                    comment: x.comment.clone(),
                    inputs: x
                        .changing
                        .split(',')
                        .filter_map(|c| CellRef::parse(c.trim()))
                        .zip(x.values.iter())
                        .map(|(cell, v)| gridcraft_model::ScenarioInput { cell, value: json_val(v), ..Default::default() })
                        .collect(),
                })
                .collect();
            let mut cells: Vec<CellRef> = Vec::new();
            for c in defs.iter().flat_map(|d| d.inputs.iter().map(|i| i.cell)) {
                if !cells.contains(&c) {
                    cells.push(c);
                }
            }
            let n = defs.len() as u32;
            let old = sh.scenarios.as_ref();
            gridcraft_model::Scenarios {
                current: old.and_then(|o| o.current).filter(|i| *i < n),
                show: old.and_then(|o| o.show).filter(|i| *i < n),
                sqref: cells.into_iter().map(gridcraft_core::RangeRef::cell).collect(),
                list: defs,
            }
        });
        if sh.scenarios != set
            && let Some(sh) = wb.sheet_mut(si)
        {
            sh.scenarios = set;
        }
    }
}

/// Records which scenario was shown last on its sheet (`<scenarios current>`).
fn mark_current(wb: &mut Workbook, sheet: &str, name: &str) {
    let Some(si) = wb.sheet_index(sheet) else { return };
    let Some(sh) = wb.sheet_mut(si) else { return };
    if let Some(set) = sh.scenarios.as_mut()
        && let Some(i) = set.list.iter().position(|d| d.name.eq_ignore_ascii_case(name))
    {
        set.current = Some(i as u32);
        set.show = Some(i as u32);
    }
}

fn scenario_json(sc: &Scenario) -> Json {
    json!({
        "name": sc.name,
        "sheet": sc.sheet,
        "changing": sc.changing,
        "values": sc.values,
        "comment": sc.comment,
        "locked": sc.locked,
        "hidden": sc.hidden,
        "user": sc.user,
    })
}

pub(super) fn scenario_list(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let sheet = str_param(p, "sheet").map(str::to_string).unwrap_or_else(|| d.wb.active().map(|s| s.name.clone()).unwrap_or_default());
    let list: Vec<Json> =
        load_scenarios(&d.wb).iter().filter(|sc| sheet == "*" || sc.sheet.eq_ignore_ascii_case(&sheet)).map(scenario_json).collect();
    Ok(json!({"scenarios": list}))
}

pub(super) fn scenario_add(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "data.scenarioAdd";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?.to_string();
    if name.chars().count() > 255 {
        return Err(bad(CMD, "the name is too long (255 characters max)"));
    }
    let changing = str_param(p, "changing").ok_or_else(|| bad(CMD, "missing `changing`"))?.to_string();
    let d = s.doc()?;
    let (si, cells) = cell_list(&d.wb, &changing, CMD, MAX_SCENARIO_CELLS)?;
    if cells.is_empty() {
        return Err(bad(CMD, "no changing cells"));
    }
    let sh = d.wb.sheet(si).ok_or(EngineError::NoDocument)?;
    let values: Vec<Json> = match p.get("values").and_then(Json::as_array) {
        Some(a) => {
            if a.len() != cells.len() {
                return Err(bad(CMD, format!("{} values for {} changing cells", a.len(), cells.len())));
            }
            a.clone()
        }
        None => cells.iter().map(|c| value_json(&sh.value(*c))).collect(),
    };
    let changing = cells.iter().map(|c| c.a1()).collect::<Vec<_>>().join(",");
    let replace = bool_param(p, "replace").unwrap_or(false);
    let mut list = load_scenarios(&d.wb);
    let sc = Scenario {
        name: name.clone(),
        sheet: sh.name.clone(),
        changing,
        values,
        comment: str_param(p, "comment").unwrap_or("").to_string(),
        // Excel ticks Prevent changes for a new scenario.
        locked: bool_param(p, "locked").unwrap_or(true),
        hidden: bool_param(p, "hidden").unwrap_or(false),
        user: s.prefs.user_name.clone(),
    };
    // Editing a scenario under a new name (the Edit Scenario dialog) replaces the one named `old`.
    let renamed_from = str_param(p, "old")
        .filter(|o| !o.eq_ignore_ascii_case(&name))
        .and_then(|o| list.iter().position(|x| x.name.eq_ignore_ascii_case(o) && x.sheet.eq_ignore_ascii_case(&sc.sheet)));
    let existing = list.iter().position(|x| x.name.eq_ignore_ascii_case(&name) && x.sheet.eq_ignore_ascii_case(&sc.sheet));
    match existing.or(renamed_from) {
        Some(i) if replace || existing.is_none() => {
            if let Some(slot) = list.get_mut(i) {
                *slot = sc.clone();
            }
        }
        Some(_) => return Err(EngineError::Other(format!("A scenario named '{name}' already exists on this sheet."))),
        None => {
            if list.len() >= MAX_SCENARIOS {
                return Err(bad(CMD, "too many scenarios"));
            }
            list.push(sc.clone());
        }
    }
    edit(s, |cx| {
        store_scenarios(&mut cx.wb, &list);
        Ok(())
    })?;
    Ok(scenario_json(&sc))
}

fn find_scenario(wb: &Workbook, p: &Json, cmd: &str) -> Result<(Vec<Scenario>, usize)> {
    let name = str_param(p, "name").ok_or_else(|| bad(cmd, "missing `name`"))?;
    let list = load_scenarios(wb);
    let active = wb.active().map(|s| s.name.clone()).unwrap_or_default();
    let i = list
        .iter()
        .position(|x| x.name.eq_ignore_ascii_case(name) && x.sheet.eq_ignore_ascii_case(&active))
        .or_else(|| list.iter().position(|x| x.name.eq_ignore_ascii_case(name)))
        .ok_or_else(|| bad(cmd, format!("no scenario named `{name}`")))?;
    Ok((list, i))
}

pub(super) fn scenario_show(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "data.scenarioShow";
    let d = s.doc()?;
    let (list, i) = find_scenario(&d.wb, p, CMD)?;
    let sc = list.get(i).cloned().ok_or_else(|| bad(CMD, "no such scenario"))?;
    let si = d.wb.sheet_index(&sc.sheet).ok_or_else(|| bad(CMD, format!("the scenario's sheet `{}` no longer exists", sc.sheet)))?;
    let (_, cells) = cell_list(&d.wb, &sc.changing, CMD, MAX_SCENARIO_CELLS)?;
    edit(s, |cx| {
        for (c, v) in cells.iter().zip(sc.values.iter()) {
            // Stored numbers are values, not typed text: entering them through the parser would
            // apply percent entry (9 -> 9%) to a Percentage-formatted cell.
            let cell = match v.as_f64() {
                Some(n) => {
                    let style = cx.wb.sheet(si).map(|sh| sh.style_id(*c)).unwrap_or_default();
                    Some(Cell { value: Value::Number(n), formula: None, style, py: None })
                }
                None => crate::cmd::edit::input_to_cell(&crate::cmd::edit::json_to_input(v), si, *c, &mut cx.wb)?,
            };
            let sh = cx.sheet_mut(si)?;
            match cell {
                Some(cell) => sh.set_cell(*c, cell),
                None => {
                    sh.remove_cell(*c);
                }
            }
            cx.touch(si, *c);
        }
        // A scenario from the legacy name is moved to its sheet first, so it can be marked.
        if cx.wb.sheet(si).is_some_and(|sh| sh.scenarios.is_none()) {
            store_scenarios(&mut cx.wb, &list);
        }
        mark_current(&mut cx.wb, &sc.sheet, &sc.name);
        Ok(())
    })?;
    Ok(scenario_json(&sc))
}

pub(super) fn scenario_delete(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let (mut list, i) = find_scenario(&d.wb, p, "data.scenarioDelete")?;
    let removed = list.remove(i);
    edit(s, |cx| {
        store_scenarios(&mut cx.wb, &list);
        Ok(())
    })?;
    Ok(json!({"deleted": removed.name}))
}

/// The words a Scenario Summary sheet is written with: Excel writes the report in its interface
/// language, so the caller passes that language (`language: "ja"`).
struct SummaryLabels {
    sheet: &'static str,
    title: &'static str,
    current: &'static str,
    changing: &'static str,
    results: &'static str,
    note: &'static str,
}

impl SummaryLabels {
    fn for_language(language: Option<&str>) -> SummaryLabels {
        match language.map(|l| l.split(['-', '_']).next().unwrap_or(l).to_ascii_lowercase()).as_deref() {
            Some("ja") => SummaryLabels {
                sheet: "シナリオ情報",
                title: "シナリオの情報",
                current: "現在の値:",
                changing: "変更セル:",
                results: "結果セル:",
                note: "注: 現在の値 列は、シナリオ情報レポートが作成された時点での変更セルの値を表します。",
            },
            _ => SummaryLabels {
                sheet: "Scenario Summary",
                title: "Scenario Summary",
                current: "Current Values:",
                changing: "Changing Cells:",
                results: "Result Cells:",
                note: "Notes: Current Values column represents values of changing cells at time Scenario Summary Report was created.",
            },
        }
    }
}

/// Most changing-cell-by-scenario cells one Scenario Summary writes.
const MAX_SUMMARY_CELLS: usize = 1_000_000;

pub(super) fn scenario_summary(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "data.scenarioSummary";
    let d = s.doc()?;
    let si = d.wb.active_sheet();
    let sh = d.wb.sheet(si).ok_or(EngineError::NoDocument)?;
    let scenarios: Vec<Scenario> = load_scenarios(&d.wb).into_iter().filter(|x| x.sheet.eq_ignore_ascii_case(&sh.name)).collect();
    if scenarios.is_empty() {
        return Err(EngineError::Other("There are no scenarios on this sheet.".into()));
    }
    let results: Vec<CellRef> = match str_param(p, "resultCells").map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => {
            let (rsi, cells) = cell_list(&d.wb, t, CMD, 255)?;
            if rsi != si {
                return Err(bad(CMD, "result cells must be on the scenarios' sheet"));
            }
            cells
        }
        None => vec![],
    };
    // Changing cells of every scenario, first-seen order; per scenario its values.
    let mut changing: Vec<CellRef> = Vec::new();
    let mut per: Vec<HashMap<CellRef, Value>> = Vec::new();
    let mut seen: std::collections::HashSet<CellRef> = Default::default();
    for sc in &scenarios {
        let (_, cells) = cell_list(&d.wb, &sc.changing, CMD, MAX_SCENARIO_CELLS)?;
        let mut m = HashMap::new();
        for (c, v) in cells.iter().zip(sc.values.iter()) {
            if seen.insert(*c) {
                changing.push(*c);
            }
            m.insert(*c, json_value(v, &d.wb));
        }
        per.push(m);
    }
    // The summary has a cell for every changing cell in every scenario, filled from the current
    // value where a scenario doesn't set it, so small disjoint scenarios multiply.
    // Each result cell is a row of the summary too, with a cell per scenario.
    if changing.len().saturating_add(results.len()).saturating_mul(per.len()) > MAX_SUMMARY_CELLS {
        return Err(bad(CMD, "the scenarios together change too many cells for a summary"));
    }
    let current: Vec<Value> = changing.iter().map(|c| sh.value(*c)).collect();
    let current_results: Vec<Value> = results.iter().map(|c| sh.value(*c)).collect();
    let mut trial = Trial::new(d);
    let reads: Vec<(usize, CellRef)> = results.iter().map(|c| (si, *c)).collect();
    let scenario_results: Vec<Vec<Value>> = per
        .iter()
        .map(|m| {
            let sets: Vec<(usize, CellRef, Value)> = m.iter().map(|(c, v)| (si, *c, v.clone())).collect();
            trial.run(&sets, &reads)
        })
        .collect();
    let fmt_of = |c: CellRef| d.wb.styles.get(sh.style_id(c)).num_fmt.clone();
    let changing_fmts: Vec<_> = changing.iter().map(|c| fmt_of(*c)).collect();
    let result_fmts: Vec<_> = results.iter().map(|c| fmt_of(*c)).collect();
    let labels = SummaryLabels::for_language(str_param(p, "language"));
    let name = (1..)
        .map(|n| if n == 1 { labels.sheet.to_string() } else { format!("{} {n}", labels.sheet) })
        .find(|n| d.wb.sheet_index(n).is_none())
        .unwrap_or_else(|| labels.sheet.into());
    let names: Vec<String> = scenarios.iter().map(|x| x.name.clone()).collect();
    let new_index = edit(s, |cx| {
        let bold = cx.wb.styles.derive(StyleId::DEFAULT, |st| st.font.bold = true);
        let mut sheet = Sheet::new(name.clone());
        let put = |sheet: &mut Sheet, row: u32, col: u32, v: Value, style: StyleId| {
            sheet.set_cell(CellRef::new(row, col), Cell { value: v, formula: None, style, py: None });
        };
        put(&mut sheet, 1, 1, Value::text(labels.title), bold);
        put(&mut sheet, 2, 2, Value::text(labels.current), bold);
        for (k, n) in names.iter().enumerate() {
            put(&mut sheet, 2, 3 + k as u32, Value::text(n.as_str()), bold);
        }
        put(&mut sheet, 3, 1, Value::text(labels.changing), bold);
        let mut row = 4u32;
        for (i, c) in changing.iter().enumerate() {
            let fmt = changing_fmts.get(i).cloned().unwrap_or_default();
            let st = cx.wb.styles.derive(StyleId::DEFAULT, |st| st.num_fmt = fmt);
            put(&mut sheet, row, 1, Value::text(format!("${}${}", gridcraft_core::col_to_letters(c.col), c.row + 1)), StyleId::DEFAULT);
            let cur = current.get(i).cloned().unwrap_or_default();
            put(&mut sheet, row, 2, cur.clone(), st);
            for (k, m) in per.iter().enumerate() {
                put(&mut sheet, row, 3 + k as u32, m.get(c).cloned().unwrap_or_else(|| cur.clone()), st);
            }
            row += 1;
        }
        if !results.is_empty() {
            put(&mut sheet, row, 1, Value::text(labels.results), bold);
            row += 1;
            for (i, c) in results.iter().enumerate() {
                let fmt = result_fmts.get(i).cloned().unwrap_or_default();
                let st = cx.wb.styles.derive(StyleId::DEFAULT, |st| st.num_fmt = fmt);
                put(&mut sheet, row, 1, Value::text(format!("${}${}", gridcraft_core::col_to_letters(c.col), c.row + 1)), StyleId::DEFAULT);
                put(&mut sheet, row, 2, current_results.get(i).cloned().unwrap_or_default(), st);
                for (k, r) in scenario_results.iter().enumerate() {
                    put(&mut sheet, row, 3 + k as u32, r.get(i).cloned().unwrap_or_default(), st);
                }
                row += 1;
            }
        }
        put(&mut sheet, row + 1, 1, Value::text(labels.note), StyleId::DEFAULT);
        sheet.cols.insert(1, gridcraft_model::LineInfo { size: Some(110.0), ..Default::default() });
        sheet.cols.insert(2, gridcraft_model::LineInfo { size: Some(100.0), ..Default::default() });
        cx.wb.sheets.push(std::sync::Arc::new(sheet));
        cx.structural = true;
        Ok(cx.wb.sheets.len() - 1)
    })?;
    s.doc_mut()?.wb_switch_sheet(new_index);
    Ok(json!({"sheet": name, "scenarios": names.len(), "changingCells": changing.len(), "resultCells": results.len()}))
}
