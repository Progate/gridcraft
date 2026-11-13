//! Interface translations. Command ids and document text stay stable; only interface chrome is
//! translated. Untranslated labels fall back to English, so coverage can grow incrementally.
//!
//! The interface language is a preference ([`crate::UiState::language`]): on first run it defaults
//! to the system's language ([`Language::system`]), and the View tab switches it by hand.
//!
//! CJK fonts stay system-provided: [`crate::theme`] appends available CJK faces to every family so
//! Japanese, Chinese and Korean labels and workbook text can render without bundling font files.

use serde::{Deserialize, Serialize};

mod cs;
mod de;
mod el;
mod es;
mod es_functions;
mod fr;
mod it;
mod ja;
mod ja_functions;
mod ko;
mod nl;
mod pl;
mod pt;
mod ro;
mod ru;
mod sr;
mod tr;
mod uk;
mod vi;
mod zh;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Es,
    El,
    Zh,
    Ja,
    Ko,
    Ru,
    #[serde(rename = "pt")]
    PtBr,
    De,
    Cs,
    Fr,
    Sr,
    Tr,
    Pl,
    It,
    Uk,
    Vi,
    Nl,
    Ro,
}

impl Language {
    pub const ALL: [Self; 19] = [
        Self::En,
        Self::Es,
        Self::El,
        Self::Zh,
        Self::Ja,
        Self::Ko,
        Self::Ru,
        Self::PtBr,
        Self::De,
        Self::Cs,
        Self::Fr,
        Self::Sr,
        Self::Pl,
        Self::It,
        Self::Uk,
        Self::Vi,
        Self::Nl,
        Self::Ro,
        Self::Tr,
    ];

    /// Every language in the order the switcher lists them: English first (the source language),
    /// then the rest by their own name with accents folded, so Čeština sits under C. Names in
    /// other scripts follow the Latin ones in code point order: Cyrillic, then the CJK languages.
    pub fn for_picker() -> Vec<Self> {
        let mut rest: Vec<Self> = Self::ALL.into_iter().filter(|l| *l != Self::En).collect();
        rest.sort_by_cached_key(|l| name_sort_key(l.name()));
        std::iter::once(Self::En).chain(rest).collect()
    }

