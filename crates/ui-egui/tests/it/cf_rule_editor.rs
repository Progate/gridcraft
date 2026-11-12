//! New Formatting Rule with every rule type, the quick rule dialogs with Custom Format…, and the
//! conditional Format Cells handing its format back, as in Excel.

use egui_kittest::kittest::Queryable;
use gridcraft_engine::model::{CfRule, CfValueKind, Color};
use gridcraft_ui_egui::SheetApp;
use gridcraft_ui_egui::control::{Outcome, handle};
use serde_json::{Value as Json, json};

type Harness = egui_kittest::Harness<'static, SheetApp>;

fn harness() -> Harness {
    let mut h = crate::support::blank_harness(6);
    h.state_mut().run("selection.set", json!({"range": "B2:B9"})).unwrap();
    h
}

fn control(h: &mut Harness, method: &str, params: Json) -> Json {
    let ctx = h.ctx.clone();
    match handle(h.state_mut(), &ctx, method, &params) {
        Outcome::Done(v) => v,
        Outcome::Screenshot { .. } => json!(null),
    }
}

fn set(h: &mut Harness, field: &str, value: Json) {
    control(h, "ui.dialog.set", json!({"field": field, "value": value}));
}

fn rules(h: &Harness) -> Vec<CfRule> {
    h.state().session.active().unwrap().wb.active().unwrap().cond_formats.iter().map(|c| c.rule.clone()).collect()
}

/// New Formatting Rule with `fields` set, confirmed; the rule it added.
fn new_rule(fields: &[(&str, Json)]) -> CfRule {
    let mut h = harness();
    control(&mut h, "ui.dialog", json!({"name": "newFormattingRule"}));
    for (k, v) in fields {
        set(&mut h, k, v.clone());
    }
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], json!(true), "{r}");
    let sh = h.state().session.active().unwrap().wb.active().unwrap();
    assert_eq!(sh.cond_formats[0].ranges[0].a1(), "B2:B9");
    rules(&h).remove(0)
}

#[test]
fn the_value_based_rule_types_of_new_formatting_rule() {
    let r = new_rule(&[("formatStyle", json!("3color"))]);
    assert!(matches!(&r, CfRule::ColorScale { stops } if stops.len() == 3 && stops[1].0 == CfValueKind::Percentile(50.0)), "{r:?}");
    let r = new_rule(&[
        ("formatStyle", json!("2color")),
        ("minType", json!("num")),
        ("minValue", json!("10")),
        ("maxType", json!("percent")),
        ("maxValue", json!("90")),
    ]);
    assert!(
        matches!(&r, CfRule::ColorScale { stops } if stops[0].0 == CfValueKind::Number(10.0) && stops[1].0 == CfValueKind::Percent(90.0)),
        "{r:?}"
    );
    let r = new_rule(&[("formatStyle", json!("dataBar")), ("barFill", json!("solid")), ("barOnly", json!(true)), ("barColor", json!("#FF0000"))]);
    assert!(matches!(&r, CfRule::DataBar { gradient: false, show_value: false, color, .. } if *color == Color::rgb(255, 0, 0)), "{r:?}");
    let r = new_rule(&[
        ("formatStyle", json!("iconSet")),
        ("iconSet", json!("3Arrows")),
        ("iconValues", json!(["80", "40"])),
        ("iconTypes", json!(["num", "percent"])),
    ]);
    assert!(
        matches!(&r, CfRule::IconSet { set, thresholds, .. } if set == "3Arrows" && thresholds[1] == CfValueKind::Percent(40.0) && thresholds[2] == CfValueKind::Number(80.0)),
        "{r:?}"
    );
}

#[test]
fn the_other_rule_types_of_new_formatting_rule() {
    let r = new_rule(&[("ruleType", json!("contain")), ("operator", json!("greaterThan")), ("value", json!("5000"))]);
    assert!(matches!(&r, CfRule::CellIs { a, .. } if a == "5000"), "{r:?}");
    let r =
        new_rule(&[("ruleType", json!("contain")), ("containKind", json!("specificText")), ("textOp", json!("beginsWith")), ("text", json!("東"))]);
    assert!(matches!(&r, CfRule::BeginsWith { text, .. } if text == "東"), "{r:?}");
    let r = new_rule(&[("ruleType", json!("contain")), ("containKind", json!("dates")), ("period", json!("lastWeek"))]);
    assert!(matches!(&r, CfRule::TimePeriod { period, .. } if period == "lastWeek"), "{r:?}");
    let r = new_rule(&[("ruleType", json!("contain")), ("containKind", json!("noErrors"))]);
    assert!(matches!(r, CfRule::NoErrors { .. }), "{r:?}");
    let r = new_rule(&[("ruleType", json!("rank")), ("topBottom", json!("bottom")), ("rank", json!(3)), ("percent", json!(true))]);
    assert!(matches!(r, CfRule::Top10 { bottom: true, percent: true, rank: 3, .. }), "{r:?}");
    let r = new_rule(&[("ruleType", json!("average")), ("avg", json!("equalBelow"))]);
    assert!(matches!(r, CfRule::AboveAverage { below: true, equal: true, std_dev: 0, .. }), "{r:?}");
    let r = new_rule(&[("ruleType", json!("unique")), ("dupUnique", json!("unique"))]);
    assert!(matches!(r, CfRule::Unique { .. }), "{r:?}");
    let style = json!({"font": {"name": "Calibri", "size": 11.0, "bold": true, "italic": false, "underline": "None", "strike": false, "color": {"Rgb": 0x9C0006}, "vert": "Baseline"}});
    let r = new_rule(&[("ruleType", json!("formula")), ("formula", json!("=$C2>100")), ("style", style)]);
    assert!(matches!(&r, CfRule::Expression { formula, style } if formula == "$C2>100" && style.font.bold), "{r:?}");
}

