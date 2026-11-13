//! Data ▸ Data Tools ▸ Consolidate: the function, the reference box with Add and Delete and the
//! list of all references, Use labels in (Top row, Left column) and Create links to source data,
//! as Excel's dialog has them. OK runs `data.consolidate` at the active cell.

use gridcraft_engine::core::{RangeRef, split_sheet};

use super::ref_edit::{RefLabel, ref_edit};
use super::show::ok_cancel;
use super::{Dialog, Json, SheetApp, json, vec2};

/// The Function list: `data.consolidate` `function` values and their labels, in Excel's order.
pub(super) const FUNCTIONS: [(&str, &str); 11] = [
    ("sum", "Consolidate|Sum"),
    ("count", "Consolidate|Count"),
    ("average", "Consolidate|Average"),
    ("max", "Consolidate|Max"),
    ("min", "Consolidate|Min"),
    ("product", "Consolidate|Product"),
    ("countNums", "Consolidate|Count Numbers"),
    ("stdDev", "Consolidate|StdDev"),
    ("stdDevP", "Consolidate|StdDevp"),
    ("var", "Consolidate|Var"),
    ("varP", "Consolidate|Varp"),
];

/// Opens the dialog with Sum, an empty reference box and no references; `params` may fill any of
/// `function`, `reference`, `sources`, `topRow`, `leftColumn` and `links` (a script, or the
/// command run with no sources).
pub(super) fn open(params: &Json) -> Dialog {
    let mut d = Dialog::custom(
        "consolidate",
        "Consolidate",
        json!({"function": "sum", "reference": "", "sources": [], "topRow": false, "leftColumn": false, "links": false}),
    );
    for key in ["function", "reference", "sources", "topRow", "leftColumn", "links"] {
        if let Some(v) = params.get(key).filter(|v| !v.is_null()) {
            d.values.insert(key.into(), v.clone());
        }
    }
    d.command = Some("data.consolidate");
    d
}

