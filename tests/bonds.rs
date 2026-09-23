use reshiki::{
    bonds::{BondPreset, DoublePosition},
    document::{Annotation, Document, Point},
    editing::{self, Transform},
    engine::{ChemistryEngine, LocalEngine, Request},
    scene::{self, Primitive},
};

fn single(preset: BondPreset) -> Document {
    let mut d = Document::default();
    let (one, two) = match preset {
        BondPreset::Dotted => ("H", "N"),
        BondPreset::Dative | BondPreset::Dashed => ("N", "Cu"),
        BondPreset::Quadruple => ("Re", "Re"),
        _ => ("C", "C"),
    };
    let a = d.add_atom(one, Point::new(0., 0.));
    let b = d.add_atom(two, Point::new(84., 0.));
    if two == "Cu" {
        d.atom_mut(b).unwrap().charge = 2;
    }
    let (order, display, _) = preset.parts();
    d.add_bond(a, b, order, display);
    preset.apply(&mut d.bonds[0]);
    if preset == BondPreset::Dotted {
        let donor = d.add_atom("O", Point::new(-42., 0.));
        d.add_bond(donor, a, 1, "plain");
    }
    d
}

#[test]
fn bond_presets_have_distinct_geometry_and_valid_native_state() {
    for preset in BondPreset::ALL {
        let d = single(preset);
        d.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<Document>(&serde_json::to_string(&d).unwrap()).unwrap(),
            d
        );
        assert_eq!(BondPreset::of(&d.bonds[0]), Some(preset));
        assert!(!scene::primitives(&d).is_empty());
    }
    use reshiki::graphics::{PathCommand, flattened};
    let solid = scene::primitives(&single(BondPreset::Wedge));
    let Primitive::Path {
        commands,
        filled: true,
        style,
    } = &solid[0]
    else {
        panic!("A solid wedge must have a filled outline");
    };
    assert_eq!(style.fill, Some([0, 0, 0]));
    assert_eq!(commands.last(), Some(&PathCommand::Close));
    let outline = &flattened(commands)[0];
    assert!(outline[0].distance(*outline.last().unwrap()) < 0.001);
    let hollow = scene::primitives(&single(BondPreset::HollowWedge));
    assert_eq!(hollow.len(), 1);
    let Primitive::Path {
        commands,
        filled: false,
        ..
    } = &hollow[0]
    else {
        panic!("A hollow wedge must retain an unfilled outline");
    };
    assert_eq!(commands.last(), Some(&PathCommand::Close));
    assert_eq!(flattened(commands)[0], *outline);
    let dashed = scene::svg(&single(BondPreset::Dashed));
    let dotted = scene::svg(&single(BondPreset::Dotted));
    assert!(dashed.contains("stroke-dasharray"));
    assert_ne!(dashed, dotted);
    let both = scene::svg(&single(BondPreset::DoubleDashed));
    assert_eq!(both.matches("stroke-dasharray").count(), 2);
    assert_eq!(
        scene::svg(&single(BondPreset::DashedDouble))
            .matches("stroke-dasharray")
            .count(),
        1
    );
    let mut malformed = single(BondPreset::HollowWedge);
    malformed.bonds[0].order = 3;
    assert!(malformed.validate().is_err());
}

#[test]
fn wavy_bonds_are_continuous_curves_with_even_pitch_in_every_direction() {
    use reshiki::graphics::PathCommand;
    for end in [
        Point::new(42., 0.),
        Point::new(84., 0.),
        Point::new(36., -36.),
        Point::new(3., 0.),
    ] {
        let mut d = single(BondPreset::Wavy);
        d.atoms[1].position = end;
        d.bonds[0].color = [32, 80, 145];
        let drawing = scene::primitives(&d);
        assert_eq!(drawing.len(), 1, "A wave must be one stroked path");
        let Primitive::Path {
            commands,
            style,
            filled,
        } = &drawing[0]
        else {
            panic!("Expected a smooth path");
        };
        assert!(!filled);
        assert_eq!(style.stroke, [32, 80, 145]);
        assert_eq!(style.width_pt, d.drawing_style.line_width_pt);
        assert_eq!(commands.first(), Some(&PathCommand::Move(Point::default())));
        let mut cursor = Point::default();
        let mut incoming: Option<Point> = None;
        for command in commands.iter().skip(1) {
            let PathCommand::Cubic(a, b, c) = command else {
                panic!("A wave must retain cubic geometry");
            };
            if let Some(incoming) = incoming {
                let outgoing = Point::new(a.x - cursor.x, a.y - cursor.y);
                assert!((incoming.x * outgoing.y - incoming.y * outgoing.x).abs() < 0.0001);
                assert!(incoming.x * outgoing.x + incoming.y * outgoing.y > 0.);
            }
            incoming = Some(Point::new(c.x - b.x, c.y - b.y));
            cursor = *c;
        }
        assert_eq!(cursor, end);
        let svg = scene::svg(&d);
        assert!(svg.contains("<path") && svg.contains("C") && !svg.contains("<line"));
    }
    let short = reshiki::bonds::wavy_path(Point::default(), Point::new(42., 0.), 10.5, 2.);
    let long = reshiki::bonds::wavy_path(Point::default(), Point::new(84., 0.), 10.5, 2.);
    assert_eq!(long.len() - 1, 2 * (short.len() - 1));
}

