//! Dialogs. Simple dialogs are forms (fields → a command with params); Format Cells, Insert
//! Function, Find & Replace, Sort, Name Manager and the command palette are custom.
//!
//! Every dialog is also reachable programmatically: `ui.dialog {name}` opens one,
//! `ui.dialog.set {field, value}` edits a field and `ui.dialog.confirm` presses OK.

use egui::{Color32, Key, Sense, Stroke, StrokeKind, vec2};

use crate::icons::{self, Icon};
use gridcraft_engine::core::{CellRef, RangeRef};
use serde_json::{Map, Value as Json, json};

use crate::SheetApp;
use crate::theme;

mod about;
mod analysis;
mod autofilter;
mod cf_rule;
mod charts;
mod command_search;
mod cond_format_rules;
mod data;
mod find;
mod format_cells;
mod insert_function;
mod names;
mod page_setup;
mod paste;
mod ref_edit;
mod scenarios;
mod show;
mod spelling;
mod text_import;
mod text_to_columns;
mod zoom;

use analysis::toolpak_defaults;
use charts::select_chart;
use data::{abs, data_validation_fields, remove_duplicates_header, remove_duplicates_range, validation_defaults};
use find::go_to_entries;
pub(crate) use names::insert_name;
use names::pasteable_names;
pub(crate) use page_setup::PAPER_SIZES;
pub(crate) use ref_edit::SHEET_DELETED;
pub use show::build_params;
pub use show::show;
use zoom::zoom_preset;

/// Value Field Settings › Summarize value field by: `pivot.valueSettings` `func` values.
pub(crate) const PIVOT_FUNCS: [(&str, &str); 11] = [
    ("sum", "Sum"),
    ("count", "Count"),
    ("average", "Average"),
    ("max", "Max"),
    ("min", "Min"),
    ("product", "Product"),
    ("countNumbers", "Count Numbers"),
    ("stdDev", "StdDev"),
    ("stdDevP", "StdDevp"),
    ("var", "Var"),
    ("varP", "Varp"),
];

/// Value Field Settings › Show values as: `pivot.valueSettings` `showAs` values, in Excel's order.
pub(crate) const PIVOT_SHOW_AS: [(&str, &str); 11] = [
    ("normal", "No Calculation"),
    ("percentOfGrandTotal", "% of Grand Total"),
    ("percentOfColumnTotal", "% of Column Total"),
    ("percentOfRowTotal", "% of Row Total"),
    ("percentOfParentRowTotal", "% of Parent Row Total"),
    ("percentOfParentColumnTotal", "% of Parent Column Total"),
    ("runningTotal", "Running Total In"),
    ("percentOfRunningTotal", "% Running Total In"),
    ("rankAscending", "Rank Smallest to Largest"),
    ("rank", "Rank Largest to Smallest"),
    ("index", "Index"),
];

/// A form field. `Ref` is a range reference that can also be picked by selecting on the grid
/// (a RefEdit box).
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Text { key: &'static str, label: &'static str },
    Ref { key: &'static str, label: &'static str },
    Password { key: &'static str, label: &'static str },
    Number { key: &'static str, label: &'static str },
    Choice { key: &'static str, label: &'static str, options: Vec<(&'static str, &'static str)> },
    Check { key: &'static str, label: &'static str },
    Note(String),
}

#[derive(Clone, Debug)]
pub struct Dialog {
    pub name: String,
    pub title: String,
    pub fields: Vec<Field>,
    pub values: Map<String, Json>,
    /// Command to run on OK (params = values, transformed by `build`).
    pub command: Option<&'static str>,
    pub tab: String,
    pub search: String,
    pub list_index: usize,
    pub result: Option<Json>,
    /// "Show password" is ticked: password fields show their text.
    pub show_password: bool,
    /// The first text or number field still has to take keyboard focus (set when a form opens,
    /// used once): like Excel, Column Width opens ready to type, its value selected.
    focus_first_field: bool,
    picture_inbox: Option<crate::Inbox>,
    picture_target: Option<(u64, usize)>,
    /// The dialog this one was opened from (Name Manager › New…): it comes back when this closes.
    back: Option<Box<Dialog>>,
    /// Open Hyperlink: the address the prompt shows, which OK opens. Private, unlike `values`, so
    /// `ui.dialog.set` can't swap it behind the text the user reads (#2181).
    link: Option<String>,
    pub(crate) focus_initialized: bool,
    /// The formula box that takes references clicked or dragged on the grid (New Formatting Rule).
    pick: Option<RefPick>,
    /// The linked cell's initial display text; unchanged text must not replace a number or formula.
    original_link_text: Option<String>,
    /// The state of the dialog's reference boxes (RefEdit).
    refs: ref_edit::RefBoxes,
    /// The formula box of `pick` doesn't take references now (New Formatting Rule on a rule type
    /// without a reference box): a click on the grid selects as usual.
    pick_off: bool,
    /// The Conditional Formatting Rules Manager's working copy of the rules.
    cf: Option<Box<cond_format_rules::CfManager>>,
}

/// A formula box in point mode, as in Excel: a click on the grid writes the cell's absolute
/// reference at the caret instead of selecting it, and a drag grows it into a range.
#[derive(Clone, Debug)]
struct RefPick {
    key: &'static str,
    /// The sheet the dialog was opened on, by name: its index changes when sheets are inserted or
    /// moved while the dialog is open. References to another sheet carry that sheet's name.
    sheet: String,
    /// Caret (char index), mirrored from the text box while it has the keyboard.
    caret: usize,
    /// The reference last inserted (byte span), replaced by the next click or the drag.
    point: Option<(usize, usize)>,
    anchor: CellRef,
    /// Give the box the keyboard back, caret after the reference, on the next frame.
    refocus: bool,
    /// The selection the rule applies to, fixed when the dialog opened as Excel's "Applies to":
    /// pointing on another sheet must not move the rule there.
    applies_to: String,
}

impl Dialog {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sets a form value. In Zoom, a Magnification choice is also the percentage `view.zoom`
    /// stores, a typed percentage selects Custom, and Fit selection makes OK run
    /// `view.zoomToSelection` (its view size was captured when the dialog opened).
    pub(crate) fn set_value(&mut self, key: &str, value: Json) {
        // Custom Format… opens Format Cells, as in Excel's quick rule dialogs.
        if self.name == "cfQuick" && key == "preset" && value.as_str() == Some("custom") {
            self.values.insert("openFormat".into(), json!(true));
        }
        if self.name == "zoom" {
            match key {
                "preset" => match value.as_str() {
                    Some("fit") => self.command = Some("view.zoomToSelection"),
                    Some(s) => {
                        self.command = Some("view.zoom");
                        if let Ok(p) = s.parse::<f64>() {
                            self.values.insert("percent".into(), json!(p));
                        }
                    }
                    None => {}
                },
                "percent" => {
                    self.command = Some("view.zoom");
                    self.values.insert("preset".into(), json!(zoom_preset(value.as_f64().unwrap_or(0.0))));
                }
                _ => {}
            }
        }
        self.values.insert(key.into(), value);
    }

    /// The reference box the dialog is collapsed to, if any.
    pub fn collapsed(&self) -> Option<&str> {
        self.refs.collapsed.as_deref()
    }

    /// Collapses the dialog to its reference box `key` and starts picking into it, as the box's
    /// collapse button does.
    pub fn collapse_to(&mut self, app: &SheetApp, key: &str) {
        ref_edit::begin_pick(app, self, key);
        self.refs.collapsed = Some(key.to_string());
    }

    /// Ends picking into the dialog's reference boxes and puts back the sheet and selection from
    /// before picking began, as closing the dialog does. Fails when the dialog's sheet could not
    /// be shown again (a text-box draft on the picked sheet could not be saved): a dialog about
    /// to run its command must then stay open instead.
    pub(crate) fn end_pick(&mut self, app: &mut SheetApp) -> Result<(), String> {
        ref_edit::end_pick(app, self)
    }

