//! The Conditional Formatting Rules Manager (Home › Conditional Formatting › Manage Rules…): the
//! rules of the selection, the sheet or another sheet, in the order they apply, each with its
//! description, format, Applies To and Stop If True; New Rule…, Edit Rule…, Delete Rule, Duplicate
//! Rule and the move buttons change a working copy that OK and Apply write, as in Excel.

use std::collections::{BTreeMap, BTreeSet};

use gridcraft_engine::cmd::cond_format::parse_applies_to;
use gridcraft_engine::core::RangeRef;
use gridcraft_engine::model::{CfRule, CondFormat};

use super::cf_rule::{applies_to_text, describe, rule_preview};
use super::ref_edit::{RefLabel, ref_edit};
use super::show::ok_cancel;
use super::{Dialog, Json, SheetApp, json, vec2};

/// The list scrolls inside the dialog: a sheet read from a file can carry thousands of rules, and
/// a list laid out in full made the window as tall as all of them (issue #3321).
const LIST_HEIGHT: f32 = 300.0;

/// The manager's working copy: the rules of each sheet it has shown, in priority order, changed
/// in place until OK or Apply writes them.
#[derive(Clone, Debug, Default)]
pub(crate) struct CfManager {
    /// What Show formatting rules for shows: `None` the current selection, `Some(i)` sheet `i`.
    scope: Option<usize>,
    /// The sheet whose rules are listed, set every frame from `scope` and the active sheet.
    sheet: usize,
    drafts: BTreeMap<usize, Vec<CondFormat>>,
    changed: BTreeSet<usize>,
    /// The selected rule, as an index into the listed sheet's rules.
    selected: Option<usize>,
    /// The rule whose Applies To the reference box holds; another selected rule refills the box.
    boxed: Option<usize>,
}

impl CfManager {
    fn rules(&mut self, app: &SheetApp, sheet: usize) -> &mut Vec<CondFormat> {
        self.drafts.entry(sheet).or_insert_with(|| {
            let mut rules = app.session.active().and_then(|d| d.wb.sheets.get(sheet)).map(|sh| sh.cond_formats.clone()).unwrap_or_default();
            rules.sort_by_key(|c| c.priority);
            rules
        })
    }

    /// Takes a rule back from the rule editor: it replaces rule `index`, or goes first, on
    /// `selection`, as a new rule does.
    pub(super) fn put(&mut self, rule: CfRule, index: Option<usize>, selection: Vec<RangeRef>) {
        let sheet = self.sheet;
        let rules = self.drafts.entry(sheet).or_default();
        match index.and_then(|i| rules.get_mut(i)) {
            Some(cf) => cf.rule = rule,
            None => {
                rules.insert(0, CondFormat { ranges: selection, rule, priority: 0, stop_if_true: false });
                self.selected = Some(0);
                self.boxed = None;
            }
        }
        self.changed.insert(sheet);
    }

    /// Writes every changed sheet's rules; the first error stops and is returned.
    fn apply(&mut self, app: &mut SheetApp) -> Result<(), String> {
        for sheet in std::mem::take(&mut self.changed) {
            let Some(rules) = self.drafts.get(&sheet) else { continue };
            let set: Vec<Json> = rules
                .iter()
                .map(|cf| {
                    json!({
                        "appliesTo": cf.ranges.iter().map(RangeRef::a1).collect::<Vec<_>>().join(","),
                        "cfRule": serde_json::to_value(&cf.rule).unwrap_or(Json::Null),
                        "stopIfTrue": cf.stop_if_true,
                    })
                })
                .collect();
            if let Err(e) = app.run("home.manageRules", json!({"sheet": sheet, "set": set})) {
                self.changed.insert(sheet);
                return Err(e);
            }
        }
        // Read again from the workbook: priorities are renumbered there.
        self.drafts.clear();
        Ok(())
    }
}

/// The manager's state for a newly opened dialog: the current selection's rules, as Excel opens it.
pub(super) fn new_manager() -> Box<CfManager> {
    Box::default()
}

