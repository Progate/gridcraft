//! Page Setup: Excel's Page, Margins and Sheet tabs over one `pageLayout.pageSetup`, so OK is one
//! undo step. The dialog opens on the active sheet's print settings: OK without edits keeps them.

use gridcraft_engine::core::{RangeRef, col_to_letters};
use gridcraft_engine::model::{Orientation, PrintSettings};

use super::data::abs;
use super::show::{form_fields, ok_cancel};
use super::{Dialog, Field, Json, SheetApp, json};

/// The paper sizes offered by Page Layout › Size and Page Setup › Page, in that order. B4/B5/B6
/// are the JIS sizes Japanese Excel uses; the ISO ones stay selectable by name.
pub(crate) const PAPER_SIZES: [&str; 20] = [
    "Letter",
    "Legal",
    "Tabloid",
    "Executive",
    "A3",
    "A4",
    "A5",
    "A6",
    "B4",
    "B5",
    "B6",
    "Japanese Postcard",
    "Japanese Double Postcard",
    "Japanese Envelope #3",
    "Japanese Envelope #4",
    "Japanese Envelope Kaku #2",
    "Japanese Envelope Kaku #3",
    "B4 (ISO)",
    "B5 (ISO)",
    "B6 (ISO)",
];

/// The dialog's tabs: the English name `open_dialog`'s `tab` and `ui.dialog.set` use, and the
/// label drawn (keyed by context where the bare word means something else elsewhere).
const TABS: [(&str, &str); 3] = [("Page", "Page Setup|Page"), ("Margins", "Margins"), ("Sheet", "Page Setup|Sheet")];

/// The margin keys, in `PrintSettings::margins` order.
const MARGINS: [&str; 6] = ["left", "right", "top", "bottom", "header", "footer"];

/// Opens Page Setup on `tab` (Page when it names none of Page, Margins, Sheet).
pub(super) fn open(app: &SheetApp, tab: Option<&str>) -> Dialog {
    let print = app.session.active().and_then(|d| d.wb.active()).map(|s| s.print.clone()).unwrap_or_default();
    // Every tab's fields, so the reference boxes are known when the dialog opens; the body shows
    // one tab's at a time.
    let fields = TABS.iter().flat_map(|(t, _)| fields(t, "fit")).collect();
    let mut d = Dialog::form("pageSetup", "Page Setup", "pageLayout.pageSetup", fields, values(&print));
    d.tab = TABS.iter().map(|(t, _)| *t).find(|t| Some(*t) == tab).unwrap_or("Page").to_string();
    d
}

/// The form values for `print`: what OK sends back unchanged when nothing is edited.
fn values(print: &PrintSettings) -> Json {
    let mut v = json!({
        "orientation": if print.orientation == Orientation::Landscape { "landscape" } else { "portrait" },
        "paper": print.paper,
        "scaling": if print.fit_to.is_some() { "fit" } else { "adjust" },
        "scale": print.scale,
        // Excel offers 1 by 1 when Fit to is first chosen.
        "fitWidth": print.fit_to.map_or(1, |f| f.0),
        "fitHeight": print.fit_to.map_or(1, |f| f.1),
        "centerH": print.center_h,
        "centerV": print.center_v,
        "printArea": print.print_area.as_ref().map(abs).unwrap_or_default(),
        "titleRows": print.title_rows.map(|(a, b)| format!("${}:${}", a.min(b) + 1, a.max(b) + 1)).unwrap_or_default(),
        "titleCols": print.title_cols.map(|(a, b)| format!("${}:${}", col_to_letters(a.min(b)), col_to_letters(a.max(b)))).unwrap_or_default(),
        "gridlines": print.gridlines,
        "headings": print.headings,
        "pageOrder": print.page_order.id(),
    });
    for (key, inches) in MARGINS.iter().zip(print.margins) {
        // Round off f32 noise so the form shows 0.1, not 0.10000000149.
        v[*key] = json!((f64::from(inches) * 1e4).round() / 1e4);
    }
    v
}

