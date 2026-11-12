//! File menu: new, open, save, close, export, properties, options.

use serde_json::{Value as Json, json};

use super::{CommandSpec, Result, Session, always, bad, bool_param, cmd, edit, has_doc, ok, str_param, target_sheet, u32_param};
use crate::DocState;

pub mod protect;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "file.new", "New Workbook", ["File"], Some("Cmd+N"), "{sample?: \"budget\"|\"sales\"|\"grades\"}", always, new_workbook),
        cmd!(noundo "file.open", "Open…", ["File"], Some("Cmd+O"), "{path} | {name, base64}, plus {password} for a password-protected workbook {csvAsText?: bool} to keep every csv/tsv/txt field as text, with no number or date conversion, and {encoding?: auto|932|65001|1252} for a csv/tsv/txt file's encoding (default: BOM, UTF-8, Shift_JIS when the bytes read as Japanese, else Windows-1252) (xlsx, xlsm, xls import, xlsb and ods data import, csv, tsv, txt, json)", always, open),
        cmd!(noundo "file.save", "Save", ["File"], Some("Cmd+S"), "{path?} (xlsx by default; .csv/.tsv/.json/.html/.pdf by extension)", has_doc, save),
        cmd!(noundo "file.saveAs", "Save As…", ["File"], Some("Cmd+Shift+S"), "{path} (.xlsx, .csv, .tsv, .json, .html, or .pdf: a PDF of the active sheet)", has_doc, save_as),
        cmd!(query "file.saveBytes", "Encode Workbook", [], None, "{format?: xlsx|csv|tsv|json|html, encoding?: utf-8|utf-8-bom|932|1252 (csv)} → {base64}", has_doc, save_bytes),
        cmd!(noundo "file.close", "Close", ["File"], Some("Cmd+W"), "{force?: bool, document?: bool} (closes the active window, or with `document` the workbook and all its windows; the workbook closes with its last window)", has_doc, close),
        cmd!(noundo "file.exportCsv", "Export as CSV", ["File", "Export"], None, "{path, sheet?, values?: bool, encoding?: utf-8|utf-8-bom|932|1252} → {path, bytes}: writes the cells as displayed, like Excel's Save As CSV (UTF-8 without a BOM unless `encoding` says: `utf-8-bom` is CSV UTF-8, 932 is Japanese CSV (Comma delimited)); `values: true` writes raw values instead (full-precision numbers, ISO 8601 dates and times, TRUE/FALSE)", has_doc, |s, p| export(s, p, "csv")),
        cmd!(noundo "file.exportHtml", "Save as Web Page", ["File", "Export"], None, "{path}", has_doc, |s, p| export(s, p, "html")),
        cmd!("file.properties", "Properties", ["File"], None, "{title?, subject?, author?, company?, keywords?, description?}", has_doc, properties),
        cmd!(
            query "file.options",
            "Options",
            ["File"],
            None,
            "{} → {calculation: {mode, iterative, maxIterations, maxChange, precisionAsDisplayed}} for the active workbook (the interface opens File › Options; change settings with their own commands, e.g. formulas.calculationOptions, view.theme, app.language.set)",
            always,
            options
        ),
        cmd!(
            noundo "app.region.set",
            "Regional Format",
            ["File", "Options"],
            None,
            "{region: \"ja-JP\" | \"en-US\" | any BCP 47 or system locale tag (ja_JP.UTF-8)} → {region, name}: the regional format typed entry is read in and built-in dates are shown in, like the operating system's regional settings Excel follows (ja-JP: 2026/11/10 and 11月10日 are dates, full-width digits are numbers, ¥ amounts; pivot and subtotal captions in Japanese). An application setting, not saved in the file; {} reports the current one",
            always,
            region_set
        ),
        cmd!(noundo "window.activate", "Switch Window", ["Window"], None, "{window} | {doc} | {index} (a window id, a workbook id, or the workbook's position in the list)", has_doc, activate_window),
        cmd!(noundo "window.close", "Close Window", ["Window"], None, "{window?, doc?, force?: bool} → {closed, document}: closes a window (the active one by default); its workbook closes with its last window, asking first when it has unsaved changes unless `force`", has_doc, close_window),
        cmd!(query "window.list", "Windows", ["Window"], None, "{} → [{window, doc, index, title, dirty, active, hidden}]", always, |s, _| Ok(s.window_list())),
    ]
    .into_iter()
    .chain(protect::specs())
    .collect()
}

