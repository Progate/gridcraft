use super::controls::{act, anchor_keytip, checkbox_on, control_keytip, invoke, keytip, menu_items, sep};
use super::{Color32, Icon, Sense, SheetApp, Tokens, Ui, big_button, json, keytip_table, vec2};

pub(super) fn page_layout(app: &mut SheetApp, ui: &mut Ui) {
    let th = big_button(ui, Icon::Theme, "Themes", "Themes", true);
    anchor_keytip(app, ui, &th, &["pageLayout.theme"]);
    egui::Popup::menu(&th).show(|ui| {
        for (name, colors) in gridcraft_engine::cmd::view::themes() {
            ui.horizontal(|ui| {
                for c in colors.iter().skip(4).take(6) {
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    ui.painter().rect_filled(r, 1.0, Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, *c as u8));
                }
                if ui.button(name).clicked() {
                    act(app, "pageLayout.theme", json!({"name": name}));
                }
            });
        }
    });
    sep(ui);
    let m = big_button(ui, Icon::Margins, "Margins", "Margins", true);
    for sequence in keytip_table::group_extension(app.keytips.prefix().unwrap_or_default(), &["pageLayout.margins", "dialog:pageSetup"]) {
        keytip(app, ui, sequence, m.rect.center_bottom());
    }
    egui::Popup::menu(&m).show(|ui| {
        menu_items(
            app,
            ui,
            &[
                ("Normal", "pageLayout.margins", json!({"preset": "normal"})),
                ("Wide", "pageLayout.margins", json!({"preset": "wide"})),
                ("Narrow", "pageLayout.margins", json!({"preset": "narrow"})),
                ("Custom Margins…", "dialog:pageSetup", json!({"tab": "Margins"})),
            ],
        );
    });
    let o = big_button(ui, Icon::Orient, "Page Layout|Orientation", "Page Layout|Orientation", true);
    control_keytip(app, ui, &o, "pageLayout.orientation");
    egui::Popup::menu(&o).show(|ui| {
        menu_items(
            app,
            ui,
            &[
                ("Portrait", "pageLayout.orientation", json!({"orientation": "portrait"})),
                ("Landscape", "pageLayout.orientation", json!({"orientation": "landscape"})),
            ],
        );
    });
    let s = big_button(ui, Icon::Paper, "Size", "Paper Size", true);
    control_keytip(app, ui, &s, "pageLayout.size");
    egui::Popup::menu(&s).show(|ui| {
        for p in crate::dialogs::PAPER_SIZES {
            if ui.button(p).clicked() {
                act(app, "pageLayout.size", json!({"paper": p}));
            }
        }
    });
    let pa = big_button(ui, Icon::PrintArea, "Print\nArea", "Print Area", true);
    anchor_keytip(app, ui, &pa, &["pageLayout.printArea"]);
    egui::Popup::menu(&pa).show(|ui| {
        menu_items(
            app,
            ui,
            &[("Set Print Area", "pageLayout.printArea", json!({})), ("Clear Print Area", "pageLayout.printArea", json!({"clear": true}))],
        );
    });
    let br = big_button(ui, Icon::PageBreak, "Breaks", "Page Breaks", true);
    for sequence in keytip_table::group_extension(app.keytips.prefix().unwrap_or_default(), &["pageLayout.breaks"]) {
        keytip(app, ui, sequence, br.rect.center_bottom());
    }
    egui::Popup::menu(&br).show(|ui| {
        menu_items(
            app,
            ui,
            &[
                ("Insert Page Break", "pageLayout.breaks", json!({"insert": true})),
                ("Remove Page Break", "pageLayout.breaks", json!({"remove": true})),
                ("Reset All Page Breaks", "pageLayout.breaks", json!({"reset": true})),
            ],
        );
    });
    let __k = big_button(ui, Icon::Sheet, "Print\nTitles", "Print Titles", false);
    control_keytip(app, ui, &__k, "pageLayout.printTitles");
    if __k.clicked() {
        app.open_dialog("pageSetup", json!({"tab": "Sheet"}));
    }
    sep(ui);
    ui.vertical(|ui| scale_to_fit(app, ui));
    sep(ui);
    let sh = app
        .session
        .active()
        .and_then(|d| d.wb.active().map(|s| (s.show_gridlines, s.show_headings, s.print.gridlines, s.print.headings)))
        .unwrap_or((true, true, false, false));
    // These two checkboxes mean the screen view vs. the printed page. English shows "View" (as in
    // Excel), but the key is distinct from the "View" tab so languages can word them apart.
    let screen = app.ui.language.tr("Sheet Options|View");
    let print = app.ui.language.tr("Print");
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(app.ui.language.tr("Gridlines")).strong().small());
        let mut v = sh.0;
        let resp = ui.checkbox(&mut v, screen);
        control_keytip(app, ui, &resp, "view.gridlines");
        if resp.changed() || invoke(app, "view.gridlines") {
            act(app, "view.gridlines", json!({"on": checkbox_on(resp.changed(), v)}));
        }
        let mut p = sh.2;
        let resp = ui.checkbox(&mut p, print);
        control_keytip(app, ui, &resp, "pageLayout.printGridlines");
        if resp.changed() || invoke(app, "pageLayout.printGridlines") {
            act(app, "pageLayout.printGridlines", json!({"on": checkbox_on(resp.changed(), p)}));
        }
    });
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(app.ui.language.tr("Headings")).strong().small());
        let mut v = sh.1;
        let resp = ui.checkbox(&mut v, screen);
        control_keytip(app, ui, &resp, "view.headings");
        if resp.changed() || invoke(app, "view.headings") {
            act(app, "view.headings", json!({"on": checkbox_on(resp.changed(), v)}));
        }
        let mut p = sh.3;
        let resp = ui.checkbox(&mut p, print);
        control_keytip(app, ui, &resp, "pageLayout.printHeadings");
        if resp.changed() || invoke(app, "pageLayout.printHeadings") {
            act(app, "pageLayout.printHeadings", json!({"on": checkbox_on(resp.changed(), p)}));
        }
    });
    let rtl = app.session.active().and_then(|d| d.wb.active()).is_some_and(|s| s.right_to_left);
    let r = big_button(ui, Icon::SheetRtl, "Sheet\nRight-to-Left", "Sheet Right-to-Left", false);
    if rtl {
        let t = Tokens::get(ui.ctx());
        ui.painter().rect_stroke(r.rect, 5.0, egui::Stroke::new(1.0, t.separator), egui::StrokeKind::Inside);
    }
    if r.clicked() {
        act(app, "view.rightToLeft", json!({"on": !rtl}));
    }
}

