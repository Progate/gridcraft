//! File ▸ Info ▸ Protect Workbook ▸ Mark as Final. Excel records it as a custom document property
//! in `docProps/custom.xml`:
//!
//! ```xml
//! <Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties"
//!     xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">
//!   <property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="_MarkAsFinal"><vt:bool>true</vt:bool></property>
//! </Properties>
//! ```
//!
//! and sets the Status core property (`cp:contentStatus`, [`gridcraft_model::DocProps::content_status`]).
//! GridCraft keeps a file's custom properties byte for byte (the pass-through part), so the mark
//! is read from and written into that part: other custom properties stay as they were.

use std::sync::Arc;

use gridcraft_model::Workbook;
use gridcraft_model::preserved::{Preserved, PreservedPart, PreservedRel};

const PART: &str = "docProps/custom.xml";
const REL_CUSTOM: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
const CT_CUSTOM: &str = "application/vnd.openxmlformats-officedocument.custom-properties+xml";
const NS_CUSTOM: &str = "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
const NS_VT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
/// The format id every user-defined custom property carries.
const FMTID_USER: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";
const NAME: &str = "_MarkAsFinal";

/// The custom-properties part the package root points at, if the workbook kept one.
fn custom_part(p: &Preserved) -> Option<usize> {
    let rel =
        p.package_rels.iter().find(|r| !r.external && r.rel_type.rsplit('/').next().is_some_and(|k| k.eq_ignore_ascii_case("custom-properties")))?;
    let target = rel.target.trim_start_matches('/');
    p.parts.iter().position(|q| q.name.trim_start_matches('/').eq_ignore_ascii_case(target))
}

/// The byte range of the `<property … name="_MarkAsFinal">…</property>` element, if any.
fn mark_span(xml: &str) -> Option<(usize, usize)> {
    let at = xml.find(&format!("name=\"{NAME}\"")).or_else(|| xml.find(&format!("name='{NAME}'")))?;
    let start = xml.get(..at)?.rfind('<')?;
    let end_tag = xml.get(at..)?.find("</")?;
    let close = xml.get(at + end_tag..).and_then(|rest| rest.find("property>")).map(|i| at + end_tag + i + "property>".len())?;
    // The closing tag found may belong to the property itself only if no other property starts first.
    Some((start, close))
}

/// Whether the workbook is marked as final: its custom properties hold `_MarkAsFinal` = true.
pub fn is_marked_final(wb: &Workbook) -> bool {
    let Some(p) = wb.preserved.as_deref() else { return false };
    let Some(i) = custom_part(p) else { return false };
    let Some(part) = p.parts.get(i) else { return false };
    let Ok(xml) = std::str::from_utf8(&part.data) else { return false };
    mark_span(xml).and_then(|(s, e)| xml.get(s..e)).is_some_and(|el| {
        let value = el.split('>').nth(2).unwrap_or("");
        let value = value.split('<').next().unwrap_or("").trim();
        value.eq_ignore_ascii_case("true") || value == "1"
    })
}

/// The highest `pid` in a custom-properties part (user properties start at 2).
fn max_pid(xml: &str) -> u32 {
    xml.match_indices("pid=\"").filter_map(|(i, m)| xml.get(i + m.len()..)?.split('"').next()?.parse::<u32>().ok()).max().unwrap_or(1)
}