fn new_workbook(s: &mut Session, p: &Json) -> Result<Json> {
    let i = match str_param(p, "sample") {
        Some(name) => {
            let wb = crate::sample::build(name).ok_or_else(|| bad("file.new", format!("unknown sample `{name}`")))?;
            let title = crate::sample::title(name);
            s.add_document(DocState::new(wb, None, title, crate::Trust::Local))
        }
        None => s.new_workbook(),
    };
    Ok(json!({"index": i, "title": s.doc()?.display_title()}))
}

fn open(s: &mut Session, p: &Json) -> Result<Json> {
    let (name, bytes) = if let Some(b) = str_param(p, "base64") {
        (str_param(p, "name").unwrap_or("Book.xlsx").to_string(), crate::io::base64_decode(b).ok_or_else(|| bad("file.open", "invalid base64"))?)
    } else if let Some(path) = str_param(p, "path") {
        // Already open? Switch to it.
        if let Some(i) = s.documents().iter().position(|d| d.path.as_deref() == Some(path)) {
            s.set_active(i);
            return Ok(json!({"index": i, "alreadyOpen": true}));
        }
        (path.to_string(), crate::io::read_file(path)?)
    } else {
        s.ui_requests.push(crate::UiRequest::Dialog("open".into(), json!({})));
        return ok();
    };
    let encrypted = gridcraft_xlsx::sniff(&bytes) == gridcraft_xlsx::Format::Encrypted;
    let encoding = match str_param(p, "encoding").map(str::to_string).or_else(|| p.get("encoding").and_then(Json::as_u64).map(|n| n.to_string())) {
        Some(e) => {
            gridcraft_xlsx::TextEncoding::parse(&e).ok_or_else(|| bad("file.open", format!("unknown encoding `{e}` (auto|932|65001|1252)")))?
        }
        None => gridcraft_xlsx::TextEncoding::Auto,
    };
    let text = crate::io::TextOpen { as_text: bool_param(p, "csvAsText").unwrap_or(false), encoding };
    let (mut wb, mut warnings) = crate::io::open_bytes_text(&name, &bytes, str_param(p, "password"), text)?;
    // Excel opens such a workbook with a message bar: marked as final, or read-only recommended.
    if wb.read_only_recommended {
        warnings.insert(0, protect::OPENED_READ_ONLY_RECOMMENDED.into());
    }
    if gridcraft_xlsx::is_marked_final(&wb) {
        warnings.insert(0, protect::OPENED_FINAL.into());
    }
    wb.source_dir = str_param(p, "path").and_then(crate::io::source_dir);
    // What `CELL("filename")` reports (#1489). A workbook opened from bytes knows its name but
    // never a folder, so it reports `[name]Sheet` with none; an import reports the file it came
    // from, as Excel does, even though Save must still ask where to put the .xlsx.
    wb.source_path = match str_param(p, "path") {
        Some(path) => crate::io::source_path(path),
        None => Some(std::path::PathBuf::from(&name)),
    };
    let imported =
        matches!(crate::io::FileKind::from_path(&name), Some(crate::io::FileKind::Ods | crate::io::FileKind::Xlsb | crate::io::FileKind::Xls))
            || matches!(gridcraft_xlsx::sniff(&bytes), gridcraft_xlsx::Format::Ods | gridcraft_xlsx::Format::Xlsb | gridcraft_xlsx::Format::Xls);
    let title = if imported {
        format!("{} (imported).xlsx", std::path::Path::new(&name).file_stem().and_then(std::ffi::OsStr::to_str).unwrap_or("Book"))
    } else {
        std::path::Path::new(&name).file_name().and_then(std::ffi::OsStr::to_str).unwrap_or("Book").to_string()
    };
    // An import has no save target: ordinary Save must ask for a new destination. Neither does a
    // workbook decrypted with its password, so Save never replaces the encrypted original with
    // an unprotected copy.
    let path = str_param(p, "path").filter(|_| !imported && !encrypted).map(str::to_string);
    // Replace an untouched blank Book1 like Excel does.
    if s.documents().len() == 1
        && s.active().is_some_and(|d| d.path.is_none() && !d.is_dirty() && d.undo.is_empty() && d.wb.sheets.iter().all(|sh| sh.cells.is_empty()))
    {
        s.close_document(0);
    }
    // Someone else may have made the file: its active content asks before it runs.
    let mut doc = DocState::new(wb, path, title, crate::Trust::FromFile);
    if crate::io::FileKind::from_path(&name).is_none() && gridcraft_xlsx::sniff(&bytes) == gridcraft_xlsx::Format::Xlsx {
        doc.content_kind = Some(crate::io::FileKind::Xlsx);
    }
    let i = s.add_document(doc);
    // The decrypted-workbook notice is for the info bar, not a message box.
    let info_bar = [crate::io::DECRYPTED_NOTICE, protect::OPENED_FINAL, protect::OPENED_READ_ONLY_RECOMMENDED];
    let shown: Vec<&str> = warnings.iter().map(String::as_str).filter(|w| !info_bar.contains(w)).collect();
    if !shown.is_empty() {
        let mut message = shown.iter().take(5).copied().collect::<Vec<_>>().join("\n\n");
        if shown.len() > 5 {
            message.push_str(&format!("\n\n{} additional import warnings.", shown.len() - 5));
        }
        s.ui_requests.push(crate::UiRequest::Message(message));
    }
    let d = s.doc()?;
    Ok(
        json!({"index": i, "title": d.display_title(), "sheets": d.wb.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), "warnings": warnings}),
    )
}