    /// Whether the dialog's own sheet is protected, for a dialog with reference boxes and while
    /// that sheet exists (the sheet [`Self::end_pick`] shows again); `None` otherwise.
    pub(crate) fn home_sheet_protected(&self, app: &SheetApp) -> Option<bool> {
        ref_edit::home_sheet_protected(app, self)
    }

    /// Whether the sheet a dialog with reference boxes was opened on was deleted while it was
    /// open: OK is then refused with [`SHEET_DELETED`], since references without a sheet name
    /// have nothing to refer to.
    pub(crate) fn home_sheet_deleted(&mut self, app: &SheetApp) -> bool {
        ref_edit::home_sheet_deleted(app, self)
    }

    /// A press on the grid that selects cells: the reference box being picked into takes the
    /// selection on this frame even when it is unchanged (a click on the cell already selected).
    pub(crate) fn note_selecting_press(&mut self) {
        ref_edit::note_selecting_press(self);
    }

    /// Whether a reference box is the one grid selections fill: a click on the grid is then a
    /// pick, and a second click on the same cell does not edit the cell.
    pub(crate) fn ref_box_picks(&self) -> bool {
        ref_edit::picks(self)
    }

    /// Whether a click on the grid goes to a formula box of this dialog rather than the selection.
    pub fn picks_references(&self) -> bool {
        self.pick.is_some() && !self.pick_off
    }

    /// A click (`extend` false) or a drag reaching `cell`: writes the absolute reference at the
    /// formula box's caret, replacing the reference the press inserted, as in the formula bar's
    /// point mode. An empty box starts with `=`. After an operand there is no place for a
    /// reference, so nothing changes. `active_sheet` names the sheet shown when it is not the
    /// dialog's own.
    pub fn pick_reference(&mut self, cell: CellRef, active_sheet: &str, extend: bool) {
        if self.pick_off {
            return;
        }
        let Some(pick) = self.pick.as_mut() else { return };
        let text = self.values.get(pick.key).and_then(Json::as_str).unwrap_or("").to_string();
        let mut ed = crate::editor::EditState::new(0, 0, cell, text, true, false);
        ed.caret = pick.caret.min(ed.text.chars().count());
        ed.point = pick.point;
        if ed.text.trim().is_empty() {
            ed.text = "=".into();
            ed.caret = 1;
            ed.point = None;
        }
        if !ed.can_point() {
            return;
        }
        if !extend {
            pick.anchor = cell;
        }
        let range = abs(&RangeRef::new(pick.anchor, cell));
        let reference = if active_sheet.eq_ignore_ascii_case(&pick.sheet) {
            range
        } else {
            format!("{}!{range}", gridcraft_engine::formula::quote_sheet(active_sheet))
        };
        ed.insert_ref(&reference);
        pick.caret = ed.caret;
        pick.point = ed.point;
        pick.refocus = true;
        self.values.insert(pick.key.into(), json!(ed.text));
    }

    /// Creates a command-backed form with its initial field values.
    fn form(name: &str, title: &str, command: &'static str, fields: Vec<Field>, defaults: Json) -> Dialog {
        Dialog {
            name: name.into(),
            title: title.into(),
            fields,
            values: defaults.as_object().cloned().unwrap_or_default(),
            command: Some(command),
            tab: String::new(),
            search: String::new(),
            list_index: 0,
            result: None,
            show_password: false,
            focus_first_field: true,
            picture_inbox: None,
            picture_target: None,
            back: None,
            link: None,
            focus_initialized: false,
            pick: None,
            original_link_text: None,
            refs: ref_edit::RefBoxes::default(),
            pick_off: false,
            cf: None,
        }
    }

    /// Creates a dialog whose body handles its own controls instead of a standard form.
    pub(crate) fn custom(name: &str, title: &str, defaults: Json) -> Dialog {
        Dialog {
            name: name.into(),
            title: title.into(),
            fields: vec![],
            values: defaults.as_object().cloned().unwrap_or_default(),
            command: None,
            tab: String::new(),
            search: String::new(),
            list_index: 0,
            result: None,
            show_password: false,
            focus_first_field: false,
            picture_inbox: None,
            picture_target: None,
            back: None,
            link: None,
            focus_initialized: false,
            pick: None,
            original_link_text: None,
            refs: ref_edit::RefBoxes::default(),
            pick_off: false,
            cf: None,
        }
    }

