use super::show::ok_cancel;
use super::{Dialog, Json, SheetApp, json, theme};

mod border;
mod number;

use number::{Category, Options};

/// Draws the six-tab Format Cells editor (number, alignment, font, border, fill, protection); the Number tab shows a sample preview, and the style draft updates.
pub(super) fn format_cells(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    // `d.tab` stays the English name (it is what `open_dialog` and tests pass); only the drawn
    // labels are translated. The Fill tab is keyed by context: the Alignment tab's "Fill" is a
    // different word in some languages.
    let lang = app.ui.language;
    // A conditional format's format (opened by Format… in a rule dialog) has only the tabs and
    // properties a rule may set, as in Excel: no alignment or protection, no font face or size.
    let dxf = d.values.get("dxf").and_then(serde_json::Value::as_bool) == Some(true);
    ui.horizontal(|ui| {
        for (tab, key) in [
            ("Number", "Format Cells|Number"),
            ("Alignment", "Alignment"),
            ("Font", "Font"),
            ("Border", "Border"),
            ("Fill", "Format Cells|Fill"),
            ("Protection", "Protection"),
        ] {
            if dxf && matches!(tab, "Alignment" | "Protection") {
                continue;
            }
            if ui.selectable_label(d.tab == tab, lang.tr(key)).clicked() {
                d.tab = tab.to_string();
            }
        }
    });
    ui.separator();
    let mut st: gridcraft_engine::model::Style = d.values.get("style").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    // The style when the dialog opened: OK applies only what was changed, as Excel does, so a
    // selection of differently formatted cells keeps what the dialog didn't touch.
    if !d.values.contains_key("style0")
        && let Some(v) = d.values.get("style").cloned()
    {
        d.values.insert("style0".into(), v);
    }
    let sample = app.session.active().and_then(|doc| doc.wb.active().map(|s| s.value(doc.selection.active))).unwrap_or_default();
    let ja = app.session.region().is_japanese();
    match d.tab.as_str() {
        "Number" => number_tab(app, ui, d, &mut st, &sample, ja),
        "Alignment" => alignment_tab(app, ui, d, &mut st),
        "Font" if dxf => dxf_font(app, ui, &mut st),
        "Font" => {
            craft_ui::ComboBox::from_label(lang.tr("Font")).selected_text(st.font.name.clone()).show_ui(ui, |ui| {
                for f in crate::fonts::families() {
                    ui.selectable_value(&mut st.font.name, f.clone(), f.clone());
                }
            });
            ui.add(egui::Slider::new(&mut st.font.size, 6.0..=72.0).text(lang.tr("Size")));
            ui.horizontal(|ui| {
                ui.checkbox(&mut st.font.bold, lang.tr("Bold"));
                ui.checkbox(&mut st.font.italic, lang.tr("Italic"));
                ui.checkbox(&mut st.font.strike, lang.tr("Strikethrough"));
            });
            use gridcraft_engine::model::{Underline, VertAlign};
            ui.horizontal(|ui| {
                for (u, n) in [
                    (Underline::None, "None"),
                    (Underline::Single, "Single"),
                    (Underline::Double, "Double"),
                    (Underline::SingleAccounting, "Single Accounting"),
                    (Underline::DoubleAccounting, "Double Accounting"),
                ] {
                    ui.radio_value(&mut st.font.underline, u, lang.tr(n));
                }
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut st.font.vert, VertAlign::Baseline, lang.tr("Normal"));
                ui.radio_value(&mut st.font.vert, VertAlign::Superscript, lang.tr("Superscript"));
                ui.radio_value(&mut st.font.vert, VertAlign::Subscript, lang.tr("Subscript"));
            });
            ui.label(lang.tr("Color:"));
            let colors = app.session.active().map(|doc| doc.wb.theme.colors).unwrap_or_default();
            if let Some(c) = crate::widgets::color_palette(ui, &colors, lang.tr("Automatic")) {
                st.font.color =
                    if c == "none" { gridcraft_engine::model::Color::Auto } else { gridcraft_engine::model::Color::from_hex(&c).unwrap_or_default() };
            }
        }
        "Border" => {
            let theme = app.session.active().map(|doc| &doc.wb.theme).cloned().unwrap_or_default();
            let mut draft = border_draft(app, d, &st, dxf);
            border::tab(ui, lang, &mut draft, &theme);
            if dxf {
                draft.apply_to(&mut st.borders);
            }
            if let Ok(v) = serde_json::to_value(&draft) {
                d.values.insert("border".into(), v);
            }
        }
        "Fill" => {
            let colors = app.session.active().map(|doc| doc.wb.theme.colors).unwrap_or_default();
            ui.label(lang.tr("Background Color:"));
            if let Some(c) = crate::widgets::color_palette(ui, &colors, lang.tr("No Color")) {
                st.fill = if c == "none" {
                    Default::default()
                } else {
                    gridcraft_engine::model::Fill::solid(gridcraft_engine::model::Color::from_hex(&c).unwrap_or_default())
                };
            }
        }
        "Protection" => {
            ui.checkbox(&mut st.protection.locked, lang.tr("Locked"));
            ui.checkbox(&mut st.protection.hidden, lang.tr("Hidden"));
            ui.label(
                egui::RichText::new(
                    lang.tr("Locking cells or hiding formulas has no effect until you protect the worksheet (Review › Protect Sheet)."),
                )
                .small(),
            );
        }
        _ => {}
    }
    if dxf {
        // Clear empties what the shown tab sets, as Excel's Clear does.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(lang.tr("Format Cells|Clear")).clicked() {
                    let none = gridcraft_engine::model::Style::default();
                    match d.tab.as_str() {
                        "Number" => st.num_fmt = none.num_fmt,
                        "Font" => {
                            st.font = none.font;
                            st.dxf_stated = Default::default();
                        }
                        "Border" => st.borders = none.borders,
                        "Fill" => st.fill = none.fill,
                        _ => {}
                    }
                }
            });
        });
    }
    if let Ok(v) = serde_json::to_value(&st) {
        d.values.insert("style".into(), v);
    }
    let mut open = true;
    ok_cancel(ui, confirm, &mut open);
    if *confirm && dxf {
        // The format goes back to the rule dialog; a quick rule dialog now uses it, not a preset.
        if let Some(back) = d.back.as_deref_mut()
            && let Some(style) = d.values.get("style").cloned()
        {
            back.values.insert("style".into(), style);
            if back.name == "cfQuick" {
                back.values.insert("preset".into(), json!("custom"));
            }
        }
    } else if *confirm && let Some(style) = d.values.get("style").cloned() {
        apply(app, d, style);
    }
    if !open {
        app.dialog = None;
        *confirm = true; // closes
    }
}