fn save(s: &mut Session, p: &Json) -> Result<Json> {
    let path = match str_param(p, "path").map(str::to_string).or_else(|| s.active().and_then(|d| d.path.clone())) {
        Some(p) => p,
        None => {
            s.ui_requests.push(crate::UiRequest::Dialog("saveAs".into(), json!({})));
            return ok();
        }
    };
    // A PDF is a print of the sheet (Page Setup applies), not a copy of the workbook: the
    // document keeps its file and its unsaved changes.
    if std::path::Path::new(&path).extension().and_then(std::ffi::OsStr::to_str).is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return super::print::export_pdf(s, &json!({"path": path}));
    }
    let d = s.doc_mut()?;
    // The document's own file keeps the format its content had when opened; any other path
    // goes by its extension. Relative paths to linked workbooks are rewritten for the new folder.
    let kind = crate::io::FileKind::from_path(&path).or(d.content_kind.filter(|_| d.path.as_deref() == Some(path.as_str())));
    let rebased = crate::io::source_dir(&path).and_then(|f| gridcraft_xlsx::rebase_links(&d.wb, &f));
    let wb = rebased.as_ref().unwrap_or(&d.wb);
    let bytes = match kind {
        Some(kind) => crate::io::save_bytes_as(wb, &path, kind)?,
        None => crate::io::save_bytes(wb, &path)?,
    };
    crate::io::write_file(&path, &bytes)?;
    // Only a full-fidelity format clears the dirty flag and becomes the document's file.
    if matches!(kind, Some(crate::io::FileKind::Xlsx | crate::io::FileKind::Json)) {
        // The new file is the workbook's own now, so `CELL("filename")` reports it (#1489). Set
        // before the `saved` snapshot, or the workbook would read as dirty the moment it was
        // saved; and only when it changes, so an ordinary Save does not clone the workbook.
        let file = crate::io::source_path(&path);
        if d.wb.source_path != file {
            std::sync::Arc::make_mut(&mut d.wb).source_path = file;
        }
        d.saved = d.wb.clone();
        d.path = Some(path.clone());
    }
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

