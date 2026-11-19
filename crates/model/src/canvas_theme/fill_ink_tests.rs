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