    /// The language's own name, shown in the switcher.
    pub fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Es => "Español",
            Self::El => "Ελληνικά",
            Self::Zh => "简体中文",
            Self::Ja => "日本語",
            Self::Ko => "한국어",
            Self::Ru => "Русский",
            Self::PtBr => "Português (Brasil)",
            Self::De => "Deutsch",
            Self::Cs => "Čeština",
            Self::Fr => "Français",
            Self::Sr => "Srpski",
            Self::Tr => "Türkçe",
            Self::Pl => "Polski",
            Self::It => "Italiano",
            Self::Uk => "Українська",
            Self::Vi => "Tiếng Việt",
            Self::Nl => "Nederlands",
            Self::Ro => "Română",
        }
    }

    /// The language's code, as saved in `ui.json` and taken by `app.language.set`.
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
            Self::El => "el",
            Self::Zh => "zh",
            Self::Ja => "ja",
            Self::Ko => "ko",
            Self::Ru => "ru",
            Self::PtBr => "pt",
            Self::De => "de",
            Self::Cs => "cs",
            Self::Fr => "fr",
            Self::Sr => "sr",
            Self::Tr => "tr",
            Self::Pl => "pl",
            Self::It => "it",
            Self::Uk => "uk",
            Self::Vi => "vi",
            Self::Nl => "nl",
            Self::Ro => "ro",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        Self::from_tag(code)
    }

    /// Every code `app.language.set` takes, quoted and joined (`"en"|"es"|…`), for command docs
    /// and errors.
    pub fn code_list(separator: &str) -> String {
        Self::ALL.iter().map(|l| format!("\"{}\"", l.code())).collect::<Vec<_>>().join(separator)
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag.trim().split(['-', '_']).next()?.to_ascii_lowercase().as_str() {
            "en" => Some(Self::En),
            "es" => Some(Self::Es),
            "el" => Some(Self::El),
            "zh" => Some(Self::Zh),
            "ja" => Some(Self::Ja),
            "ko" => Some(Self::Ko),
            "ru" => Some(Self::Ru),
            "pt" => Some(Self::PtBr),
            "de" => Some(Self::De),
            "cs" => Some(Self::Cs),
            "fr" => Some(Self::Fr),
            "sr" => Some(Self::Sr),
            "tr" => Some(Self::Tr),
            "pl" => Some(Self::Pl),
            "it" => Some(Self::It),
            "uk" => Some(Self::Uk),
            "vi" => Some(Self::Vi),
            "nl" => Some(Self::Nl),
            "ro" => Some(Self::Ro),
            _ => None,
        }
    }

    /// The system's preferred interface language, best effort; English when it is unknown or
    /// unsupported. Used as the first-run default while nothing is saved.
    pub fn system() -> Self {
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        {
            // WASI (BrowserOS) has no locale API; the locale variables say it, as on Linux.
            let from_env = || ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()));
            // sys-locale takes its wasm-bindgen path on every wasm32 target, which panics on WASI.
            #[cfg(not(target_arch = "wasm32"))]
            let tag = sys_locale::get_locale().or_else(from_env);
            #[cfg(target_arch = "wasm32")]
            let tag = from_env();
            tag.and_then(|tag| Self::from_tag(&tag)).unwrap_or(Self::En)
        }
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        {
            web_sys::window().and_then(|window| window.navigator().language()).and_then(|tag| Self::from_tag(&tag)).unwrap_or(Self::En)
        }
    }

    pub fn formula_locale(self) -> gridcraft_engine::formula::FormulaLocale {
        if self == Self::Es { gridcraft_engine::formula::FormulaLocale::Es } else { gridcraft_engine::formula::FormulaLocale::En }
    }

    /// Translates an interface label; untranslated labels fall back to English.
    ///
    /// A label that needs two translations (Home's text "Orientation" vs Page Layout's print
    /// "Orientation") is keyed `context|label`: English shows only the part after the `|`.
    pub fn tr(self, text: &str) -> &str {
        self.translated(text).unwrap_or_else(|| text.rsplit_once('|').map_or(text, |(_, label)| label))
    }

    /// The catalog's own translation of `text`, or `None` when it falls back to English.
    fn translated(self, text: &str) -> Option<&'static str> {
        match self {
            Self::En => None,
            Self::Es => es::translate(text),
            Self::El => el::translate(text),
            Self::Ja => ja::translate(text),
            Self::Zh => zh::translate(text),
            Self::Ko => ko::translate(text),
            Self::Ru => ru::translate(text),
            Self::PtBr => pt::translate(text),
            Self::De => de::translate(text),
            Self::Cs => cs::translate(text),
            Self::Fr => fr::translate(text),
            Self::Sr => sr::translate(text),
            Self::Tr => tr::translate(text),
            Self::Pl => pl::translate(text),
            Self::It => it::translate(text),
            Self::Uk => uk::translate(text),
            Self::Vi => vi::translate(text),
            Self::Nl => nl::translate(text),
            Self::Ro => ro::translate(text),
        }
    }

    /// The name Excel's style galleries show for a table or PivotTable style, built from the
    /// `TableStyleMedium7` / `PivotStyleLight16` name it is saved under: the colour of its accent
    /// in `theme`, then the family and number ("Green, Table Style Medium 7"; in Japanese
    /// 「緑, テーブル スタイル (中間) 7」). A name in another form is returned as it is.
    pub fn style_name(self, theme: &gridcraft_engine::model::Theme, style: &str) -> String {
        let (rest, kind) = if let Some(rest) = style.strip_prefix("TableStyle") {
            (rest, "Table Style {family} {n}")
        } else if let Some(rest) = style.strip_prefix("PivotStyle") {
            (rest, "Pivot Style {family} {n}")
        } else {
            return style.to_owned();
        };
        let Some((family, n)) = ["Light", "Medium", "Dark"].into_iter().find_map(|f| Some((f, rest.strip_prefix(f)?.parse::<u32>().ok()?))) else {
            return style.to_owned();
        };
        // Styles 1, 8, 15 and 22 of a family are the neutral ones; the six after each use Accent 1–6,
        // the slots `gridcraft_engine::tables::look` paints the swatches with.
        let color = match (n.saturating_sub(1) % 7) as usize {
            0 if family == "Light" => "Style Color|White",
            0 => "Style Color|Black",
            k => theme.colors.get(3 + k).map_or("Style Color|Black", |rgb| style_color(*rgb)),
        };
        let family = self.tr(match family {
            "Light" => "Table Styles|Light",
            "Medium" => "Table Styles|Medium",
            _ => "Table Styles|Dark",
        });
        let name = self.tr(kind).replace("{family}", family).replace("{n}", &n.to_string());
        self.tr("{color}, {style}").replace("{color}", self.tr(color)).replace("{style}", &name)
    }

    pub(crate) fn tr_error(self, message: &str) -> String {
        let message = if message.starts_with("command `") { crate::clean_error(message) } else { message.to_owned() };

        let translated = self.tr(&message);
        let fallback = message.rsplit_once('|').map_or(message.as_str(), |(_, label)| label);

        if translated != fallback {
            return translated.to_owned();
        }
        // Messages with a varying part (a sheet action, a range title) match a template instead.
        match self {
            Self::Ja => ja::translate_message(&message).unwrap_or(message),
            _ => message,
        }
    }

    /// A worksheet function's description in this language, looked up by the English text of the
    /// function catalog; `None` when untranslated, and the help shows the English.
    pub fn function_description(self, english: &str) -> Option<&'static str> {
        lookup(self.function_help()?.0, english)
    }

    /// A function argument name in this language, given as the catalog spells it without
    /// brackets or index (`number`, `logical_test`).
    pub fn function_argument(self, english: &str) -> Option<&'static str> {
        lookup(self.function_help()?.1, english)
    }

    /// The description of the function `name` (its English name, any case) in this language: the
    /// function's own help when the language keys its help by function, else the translation of
    /// its `english` description; `None` when untranslated.
    pub fn function_description_of(self, name: &str, english: &str) -> Option<&'static str> {
        self.named_function_help(name).map(|(_, description, _)| *description).or_else(|| self.function_description(english))
    }

    /// The name of argument `english` (`lookup_value`, without brackets or index) of the function
    /// `name` in this language. Japanese names an argument per function (VLOOKUP's `table_array`
    /// is 範囲, SUMIFS' `criteria` is 条件), so that function's own name wins over the shared one.
    pub fn function_argument_of(self, name: &str, english: &str) -> Option<&'static str> {
        match self.named_function_help(name) {
            Some((_, _, arguments)) => arguments.iter().find(|(key, _)| *key == english).map(|(_, translated)| *translated),
            None => self.function_argument(english),
        }
    }

    /// The help this language keys by function name, for the function `name`.
    fn named_function_help(self, name: &str) -> Option<&'static ja_functions::FunctionHelp> {
        let table = match self {
            Self::Ja => ja_functions::FUNCTIONS,
            _ => return None,
        };
        let name = name.to_ascii_uppercase();
        table.binary_search_by(|(key, _, _)| (*key).cmp(name.as_str())).ok().and_then(|i| table.get(i))
    }

    /// The function help tables (descriptions, argument names) of the languages that have them.
    fn function_help(self) -> Option<(HelpTable, HelpTable)> {
        match self {
            Self::Es => Some((es_functions::DESCRIPTIONS, es_functions::ARGUMENTS)),
            _ => None,
        }
    }
}

