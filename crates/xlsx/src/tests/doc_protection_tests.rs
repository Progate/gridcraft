//! File ▸ Info ▸ Protect Workbook: Always Open Read-Only, Mark as Final, Encrypt with Password,
//! and Save As ▸ General Options' password to modify.

use std::io::Read as _;

use gridcraft_model::{PasswordHash, Workbook};

use crate::{decrypt, encrypt, is_marked_final, read_xlsx, set_marked_final, write_xlsx};

fn part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

#[test]
fn read_only_recommended_and_password_to_modify_round_trip() {
    let mut wb = Workbook::new();
    wb.read_only_recommended = true;
    let bytes = write_xlsx(&wb).unwrap();
    let xml = part(&bytes, "xl/workbook.xml");
    assert!(xml.contains("<fileVersion appName=\"gridcraft\"/><fileSharing readOnlyRecommended=\"1\"/><workbookPr"), "{xml}");
    let (back, _) = read_xlsx(&bytes).unwrap();
    assert!(back.read_only_recommended && back.modify_password.is_none());

    wb.modify_password = Some(PasswordHash::new_sha512("edit", b"0123456789abcdef"));
    wb.reservation_user = "Sato & Co".into();
    let bytes = write_xlsx(&wb).unwrap();
    let xml = part(&bytes, "xl/workbook.xml");
    assert!(xml.contains("<fileSharing readOnlyRecommended=\"1\" userName=\"Sato &amp; Co\" algorithmName=\"SHA-512\" hashValue=\""), "{xml}");
    assert!(xml.contains("spinCount=\"100000\"/>"), "{xml}");
    let (back, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(back.modify_password, wb.modify_password);
    assert_eq!(back.reservation_user, "Sato & Co");
    assert!(back.modify_password.as_ref().is_some_and(|h| h.verify("edit") == Ok(true)));

    // Neither set: no element.
    let xml = part(&write_xlsx(&Workbook::new()).unwrap(), "xl/workbook.xml");
    assert!(!xml.contains("fileSharing"));
}

#[test]
fn mark_as_final_writes_the_custom_property_and_status() {
    let mut wb = Workbook::new();
    set_marked_final(&mut wb, true);
    wb.props.content_status = "最終版".into();
    let bytes = write_xlsx(&wb).unwrap();
    let custom = part(&bytes, "docProps/custom.xml");
    assert!(
        custom.contains(r#"<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="_MarkAsFinal"><vt:bool>true</vt:bool></property>"#),
        "{custom}"
    );
    let rels = part(&bytes, "_rels/.rels");
    assert!(rels.contains("relationships/custom-properties\"") && rels.contains("docProps/custom.xml\""), "{rels}");
    assert!(part(&bytes, "[Content_Types].xml").contains("application/vnd.openxmlformats-officedocument.custom-properties+xml"));
    assert!(part(&bytes, "docProps/core.xml").contains("<cp:contentStatus>最終版</cp:contentStatus>"));
    let (mut back, _) = read_xlsx(&bytes).unwrap();
    assert!(is_marked_final(&back));
    assert_eq!(back.props.content_status, "最終版");
    set_marked_final(&mut back, false);
    let bytes = write_xlsx(&back).unwrap();
    let (again, _) = read_xlsx(&bytes).unwrap();
    assert!(!is_marked_final(&again));
}

#[test]
fn an_encrypted_save_opens_with_its_password() {
    let b2 = gridcraft_core::CellRef::parse("B2").unwrap();
    let mut wb = Workbook::new();
    wb.sheet_mut(0).unwrap().set_value(b2, gridcraft_core::Value::Number(7.0));
    let file = encrypt(&write_xlsx(&wb).unwrap(), "パスワード1").unwrap();
    assert_eq!(crate::sniff(&file), crate::Format::Encrypted);
    assert!(decrypt(&file, "wrong").is_err());
    let (back, _) = read_xlsx(&decrypt(&file, "パスワード1").unwrap()).unwrap();
    assert_eq!(back.active().unwrap().value(b2), gridcraft_core::Value::Number(7.0));
}
