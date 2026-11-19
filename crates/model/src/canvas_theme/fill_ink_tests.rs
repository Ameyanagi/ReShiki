use super::*;
use crate::{
    document::{Annotation, Document, Point},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
    palette::{Hue, Row},
};

/// A filled rectangle from `lo` to `hi`, behind the chemistry.
fn card(doc: &mut Document, lo: Point, hi: Point, fill: Color) -> u64 {
    let id = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        lo,
        hi,
        GraphicStyle {
            fill: Some(fill),
            ..Default::default()
        },
        BracketSides::Both,
        false,
    ));
    id
}

/// An ethane on the card, one outside it, and a caption on the card.
fn figure(canvas: CanvasTheme, fill: Color) -> (Document, [u64; 4]) {
    let mut doc = Document {
        canvas_theme: canvas,
        ..Default::default()
    };
    card(&mut doc, Point::new(-10., -10.), Point::new(60., 30.), fill);
    let a = doc.add_atom("O", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(20., 0.));
    doc.add_bond(a, b, 1, "plain");
    let outside = doc.add_atom("O", Point::new(200., 0.));
    let text = doc.next_id();
    doc.annotations.push(Annotation {
        id: text,
        position: Point::new(30., 10.),
        text: "card".into(),
        format: Default::default(),
    });
    (doc, [a, b, outside, text])
}

fn shown(doc: &Document, color: Color) -> [u8; 3] {
    Palette::of(doc).rgb(color)
}

fn label(doc: &Document, id: u64) -> [u8; 3] {
    let atom = doc.atom(id).unwrap();
    shown(
        doc,
        atom.text_style.as_ref().map_or(Color::Ink, |s| s.color),
    )
}

#[test]
fn a_light_card_keeps_dark_ink_on_the_dark_canvas() {
    let (doc, [a, _, outside, text]) = figure(CanvasTheme::Dark, Color::Custom([0xD9; 3]));
    let resolved = resolved_document(&doc);
    assert_eq!(label(&resolved, a), [0; 3], "a label on the card");
    assert_eq!(shown(&resolved, resolved.bonds[0].color), [0; 3]);
    let caption = resolved.annotations.iter().find(|t| t.id == text).unwrap();
    assert_eq!(shown(&resolved, caption.format.style.color), [0; 3]);
    // Element ink takes the light canvas's tone too: oxygen stays readable.
    let oxygen = label(&resolved, a);
    assert!(crate::color_contrast::contrast(oxygen, [0xD9; 3]) >= 4.5);
    // Off the card the drawing keeps the dark canvas's ink.
    assert_eq!(
        label(&resolved, outside),
        element_color(&doc, "O", CanvasTheme::Dark)
    );
    // The card itself keeps its exact color.
    assert_eq!(
        resolved.graphics[0].style.fill,
        Some(Color::Custom([0xD9; 3]))
    );
}

#[test]
fn a_dark_card_takes_light_ink_on_the_light_canvas() {
    let (doc, [a, ..]) = figure(CanvasTheme::Light, Color::Custom([0x20, 0x20, 0x40]));
    let resolved = resolved_document(&doc);
    assert_eq!(label(&resolved, a), [255; 3]);
    assert_eq!(shown(&resolved, resolved.bonds[0].color), [255; 3]);
}

#[test]
fn palette_tints_and_see_through_ink_follow_the_canvas_unchanged() {
    for canvas in CanvasTheme::ALL {
        let (doc, _) = figure(canvas, Color::Palette(Hue::Blue, Row::Tint));
        assert_eq!(resolved_document(&doc).bonds, doc.bonds, "{canvas}");
        let (mut doc, _) = figure(canvas, Color::Ink);
        doc.graphics[0].style.effects.opacity = Some(0.08);
        assert_eq!(resolved_document(&doc).bonds, doc.bonds, "{canvas}");
    }
}

