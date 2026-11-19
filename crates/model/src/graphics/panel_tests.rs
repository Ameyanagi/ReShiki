use super::*;
use crate::graphics::{BracketSides, GraphicStyle};

fn panel(titles: &[&str], place: TitlePlace) -> Graphic {
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        Point::new(200., 100.),
        GraphicStyle {
            stroke: Color::Palette(crate::palette::Hue::Blue, crate::palette::Row::Strong),
            fill: Some(Color::Palette(
                crate::palette::Hue::Blue,
                crate::palette::Row::Tint,
            )),
            ..Default::default()
        },
        BracketSides::Both,
        false,
    );
    g.panel = Some(Panel {
        titles: titles.iter().map(|t| t.to_string()).collect(),
        place,
        ..Default::default()
    });
    g
}

fn molecule(doc: &mut Document, x: f32) -> Vec<u64> {
    let a = doc.add_atom("C", Point::new(x, 50.));
    let b = doc.add_atom("C", Point::new(x + 20., 50.));
    doc.add_bond(a, b, 1, "plain");
    vec![a, b]
}

#[test]
fn a_plain_rectangle_is_stored_without_panel_settings() {
    let mut g = panel(&[], TitlePlace::Bar);
    g.panel = None;
    let json = serde_json::to_value(&g).unwrap();
    assert!(json.get("panel").is_none());
    let mut g = panel(&["This work"], TitlePlace::Border);
    g.panel.as_mut().unwrap().dividers.push(Divider {
        vertical: true,
        at: 0.6,
    });
    let json = serde_json::to_value(&g).unwrap();
    assert_eq!(json["panel"]["titles"][0], "This work");
    assert_eq!(json["panel"]["place"], "border");
    assert_eq!(serde_json::from_value::<Graphic>(json).unwrap(), g);
}

#[test]
fn corners_are_the_shapes_own_rounded_or_square() {
    let mut g = panel(&[], TitlePlace::Bar);
    let curves = |g: &Graphic| {
        g.commands()
            .iter()
            .filter(|c| matches!(c, PathCommand::Cubic(..)))
            .count()
    };
    assert_eq!(curves(&g), 0);
    g.kind = GraphicKind::RoundedRectangle;
    g.style.corner_pt = Some(Panel::ROUNDED_PT);
    g.validate().unwrap();
    assert_eq!(curves(&g), 4);
    let (lo, hi) = g.shape_bounds();
    assert!((hi.x - lo.x - 200.).abs() < 2.5 && (hi.y - lo.y - 100.).abs() < 2.5);
}

#[test]
fn a_title_bar_draws_a_band_in_the_outline_color_with_page_colored_text() {
    let text = TextStyle::default();
    let g = panel(&[], TitlePlace::Bar);
    assert!(g.panel_parts(&text).is_empty());
    assert_eq!(g.panel_bar_height(&text), 0.);
    let g = panel(&["Graphical abstract"], TitlePlace::Bar);
    let parts = g.panel_parts(&text);
    assert_eq!(parts.len(), 1);
    assert!(parts[0].filled);
    assert_eq!(parts[0].style.fill, Some(g.style.stroke));
    let band = g.panel_bar_height(&text);
    assert!(band > text.size());
    let titles = g.panel_titles(&text);
    let [title] = titles.as_slice() else {
        panic!("one title")
    };
    assert!(title.style.bold);
    assert_eq!(title.style.color, Color::Custom([255; 3]));
    assert!(title.position.y > 0. && title.position.y + title.size < band);
    // The title stays inside the panel: its bounds do not grow.
    let (lo, _) = g.bounds_with_text_style(&text);
    assert!(lo.y >= -1.);
}

/// The scene of a drawing holding only `g`, on the given canvas.
fn drawn(g: &Graphic, theme: crate::canvas_theme::CanvasTheme) -> Vec<crate::scene::Primitive> {
    let doc = Document {
        canvas_theme: theme,
        graphics: vec![g.clone()],
        ..Default::default()
    };
    crate::scene::primitives(&doc)
}

/// Points of the stroked (unfilled) paths in `parts`, flattened.
fn stroked(parts: &[crate::scene::Primitive]) -> Vec<Point> {
    use crate::scene::Primitive;
    let mut points = vec![];
    for part in parts {
        let part = match part {
            Primitive::Opacity { primitive, .. } => primitive.as_ref(),
            part => part,
        };
        if let Primitive::Path {
            commands,
            filled: false,
            ..
        } = part
        {
            points.extend(crate::graphics::flattened(commands).into_iter().flatten());
        }
    }
    points
}

