//! Repeat (Excel's F4, and Ctrl+Y with nothing to redo): the last formatting, insert or delete
//! action again, on the current selection.

use serde_json::{Value as Json, json};

use super::super::{EngineError, Result, has_doc};
use crate::Session;

/// The commands Repeat runs again: the formatting, insert and delete actions Excel offers to
/// repeat. Their target defaults to the selection, so a repeat with the target left out applies to
/// wherever the person has moved.
const REPEATABLE: &[&str] = &[
    "home.bold",
    "home.italic",
    "home.underline",
    "home.doubleUnderline",
    "home.strikethrough",
    "home.superscript",
    "home.subscript",
    "home.fontName",
    "home.fontSize",
    "home.increaseFontSize",
    "home.decreaseFontSize",
    "home.fontColor",
    "home.fillColor",
    "home.borders",
    "home.formatCells",
    "home.alignLeft",
    "home.alignCenter",
    "home.alignRight",
    "home.justify",
    "home.alignTop",
    "home.alignMiddle",
    "home.alignBottom",
    "home.wrapText",
    "home.shrinkToFit",
    "home.increaseIndent",
    "home.decreaseIndent",
    "home.orientation",
    "home.mergeCenter",
    "home.mergeAcross",
    "home.mergeCells",
    "home.unmergeCells",
    "home.numberFormat",
    "home.accounting",
    "home.percent",
    "home.comma",
    "home.increaseDecimal",
    "home.decreaseDecimal",
    "home.cellStyle",
    "home.rowHeight",
    "home.columnWidth",
    "home.hideRows",
    "home.hideColumns",
    "home.insertRows",
    "home.insertColumns",
    "home.insertCells",
    "home.deleteRows",
    "home.deleteColumns",
    "home.deleteCells",
    "edit.clearAll",
    "edit.clearFormats",
    "edit.clearContents",
];

/// Params that name where the action ran; Repeat leaves them out so the command takes the selection.
const TARGET_PARAMS: &[&str] = &["range", "cell", "rows", "cols", "sheet"];

/// The command and params Repeat would run, if the last action can be repeated.
pub fn repeatable(s: &Session) -> Option<(&'static str, Json)> {
    let (id, params) = s.last_action.as_ref()?;
    let id = *REPEATABLE.iter().find(|r| **r == id)?;
    let params = match params {
        Json::Object(o) => {
            Json::Object(o.iter().filter(|(k, _)| !TARGET_PARAMS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        _ => json!({}),
    };
    Some((id, params))
}

/// The label of the action Repeat would run ("Bold", "Fill Color"), for the Repeat button.
pub fn repeat_label(s: &Session) -> Option<String> {
    let (id, _) = repeatable(s)?;
    crate::cmd::find_command(id).map(|c| c.label.trim_end_matches('…').to_string())
}

pub fn can_repeat(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    if s.active().is_some_and(|d| s.in_group(d.uid)) {
        return Err("finish or stop the running action first (automate.stopAction)".into());
    }
    repeatable(s).map(|_| ()).ok_or_else(|| "nothing to repeat".into())
}

/// Runs the last action again on the selection. The step it makes is the action's own ("Bold").
pub(super) fn repeat(s: &mut Session, _: &Json) -> Result<Json> {
    let (id, params) = repeatable(s).ok_or_else(|| EngineError::Disabled("edit.repeat".into(), "nothing to repeat".into()))?;
    let result = s.execute(id, params.clone())?;
    Ok(json!({"repeated": id, "params": params, "result": result}))
}
