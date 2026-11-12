//! Where the operating system keeps the fonts for scripts beyond Latin, shared by the UI (its
//! fallback faces for drawing cells) and PDF export (the fonts it embeds, #3724).
//!
//! The lists are file paths to probe in order; nothing is bundled, and a machine without any of
//! them keeps the old behaviour (tofu in the grid, the predefined CID font in PDFs).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gridcraft_pdf::{EmbeddedFont, FontRequest, FontResolver, MAX_FONT_BYTES, Script};

/// The installed font files of a family, best match for the weight and slant first: `(family,
/// bold, italic) → [(file, face index)]`. A front end with a font catalog provides it (see
/// [`crate::Session::font_files`]); without one, PDF export embeds only the script fallback fonts.
pub type FontFiles = Arc<dyn Fn(&str, bool, bool) -> Vec<(PathBuf, u32)> + Send + Sync>;

/// Linux directories that hold CJK fonts across distributions: Debian/Ubuntu (`opentype/noto`,
/// `truetype/wqy`), Fedora (`google-noto-cjk`), Arch (`noto-cjk`, `wenquanyi`), ChromeOS's Linux
/// container (`chromeos/notocjk`, #1762), plus local and
/// flat layouts. Flatpak exposes the same host layouts through read-only font mounts.
pub const LINUX_CJK_DIRS: [&str; 22] = [
    // Flatpak mounts host fonts here without a broad filesystem permission. Keep the same
    // distribution layouts as below; /usr/share/fonts inside the sandbox belongs to the runtime.
    "/run/host/fonts/opentype/noto",
    "/run/host/fonts/google-noto-cjk",
    "/run/host/fonts/noto-cjk",
    "/run/host/fonts/truetype/noto",
    "/run/host/fonts/noto",
    "/run/host/fonts/truetype",
    "/run/host/fonts/TTF",
    "/run/host/fonts",
    "/run/host/local-fonts",
    "/run/host/local-fonts/noto-cjk",
    "/run/host/user-fonts",
    "/usr/share/fonts/opentype/noto",
    "/usr/share/fonts/google-noto-cjk",
    "/usr/share/fonts/noto-cjk",
    "/usr/share/fonts/truetype/noto",
    "/usr/share/fonts/noto",
    "/usr/share/fonts/truetype",
    "/usr/share/fonts/TTF",
    "/usr/share/fonts",
    "/usr/local/share/fonts",
    "/usr/local/share/fonts/noto-cjk",
    "/usr/share/fonts/chromeos/notocjk",
];

/// The Windows fonts folder (`%WINDIR%\Fonts`, or the default install location).
pub fn windows_fonts() -> String {
    std::env::var("WINDIR").map(|w| format!("{w}\\Fonts")).unwrap_or_else(|_| "C:\\Windows\\Fonts".into())
}

/// The Noto Sans CJK faces for one locale (`jp`, `sc`, `kr`): the per-locale OTF, then the
/// region-merged TTC at `face`, in every Linux font directory.
fn noto_cjk(locale: &str, face: u32) -> Vec<(String, u32)> {
    let mut v = Vec::new();
    for d in LINUX_CJK_DIRS {
        v.push((format!("{d}/NotoSansCJK{locale}-Regular.otf"), 0));
        v.push((format!("{d}/NotoSansCJK-Regular.ttc"), face));
    }
    v
}

/// Japanese Han + Kana faces (Japanese kanji glyph shapes).
///
/// The region-merged `NotoSansCJK-Regular.ttc` holds one face per locale (JP 0, KR 1, SC 2, TC 3,
/// HK 4) with the same glyph coverage, so the index only picks glyph shapes.
pub fn cjk_han_japanese() -> Vec<(String, u32)> {
    let mac = "/System/Library/Fonts";
    let win = windows_fonts();
    let mut v: Vec<(String, u32)> = Vec::new();
    v.push((format!("{win}\\YuGothR.ttc"), 0)); // Yu Gothic
    v.push((format!("{win}\\msgothic.ttc"), 0)); // MS Gothic
    v.push((format!("{mac}/ヒラギノ角ゴシック W3.ttc"), 0)); // Hiragino Sans
    v.extend(noto_cjk("jp", 0));
    // Noto Sans JP as Google Fonts ships it (the region subset), which BrowserOS installs.
    for d in LINUX_CJK_DIRS {
        v.push((format!("{d}/NotoSansJP-Regular.otf"), 0));
    }
    v
}

