use reshiki::{
    bonds::{BondPreset, DoublePosition},
    document::{Annotation, Document, Point},
    editing::{self, Transform},
    engine::{ChemistryEngine, PythonEngine, Request},
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
    assert!(matches!(
        &scene::primitives(&single(BondPreset::Wedge))[0],
        Primitive::Polygon(_)
    ));
    let hollow = scene::primitives(&single(BondPreset::HollowWedge));
    assert_eq!(hollow.len(), 3);
    assert!(hollow.iter().all(|p| matches!(p, Primitive::Line(..))));
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

#[tokio::test]
async fn reflected_stereo_styles_keep_the_drawn_molecular_identity() {
    let engine = PythonEngine::default();
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
    let engine = PythonEngine::default();
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
