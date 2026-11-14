//! Titles a click selects (the chart title and the axis titles) and the Centered Overlay title.

use super::*;

fn plot_top(prims: &[Prim]) -> f32 {
    // The highest gridline: the plot's top edge.
    prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { pts, .. } if pts.len() == 2 && pts[0][1] == pts[1][1] && pts[0][0] != pts[1][0] => Some(pts[0][1]),
            _ => None,
        })
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn the_title_and_axis_titles_can_be_found_where_they_are_drawn() {
    let c = chart(ChartKind::ColumnClustered);
    let d = sample(c.kind);
    let (w, h) = (480.0, 288.0);
    let title = element_rect(&c, &d, w, h, &ApproxMeasure, ChartElement::Title).unwrap();
    assert!(title[0] > 100.0 && title[0] + title[2] < 380.0 && title[1] < 30.0, "centred at the top: {title:?}");
    let centre = |r: [f32; 4]| (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    let (x, y) = centre(title);
    assert_eq!(element_at(&c, &d, w, h, &ApproxMeasure, x, y), Some(ChartElement::Title));
    let across = element_rect(&c, &d, w, h, &ApproxMeasure, ChartElement::HorizontalAxisTitle).unwrap();
    assert!(across[1] > h * 0.75, "the horizontal axis title is below the plot: {across:?}");
    let (x, y) = centre(across);
    assert_eq!(element_at(&c, &d, w, h, &ApproxMeasure, x, y), Some(ChartElement::HorizontalAxisTitle));
    let up = element_rect(&c, &d, w, h, &ApproxMeasure, ChartElement::VerticalAxisTitle).unwrap();
    assert!(up[0] < 30.0 && up[3] > up[2], "the vertical axis title runs up the left: {up:?}");
    let (x, y) = centre(up);
    assert_eq!(element_at(&c, &d, w, h, &ApproxMeasure, x, y), Some(ChartElement::VerticalAxisTitle));
    assert_eq!(element_at(&c, &d, w, h, &ApproxMeasure, w / 2.0, h / 2.0), None, "the plot is not a title");
    let mut bare = c.clone();
    bare.title = Some(String::new());
    bare.x_title = None;
    assert_eq!(element_rect(&bare, &d, w, h, &ApproxMeasure, ChartElement::Title), None);
    assert_eq!(element_rect(&bare, &d, w, h, &ApproxMeasure, ChartElement::HorizontalAxisTitle), None);
}

#[test]
fn a_bar_charts_value_axis_title_is_drawn_across() {
    let mut c = chart(ChartKind::BarClustered);
    c.x_title = Some("Region".into());
    c.y_title = Some("Revenue".into());
    let d = sample(c.kind);
    let p = render(&c, &d, 480.0, 288.0, &ApproxMeasure);
    let rotation = |t: &str| {
        p.iter().find_map(|p| match p {
            Prim::Text { text, rotation, .. } if text == t => Some(*rotation),
            _ => None,
        })
    };
    assert_eq!(rotation("Revenue"), Some(0.0), "the value axis runs across a bar chart");
    assert!(rotation("Region").is_some_and(|r| r != 0.0), "the categories run up it");
}

#[test]
fn a_centered_overlay_title_leaves_the_plot_its_room() {
    let mut c = chart(ChartKind::ColumnClustered);
    c.legend = LegendPos::None;
    let d = sample(c.kind);
    let above = plot_top(&render(&c, &d, 480.0, 288.0, &ApproxMeasure));
    c.title_overlay = true;
    let over = render(&c, &d, 480.0, 288.0, &ApproxMeasure);
    assert!(plot_top(&over) < above - 10.0, "the plot grows into the title's room: {} vs {above}", plot_top(&over));
    assert!(texts(&over).contains(&"Quarterly Sales"));
}
