//! Embedded fonts for text the standard 14 fonts cannot encode (Japanese, Chinese, Korean and other
//! scripts outside WinAnsiEncoding, #3724).
//!
//! A font file is subset to the glyphs a document uses ([`subsetter`]) and written as a Type0 font
//! with the `Identity-H` encoding: the content stream addresses glyphs by CID, which is the
//! subset's glyph id. CFF-flavoured OpenType fonts become a `CIDFontType0` with a bare CFF program
//! (`FontFile3`, `/Subtype /CIDFontType0C`), TrueType ones a `CIDFontType2` with the subset font
//! (`FontFile2`, `/CIDToGIDMap /Identity`). Every font carries `/W` widths and a `ToUnicode` CMap
//! so text can be copied and searched.
//!
//! The writer reads no files: the caller hands it a [`FontResolver`] that answers
//! [`FontRequest`]s with font bytes, and the writer asks only when a text run needs a glyph.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard};

use skrifa::{FontRef, MetadataProvider, instance::LocationRef, instance::Size, raw::TableProvider, string::StringId};
use subsetter::GlyphRemapper;

/// Largest font file accepted, in bytes (the biggest CJK collections are around 60 MB).
pub const MAX_FONT_BYTES: usize = 64 << 20;
/// Most embedded fonts one document loads, so a workbook naming thousands of fonts can't make the
/// writer read and subset thousands of files.
const MAX_FONTS: usize = 16;
/// Most distinct font requests answered (and remembered) per document.
const MAX_REQUESTS: usize = 256;
/// Default advance of a glyph the font gives no width for, in 1/1000 em.
const DEFAULT_WIDTH: u16 = 1000;

/// What the writer asks a [`FontResolver`] for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FontRequest {
    /// The installed font of this family (the font the text is formatted with), in the weight and
    /// slant nearest to the ones given.
    Family { name: String, bold: bool, italic: bool },
    /// A system font for this script, for text the family font lacks.
    Script(Script),
}

/// The scripts the writer asks fallback fonts for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Script {
    /// Japanese: kana, and Han characters with Japanese glyph shapes. Also asked for other
    /// characters outside WinAnsiEncoding (Greek, Cyrillic, symbols), which the Japanese system
    /// fonts carry.
    Japanese,
    /// Simplified Chinese Han characters.
    Chinese,
    /// Korean Hangul.
    Korean,
}

/// Answers font requests with a parsed font, or `None` when nothing suitable is installed.
pub type FontResolver = Box<dyn FnMut(&FontRequest) -> Option<EmbeddedFont> + Send>;

/// A parsed font file (one face of it) that can be embedded.
#[derive(Clone)]
pub struct EmbeddedFont {
    data: Arc<[u8]>,
    index: u32,
    cff: bool,
    units_per_em: f32,
    /// PostScript name, reduced to characters a PDF name can hold unescaped.
    name: String,
    /// `[x_min, y_min, x_max, y_max]` in 1/1000 em.
    bbox: [i32; 4],
    ascent: i32,
    descent: i32,
    cap_height: i32,
    bold: bool,
}

impl std::fmt::Debug for EmbeddedFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddedFont").field("name", &self.name).field("index", &self.index).field("cff", &self.cff).finish()
    }
}

