use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    graphics::GraphicKind,
    grouping::Group,
    scientific::{AtomMark, MarkKind},
    typography::TextStyle,
};

pub fn documents() -> anyhow::Result<Vec<(String, Document)>> {
    let mut docs = Vec::new();
    for kind in [
        MarkKind::Charge,
        MarkKind::CircledCharge,
        MarkKind::Radical,
        MarkKind::RadicalIon,
        MarkKind::LonePair,
        MarkKind::LonePairBar,
    ] {
        for element in ["C", "N", "O"] {
            for charge in [-2, -1, 0, 1, 2] {
                for offset in [
                    Point::new(15., 0.),
                    Point::new(8., -13.),
                    Point::new(80., 30.),
                ] {
                    let mut doc = Document::default();
                    let id = doc.add_atom(element, Point::new(30., 30.));
                    let a = doc.atom_mut(id).context("Missing test atom")?;
                    a.charge = charge;
                    a.radical_electrons =
                        if matches!(kind, MarkKind::Radical | MarkKind::RadicalIon) {
                            1
                        } else {
                            0
                        };
                    a.marks.push(AtomMark {
                        kind,
                        offset,
                        angle: 37.,
                        size_pt: Some(4.5),
                    });
                    docs.push((format!("mark/{kind:?}/{element}/{charge}/{offset:?}"), doc));
                }
            }
        }
    }
    for pattern in [
        "plain",
        "bold",
        "dashed",
        "dotted",
        "hashed",
        "wedge",
        "hash",
        "hollow_wedge",
        "wavy",
    ] {
        for order in 0..=7 {
            for position in [
                reshiki::bonds::DoublePosition::Auto,
                reshiki::bonds::DoublePosition::Center,
                reshiki::bonds::DoublePosition::Left,
                reshiki::bonds::DoublePosition::Right,
            ] {
                let mut doc = Document::default();
                let a = doc.add_atom("C", Point::new(-42., -42.));
                let b = doc.add_atom("C", Point::new(42., 42.));
                doc.add_bond(a, b, order, pattern);
                let bond = doc.bonds.last_mut().context("Missing test bond")?;
                bond.double_position = position;
                bond.color = [219, 113, 51];
                let c = doc.add_atom("C", Point::new(-42., 42.));
                let d = doc.add_atom("C", Point::new(42., -42.));
                doc.add_bond(c, d, 1, "plain");
                doc.bonds.last_mut().context("Missing test bond")?.z_order = -3;
                docs.push((format!("crossing/{pattern}/{order}/{position:?}"), doc));
            }
        }
    }
    let mut group = Document::default();
    let a = group.add_atom("C", Point::new(0., 0.));
    let b = group.add_atom("C", Point::new(42., 0.));
    let c = group.add_atom("O", Point::new(84., 0.));
    group.add_bond(a, b, 1, "plain");
    group.add_bond(b, c, 1, "plain");
    group.annotations.push(Annotation {
        id: 4,
        position: Point::new(0., 60.),
        text: "ethyl alcohol".into(),
        format: Default::default(),
    });
    let n = group.add_atom("N", Point::new(150., 50.));
    group.groups.push(Group {
        id: 6,
        members: vec![a, b, c, 4],
        integral: true,
    });
    group.groups.push(Group {
        id: 7,
        members: vec![a, b, c, 4, n],
        integral: false,
    });
    docs.push(("nested mixed groups".into(), group.clone()));
    group
        .abbreviations
        .push(reshiki::abbreviations::Abbreviation {
            label: "Et".into(),
            reverse_label: String::new(),
            anchor: b,
            members: vec![a, b],
        });
    docs.push(("grouped abbreviation".into(), group.clone()));
    group.atom_mut(b).context("Missing anchor")?.text_style = Some(TextStyle {
        family: "Helvetica".into(),
        color: [60, 130, 210],
        size_pt: 14.,
        ..Default::default()
    });
    let ids = group.all_ids();
    reshiki::editing::transform(
        &mut group,
        &ids,
        reshiki::editing::Transform::FlipHorizontal,
    );
    docs.push(("reversed styled abbreviation".into(), group.clone()));
    group.atom_mut(a).context("Missing member")?.display.number =
        Some(reshiki::atom_labels::Number {
            text: "A1".into(),
            offset: None,
            style: Default::default(),
        });
    docs.push(("reject hidden member number".into(), group));

    let pixels = image::RgbaImage::from_fn(7, 5, |x, y| {
        image::Rgba([(x * 30) as u8, (y * 40) as u8, 123, (x * 13 + y * 30) as u8])
    });
    let mut buffer = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut buffer, image::ImageFormat::Png)?;
    let picture =
        reshiki::pictures::Picture::import(&buffer.into_inner()).map_err(anyhow::Error::msg)?;
    for angle in [0., 37.25, 90., -105.] {
        for flip in [false, true] {
            let mut doc = picture.document();
            let ids = doc.all_ids();
            reshiki::editing::transform(&mut doc, &ids, reshiki::editing::Transform::Rotate(angle));
            if flip {
                reshiki::editing::transform(
                    &mut doc,
                    &ids,
                    reshiki::editing::Transform::FlipVertical,
                );
            }
            let g = doc.graphics.first_mut().context("Missing image")?;
            g.layer = 3;
            let aid = doc.add_atom("O", Point::new(-300., 200.));
            let gid = doc.graphics.first().context("Missing image")?.id;
            doc.groups.push(Group {
                id: doc.next_id(),
                members: vec![aid, gid],
                integral: true,
            });
            docs.push((format!("picture/{angle}/{flip}"), doc));
        }
    }
    // Invalid affine frames must fail instead of flattening their content.
    let mut sheared = picture.document();
    let g = sheared.graphics.first_mut().context("Missing image")?;
    assert_eq!(g.kind, GraphicKind::Picture);
    g.axis_y.x += g.axis_x.x * 0.3;
    docs.push(("reject sheared picture".into(), sheared));
    Ok(docs)
}