#[test]
fn double_bond_side_reflects_and_reverses_without_changing_order() {
    let mut d = single(BondPreset::Double);
    for (position, expected) in [(DoublePosition::Left, -1.), (DoublePosition::Right, 1.)] {
        d.bonds[0].double_position = position;
        let Primitive::Line(a, b, _) = scene::primitives(&d)[1] else {
            panic!("expected second line")
        };
        assert_eq!(a.y.signum(), expected);
        assert_eq!(a.y, b.y);
        assert!(a.x > 0. && b.x < 84.);
        let before = d.clone();
        d.bonds[0].reverse();
        assert_eq!(d.bonds[0].double_position, position.reversed());
        d.bonds[0].reverse();
        assert_eq!(d, before);
        let ids = d.all_ids();
        editing::transform(&mut d, &ids, Transform::FlipVertical);
        assert_eq!(d.bonds[0].double_position, position.reversed());
        editing::transform(&mut d, &ids, Transform::FlipVertical);
        assert_eq!(d, before);
    }
}

#[test]
fn bond_color_reaches_shared_vector_and_raster_exports() {
    let mut d = single(BondPreset::BoldDouble);
    d.bonds[0].color = [32, 80, 145];
    let svg = scene::svg(&d);
    assert!(svg.contains("rgb(32,80,145)"));
    let png = reshiki::export::drawing(&d, "png").unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert!(
        pixels[..info.buffer_size()]
            .chunks_exact(4)
            .any(|p| p[2] > 110 && p[0] < 60)
    );
    assert!(
        reshiki::export::drawing(&d, "pdf")
            .unwrap()
            .starts_with(b"%PDF")
    );
}

#[test]
fn hashed_wedge_bars_never_taper_below_the_normal_bond_width() {
    for width in [0.1, 0.6, 1.5] {
        let mut doc = single(BondPreset::Wedge);
        doc.bonds[0].display = "hash".into();
        doc.drawing_style.line_width_pt = width;
        doc.drawing_style.bold_width_pt = width * 3.;
        let bars = scene::primitives(&doc);
        assert!(bars.len() > 1);
        for bar in bars {
            let Primitive::Line(a, b, stroke) = bar else {
                panic!("Expected separated hash bars");
            };
            assert!(a.distance(b) >= doc.drawing_style.line_width() - 0.001);
            assert_eq!(stroke, doc.drawing_style.line_width());
        }
    }
}

#[tokio::test]
async fn reflected_stereo_styles_keep_the_drawn_molecular_identity() {
    let engine = LocalEngine::default();
    for smiles in ["F[C@](Cl)(Br)I", "F[C@@](Cl)(Br)I"] {
        for up_style in ["bold", "hollow_wedge"] {
            let initial = engine
                .execute(Request::import("smiles", smiles))
                .await
                .unwrap();
            let expected = initial.analysis.unwrap().smiles;
            let mut d = initial.document.unwrap();
            for bond in &mut d.bonds {
                bond.display = match bond.display.as_str() {
                    "wedge" => up_style,
                    "hash" => "hashed",
                    other => other,
                }
                .into();
            }
            let ids = d.all_ids();
            editing::transform(&mut d, &ids, Transform::FlipHorizontal);
            for atom in &mut d.atoms {
                atom.stereo = None;
            }
            let result = engine
                .execute(Request::molecule("analyze", d))
                .await
                .unwrap();
            assert_eq!(result.analysis.unwrap().smiles, expected);
        }
    }
    let mut double = single(BondPreset::BoldDouble);
    let ids = double.all_ids();
    editing::transform(&mut double, &ids, Transform::FlipVertical);
    assert_eq!(double.bonds[0].display, "bold");
}

#[tokio::test]
async fn bond_gallery_survives_checks_cleanup_and_cdxml_with_appearance_intact() {
    let engine = LocalEngine::default();
    let mut gallery = Document::default();
    for (i, preset) in BondPreset::ALL.into_iter().enumerate() {
        let mut d = single(preset);
        d.bonds[0].color = [32, 80, 145];
        if [2, 7].contains(&d.bonds[0].order) {
            d.bonds[0].double_position = DoublePosition::Left;
        }
        for op in ["analyze", "clean"] {
            let r = engine
                .execute(Request::molecule(op, d.clone()))
                .await
                .unwrap()
                .document
                .unwrap();
            assert_eq!(r.bonds[0].display, d.bonds[0].display, "{preset:?} {op}");
            assert_eq!(r.bonds[0].secondary_display, d.bonds[0].secondary_display);
            assert_eq!(r.bonds[0].double_position, d.bonds[0].double_position);
            assert_eq!(r.bonds[0].color, d.bonds[0].color);
        }
        let ids = editing::append(
            &mut gallery,
            &d,
            Point::new((i % 3) as f32 * 280., (i / 3) as f32 * 140.),
        );
        let origin = gallery.atom(ids[0]).unwrap().position;
        gallery.annotations.push(Annotation {
            id: gallery.next_id(),
            position: origin.offset(0., 30.),
            text: preset.name().into(),
            format: Default::default(),
        });
    }
    let mut req = Request::molecule("export", gallery.clone());
    req.format = Some("cdxml".into());
    let xml = engine.execute(req).await.unwrap().output.unwrap();
    let doc = engine
        .execute(Request::import("cdxml", &xml))
        .await
        .unwrap()
        .document
        .unwrap();
    doc.validate().unwrap();
    assert_eq!(doc.bonds.len(), gallery.bonds.len());
    for (a, b) in gallery.bonds.iter().zip(&doc.bonds) {
        assert_eq!(a.display, b.display);
        assert_eq!(a.secondary_display, b.secondary_display);
        assert_eq!(a.color, b.color);
        assert_eq!(a.double_position, b.double_position);
    }
    if let Ok(dir) = std::env::var("RESHIKI_BOND_QA_DIR") {
        let path = std::path::Path::new(&dir);
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("bond-gallery.reshiki"),
            serde_json::to_string_pretty(&gallery).unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("bond-gallery.cdxml"), xml).unwrap();
        std::fs::write(path.join("bond-gallery.svg"), scene::svg(&gallery)).unwrap();
    }
}