impl EmbeddedFont {
    /// Parses face `index` of an OpenType/TrueType file (or collection). Returns `None` when the
    /// data is not a font the writer can subset (CFF2 or bitmap-only outlines, damage), when it is
    /// larger than [`MAX_FONT_BYTES`], when its licence restricts embedding (`OS/2.fsType` 2), or
    /// when it is a variable font whose default instance is not of regular weight (the subset would
    /// keep the default instance: a Thin default would print every glyph hairline-thin).
    pub fn parse(data: Vec<u8>, index: u32) -> Option<EmbeddedFont> {
        if data.len() > MAX_FONT_BYTES {
            return None;
        }
        let font = FontRef::from_index(&data, index).ok()?;
        if font.os2().is_ok_and(|os2| os2.fs_type() & 0xF == 2) {
            return None;
        }
        let cff = font.cff().is_ok();
        if !cff && font.glyf().is_err() {
            return None;
        }
        if let Some(wght) = font.axes().iter().find(|a| a.tag() == skrifa::Tag::new(b"wght"))
            && !(350.0..=450.0).contains(&wght.default_value())
        {
            return None;
        }
        let m = font.metrics(Size::unscaled(), LocationRef::default());
        let upem = f32::from(m.units_per_em.max(16));
        let to_pdf = |v: f32| if v.is_finite() { (v * 1000.0 / upem).round().clamp(-32_000.0, 32_000.0) as i32 } else { 0 };
        let bbox = m.bounds.map_or([0, -200, 1000, 900], |b| [to_pdf(b.x_min), to_pdf(b.y_min), to_pdf(b.x_max), to_pdf(b.y_max)]);
        let ps = font.localized_strings(StringId::POSTSCRIPT_NAME).english_or_first().map(|s| s.to_string()).unwrap_or_default();
        let mut name: String = ps.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(63).collect();
        if name.is_empty() {
            name = "Embedded".into();
        }
        let bold = font.attributes().weight.value() >= 600.0;
        let (ascent, descent) = (to_pdf(m.ascent), to_pdf(m.descent));
        let cap_height = m.cap_height.map_or(ascent, to_pdf);
        let font = EmbeddedFont { data: Arc::from(data), index, cff, units_per_em: upem, name, bbox, ascent, descent, cap_height, bold };
        // A font the subsetter rejects would leave the content pointing at glyphs that are never
        // written; find out now, while the caller can still pick another font.
        subsetter::subset(&font.data, index, &GlyphRemapper::new()).ok()?;
        Some(font)
    }

    /// The PostScript name (sanitised).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the font has a glyph for `c`.
    pub fn has_char(&self, c: char) -> bool {
        self.glyph(c).is_some()
    }

    /// Whether the face is bold (weight 600 or more).
    pub fn is_bold(&self) -> bool {
        self.bold
    }

    fn font(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(&self.data, self.index).ok()
    }

    /// The glyph id and advance (1/1000 em) of `c`; `None` when the font has no glyph for it.
    fn glyph(&self, c: char) -> Option<(u16, u16)> {
        let font = self.font()?;
        let gid = font.charmap().map(c)?;
        let gid16 = u16::try_from(gid.to_u32()).ok().filter(|g| *g != 0)?;
        let advance = font.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(gid).unwrap_or(self.units_per_em);
        let w = (advance * 1000.0 / self.units_per_em).round();
        let w = if w.is_finite() { w.clamp(0.0, 10_000.0) as u16 } else { DEFAULT_WIDTH };
        Some((gid16, w))
    }
}

/// One embedded font of a document and the glyphs its pages use.
struct Slot {
    font: EmbeddedFont,
    remap: GlyphRemapper,
    /// Character → CID (`None`: the font has no glyph for it).
    chars: HashMap<char, Option<u16>>,
    /// By CID: advance width in 1/1000 em and the character it stands for.
    glyphs: Vec<(u16, Option<char>)>,
}

impl Slot {
    fn new(font: EmbeddedFont) -> Slot {
        Slot { font, remap: GlyphRemapper::new(), chars: HashMap::new(), glyphs: vec![(0, None)] }
    }

    /// The CID of `c` in this font's subset, adding the glyph on first use.
    fn cid(&mut self, c: char) -> Option<u16> {
        if let Some(cid) = self.chars.get(&c) {
            return *cid;
        }
        let cid = self.font.glyph(c).map(|(gid, w)| {
            let cid = self.remap.remap(gid);
            let i = usize::from(cid);
            if self.glyphs.len() <= i {
                self.glyphs.resize(i + 1, (DEFAULT_WIDTH, None));
                self.glyphs[i] = (w, Some(c));
            }
            cid
        });
        self.chars.insert(c, cid);
        cid
    }
}

/// The embedded fonts of one document, shared by its pages.
#[derive(Default)]
pub(crate) struct FontSet {
    resolver: Option<FontResolver>,
    slots: Vec<Slot>,
    /// Requests already answered: the slot that holds the answer, or `None`.
    answers: HashMap<FontRequest, Option<usize>>,
}

