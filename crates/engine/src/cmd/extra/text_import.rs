//! Data ▸ Get Data ▸ From Text/CSV, as Excel's import preview does it: the file's encoding (File
//! Origin), its delimiter and Data Type Detection decide what loads; Load puts the data on a new
//! sheet of the current workbook as a table named after the file.

use std::collections::HashMap;

use gridcraft_model::{Cell, NumFmt, StyleId};
use gridcraft_xlsx::TextEncoding;

use super::{CellRef, EngineError, Json, RangeRef, Result, Session, Value, bad, edit, json, str_param};

/// Data Type Detection (データ型検出).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Detect {
    /// Based on the first 200 rows (the default).
    First200,
    /// Based on the entire dataset.
    All,
    /// Do not detect data types: every field stays text (`0101` keeps its zero).
    None,
}

impl Detect {
    fn parse(p: &Json) -> Result<Detect> {
        Ok(match str_param(p, "detect").unwrap_or("first200").to_ascii_lowercase().as_str() {
            "first200" | "200" | "rows" => Detect::First200,
            "all" | "dataset" | "entire" => Detect::All,
            "none" | "off" | "text" => Detect::None,
            d => return Err(bad("data.fromTextCsv", format!("unknown detect `{d}` (first200|all|none)"))),
        })
    }
    fn name(self) -> &'static str {
        match self {
            Detect::First200 => "first200",
            Detect::All => "all",
            Detect::None => "none",
        }
    }
}

/// Rows Excel looks at for Based on first 200 rows.
const DETECT_ROWS: u32 = 200;

/// The `encoding` param (a code page number or a name); `auto` detects.
pub(super) fn encoding_param(cmd: &str, p: &Json) -> Result<TextEncoding> {
    match str_param(p, "encoding").map(str::to_string).or_else(|| p.get("encoding").and_then(Json::as_u64).map(|n| n.to_string())) {
        Some(e) => TextEncoding::parse(&e).ok_or_else(|| bad(cmd, format!("unknown encoding `{e}` (auto|932|65001|1252)"))),
        None => Ok(TextEncoding::Auto),
    }
}

/// The `delimiter` param: a character or `comma`, `tab`, `semicolon`, `space`; `0` sniffs it.
pub(super) fn delimiter_param(cmd: &str, p: &Json) -> Result<u8> {
    let Some(d) = str_param(p, "delimiter") else { return Ok(0) };
    let b = match d.to_ascii_lowercase().as_str() {
        "" | "auto" => 0,
        "comma" => b',',
        "tab" | "\\t" => b'\t',
        "semicolon" => b';',
        "space" => b' ',
        _ => {
            let mut chars = d.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii() && c != '"' && c != '\n' && c != '\r' => c as u8,
                _ => return Err(bad(cmd, "the delimiter must be one ASCII character (or comma|tab|semicolon|space)")),
            }
        }
    };
    Ok(b)
}

/// One imported field: its value and, for a number read as a date or time, the number format.
type Field = (Value, Option<String>);