fn save_as(s: &mut Session, p: &Json) -> Result<Json> {
    if str_param(p, "path").is_none() {
        s.ui_requests.push(crate::UiRequest::Dialog("saveAs".into(), json!({})));
        return ok();
    }
    save(s, p)
}

fn save_bytes(s: &mut Session, p: &Json) -> Result<Json> {
    let fmt = str_param(p, "format").unwrap_or("xlsx");
    let d = s.doc()?;
    let name = format!("{}.{fmt}", std::path::Path::new(&d.display_title()).file_stem().and_then(|x| x.to_str()).unwrap_or("Book"));
    // The file name goes into a PivotChart's link to its PivotTable.
    let bytes = crate::io::save_bytes(&d.wb, &name)?;
    let bytes = if fmt == "csv" { csv_encoding(p, bytes)? } else { bytes };
    Ok(json!({"name": name, "base64": crate::io::base64_encode(&bytes), "bytes": bytes.len()}))
}

/// File › Close closes the active window; the workbook closes with its last window.
fn close(s: &mut Session, p: &Json) -> Result<Json> {
    if !bool_param(p, "document").unwrap_or(false)
        && let Some(id) = s.active_window()
        && let Some(doc) = s.window(id).map(|w| w.doc_uid)
        && s.windows().iter().filter(|w| w.doc_uid == doc).count() > 1
    {
        s.close_window(id);
        return Ok(json!({"closed": true, "document": false}));
    }
    let force = bool_param(p, "force").unwrap_or(false);
    let d = s.doc()?;
    if d.is_dirty() && !force {
        s.ui_requests.push(crate::UiRequest::Dialog("saveChanges".into(), json!({"title": d.display_title()})));
        return Ok(json!({"closed": false, "dirty": true}));
    }
    let i = s.active_index();
    s.close_document(i);
    Ok(json!({"closed": true}))
}

fn export(s: &mut Session, p: &Json, ext: &str) -> Result<Json> {
    let path = str_param(p, "path").ok_or_else(|| bad("file.export", "missing `path`"))?.to_string();
    let d = s.doc()?;
    let wb = (*d.wb).clone();
    if p.get("sheet").is_some() {
        wb.set_active_sheet(target_sheet(s, p)?);
    }
    let target = if path.ends_with(&format!(".{ext}")) { path.clone() } else { format!("{path}.{ext}") };
    let bytes = if ext == "csv" && bool_param(p, "values").unwrap_or(false) {
        crate::io::csv_values_bytes(&wb)?
    } else {
        crate::io::save_bytes(&wb, &target)?
    };
    let bytes = if ext == "csv" { csv_encoding(p, bytes)? } else { bytes };
    crate::io::write_file(&target, &bytes)?;
    Ok(json!({"path": target, "bytes": bytes.len()}))
}

/// CSV text (UTF-8) in the `encoding` param: `utf-8-bom` (Excel's "CSV UTF-8 (Comma delimited)"),
/// `932` (Japanese Excel's "CSV (Comma delimited)", Shift_JIS), `1252`, or UTF-8 as it is.
fn csv_encoding(p: &Json, bytes: Vec<u8>) -> Result<Vec<u8>> {
    let Some(e) = str_param(p, "encoding").map(str::to_string).or_else(|| p.get("encoding").and_then(Json::as_u64).map(|n| n.to_string())) else {
        return Ok(bytes);
    };
    if matches!(e.to_ascii_lowercase().replace('_', "-").as_str(), "utf-8-bom" | "utf8-bom" | "utf-8-sig") {
        return Ok([&b"\xEF\xBB\xBF"[..], &bytes].concat());
    }
    let encoding =
        gridcraft_xlsx::TextEncoding::parse(&e).ok_or_else(|| bad("file.exportCsv", format!("unknown encoding `{e}` (utf-8|utf-8-bom|932|1252)")))?;
    Ok(gridcraft_xlsx::encode_with(&String::from_utf8_lossy(&bytes), encoding))
}