/// A document's [`FontSet`], shared between the document and its pages.
pub(crate) type SharedFonts = Arc<Mutex<FontSet>>;

/// Locks the font set; a panic while it was held (it can't happen: nothing in here panics) would
/// leave it usable, not poisoned.
pub(crate) fn lock(fonts: &SharedFonts) -> MutexGuard<'_, FontSet> {
    fonts.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where a character of a text run is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// An embedded font (slot index) and the glyph's CID in it.
    Embedded(usize, u16),
    /// The predefined, non-embedded CID font (no font installed has the glyph).
    Predefined,
}

impl FontSet {
    pub(crate) fn set_resolver(&mut self, resolver: Option<FontResolver>) {
        self.resolver = resolver;
        self.answers.clear();
    }

    /// The slot answering `req`, asking the resolver the first time.
    fn answer(&mut self, req: &FontRequest) -> Option<usize> {
        if let Some(a) = self.answers.get(req) {
            return *a;
        }
        if self.answers.len() >= MAX_REQUESTS {
            return None;
        }
        let resolver = self.resolver.as_mut()?;
        let slot = if self.slots.len() >= MAX_FONTS {
            None
        } else {
            resolver(req).map(|font| {
                // Two requests answered by the same face share one subset.
                let same = |s: &Slot| {
                    Arc::ptr_eq(&s.font.data, &font.data)
                        || (s.font.index == font.index && s.font.name == font.name && s.font.data.len() == font.data.len())
                };
                match self.slots.iter().position(same) {
                    Some(i) => i,
                    None => {
                        self.slots.push(Slot::new(font));
                        self.slots.len() - 1
                    }
                }
            })
        };
        self.answers.insert(req.clone(), slot);
        slot
    }

    /// Picks the font for `c` (a character outside WinAnsiEncoding) set in `family`: the family's
    /// own font when it is installed and has the glyph, else the system fonts for the character's
    /// script, else the predefined CID font.
    pub(crate) fn target(&mut self, family: &str, bold: bool, italic: bool, c: char) -> Target {
        if self.resolver.is_none() {
            return Target::Predefined;
        }
        let family = family.trim();
        let mut requests: Vec<FontRequest> = Vec::with_capacity(4);
        if !family.is_empty() {
            requests.push(FontRequest::Family { name: family.to_string(), bold, italic });
        }
        requests.extend(script_order(c).into_iter().map(FontRequest::Script));
        for req in &requests {
            if let Some(i) = self.answer(req)
                && let Some(cid) = self.slots.get_mut(i).and_then(|s| s.cid(c))
            {
                return Target::Embedded(i, cid);
            }
        }
        Target::Predefined
    }

    /// Whether the embedded font in `slot` lacks a bold face (bold text is then drawn with a
    /// stroked outline).
    pub(crate) fn needs_fake_bold(&self, slot: usize) -> bool {
        self.slots.get(slot).is_some_and(|s| !s.font.bold)
    }