/// Marks the workbook as final (`on`) or clears the mark, in its custom properties. A part with
/// no property left is removed with its relationship.
pub fn set_marked_final(wb: &mut Workbook, on: bool) {
    let p = Arc::make_mut(wb.preserved.get_or_insert_with(Default::default));
    let existing = custom_part(p);
    let mut xml = existing.and_then(|i| p.parts.get(i)).and_then(|q| std::str::from_utf8(&q.data).ok()).map(str::to_string).unwrap_or_else(|| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Properties xmlns=\"{NS_CUSTOM}\" xmlns:vt=\"{NS_VT}\"></Properties>"
        )
    });
    if let Some((s, e)) = mark_span(&xml) {
        xml.replace_range(s..e, "");
    }
    if on {
        let pid = max_pid(&xml).saturating_add(1).max(2);
        let vt = if xml.contains(&format!("xmlns:vt=\"{NS_VT}\"")) { String::new() } else { format!(" xmlns:vt=\"{NS_VT}\"") };
        let el = format!("<property fmtid=\"{FMTID_USER}\" pid=\"{pid}\" name=\"{NAME}\"><vt:bool{vt}>true</vt:bool></property>");
        match xml.rfind("</") {
            Some(at) => xml.insert_str(at, &el),
            None => return,
        }
    }
    let empty = !xml.contains("property");
    match existing {
        Some(i) if empty => {
            if let Some(part) = p.parts.get(i) {
                let name = part.name.trim_start_matches('/').to_ascii_lowercase();
                p.package_rels.retain(|r| r.external || r.target.trim_start_matches('/').to_ascii_lowercase() != name);
            }
            p.parts.remove(i);
        }
        Some(i) => {
            if let Some(part) = p.parts.get_mut(i) {
                part.data = xml.into_bytes().into();
            }
        }
        None if empty => {}
        None => {
            p.parts.push(PreservedPart { name: PART.into(), content_type: CT_CUSTOM.into(), data: xml.into_bytes().into(), rels: vec![] });
            p.package_rels.push(PreservedRel { id: "rIdCustom".into(), rel_type: REL_CUSTOM.into(), target: PART.into(), external: false });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_as_final_adds_and_clears_the_property() {
        let mut wb = Workbook::new();
        assert!(!is_marked_final(&wb));
        set_marked_final(&mut wb, true);
        assert!(is_marked_final(&wb));
        let p = wb.preserved.as_deref().unwrap();
        let xml = std::str::from_utf8(&p.parts[0].data).unwrap();
        assert!(xml.contains(r#"<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="_MarkAsFinal"><vt:bool>true</vt:bool></property></Properties>"#), "{xml}");
        // Marking twice keeps one property.
        set_marked_final(&mut wb, true);
        let p = wb.preserved.as_deref().unwrap();
        assert_eq!(std::str::from_utf8(&p.parts[0].data).unwrap().matches("_MarkAsFinal").count(), 1);
        set_marked_final(&mut wb, false);
        assert!(!is_marked_final(&wb));
        let p = wb.preserved.as_deref().unwrap();
        assert!(p.parts.is_empty() && p.package_rels.is_empty());
    }

    #[test]
    fn other_custom_properties_are_kept() {
        let mut wb = Workbook::new();
        let xml = format!(
            "<Properties xmlns=\"{NS_CUSTOM}\" xmlns:vt=\"{NS_VT}\"><property fmtid=\"{FMTID_USER}\" pid=\"2\" name=\"Client\"><vt:lpwstr>ACME</vt:lpwstr></property></Properties>"
        );
        wb.preserved = Some(Arc::new(Preserved {
            package_rels: vec![PreservedRel { id: "rId4".into(), rel_type: REL_CUSTOM.into(), target: PART.into(), external: false }],
            parts: vec![PreservedPart { name: PART.into(), content_type: CT_CUSTOM.into(), data: xml.into_bytes().into(), rels: vec![] }],
            ..Default::default()
        }));
        set_marked_final(&mut wb, true);
        let p = wb.preserved.as_deref().unwrap();
        let got = std::str::from_utf8(&p.parts[0].data).unwrap();
        assert!(got.contains("name=\"Client\"") && got.contains("pid=\"3\" name=\"_MarkAsFinal\""), "{got}");
        assert!(is_marked_final(&wb));
        set_marked_final(&mut wb, false);
        let p = wb.preserved.as_deref().unwrap();
        let got = std::str::from_utf8(&p.parts[0].data).unwrap();
        assert!(got.contains("name=\"Client\"") && !got.contains("_MarkAsFinal"), "{got}");
        assert_eq!(p.package_rels.len(), 1);
    }
}