#[test]
fn a_border_title_sits_on_the_top_edge_in_a_break_of_the_outline() {
    let text = TextStyle::default();
    let g = panel(&["Ours"], TitlePlace::Border);
    // Nothing is painted over the edge: no box covers what is behind.
    assert!(g.panel_parts(&text).is_empty());
    let [title] = g.panel_titles(&text).try_into().ok().unwrap();
    assert_eq!(title.style.color, g.style.stroke);
    assert!(title.position.y < 0. && title.position.y + title.size > 0.);
    let [(lo, hi)] = g.outline_breaks(&text).try_into().ok().unwrap();
    assert!(lo.x < title.position.x && lo.y <= title.position.y);
    assert!(hi.y >= title.position.y + title.size);
    // The outline runs on either side of the title but not under it.
    let points = stroked(&drawn(&g, Default::default()));
    let under = |p: &Point| p.x > lo.x + 0.01 && p.x < hi.x - 0.01 && p.y.abs() < 1.;
    assert!(!points.iter().any(under), "{points:?}");
    assert!(points.iter().any(|p| p.y.abs() < 0.01 && p.x < lo.x));
    assert!(points.iter().any(|p| p.y.abs() < 0.01 && p.x > hi.x));
    // A panel without its outline has nothing to break.
    let mut open = g.clone();
    open.panel.as_mut().unwrap().border = false;
    assert!(open.outline_breaks(&text).is_empty());
    // The bounds reach the title's ink above the edge.
    let (lo, _) = g.bounds_with_text_style(&text);
    let ink = title
        .ink_boxes()
        .into_iter()
        .map(|(a, _)| a.y)
        .fold(f32::MAX, f32::min);
    assert!(ink < 0. && lo.y <= ink);
    // A click on the title's letters picks the panel.
    let (a, b) = title.ink_boxes()[0];
    let middle = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
    assert!(g.hit_with_text_style(middle, 0.5, &text));
}

#[test]
fn wavy_and_rotated_outlines_stay_out_of_a_border_title() {
    let text = TextStyle::default();
    let mut wavy = panel(&["This work"], TitlePlace::Border);
    wavy.style.pattern = crate::graphics::LinePattern::Wavy;
    let mut turned = panel(&["This work"], TitlePlace::Border);
    // Nearly a quarter turn: the edge runs down through the upright title.
    let (s, c) = 88f32.to_radians().sin_cos();
    turned.map_positions(|p| Point::new(p.x * c - p.y * s, p.x * s + p.y * c));
    for g in [wavy, turned] {
        let [(lo, hi)] = g.outline_breaks(&text).try_into().ok().unwrap();
        let points = stroked(&drawn(&g, Default::default()));
        assert!(points.len() > 4);
        let inside = |p: &Point| {
            p.x > lo.x + 0.01 && p.x < hi.x - 0.01 && p.y > lo.y + 0.01 && p.y < hi.y - 0.01
        };
        assert!(!points.iter().any(inside), "{:?}", g.style.pattern);
    }
}

#[test]
fn vertical_dividers_split_the_panel_into_titled_columns() {
    let text = TextStyle::default();
    let mut g = panel(&["Scheme", "Evidence"], TitlePlace::Border);
    let p = g.panel.as_mut().unwrap();
    p.dividers.push(Divider {
        vertical: true,
        at: 0.6,
    });
    assert_eq!(p.columns(), vec![(0., 0.6), (0.6, 1.)]);
    g.validate().unwrap();
    let parts = g.panel_parts(&text);
    let line = parts.iter().find(|p| !p.filled).unwrap();
    let points: Vec<_> = line.commands.iter().flat_map(|c| c.points()).collect();
    assert!(points.iter().all(|p| (p.x - 120.).abs() < 0.01));
    assert!(points.iter().all(|p| p.y > 0. && p.y < 100.));
    let titles = g.panel_titles(&text);
    assert_eq!(titles.len(), 2);
    assert!(titles[1].position.x > 120.);
    // A title per column, not more.
    g.panel.as_mut().unwrap().titles.push("Extra".into());
    assert!(g.validate().is_err());
}

#[test]
fn only_rectangles_carry_panel_settings_and_dividers_stay_inside() {
    let mut g = panel(&[], TitlePlace::Bar);
    g.validate().unwrap();
    g.panel.as_mut().unwrap().dividers.push(Divider {
        vertical: false,
        at: 1.2,
    });
    assert!(g.validate().is_err());
    let mut g = panel(&[], TitlePlace::Bar);
    g.kind = GraphicKind::Ellipse;
    assert!(g.validate().is_err());
}

#[test]
fn contents_are_the_objects_wholly_inside_the_panel() {
    let mut doc = Document::default();
    let inside = molecule(&mut doc, 40.);
    let outside = molecule(&mut doc, 190.);
    let mut g = panel(&[], TitlePlace::Bar);
    g.id = doc.next_id();
    let id = g.id;
    doc.graphics.push(g);
    let found = contents(&doc, id);
    assert!(inside.iter().all(|a| found.contains(a)));
    assert!(!outside.iter().any(|a| found.contains(a)));
    assert!(!found.contains(&id));
    assert!(contents(&doc, inside[0]).is_empty());
}