#[test]
fn a_tinted_panel_on_a_light_card_takes_the_light_tint_and_ink() {
    let (mut doc, [a, ..]) = figure(CanvasTheme::Dark, Color::Custom([0xF0; 3]));
    // A panel drawn over the page card, around the molecule.
    card(
        &mut doc,
        Point::new(-5., -5.),
        Point::new(40., 10.),
        Color::Palette(Hue::Blue, Row::Tint),
    );
    let resolved = resolved_document(&doc);
    let light = Palette::in_mode(&doc, CanvasTheme::Light).swatch(Hue::Blue, Row::Tint);
    assert_eq!(resolved.graphics[1].style.fill, Some(Color::Custom(light)));
    assert_eq!(shown(&resolved, resolved.bonds[0].color), [0; 3]);
    assert!(crate::color_contrast::contrast(label(&resolved, a), light) >= 4.5);
}

#[test]
fn shapes_in_front_of_the_chemistry_do_not_recolor_it() {
    let (mut doc, [a, ..]) = figure(CanvasTheme::Dark, Color::Custom([0xD9; 3]));
    doc.graphics[0].layer = 0;
    let resolved = resolved_document(&doc);
    assert_eq!(
        label(&resolved, a),
        element_color(&doc, "O", CanvasTheme::Dark)
    );
}

#[test]
fn the_canonical_scene_shows_dark_ink_on_a_light_card() {
    let (doc, _) = figure(CanvasTheme::Dark, Color::Custom([0xD9; 3]));
    // Scene colors are canonical light-canvas bytes that the dark canvas
    // flips once: black on the card arrives as white.
    let canonical = canonical_document(&doc);
    assert_eq!(canonical.bonds[0].color, Color::Custom([255; 3]));
}

#[test]
fn paper_is_the_canvas_color_and_saves_by_name() {
    for canvas in CanvasTheme::ALL {
        let doc = Document {
            canvas_theme: canvas,
            ..Default::default()
        };
        assert_eq!(Palette::of(&doc).rgb(Color::Paper), canvas.background());
    }
    assert_eq!(serde_json::to_string(&Color::Paper).unwrap(), "\"paper\"");
    assert_eq!(
        serde_json::from_str::<Color>("\"paper\"").unwrap(),
        Color::Paper
    );
}

#[test]
fn a_stored_paper_color_saves_as_the_drawing_release() {
    let (mut doc, _) = figure(CanvasTheme::Light, Color::Custom([0xD9; 3]));
    let before = doc.file_version();
    doc.bonds[0].color = Color::Paper;
    assert!(before < crate::document::DRAWING_ELEMENTS);
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
}

#[test]
fn a_border_title_reads_on_an_ink_panel() {
    use crate::graphics::panel::border_title_rgb;
    // An Ink outline on an Ink fill: the title takes the other ink.
    assert_eq!(border_title_rgb([0; 3], [0; 3]), [255; 3]);
    assert_eq!(border_title_rgb([255; 3], [255; 3]), [0; 3]);
    // A colored outline that reads on its tint keeps its color.
    assert_eq!(
        border_title_rgb([32, 80, 145], [221, 232, 248]),
        [32, 80, 145]
    );
}

#[test]
fn a_gradient_arrow_on_a_card_of_the_other_canvas_keeps_its_ramp() {
    use crate::{
        arrows::{ArrowStyle, Body, Preset},
        document::Arrow,
        scene::Primitive,
    };
    for (canvas, fill) in [
        (CanvasTheme::Dark, Color::Custom([0xD9; 3])),
        (CanvasTheme::Light, Color::Custom([0x20, 0x20, 0x40])),
    ] {
        let (mut doc, _) = figure(canvas, fill);
        let mut style = ArrowStyle::preset(Preset::Forward);
        style.set_body(Body::Gradient);
        let id = doc.next_id();
        doc.arrows.push(Arrow::new(
            id,
            Point::new(0., 20.),
            Point::new(40., 20.),
            Preset::Forward,
            style,
        ));
        let scene = crate::scene::primitives(&doc);
        let Some(Primitive::Gradient { stops, .. }) = scene
            .iter()
            .find(|p| matches!(p, Primitive::Gradient { .. }))
        else {
            panic!("a gradient body: {scene:?}");
        };
        let [(_, start, _), (_, end, _)] = stops.as_slice() else {
            panic!("two stops: {stops:?}");
        };
        // The card's ink, from the card's paper.
        let ink = ink_canvas(match fill {
            Color::Custom(rgb) => rgb,
            _ => unreachable!(),
        });
        assert_eq!(*end, ink.color([0; 3]), "{canvas}");
        assert_eq!(*start, ink.background(), "{canvas}");
    }
}