#[test]
fn a_bad_formula_keeps_the_dialog_open() {
    let mut h = harness();
    control(&mut h, "ui.dialog", json!({"name": "newFormattingRule", "ruleType": "formula"}));
    set(&mut h, "formula", json!("=SUM("));
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_ne!(r["ok"], json!(true), "{r}");
    assert!(h.state().dialog.is_some());
    assert!(rules(&h).is_empty());
}

#[test]
fn format_opens_format_cells_and_its_ok_sets_the_rules_format() {
    let mut h = harness();
    control(&mut h, "ui.dialog", json!({"name": "newFormattingRule", "ruleType": "unique"}));
    h.run_steps(4);
    h.get_by_label("Format…").click();
    h.run_steps(6);
    let d = h.state().dialog.as_ref().expect("Format Cells is open");
    assert_eq!(d.name(), "formatCells");
    assert_eq!(d.values.get("dxf"), Some(&json!(true)));
    // Only what a rule may set: no Alignment or Protection tab.
    assert!(h.query_by_label("Alignment").is_none() && h.query_by_label("Protection").is_none());
    let mut st = serde_json::from_value::<gridcraft_engine::model::Style>(d.values["style"].clone()).unwrap();
    st.font.italic = true;
    st.fill = gridcraft_engine::model::Fill::solid(Color::rgb(0xFF, 0xC7, 0xCE));
    set(&mut h, "style", serde_json::to_value(&st).unwrap());
    let window = h.get_by_label("Format Cells").rect();
    h.query_all_by_label_contains("OK").find(|n| window.contains_rect(n.rect())).unwrap().click();
    h.run_steps(6);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name().to_string()).as_deref(), Some("newFormattingRule"), "back in the rule editor");
    assert!(rules(&h).is_empty(), "Format Cells changed no cells and added no rule");
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], json!(true), "{r}");
    let rule = rules(&h).remove(0);
    assert!(
        matches!(&rule, CfRule::Duplicate { style } if style.font.italic && style.fill == gridcraft_engine::model::Fill::solid(Color::rgb(0xFF, 0xC7, 0xCE))),
        "{rule:?}"
    );
    let cell = h.state().session.active().unwrap().wb.active().unwrap().cell(gridcraft_engine::core::CellRef::new(1, 1)).map(|c| c.style);
    assert!(cell.is_none_or(|s| s == gridcraft_engine::model::StyleId::DEFAULT), "the cells keep their own format");
}

#[test]
fn top_ten_asks_for_the_count_and_takes_a_custom_format() {
    let mut h = harness();
    control(&mut h, "ui.dialog", json!({"name": "cfQuick", "type": "top10", "rank": 10, "title": "Top 10 Items"}));
    set(&mut h, "rank", json!(3));
    set(&mut h, "preset", json!("custom"));
    h.run_steps(6);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name().to_string()).as_deref(), Some("formatCells"), "Custom Format… opens Format Cells");
    let mut st = gridcraft_engine::model::Style::default();
    st.font.color = Color::rgb(0, 0x61, 0);
    set(&mut h, "style", serde_json::to_value(&st).unwrap());
    let window = h.get_by_label("Format Cells").rect();
    h.query_all_by_label_contains("OK").find(|n| window.contains_rect(n.rect())).unwrap().click();
    h.run_steps(6);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.name().to_string()).as_deref(), Some("cfQuick"));
    let r = control(&mut h, "ui.dialog.confirm", json!({}));
    assert_eq!(r["ok"], json!(true), "{r}");
    let rule = rules(&h).remove(0);
    assert!(
        matches!(&rule, CfRule::Top10 { rank: 3, bottom: false, percent: false, style } if style.font.color == Color::rgb(0, 0x61, 0)),
        "{rule:?}"
    );
}

#[test]
fn quick_rules_keep_their_presets() {
    for (params, check) in [
        (json!({"type": "aboveAverage", "below": true, "title": "Below Average"}), "below"),
        (json!({"type": "duplicate", "title": "Duplicate Values"}), "duplicate"),
        (json!({"type": "top10", "rank": 10, "percent": true, "bottom": true, "title": "Bottom 10%"}), "bottom10pct"),
    ] {
        let mut h = harness();
        let mut p = params.clone();
        p["name"] = json!("cfQuick");
        control(&mut h, "ui.dialog", p);
        set(&mut h, "preset", json!("greenFillDarkGreenText"));
        let r = control(&mut h, "ui.dialog.confirm", json!({}));
        assert_eq!(r["ok"], json!(true), "{r}");
        let rule = rules(&h).remove(0);
        let green = rule.style().is_some_and(|s| s.font.color == Color::rgb(0, 0x61, 0));
        assert!(green, "{rule:?}");
        match check {
            "below" => assert!(matches!(rule, CfRule::AboveAverage { below: true, .. })),
            "duplicate" => assert!(matches!(rule, CfRule::Duplicate { .. })),
            _ => assert!(matches!(rule, CfRule::Top10 { bottom: true, percent: true, rank: 10, .. })),
        }
    }
}
