use super::*;
use crate::{
    chemistry::cdxml,
    graphics::{BracketSides, Divider, Graphic, GraphicKind, GraphicStyle, Panel, TitlePlace},
    palette::{Color, Hue, Row},
};

fn panel_document(place: TitlePlace, border: bool) -> Document {
    let mut document = Document::default();
    let mut panel = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        Point::new(240., 120.),
        GraphicStyle {
            stroke: Color::Palette(Hue::Blue, Row::Strong),
            fill: Some(Color::Palette(Hue::Blue, Row::Tint)),
            ..Default::default()
        },
        BracketSides::Both,
        false,
    );
    panel.panel = Some(Panel {
        titles: vec!["Scheme".into(), "Evidence".into()],
        place,
        border,
        dividers: vec![Divider {
            vertical: true,
            at: 0.6,
        }],
    });
    document.graphics.push(panel);
    document
}

fn export(document: &Document) -> String {
    let request = Request::molecule("export", document.clone());
    write(document, (&request).into()).unwrap()
}

#[test]
fn panels_export_as_one_group_of_curves_and_uninterpreted_titles() {
    for place in [TitlePlace::Bar, TitlePlace::Border] {
        for border in [true, false] {
            let document = panel_document(place, border);
            let xml = export(&document);
            let decoded =
                crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
            for xml in [&xml, &decoded] {
                let tree = roxmltree::Document::parse(xml).unwrap();
                let groups: Vec<_> = tree
                    .descendants()
                    .filter(|node| node.has_tag_name("group"))
                    .collect();
                assert_eq!(groups.len(), 1, "{place:?} {border}");
                let titles: Vec<_> = groups[0]
                    .children()
                    .filter(|node| node.has_tag_name("t"))
                    .map(|t| {
                        assert_eq!(t.attribute("InterpretChemically"), Some("no"));
                        t.descendants()
                            .filter_map(|s| s.has_tag_name("s").then(|| s.text()).flatten())
                            .collect::<String>()
                    })
                    .collect();
                assert_eq!(titles, ["Scheme", "Evidence"]);
                let curves: Vec<_> = groups[0]
                    .children()
                    .filter(|node| node.has_tag_name("curve"))
                    .collect();
                // Fill and outline (or fill alone), the band or two title
                // backings, and the divider.
                let body = if border { 2 } else { 1 };
                let extras = if place == TitlePlace::Bar { 1 } else { 2 };
                assert_eq!(curves.len(), body + extras + 1, "{place:?} {border}");
                let imported = cdxml::import_cdxml(xml).unwrap().document;
                if groups[0].has_attribute(crate::exchange::PANEL) {
                    // ReShiki reads its own CDXML back as the panel.
                    assert!(imported.annotations.is_empty());
                    assert_eq!(imported.graphics.len(), 1);
                    assert_eq!(imported.graphics[0].panel, document.graphics[0].panel);
                    continue;
                }
                // CDX has no place for the record: shapes and text.
                assert_eq!(imported.annotations.len(), 2);
                assert!(imported.groups.iter().any(|g| {
                    imported
                        .annotations
                        .iter()
                        .all(|a| g.members.contains(&a.id))
                }));
            }
        }
    }
}

/// The color a CDXML `color` index names, as 8-bit channels.
fn rgb(tree: &roxmltree::Document<'_>, index: &str) -> [u8; 3] {
    let colors: Vec<_> = tree
        .descendants()
        .filter(|n| n.has_tag_name("color"))
        .collect();
    let color = colors[index.parse::<usize>().unwrap() - 2];
    ["r", "g", "b"].map(|c| (color.attribute(c).unwrap().parse::<f32>().unwrap() * 255.) as u8)
}