/// The fields of `tab`. On the Page tab, `scaling` picks the percentage or the page counts.
fn fields(tab: &str, scaling: &str) -> Vec<Field> {
    use Field::*;
    match tab {
        "Page" => {
            let mut f = vec![
                Choice { key: "orientation", label: "Page Layout|Orientation", options: vec![("portrait", "Portrait"), ("landscape", "Landscape")] },
                Choice { key: "paper", label: "Paper size:", options: PAPER_SIZES.iter().map(|p| (*p, *p)).collect() },
                Choice {
                    key: "scaling",
                    label: "Scaling",
                    options: vec![("adjust", "Adjust to a percentage of normal size"), ("fit", "Fit to a number of pages")],
                },
            ];
            if scaling == "fit" {
                f.push(Number { key: "fitWidth", label: "Pages wide (0 = automatic):" });
                f.push(Number { key: "fitHeight", label: "Pages tall (0 = automatic):" });
            } else {
                f.push(Number { key: "scale", label: "% of normal size:" });
            }
            f
        }
        "Margins" => vec![
            Number { key: "left", label: "Left margin (in):" },
            Number { key: "right", label: "Right margin (in):" },
            Number { key: "top", label: "Top margin (in):" },
            Number { key: "bottom", label: "Bottom margin (in):" },
            Number { key: "header", label: "Header (in):" },
            Number { key: "footer", label: "Footer (in):" },
            Check { key: "centerH", label: "Center on page horizontally" },
            Check { key: "centerV", label: "Center on page vertically" },
        ],
        "Sheet" => vec![
            Ref { key: "printArea", label: "Print area:" },
            Ref { key: "titleRows", label: "Rows to repeat at top:" },
            Ref { key: "titleCols", label: "Columns to repeat at left:" },
            Check { key: "gridlines", label: "Gridlines" },
            Check { key: "headings", label: "Row and column headings" },
            Choice { key: "pageOrder", label: "Page order", options: vec![("downThenOver", "Down, then over"), ("overThenDown", "Over, then down")] },
        ],
        _ => vec![],
    }
}

/// Draws the tab strip and the shown tab's fields.
pub(super) fn page_setup(app: &SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let lang = crate::i18n::current(ui.ctx());
    ui.horizontal(|ui| {
        for (tab, label) in TABS {
            if ui.selectable_label(d.tab == tab, lang.tr(label)).clicked() {
                d.tab = tab.to_string();
            }
        }
    });
    ui.separator();
    let scaling = d.values.get("scaling").and_then(Json::as_str).unwrap_or("adjust").to_string();
    d.fields = fields(&d.tab, &scaling);
    form_fields(app, ui, d);
    ok_cancel(ui, confirm, open);
}

/// Reshapes the form values into `pageLayout.pageSetup` params.
pub(super) fn params(d: &Dialog, v: &mut Json) {
    // A margin dragged below zero is zero, as the Margins command makes it (Page Setup refuses
    // negative margins from other callers).
    for key in MARGINS {
        if let Some(inches) = d.values.get(key).and_then(Json::as_f64) {
            v[key] = json!(inches.max(0.0));
        }
    }
    let Some(m) = v.as_object_mut() else { return };
    // `scale` turns fitting off and the page counts turn it on, so only the chosen mode's values
    // go: sending both would leave the sheet fitted after "Adjust to" was chosen.
    let fit = m.remove("scaling").and_then(|s| s.as_str().map(str::to_string)).as_deref() == Some("fit");
    if fit {
        m.remove("scale");
        // Number boxes hold floats; the command reads whole pages, and 0 is "automatic".
        for key in ["fitWidth", "fitHeight"] {
            let pages = m.get(key).and_then(Json::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0).round().clamp(0.0, 32767.0) as u64;
            m.insert(key.into(), json!(pages));
        }
    } else {
        m.remove("fitWidth");
        m.remove("fitHeight");
    }
    // An empty paper size is no choice; an unknown one stays for the command to refuse.
    if m.get("paper").and_then(Json::as_str).is_some_and(|p| p.trim().is_empty()) {
        m.remove("paper");
    }
}

/// A selection picked into Rows to repeat at top or Columns to repeat at left, as Excel writes it
/// there: whole rows (`$1:$2`) or whole columns (`$A:$B`), keeping a sheet name in front.
pub(super) fn title_ref(key: &str, reference: String) -> String {
    let (sheet, area) = match reference.rsplit_once('!') {
        Some((sheet, area)) => (Some(sheet), area),
        None => (None, reference.as_str()),
    };
    let Some(r) = RangeRef::parse(area) else { return reference };
    let shaped = match key {
        "titleRows" => format!("${}:${}", r.start.row + 1, r.end.row + 1),
        "titleCols" => format!("${}:${}", col_to_letters(r.start.col), col_to_letters(r.end.col)),
        _ => return reference,
    };
    match sheet {
        Some(sheet) => format!("{sheet}!{shaped}"),
        None => shaped,
    }
}