/// The Font tab for a conditional format: Font style, Underline, Color and Strikethrough, the
/// font properties a rule may set (Excel greys out the face, size and the other effects).
fn dxf_font(app: &SheetApp, ui: &mut egui::Ui, st: &mut gridcraft_engine::model::Style) {
    use gridcraft_engine::model::Underline;
    let lang = app.ui.language;
    ui.label(egui::RichText::new(lang.tr("Font style:")).strong());
    ui.horizontal(|ui| {
        for (bold, italic, name) in
            [(false, false, "Font Style|Regular"), (false, true, "Italic"), (true, false, "Bold"), (true, true, "Bold Italic")]
        {
            if ui.radio(st.font.bold == bold && st.font.italic == italic, lang.tr(name)).clicked() {
                st.font.bold = bold;
                st.font.italic = italic;
                // Regular states both off, so it overrides a bold or italic cell; the other
                // styles state only what they turn on.
                let regular = !bold && !italic;
                st.dxf_stated.bold = regular;
                st.dxf_stated.italic = regular;
            }
        }
    });
    ui.label(egui::RichText::new(lang.tr("Underline:")).strong());
    ui.horizontal(|ui| {
        for (u, n) in [(Underline::None, "None"), (Underline::Single, "Single"), (Underline::Double, "Double")] {
            ui.radio_value(&mut st.font.underline, u, lang.tr(n));
        }
    });
    ui.checkbox(&mut st.font.strike, lang.tr("Strikethrough"));
    ui.label(lang.tr("Color:"));
    let colors = app.session.active().map(|doc| doc.wb.theme.colors).unwrap_or_default();
    if let Some(c) = crate::widgets::color_palette(ui, &colors, lang.tr("Automatic")) {
        st.font.color =
            if c == "none" { gridcraft_engine::model::Color::Auto } else { gridcraft_engine::model::Color::from_hex(&c).unwrap_or_default() };
    }
}