#[tokio::test]
async fn a_panel_comes_back_from_cdxml_as_a_panel_and_chemdraw_sees_its_shadow() {
    let mut document = panel_document(TitlePlace::Bar, true);
    document.graphics[0].style.effects.shadow_pt = Some(4.);
    let original = document.graphics[0].clone();
    let bytes = crate::formats::write(document, "cdxml").await.unwrap();
    let xml = String::from_utf8(bytes.clone()).unwrap();
    let tree = roxmltree::Document::parse(&xml).unwrap();
    // ChemDraw has no panel: its group starts with the shadow, a grey shape.
    let group = tree
        .descendants()
        .find(|n| n.has_tag_name("group"))
        .unwrap();
    assert!(group.has_attribute("ReShikiPanel"));
    let shadow = group
        .descendants()
        .find(|n| n.has_tag_name("curve"))
        .unwrap();
    assert_eq!(shadow.attribute("FillType"), Some("Solid"));
    assert_eq!(
        rgb(&tree, shadow.attribute("color").unwrap()),
        crate::graphics::SHADOW_RGB
    );

    let back = crate::formats::read(bytes, "cdxml").await.unwrap();
    assert!(back.annotations.is_empty(), "titles stay in the panel");
    let [panel] = back.graphics.as_slice() else {
        panic!("one panel, not its parts: {:?}", back.graphics);
    };
    assert_eq!(panel.panel, original.panel);
    assert_eq!(
        (panel.kind, panel.style.effects),
        (original.kind, original.style.effects)
    );
    assert_eq!(panel.style.corner_pt, original.style.corner_pt);
    for (a, b) in [
        (panel.axis_x, original.axis_x),
        (panel.axis_y, original.axis_y),
    ] {
        assert!(a.distance(b) < 0.01, "{a:?} {b:?}");
    }

    // An unfilled shadowed panel from the dark canvas keeps the dark inside
    // on the light canvas it opens on.
    let mut dark = panel_document(TitlePlace::Bar, true);
    dark.canvas_theme = crate::canvas_theme::CanvasTheme::Dark;
    dark.graphics[0].style.fill = None;
    dark.graphics[0].style.effects.shadow_pt = Some(4.);
    let bytes = crate::formats::write(dark.clone(), "cdxml").await.unwrap();
    let opened = crate::formats::read(bytes, "cdxml").await.unwrap();
    let inside = opened
        .graphics
        .iter()
        .find(|g| g.panel.is_some())
        .and_then(|g| g.style.fill)
        .map(|c| c.rgb());
    assert_eq!(inside, Some(dark.canvas_theme.background()));

    // CDX has no place for ReShiki's own data: the panel stays shapes.
    let cdx = crate::formats::write(back, "cdx").await.unwrap();
    let shapes = crate::formats::read(cdx, "cdx").await.unwrap();
    assert!(shapes.graphics.iter().all(|g| g.panel.is_none()));
}

#[tokio::test]
async fn an_unfilled_shadowed_shape_keeps_the_dark_canvas_inside() {
    for theme in crate::canvas_theme::CanvasTheme::ALL {
        let mut document = Document {
            canvas_theme: theme,
            ..Default::default()
        };
        document.graphics.push(Graphic::dragged(
            1,
            GraphicKind::Rectangle,
            Point::new(0., 0.),
            Point::new(60., 30.),
            GraphicStyle {
                fill: None,
                effects: crate::graphics::Effects {
                    shadow_pt: Some(4.),
                    ..Default::default()
                },
                ..Default::default()
            },
            BracketSides::Both,
            false,
        ));
        let bytes = crate::formats::write(document, "cdxml").await.unwrap();
        // Unfilled, so ChemDraw draws its own grey shadow.
        let xml = String::from_utf8(bytes.clone()).unwrap();
        assert!(xml.contains(r#"RectangleType="Shadow""#), "{xml}");
        let back = crate::formats::read(bytes, "cdxml").await.unwrap();
        let shape = back
            .graphics
            .iter()
            .find(|g| g.style.effects.shadow_pt.is_some())
            .unwrap();
        // ChemDraw paints a shadowed outline's inside with the page: on the
        // dark-canvas export's black page that opens as a black fill.
        let inside = shape.style.fill.map(|c| c.rgb());
        if theme.is_dark() {
            assert_eq!(inside, Some(theme.background()));
        } else {
            assert_eq!(inside, None);
        }
    }
}