/// The catalog key naming the colour `rgb` (0xRRGGBB) the way Office's colour pickers do, for a
/// style name: near-greys by lightness, the rest by hue band. Office's own accents land on its
/// names (0xFFC000 Gold, 0xA5A5A5 Gray, 0x4472C4 Blue, 0x4EA72E Green).
fn style_color(rgb: u32) -> &'static str {
    let [r, g, b] = [(rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF].map(|c| c as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let chroma = max - min;
    if chroma < 0.12 {
        return match (max + min) / 2.0 {
            l if l > 0.85 => "Style Color|White",
            l if l < 0.2 => "Style Color|Black",
            _ => "Style Color|Gray",
        };
    }
    let hue = if max == r {
        60.0 * ((g - b) / chroma)
    } else if max == g {
        60.0 * (2.0 + (b - r) / chroma)
    } else {
        60.0 * (4.0 + (r - g) / chroma)
    }
    .rem_euclid(360.0);
    match hue {
        h if !(15.0..345.0).contains(&h) => "Style Color|Red",
        h if h < 38.0 => "Style Color|Orange",
        h if h < 52.0 => "Style Color|Gold",
        h if h < 70.0 => "Style Color|Yellow",
        h if h < 165.0 => "Style Color|Green",
        h if h < 190.0 => "Style Color|Teal",
        h if h < 260.0 => "Style Color|Blue",
        h if h < 315.0 => "Style Color|Purple",
        _ => "Style Color|Pink",
    }
}

/// English text and its translation, sorted by the English.
type HelpTable = &'static [(&'static str, &'static str)];

/// The translation of `english` in a help table (binary search, so the table must stay sorted).
fn lookup(table: HelpTable, english: &str) -> Option<&'static str> {
    let i = table.binary_search_by(|(key, _)| (*key).cmp(english)).ok()?;
    table.get(i).map(|(_, translated)| *translated)
}