/// Applies the dialog: the style properties changed since it opened, the Border tab's changed
/// edges, and Merge cells when its box was changed.
fn apply(app: &mut SheetApp, d: &Dialog, style: Json) {
    let base = d.values.get("style0").cloned().unwrap_or(Json::Null);
    let mut patch = style_patch(&base, &style);
    // Borders go as `edges`: the Border tab sets lines on the selection's outline and between its
    // cells, which one style for every cell can't say.
    if let Some(Json::Object(m)) = &mut patch {
        m.remove("borders");
        if m.is_empty() {
            patch = None;
        }
    }
    let edges = d.values.get("border").and_then(|v| serde_json::from_value::<border::Draft>(v.clone()).ok()).and_then(|b| b.edges());
    let mut params = serde_json::Map::new();
    if let Some(patch) = patch {
        params.insert("style".into(), patch);
    }
    if let Some(edges) = edges {
        params.insert("edges".into(), edges);
    }
    if !params.is_empty() {
        app.run_or_alert("home.formatCells", Json::Object(params));
    }
    let flag = |k: &str| d.values.get(k).and_then(Json::as_bool);
    if let (Some(was), Some(now)) = (flag("merge0"), flag("merge"))
        && was != now
    {
        app.run_or_alert(if now { "home.mergeCells" } else { "home.unmergeCells" }, json!({}));
    }
}

/// The part of `new` that differs from `old`, as a patch `home.formatCells` merges: changed
/// fields only, an enum value (`{"Rgb": …}`) whole. `None` when nothing changed.
fn style_patch(old: &Json, new: &Json) -> Option<Json> {
    let is_enum = |m: &serde_json::Map<String, Json>| m.keys().any(|k| k.starts_with(|c: char| c.is_ascii_uppercase()));
    match (old, new) {
        (Json::Object(a), Json::Object(b)) if !is_enum(a) && !is_enum(b) => {
            let mut out = serde_json::Map::new();
            for (k, v) in b {
                let changed = match a.get(k) {
                    Some(old) => style_patch(old, v),
                    None => Some(v.clone()),
                };
                if let Some(changed) = changed {
                    out.insert(k.clone(), changed);
                }
            }
            (!out.is_empty()).then_some(Json::Object(out))
        }
        _ => (old != new).then(|| new.clone()),
    }
}

/// The Border tab's draft, made when the tab is first shown: from the selection's cells, or for a
/// conditional format from its own borders.
fn border_draft(app: &SheetApp, d: &Dialog, st: &gridcraft_engine::model::Style, dxf: bool) -> border::Draft {
    if let Some(draft) = d.values.get("border").and_then(|v| serde_json::from_value(v.clone()).ok()) {
        return draft;
    }
    if dxf {
        return border::draft_for_borders(&st.borders);
    }
    match app.session.active() {
        Some(doc) => {
            let range = doc.selection.ranges.last().copied().unwrap_or_else(|| gridcraft_engine::core::RangeRef::cell(doc.selection.active));
            border::draft_for_selection(&doc.wb, range)
        }
        None => border::draft_for_borders(&st.borders),
    }
}

/// A scrolling list of rows (text, colour); returns the row clicked.
fn pick_list(ui: &mut egui::Ui, id: &str, rows: &[(String, Option<egui::Color32>)], selected: Option<usize>, height: f32) -> Option<usize> {
    let mut picked = None;
    egui::Frame::NONE.stroke(egui::Stroke::new(1.0, theme::Tokens::get(ui.ctx()).input_border)).inner_margin(2.0).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt(id).max_height(height).auto_shrink([false, true]).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            for (i, (text, color)) in rows.iter().enumerate() {
                let mut rich = egui::RichText::new(text);
                if let Some(c) = color {
                    rich = rich.color(*c);
                }
                if ui.selectable_label(selected == Some(i), rich).clicked() {
                    picked = Some(i);
                }
            }
        });
    });
    picked
}

