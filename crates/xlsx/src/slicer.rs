//! Slicers: the `xl/slicers/slicerN.xml` part of a sheet (its slicers), one
//! `xl/slicerCaches/slicerCacheN.xml` per slicer (what it filters), the workbook's `x15:slicerCaches`
//! extension and `Slicer_…` names, the sheet's `x15` slicer list and the drawing's `sle:slicer` frame.
//!
//! GridCraft's slicers filter a table column (Excel 2013's table slicers): the cache names the
//! table by its id and the column by its `tableColumn` id, and the selection itself lives in the
//! table's AutoFilter, as Excel keeps it.

use std::collections::HashSet;
use std::fmt::Write as _;

use gridcraft_model::{Anchor, Slicer};

use crate::xml::{El, esc_attr};

pub const CT_SLICER: &str = "application/vnd.ms-excel.slicer+xml";
pub const CT_SLICER_CACHE: &str = "application/vnd.ms-excel.slicerCache+xml";
pub const REL_SLICER: &str = "http://schemas.microsoft.com/office/2007/relationships/slicer";
pub const REL_SLICER_CACHE: &str = "http://schemas.microsoft.com/office/2007/relationships/slicerCache";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_X15: &str = "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
/// Workbook extension listing the table slicer caches.
const EXT_WORKBOOK_TABLE_CACHES: &str = "{46BE6895-7355-4a93-B00E-2C351335B9C9}";
/// Worksheet extension listing the slicer parts of table slicers.
pub const EXT_SHEET_TABLE_SLICERS: &str = "{3A4CF648-6AED-40f4-86FF-DC5316D8AED3}";
/// Slicer cache extension naming the table and column.
const EXT_TABLE_SLICER_CACHE: &str = "{2F2917AC-EB37-4324-AD4E-5DD8C200BD13}";
/// Height of one slicer button, in EMUs (0.25 inch, Excel's default).
const ROW_HEIGHT: u32 = 241_300;

/// A slicer as written: its unique names and what its cache points at.
pub struct Planned<'a> {
    pub slicer: &'a Slicer,
    /// The slicer's name (unique in the workbook), also the drawing frame's name.
    pub name: String,
    /// Its cache's name (`Slicer_…`, unique among the workbook's names).
    pub cache: String,
    pub table_id: u32,
    /// The `tableColumn` id of the filtered column (one-based position).
    pub column_id: u32,
}

/// Names used so far in the workbook being written: slicer names and cache names (which are also
/// defined names, so they must not clash with the workbook's own names).
#[derive(Default)]
pub struct Names {
    slicers: HashSet<String>,
    caches: HashSet<String>,
    /// Cache parts written, with their cache names, for the workbook's extension and names.
    pub written: Vec<(String, String)>,
}

impl Names {
    pub fn new(wb: &gridcraft_model::Workbook) -> Names {
        Names { caches: wb.names.iter().map(|n| n.name.to_lowercase()).collect(), ..Names::default() }
    }

    fn slicer_name(&mut self, caption: &str) -> String {
        let base = if caption.trim().is_empty() { "Slicer".to_string() } else { caption.trim().chars().take(200).collect() };
        let mut name = base.clone();
        let mut k = 1;
        while !self.slicers.insert(name.to_lowercase()) {
            name = format!("{base} {k}");
            k += 1;
        }
        name
    }

    fn cache_name(&mut self, source: &str) -> String {
        let cleaned: String = source.chars().take(200).map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
        let base = format!("Slicer_{cleaned}");
        let mut name = base.clone();
        let mut k = 1;
        while !self.caches.insert(name.to_lowercase()) {
            name = format!("{base}{k}");
            k += 1;
        }
        name
    }
}

/// The sheet's slicers that can be written: each filters a table written on this sheet (`tables`
/// holds the written tables as model name, file id and column names).
pub fn plan<'a>(slicers: &'a [Slicer], tables: &[(&str, u32, &[String])], names: &mut Names) -> Vec<Planned<'a>> {
    let mut out = Vec::new();
    for sl in slicers {
        let Some((_, table_id, cols)) = tables.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(&sl.table)) else { continue };
        let Some(ci) = cols.iter().position(|c| c.eq_ignore_ascii_case(&sl.column)) else { continue };
        let source = cols.get(ci).cloned().unwrap_or_default();
        let name = names.slicer_name(if sl.caption.is_empty() { &sl.column } else { &sl.caption });
        let cache = names.cache_name(&source);
        out.push(Planned { slicer: sl, name, cache, table_id: *table_id, column_id: ci as u32 + 1 });
    }
    out
}