/// Deserializes a saved language leniently: an unknown or malformed value (a language removed in a
/// later version, a hand-edited `ui.json`) reads as English instead of failing the whole `UiState`,
/// which would reset every other preference with it.
pub fn lenient<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Language, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Code(String),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Raw::deserialize(d)? {
        Raw::Code(code) => Language::parse(&code).unwrap_or_default(),
        Raw::Other(_) => Language::default(),
    })
}

/// The language a saved `ui.json` asks for, if it names a supported one. The desktop app uses it
/// to tell "saved English" from "nothing usable saved" (first run, an older file, an unknown
/// value), which starts from [`Language::system`] instead.
pub fn saved_language(ui_json: &serde_json::Value) -> Option<Language> {
    ui_json.get("language").and_then(serde_json::Value::as_str).and_then(Language::parse)
}

/// egui context-data key holding the language of the frame being drawn. Widgets that only get a
/// `Ui` (not the app) read it to translate their labels; [`set_current`] writes it once per frame.
fn lang_key() -> egui::Id {
    egui::Id::unique("gridcraft.language")
}

/// The language the current frame is drawing in. `En` when unset.
pub fn current(ctx: &egui::Context) -> Language {
    ctx.data(|d| d.get_temp(lang_key())).unwrap_or_default()
}

/// Sets the language for the current frame; call once per frame before anything is drawn.
pub fn set_current(ctx: &egui::Context, language: Language) {
    ctx.data_mut(|d| d.insert_temp(lang_key(), language));
}

