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