/// Simplified Chinese Han faces, apart from the Japanese ones so a glyph one family lacks
/// (simplified-only hanzi in a Japanese font) still falls through to a real glyph.
pub fn cjk_han_simplified() -> Vec<(String, u32)> {
    let mac = "/System/Library/Fonts";
    let win = windows_fonts();
    let mut v: Vec<(String, u32)> = vec![
        (format!("{win}\\msyh.ttc"), 0),            // Microsoft YaHei
        (format!("{win}\\simsun.ttc"), 0),          // SimSun
        (format!("{win}\\simhei.ttf"), 0),          // SimHei
        (format!("{mac}/Hiragino Sans GB.ttc"), 0), // ships on every recent macOS
        (format!("{mac}/PingFang.ttc"), 0),         // older macOS only; now a downloadable asset
    ];
    v.extend(noto_cjk("sc", 2));
    for d in LINUX_CJK_DIRS {
        v.push((format!("{d}/wqy/wqy-microhei.ttc"), 0));
        v.push((format!("{d}/wenquanyi/wqy-microhei.ttc"), 0));
        v.push((format!("{d}/wqy-microhei.ttc"), 0));
    }
    v
}

/// Hangul faces (Korean). The region-merged Noto Sans CJK file also carries Hangul.
pub fn cjk_hangul() -> Vec<(String, u32)> {
    let mac = "/System/Library/Fonts";
    let win = windows_fonts();
    let mut v: Vec<(String, u32)> = Vec::new();
    v.push((format!("{win}\\malgun.ttf"), 0)); // Malgun Gothic
    v.push((format!("{mac}/AppleSDGothicNeo.ttc"), 0));
    v.extend(noto_cjk("kr", 1));
    v
}

/// The candidate files for a script, in the order they are tried.
pub fn script_candidates(script: Script) -> Vec<(String, u32)> {
    match script {
        Script::Japanese => cjk_han_japanese(),
        Script::Chinese => cjk_han_simplified(),
        Script::Korean => cjk_hangul(),
    }
}

/// Reads a font file of at most [`MAX_FONT_BYTES`], even when it grows after it was found.
pub fn read_font(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).ok()?;
    let limit = MAX_FONT_BYTES as u64;
    if file.metadata().ok()?.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= MAX_FONT_BYTES).then_some(bytes)
}

/// The first of `candidates` that exists and can be embedded.
fn first_embeddable<P: AsRef<Path>>(candidates: impl IntoIterator<Item = (P, u32)>) -> Option<EmbeddedFont> {
    candidates.into_iter().find_map(|(path, index)| EmbeddedFont::parse(read_font(path.as_ref())?, index))
}

/// The font source PDF export uses: a cell's own family through `family_files` when it is
/// installed, and the CJK system fonts above for each script.
pub fn pdf_font_resolver(family_files: Option<FontFiles>) -> FontResolver {
    Box::new(move |req: &FontRequest| match req {
        FontRequest::Family { name, bold, italic } => first_embeddable(family_files.as_ref()?(name, *bold, *italic)),
        FontRequest::Script(script) => first_embeddable(script_candidates(*script)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browseros_japanese_font_is_a_candidate() {
        assert!(cjk_han_japanese().iter().any(|(p, i)| p == "/usr/share/fonts/opentype/noto/NotoSansJP-Regular.otf" && *i == 0));
        assert!(script_candidates(Script::Korean).iter().any(|(p, i)| p.ends_with("/NotoSansCJK-Regular.ttc") && *i == 1));
    }

    #[test]
    fn family_requests_go_to_the_front_end_and_missing_files_resolve_to_nothing() {
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = Arc::clone(&asked);
        let files: FontFiles = Arc::new(move |name: &str, bold, _| {
            log.lock().unwrap().push((name.to_string(), bold));
            vec![(PathBuf::from("/nonexistent/font.otf"), 0)]
        });
        let mut resolve = pdf_font_resolver(Some(files));
        assert!(resolve(&FontRequest::Family { name: "Meiryo".into(), bold: true, italic: false }).is_none());
        assert_eq!(*asked.lock().unwrap(), vec![("Meiryo".to_string(), true)]);
        let mut without = pdf_font_resolver(None);
        assert!(without(&FontRequest::Family { name: "Meiryo".into(), bold: false, italic: false }).is_none());
    }
}
