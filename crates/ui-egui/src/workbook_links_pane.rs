//! Data ▸ Queries & Connections ▸ Workbook Links: Excel's ブックのリンク task pane (Microsoft 365's
//! successor to the Edit Links dialog). It lists the other workbooks this one's formulas read,
//! each with its folder and status, and Refresh, Change source and Break links; Refresh All and
//! Break All act on every link. Each button runs the engine's `data.*Links` command.

use egui::Ui;
use serde_json::{Value as Json, json};

use crate::SheetApp;
use crate::theme::Tokens;

/// The links of the active workbook, read again only when the workbook changed (the list walks
/// every formula).
fn links(app: &mut SheetApp, ui: &Ui) -> Vec<Json> {
    let Some(d) = app.session.active() else { return vec![] };
    let key = egui::Id::unique(("workbook_links", d.uid));
    let rev = d.revision;
    if let Some((r, list)) = ui.ctx().data(|m| m.get_temp::<(u64, Vec<Json>)>(key))
        && r == rev
    {
        return list;
    }
    let list =
        app.session.execute("data.editLinks", json!({})).ok().and_then(|r| r.get("links").and_then(Json::as_array).cloned()).unwrap_or_default();
    ui.ctx().data_mut(|m| m.insert_temp(key, (rev, list.clone())));
    list
}

/// Statuses the last Refresh reported, by file name.
fn statuses_id(app: &SheetApp) -> egui::Id {
    egui::Id::unique(("workbook_link_status", app.session.active().map_or(0, |d| d.uid)))
}

/// Runs a refresh and remembers the statuses it reports.
fn refresh(app: &mut SheetApp, ui: &Ui, params: Json) {
    if let Ok(r) = app.run("data.updateLinks", params) {
        let found: Vec<(String, String)> = r
            .get("links")
            .and_then(Json::as_array)
            .map(|a| a.iter().filter_map(|l| Some((l.get("name")?.as_str()?.to_string(), l.get("status")?.as_str()?.to_string()))).collect())
            .unwrap_or_default();
        let id = statuses_id(app);
        ui.ctx().data_mut(|m| {
            let mut all: Vec<(String, String)> = m.get_temp(id).unwrap_or_default();
            for (name, status) in found {
                all.retain(|(n, _)| *n != name);
                all.push((name, status));
            }
            m.insert_temp(id, all);
        });
    }
}

/// A status as the pane words it.
fn status_text(status: &str) -> &'static str {
    match status {
        "OK" => "Workbook Links|OK",
        s if s.starts_with("Error") => "Workbook Links|Error: Source not found",
        _ => "Workbook Links|Unknown",
    }
}

pub(crate) fn body(app: &mut SheetApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let list = links(app, ui);
    let lang = app.ui.language;
    ui.horizontal(|ui| {
        if ui.add_enabled(!list.is_empty(), egui::Button::new(lang.tr("Workbook Links|Refresh All"))).clicked() {
            refresh(app, ui, json!({}));
        }
        if ui.add_enabled(!list.is_empty(), egui::Button::new(lang.tr("Workbook Links|Break All"))).clicked() {
            app.run_or_alert("data.breakLinks", json!({}));
        }
    });
    ui.add_space(6.0);
    if list.is_empty() {
        ui.label(egui::RichText::new(lang.tr("Workbook Links|This workbook has no links to other workbooks.")).italics());
        return;
    }
    let statuses: Vec<(String, String)> = ui.ctx().data(|m| m.get_temp(statuses_id(app))).unwrap_or_default();
    for l in list {
        let n = l.get("link").and_then(Json::as_u64).unwrap_or(0);
        let name = l.get("name").and_then(Json::as_str).unwrap_or("").to_string();
        let folder = l.get("folder").and_then(Json::as_str).unwrap_or("").to_string();
        let target = l.get("target").and_then(Json::as_str).unwrap_or("").to_string();
        let status = statuses.iter().find(|(s, _)| *s == name).map_or("Unknown", |(_, s)| s.as_str()).to_string();
        egui::Frame::NONE.fill(t.menu_bg).corner_radius(6.0).inner_margin(8.0).stroke(egui::Stroke::new(1.0, t.separator)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(&name).strong());
            if !folder.is_empty() {
                ui.label(egui::RichText::new(&folder).small().color(t.text_dim));
            }
            ui.label(egui::RichText::new(lang.tr(status_text(&status))).small().color(if status.starts_with("Error") {
                t.danger
            } else {
                t.text_dim
            }));
            ui.horizontal_wrapped(|ui| {
                if ui.small_button(lang.tr("Workbook Links|Refresh")).clicked() {
                    refresh(app, ui, json!({"link": n}));
                }
                if ui.small_button(lang.tr("Workbook Links|Change source")).clicked() {
                    app.open_dialog("changeLinkSource", json!({"link": n, "source": target}));
                }
                if ui.small_button(lang.tr("Workbook Links|Break links")).clicked() {
                    app.run_or_alert("data.breakLinks", json!({"link": n}));
                }
            });
        });
        ui.add_space(6.0);
    }
}
