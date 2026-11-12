//! Text file encodings for CSV and text import and export: UTF-8, Windows-1252 and Japanese
//! Shift_JIS (Windows code page 932, which Japanese Excel reads and writes as "CSV (Comma
//! delimited)"). Encoding tables are `encoding_rs`'s (the WHATWG Shift_JIS, which is CP932).

use encoding_rs::{EncoderResult, SHIFT_JIS, WINDOWS_1252};

/// The encoding of a text file, as Excel's import "File Origin" (元のファイル) names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextEncoding {
    /// Byte order mark, then UTF-8, then Shift_JIS when the bytes read as Japanese, else
    /// Windows-1252 (or Windows-1256 for Arabic text).
    #[default]
    Auto,
    /// 65001: Unicode (UTF-8).
    Utf8,
    /// 932: Japanese (Shift-JIS).
    ShiftJis,
    /// 1252: Western European (Windows).
    Windows1252,
}

impl TextEncoding {
    /// From a code page number or an encoding name: `932`, `shift_jis`, `sjis`, `cp932`;
    /// `65001`, `utf-8`, `utf8`; `1252`, `windows-1252`, `cp1252`; `auto`.
    pub fn parse(text: &str) -> Option<TextEncoding> {
        Some(match text.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "" | "auto" => TextEncoding::Auto,
            "65001" | "utf-8" | "utf8" => TextEncoding::Utf8,
            "932" | "shift-jis" | "sjis" | "cp932" | "windows-31j" | "ms932" => TextEncoding::ShiftJis,
            "1252" | "windows-1252" | "cp1252" | "latin1" => TextEncoding::Windows1252,
            _ => return None,
        })
    }

    /// The Windows code page (`None` for automatic detection).
    pub fn code_page(self) -> Option<u32> {
        match self {
            TextEncoding::Auto => None,
            TextEncoding::Utf8 => Some(65001),
            TextEncoding::ShiftJis => Some(932),
            TextEncoding::Windows1252 => Some(1252),
        }
    }
}

/// The text of a file's bytes in `encoding`. A UTF-8 byte order mark is dropped whatever the
/// encoding, as Excel does.
pub fn decode_with(bytes: &[u8], encoding: TextEncoding) -> String {
    let without_bom = bytes.strip_prefix(b"\xEF\xBB\xBF");
    match encoding {
        TextEncoding::Auto => decode_auto(bytes),
        TextEncoding::Utf8 => String::from_utf8_lossy(without_bom.unwrap_or(bytes)).into_owned(),
        // A file that says it is UTF-8 is read as UTF-8 even with another origin chosen.
        TextEncoding::ShiftJis if without_bom.is_none() => SHIFT_JIS.decode_without_bom_handling(bytes).0.into_owned(),
        TextEncoding::Windows1252 if without_bom.is_none() => WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned(),
        _ => String::from_utf8_lossy(without_bom.unwrap_or(bytes)).into_owned(),
    }
}

/// Which encoding automatic detection picks for `bytes` (never [`TextEncoding::Auto`]):
/// UTF-8 for a byte order mark or valid UTF-8, Shift_JIS when the bytes read as Japanese,
/// else Windows-1252 (which also stands for the Windows-1256 guess of `craft-csv`).
pub fn detect(bytes: &[u8]) -> TextEncoding {
    if bytes.starts_with(b"\xEF\xBB\xBF") || bytes.starts_with(b"\xFF\xFE") || bytes.starts_with(b"\xFE\xFF") || std::str::from_utf8(bytes).is_ok() {
        TextEncoding::Utf8
    } else if reads_as_shift_jis(bytes) {
        TextEncoding::ShiftJis
    } else {
        TextEncoding::Windows1252
    }
}

/// Byte order marks and UTF-8 first, as before; bytes that aren't UTF-8 and read as Japanese
/// Shift_JIS are decoded so (Japanese Excel opens them directly: 932 is its ANSI code page);
/// anything else goes to `craft-csv`'s Windows-1252/1256 guess.
fn decode_auto(bytes: &[u8]) -> String {
    match detect(bytes) {
        TextEncoding::ShiftJis => SHIFT_JIS.decode_without_bom_handling(bytes).0.into_owned(),
        _ => craft_csv::decode_text(bytes),
    }
}

/// Whether bytes (not UTF-8) are plausibly Shift_JIS: every sequence decodes, nothing falls in
/// the user-defined area, double-byte characters (kana, kanji, full-width forms) are at least as
/// many as half-width katakana, and Japanese-looking pairs outnumber Western-looking ones.
/// Western text in Windows-1252 fails this: an accented letter is a lead byte followed by a byte
/// Shift_JIS doesn't allow (a space, a comma), a user-defined code, a lone half-width katakana,
/// or, inside a word (`Käse`), a lowercase accented letter (lead byte 0xE0 or more) followed by an
/// ASCII letter. Japanese text is mostly pairs led by 0x81-0x9F (symbols, kana, the common kanji)
/// or, for the rarer kanji, ending in a byte that isn't an ASCII letter.
fn reads_as_shift_jis(bytes: &[u8]) -> bool {
    let (text, malformed) = SHIFT_JIS.decode_without_bom_handling(bytes);
    if malformed {
        return false;
    }
    let (mut wide, mut half) = (0usize, 0usize);
    for c in text.chars() {
        match c {
            '\u{E000}'..='\u{F8FF}' => return false,
            '\u{FF61}'..='\u{FF9F}' => half += 1,
            c if u32::from(c) >= 0x2000 => wide += 1,
            _ => {}
        }
    }
    let (mut japanese, mut western) = (0usize, 0usize);
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if matches!(b, 0x81..=0x9F | 0xE0..=0xFC) {
            match bytes.get(i + 1) {
                Some(t) if b >= 0xE0 && t.is_ascii_alphabetic() => western += 1,
                Some(_) => japanese += 1,
                None => {}
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    wide > 0 && wide >= half && japanese > western
}

/// `text` in `encoding` ([`TextEncoding::Auto`] writes UTF-8). Characters Shift_JIS or
/// Windows-1252 can't hold become `?`, as in Excel's Save As.
pub fn encode_with(text: &str, encoding: TextEncoding) -> Vec<u8> {
    let enc = match encoding {
        TextEncoding::Auto | TextEncoding::Utf8 => return text.as_bytes().to_vec(),
        TextEncoding::ShiftJis => SHIFT_JIS,
        TextEncoding::Windows1252 => WINDOWS_1252,
    };
    let mut encoder = enc.new_encoder();
    let mut out: Vec<u8> = Vec::with_capacity(text.len().saturating_add(16));
    let mut rest = text;
    let mut buf = [0u8; 4096];
    loop {
        let (result, read, written) = encoder.encode_from_utf8_without_replacement(rest, &mut buf, true);
        out.extend_from_slice(buf.get(..written).unwrap_or_default());
        rest = rest.get(read..).unwrap_or_default();
        match result {
            EncoderResult::InputEmpty => break,
            EncoderResult::OutputFull => {}
            EncoderResult::Unmappable(_) => out.push(b'?'),
        }
    }
    out
}