#[cfg(test)]
mod tests {
    use super::super::show::build_params;
    use super::*;

    fn app() -> SheetApp {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        SheetApp::new(session, Default::default())
    }

    fn print(app: &SheetApp) -> PrintSettings {
        app.session.active().and_then(|d| d.wb.active()).map(|s| s.print.clone()).unwrap_or_default()
    }

    fn confirm(app: &mut SheetApp, d: &Dialog) {
        app.run(d.command.unwrap_or_default(), build_params(d)).unwrap();
    }

    #[test]
    fn opens_on_the_named_tab_and_on_page_otherwise() {
        let app = app();
        assert_eq!(open(&app, Some("Sheet")).tab, "Sheet");
        assert_eq!(open(&app, Some("Margins")).tab, "Margins");
        assert_eq!(open(&app, None).tab, "Page");
        assert_eq!(open(&app, Some("Nope")).tab, "Page");
        assert!(open(&app, None).fields.iter().any(|f| matches!(f, Field::Ref { key: "titleRows", .. })));
    }

    #[test]
    fn ok_without_edits_keeps_every_setting() {
        let mut app = app();
        let setups = [
            json!({"orientation": "landscape", "paper": "A4", "fitWidth": 1, "fitHeight": 0, "titleRows": "1:2", "titleCols": "A:A",
                "printArea": "A1:F40", "gridlines": true, "headings": true, "centerH": true, "left": 0.25, "pageOrder": "overThenDown"}),
            json!({"scale": 85, "titleRows": "", "titleCols": "", "printArea": ""}),
        ];
        for setup in setups {
            app.run("pageLayout.pageSetup", setup).unwrap();
            let before = print(&app);
            let d = open(&app, None);
            confirm(&mut app, &d);
            assert_eq!(print(&app), before);
        }
    }

    #[test]
    fn fit_to_one_page_wide_by_automatic_tall() {
        let mut app = app();
        let mut d = open(&app, Some("Page"));
        assert_eq!(d.values["scaling"], "adjust");
        d.set_value("scaling", json!("fit"));
        d.set_value("fitWidth", json!(1.0));
        d.set_value("fitHeight", json!(0.0));
        confirm(&mut app, &d);
        assert_eq!(print(&app).fit_to, Some((1, 0)));
        // Back to a percentage turns fitting off.
        let mut d = open(&app, Some("Page"));
        assert_eq!(d.values["scaling"], "fit");
        d.set_value("scaling", json!("adjust"));
        d.set_value("scale", json!(75.0));
        confirm(&mut app, &d);
        assert_eq!((print(&app).fit_to, print(&app).scale), (None, 75));
    }

    #[test]
    fn print_titles_and_area_set_and_clear_in_one_undo_step() {
        let mut app = app();
        let mut d = open(&app, Some("Sheet"));
        d.set_value("titleRows", json!("$1:$1"));
        d.set_value("titleCols", json!("$A:$B"));
        d.set_value("printArea", json!("$A$1:$D$20"));
        d.set_value("orientation", json!("landscape"));
        confirm(&mut app, &d);
        let p = print(&app);
        assert_eq!((p.title_rows, p.title_cols, p.print_area.map(|r| r.a1())), (Some((0, 0)), Some((0, 1)), Some("A1:D20".to_string())));
        assert_eq!(p.orientation, Orientation::Landscape);
        let d = open(&app, Some("Sheet"));
        assert_eq!(
            (&d.values["titleRows"], &d.values["titleCols"], &d.values["printArea"]),
            (&json!("$1:$1"), &json!("$A:$B"), &json!("$A$1:$D$20"))
        );
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(print(&app), PrintSettings::default());
    }

    #[test]
    fn picked_titles_become_whole_rows_or_columns() {
        assert_eq!(title_ref("titleRows", "$A$1:$C$2".into()), "$1:$2");
        assert_eq!(title_ref("titleCols", "$B$3".into()), "$B:$B");
        assert_eq!(title_ref("titleRows", "'My Sheet'!$A$4:$XFD$4".into()), "'My Sheet'!$4:$4");
        assert_eq!(title_ref("printArea", "$A$1:$C$2".into()), "$A$1:$C$2");
        assert_eq!(title_ref("titleRows", "nonsense".into()), "nonsense");
    }
}