    /// Opens the named dialog with defaults from the active workbook, including a cell's stored hyperlink.
    pub fn open(app: &mut SheetApp, name: &str, params: Json) -> Option<Dialog> {
        let sel = app.session.active().map(|d| d.selection.current().a1()).unwrap_or_default();
        let active = app.session.active().map(|d| d.selection.active.a1()).unwrap_or_default();
        let p = |k: &str| params.get(k).cloned().unwrap_or(Json::Null);
        use Field::*;
        let mut d = match name {
            "insertCells" => Dialog::form(
                "insertCells",
                "Insert",
                "home.insertCells",
                vec![Choice {
                    key: "shift",
                    label: "Insert",
                    options: vec![("right", "Shift cells right"), ("down", "Shift cells down"), ("row", "Entire row"), ("column", "Entire column")],
                }],
                json!({"shift": "down"}),
            ),
            "arrangeAll" => Dialog::form(
                "arrangeAll",
                "Arrange Windows",
                "view.arrangeAll",
                vec![
                    Choice {
                        key: "arrange",
                        label: "Arrange",
                        options: vec![("tiled", "Tiled"), ("horizontal", "Horizontal"), ("vertical", "Vertical"), ("cascade", "Cascade")],
                    },
                    Check { key: "activeWorkbook", label: "Windows of active workbook" },
                ],
                json!({"arrange": "tiled", "activeWorkbook": false}),
            ),
            "insertPaste" => Dialog::form(
                "insertPaste",
                "Insert Paste",
                "home.insertCopiedCells",
                vec![Choice { key: "shift", label: "Insert", options: vec![("right", "Shift cells right"), ("down", "Shift cells down")] }],
                json!({"shift": "down"}),
            ),
            "deleteCells" => Dialog::form(
                "deleteCells",
                "Delete",
                "home.deleteCells",
                vec![Choice {
                    key: "shift",
                    label: "Delete",
                    options: vec![("left", "Shift cells left"), ("up", "Shift cells up"), ("row", "Entire row"), ("column", "Entire column")],
                }],
                json!({"shift": "up"}),
            ),
            "rowHeight" => Dialog::form(
                "rowHeight",
                "Row Height",
                "home.rowHeight",
                vec![Number { key: "points", label: "Row height (points):" }],
                json!({"points": app.session.active().and_then(|d| d.wb.active().map(|s| s.row_height(d.selection.active.row) as f64 * 0.75)).unwrap_or(15.0)}),
            ),
            "columnWidth" => Dialog::form(
                "columnWidth",
                "Column Width",
                "home.columnWidth",
                vec![Number { key: "chars", label: "Column width (characters):" }],
                json!({"chars": app.session.active().and_then(|d| d.wb.active().map(|s| gridcraft_engine::cmd::format::points_to_chars(s.col_width(d.selection.active.col) as f64))).unwrap_or(8.43)}),
            ),
            "defaultWidth" => Dialog::form(
                "defaultWidth",
                "Standard Width",
                "home.defaultWidth",
                vec![Number { key: "width", label: "Standard column width (pixels):" }],
                json!({"width": 64}),
            ),
            "renameSheet" => Dialog::form(
                "renameSheet",
                "Rename Sheet",
                "sheet.rename",
                vec![Text { key: "name", label: "Name:" }],
                json!({"name": app.session.active().and_then(|d| d.wb.active().map(|s| s.name.clone()))}),
            ),
            "moveSheet" => {
                let sheets: Vec<String> = app.session.active().map(|d| d.wb.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
                let mut d = Dialog::form(
                    "moveSheet",
                    "Move or Copy",
                    "sheet.move",
                    vec![
                        Number { key: "to", label: "Before sheet (position, 0 = first):" },
                        Check { key: "copy", label: "Create a copy" },
                        Note(format!("Sheets: {}", sheets.join(", "))),
                    ],
                    json!({"to": 0, "copy": false}),
                );
                if let Some(s) = params.get("sheet") {
                    d.values.insert("sheet".into(), s.clone());
                }
                d
            }
            "protectSheet" => Dialog::form(
                "protectSheet",
                "Protect Sheet",
                "review.protectSheet",
                vec![
                    Password { key: "password", label: "Password (optional):" },
                    Note("Allow all users of this sheet to:".into()),
                    Check { key: "selectLockedCells", label: "Select locked cells" },
                    Check { key: "selectUnlockedCells", label: "Select unlocked cells" },
                    Check { key: "formatCells", label: "Format cells" },
                    Check { key: "formatColumns", label: "Format columns" },
                    Check { key: "formatRows", label: "Format rows" },
                    Check { key: "insertColumns", label: "Insert columns" },
                    Check { key: "insertRows", label: "Insert rows" },
                    Check { key: "deleteColumns", label: "Delete columns" },
                    Check { key: "deleteRows", label: "Delete rows" },
                    Check { key: "sort", label: "Sort" },
                    Check { key: "autofilter", label: "Use AutoFilter" },
                ],
                json!({"selectLockedCells": true, "selectUnlockedCells": true}),
            ),
            "allowEditRanges" => {
                let list = app.session.run("review.allowEditRanges", json!({})).unwrap_or(Json::Null);
                let rows: Vec<String> = list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|r| {
                                let lock = match (r["password"].as_bool(), r["unlocked"].as_bool()) {
                                    (Some(true), Some(true)) => " (password, unlocked)",
                                    (Some(true), _) => " (password)",
                                    _ => "",
                                };
                                format!("{}: {}{lock}", r["title"].as_str().unwrap_or(""), r["range"].as_str().unwrap_or(""))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let summary = if rows.is_empty() { "No ranges yet.".to_string() } else { rows.join("\n") };
                Dialog::form(
                    "allowEditRanges",
                    "Allow Edit Ranges",
                    "review.allowEditRanges",
                    vec![
                        Note(format!("Ranges that stay editable when the sheet is protected:\n{summary}")),
                        Choice {
                            key: "action",
                            label: "Action",
                            options: vec![("add", "New"), ("modify", "Modify"), ("delete", "Delete"), ("unlock", "Unlock with password")],
                        },
                        Text { key: "title", label: "Title:" },
                        Text { key: "range", label: "Refers to cells:" },
                        Text { key: "password", label: "Range password:" },
                        Text { key: "newTitle", label: "New title (Modify):" },
                        Check { key: "removePassword", label: "Remove the password (Modify)" },
                    ],
                    json!({"action": "add", "range": sel}),
                )
            }
            "protectWorkbook" => Dialog::form(
                "protectWorkbook",
                "Protect Structure and Windows",
                "review.protectWorkbook",
                vec![
                    Password { key: "password", label: "Password (optional):" },
                    Note("Protect workbook for: Structure (sheets can't be added, moved, renamed, hidden or deleted).".into()),
                ],
                json!({"on": true}),
            ),
            "encryptPassword" => Dialog::form(
                "encryptPassword",
                "Encrypt Document",
                "file.encryptWithPassword",
                vec![
                    Note("Encrypt the contents of this file".into()),
                    Password { key: "password", label: "Password:" },
                    Password { key: "confirmPassword", label: "Reenter password:" },
                    Note("Caution: If you lose or forget the password, it cannot be recovered. Leave the password empty to remove it.".into()),
                ],
                json!({"password": "", "confirmPassword": ""}),
            ),
            "generalOptions" => Dialog::form(
                "generalOptions",
                "General Options",
                "file.generalOptions",
                vec![
                    Password { key: "openPassword", label: "Password to open:" },
                    Password { key: "modifyPassword", label: "Password to modify:" },
                    Check { key: "readOnlyRecommended", label: "Read-only recommended" },
                ],
                json!({"openPassword": "", "modifyPassword": "", "readOnlyRecommended": p("readOnlyRecommended").as_bool().unwrap_or(false)}),
            ),
            "unprotectWorkbook" => Dialog::form(
                "unprotectWorkbook",
                "Unprotect Workbook",
                "review.protectWorkbook",
                vec![Password { key: "password", label: "Password:" }],
                json!({"on": false}),
            ),
            "openPassword" => {
                // The file rides along in the values (`path` on the desktop, `name` and `base64`
                // on the web) and goes back to `file.open` with the password.
                let source = p("path").as_str().or(p("name").as_str()).map(str::to_string).unwrap_or_default();
                let file = std::path::Path::new(&source).file_name().and_then(std::ffi::OsStr::to_str).unwrap_or("This workbook").to_string();
                // An empty password from the start, so OK on the untouched field says "incorrect"
                // rather than asking for the password again.
                let mut values = params.clone();
                values["password"] = json!("");
                Dialog::form(
                    "openPassword",
                    "Password",
                    "file.open",
                    vec![Note(format!("'{file}' is protected.")), Password { key: "password", label: "Password:" }],
                    values,
                )
            }
            "unprotectSheet" => Dialog::form(
                "unprotectSheet",
                "Unprotect Sheet",
                "review.unprotectSheet",
                vec![Password { key: "password", label: "Password:" }],
                json!({}),
            ),
            "unhideSheet" => {
                let opts: Vec<String> =
                    p("sheets").as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                let first = opts.first().cloned().unwrap_or_default();
                let mut d = Dialog::form(
                    "unhideSheet",
                    "Unhide",
                    "sheet.unhide",
                    vec![Text { key: "sheet", label: "Unhide sheet:" }, Note(format!("Hidden: {}", opts.join(", ")))],
                    json!({"sheet": first}),
                );
                d.values.insert("sheet".into(), json!(first));
                d
            }
            "pasteSpecial" => Dialog::form(
                "pasteSpecial",
                "Paste Special",
                "edit.pasteSpecial",
                vec![
                    Choice {
                        key: "what",
                        label: "Paste",
                        options: vec![
                            ("all", "All"),
                            ("formulas", "Formulas (F)"),
                            ("values", "Values (V)"),
                            ("formats", "Formats (T)"),
                            ("comments", "Comments and notes (C)"),
                            ("validation", "Validation"),
                            ("allExceptBorders", "All except borders"),
                            ("columnWidths", "Column widths"),
                            ("formulasAndNumberFormats", "Formulas and number formats"),
                            ("valuesAndNumberFormats", "Values and number formats"),
                        ],
                    },
                    Choice {
                        key: "operation",
                        label: "Operation",
                        options: vec![("none", "None"), ("add", "Add"), ("subtract", "Subtract"), ("multiply", "Multiply"), ("divide", "Divide")],
                    },
                    Check { key: "skipBlanks", label: "Skip blanks" },
                    Check { key: "transpose", label: "Transpose (E)" },
                    Check { key: "link", label: "Paste Link" },
                ],
                json!({"what": "all", "operation": "none"}),
            ),
            "series" => Dialog::form(
                "series",
                "Series",
                "edit.fillSeries",
                vec![
                    Choice { key: "direction", label: "Series in", options: vec![("columns", "Columns"), ("rows", "Rows")] },
                    Choice { key: "type", label: "Type", options: vec![("linear", "Linear"), ("growth", "Growth"), ("date", "Date")] },
                    Choice {
                        key: "dateUnit",
                        label: "Date unit",
                        options: vec![("day", "Day"), ("weekday", "Weekday"), ("month", "Month"), ("year", "Year")],
                    },
                    Number { key: "step", label: "Step value:" },
                    Text { key: "stop", label: "Stop value:" },
                ],
                json!({
                    "direction": if p("direction") == "rows" { "rows" } else { "columns" },
                    // A date in the active cell starts the dialog on Date, as in Excel.
                    "type": if series_starts_with_date(app) { "date" } else { "linear" },
                    "dateUnit": "day",
                    "step": 1
                }),
            ),
            "comment" => {
                let threaded = p("threaded").as_bool().unwrap_or(true);
                let existing = app
                    .session
                    .active()
                    .and_then(|d| d.wb.active().and_then(|s| s.comments.get(&d.selection.active).map(|c| c.text.clone())))
                    .unwrap_or_default();
                Dialog::form(
                    if threaded { "comment" } else { "note" },
                    if threaded { "New Comment" } else { "New Note" },
                    if threaded { "review.newComment" } else { "review.newNote" },
                    vec![Text { key: "text", label: if threaded { "Start a conversation:" } else { "Note:" } }],
                    json!({"text": existing}),
                )
            }
            // Asks before a web or e-mail link leaves the workbook. OK opens it through
            // `Session::open_confirmed_link` (which refuses anything but http, https and mailto),
            // not a command: `link.open` refuses web addresses for programmatic callers, and a
            // dialog without a command can't be confirmed over the control channel (#2181).
            "openLink" => {
                let url = params.get("url").and_then(Json::as_str).unwrap_or_default().to_string();
                let mut d = Dialog::custom("openLink", "Open Hyperlink", json!({"url": url}));
                d.fields = vec![Note(format!(
                    "This workbook asks to open the address below. Links can lead to harmful sites or start other programs: open it only if you trust the workbook.\n\n{url}"
                ))];
                d.link = Some(url);
                d
            }
            "insertLink" => {
                let mut d = Dialog::form(
                    "insertLink",
                    "Insert Hyperlink",
                    "insert.link",
                    vec![
                        Text { key: "text", label: "Display:" },
                        Text { key: "target", label: "Link to (web address, e-mail or Sheet!A1):" },
                        Text { key: "tooltip", label: "ScreenTip:" },
                    ],
                    json!({"target": "https://"}),
                );
                if let Some(doc) = app.session.active()
                    && let Some(sheet) = doc.wb.active()
                    && let Some(link) = sheet.hyperlinks.get(&doc.selection.active)
                {
                    let text = gridcraft_engine::display::cell_text(&doc.wb, sheet, doc.selection.active);
                    d.values.insert("target".into(), json!(link.target));
                    d.values.insert("text".into(), json!(text));
                    d.values.insert("tooltip".into(), json!(link.tooltip));
                    d.original_link_text = Some(text);
                    // As in Excel, the dialog on a linked cell edits that link.
                    d.title = "Edit Hyperlink".into();
                }
                d
            }
            "sparkline" => Dialog::form(
                "sparkline",
                "Create Sparklines",
                "insert.sparkline",
                vec![
                    Text { key: "range", label: "Data Range:" },
                    Text { key: "location", label: "Location Range:" },
                    Check { key: "markers", label: "Markers" },
                ],
                json!({"range": sel, "location": active, "type": p("type")}),
            ),
            "headerFooter" => Dialog::form(
                "headerFooter",
                "Header & Footer",
                "pageLayout.headerFooter",
                vec![
                    Text { key: "header", label: "Header (&L left, &C center, &R right; &P page, &N pages, &D date, &A sheet):" },
                    Text { key: "footer", label: "Footer:" },
                ],
                {
                    let sheet = app.session.active().and_then(|d| d.wb.active());
                    let (header, footer) = sheet.map(|s| (s.print.header.clone(), s.print.footer.clone())).unwrap_or_default();
                    json!({"header": header, "footer": footer})
                },
            ),
            "pageSetup" => page_setup::open(app, p("tab").as_str()),
            "textToColumns" => {
                let mut d = Dialog::custom("textToColumns", "Convert Text to Columns Wizard - Step 1 of 3", text_to_columns::defaults(&active));
                d.command = Some("data.textToColumns");
                d
            }
            "removeDuplicates" => Dialog::form(
                "removeDuplicates",
                "Remove Duplicates",
                "data.removeDuplicates",
                vec![Text { key: "range", label: "Range:" }, Check { key: "header", label: "My data has headers" }],
                json!({"range": remove_duplicates_range(app, &sel), "header": remove_duplicates_header(app), "unchecked": []}),
            ),
            "advancedFilter" => {
                let list = app
                    .session
                    .active()
                    .and_then(|d| d.wb.active().map(|sh| gridcraft_engine::selection::current_region(sh, d.selection.active).a1()))
                    .unwrap_or(sel.clone());
                let mut d = Dialog::form(
                    "advancedFilter",
                    "Advanced Filter",
                    "data.advancedFilter",
                    vec![],
                    json!({"action": "inPlace", "range": list, "criteria": "", "copyTo": "", "unique": false}),
                );
                for key in ["action", "range", "criteria", "copyTo", "unique"] {
                    if let Some(value) = params.get(key) {
                        d.set_value(key, value.clone());
                    }
                }
                d
            }
            "equation" => return crate::equation::open_editor(app, &params),
            "goalSeek" => Dialog::custom("goalSeek", "Goal Seek", json!({"set": active, "to": "0", "changing": ""})),
            "scenarioManager" => scenarios::open(app),
            "customAutoFilter" => autofilter::open_custom(app, &params)?,
            "top10AutoFilter" => autofilter::open_top10(app, &params)?,
            "forecastSheet" => Dialog::form(
                "forecastSheet",
                "Create Forecast Worksheet",
                "data.forecastSheet",
                vec![
                    Text { key: "timeline", label: "Timeline range (blank: first column of the selection):" },
                    Text { key: "values", label: "Values range (blank: second column of the selection):" },
                    Text { key: "end", label: "Forecast end (date or number; blank: a third of the history):" },
                    Text { key: "forecastStart", label: "Forecast start (a date or number in the history; blank: after the last point):" },
                    Check { key: "confidenceInterval", label: "Confidence interval" },
                    Text { key: "confidence", label: "Confidence level (%):" },
                    Text { key: "seasonality", label: "Seasonality (auto, 0 for none, or a season length):" },
                    Choice {
                        key: "fillMissing",
                        label: "Fill missing points using",
                        options: vec![("interpolate", "Interpolation"), ("zeros", "Zeros")],
                    },
                    Choice {
                        key: "aggregate",
                        label: "Aggregate duplicates using",
                        options: vec![
                            ("average", "Average"),
                            ("count", "Count"),
                            ("counta", "CountA"),
                            ("max", "Max"),
                            ("median", "Median"),
                            ("min", "Min"),
                            ("sum", "Sum"),
                        ],
                    },
                    Check { key: "statistics", label: "Include forecast statistics" },
                    Choice { key: "chart", label: "Chart", options: vec![("line", "Line chart"), ("column", "Column chart")] },
                ],
                json!({"confidenceInterval": true, "confidence": "95", "seasonality": "auto", "fillMissing": "interpolate", "aggregate": "average", "chart": "line"}),
            ),
            "analysisToolPak" => {
                // Opened with a known `tool`, it goes straight to that tool's form.
                let tool = params.get("tool").and_then(Json::as_str).unwrap_or("");
                let mut d = Dialog::custom("analysisToolPak", "Data Analysis", json!({"tool": "", "pick": tool}));
                if gridcraft_engine::cmd::analysis::TOOLS.iter().any(|(id, _)| *id == tool) {
                    d.values = toolpak_defaults(&sel, tool);
                }
                d.command = Some("data.analysisToolPak");
                d
            }
            "solver" => {
                // Starts from the sheet's saved model, else the active cell as objective; fields
                // given when opening it go over either.
                let saved = app.session.active().and_then(|doc| gridcraft_engine::cmd::solver::saved_model(&doc.wb, doc.wb.active_sheet()));
                let mut values = json!({"objective": active, "goal": "max", "value": "0", "variables": "", "constraints": [], "nonNegative": true, "method": "grg"});
                for m in [saved.as_ref(), Some(&params)].into_iter().flatten().filter_map(Json::as_object) {
                    if let Some(v) = values.as_object_mut() {
                        for (k, x) in m {
                            v.insert(
                                k.clone(),
                                if k == "value" { json!(x.as_str().map_or_else(|| x.to_string(), str::to_string)) } else { x.clone() },
                            );
                        }
                    }
                }
                if let Some(v) = values.as_object_mut() {
                    v.insert("newOp".into(), json!("<="));
                }
                let options = values.get("optionsView").and_then(Json::as_bool) == Some(true);
                let mut d = Dialog::custom("solver", "Solver Parameters", values);
                d.command = Some("data.solver");
                // Straight into the Solver Options: the same setup as the Options button, so the
                // fields show their defaults and Cancel restores what was there.
                if options {
                    analysis::open_solver_options(&mut d);
                }
                d
            }
            "solverResults" => Dialog::custom("solverResults", "Solver Results", params.clone()),
            "subtotal" => Dialog::form(
                "subtotal",
                "Subtotal",
                "data.subtotal",
                vec![
                    Text { key: "groupBy", label: "At each change in (column letter):" },
                    Choice {
                        key: "function",
                        label: "Use function",
                        options: vec![
                            ("sum", "Sum"),
                            ("count", "Count"),
                            ("average", "Average"),
                            ("max", "Max"),
                            ("min", "Min"),
                            ("product", "Product"),
                        ],
                    },
                    Text { key: "columnsText", label: "Add subtotal to (columns, e.g. C,D):" },
                ],
                json!({"groupBy": "A", "function": "sum"}),
            ),
            "outlineSettings" => {
                let (below, right) =
                    app.session.active().and_then(|d| d.wb.active().map(|s| (s.summary_below, s.summary_right))).unwrap_or((true, true));
                Dialog::form(
                    "outlineSettings",
                    "Settings",
                    "data.outlineSettings",
                    vec![
                        Check { key: "summaryBelow", label: "Summary rows below detail" },
                        Check { key: "summaryRight", label: "Summary columns to right of detail" },
                    ],
                    json!({"summaryBelow": below, "summaryRight": right}),
                )
            }
            "zoom" => {
                // The current zoom is pre-selected (Custom when it matches no preset). Fit
                // selection needs the grid's size, so it is captured here for `view.zoomToSelection`.
                let zoom = app.session.active().and_then(|d| d.wb.active().map(|s| s.zoom)).unwrap_or(100);
                let view = app.grid.cells_rect.unwrap_or(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1200.0, 700.0)));
                Dialog::form(
                    "zoom",
                    "Zoom",
                    "view.zoom",
                    vec![],
                    json!({"percent": zoom, "preset": zoom_preset(zoom as f64), "viewWidth": view.width(), "viewHeight": view.height()}),
                )
            }
            "defineName" => {
                // Name Manager › Edit… passes the name being edited (`oldName`, `oldScope`) and
                // its current fields; New… and Define Name start from the selection.
                let editing = params.get("oldName").and_then(Json::as_str).is_some();
                let selection = app.session.active().and_then(|d| {
                    d.wb.active().map(|s| format!("={}!{}", gridcraft_engine::formula::quote_sheet(&s.name), abs(&d.selection.current())))
                });
                let mut defaults = json!({"scope": "Workbook", "refersTo": selection});
                for key in ["name", "scope", "comment", "refersTo", "oldName", "oldScope"] {
                    if let Some(v) = params.get(key).filter(|v| v.is_string()) {
                        defaults[key] = v.clone();
                    }
                }
                Dialog::form(
                    "defineName",
                    if editing { "Edit Name" } else { "New Name" },
                    "formulas.defineName",
                    vec![
                        Text { key: "name", label: "Name:" },
                        Text { key: "scope", label: "Scope (Workbook or sheet name):" },
                        Text { key: "comment", label: "Comment:" },
                        Text { key: "refersTo", label: "Refers to:" },
                    ],
                    defaults,
                )
            }
            "tableResize" => {
                let (table, range) = app
                    .session
                    .active()
                    .and_then(|d| {
                        let sh = d.wb.active()?;
                        let t = sh.table_at(d.selection.active)?;
                        Some((t.name.clone(), t.range.a1()))
                    })
                    .unwrap_or_default();
                Dialog::form(
                    "tableResize",
                    "Resize Table",
                    "table.resize",
                    vec![Text { key: "range", label: "New range for the table:" }],
                    json!({"table": table, "range": range}),
                )
            }
            "dataValidation" => Dialog::form(
                "dataValidation",
                "Data Validation",
                "data.validation",
                data_validation_fields("any", "between"),
                validation_defaults(app),
            ),
            "cfQuick" => cf_rule::quick(app, &params),
            "newFormattingRule" => cf_rule::open(app, &params),
            "formatCells" => {
                let mut d = Dialog::custom("formatCells", "Format Cells", json!({}));
                if p("dxf").as_bool() == Some(true) {
                    // A conditional format's own format (Format… in a rule dialog): only what a
                    // rule may set, handed back to the dialog that opened it.
                    d.values.insert("dxf".into(), json!(true));
                    let style = p("style");
                    let style =
                        if style.is_object() { style } else { serde_json::to_value(gridcraft_engine::model::Style::default()).unwrap_or(Json::Null) };
                    d.values.insert("style".into(), style);
                    d.tab = p("tab").as_str().unwrap_or("Font").to_string();
                    return Some(d);
                }
                d.tab = p("tab").as_str().unwrap_or("Number").to_string();
                if let Ok(st) = app.session.run("cell.get", json!({})) {
                    d.values.insert("style".into(), st["style"].clone());
                }
                d
            }
            "insertFunction" => {
                // A Function Library menu opens it on its own category.
                let category = p("category").as_str().unwrap_or(crate::function_library::MOST_RECENTLY_USED).to_string();
                let mut d = Dialog::custom("insertFunction", "Insert Function", json!({"category": category}));
                d.focus_first_field = true;
                d
            }
            "find" => {
                let mut d = Dialog::custom("find", "Find and Replace", json!({"what": "", "with": "", "within": "sheet", "lookIn": "formulas"}));
                d.tab = if p("replace").as_bool() == Some(true) { "Replace".into() } else { "Find".into() };
                d
            }
            "goTo" => {
                let mut dialog = Dialog::custom("goTo", "Go To", json!({"reference": ""}));
                // Listed once, when the dialog opens.
                dialog.result = Some(json!(go_to_entries(app)));
                dialog
            }
            "pivotCalculatedField" => Dialog::form(
                "pivotCalculatedField",
                "Insert Calculated Field",
                "pivot.calculatedField",
                vec![Text { key: "name", label: "Name:" }, Text { key: "formula", label: "Formula:" }],
                json!({"pivot": p("pivot"), "name": p("name"), "formula": p("formula")}),
            ),
            "textImport" => {
                // Excel titles the preview with the file's name.
                let title = params
                    .get("name")
                    .and_then(Json::as_str)
                    .or(params.get("path").and_then(Json::as_str))
                    .map(|n| std::path::Path::new(n).file_name().and_then(std::ffi::OsStr::to_str).unwrap_or(n).to_string());
                let mut d = Dialog::custom("textImport", &title.unwrap_or_else(|| "From Text/CSV".into()), params.clone());
                d.command = Some("data.fromTextCsv");
                d
            }
            "pivotGroup" => {
                // Excel's Grouping dialog: Auto ▸ Starting at / Ending at and By for numbers, By for dates.
                let fields = if p("kind").as_str() == Some("date") {
                    vec![Choice {
                        key: "by",
                        label: "Grouping|By",
                        options: vec![
                            ("days", "Grouping|Days"),
                            ("months", "Grouping|Months"),
                            ("quarters", "Grouping|Quarters"),
                            ("years", "Grouping|Years"),
                        ],
                    }]
                } else {
                    vec![
                        Note("Auto".into()),
                        Check { key: "autoStart", label: "Grouping|Starting at:" },
                        Text { key: "start", label: "Grouping|Starting at:" },
                        Check { key: "autoEnd", label: "Grouping|Ending at:" },
                        Text { key: "end", label: "Grouping|Ending at:" },
                        Text { key: "interval", label: "Grouping|By:" },
                    ]
                };
                let mut values = json!({"pivot": p("pivot"), "field": p("field"), "kind": p("kind")});
                for k in ["by", "autoStart", "autoEnd", "start", "end", "interval", "originalStart", "originalEnd"] {
                    if !p(k).is_null() {
                        values[k] = p(k).clone();
                    }
                }
                if p("kind").as_str() != Some("date") {
                    values["by"] = json!("range");
                }
                Dialog::form("pivotGroup", "PivotTable|Grouping", "pivot.group", fields, values)
            }
            "pivotValueSettings" => {
                let source = p("sourceName").as_str().unwrap_or_default().to_string();
                Dialog::form(
                    "pivotValueSettings",
                    "Value Field Settings",
                    "pivot.valueSettings",
                    vec![
                        Note(format!("Source Name: {source}")),
                        Text { key: "name", label: "Custom Name:" },
                        Choice { key: "func", label: "Summarize value field by", options: PIVOT_FUNCS.to_vec() },
                        Choice { key: "showAs", label: "Show values as", options: PIVOT_SHOW_AS.to_vec() },
                        Note("Running Total In, % Running Total In and the ranks run along the first row field.".into()),
                        Text { key: "numberFormat", label: "Number Format (blank = automatic):" },
                    ],
                    json!({
                        "pivot": p("pivot"),
                        "field": p("field"),
                        "name": p("name"),
                        "originalName": p("originalName"),
                        "func": p("func").as_str().unwrap_or("sum"),
                        "showAs": p("showAs").as_str().unwrap_or("normal"),
                        "numberFormat": p("numberFormat").as_str().unwrap_or_default(),
                    }),
                )
            }
            "goToSpecial" => Dialog::form(
                "goToSpecial",
                "Go To Special",
                "edit.goToSpecial",
                vec![Choice {
                    key: "kind",
                    label: "Select",
                    options: vec![
                        ("blanks", "Blanks"),
                        ("constants", "Constants"),
                        ("formulas", "Formulas"),
                        ("notes", "Notes"),
                        ("errors", "Errors"),
                        ("numbers", "Numbers"),
                        ("text", "Text"),
                        ("currentRegion", "Current region"),
                        ("lastCell", "Last cell"),
                        ("visible", "Visible cells only"),
                        ("conditionalFormats", "Conditional formats"),
                        ("dataValidation", "Data validation"),
                    ],
                }],
                json!({"kind": "blanks"}),
            ),
            "sort" => {
                // Seed "Sort by" from the column the user is actually in (#1906).
                let col = app.session.active().map(|d| gridcraft_engine::core::col_to_letters(d.selection.active.col)).unwrap_or_else(|| "A".into());
                Dialog::custom("sort", "Sort", json!({"header": true, "levels": [{"column": col, "order": "asc"}]}))
            }
            "nameManager" => Dialog::custom("nameManager", "Name Manager", json!({})),
            "pasteName" => {
                // F3 opens it only when the workbook has names to paste, as in Excel.
                let names = pasteable_names(app);
                if p("shortcut") == json!(true) && names.is_empty() {
                    return None;
                }
                // OK runs `formulas.pasteName` with the selected `name`, so `ui.dialog.confirm` does too.
                let mut dialog = Dialog::custom("pasteName", "Paste Name", json!({"name": names.first()}));
                dialog.command = Some("formulas.pasteName");
                // Listed once, when the dialog opens.
                dialog.result = Some(json!(names));
                dialog
            }
            "manageRules" => {
                let mut d = Dialog::custom("manageRules", "Conditional Formatting Rules Manager", json!({}));
                d.cf = Some(cond_format_rules::new_manager());
                d
            }
            "commandSearch" => Dialog::custom("commandSearch", "Search Commands", json!({})),
            "agents" => Dialog::custom("agents", "Agent Control", json!({})),
            "about" => {
                let mut d = Dialog::custom("about", "About GridCraft", json!({}));
                d.tab = match p("tab").as_str() {
                    Some(t @ ("Contributors" | "Models")) => t.to_string(),
                    _ => "About".into(),
                };
                d
            }
            "recommendedCharts" => {
                // Insert › Recommended Charts: chart types for the selected data, with previews.
                let recs = match app.session.run("insert.chartRecommendations", json!({})) {
                    Ok(r) => r,
                    Err(e) => {
                        app.message = Some(("Insert Chart".into(), crate::clean_error(&e)));
                        return None;
                    }
                };
                let mut d = Dialog::custom("recommendedCharts", "Insert Chart", json!({}));
                d.tab = if p("tab").as_str() == Some("All Charts") { "All Charts".into() } else { "Recommended Charts".into() };
                d.result = Some(recs);
                select_chart(&mut d, 0);
                d
            }
            "selectData" => {
                // Chart Design › Select Data: data range, series to keep, category axis labels.
                let id = p("chart").as_u64();
                let found = app.session.active().and_then(|doc| {
                    let si = doc.wb.active_sheet();
                    let sh = doc.wb.sheet(si)?;
                    let ch = sh.charts.iter().find(|c| Some(u64::from(c.id)) == id)?;
                    let data = gridcraft_chart::resolve(&doc.wb, si, ch);
                    let names: Vec<String> = data.series.iter().map(|x| x.name.clone()).collect();
                    let cats = ch.series.first().and_then(|x| x.categories.clone()).unwrap_or_default();
                    Some((ch.source.clone().unwrap_or_default(), cats, names))
                });
                let Some((range, cats, names)) = found else {
                    app.message = Some(("Select Data".into(), "Select a chart first.".into()));
                    return None;
                };
                let eq = |t: &str| if t.is_empty() { String::new() } else { format!("={t}") };
                Dialog::custom(
                    "selectData",
                    "Select Data Source",
                    json!({"chart": id, "range": eq(&range), "range0": eq(&range), "categories": eq(&cats), "categories0": eq(&cats), "names": names, "remove": []}),
                )
            }
            "comboChart" => {
                // Change Chart Type ▸ Combo: a chart type and a Secondary Axis box per series.
                let id = p("chart").as_u64();
                let rows = app.session.active().and_then(|doc| {
                    let si = doc.wb.active_sheet();
                    let ch = doc.wb.sheet(si)?.charts.iter().find(|c| Some(u64::from(c.id)) == id)?;
                    let data = gridcraft_chart::resolve(&doc.wb, si, ch);
                    let rows: Vec<Json> = ch
                        .series
                        .iter()
                        .enumerate()
                        .map(|(i, x)| {
                            let name = data.series.get(i).map(|s| s.name.clone()).unwrap_or_else(|| format!("Series{}", i + 1));
                            let secondary = ch.kind == gridcraft_chart::ChartKind::Combo && x.secondary;
                            let (t, sub) = charts::combo_row_type(gridcraft_chart::series_plot_kind(ch, i), i);
                            json!({"name": name, "type": t, "subtype": sub, "secondary": secondary})
                        })
                        .collect();
                    Some(rows)
                });
                let Some(rows) = rows.filter(|r| !r.is_empty()) else {
                    app.message = Some(("Change Chart Type".into(), "Select a chart first.".into()));
                    return None;
                };
                Dialog::custom("comboChart", "Change Chart Type", json!({"chart": id, "rows": rows}))
            }
            "altText" => {
                // Alt Text of a chart or picture: Title, Description and Mark as decorative.
                let (kind, id) = (p("kind"), p("id"));
                let current = app.session.run("object.altText", json!({"kind": kind, "id": id}));
                let current = match current {
                    Ok(v) => v,
                    Err(e) => {
                        app.message = Some(("Alt Text".into(), crate::clean_error(&e)));
                        return None;
                    }
                };
                Dialog::form(
                    "altText",
                    "Alt Text",
                    "object.altText",
                    vec![
                        Note("How would you describe this object and its context to someone who is blind or has low vision?".into()),
                        Text { key: "title", label: "Title:" },
                        Text { key: "description", label: "Description:" },
                        Check { key: "decorative", label: "Mark as decorative" },
                    ],
                    json!({"kind": kind, "id": id, "title": current["title"], "description": current["description"], "decorative": current["decorative"]}),
                )
            }
            "journal" => Dialog::custom("journal", "Action Journal", json!({})),
            "macros" => crate::macro_ui::open_dialog(app, &params),
            "recordMacro" => return crate::macro_ui::open_record(app, &params),
            "macroEditor" => return crate::macro_editor::open(app, &params),
            "statistics" => {
                let mut d = Dialog::custom("statistics", "Workbook Statistics", json!({}));
                d.result = app.session.run("review.workbookStatistics", json!({})).ok();
                d
            }
            "accessibility" => {
                let mut d = Dialog::custom("accessibility", "Accessibility", json!({}));
                d.result = app.session.run("review.checkAccessibility", json!({})).ok();
                d
            }
            "errorChecking" => {
                let mut d = Dialog::custom("errorChecking", "Error Checking", json!({}));
                d.result = app.session.run("formulas.errorChecking", json!({})).ok();
                d
            }
            "evaluateFormula" => {
                let mut d = Dialog::custom("evaluateFormula", "Evaluate Formula", json!({}));
                d.result = app.session.run("formulas.evaluateFormula", json!({})).ok();
                d
            }
            "spelling" => {
                let mut d = Dialog::custom("spelling", "Spelling", json!({}));
                d.result = Some(app.session.run("review.spelling", json!({})).unwrap_or_else(|e| json!({"error": e})));
                d
            }
            "chartTitle" => Dialog::form(
                "chartTitle",
                "Chart Title",
                "chart.set",
                vec![
                    Text { key: "title", label: "Title:" },
                    Text { key: "xTitle", label: "Horizontal axis title:" },
                    Text { key: "yTitle", label: "Vertical axis title:" },
                ],
                json!({"chart": p("chart")}),
            ),
            "insertPicture" => {
                let placement = params.get("placement").and_then(Json::as_str).unwrap_or("overCells");
                let mut dialog = Dialog::form(
                    "insertPicture",
                    if placement == "cell" { "Place Picture in Cell" } else { "Place Picture over Cells" },
                    "insert.picture",
                    vec![],
                    json!({"path": "", "placement": placement, "at": active}),
                );
                dialog.picture_target = app.view_key();
                dialog
            }
            "pickList" => Dialog::custom("pickList", "Pick From List", json!({})),
            "assignAction" => return crate::control_ui::open_assign(app, &params),
            "formatControl" => return crate::control_ui::open_format(app, &params),
            "controlQuestion" => return crate::control_ui::open_question(&params),
            "saveCopy" => Dialog::custom("saveCopy", "Save a Copy", json!({"format": p("format")})),
            "saveChanges" => Dialog::custom("saveChanges", "Save Changes?", json!({"title": p("title"), "quit": p("quit")})),
            "start" => Dialog::custom("start", "GridCraft", json!({})),
            "comments" => {
                app.grid.pane = None;
                app.run_or_alert("review.showComments", json!({"on": true}));
                return None;
            }
            "watch" | "selectionPane" | "formatChart" => {
                app.grid.pane = Some(
                    match name {
                        "watch" => "watch",
                        "selectionPane" => "selection",
                        _ => "formatChart",
                    }
                    .into(),
                );
                return None;
            }
            _ => return None,
        };
        ref_edit::record_home(app, &mut d);
        Some(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_duplicates_prefills_the_selection_and_submits_the_edited_range() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        session.execute("selection.set", json!({"range": "B2:C5"})).unwrap();
        let mut app = SheetApp::new(session, Default::default());
        let mut dialog = Dialog::open(&mut app, "removeDuplicates", json!({})).unwrap();
        assert_eq!(dialog.values.get("range"), Some(&json!("B2:C5")));
        assert!(dialog.fields.iter().any(|field| matches!(field, Field::Text { key: "range", .. })));
        assert_eq!(build_params(&dialog)["range"], "B2:C5");
        dialog.set_value("range", json!("D2:E6"));
        assert_eq!(build_params(&dialog)["range"], "D2:E6");
    }

    /// Right-click ▸ Group… on a number item opens Grouping with the field's smallest and largest
    /// values; typing a start turns its Auto box off, and OK groups the field into ranges.
    #[test]
    fn grouping_dialog_groups_numbers_into_ranges() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let rows = json!([["Score", "N"], [3, 1], [18, 1], [42, 1], [98, 1]]);
        session.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
        session.execute("selection.set", json!({"range": "A1"})).unwrap();
        session.execute("insert.pivotTable", json!({"rows": ["Score"], "values": ["N"]})).unwrap();
        session.execute("selection.set", json!({"range": "A4"})).unwrap();
        let mut app = SheetApp::new(session, Default::default());
        app.run("pivot.group", json!({})).unwrap();
        app.after_engine();
        let mut d = app.dialog.take().unwrap();
        assert_eq!(d.name(), "pivotGroup");
        assert_eq!((d.values["start"].as_str(), d.values["end"].as_str(), d.values["interval"].as_str()), (Some("3"), Some("98"), Some("10")));
        d.set_value("start", json!("0"));
        d.set_value("end", json!("100"));
        let params = build_params(&d);
        assert_eq!((params["autoStart"].as_bool(), params["autoEnd"].as_bool()), (Some(false), Some(false)), "{params}");
        assert!(params.get("originalStart").is_none() && params.get("kind").is_none(), "{params}");
        app.run(d.command.unwrap(), params).unwrap();
        let sh = app.session.active().unwrap().wb.active().unwrap().clone();
        let g = sh.pivots[0].rows[0].range_group.unwrap();
        assert_eq!((g.start, g.end, g.interval), (0.0, 100.0, 10.0));
        assert_eq!(sh.value(gridcraft_engine::core::CellRef::parse("A4").unwrap()).display(), "0-9");
    }