/// The Number tab: Category on the left; Sample and the category's options on the right.
fn number_tab(
    app: &mut SheetApp,
    ui: &mut egui::Ui,
    d: &mut Dialog,
    st: &mut gridcraft_engine::model::Style,
    sample: &gridcraft_engine::core::Value,
    ja: bool,
) {
    use gridcraft_engine::core::Value;
    let lang = app.ui.language;
    let code_now = st.num_fmt.as_str().to_string();
    let mut o: Options = d.values.get("nf").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_else(|| number::detect(&code_now, ja));
    let mut text: String = d.values.get("nfText").and_then(Json::as_str).map(str::to_string).unwrap_or_else(|| number::shown_code(&code_now, ja));
    let (o0, text0) = (o, text.clone());
    let mut delete: Option<String> = None;
    {
        let Some(doc) = app.session.active() else { return };
        let wb = &doc.wb;
        let show = |v: &Value, code: &str| {
            let f = gridcraft_engine::display::format(v, code, wb);
            let red = matches!(f.color, Some(gridcraft_engine::numfmt::FormatColor::Red)).then_some(egui::Color32::from_rgb(0xD0, 0x10, 0x10));
            (f.text, red)
        };
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(150.0);
                ui.label(lang.tr("Category:"));
                let rows: Vec<(String, Option<egui::Color32>)> = Category::ALL.iter().map(|c| (lang.tr(c.label()).to_string(), None)).collect();
                let at = Category::ALL.iter().position(|c| *c == o.category);
                if let Some(i) = pick_list(ui, "nfCategory", &rows, at, 300.0)
                    && Category::ALL[i] != o.category
                {
                    let shown = if o.category == Category::Custom { number::stored_code(&text) } else { number::build(&o, ja) };
                    o = Options::new(Category::ALL[i], ja);
                    if o.category == Category::Custom {
                        text = number::shown_code(&shown, ja);
                    }
                }
            });
            ui.vertical(|ui| {
                ui.set_width(330.0);
                let code = if o.category == Category::Custom {
                    gridcraft_engine::numfmt::canonical_code(&number::stored_code(&text))
                } else {
                    number::build(&o, ja)
                };
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_width(310.0);
                    ui.label(lang.tr("Sample"));
                    let (preview, red) = if matches!(sample, Value::Empty) { (String::new(), None) } else { show(sample, &code) };
                    let mut rich = egui::RichText::new(preview).font(theme::ui_font(15.0));
                    if let Some(c) = red {
                        rich = rich.color(c);
                    }
                    ui.label(rich);
                });
                ui.add_space(4.0);
                if o.category.has_decimals() {
                    ui.horizontal(|ui| {
                        let l = ui.label(lang.tr("Decimal places:"));
                        ui.add(egui::DragValue::new(&mut o.decimals).range(0..=30)).labelled_by(l.id);
                    });
                }
                match o.category {
                    Category::General => {
                        ui.label(lang.tr("General format cells have no specific number format."));
                    }
                    Category::Text => {
                        ui.label(lang.tr(
                            "Text format cells are treated as text even when a number is in the cell. The cell is displayed exactly as entered.",
                        ));
                    }
                    Category::Number | Category::Currency => {
                        if o.category == Category::Number {
                            ui.checkbox(&mut o.separator, lang.tr("Use 1000 Separator (,)"));
                        } else {
                            symbol_combo(ui, lang, &mut o.symbol);
                        }
                        ui.label(lang.tr("Negative numbers:"));
                        let rows: Vec<_> = (0..number::negative_count(o.category, ja))
                            .map(|negative| show(&Value::Number(-1234.10), &number::build(&Options { negative, ..o }, ja)))
                            .collect();
                        if let Some(i) = pick_list(ui, "nfNegative", &rows, Some(o.negative), 110.0) {
                            o.negative = i;
                        }
                    }
                    Category::Accounting => symbol_combo(ui, lang, &mut o.symbol),
                    Category::Date | Category::Time => {
                        // Excel shows each type on 2012/3/14 13:30:55; a star marks the system formats.
                        let when = Value::Number(40982.0 + (13.0 * 3600.0 + 30.0 * 60.0 + 55.0) / 86400.0);
                        ui.label(lang.tr("Type:"));
                        let list = if o.category == Category::Date { number::date_list(ja, o.era) } else { number::time_list(ja) };
                        let rows: Vec<_> =
                            list.iter().map(|(c, star)| (format!("{}{}", if *star { "*" } else { "" }, show(&when, c).0), None)).collect();
                        if let Some(i) = pick_list(ui, "nfKind", &rows, Some(o.kind), 160.0) {
                            o.kind = i;
                        }
                        if o.category == Category::Date && ja {
                            ui.horizontal(|ui| {
                                let l = ui.label(lang.tr("Calendar type:"));
                                let shown = lang.tr(if o.era { "Japanese Era" } else { "Gregorian" });
                                craft_ui::ComboBox::from_id_salt("nfCalendar")
                                    .selected_text(shown)
                                    .show_ui(ui, |ui| {
                                        for (era, name) in [(false, "Gregorian"), (true, "Japanese Era")] {
                                            if ui.selectable_label(o.era == era, lang.tr(name)).clicked() && o.era != era {
                                                o.era = era;
                                                o.kind = 0;
                                            }
                                        }
                                    })
                                    .response
                                    .labelled_by(l.id);
                            });
                        }
                    }
                    Category::Fraction | Category::Special => {
                        ui.label(lang.tr("Type:"));
                        let list: Vec<&str> = if o.category == Category::Fraction {
                            number::FRACTIONS.iter().map(|(l, _)| *l).collect()
                        } else {
                            number::special_list(ja).iter().map(|(l, _)| *l).collect()
                        };
                        let rows: Vec<_> = list.iter().map(|l| (lang.tr(l).to_string(), None)).collect();
                        if let Some(i) = pick_list(ui, "nfKind", &rows, Some(o.kind), 160.0) {
                            o.kind = i;
                        }
                    }
                    Category::Percentage | Category::Scientific => {}
                    Category::Custom => {
                        let l = ui.label(lang.tr("Type:"));
                        ui.add(egui::TextEdit::singleline(&mut text).desired_width(300.0)).labelled_by(l.id);
                        let builtin = number::custom_list(ja);
                        // Gathered once from the workbook's cells (again after Delete).
                        let own: Vec<String> = match d.values.get("nfOwn").and_then(|v| serde_json::from_value(v.clone()).ok()) {
                            Some(own) => own,
                            None => {
                                let own = number::workbook_codes(wb, ja);
                                d.values.insert("nfOwn".into(), json!(own));
                                own
                            }
                        };
                        let codes: Vec<&str> = builtin.iter().copied().chain(own.iter().map(String::as_str)).collect();
                        let rows: Vec<_> = codes.iter().map(|c| (number::shown_code(c, ja), None)).collect();
                        let at = codes.iter().position(|c| *c == code);
                        if let Some(i) = pick_list(ui, "nfCustom", &rows, at, 150.0) {
                            text = number::shown_code(codes[i], ja);
                        }
                        // Only the workbook's own codes can be deleted, as in Excel.
                        let deletable = own.iter().any(|c| *c == code);
                        if ui.add_enabled(deletable, egui::Button::new(lang.tr("Delete"))).clicked() {
                            delete = Some(code.clone());
                        }
                    }
                }
            });
        });
    }
    if o != o0 || text != text0 {
        let code = if o.category == Category::Custom {
            gridcraft_engine::numfmt::canonical_code(&number::stored_code(&text))
        } else {
            let code = number::build(&o, ja);
            text = number::shown_code(&code, ja);
            code
        };
        st.num_fmt = gridcraft_engine::model::NumFmt::new(&code);
    }
    if let Some(code) = delete {
        // Cells in the deleted format return to General, so the box shows General too.
        app.run_or_alert("home.deleteNumberFormat", json!({"code": code}));
        st.num_fmt = gridcraft_engine::model::NumFmt::new("General");
        if let Some(Json::Object(m)) = d.values.get_mut("style0") {
            m.insert("num_fmt".into(), json!("General"));
        }
        text = number::shown_code("General", ja);
        d.values.remove("nfOwn");
    }
    if let Ok(v) = serde_json::to_value(o) {
        d.values.insert("nf".into(), v);
    }
    d.values.insert("nfText".into(), json!(text));
}