pub(super) fn manage_rules(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = app.ui.language;
    let mut m = d.cf.take().unwrap_or_else(new_manager);
    let (active, sheet_names, selection) = app
        .session
        .active()
        .map(|doc| (doc.wb.active_sheet(), doc.wb.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), doc.selection.ranges.clone()))
        .unwrap_or_default();
    if m.scope.is_some_and(|s| s >= sheet_names.len()) {
        m.scope = None;
    }
    let sheet = m.scope.unwrap_or(active);
    if sheet != m.sheet {
        m.selected = None;
        m.boxed = None;
    }
    m.sheet = sheet;
    let theme = app.session.active().map(|doc| doc.wb.theme.clone()).unwrap_or_default();

    ui.horizontal(|ui| {
        let label = ui.label(lang.tr("Show formatting rules for:"));
        let shown = match m.scope {
            None => lang.tr("Current Selection").to_string(),
            Some(i) if i == active => lang.tr("This Worksheet").to_string(),
            Some(i) => sheet_names.get(i).cloned().unwrap_or_default(),
        };
        craft_ui::ComboBox::from_id_salt("cfManagerScope")
            .selected_text(shown)
            .width(200.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(m.scope.is_none(), lang.tr("Current Selection")).clicked() {
                    m.scope = None;
                }
                if ui.selectable_label(m.scope == Some(active), lang.tr("This Worksheet")).clicked() {
                    m.scope = Some(active);
                }
                for (i, name) in sheet_names.iter().enumerate().filter(|(i, _)| *i != active) {
                    if ui.selectable_label(m.scope == Some(i), name).clicked() {
                        m.scope = Some(i);
                    }
                }
            })
            .response
            .labelled_by(label.id);
    });

    // The rows shown: all of the sheet's rules, or those on the selection, in priority order.
    let on_selection = m.scope.is_none();
    let rows: Vec<usize> = m
        .rules(app, sheet)
        .iter()
        .enumerate()
        .filter(|(_, cf)| !on_selection || cf.ranges.iter().any(|r| selection.iter().any(|s| s.intersects(r))))
        .map(|(i, _)| i)
        .collect();
    if m.selected.is_none_or(|i| !rows.contains(&i)) {
        m.selected = rows.first().copied();
    }
    let selected = m.selected;
    let count = m.rules(app, sheet).len();

    let mut select: Option<usize> = None;
    ui.horizontal(|ui| {
        if ui.button(lang.tr("Manager|New Rule…")).clicked() {
            app.open_dialog("newFormattingRule", json!({"manager": true}));
        }
        let has = selected.is_some();
        if ui.add_enabled(has, egui::Button::new(lang.tr("Edit Rule…"))).clicked()
            && let Some(i) = selected
            && let Some(cf) = m.rules(app, sheet).get(i)
        {
            let rule = gridcraft_engine::cmd::cond_format::rule_params(&cf.rule);
            app.open_dialog("newFormattingRule", json!({"manager": true, "index": i, "rule": rule}));
        }
        if ui.add_enabled(has, egui::Button::new(lang.tr("Delete Rule"))).clicked()
            && let Some(i) = selected
        {
            let rules = m.rules(app, sheet);
            if i < rules.len() {
                rules.remove(i);
                m.changed.insert(sheet);
                m.selected = None;
                m.boxed = None;
            }
        }
        if ui.add_enabled(has, egui::Button::new(lang.tr("Duplicate Rule"))).clicked()
            && let Some(i) = selected
        {
            let rules = m.rules(app, sheet);
            if let Some(cf) = rules.get(i).cloned() {
                rules.insert(i + 1, cf);
                m.changed.insert(sheet);
                select = Some(i + 1);
            }
        }
        let up = ui.add_enabled(selected.is_some_and(|i| i > 0), egui::Button::new("▲")).on_hover_text(lang.tr("Move Up"));
        if up.clicked()
            && let Some(i) = selected
        {
            m.rules(app, sheet).swap(i, i - 1);
            m.changed.insert(sheet);
            select = Some(i - 1);
        }
        let down = ui.add_enabled(selected.is_some_and(|i| i + 1 < count), egui::Button::new("▼")).on_hover_text(lang.tr("Move Down"));
        if down.clicked()
            && let Some(i) = selected
        {
            m.rules(app, sheet).swap(i, i + 1);
            m.changed.insert(sheet);
            select = Some(i + 1);
        }
    });

    let widths = [240.0, 120.0, 150.0, 60.0];
    ui.horizontal(|ui| {
        for (w, title) in widths.iter().zip(["Rule (applied in order shown)", "Manager|Format", "Applies to", "Stop If True"]) {
            ui.add_sized([*w, 18.0], egui::Label::new(egui::RichText::new(lang.tr(title)).strong()).truncate());
        }
    });
    ui.separator();
    if rows.is_empty() {
        ui.label(lang.tr("There are no conditional formatting rules on this sheet."));
    }
    let row_height = 24.0;
    let mut stop_toggle: Option<(usize, bool)> = None;
    let rules = &*m.rules(app, sheet);
    egui::ScrollArea::vertical().max_height(LIST_HEIGHT).auto_shrink([false, true]).show_rows(ui, row_height, rows.len(), |ui, range| {
        for &i in rows.get(range).unwrap_or_default() {
            let Some(cf) = rules.get(i) else { continue };
            ui.horizontal(|ui| {
                let h = row_height - 4.0;
                let desc = format!("{}. {}", i + 1, describe(&cf.rule, lang));
                if ui.add_sized([widths[0], h], egui::Button::selectable(selected == Some(i), desc).truncate()).clicked() {
                    select = Some(i);
                }
                ui.allocate_ui(vec2(widths[1], h), |ui| {
                    ui.set_width(widths[1]);
                    rule_preview(ui, &cf.rule, &theme, lang, vec2(widths[1] - 8.0, h - 2.0));
                });
                ui.add_sized([widths[2], h], egui::Label::new(applies_to_text(cf)).truncate());
                let mut stop = cf.stop_if_true;
                // Colour scales, data bars and icon sets can't stop the rules after them, as in Excel.
                let can_stop = cf.rule.style().is_some();
                if ui.add_enabled(can_stop, egui::Checkbox::without_text(&mut stop)).on_hover_text(lang.tr("Stop If True")).changed() {
                    stop_toggle = Some((i, stop));
                }
            });
        }
    });
    if let Some((i, stop)) = stop_toggle
        && let Some(cf) = m.rules(app, sheet).get_mut(i)
    {
        cf.stop_if_true = stop;
        m.changed.insert(sheet);
        select = Some(i);
    }
    if let Some(i) = select {
        m.selected = Some(i);
        m.boxed = None;
    }

    // Applies To of the selected rule: a reference box that also takes a selection on the grid.
    if let Some(i) = m.selected {
        if m.boxed != Some(i) {
            d.values.insert("appliesTo".into(), json!(m.rules(app, sheet).get(i).map(applies_to_text).unwrap_or_default()));
            m.boxed = Some(i);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let l = ui.label(lang.tr("Applies to"));
            ref_edit(app, ui, d, "appliesTo", RefLabel::By(l.id), 320.0);
        });
        let text = d.values.get("appliesTo").and_then(Json::as_str).unwrap_or("").to_string();
        if let Some(ranges) = parse_applies_to(&text)
            && let Some(cf) = m.rules(app, sheet).get_mut(i)
            && cf.ranges != ranges
        {
            cf.ranges = ranges;
            m.changed.insert(sheet);
        }
    }

    ui.add_space(4.0);
    let mut apply = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(!m.changed.is_empty(), egui::Button::new(lang.tr("Apply"))).clicked() {
                apply = true;
            }
        });
    });
    ok_cancel(ui, confirm, open);
    if (apply || *confirm)
        && let Err(e) = m.apply(app)
    {
        app.message = Some((d.title.clone(), crate::clean_error(&e)));
        *confirm = false;
    }
    d.cf = Some(m);
}
