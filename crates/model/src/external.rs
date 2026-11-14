//! Links to other workbooks: the references `=[予算.xlsx]予算!B3` make, as Excel keeps them in a
//! file (`xl/externalLinks/externalLinkN.xml`): the linked file, its sheet names, and the values of
//! the cells the formulas read, which formulas show until the values are updated from the file.

use std::collections::BTreeMap;

use gridcraft_core::{Array, CellError, CellRef, RangeRef, Value, split_external};
use serde::{Deserialize, Serialize};

/// Most cells one reference into another workbook reads (one full column's worth).
const MAX_EXTERNAL_CELLS: u64 = 1_048_576;

/// A linked workbook.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExternalLink {
    /// The linked file as the file's relationship names it: `予算.xlsx` (beside this workbook),
    /// `file:///C:\会計\予算.xlsx`, `/home/me/予算.xlsx`.
    pub target: String,
    /// Its sheets, in its order, with the values of the cells this workbook's formulas read.
    pub sheets: Vec<ExternalSheet>,
}

/// A sheet of a linked workbook and the values cached from it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExternalSheet {
    pub name: String,
    #[serde(with = "crate::cellmap", default)]
    pub cells: BTreeMap<CellRef, Value>,
}

/// Whether `dir` starts with a drive letter (`C:\`, `C:/`).
fn drive(dir: &str) -> bool {
    let b = dir.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

impl ExternalLink {
    /// A link to the workbook a formula names by folder (`""` beside this workbook, or
    /// `C:\会計\`) and file name.
    pub fn new(dir: &str, book: &str) -> ExternalLink {
        let target = if dir.is_empty() {
            book.to_string()
        } else if drive(dir) {
            format!("file:///{dir}{book}")
        } else {
            format!("{dir}{book}")
        };
        ExternalLink { target, sheets: vec![] }
    }

    /// The folder (with its closing separator, or `""`) and file name, as formulas write them:
    /// `file:///C:/会計/予算.xlsx` → (`C:\会計\`, `予算.xlsx`).
    pub fn path(&self) -> (String, String) {
        let t = self.target.trim();
        let t = t.strip_prefix("file:///").or_else(|| t.strip_prefix("file://")).unwrap_or(t);
        let t = if drive(t) { t.replace('/', "\\") } else { t.to_string() };
        match t.rfind(['/', '\\']) {
            Some(i) => (t.get(..=i).unwrap_or("").to_string(), t.get(i + 1..).unwrap_or("").to_string()),
            None => (String::new(), t),
        }
    }

    /// The linked file's name (`予算.xlsx`).
    pub fn file_name(&self) -> String {
        self.path().1
    }

    /// The sheet name formulas use for `sheet` of this workbook: `[予算.xlsx]予算`, or with the
    /// folder `C:\会計\[予算.xlsx]予算`.
    pub fn sheet_ref(&self, sheet: &str) -> String {
        let (dir, book) = self.path();
        format!("{dir}[{book}]{sheet}")
    }

    /// Whether a formula's `dir[book]` names this workbook. The file names must match (in any
    /// case); the folders too when both are given.
    pub fn matches(&self, dir: &str, book: &str) -> bool {
        let (d, b) = self.path();
        b.to_lowercase() == book.trim().to_lowercase() && (dir.is_empty() || d.is_empty() || d.to_lowercase() == dir.to_lowercase())
    }

    pub fn sheet(&self, name: &str) -> Option<&ExternalSheet> {
        let name = name.to_lowercase();
        self.sheets.iter().find(|s| s.name.to_lowercase() == name)
    }

    pub fn sheet_mut(&mut self, name: &str) -> Option<&mut ExternalSheet> {
        let name = name.to_lowercase();
        self.sheets.iter_mut().find(|s| s.name.to_lowercase() == name)
    }
}

impl ExternalSheet {
    /// The cached values of `range`: one value, or an array the size of the range (cells not cached
    /// are blank). A range that runs to the sheet's edge (`A:A`) stops at the last cached cell.
    pub fn value(&self, range: RangeRef) -> Value {
        if range.is_single() {
            return self.cells.get(&range.start).cloned().unwrap_or(Value::Empty);
        }
        let to_edge = range.end.row == gridcraft_core::MAX_ROWS - 1 || range.end.col == gridcraft_core::MAX_COLS - 1;
        let range = if to_edge {
            let (mut r1, mut c1) = (range.start.row, range.start.col);
            for c in self.cells.keys().filter(|c| range.contains(**c)) {
                r1 = r1.max(c.row);
                c1 = c1.max(c.col);
            }
            RangeRef::new(range.start, CellRef::new(r1, c1))
        } else {
            range
        };
        let (h, w) = (u64::from(range.height()), u64::from(range.width()));
        if h * w > MAX_EXTERNAL_CELLS {
            return Value::Error(CellError::Num);
        }
        let data: Vec<Value> = range.iter().map(|c| self.cells.get(&c).cloned().unwrap_or(Value::Empty)).collect();
        Array::new(h as usize, w as usize, data).map(Value::from).unwrap_or(Value::Error(CellError::Value))
    }
}

/// The link of `links` a formula's `dir[book]` names: by number in a file's spelling (`[1]`), else
/// by folder and file name (the same folder first, then any link to a file of that name).
pub fn find_link(links: &[ExternalLink], dir: &str, book: &str) -> Option<usize> {
    if dir.is_empty()
        && !book.is_empty()
        && book.bytes().all(|b| b.is_ascii_digit())
        && let Ok(n) = book.parse::<usize>()
        && (1..=links.len()).contains(&n)
    {
        return Some(n - 1);
    }
    let exact = links.iter().position(|l| {
        let (d, b) = l.path();
        d.to_lowercase() == dir.to_lowercase() && b.to_lowercase() == book.to_lowercase()
    });
    exact.or_else(|| links.iter().position(|l| l.matches(dir, book)))
}

impl crate::Workbook {
    /// The link a formula's `dir[book]` names ([`find_link`]).
    pub fn external_link(&self, dir: &str, book: &str) -> Option<usize> {
        find_link(&self.external_links, dir, book)
    }

    /// The value a reference to another workbook's sheet (`[予算.xlsx]予算`, as `split_external`
    /// reads it) gives from the cached values: `None` when this workbook has no such link or the
    /// linked workbook no such sheet, which a formula shows as `#REF!`.
    pub fn external_value(&self, sheet_name: &str, range: RangeRef) -> Option<Value> {
        let (dir, book, sheet) = split_external(sheet_name)?;
        let link = self.external_links.get(self.external_link(dir, book)?)?;
        Some(link.sheet(sheet)?.value(range))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_read_like_excels_formulas() {
        let l = ExternalLink { target: "file:///C:/会計/予算.xlsx".into(), sheets: vec![] };
        assert_eq!(l.path(), ("C:\\会計\\".to_string(), "予算.xlsx".to_string()));
        assert_eq!(l.sheet_ref("予算"), "C:\\会計\\[予算.xlsx]予算");
        assert!(l.matches("", "予算.XLSX") && l.matches("c:\\会計\\", "予算.xlsx") && !l.matches("D:\\", "予算.xlsx"));
        let l = ExternalLink::new("", "予算 2025.xlsx");
        assert_eq!(l.target, "予算 2025.xlsx");
        assert_eq!(l.sheet_ref("予算"), "[予算 2025.xlsx]予算");
        assert_eq!(ExternalLink::new("C:\\会計\\", "予算.xlsx").target, "file:///C:\\会計\\予算.xlsx");
        assert_eq!(ExternalLink::new("/home/me/", "予算.xlsx").path(), ("/home/me/".to_string(), "予算.xlsx".to_string()));
    }

    #[test]
    fn values_come_from_the_cache_by_name_or_number() {
        let mut wb = crate::Workbook::new();
        let mut sheet = ExternalSheet { name: "予算".into(), cells: BTreeMap::new() };
        sheet.cells.insert(CellRef::new(2, 1), Value::number(120.0));
        sheet.cells.insert(CellRef::new(3, 1), Value::number(30.0));
        wb.external_links.push(ExternalLink { target: "予算.xlsx".into(), sheets: vec![sheet] });
        let b3 = RangeRef::cell(CellRef::new(2, 1));
        assert_eq!(wb.external_value("[予算.xlsx]予算", b3), Some(Value::number(120.0)));
        assert_eq!(wb.external_value("[1]予算", b3), Some(Value::number(120.0)));
        assert_eq!(wb.external_value("[予算.xlsx]予算", RangeRef::cell(CellRef::new(9, 9))), Some(Value::Empty));
        assert_eq!(wb.external_value("[予算.xlsx]実績", b3), None, "no such sheet");
        assert_eq!(wb.external_value("[実績.xlsx]予算", b3), None, "no such workbook");
        let Some(Value::Array(a)) = wb.external_value("[予算.xlsx]予算", RangeRef::new(CellRef::new(2, 1), CellRef::new(3, 1))) else { panic!() };
        assert_eq!(a.data.to_vec(), vec![Value::number(120.0), Value::number(30.0)]);
        // A whole column stops at the last cached cell.
        let Some(Value::Array(a)) = wb.external_value("[予算.xlsx]予算", RangeRef::cols(1, 1)) else { panic!() };
        assert_eq!(a.data.len(), 4);
    }
}