/// The sheet's slicer part.
pub fn slicers_xml(planned: &[Planned<'_>]) -> String {
    let mut s = format!("{}<slicers xmlns=\"{NS_X14}\" xmlns:x=\"{NS_MAIN}\">", crate::write::XML_DECL);
    for p in planned {
        let caption = if p.slicer.caption.is_empty() { &p.slicer.column } else { &p.slicer.caption };
        let _ = write!(s, "<slicer name=\"{}\" cache=\"{}\" caption=\"{}\"", esc_attr(&p.name), esc_attr(&p.cache), esc_attr(caption));
        if p.slicer.columns > 1 {
            let _ = write!(s, " columnCount=\"{}\"", p.slicer.columns.min(20_000));
        }
        let _ = write!(s, " rowHeight=\"{ROW_HEIGHT}\"/>");
    }
    s.push_str("</slicers>");
    s
}

/// A table slicer's cache part.
pub fn table_cache_xml(p: &Planned<'_>) -> String {
    let source = &p.slicer.column;
    format!(
        "{}<slicerCacheDefinition xmlns=\"{NS_X14}\" xmlns:x=\"{NS_MAIN}\" name=\"{}\" sourceName=\"{}\"><extLst><x:ext uri=\"{EXT_TABLE_SLICER_CACHE}\" xmlns:x15=\"{NS_X15}\"><x15:tableSlicerCache tableId=\"{}\" column=\"{}\"/></x:ext></extLst></slicerCacheDefinition>",
        crate::write::XML_DECL,
        esc_attr(&p.cache),
        esc_attr(source),
        p.table_id,
        p.column_id
    )
}

/// The worksheet `<ext>` naming the sheet's slicer part (relationship `rid`).
pub fn sheet_ext(rid: &str) -> String {
    format!(
        "<ext uri=\"{EXT_SHEET_TABLE_SLICERS}\" xmlns:x15=\"{NS_X15}\"><x14:slicerList xmlns:x14=\"{NS_X14}\"><x14:slicer r:id=\"{rid}\"/></x14:slicerList></ext>"
    )
}

/// The workbook `<ext>` listing the table slicer caches by relationship id.
pub fn workbook_ext(rids: &[String]) -> String {
    if rids.is_empty() {
        return String::new();
    }
    let mut s = format!("<ext uri=\"{EXT_WORKBOOK_TABLE_CACHES}\" xmlns:x15=\"{NS_X15}\"><x15:slicerCaches xmlns:x14=\"{NS_X14}\">");
    for rid in rids {
        let _ = write!(s, "<x14:slicerCache r:id=\"{rid}\"/>");
    }
    s.push_str("</x15:slicerCaches></ext>");
    s
}

/// The drawing frame of a table slicer (its body inside the anchor): an `sle:slicer` frame for
/// Excel 2013 and later, a plain rectangle saying what it is for other readers.
pub fn frame_xml(name: &str, id: usize, anchor: &Anchor, xfrm: &dyn Fn(&Anchor, &str) -> String) -> String {
    let name = esc_attr(name);
    format!(
        "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:sle15=\"http://schemas.microsoft.com/office/drawing/2012/slicer\" Requires=\"sle15\"><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2010/slicer\"><sle:slicer xmlns:sle=\"http://schemas.microsoft.com/office/drawing/2010/slicer\" name=\"{name}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice><mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"0\" name=\"\"/><xdr:cNvSpPr><a:spLocks noTextEdit=\"1\"/></xdr:cNvSpPr></xdr:nvSpPr><xdr:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:prstClr val=\"white\"/></a:solidFill><a:ln w=\"1\"><a:solidFill><a:prstClr val=\"black\"/></a:solidFill></a:ln></xdr:spPr><xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\"/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1100\"/><a:t>Slicer: shown by applications that support table slicers.</a:t></a:r></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent>",
        xfrm(anchor, "a:xfrm")
    )
}

// ---------------------------------------------------------------- reading

/// A slicer cache read from the workbook: its name and, for a table slicer, the table and column ids.
#[derive(Clone, Debug, Default)]
pub struct CacheIn {
    pub name: String,
    pub table: Option<(u32, u32)>,
}

/// Reads one slicer cache part.
pub fn read_cache(x: &El) -> Option<CacheIn> {
    let name = x.attr("name")?.to_string();
    let table = x
        .child("extLst")
        .into_iter()
        .flat_map(|l| l.kids("ext"))
        .filter_map(|e| e.child("tableSlicerCache"))
        .find_map(|t| Some((t.attr_u32("tableId")?, t.attr_u32("column")?)));
    Some(CacheIn { name, table })
}

/// A slicer listed in a sheet's slicer part.
#[derive(Clone, Debug)]
pub struct SlicerIn {
    pub name: String,
    pub cache: String,
    pub caption: Option<String>,
    pub columns: u32,
}

/// Reads a sheet's slicer part.
pub fn read_slicers(x: &El) -> Vec<SlicerIn> {
    x.kids("slicer")
        .filter_map(|s| {
            Some(SlicerIn {
                name: s.attr("name")?.to_string(),
                cache: s.attr("cache")?.to_string(),
                caption: s.attr("caption").map(str::to_string),
                columns: s.attr_u32("columnCount").unwrap_or(1).clamp(1, 10),
            })
        })
        .collect()
}
