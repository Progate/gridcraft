//! File ▸ Info ▸ Protect Workbook (Always Open Read-Only, Encrypt with Password, Mark as Final)
//! and Save As ▸ Tools ▸ General Options (password to open, password to modify, read-only
//! recommended). Each setting is saved with the workbook the way Excel saves it; see
//! `gridcraft_xlsx::final_mark` and the `fileSharing` writer.

use serde_json::{Value as Json, json};

use super::super::{CommandSpec, Result, Session, bad, bool_param, cmd, edit, has_doc, str_param};

/// The Status Mark as Final sets when the caller names none (Japanese Excel writes 最終版; the
/// interface passes its own language's word).
const FINAL: &str = "Final";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "file.alwaysOpenReadOnly",
            "Always Open Read-Only",
            ["File", "Info", "Protect Workbook"],
            None,
            "{on?: bool} (default: toggles) → saved as <fileSharing readOnlyRecommended=\"1\"/>",
            has_doc,
            always_open_read_only
        ),
        cmd!(
            "file.markAsFinal",
            "Mark as Final",
            ["File", "Info", "Protect Workbook"],
            None,
            "{on?: bool (default: toggles), status?: \"Final\"} → the custom property _MarkAsFinal and the Status (cp:contentStatus)",
            has_doc,
            mark_as_final
        ),
        cmd!(
            "file.encryptWithPassword",
            "Encrypt with Password",
            ["File", "Info", "Protect Workbook"],
            None,
            "{password, confirmPassword?} (\"\" removes it): the workbook is saved encrypted (Agile, AES-256) and opens only with the password; {} opens the dialog",
            has_doc,
            encrypt_with_password
        ),
        cmd!(
            "file.generalOptions",
            "General Options",
            ["File", "Save As", "Tools"],
            None,
            "{openPassword?, modifyPassword?, readOnlyRecommended?: bool} (\"\" removes a password) → <fileSharing> and encryption on the next save; {} opens the dialog",
            has_doc,
            general_options
        ),
    ]
}

/// The state the interface shows: `{readOnlyRecommended, final, encrypted, modifyPassword}`.
pub fn state(s: &Session) -> Json {
    let Some(d) = s.active() else { return Json::Null };
    json!({
        "readOnlyRecommended": d.wb.read_only_recommended,
        "final": gridcraft_xlsx::is_marked_final(&d.wb),
        "encrypted": d.wb.open_password.is_some(),
        "modifyPassword": d.wb.modify_password.is_some(),
    })
}

fn always_open_read_only(s: &mut Session, p: &Json) -> Result<Json> {
    let on = bool_param(p, "on").unwrap_or(!s.doc()?.wb.read_only_recommended);
    edit(s, |cx| {
        cx.wb.read_only_recommended = on;
        Ok(json!({"on": on}))
    })
}

fn mark_as_final(s: &mut Session, p: &Json) -> Result<Json> {
    let on = bool_param(p, "on").unwrap_or(!gridcraft_xlsx::is_marked_final(&s.doc()?.wb));
    let status = str_param(p, "status").unwrap_or(FINAL).to_string();
    edit(s, |cx| {
        gridcraft_xlsx::set_marked_final(&mut cx.wb, on);
        if on {
            cx.wb.props.content_status = status;
        } else if [FINAL, "最終版", status.as_str()].contains(&cx.wb.props.content_status.as_str()) {
            cx.wb.props.content_status.clear();
        }
        Ok(json!({"on": on}))
    })?;
    if on {
        s.ui_requests.push(crate::UiRequest::Message(MARKED_FINAL.into()));
    }
    Ok(json!({"on": on}))
}

/// What Excel says once a workbook is marked as final.
pub const MARKED_FINAL: &str =
    "This workbook will be marked as final and then saved. It's marked as final to let readers know it's the final version and discourage editing.";
/// The notice a workbook marked as final opens with.
pub const OPENED_FINAL: &str = "MARKED AS FINAL: An author has marked this workbook as final to discourage editing.";
/// The notice a workbook saved with Always Open Read-Only opens with.
pub const OPENED_READ_ONLY_RECOMMENDED: &str = "The author would like you to open this workbook as read-only unless you need to make changes. Save changes as a new file if you don't need to edit the original.";

/// A new password, or `None` for "" (remove). Excel limits passwords to 255 characters.
fn password_param(p: &Json, key: &str, id: &str) -> Result<Option<Option<String>>> {
    match p.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::String(pw)) if pw.is_empty() => Ok(Some(None)),
        Some(Json::String(pw)) if pw.chars().count() > 255 => Err(bad(id, "a password can have at most 255 characters")),
        Some(Json::String(pw)) => Ok(Some(Some(pw.clone()))),
        Some(_) => Err(bad(id, format!("`{key}` must be text"))),
    }
}

fn encrypt_with_password(s: &mut Session, p: &Json) -> Result<Json> {
    let Some(pw) = password_param(p, "password", "file.encryptWithPassword")? else {
        s.ui_requests.push(crate::UiRequest::Dialog("encryptPassword".into(), json!({})));
        return Ok(Json::Null);
    };
    if let Some(confirm) = str_param(p, "confirmPassword")
        && confirm != pw.as_deref().unwrap_or("")
    {
        return Err(bad("file.encryptWithPassword", "The confirmation password is not identical."));
    }
    let on = pw.is_some();
    edit(s, |cx| {
        cx.wb.open_password = pw.map(gridcraft_model::Secret);
        Ok(json!({"encrypted": on}))
    })
}

