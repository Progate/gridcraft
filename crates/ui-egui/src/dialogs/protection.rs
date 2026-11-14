//! Review ▸ Protect Sheet, Protect Workbook and Allow Edit Ranges as Excel's dialogs: the
//! permissions list of Protect Sheet, Structure in Protect Structure and Windows, the Allow Users
//! to Edit Ranges list with New…, Modify… and Delete, the Unlock Range prompt a protected range's
//! cell opens when it is typed into, and Confirm Password after a new password.

use super::{Dialog, Field, SheetApp, json};
use serde_json::{Map, Value as Json};

/// Opens one of this module's dialogs, `None` for a name it doesn't know.
pub(super) fn open(app: &SheetApp, name: &str, params: &Json, sel: &str) -> Option<Dialog> {
    use Field::*;
    let p = |k: &str| params.get(k).and_then(Json::as_str).unwrap_or_default().to_string();
    Some(match name {
        "protectSheet" => Dialog::form(
            "protectSheet",
            "Protect Sheet",
            "review.protectSheet",
            vec![
                Check { key: "protect", label: "Protect worksheet and contents of locked cells" },
                Password { key: "password", label: "Password to unprotect sheet:" },
                Note("Allow all users of this worksheet to:".into()),
                Check { key: "selectLockedCells", label: "Select locked cells" },
                Check { key: "selectUnlockedCells", label: "Select unlocked cells" },
                Check { key: "formatCells", label: "Format cells" },
                Check { key: "formatColumns", label: "Format columns" },
                Check { key: "formatRows", label: "Format rows" },
                Check { key: "insertColumns", label: "Insert columns" },
                Check { key: "insertRows", label: "Insert rows" },
                Check { key: "insertHyperlinks", label: "Insert hyperlinks" },
                Check { key: "deleteColumns", label: "Delete columns" },
                Check { key: "deleteRows", label: "Delete rows" },
                Check { key: "sort", label: "Protect Sheet|Sort" },
                Check { key: "autofilter", label: "Use AutoFilter" },
                Check { key: "pivotTables", label: "Use PivotTable & PivotChart" },
                Check { key: "editObjects", label: "Edit objects" },
                Check { key: "editScenarios", label: "Edit scenarios" },
            ],
            json!({"protect": true, "selectLockedCells": true, "selectUnlockedCells": true}),
        ),
        "protectWorkbook" => Dialog::form(
            "protectWorkbook",
            "Protect Structure and Windows",
            "review.protectWorkbook",
            vec![
                Note("Protect workbook for".into()),
                Check { key: "structure", label: "Structure" },
                Password { key: "password", label: "Password (optional):" },
            ],
            json!({"on": true, "structure": true}),
        ),
        "allowEditRanges" => Dialog::custom("allowEditRanges", "Allow Users to Edit Ranges", json!({})),
        "newEditRange" => {
            // Excel numbers new ranges Range1, Range2… (範囲1, 範囲2…), the first title not taken.
            let taken = edit_ranges(app);
            let title = (1..)
                .map(|n| app.ui.language.tr("Range{n}").replace("{n}", &n.to_string()))
                .find(|t| !taken.iter().any(|(x, _)| x.eq_ignore_ascii_case(t)))
                .unwrap_or_default();
            Dialog::form(
                "newEditRange",
                "New Range",
                "review.allowEditRanges",
                vec![
                    Text { key: "title", label: "Title:" },
                    Ref { key: "range", label: "Refers to cells:" },
                    Password { key: "password", label: "Range password:" },
                ],
                json!({"action": "add", "title": title, "range": format!("={}", absolute(sel))}),
            )
        }
        "modifyEditRange" => Dialog::form(
            "modifyEditRange",
            "Modify Range",
            "review.allowEditRanges",
            vec![
                Text { key: "newTitle", label: "Title:" },
                Ref { key: "range", label: "Refers to cells:" },
                Password { key: "password", label: "Range password:" },
            ],
            json!({"action": "modify", "title": p("title"), "newTitle": p("title"), "range": format!("={}", absolute(&p("range")))}),
        ),
        "unlockRange" => Dialog::form(
            "unlockRange",
            "Unlock Range",
            "review.allowEditRanges",
            vec![
                Note("A cell you are trying to change is password protected.".into()),
                Password { key: "password", label: "Enter the password to change this cell:" },
            ],
            json!({"action": "unlock", "title": p("title"), "password": ""}),
        ),
        _ => return None,
    })
}