/// Page Layout › Scale to Fit: the pages wide and tall the sheet prints on (Automatic = 0), and
/// the percentage when both are automatic, as in Excel. Each choice is one `pageLayout.pageSetup`.
fn scale_to_fit(app: &mut SheetApp, ui: &mut Ui) {
    let lang = app.ui.language;
    let (fit, scale) = app.session.active().and_then(|d| d.wb.active()).map(|s| (s.print.fit_to, s.print.scale)).unwrap_or((None, 100));
    let (w, h) = fit.unwrap_or((0, 0));
    let pages = |n: u16| match n {
        0 => lang.tr("Automatic").to_string(),
        1 => lang.tr("1 page").to_string(),
        n => lang.tr("{n} pages").replace("{n}", &n.to_string()),
    };
    let small = |text: &str| egui::RichText::new(text).font(crate::theme::ui_font(12.0));
    // Both automatic is no fitting at all: the percentage applies again.
    let set = |app: &mut SheetApp, w: u16, h: u16| {
        let p = if w == 0 && h == 0 { json!({"fitToPage": false}) } else { json!({"fitWidth": w, "fitHeight": h}) };
        act(app, "pageLayout.pageSetup", p);
    };
    // Three rows of boxes in the card's height, as the columns of small buttons hold three.
    ui.spacing_mut().interact_size.y = 20.0;
    ui.spacing_mut().button_padding.y = 1.0;
    egui::Grid::new("scale_to_fit").num_columns(2).spacing(vec2(6.0, 1.0)).show(ui, |ui| {
        for (key, current) in [("Width:", w), ("Height:", h)] {
            let label = ui.label(small(lang.tr(key)));
            craft_ui::ComboBox::from_id_salt(("scale_to_fit", key))
                .width(96.0)
                .selected_text(small(&pages(current)))
                .show_ui(ui, |ui| {
                    for n in 0..=9 {
                        if ui.selectable_label(n == current, pages(n)).clicked() {
                            if key == "Width:" { set(app, n, h) } else { set(app, w, n) }
                        }
                    }
                })
                .response
                .labelled_by(label.id);
            ui.end_row();
        }
        let label = ui.label(small(lang.tr("Scale:")));
        ui.add_enabled_ui(fit.is_none(), |ui| {
            craft_ui::ComboBox::from_id_salt("scale_to_fit_scale")
                .width(96.0)
                .selected_text(small(&format!("{scale}%")))
                .show_ui(ui, |ui| {
                    for v in [400u16, 200, 150, 125, 110, 100, 95, 90, 85, 80, 75, 70, 65, 60, 55, 50, 40, 30, 25, 10] {
                        if ui.selectable_label(v == scale, format!("{v}%")).clicked() {
                            act(app, "pageLayout.pageSetup", json!({"scale": v}));
                        }
                    }
                })
                .response
                .labelled_by(label.id);
        });
        ui.end_row();
    });
}