/// The records of a text file as they load: the header row first, each column typed when its
/// rows (the first 200, or all of them) all read as numbers, dates or booleans, else text.
/// Also returns the encoding and delimiter actually used.
pub(super) fn parse(
    bytes: &[u8],
    encoding: TextEncoding,
    delimiter: u8,
    detect: Detect,
    max_cells: u64,
) -> Result<(Vec<Vec<Field>>, TextEncoding, u8)> {
    let encoding = if encoding == TextEncoding::Auto { gridcraft_xlsx::detect_encoding(bytes) } else { encoding };
    let too_large = |e: gridcraft_xlsx::IoError| match e {
        gridcraft_xlsx::IoError::TooLarge(_) => EngineError::Other(format!("The data is too large to import ({max_cells} cells at most).")),
        e => EngineError::Other(e.to_string()),
    };
    let opts =
        |parse_values: bool| gridcraft_xlsx::CsvOptions { delimiter, encoding, parse_values, max_used_cells: Some(max_cells), ..Default::default() };
    let (raw_wb, _) = gridcraft_xlsx::read_csv_with_report(bytes, &opts(false)).map_err(too_large)?;
    let used_delimiter = raw_wb.csv_delimiter.unwrap_or(b',');
    let raw = raw_wb.sheet(0).ok_or(EngineError::NoDocument)?;
    let Some(used) = raw.used_range() else { return Ok((vec![], encoding, used_delimiter)) };
    let (rows, cols) = (used.end.row + 1, used.end.col + 1);
    let typed_wb = if detect == Detect::None { None } else { Some(gridcraft_xlsx::read_csv_with_report(bytes, &opts(true)).map_err(too_large)?.0) };
    let typed = typed_wb.as_ref().and_then(|wb| wb.sheet(0).map(|sh| (wb, sh)));
    let last = match detect {
        Detect::First200 => DETECT_ROWS.min(rows.saturating_sub(1)),
        _ => rows.saturating_sub(1),
    };
    // A column is typed when every non-empty field of the rows looked at parses.
    let typed_cols: Vec<bool> = (0..cols)
        .map(|c| {
            typed.is_some_and(|(_, sh)| {
                let mut any = false;
                for r in 1..=last {
                    match sh.value(CellRef::new(r, c)) {
                        Value::Empty => {}
                        Value::Text(_) | Value::Error(_) | Value::Array(_) => return false,
                        _ => any = true,
                    }
                }
                any
            })
        })
        .collect();
    let mut out = Vec::with_capacity(rows as usize);
    for r in 0..rows {
        let mut line = Vec::with_capacity(cols as usize);
        for c in 0..cols {
            let at = CellRef::new(r, c);
            let field = match typed {
                Some((wb, sh)) if r > 0 && typed_cols.get(c as usize).copied().unwrap_or(false) => {
                    let v = sh.value(at);
                    // A field past the rows looked at that doesn't parse keeps its text.
                    if matches!(v, Value::Text(_)) {
                        (raw.value(at), None)
                    } else {
                        let fmt = sh.cells.get(at).map(|cell| wb.styles.get(cell.style).num_fmt.as_str().to_string());
                        // A date loads as Japanese Excel shows its Date type (the short date 2025/4/1),
                        // whatever order the file wrote it in; times keep theirs.
                        let fmt = fmt.filter(|f| !f.eq_ignore_ascii_case("General")).map(|f| {
                            let l = f.to_ascii_lowercase();
                            if l.contains('y') && l.contains('d') && !l.contains('h') { "yyyy/m/d".to_string() } else { f }
                        });
                        (v, fmt)
                    }
                }
                _ => (raw.value(at), None),
            };
            line.push(field);
        }
        out.push(line);
    }
    Ok((out, encoding, used_delimiter))
}

fn delimiter_name(d: u8) -> String {
    match d {
        b',' => "comma".into(),
        b'\t' => "tab".into(),
        b';' => "semicolon".into(),
        b' ' => "space".into(),
        other => (other as char).to_string(),
    }
}

/// Rows the preview shows.
const PREVIEW_ROWS: usize = 20;
/// Bytes of a large file the preview reads (up to a line end).
const PREVIEW_BYTES: usize = 256 << 10;

/// `data.textPreview`: what From Text/CSV would load, for the preview dialog: the encoding and
/// delimiter used (detected unless given), the header and the first rows as shown.
pub(super) fn text_preview(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "data.textPreview";
    let bytes = source_bytes(C, p)?;
    let encoding = encoding_param(C, p)?;
    let delimiter = delimiter_param(C, p)?;
    let detect = Detect::parse(p)?;
    // A long file previews its beginning, cut at a line end so the last record is whole.
    let head = match bytes.get(..PREVIEW_BYTES) {
        Some(h) if bytes.len() > PREVIEW_BYTES => h.iter().rposition(|b| *b == b'\n').and_then(|i| h.get(..=i)).unwrap_or(h),
        _ => &bytes[..],
    };
    let (rows, used, used_delim) = parse(head, encoding, delimiter, detect, 10_000_000)?;
    let d = s.doc()?;
    let shown: Vec<Vec<String>> = rows
        .iter()
        .take(PREVIEW_ROWS + 1)
        .map(|line| {
            line.iter()
                .map(|(v, fmt)| match (v, fmt) {
                    (Value::Number(_), Some(f)) => crate::display::format(v, f, &d.wb).text,
                    _ => v.display(),
                })
                .collect()
        })
        .collect();
    Ok(json!({
        "encoding": used.code_page().unwrap_or(65001).to_string(),
        "delimiter": delimiter_name(used_delim),
        "detect": detect.name(),
        "rows": shown,
    }))
}