fn symbol_combo(ui: &mut egui::Ui, lang: crate::i18n::Language, symbol: &mut usize) {
    ui.horizontal(|ui| {
        let l = ui.label(lang.tr("Symbol:"));
        let shown = lang.tr(number::SYMBOLS.get(*symbol).map_or("", |(n, _)| *n));
        craft_ui::ComboBox::from_id_salt("nfSymbol")
            .width(180.0)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (i, (name, _)) in number::SYMBOLS.iter().enumerate() {
                    ui.selectable_value(symbol, i, lang.tr(name));
                }
            })
            .response
            .labelled_by(l.id);
    });
}

/// The Alignment tab, as Excel's: Text alignment (Horizontal with Indent, Vertical), Text control
/// (Wrap text, Shrink to fit, Merge cells), Right-to-left, and Orientation with the Vertical Text
/// box, the dial and Degrees.
fn alignment_tab(app: &SheetApp, ui: &mut egui::Ui, d: &mut Dialog, st: &mut gridcraft_engine::model::Style) {
    use gridcraft_engine::model::{HAlign, ReadingOrder, VAlign};
    let lang = app.ui.language;
    if !d.values.contains_key("merge0") {
        let merged = app.session.active().and_then(|doc| doc.wb.active().map(|sh| sh.merge_at(doc.selection.active).is_some())).unwrap_or(false);
        d.values.insert("merge0".into(), json!(merged));
        d.values.insert("merge".into(), json!(merged));
    }
    let mut merge = d.values.get("merge").and_then(Json::as_bool).unwrap_or(false);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(300.0);
            ui.label(egui::RichText::new(lang.tr("Text alignment")).strong());
            let h_names = [
                (HAlign::General, "Horizontal Alignment|General"),
                (HAlign::Left, "Left (Indent)"),
                (HAlign::Center, "Horizontal Alignment|Center"),
                (HAlign::Right, "Right (Indent)"),
                (HAlign::Fill, "Horizontal Alignment|Fill"),
                (HAlign::Justify, "Horizontal Alignment|Justify"),
                (HAlign::CenterAcross, "Center Across Selection"),
                (HAlign::Distributed, "Distributed (Indent)"),
            ];
            let l = ui.label(lang.tr("Horizontal:"));
            let shown = h_names.iter().find(|(h, _)| *h == st.align.h).map_or("", |(_, n)| lang.tr(n));
            craft_ui::ComboBox::from_id_salt("alignH")
                .width(200.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (h, n) in h_names {
                        if ui.selectable_value(&mut st.align.h, h, lang.tr(n)).clicked()
                            && !matches!(h, HAlign::Left | HAlign::Right | HAlign::Distributed)
                        {
                            st.align.indent = 0;
                        }
                    }
                })
                .response
                .labelled_by(l.id);
            ui.horizontal(|ui| {
                let l = ui.label(lang.tr("Indent:"));
                let mut indent = st.align.indent;
                let r = ui.add(egui::DragValue::new(&mut indent).range(0..=250)).labelled_by(l.id);
                if r.changed() {
                    st.align.indent = indent;
                    // An indent needs a side to count from: General and Center become Left, as in Excel.
                    if indent > 0 && !matches!(st.align.h, HAlign::Left | HAlign::Right | HAlign::Distributed) {
                        st.align.h = HAlign::Left;
                    }
                }
            });
            let v_names = [
                (VAlign::Top, "Vertical Alignment|Top"),
                (VAlign::Center, "Vertical Alignment|Center"),
                (VAlign::Bottom, "Vertical Alignment|Bottom"),
                (VAlign::Justify, "Vertical Alignment|Justify"),
                (VAlign::Distributed, "Vertical Alignment|Distributed"),
            ];
            let l = ui.label(lang.tr("Vertical:"));
            let shown = v_names.iter().find(|(v, _)| *v == st.align.v).map_or("", |(_, n)| lang.tr(n));
            craft_ui::ComboBox::from_id_salt("alignV")
                .width(200.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (v, n) in v_names {
                        ui.selectable_value(&mut st.align.v, v, lang.tr(n));
                    }
                })
                .response
                .labelled_by(l.id);
            ui.add_space(6.0);
            ui.label(egui::RichText::new(lang.tr("Text control")).strong());
            ui.checkbox(&mut st.align.wrap, lang.tr("Wrap text"));
            // Excel greys Shrink to fit while Wrap text is on.
            ui.add_enabled(!st.align.wrap, egui::Checkbox::new(&mut st.align.shrink, lang.tr("Shrink to fit")));
            ui.checkbox(&mut merge, lang.tr("Merge cells"));
            ui.add_space(6.0);
            ui.label(egui::RichText::new(lang.tr("Right-to-left")).strong());
            let l = ui.label(lang.tr("Text direction:"));
            let orders =
                [(ReadingOrder::Context, "Context"), (ReadingOrder::LeftToRight, "Left to right"), (ReadingOrder::RightToLeft, "Right to left")];
            let cur = st.align.reading_order.unwrap_or_default();
            let shown = orders.iter().find(|(o, _)| *o == cur).map_or("", |(_, n)| lang.tr(n));
            craft_ui::ComboBox::from_id_salt("alignDir")
                .width(200.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (order, name) in orders {
                        if ui.selectable_label(cur == order, lang.tr(name)).clicked() {
                            st.align.reading_order = Some(order);
                        }
                    }
                })
                .response
                .labelled_by(l.id);
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(lang.tr("Orientation")).strong());
            ui.horizontal_top(|ui| {
                vertical_text_box(ui, lang, &mut st.align.rotation);
                orientation_dial(ui, lang, &mut st.align.rotation);
            });
            ui.horizontal(|ui| {
                let vertical = st.align.rotation == 255;
                let mut deg = if vertical { 0 } else { st.align.rotation };
                let r = ui.add_enabled(!vertical, egui::DragValue::new(&mut deg).range(-90..=90));
                let l = ui.label(lang.tr("Degrees"));
                let r = r.labelled_by(l.id);
                if r.changed() {
                    st.align.rotation = deg;
                }
            });
        });
    });
    d.values.insert("merge".into(), json!(merge));
}