fn general_options(s: &mut Session, p: &Json) -> Result<Json> {
    let id = "file.generalOptions";
    let open = password_param(p, "openPassword", id)?;
    let modify = password_param(p, "modifyPassword", id)?;
    let read_only = bool_param(p, "readOnlyRecommended");
    if open.is_none() && modify.is_none() && read_only.is_none() {
        s.ui_requests.push(crate::UiRequest::Dialog("generalOptions".into(), state(s)));
        return Ok(Json::Null);
    }
    let modify_hash = match &modify {
        Some(Some(pw)) => Some(Some(super::super::review::new_password_hash(s.doc()?, "fileSharing", pw))),
        Some(None) => Some(None),
        None => None,
    };
    let user = s.prefs.user_name.clone();
    edit(s, |cx| {
        if let Some(pw) = open {
            cx.wb.open_password = pw.map(gridcraft_model::Secret);
        }
        if let Some(h) = modify_hash {
            cx.wb.reservation_user = if h.is_some() { user } else { String::new() };
            cx.wb.modify_password = h;
        }
        if let Some(on) = read_only {
            cx.wb.read_only_recommended = on;
        }
        Ok(json!({
            "encrypted": cx.wb.open_password.is_some(),
            "modifyPassword": cx.wb.modify_password.is_some(),
            "readOnlyRecommended": cx.wb.read_only_recommended,
        }))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn session() -> Session {
        let mut s = Session::new();
        s.new_workbook();
        s
    }

    #[test]
    fn protect_workbook_settings_are_saved_like_excel() {
        let mut s = session();
        s.execute("file.alwaysOpenReadOnly", json!({})).unwrap();
        assert!(s.doc().unwrap().wb.read_only_recommended);
        s.execute("file.markAsFinal", json!({"status": "最終版"})).unwrap();
        assert!(gridcraft_xlsx::is_marked_final(&s.doc().unwrap().wb));
        assert_eq!(s.doc().unwrap().wb.props.content_status, "最終版");
        s.execute("edit.undo", json!({})).unwrap();
        assert!(!gridcraft_xlsx::is_marked_final(&s.doc().unwrap().wb));
        s.execute("file.markAsFinal", json!({"on": true})).unwrap();

        let bytes = crate::io::save_bytes(&s.doc().unwrap().wb, "Book.xlsx").unwrap();
        let (wb, _) = crate::io::open_bytes("Book.xlsx", &bytes).unwrap();
        assert!(wb.read_only_recommended);
        assert!(gridcraft_xlsx::is_marked_final(&wb));
        assert_eq!(wb.props.content_status, "Final");

        s.execute("file.markAsFinal", json!({})).unwrap();
        assert!(!gridcraft_xlsx::is_marked_final(&s.doc().unwrap().wb));
        assert_eq!(s.doc().unwrap().wb.props.content_status, "");
    }

    #[test]
    fn encrypt_with_password_saves_an_encrypted_file() {
        let mut s = session();
        s.execute("cell.set", json!({"cell": "A1", "input": "42"})).unwrap();
        s.execute("file.encryptWithPassword", json!({"password": "secret"})).unwrap();
        // The password is not journaled.
        assert!(!serde_json::to_string(&s.journal).unwrap().contains("secret"));
        let bytes = crate::io::save_bytes(&s.doc().unwrap().wb, "Book.xlsx").unwrap();
        assert_eq!(gridcraft_xlsx::sniff(&bytes), gridcraft_xlsx::Format::Encrypted);
        assert!(crate::io::open_bytes("Book.xlsx", &bytes).is_err());
        let (wb, _) = crate::io::open_bytes_with_password("Book.xlsx", &bytes, Some("secret")).unwrap();
        assert_eq!(wb.active().unwrap().value(gridcraft_core::CellRef::new(0, 0)), gridcraft_core::Value::Number(42.0));
        s.execute("file.encryptWithPassword", json!({"password": ""})).unwrap();
        let bytes = crate::io::save_bytes(&s.doc().unwrap().wb, "Book.xlsx").unwrap();
        assert_eq!(gridcraft_xlsx::sniff(&bytes), gridcraft_xlsx::Format::Xlsx);
        assert!(s.execute("file.encryptWithPassword", json!({"password": "a", "confirmPassword": "b"})).is_err());
        // No params: the dialog.
        s.execute("file.encryptWithPassword", json!({})).unwrap();
        assert!(s.ui_requests.iter().any(|r| matches!(r, crate::UiRequest::Dialog(n, _) if n == "encryptPassword")));
    }

    #[test]
    fn general_options_set_the_modify_password_and_read_only_recommendation() {
        let mut s = session();
        s.execute("file.generalOptions", json!({"modifyPassword": "edit", "readOnlyRecommended": true})).unwrap();
        let wb = &s.doc().unwrap().wb;
        assert!(wb.modify_password.as_ref().is_some_and(|h| h.verify("edit") == Ok(true)));
        let bytes = crate::io::save_bytes(wb, "Book.xlsx").unwrap();
        let (back, _) = crate::io::open_bytes("Book.xlsx", &bytes).unwrap();
        assert!(back.read_only_recommended && back.modify_password.is_some());
        s.execute("file.generalOptions", json!({"modifyPassword": ""})).unwrap();
        assert!(s.doc().unwrap().wb.modify_password.is_none());
        assert!(s.execute("file.generalOptions", json!({"openPassword": 5})).is_err());
    }
}
