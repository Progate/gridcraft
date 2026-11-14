//! Data ▸ Outline ▸ Subtotal: Excel's 集計の設定 dialog. At each change in and Add subtotal to
//! list the list's headers, Use function has Excel's eleven functions, and the three options
//! (Replace current subtotals, Page break between groups, Summary below data) start as Excel's
//! do. Remove All takes the list's subtotals away; OK runs `data.subtotal`.

use gridcraft_engine::core::{RangeRef, col_to_letters};

use super::show::ok_cancel;
use super::{Dialog, Json, SheetApp, json, vec2};

/// The Use function list: `data.subtotal` `function` values and their labels, in Excel's order.
pub(super) const FUNCTIONS: [(&str, &str); 11] = [
    ("sum", "Subtotal|Sum"),
    ("count", "Subtotal|Count"),
    ("average", "Subtotal|Average"),
    ("max", "Subtotal|Max"),
    ("min", "Subtotal|Min"),
    ("product", "Subtotal|Product"),
    ("countNums", "Subtotal|Count Numbers"),
    ("stdDev", "Subtotal|StdDev"),
    ("stdDevP", "Subtotal|StdDevp"),
    ("var", "Subtotal|Var"),
    ("varP", "Subtotal|Varp"),
];

/// Opens the dialog on the list around the selection: grouped by its first column, summing its
/// last, with Replace current subtotals and Summary below data on, as Excel opens it. `params`
/// may set any of `groupBy` and `columns` (positions in the list), `function`, `replace`,
/// `pageBreaks` and `summaryBelow`.
pub(super) fn open(app: &SheetApp, params: &Json) -> Dialog {
    let (range, fields) =
        app.session.active().and(gridcraft_engine::cmd::data::subtotal_list(&app.session)).map_or((String::new(), Vec::new()), |(r, f)| (r.a1(), f));
    let last = fields.len().saturating_sub(1);
    let mut d = Dialog::custom(
        "subtotal",
        "Subtotal|Subtotal",
        json!({
            "range": range,
            "fields": fields,
            "groupBy": 0,
            "function": "sum",
            "columns": [last],
            "replace": true,
            "pageBreaks": false,
            "summaryBelow": true,
        }),
    );
    for key in ["groupBy", "function", "columns", "replace", "pageBreaks", "summaryBelow"] {
        if let Some(v) = params.get(key).filter(|v| !v.is_null()) {
            d.values.insert(key.into(), v.clone());
        }
    }
    d.command = Some("data.subtotal");
    d
}

fn fields(d: &Dialog) -> Vec<String> {
    d.values.get("fields").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn columns(d: &Dialog) -> Vec<usize> {
    let mut v: Vec<usize> = d
        .values
        .get("columns")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(Json::as_u64).filter_map(|n| usize::try_from(n).ok()).collect())
        .unwrap_or_default();
    v.sort_unstable();
    v.dedup();
    v
}

fn flag(d: &Dialog, key: &str) -> bool {
    d.values.get(key).and_then(Json::as_bool).unwrap_or(false)
}

fn list(d: &Dialog) -> Option<RangeRef> {
    d.values.get("range").and_then(Json::as_str).and_then(RangeRef::parse)
}

/// The column letter of the list's field at position `i`.
fn letter(d: &Dialog, i: usize) -> Option<String> {
    let r = list(d)?;
    let col = r.start.col.checked_add(u32::try_from(i).ok()?)?;
    (col <= r.end.col).then(|| col_to_letters(col))
}