    /// The value field's menu opens Value Field Settings on the field; OK applies the choices and
    /// leaves an unchanged caption alone (renaming "Sum of Sales" to itself would be refused).
    #[test]
    fn value_field_settings_opens_on_the_field_and_applies_the_choices() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        session.execute("range.setValues", json!({"range": "A1", "values": [["Region", "Sales"], ["East", 1], ["West", 3]]})).unwrap();
        session.execute("selection.set", json!({"range": "A1"})).unwrap();
        session.execute("insert.pivotTable", json!({"rows": ["Region"], "values": ["Sales"]})).unwrap();
        let mut app = SheetApp::new(session, Default::default());
        app.run("pivot.valueSettings", json!({"field": "Sum of Sales"})).unwrap();
        app.after_engine();
        let mut d = app.dialog.take().unwrap();
        assert_eq!(d.name(), "pivotValueSettings");
        assert_eq!(
            (d.values["name"].as_str(), d.values["func"].as_str(), d.values["showAs"].as_str()),
            (Some("Sum of Sales"), Some("sum"), Some("normal"))
        );
        d.set_value("showAs", json!("percentOfColumnTotal"));
        d.set_value("func", json!("average"));
        let params = build_params(&d);
        assert!(params.get("name").is_none() && params.get("originalName").is_none(), "{params}");
        assert_eq!(params["numberFormat"], Json::Null);
        app.run(d.command.unwrap(), params).unwrap();
        let v = app.session.active().unwrap().wb.active().unwrap().pivots[0].values[0].clone();
        assert_eq!(
            (v.name.as_str(), v.func, v.show_as),
            ("Average of Sales", gridcraft_engine::model::PivotFunc::Average, gridcraft_engine::model::PivotShowAs::PercentOfColumnTotal)
        );
    }

    #[test]
    fn remove_duplicates_lists_columns_by_header_and_submits_the_ticked_ones() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        session.execute("range.setValues", json!({"range": "A1:C3", "values": [["Name", "", "City"], ["a", 1, "x"], ["a", 2, "x"]]})).unwrap();
        session.execute("selection.set", json!({"range": "A1:C3"})).unwrap();
        let mut app = SheetApp::new(session, Default::default());
        app.ui.language = crate::i18n::Language::Ja;
        let mut dialog = Dialog::open(&mut app, "removeDuplicates", json!({})).unwrap();
        let labels = |header| data::remove_duplicates_columns(&app, "A1:C3", header).into_iter().map(|(_, l)| l).collect::<Vec<_>>();
        assert_eq!(labels(true), ["Name", "列 B", "City"]);
        assert_eq!(labels(false), ["列 A", "列 B", "列 C"]);
        assert!(build_params(&dialog).get("columns").is_none(), "every column ticked compares all of them");
        dialog.set_value("unchecked", json!(["B"]));
        let params = build_params(&dialog);
        assert_eq!(params["columns"], json!(["A", "C"]));
        assert!(params.get("unchecked").is_none());
        let r = app.run("data.removeDuplicates", params).unwrap();
        assert_eq!(
            data::remove_duplicates_message(app.ui.language, &r),
            "1 個の重複する値が見つかり、削除されました。1 個の一意の値が残っています。"
        );
        assert_eq!(data::remove_duplicates_message(crate::i18n::Language::En, &json!({"removed": 0, "remaining": 2})), "No duplicate values found.");
    }

    #[test]
    fn data_validation_shows_stored_date_and_time_serials_as_dates() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        app.run("data.validation", json!({"range": "A1", "type": "date", "operator": "between", "formula1": "2026/4/1", "formula2": "=TODAY()"}))
            .unwrap();
        app.run("data.validation", json!({"range": "B1", "type": "time", "operator": "greater", "formula1": "9:00"})).unwrap();
        app.ui.language = crate::i18n::Language::Ja;
        let d = Dialog::open(&mut app, "dataValidation", json!({})).unwrap();
        assert_eq!((d.values["formula1"].as_str(), d.values["formula2"].as_str()), (Some("2026/4/1"), Some("=TODAY()")));
        // OK without edits stores the same rule.
        app.run("data.validation", build_params(&d)).unwrap();
        let a1 = gridcraft_engine::core::CellRef::new(0, 0);
        let dv =
            app.session.active().unwrap().wb.active().unwrap().validations.iter().find(|v| v.ranges.iter().any(|r| r.contains(a1))).cloned().unwrap();
        assert_eq!((dv.f1.as_str(), dv.f2.as_deref()), ("46113", Some("TODAY()")));
        app.ui.language = crate::i18n::Language::En;
        let d = Dialog::open(&mut app, "dataValidation", json!({})).unwrap();
        assert_eq!(d.values["formula1"], "4/1/2026");
        app.run("selection.set", json!({"range": "B1"})).unwrap();
        let d = Dialog::open(&mut app, "dataValidation", json!({})).unwrap();
        assert_eq!(d.values["formula1"], "9:00");
        // A list source formula reopens with its `=` and OK keeps it a formula; a literal list shows its items.
        app.run("data.validation", json!({"range": "C1", "type": "list", "formula1": "=INDIRECT($D1)"})).unwrap();
        app.run("data.validation", json!({"range": "C2", "type": "list", "formula1": "Open,\"Done\""})).unwrap();
        for (cell, shown, stored) in [("C1", "=INDIRECT($D1)", "INDIRECT($D1)"), ("C2", "Open,\"Done\"", "\"Open,\"\"Done\"\"\"")] {
            app.run("selection.set", json!({"range": cell})).unwrap();
            let d = Dialog::open(&mut app, "dataValidation", json!({})).unwrap();
            assert_eq!(d.values["formula1"], shown);
            app.run("data.validation", build_params(&d)).unwrap();
            let wb = &app.session.active().unwrap().wb;
            let at = gridcraft_engine::core::CellRef::parse(cell).unwrap();
            let dv = wb.active().unwrap().validations.iter().find(|v| v.ranges.iter().any(|r| r.contains(at))).unwrap();
            assert_eq!(dv.f1, stored, "{cell}");
        }
    }

    #[test]
    fn remove_duplicates_with_only_the_cursor_prefills_the_current_region() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        session.execute("range.setValues", json!({"range": "B2:C4", "values": [["a", 1], ["a", 1], ["b", 2]]})).unwrap();
        session.execute("selection.set", json!({"range": "C3"})).unwrap();
        let mut app = SheetApp::new(session, Default::default());
        let dialog = Dialog::open(&mut app, "removeDuplicates", json!({})).unwrap();
        assert_eq!(dialog.values.get("range"), Some(&json!("B2:C4")));
    }

    #[test]
    fn data_validation_opens_on_the_rule_of_the_active_cell() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        app.run("data.validation", json!({"range": "A1", "type": "whole", "operator": "greater", "formula1": "5"})).unwrap();
        let d = Dialog::open(&mut app, "dataValidation", json!({})).unwrap();
        assert_eq!(d.values["type"], "whole");
        assert_eq!(d.values["operator"], "greater");
        assert_eq!(d.values["formula1"], "5");
        // OK without edits keeps the rule: 6 is accepted, 4 is rejected.
        app.run(d.command.unwrap(), build_params(&d)).unwrap();
        let doc = app.session.active().unwrap();
        let at = gridcraft_engine::core::CellRef::new(0, 0);
        assert!(gridcraft_engine::cmd::data::check_validation(&doc.wb, 0, at, "6").is_none());
        assert!(gridcraft_engine::cmd::data::check_validation(&doc.wb, 0, at, "4").is_some());
    }

    /// #2181: a programmatic `link.open` opens nothing; the Open Hyperlink prompt has no command
    /// the control channel could confirm, and its OK opens exactly the address it showed.
    #[test]
    fn open_hyperlink_opens_only_the_address_the_user_confirmed() {
        use show::open_confirmed_link;
        use std::{cell::RefCell, rc::Rc};
        let opened = Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = opened.clone();
        let services = crate::Services { open_url: Some(Box::new(move |u: &str| sink.borrow_mut().push(u.to_string()))), ..Default::default() };
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let mut app = SheetApp::new(session, services);
        assert!(app.run("link.open", json!({"url": "https://attacker.example/?d=secret"})).is_err());
        app.after_engine();
        assert!(opened.borrow().is_empty(), "a programmatic link.open opens nothing");
        assert!(app.dialog.is_none(), "and opens no dialog");

        let mut d = Dialog::open(&mut app, "openLink", json!({"url": "https://example.org/a"})).unwrap();
        assert!(d.command.is_none(), "`ui.dialog.confirm` can't press OK for the user");
        // A value edited behind the prompt's text is ignored: OK opens what the user read.
        d.set_value("url", json!("https://attacker.example/?d=secret"));
        open_confirmed_link(&mut app, &d).unwrap();
        assert_eq!(*opened.borrow(), vec!["https://example.org/a".to_string()]);

        let bad = Dialog::open(&mut app, "openLink", json!({"url": "file:///etc/passwd"})).unwrap();
        assert!(open_confirmed_link(&mut app, &bad).is_err());
        assert_eq!(opened.borrow().len(), 1);
    }

    /// #2181: an OK that arrives with synthetic control-channel input (`ui.key` Enter, `ui.click`
    /// on OK) opens nothing and keeps the prompt open; the same OK from the user opens the link.
    #[test]
    fn synthetic_input_cannot_confirm_open_hyperlink() {
        use show::{SYNTHETIC_LINK_CONFIRM, confirm_open_link};
        use std::{cell::RefCell, rc::Rc};
        let opened = Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = opened.clone();
        let services = crate::Services { open_url: Some(Box::new(move |u: &str| sink.borrow_mut().push(u.to_string()))), ..Default::default() };
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let mut app = SheetApp::new(session, services);
        let d = Dialog::open(&mut app, "openLink", json!({"url": "https://example.org/a"})).unwrap();

        app.synthetic_pending = true;
        assert!(!confirm_open_link(&mut app, &d), "the prompt stays open");
        assert!(opened.borrow().is_empty(), "synthetic input opens nothing");
        assert_eq!(app.message.as_ref().map(|m| m.1.as_str()), Some(SYNTHETIC_LINK_CONFIRM));

        app.synthetic_pending = false;
        app.message = None;
        assert!(confirm_open_link(&mut app, &d), "the user's OK closes the prompt");
        assert_eq!(*opened.borrow(), vec!["https://example.org/a".to_string()]);
    }

    #[test]
    fn data_validation_without_a_rule_opens_on_a_blank_list() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        let d = Dialog::open(&mut app, "dataValidation", json!({})).unwrap();
        assert_eq!(d.values["type"], "list");
        assert!(d.values.get("formula1").is_none());
    }
}

/// Whether the active cell holds a date, so the Series dialog opens on the Date type.
fn series_starts_with_date(app: &SheetApp) -> bool {
    app.session.active().is_some_and(|d| {
        let at = d.selection.active;
        gridcraft_engine::fill::starts_with_date(&d.wb, d.wb.active_sheet(), RangeRef::new(at, at))
    })
}