    /// The slots that have glyphs beyond `.notdef`.
    pub(crate) fn used(&self) -> impl Iterator<Item = usize> + '_ {
        self.slots.iter().enumerate().filter(|(_, s)| s.glyphs.len() > 1).map(|(i, _)| i)
    }

    /// The PDF objects of the font in `slot`, numbered from `first` (five objects: Type0 font,
    /// CIDFont, font descriptor, font program, ToUnicode CMap). `None` when subsetting fails.
    pub(crate) fn objects(&self, slot: usize, first: u32) -> Option<Vec<(u32, Vec<u8>)>> {
        let s = self.slots.get(slot)?;
        let f = &s.font;
        let subset = subsetter::subset(&f.data, f.index, &s.remap).ok()?;
        let program = if f.cff { FontRef::new(&subset).ok()?.data_for_tag(skrifa::Tag::new(b"CFF "))?.as_bytes().to_vec() } else { subset };
        let (type0, cid_id, desc_id, file_id, cmap_id) = (first, first + 1, first + 2, first + 3, first + 4);
        let base = format!("{}+{}", subset_tag(s), f.name);
        let mut out = Vec::with_capacity(5);
        let type0_name = if f.cff { format!("{base}-Identity-H") } else { base.clone() };
        out.push((
            type0,
            format!("<< /Type /Font /Subtype /Type0 /BaseFont /{type0_name} /Encoding /Identity-H /DescendantFonts [{cid_id} 0 R] /ToUnicode {cmap_id} 0 R >>")
                .into_bytes(),
        ));
        let mut widths = String::from("[0 [");
        for (i, (w, _)) in s.glyphs.iter().enumerate() {
            if i > 0 {
                widths.push(' ');
            }
            let _ = write!(widths, "{w}");
        }
        widths.push_str("]]");
        let (subtype, gid_map) = if f.cff { ("CIDFontType0", "") } else { ("CIDFontType2", " /CIDToGIDMap /Identity") };
        out.push((
            cid_id,
            format!(
                "<< /Type /Font /Subtype /{subtype} /BaseFont /{base} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {desc_id} 0 R /DW {DEFAULT_WIDTH} /W {widths}{gid_map} >>"
            )
            .into_bytes(),
        ));
        let [x0, y0, x1, y1] = f.bbox;
        let file_key = if f.cff { "FontFile3" } else { "FontFile2" };
        out.push((
            desc_id,
            format!(
                "<< /Type /FontDescriptor /FontName /{base} /Flags 4 /FontBBox [{x0} {y0} {x1} {y1}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV {} /{file_key} {file_id} 0 R >>",
                f.ascent,
                f.descent,
                f.cap_height,
                if f.bold { 140 } else { 80 }
            )
            .into_bytes(),
        ));
        let dict = if f.cff {
            format!("<< /Subtype /CIDFontType0C /Length {} >>", program.len())
        } else {
            format!("<< /Length {0} /Length1 {0} >>", program.len())
        };
        out.push((file_id, stream(dict, &program)));
        let cmap = to_unicode(&s.glyphs);
        out.push((cmap_id, stream(format!("<< /Length {} >>", cmap.len()), cmap.as_bytes())));
        Some(out)
    }
}

/// The scripts whose fonts are tried for `c`, in order.
fn script_order(c: char) -> [Script; 3] {
    let u = c as u32;
    let hangul = (0xAC00..=0xD7AF).contains(&u) || (0x1100..=0x11FF).contains(&u) || (0x3130..=0x318F).contains(&u) || (0xA960..=0xA97F).contains(&u);
    if hangul { [Script::Korean, Script::Japanese, Script::Chinese] } else { [Script::Japanese, Script::Chinese, Script::Korean] }
}

/// A stream object: `dict`, then the data.
fn stream(dict: String, data: &[u8]) -> Vec<u8> {
    let mut o = dict.into_bytes();
    o.extend_from_slice(b"\nstream\n");
    o.extend_from_slice(data);
    o.extend_from_slice(b"\nendstream");
    o
}

/// The six-letter subset prefix of a font's name (ISO 32000-1 9.6.4), derived from the glyphs it
/// keeps so the same subset gets the same tag.
fn subset_tag(s: &Slot) -> String {
    // FNV-1a over the original glyph ids, in CID order.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for c in s.glyphs.iter().filter_map(|(_, c)| *c) {
        for b in (c as u32).to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    (0..6)
        .map(|i| {
            let letter = ((h >> (i * 5)) % 26) as u8;
            char::from(b'A' + letter)
        })
        .collect()
}

/// A ToUnicode CMap mapping each CID to the character it was added for.
fn to_unicode(glyphs: &[(u16, Option<char>)]) -> String {
    let entries: Vec<(usize, char)> = glyphs.iter().enumerate().filter_map(|(cid, (_, c))| c.map(|c| (cid, c))).collect();
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    // At most 100 entries per bfchar block (PostScript CMap limit).
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (cid, c) in chunk {
            let mut units = [0u16; 2];
            let hex: String = c.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(s, "<{cid:04X}> <{hex}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend");
    s
}