/// `name` lowercased with the Latin accents that occur in language names folded away, so a
/// byte-wise sort reads alphabetically. Other scripts are left as they are.
fn name_sort_key(name: &str) -> String {
    name.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'è'..='ë' | 'ē' | 'ě' | 'ę' | 'ế' | 'ệ' => 'e',
            'ì'..='ï' | 'ī' => 'i',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò'..='ö' | 'ø' | 'ő' => 'o',
            'ś' | 'š' => 's',
            'ù'..='ü' | 'ū' | 'ů' | 'ű' => 'u',
            'ý' | 'ÿ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shipped catalog names GridCraft's own Diagram and Decorative Text features, and
    /// Diagram never repeats that catalog's chart word (both sit on the Insert tab).
    #[test]
    fn every_catalog_has_distinct_diagram_and_decorative_text() {
        for language in Language::ALL.into_iter().filter(|l| *l != Language::En) {
            let code = language.code();
            let diagram = language.translated("Diagram");
            assert!(diagram.is_some(), "{code}: Diagram");
            assert!(language.translated("Decorative Text").is_some(), "{code}: Decorative Text");
            for chart in ["Chart", "Charts"] {
                if let Some(chart_text) = language.translated(chart) {
                    assert_ne!(diagram, Some(chart_text), "{code}: Diagram equals {chart}");
                }
            }
        }
    }

    /// Ukrainian Chart uses the shared craft-i18n term, which is not the Diagram word.
    #[test]
    fn ukrainian_chart_uses_the_shared_term() {
        assert_eq!(Language::Uk.translated("Chart"), Some("Діаграма"));
        assert_eq!(Language::Uk.translated("Insert Chart"), Some("Вставити діаграму"));
        assert_eq!(Language::Uk.translated("Diagram"), Some("Схема"));
    }

    /// Style galleries name a style by its accent's colour, family and number, as Excel does; the
    /// Japanese catalog uses Excel's 淡色 / 中間 / 濃色 family names.
    #[test]
    fn style_names_read_like_excels_gallery_tooltips() {
        let office = gridcraft_engine::model::Theme::default();
        assert_eq!(Language::En.style_name(&office, "TableStyleMedium7"), "Green, Table Style Medium 7");
        assert_eq!(Language::Ja.style_name(&office, "TableStyleMedium7"), "緑, テーブル スタイル (中間) 7");
        assert_eq!(Language::Ja.style_name(&office, "TableStyleLight1"), "白, テーブル スタイル (淡色) 1");
        assert_eq!(Language::Ja.style_name(&office, "TableStyleDark1"), "黒, テーブル スタイル (濃色) 1");
        assert_eq!(Language::Ja.style_name(&office, "TableStyleMedium3"), "オレンジ, テーブル スタイル (中間) 3");
        assert_eq!(Language::Ja.style_name(&office, "PivotStyleLight16"), "青, ピボットスタイル (淡色) 16");
        assert_eq!(Language::En.style_name(&office, "PivotStyleLight16"), "Blue, Pivot Style Light 16");
        // Office 2013–2022's accents keep the names its colour pickers give them.
        let mut office2013 = office.clone();
        office2013.colors[4..10].copy_from_slice(&[0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47]);
        let names: Vec<String> = (2..=7).map(|n| Language::Ja.style_name(&office2013, &format!("TableStyleMedium{n}"))).collect();
        let colors: Vec<&str> = names.iter().map(|n| n.split(',').next().unwrap_or_default()).collect();
        assert_eq!(colors, ["青", "オレンジ", "灰色", "ゴールド", "青", "緑"]);
        assert_eq!(Language::Ja.style_name(&office, "Custom Style"), "Custom Style");
    }

    /// Help tables are sorted and unique: lookups binary-search them.
    #[test]
    fn function_help_tables_are_sorted_and_unique() {
        for language in Language::ALL {
            let Some((description_table, argument_table)) = language.function_help() else { continue };
            for table in [description_table, argument_table] {
                assert!(table.windows(2).all(|pair| pair[0].0 < pair[1].0), "{language:?} function help is not sorted and unique");
                for (english, translated) in table {
                    assert!(!translated.is_empty(), "{language:?}: empty translation of {english:?}");
                }
            }
        }
    }

    /// Japanese help is keyed by function name: sorted, unique, and every argument it names is one
    /// of that function's arguments in the catalog (a renamed argument would fall back to English).
    #[test]
    fn japanese_function_help_matches_the_catalog() {
        assert!(ja_functions::FUNCTIONS.windows(2).all(|pair| pair[0].0 < pair[1].0), "ja function help is not sorted and unique");
        for (name, description, arguments) in ja_functions::FUNCTIONS {
            assert!(!description.is_empty(), "{name}");
            let signature = crate::formula_bar::function_signature(name).unwrap_or_else(|| panic!("{name} is not in the function catalog"));
            for (english, japanese) in *arguments {
                assert!(!japanese.is_empty(), "{name}: {english}");
                assert!(signature.contains(english), "{name}: {english} is not an argument of {signature}");
            }
        }
        assert_eq!(Language::Ja.function_argument_of("vlookup", "range_lookup"), Some("検索方法"));
        assert_eq!(Language::Ja.function_argument_of("SUMIFS", "criteria"), Some("条件"));
        assert_eq!(Language::Ja.function_argument_of("COUNTIF", "criteria"), Some("検索条件"));
        assert_eq!(Language::Es.function_argument_of("VLOOKUP", "range_lookup"), Some("ordenado"));
        assert_eq!(Language::Ja.function_description_of("BESSELJ", "Bessel function Jn(x)."), None);
    }

    /// A Japanese hint shows all its arguments in Japanese or none (`formula_locale::signature`),
    /// so a function the Japanese help lists must name every argument of its catalog signature.
    #[test]
    fn japanese_function_help_names_every_argument() {
        use gridcraft_engine::formula::FormulaLocale::En;
        let untranslated: Vec<String> = ja_functions::FUNCTIONS
            .iter()
            .filter(|(_, _, arguments)| !arguments.is_empty())
            .filter_map(|(name, _, _)| {
                crate::formula_locale::signature(name, En, Language::Ja)
                    .filter(|hint| Some(hint) == crate::formula_locale::signature(name, En, Language::En).as_ref())
            })
            .collect();
        assert!(untranslated.is_empty(), "Japanese help misses arguments of {untranslated:#?}");
    }

    #[test]
    fn untranslated_text_falls_back_to_english() {
        let label = |key: &'static str| key.rsplit_once('|').map_or(key, |(_, label)| label);
        for (english, _) in ja::CORE {
            assert_eq!(Language::En.tr(english), label(english));
        }
        for language in Language::ALL {
            assert_eq!(language.tr("Sheet1!A1"), "Sheet1!A1");
            assert_eq!(language.tr("Not in any catalog|Untranslated"), "Untranslated");
        }
        // A `context|label` key shows only its label in English.
        assert_eq!(Language::En.tr("Page Layout|Orientation"), "Orientation");
        // Errors drop the `command `id`:` prefix before translating; an untranslated one stays whole.
        assert_eq!(Language::PtBr.tr_error("command `chart`: Select a chart first."), Language::PtBr.tr("Select a chart first."));
        assert_eq!(Language::PtBr.tr_error("Sheet|Unknown failure"), "Sheet|Unknown failure");
        assert_eq!(Language::PtBr.tr_error("Printer Office: permission denied"), "Printer Office: permission denied");
        assert_eq!(Language::parse("xx"), None);
    }

    /// A catalog whose translations slip onto their neighbours' keys still covers every key, so
    /// check the shape of each translation against its English: placeholders survive, and a
    /// one-line label (a menu item, a tooltip) doesn't turn into a two-line ribbon label.
    #[test]
    fn translations_keep_the_shape_of_their_english() {
        let placeholders =
            |s: &str| s.split('{').skip(1).filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_string())).collect::<Vec<_>>();
        for language in Language::ALL {
            for (english, _) in ja::CORE {
                let label = english.rsplit_once('|').map_or(*english, |(_, label)| label);
                let text = language.tr(english);
                assert_eq!(placeholders(text), placeholders(label), "{language:?}: {english:?} => {text:?}");
                // Big ribbon buttons whose English fits on one line may still need two.
                if !label.contains('\n') && !["AutoSum", "PivotTable"].contains(english) {
                    assert!(!text.contains('\n'), "{language:?}: one-line {english:?} => {text:?}");
                }
            }
        }
    }

    /// The screen-reader names for the formula-bar controls must be translated in every catalog
    /// (no silent English fallback) and stay single-line labels.
    #[test]
    fn accessible_control_names_are_translated_in_every_catalog() {
        for key in ["Name Box", "Cell Editor", "Font Size", "Sheet Name", "Text Box"] {
            for language in Language::ALL {
                let shown = language.tr(key);
                assert!(!shown.contains('\n'), "{}: {key:?} is not a single-line label: {shown:?}", language.code());
                if language != Language::En {
                    assert_ne!(shown, key, "{} falls back to English for {key:?}", language.code());
                }
            }
        }
    }

    #[test]
    fn translations_never_show_the_context_prefix() {
        // `tr` returns a found translation as it is, so a catalog that keeps `context|` in its
        // value shows it on screen (File › Options read "Options|Général" in French).
        let keys = ja::CORE.iter().map(|(english, _)| *english).chain(["Options|General", "Options|Save", "Sheet Options|View"]);
        for english in keys {
            for language in Language::ALL {
                let shown = language.tr(english);
                assert!(!shown.contains('|'), "{} shows a context prefix for {english}: {shown}", language.code());
            }
        }
    }

    #[test]
    fn unknown_saved_language_keeps_the_other_preferences() {
        let ui: crate::UiState = serde_json::from_str(r#"{"dark": true, "language": "xx"}"#).unwrap();
        assert!(ui.dark);
        assert_eq!(ui.language, Language::En);
        let ui: crate::UiState = serde_json::from_str(r#"{"dark": true, "language": 7}"#).unwrap();
        assert!(ui.dark);
        let ui: crate::UiState = serde_json::from_str(r#"{"language": "ja"}"#).unwrap();
        assert_eq!(ui.language, Language::Ja);
        assert_eq!(crate::UiState::default().language, Language::En, "the default never reads the host locale");
        assert_eq!(saved_language(&serde_json::json!({"language": "ja"})), Some(Language::Ja));
        assert_eq!(saved_language(&serde_json::json!({"language": "xx"})), None);
        assert_eq!(saved_language(&serde_json::json!({})), None);
    }

    #[test]
    fn tags_reduce_to_supported_languages() {
        assert_eq!(Language::from_tag("ja-JP"), Some(Language::Ja));
        assert_eq!(Language::from_tag("en_US"), Some(Language::En));
        assert_eq!(Language::from_tag("es_MX"), Some(Language::Es));
        assert_eq!(Language::from_tag("el-GR"), Some(Language::El));
        assert_eq!(Language::from_tag("el_GR.UTF-8"), Some(Language::El));
        assert_eq!(Language::parse("el"), Some(Language::El));
        assert_eq!(Language::from_tag("zh-Hans-CN"), Some(Language::Zh));
        assert_eq!(Language::from_tag("ko-KR"), Some(Language::Ko));
        assert_eq!(Language::from_tag("RU_ru"), Some(Language::Ru));
        assert_eq!(Language::from_tag("de-DE"), Some(Language::De));
        assert_eq!(Language::from_tag("de_AT"), Some(Language::De));
    }

    #[test]
    fn tags_reduce_to_supported_languages_with_locale_suffixes() {
        assert_eq!(Language::from_tag("de-CH"), Some(Language::De));
        assert_eq!(Language::from_tag("pt-BR"), Some(Language::PtBr));
        assert_eq!(Language::parse("pt"), Some(Language::PtBr));
        assert_eq!(Language::parse("pt-br"), Some(Language::PtBr));
        assert_eq!(Language::from_tag("cs-CZ"), Some(Language::Cs));
        assert_eq!(Language::from_tag("cs_CZ.UTF-8"), Some(Language::Cs));
        assert_eq!(Language::parse("cs"), Some(Language::Cs));
        assert_eq!(Language::from_tag("fr-FR"), Some(Language::Fr));
        assert_eq!(Language::from_tag("fr_CA"), Some(Language::Fr));
        assert_eq!(Language::parse("fr"), Some(Language::Fr));
        assert_eq!(Language::from_tag("sr-Latn-RS"), Some(Language::Sr));
    }

    #[test]
    fn tags_reduce_to_other_supported_languages() {
        assert_eq!(Language::from_tag("sr-RS"), Some(Language::Sr));
        assert_eq!(Language::parse("sr"), Some(Language::Sr));
        assert_eq!(Language::from_tag("tr-TR"), Some(Language::Tr));
        assert_eq!(Language::from_tag("tr_TR.UTF-8"), Some(Language::Tr));
        assert_eq!(Language::parse("tr"), Some(Language::Tr));
        assert_eq!(Language::from_tag("pl-PL"), Some(Language::Pl));
        assert_eq!(Language::from_tag("pl_PL.UTF-8"), Some(Language::Pl));
        assert_eq!(Language::parse("pl"), Some(Language::Pl));
        assert_eq!(Language::from_tag("it-IT"), Some(Language::It));
        assert_eq!(Language::from_tag("it_CH.UTF-8"), Some(Language::It));
        assert_eq!(Language::parse("it"), Some(Language::It));
    }

    #[test]
    fn tags_reduce_to_remaining_supported_languages() {
        assert_eq!(Language::from_tag("uk-XX"), Some(Language::Uk));
        assert_eq!(Language::parse("uk"), Some(Language::Uk));
        assert_eq!(Language::from_tag("vi-VN"), Some(Language::Vi));
        assert_eq!(Language::parse("vi"), Some(Language::Vi));
        assert_eq!(Language::from_tag("nl-NL"), Some(Language::Nl));
        assert_eq!(Language::from_tag("nl_BE"), Some(Language::Nl));
        assert_eq!(Language::parse("nl"), Some(Language::Nl));
        assert_eq!(Language::from_tag("ro-RO"), Some(Language::Ro));
        assert_eq!(Language::from_tag("ro_MD.UTF-8"), Some(Language::Ro));
        assert_eq!(Language::parse("ro"), Some(Language::Ro));
    }
    #[test]
    fn an_unset_language_preference_is_english_until_the_app_applies_the_system_one() {
        // Deserializing never reads the host locale (tests stay deterministic); the desktop app
        // applies `Language::system()` itself when `ui.json` names no usable language.
        let restored: crate::UiState = serde_json::from_str("{}").unwrap();
        assert_eq!(restored.language, Language::En);
        assert_eq!(saved_language(&serde_json::json!({"language": "zh"})), Some(Language::Zh));
    }

    #[test]
    fn the_japanese_interface_reads_entries_in_the_japanese_region() {
        let mut app = crate::SheetApp::new(gridcraft_engine::Session::default(), Default::default());
        app.session.new_workbook();
        app.run("app.language.set", serde_json::json!({"language": "ja"})).unwrap();
        assert_eq!(app.session.prefs.region, "ja-JP");
        assert!(app.session.active().is_some_and(|d| d.wb.region.is_japanese()));
        app.run("cell.set", serde_json::json!({"cell": "A1", "input": "11/10"})).unwrap();
        let d = app.session.active().unwrap();
        let sh = d.wb.active().unwrap();
        let a1 = gridcraft_engine::core::CellRef::new(0, 0);
        assert_eq!(gridcraft_engine::display::format(&sh.value(a1), d.wb.styles.get(sh.style_id(a1)).num_fmt.as_str(), &d.wb).text, "11月10日");
        // An explicit region holds until the language changes again.
        app.run("app.region.set", serde_json::json!({"region": "en-US"})).unwrap();
        app.follow_language_region();
        assert_eq!(app.session.prefs.region, "en-US");
        app.run("app.language.english", serde_json::json!({})).unwrap();
        app.run("app.language.japanese", serde_json::json!({})).unwrap();
        assert_eq!(app.session.prefs.region, "ja-JP");
        app.run("app.language.set", serde_json::json!({"language": "de"})).unwrap();
        assert_eq!(app.session.prefs.region, "en-US", "only Japanese has its entry rules modelled");
    }

    #[test]
    fn language_commands_switch_and_persist() {
        let mut app = crate::SheetApp::new(gridcraft_engine::Session::default(), Default::default());
        for language in Language::ALL {
            app.run("app.language.set", serde_json::json!({"language": language.code()})).unwrap();
            assert_eq!(app.ui.language, language);
            let restored: crate::UiState = serde_json::from_str(&serde_json::to_string(&app.ui).unwrap()).unwrap();
            assert_eq!(restored.language, language);
        }
        app.run("app.language.set", serde_json::json!({"code": "ja"})).unwrap();
        assert_eq!(app.ui.language, Language::Ja, "`code` is an alias for `language`");
        assert!(app.run("app.language.set", serde_json::json!({})).is_err(), "no language is an error, not a dialog");
        let unknown = app.run("app.language.set", serde_json::json!({"language": "xx"})).unwrap_err().to_string();
        for l in Language::ALL {
            assert!(unknown.contains(&format!("\"{}\"", l.code())), "the error lists {:?}: {unknown}", l.code());
        }
        app.run("app.language.english", serde_json::json!({})).unwrap();
        assert_eq!(app.ui.language, Language::En);
        for l in Language::ALL {
            assert_eq!(Language::parse(l.code()), Some(l));
        }
    }

    #[test]
    fn picker_lists_english_first_then_names_alphabetically() {
        let order = Language::for_picker();
        assert_eq!(order.first(), Some(&Language::En));
        assert_eq!(order.len(), Language::ALL.len());
        let keys: Vec<String> = order[1..].iter().map(|l| name_sort_key(l.name())).collect();
        assert!(keys.windows(2).all(|w| w[0] <= w[1]), "{keys:?}");
        assert_eq!(name_sort_key("Čeština"), "cestina");
    }
}