fn sources(d: &Dialog) -> Vec<String> {
    d.values.get("sources").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn reference(d: &Dialog) -> String {
    d.values.get("reference").and_then(Json::as_str).unwrap_or("").trim().to_string()
}

fn flag(d: &Dialog, key: &str) -> bool {
    d.values.get(key).and_then(Json::as_bool).unwrap_or(false)
}

/// Whether `r` reads as one range, with or without a sheet name (`East!$A$1:$C$3`).
fn valid_reference(r: &str) -> bool {
    let (_, body) = split_sheet(r.trim().trim_start_matches('='));
    RangeRef::parse(&body.replace('$', "")).is_some()
}

/// The `data.consolidate` parameters for OK: the listed references, or the reference box alone
/// when nothing was added (Excel consolidates what the box holds then). The destination is left
/// out, so the table starts at the active cell.
pub(super) fn params(d: &Dialog) -> Json {
    let mut list = sources(d);
    let typed = reference(d);
    if list.is_empty() && !typed.is_empty() {
        list.push(typed);
    }
    let list: Vec<String> = list.into_iter().map(|r| r.trim_start_matches('=').to_string()).collect();
    json!({
        "function": d.values.get("function").and_then(Json::as_str).unwrap_or("sum"),
        "sources": list,
        "topRow": flag(d, "topRow"),
        "leftColumn": flag(d, "leftColumn"),
        "links": flag(d, "links"),
    })
}

/// Add: the reference box joins the list, once (references match whatever their case).
fn add(app: &mut SheetApp, d: &mut Dialog) {
    let r = reference(d);
    if r.is_empty() {
        return;
    }
    if !valid_reference(&r) {
        app.message = Some((d.title.clone(), "Reference isn't valid.".into()));
        return;
    }
    let mut list = sources(d);
    match list.iter().position(|x| x.eq_ignore_ascii_case(&r)) {
        Some(i) => d.list_index = i,
        None => {
            list.push(r);
            d.list_index = list.len() - 1;
        }
    }
    d.values.insert("sources".into(), json!(list));
}

/// Delete: the selected reference leaves the list.
fn delete(d: &mut Dialog) {
    let mut list = sources(d);
    if d.list_index < list.len() {
        list.remove(d.list_index);
        d.list_index = d.list_index.min(list.len().saturating_sub(1));
        d.values.insert("sources".into(), json!(list));
    }
}

/// The dialog's body; OK goes through the dialog's command like a form's.
pub(super) fn consolidate(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let width = 300.0;
    let l = ui.label(lang.tr("Consolidate|Function:"));
    let cur = d.values.get("function").and_then(Json::as_str).unwrap_or("sum").to_string();
    let selected = FUNCTIONS.iter().find(|(v, _)| v.eq_ignore_ascii_case(&cur)).map_or("", |(_, label)| lang.tr(label));
    craft_ui::ComboBox::from_id_salt("consolidate_function")
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
    let l = ui.label(lang.tr("Consolidate|Reference:"));
    ref_edit(app, ui, d, "reference", RefLabel::By(l.id), width + 30.0);
    ui.add_space(4.0);
    let list = sources(d);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(lang.tr("Consolidate|All references:"));
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_size(vec2(width - 12.0, 90.0));
                egui::ScrollArea::vertical().max_height(90.0).show(ui, |ui| {
                    for (i, r) in list.iter().enumerate() {
                        // Choosing a reference puts it in the box, to edit or delete, as in Excel.
                        if ui.selectable_label(i == d.list_index, r).clicked() {
                            d.list_index = i;
                            d.values.insert("reference".into(), json!(r));
                        }
                    }
                });
            });
        });
        ui.vertical(|ui| {
            ui.add_space(18.0);
            let w = vec2(80.0, 0.0);
            if ui.add(egui::Button::new(lang.tr("Consolidate|Add")).min_size(w)).clicked() {
                add(app, d);
            }
            if ui.add_enabled(d.list_index < list.len(), egui::Button::new(lang.tr("Consolidate|Delete")).min_size(w)).clicked() {
                delete(d);
            }
        });
    });
    ui.add_space(6.0);
    ui.label(lang.tr("Consolidate|Use labels in"));
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            for (key, label) in [("topRow", "Consolidate|Top row"), ("leftColumn", "Consolidate|Left column")] {
                let mut on = flag(d, key);
                if ui.checkbox(&mut on, lang.tr(label)).changed() {
                    d.values.insert(key.into(), json!(on));
                }
            }
        });
        ui.add_space(16.0);
        let mut on = flag(d, "links");
        if ui.checkbox(&mut on, lang.tr("Consolidate|Create links to source data")).changed() {
            d.values.insert("links".into(), json!(on));
        }
    });
    ok_cancel(ui, confirm, open);
    if *confirm && sources(d).is_empty() && reference(d).is_empty() {
        app.message = Some((d.title.clone(), "Add the ranges to consolidate to All references.".into()));
        *confirm = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_sends_the_listed_references_or_else_the_box() {
        let mut d = open(&json!({}));
        d.values.insert("reference".into(), json!("East!$A$1:$C$3"));
        assert_eq!(params(&d)["sources"], json!(["East!$A$1:$C$3"]));
        d.values.insert("sources".into(), json!(["=West!$A$1:$C$3", "North!$A$1:$C$3"]));
        d.values.insert("function".into(), json!("stdDevP"));
        d.values.insert("leftColumn".into(), json!(true));
        let p = params(&d);
        assert_eq!(p["sources"], json!(["West!$A$1:$C$3", "North!$A$1:$C$3"]));
        assert_eq!(p["function"], "stdDevP");
        assert_eq!((p["topRow"].as_bool(), p["leftColumn"].as_bool(), p["links"].as_bool()), (Some(false), Some(true), Some(false)));
        assert!(p.get("destination").is_none());
    }

    #[test]
    fn references_are_checked_before_they_are_added() {
        assert!(valid_reference("East!$A$1:$C$3"));
        assert!(valid_reference("'Sales 2024'!B2:D9"));
        assert!(valid_reference("A1"));
        assert!(!valid_reference("East!"));
        assert!(!valid_reference("hello world"));
    }
}