fn properties(s: &mut Session, p: &Json) -> Result<Json> {
    edit(s, |cx| {
        let pr = &mut cx.wb.props;
        for (k, slot) in [
            ("title", &mut pr.title),
            ("subject", &mut pr.subject),
            ("author", &mut pr.author),
            ("company", &mut pr.company),
            ("keywords", &mut pr.keywords),
            ("description", &mut pr.description),
        ] {
            if let Some(v) = str_param(p, k) {
                *slot = v.to_string();
            }
        }
        Ok(serde_json::to_value(&cx.wb.props).unwrap_or_default())
    })
}

/// File › Options: reports the workbook-level settings and asks the interface to show its Options
/// page. Nothing changes here; every setting has its own command.
fn options(s: &mut Session, _: &Json) -> Result<Json> {
    s.ui_requests.push(crate::UiRequest::Dialog("options".into(), json!({})));
    let calculation = s.active().map(|d| {
        let c = &d.wb.calc;
        let mode = match c.mode {
            gridcraft_model::CalcMode::Automatic => "automatic",
            gridcraft_model::CalcMode::AutomaticExceptTables => "automaticExceptTables",
            gridcraft_model::CalcMode::Manual => "manual",
        };
        json!({"mode": mode, "iterative": c.iterative, "maxIterations": c.max_iterations, "maxChange": c.max_change, "precisionAsDisplayed": c.precision_as_displayed})
    });
    Ok(json!({"calculation": calculation}))
}

fn region_set(s: &mut Session, p: &Json) -> Result<Json> {
    if let Some(tag) = str_param(p, "region").or_else(|| str_param(p, "locale")) {
        let tag = tag.trim();
        if tag.is_empty() {
            return Err(bad("app.region.set", "`region` is empty"));
        }
        s.set_region(gridcraft_core::region::lookup(tag));
    }
    let r = s.region();
    Ok(json!({"region": r.tag, "name": r.name}))
}

fn activate_window(s: &mut Session, p: &Json) -> Result<Json> {
    let id = match u32_param(p, "index").filter(|_| p.get("window").is_none_or(Json::is_null) && p.get("doc").is_none_or(Json::is_null)) {
        Some(i) => {
            let uid = s.documents().get(i as usize).map(|d| d.uid).ok_or_else(|| bad("window.activate", "no such window"))?;
            s.doc_window(uid).ok_or_else(|| bad("window.activate", "no such window"))?
        }
        None if p.get("window").is_none_or(Json::is_null) && p.get("doc").is_none_or(Json::is_null) => {
            return Err(bad("window.activate", "missing `window`"));
        }
        None => super::extra::window_param(s, "window.activate", p)?,
    };
    s.unhide_window(id);
    ok()
}

fn close_window(s: &mut Session, p: &Json) -> Result<Json> {
    let id = super::extra::window_param(s, "window.close", p)?;
    let doc = s.window(id).map(|w| w.doc_uid).unwrap_or_default();
    if s.windows().iter().filter(|w| w.doc_uid == doc).count() > 1 {
        s.close_window(id);
        return Ok(json!({"closed": true, "document": false}));
    }
    let force = bool_param(p, "force").unwrap_or(false);
    let d = s.documents().iter().find(|d| d.uid == doc).ok_or(crate::EngineError::NoDocument)?;
    if d.is_dirty() && !force {
        let title = d.display_title();
        s.activate_window(id);
        s.ui_requests.push(crate::UiRequest::Dialog("saveChanges".into(), json!({"title": title})));
        return Ok(json!({"closed": false, "dirty": true}));
    }
    s.close_window(id);
    Ok(json!({"closed": true, "document": true}))
}