/// The Orientation group's Vertical Text box: the word stacked letter by letter; a click turns
/// vertical text (textRotation 255) on or back to horizontal.
fn vertical_text_box(ui: &mut egui::Ui, lang: crate::i18n::Language, rotation: &mut i16) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 130.0), egui::Sense::click());
    let on = *rotation == 255;
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, lang.tr("Vertical Text")));
    let t = theme::Tokens::get(ui.ctx());
    if on {
        ui.painter().rect_filled(rect, 2.0, t.accent);
    }
    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
    let color = if on { t.on_accent } else { t.text };
    let word = lang.tr("Orientation|Text");
    let n = word.chars().count().max(1) as f32;
    for (i, ch) in word.chars().enumerate() {
        let y = rect.center().y + (i as f32 - (n - 1.0) / 2.0) * 16.0;
        ui.painter().text(egui::pos2(rect.center().x, y), egui::Align2::CENTER_CENTER, ch, theme::ui_font(13.0), color);
    }
    if response.clicked() {
        *rotation = if on { 0 } else { 255 };
    }
}

/// The Orientation dial: a half circle from 90° (up) to -90° (down); clicking or dragging on it
/// sets the angle in whole degrees.
fn orientation_dial(ui: &mut egui::Ui, lang: crate::i18n::Language, rotation: &mut i16) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(120.0, 130.0), egui::Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, lang.tr("Orientation")));
    let t = theme::Tokens::get(ui.ctx());
    let c = egui::pos2(rect.left() + 10.0, rect.center().y);
    let radius = 95.0;
    let point = |deg: f32| c + radius * egui::vec2(deg.to_radians().cos(), -deg.to_radians().sin());
    for deg in (-90..=90).step_by(15) {
        let size = if deg % 45 == 0 { 3.0 } else { 1.8 };
        ui.painter().circle_filled(point(deg as f32), size, t.text);
    }
    let vertical = *rotation == 255;
    if !vertical {
        let deg = *rotation as f32;
        let color = egui::Color32::from_rgb(0xC0, 0x20, 0x20);
        ui.painter().line_segment([c + 0.45 * (point(deg) - c), point(deg)], egui::Stroke::new(1.5, color));
        ui.painter().circle_filled(point(deg), 4.0, color);
        let galley = ui.painter().layout_no_wrap(lang.tr("Orientation|Text").to_string(), theme::ui_font(12.0), t.text);
        let h = galley.size().y;
        let angle = -deg.to_radians();
        let offset = egui::vec2(angle.sin(), -angle.cos()) * (h / 2.0);
        let mut shape = egui::epaint::TextShape::new(c + offset * -1.0, galley, t.text);
        shape.angle = angle;
        ui.painter().add(shape);
    }
    if (response.clicked() || response.dragged())
        && let Some(p) = response.interact_pointer_pos()
    {
        let v = p - c;
        *rotation = (-v.y).atan2(v.x.max(0.0)).to_degrees().round().clamp(-90.0, 90.0) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_sends_only_what_changed() {
        let old = json!({"num_fmt": "General", "font": {"bold": false, "color": {"Rgb": 255}, "size": 11.0}, "align": {"reading_order": null}});
        let new =
            json!({"num_fmt": "0.00", "font": {"bold": false, "color": {"Theme": [4, 0]}, "size": 11.0}, "align": {"reading_order": "RightToLeft"}});
        assert_eq!(
            style_patch(&old, &new),
            Some(json!({"num_fmt": "0.00", "font": {"color": {"Theme": [4, 0]}}, "align": {"reading_order": "RightToLeft"}}))
        );
        assert_eq!(style_patch(&old, &old), None);
    }
}