#[test]
fn a_rotated_panel_holds_only_what_its_frame_shows() {
    let mut doc = Document::default();
    // A molecule in the middle and a line in a corner of the turned panel's
    // upright bounding box, outside the frame itself.
    let inside = molecule(&mut doc, 90.);
    let mut g = panel(&[], TitlePlace::Bar);
    let (s, c) = 45f32.to_radians().sin_cos();
    let middle = Point::new(100., 50.);
    g.map_positions(|p| {
        let (x, y) = (p.x - middle.x, p.y - middle.y);
        Point::new(middle.x + x * c - y * s, middle.y + x * s + y * c)
    });
    let (lo, _) = g.shape_bounds();
    let corner = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        corner,
        GraphicKind::Line,
        lo.offset(2., 2.),
        lo.offset(8., 4.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    g.id = doc.next_id();
    let id = g.id;
    doc.graphics.push(g);
    let found = contents(&doc, id);
    assert!(inside.iter().all(|a| found.contains(a)), "{found:?}");
    assert!(!found.contains(&corner));
}

#[test]
fn a_new_divider_goes_into_the_widest_gap_between_contents() {
    let mut doc = Document::default();
    molecule(&mut doc, 10.);
    molecule(&mut doc, 50.);
    molecule(&mut doc, 150.);
    let mut g = panel(&[], TitlePlace::Bar);
    g.id = doc.next_id();
    let id = g.id;
    doc.graphics.push(g);
    // Wrapped panels are grouped with what they hold.
    let all = doc.all_ids();
    doc.group_selection(&all).unwrap();
    let at = divider_position(&doc, id, true);
    // The gap runs from x = 70 to 150; its middle is 110 of 200.
    assert!((at - 0.55).abs() < 0.03, "{at}");
    // Without a gap along y the divider halves the panel.
    assert!((divider_position(&doc, id, false) - 0.5).abs() < 0.01);
}

#[test]
fn a_flipped_panel_keeps_upright_titles_and_mirrors_its_dividers() {
    let mut g = panel(&["Scheme", "Evidence"], TitlePlace::Border);
    g.panel.as_mut().unwrap().dividers.push(Divider {
        vertical: true,
        at: 0.6,
    });
    g.map_positions(|p| Point::new(-p.x, p.y));
    assert!(g.axis_x.x > 0.);
    assert!((g.origin.x + 200.).abs() < 0.01);
    let p = g.panel.as_ref().unwrap();
    assert!((p.dividers[0].at - 0.4).abs() < 0.0001);
    assert_eq!(p.titles, vec!["Evidence".to_string(), "Scheme".to_string()]);
    g.validate().unwrap();
}

#[test]
fn a_band_title_reads_on_the_band_as_the_dark_canvas_shows_it() {
    let mut doc = Document {
        canvas_theme: crate::canvas_theme::CanvasTheme::Dark,
        ..Default::default()
    };
    let mut g = panel(&["This work"], TitlePlace::Bar);
    g.style.stroke = Color::Custom([0, 0, 255]);
    doc.graphics.push(g);
    let title = crate::scene::primitives(&doc)
        .into_iter()
        .find_map(|p| match p {
            crate::scene::Primitive::Text { text, color, .. } if text == "This work" => Some(color),
            _ => None,
        })
        .unwrap();
    // Pure blue stays dark blue on the dark canvas, so the title shows white.
    assert_eq!(doc.canvas_theme.color(title), [255; 3]);
}

#[test]
fn a_click_on_a_divider_picks_an_unfilled_panel() {
    let text = TextStyle::default();
    let mut g = panel(&[], TitlePlace::Bar);
    g.style.fill = None;
    g.panel.as_mut().unwrap().dividers.push(Divider {
        vertical: true,
        at: 0.5,
    });
    assert!(g.hit_with_text_style(Point::new(100., 50.), 1., &text));
    assert!(!g.hit_with_text_style(Point::new(60., 50.), 1., &text));
}

#[test]
fn a_border_title_reads_on_the_fill_and_the_page_on_both_canvases() {
    use crate::{canvas_theme::CanvasTheme, color_contrast::contrast, scene::Primitive};
    let tinted = panel(&["Previous work"], TitlePlace::Border);
    let mut faint = tinted.clone();
    faint.style.fill = Some(Color::Ink);
    faint.style.effects.opacity = Some(0.08);
    let mut open = tinted.clone();
    open.style.fill = None;
    // Ink on Ink: neither black nor white reads on both the panel and the page.
    let mut ink = tinted.clone();
    ink.style.stroke = Color::Ink;
    ink.style.fill = Some(Color::Ink);
    for theme in CanvasTheme::ALL {
        for g in [&tinted, &faint, &open, &ink] {
            let parts = drawn(g, theme);
            let title = parts
                .iter()
                .find_map(|p| match p {
                    Primitive::Text { color, .. } => Some(theme.color(*color)),
                    _ => None,
                })
                .unwrap();
            let paper = theme.background();
            let fill = g.backing(|c| theme.color(c.rgb()), paper).unwrap_or(paper);
            for surface in [fill, paper] {
                assert!(
                    contrast(title, surface) >= 3.,
                    "{theme}: {title:?} on {surface:?}"
                );
            }
        }
    }
}