/// The file's bytes from `path`, `base64` or `text`.
pub(super) fn source_bytes(cmd: &str, p: &Json) -> Result<Vec<u8>> {
    if let Some(t) = str_param(p, "text") {
        Ok(t.as_bytes().to_vec())
    } else if let Some(b) = str_param(p, "base64") {
        crate::io::base64_decode(b).ok_or_else(|| bad(cmd, "invalid base64"))
    } else if let Some(path) = str_param(p, "path") {
        crate::io::read_file(path)
    } else {
        Err(bad(cmd, "missing `path`, `base64` or `text`"))
    }
}

/// A table name from a file name, as Excel makes one (`pos_2025.csv` → `pos_2025`): letters,
/// digits, `_` and `.`; a leading digit or a name like a cell reference gets a `_` in front.
fn table_name(stem: &str) -> String {
    let mut n: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '.' { c } else { '_' }).take(250).collect();
    if n.is_empty() || n.starts_with(|c: char| !(c.is_alphabetic() || c == '_')) || CellRef::parse(&n).is_some() {
        n.insert(0, '_');
    }
    n
}

/// Load: the records on a new sheet before the active one (named after the file), typed per
/// [`parse`], dates with their number format, as a table named after the file.
pub(super) fn load(s: &mut Session, p: &Json, bytes: &[u8], encoding: TextEncoding, max_cells: u64) -> Result<Json> {
    const C: &str = "data.fromTextCsv";
    let delimiter = delimiter_param(C, p)?;
    let detect = Detect::parse(p)?;
    let (rows, used, used_delim) = parse(bytes, encoding, delimiter, detect, max_cells)?;
    if rows.is_empty() || rows.len() > gridcraft_core::MAX_ROWS as usize {
        return Err(EngineError::Other("Nothing to import.".into()));
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let stem = str_param(p, "path")
        .or_else(|| str_param(p, "name"))
        .and_then(|x| std::path::Path::new(x).file_stem().and_then(std::ffi::OsStr::to_str))
        .map(str::to_string);
    let sheet_name = str_param(p, "sheet").map(str::to_string).or_else(|| stem.clone()).map(|n| n.chars().take(31).collect::<String>());
    let r = s.execute("home.insertSheet", json!({"name": sheet_name})).or_else(|_| s.execute("home.insertSheet", json!({})))?;
    let si = s.doc()?.wb.active_sheet();
    edit(s, |cx| {
        let mut styles: HashMap<String, StyleId> = HashMap::new();
        for (ri, line) in rows.into_iter().enumerate() {
            for (ci, (value, fmt)) in line.into_iter().enumerate() {
                if value.is_empty() {
                    continue;
                }
                let style = match fmt {
                    Some(f) => *styles.entry(f.clone()).or_insert_with(|| cx.wb.styles.derive(StyleId::DEFAULT, |st| st.num_fmt = NumFmt::new(&f))),
                    None => StyleId::DEFAULT,
                };
                let sheet = cx.sheet_mut(si)?;
                sheet.cells.set(CellRef::new(ri as u32, ci as u32), Cell { value, formula: None, style, py: None });
            }
        }
        cx.structural = true;
        Ok(())
    })?;
    let height = s.doc()?.wb.sheet(si).and_then(|sh| sh.used_range()).map_or(1, |u| u.end.row + 1);
    let range = RangeRef::new(CellRef::new(0, 0), CellRef::new(height - 1, width as u32 - 1));
    let name = str_param(p, "table").map(str::to_string).or_else(|| stem.as_deref().map(table_name));
    let table = s
        .execute("insert.table", json!({"range": range.a1(), "header": true, "name": name}))
        .or_else(|_| s.execute("insert.table", json!({"range": range.a1(), "header": true})))
        .ok()
        .and_then(|t| t.get("table").cloned());
    let _ = s.execute("home.autofitColumnWidth", json!({"cols": RangeRef::cols(0, width as u32 - 1).a1()}));
    let _ = s.execute("selection.set", json!({"range": "A1"}));
    Ok(json!({
        "sheet": r["name"],
        "table": table,
        "rows": height,
        "cols": width,
        "encoding": used.code_page().unwrap_or(65001).to_string(),
        "delimiter": delimiter_name(used_delim),
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;
    use gridcraft_core::CellRef;
    use gridcraft_xlsx::{TextEncoding, encode_with};

    const CSV: &str = "日付,店舗,商品コード,金額\r\n2025/04/01,駅前店,0101,8740\r\n2025/04/02,本店,0102,5500\r\n";

    fn session() -> Session {
        let mut s = Session::new();
        s.new_workbook();
        s
    }

    fn sjis_b64() -> String {
        crate::io::base64_encode(&encode_with(CSV, TextEncoding::ShiftJis))
    }

    #[test]
    fn from_text_csv_loads_a_shift_jis_file_into_this_workbook_as_a_table() {
        let mut s = session();
        let r = s.execute("data.fromTextCsv", json!({"name": "pos_2025.csv", "base64": sjis_b64()})).unwrap();
        assert_eq!((r["sheet"].as_str(), r["table"].as_str(), r["encoding"].as_str()), (Some("pos_2025"), Some("pos_2025"), Some("932")));
        let d = s.doc().unwrap();
        assert_eq!(d.wb.sheets.len(), 2, "a new sheet in the same workbook");
        let sh = d.wb.active().unwrap();
        let at = |a: &str| CellRef::parse(a).unwrap();
        assert_eq!(sh.value(at("B1")).display(), "店舗");
        assert_eq!(sh.value(at("B2")).display(), "駅前店");
        // Detected types: the code is a number, the date a date shown as a date.
        assert_eq!(sh.value(at("C2")), gridcraft_core::Value::Number(101.0));
        assert!(matches!(sh.value(at("A2")), gridcraft_core::Value::Number(_)));
        assert_eq!(crate::display::cell_text(&d.wb, sh, at("A2")), "2025/4/1", "a date shown as a date");
        assert_eq!(sh.tables[0].name, "pos_2025");
        assert_eq!(sh.tables[0].range.a1(), "A1:D3");
    }

    #[test]
    fn do_not_detect_keeps_every_field_as_text() {
        let mut s = session();
        s.execute("data.fromTextCsv", json!({"name": "codes.csv", "base64": sjis_b64(), "encoding": 932, "detect": "none"})).unwrap();
        let d = s.doc().unwrap();
        let sh = d.wb.active().unwrap();
        assert_eq!(sh.value(CellRef::parse("C2").unwrap()), gridcraft_core::Value::text("0101"));
        assert_eq!(sh.value(CellRef::parse("D2").unwrap()), gridcraft_core::Value::text("8740"));
    }

    #[test]
    fn preview_reports_the_detected_origin_and_follows_the_choices() {
        let mut s = session();
        let p = s.execute("data.textPreview", json!({"base64": sjis_b64()})).unwrap();
        assert_eq!((p["encoding"].as_str(), p["delimiter"].as_str()), (Some("932"), Some("comma")));
        assert_eq!(p["rows"][0][1], "店舗");
        assert_eq!(p["rows"][1][2], "101");
        let p = s.execute("data.textPreview", json!({"base64": sjis_b64(), "detect": "none", "encoding": "1252"})).unwrap();
        assert_ne!(p["rows"][0][1], "店舗");
        assert_eq!(p["rows"][1][2], "0101");
        let p = s.execute("data.textPreview", json!({"base64": sjis_b64(), "delimiter": "tab"})).unwrap();
        assert_eq!(p["rows"][0].as_array().map(Vec::len), Some(1));
        // Menu-style: the preview dialog.
        s.execute("data.fromTextCsv", json!({"name": "x.csv", "base64": sjis_b64(), "prompt": true})).unwrap();
        assert!(matches!(s.ui_requests.last(), Some(crate::UiRequest::Dialog(n, _)) if n == "textImport"));
    }
}