/// The `data.subtotal` parameters for OK. Fields go by column letter, so two headers with the
/// same text still name their own columns.
pub(super) fn params(d: &Dialog) -> Json {
    let group = d.values.get("groupBy").and_then(Json::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    json!({
        "range": d.values.get("range").cloned().unwrap_or(Json::Null),
        "groupBy": letter(d, group),
        "function": d.values.get("function").and_then(Json::as_str).unwrap_or("sum"),
        "columns": columns(d).into_iter().filter_map(|i| letter(d, i)).collect::<Vec<_>>(),
        "replace": flag(d, "replace"),
        "pageBreaks": flag(d, "pageBreaks"),
        "summaryBelow": flag(d, "summaryBelow"),
    })
}

/// The dialog's body; OK goes through the dialog's command like a form's.
pub(super) fn subtotal(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let width = 300.0;
    let names = fields(d);
    let l = ui.label(lang.tr("Subtotal|At each change in:"));
    let group = d.values.get("groupBy").and_then(Json::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    craft_ui::ComboBox::from_id_salt("subtotal_group_by")
        .selected_text(names.get(group).cloned().unwrap_or_default())
        .width(width)
        .show_ui(ui, |ui| {
            for (i, name) in names.iter().enumerate() {
                if ui.selectable_label(group == i, name).clicked() {
                    d.values.insert("groupBy".into(), json!(i));
                }
            }
        })
        .response
        .labelled_by(l.id);
    ui.add_space(4.0);
    let l = ui.label(lang.tr("Subtotal|Use function:"));
    let cur = d.values.get("function").and_then(Json::as_str).unwrap_or("sum").to_string();
    let selected = FUNCTIONS.iter().find(|(v, _)| v.eq_ignore_ascii_case(&cur)).map_or("", |(_, label)| lang.tr(label));
    craft_ui::ComboBox::from_id_salt("subtotal_function")
        .selected_text(selected)
        .width(width)
        .show_ui(ui, |ui| {
            for (v, label) in FUNCTIONS {
                if ui.selectable_label(cur == v, lang.tr(label)).clicked() {
                    d.values.insert("function".into(), json!(v));
                }
            }
        })
        .response
        .labelled_by(l.id);
    ui.add_space(4.0);
    ui.label(lang.tr("Subtotal|Add subtotal to:"));
    let mut chosen = columns(d);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(vec2(width - 12.0, 100.0));
        egui::ScrollArea::vertical().id_salt("subtotal_columns").max_height(100.0).show(ui, |ui| {
            for (i, name) in names.iter().enumerate() {
                let mut on = chosen.contains(&i);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        chosen.push(i);
                    } else {
                        chosen.retain(|c| *c != i);
                    }
                    chosen.sort_unstable();
                    d.values.insert("columns".into(), json!(chosen));
                }
            }
        });
    });
    ui.add_space(4.0);
    for (key, label) in [
        ("replace", "Subtotal|Replace current subtotals"),
        ("pageBreaks", "Subtotal|Page break between groups"),
        ("summaryBelow", "Subtotal|Summary below data"),
    ] {
        let mut on = flag(d, key);
        if ui.checkbox(&mut on, lang.tr(label)).changed() {
            d.values.insert(key.into(), json!(on));
        }
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(lang.tr("Subtotal|Remove All")).clicked() {
            let range = d.values.get("range").cloned().unwrap_or(Json::Null);
            app.run_or_alert("data.removeSubtotals", json!({"range": range}));
            *open = false;
        }
    });
    ok_cancel(ui, confirm, open);
    if *confirm && columns(d).is_empty() {
        app.message = Some((d.title.clone(), "Choose at least one column to add a subtotal to.".into()));
        *confirm = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_sends_the_fields_as_column_letters() {
        let mut d = Dialog::custom("subtotal", "Subtotal|Subtotal", json!({"range": "B2:E9", "fields": ["店舗", "費目", "1月", "2月"]}));
        d.values.insert("groupBy".into(), json!(0));
        d.values.insert("columns".into(), json!([3, 2, 3, 9]));
        d.values.insert("function".into(), json!("average"));
        d.values.insert("replace".into(), json!(true));
        let p = params(&d);
        assert_eq!(p["groupBy"], "B");
        assert_eq!(p["columns"], json!(["D", "E"]), "positions past the list are left out");
        assert_eq!(p["function"], "average");
        assert_eq!((p["replace"].as_bool(), p["pageBreaks"].as_bool(), p["summaryBelow"].as_bool()), (Some(true), Some(false), Some(false)));
        assert_eq!(p["range"], "B2:E9");
    }
}