/// `A1:B2` (or a list of areas) with every reference made absolute, as Excel's boxes show it.
fn absolute(range: &str) -> String {
    range
        .split(',')
        .map(|a| match gridcraft_engine::core::RangeRef::parse(a.trim()) {
            Some(r) if r.is_single() => format!("${}${}", gridcraft_engine::core::col_to_letters(r.start.col), r.start.row + 1),
            Some(r) => format!(
                "${}${}:${}${}",
                gridcraft_engine::core::col_to_letters(r.start.col),
                r.start.row + 1,
                gridcraft_engine::core::col_to_letters(r.end.col),
                r.end.row + 1
            ),
            None => a.trim().to_string(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The active sheet's Allow Edit Ranges as (title, cells).
fn edit_ranges(app: &SheetApp) -> Vec<(String, String)> {
    let Some(sh) = app.session.active().and_then(|d| d.wb.active()) else { return Vec::new() };
    sh.edit_ranges.iter().map(|r| (r.title.clone(), r.ranges.iter().map(|x| x.a1()).collect::<Vec<_>>().join(","))).collect()
}

/// Allow Users to Edit Ranges: the ranges a password unlocks while the sheet is protected, with
/// New…, Modify… and Delete, and Protect Sheet… to protect the sheet from here, as in Excel.
pub(super) fn allow_edit_ranges(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    let ranges = edit_ranges(app);
    let protected = app.session.active().and_then(|doc| doc.wb.active().map(|sh| sh.is_protected())).unwrap_or(false);
    ui.label(lang.tr("Ranges unlocked by a password when sheet is protected:"));
    ui.horizontal(|ui| {
        egui::Frame::NONE.stroke(egui::Stroke::new(1.0, crate::theme::Tokens::get(ui.ctx()).separator)).inner_margin(4.0).show(ui, |ui| {
            ui.set_min_size(egui::vec2(300.0, 140.0));
            egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                egui::Grid::new("edit_ranges").num_columns(2).striped(true).show(ui, |ui| {
                    ui.strong(lang.tr("Edit Ranges|Title"));
                    ui.strong(lang.tr("Refers to cells"));
                    ui.end_row();
                    for (i, (title, range)) in ranges.iter().enumerate() {
                        if ui.selectable_label(i == d.list_index, title).clicked() {
                            d.list_index = i;
                        }
                        ui.label(format!("={}", absolute(range)));
                        ui.end_row();
                    }
                });
            });
        });
        ui.vertical(|ui| {
            let selected = ranges.get(d.list_index).cloned();
            // A protected sheet's ranges can't be changed, as in Excel, whose buttons grey out.
            if ui.add_enabled(!protected, egui::Button::new(lang.tr("Edit Ranges|New…")).min_size(egui::vec2(90.0, 0.0))).clicked() {
                app.open_dialog("newEditRange", json!({}));
            }
            if ui
                .add_enabled(!protected && selected.is_some(), egui::Button::new(lang.tr("Edit Ranges|Modify…")).min_size(egui::vec2(90.0, 0.0)))
                .clicked()
                && let Some((title, range)) = &selected
            {
                app.open_dialog("modifyEditRange", json!({"title": title, "range": range}));
            }
            if ui
                .add_enabled(!protected && selected.is_some(), egui::Button::new(lang.tr("Edit Ranges|Delete")).min_size(egui::vec2(90.0, 0.0)))
                .clicked()
                && let Some((title, _)) = &selected
            {
                app.run_or_alert("review.allowEditRanges", json!({"action": "delete", "title": title}));
                d.list_index = d.list_index.saturating_sub(1);
            }
        });
    });
    ui.add_space(6.0);
    if ui.add_enabled(!protected, egui::Button::new(lang.tr("Protect Sheet…"))).clicked() {
        app.open_dialog("protectSheet", json!({}));
    }
    ui.add_space(6.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button(format!("   {}   ", lang.tr("OK"))).clicked() {
            *open = false;
        }
    });
}

/// The command parameters of this module's dialogs, from their values.
pub(super) fn params(d: &Dialog, v: &mut Json) {
    let Some(m) = v.as_object_mut() else { return };
    match d.name.as_str() {
        "protectSheet" | "protectWorkbook" => {
            m.remove("protect");
            m.remove("structure");
        }
        "newEditRange" | "modifyEditRange" => {
            // The box reads `=$B$4:$B$9`, as Excel's; the command takes the reference without `=`.
            if let Some(r) = m.get("range").and_then(Json::as_str).map(|r| r.trim().trim_start_matches('=').to_string()) {
                m.insert("range".into(), json!(r));
            }
            // Modify keeps a password left blank (Excel shows the old one as asterisks).
            if d.name == "modifyEditRange" && m.get("password").and_then(Json::as_str).is_none_or(str::is_empty) {
                m.remove("password");
            }
        }
        "confirmPassword" => {
            if let Some(Json::Object(inner)) = m.get("params").cloned() {
                *m = inner;
            }
        }
        _ => {}
    }
}

/// What OK does before a protection dialog's command runs: `Some(dialog)` to show instead of
/// running it now — Confirm Password after a new password, as Excel asks to reenter it.
/// Protect Sheet without its first box and Protect Workbook without Structure protect nothing,
/// as Excel's OK does then: `Err(())` closes the dialog without running the command.
pub(super) fn before_ok(d: &Dialog, params: &Json) -> Result<Option<Dialog>, ()> {
    let on = |k: &str| d.values.get(k).and_then(Json::as_bool).unwrap_or(false);
    match d.name.as_str() {
        "protectSheet" if !on("protect") => return Err(()),
        "protectWorkbook" if !on("structure") => return Err(()),
        "protectSheet" | "protectWorkbook" | "newEditRange" | "modifyEditRange" => {}
        _ => return Ok(None),
    }
    let password = params.get("password").and_then(Json::as_str).unwrap_or_default();
    if password.is_empty() {
        return Ok(None);
    }
    let Some(command) = d.command else { return Ok(None) };
    let mut values = Map::new();
    values.insert("reenter".into(), json!(""));
    values.insert("params".into(), params.clone());
    Ok(Some(Dialog::form(
        "confirmPassword",
        "Confirm Password",
        command,
        vec![
            Field::Password { key: "reenter", label: "Reenter password to proceed." },
            Field::Note(
                "Caution: If you lose or forget the password, it cannot be recovered. It is advisable to keep a list of passwords and their corresponding workbook and sheet names in a safe place. (Remember that passwords are case-sensitive.)"
                    .into(),
            ),
        ],
        Json::Object(values),
    )))
}

/// Confirm Password's OK: whether the reentered password matches the one being set.
pub(super) fn confirmed(d: &Dialog) -> bool {
    let reenter = d.values.get("reenter").and_then(Json::as_str).unwrap_or_default();
    let set = d.values.get("params").and_then(|p| p.get("password")).and_then(Json::as_str).unwrap_or_default();
    reenter == set
}
