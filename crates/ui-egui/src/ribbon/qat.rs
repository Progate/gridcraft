//! The Quick Access Toolbar as Excel's: commands added from a ribbon button's right-click menu
//! (Add to Quick Access Toolbar), removed the same way or from Customize Quick Access Toolbar,
//! which also moves the toolbar below the ribbon and back. The list is a UI preference
//! (`UiState::qat`, `UiState::qat_below`), not part of the workbook.

use egui::{Ui, vec2};
use serde_json::json;

use crate::SheetApp;
use crate::icons::Icon;
use crate::theme::{self, Tokens};
use crate::widgets::icon_button;

/// Most commands the toolbar keeps, so a preferences file can't make it endless.
pub const MAX_COMMANDS: usize = 64;

/// The frame's pending add or remove, left by a right-click menu that only borrows the app.
fn request_id() -> egui::Id {
    egui::Id::unique("gridcraft.qat_request")
}

/// The label of command `id` as the ribbon names it, `None` for an id that isn't a command.
fn command_label(id: &str) -> Option<&'static str> {
    gridcraft_engine::cmd::command_specs().iter().find(|c| c.id == id).map(|c| c.label)
}

/// Adds Excel's Add to Quick Access Toolbar (or Remove from Quick Access Toolbar, once it is
/// there) to the right-click menu of the ribbon control that runs command `id`.
pub(super) fn context_menu(app: &SheetApp, response: &egui::Response, id: &str) {
    if command_label(id).is_none() {
        return;
    }
    let on = app.ui.qat.iter().any(|c| c == id);
    let lang = app.ui.language;
    response.context_menu(|ui| {
        let label = if on { "Remove from Quick Access Toolbar" } else { "Add to Quick Access Toolbar" };
        if ui.button(lang.tr(label)).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(request_id(), (id.to_string(), !on)));
            ui.close();
        }
    });
}

/// Adds command `id` to the toolbar (`add`) or removes it.
pub fn set_command(app: &mut SheetApp, id: &str, add: bool) {
    app.ui.qat.retain(|c| c != id);
    if add && command_label(id).is_some() && app.ui.qat.len() < MAX_COMMANDS {
        app.ui.qat.push(id.to_string());
    }
}

/// Applies the add or remove a right-click menu asked for this frame.
fn apply_request(app: &mut SheetApp, ui: &Ui) {
    if let Some((id, add)) = ui.ctx().data_mut(|d| d.remove_temp::<(String, bool)>(request_id())) {
        set_command(app, &id, add);
    }
}

/// The added commands' buttons and the Customize Quick Access Toolbar menu, in the title bar or
/// in the strip below the ribbon.
pub(super) fn commands(app: &mut SheetApp, ui: &mut Ui) {
    apply_request(app, ui);
    let lang = app.ui.language;
    let t = Tokens::get(ui.ctx());
    for id in app.ui.qat.clone() {
        let Some(label) = command_label(&id) else { continue };
        let text = egui::RichText::new(lang.tr(label)).font(theme::ui_font(12.0)).color(t.text_dim);
        let b = ui.add(egui::Button::new(text).frame(false).min_size(vec2(0.0, 22.0))).on_hover_text(lang.tr(label));
        if b.clicked() {
            app.run_or_alert(&id, json!({}));
        }
        b.context_menu(|ui| {
            if ui.button(lang.tr("Remove from Quick Access Toolbar")).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(request_id(), (id.clone(), false)));
                ui.close();
            }
        });
    }
    let customize = icon_button(ui, Icon::Chevron, t.text_dim, "Customize Quick Access Toolbar", vec2(14.0, 22.0));
    egui::Popup::menu(&customize).show(|ui| {
        ui.label(egui::RichText::new(lang.tr("Customize Quick Access Toolbar")).strong());
        for id in app.ui.qat.clone() {
            let Some(label) = command_label(&id) else { continue };
            // Every listed command is on the toolbar: clearing its check removes it, as in Excel.
            if ui.add(egui::Button::selectable(true, lang.tr(label))).clicked() {
                set_command(app, &id, false);
            }
        }
        ui.separator();
        let (label, below) = if app.ui.qat_below { ("Show Above the Ribbon", false) } else { ("Show Below the Ribbon", true) };
        if ui.button(lang.tr(label)).clicked() {
            app.ui.qat_below = below;
            ui.close();
        }
    });
}

/// The toolbar's strip under the ribbon, while Show Below the Ribbon is chosen (or `always`, when
/// the host draws no title bar). Without added commands a hidden title bar shows no strip.
pub fn strip(app: &mut SheetApp, ui: &mut Ui, always: bool) {
    if !(app.ui.qat_below || (always && !app.ui.qat.is_empty())) {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("quick_access_toolbar")
        .exact_size(28.0)
        .frame(egui::Frame::NONE.fill(t.window).inner_margin(egui::Margin { left: 12, right: 12, top: 2, bottom: 2 }))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| commands(app, ui));
        });
}