/// A hexagon of carbons around `center`, with a ring fill in `color` drawn as a
/// gradient.
fn gradient_ring(doc: &mut Document, center: Point, color: Color) {
    let ids: Vec<u64> = (0..6)
        .map(|i| {
            let angle = (60. * i as f32).to_radians();
            doc.add_atom("C", center.offset(8. * angle.cos(), 8. * angle.sin()))
        })
        .collect();
    for (a, b) in ids.iter().zip(ids.iter().cycle().skip(1)) {
        doc.add_bond(*a, *b, 1, "plain");
    }
    crate::ring_fills::apply(doc, &ids, Some(color));
    for fill in &mut doc.ring_fills {
        fill.effect = crate::ring_fills::RingEffect::Gradient;
    }
}

fn ring_stops(doc: &Document) -> ([u8; 3], [u8; 3]) {
    let scene = crate::scene::primitives(doc);
    let Some(crate::scene::Primitive::Gradient { stops, .. }) = scene
        .iter()
        .find(|p| matches!(p, crate::scene::Primitive::Gradient { .. }))
    else {
        panic!("a ring gradient: {scene:?}");
    };
    let [(_, start, _), (_, end, _)] = stops.as_slice() else {
        panic!("two stops: {stops:?}");
    };
    (*start, *end)
}

#[test]
fn a_white_ring_gradient_off_any_card_starts_from_the_canvas_paper() {
    let mut doc = Document::default();
    gradient_ring(&mut doc, Point::new(300., 0.), Color::Custom([255; 3]));
    assert_eq!(ring_stops(&doc), ([255; 3], [255; 3]));
}

#[test]
fn a_ring_gradient_on_a_card_of_the_other_canvas_starts_from_its_paper() {
    let (mut doc, _) = figure(CanvasTheme::Dark, Color::Custom([0xD9; 3]));
    gradient_ring(
        &mut doc,
        Point::new(20., 10.),
        Color::Palette(Hue::Teal, Row::Tint),
    );
    let light = Palette::in_mode(&doc, CanvasTheme::Light).swatch(Hue::Teal, Row::Tint);
    assert_eq!(ring_stops(&doc), ([255; 3], light));
}

#[test]
fn chemistry_in_the_hole_of_a_compound_path_keeps_the_canvas_ink() {
    use crate::graphics::PathCommand::{Close, Line, Move};
    let (mut doc, [a, ..]) = figure(CanvasTheme::Dark, Color::Custom([0xD9; 3]));
    // A light frame around the molecule: an outer outline and an inner one
    // running the other way, which leaves the middle empty.
    let g = &mut doc.graphics[0];
    g.kind = GraphicKind::Path;
    g.reset_frame();
    let p = Point::new;
    g.path = vec![
        Move(p(-30., -30.)),
        Line(p(80., -30.)),
        Line(p(80., 50.)),
        Line(p(-30., 50.)),
        Close,
        Move(p(-20., -20.)),
        Line(p(-20., 40.)),
        Line(p(70., 40.)),
        Line(p(70., -20.)),
        Close,
    ];
    doc.validate().unwrap();
    let resolved = resolved_document(&doc);
    assert_eq!(
        label(&resolved, a),
        element_color(&doc, "O", CanvasTheme::Dark),
        "the hole shows the dark canvas, so the ink stays light"
    );
    // On the frame itself the light fill still calls for black ink.
    let on_frame = doc.add_atom("O", p(-25., 0.));
    assert_eq!(label(&resolved_document(&doc), on_frame), [0; 3]);
}
