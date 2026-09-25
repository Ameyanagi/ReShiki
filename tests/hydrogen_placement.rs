use reshiki::{
    atom_labels::HydrogenPosition,
    document::{Document, Point},
    engine::{LocalEngine, Request},
    scene::{self, Primitive},
};

fn amine(degrees: f32, lengths: [f32; 2]) -> Document {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::default());
    for (angle, length) in [30.0_f32, 150.].into_iter().zip(lengths) {
        let (sin, cos) = (angle + degrees).to_radians().sin_cos();
        let c = doc.add_atom("C", Point::new(cos * length, sin * length));
        doc.add_bond(n, c, 1, "plain");
    }
    doc.atom_mut(n).unwrap().label_h = 1;
    doc
}

fn position(doc: &Document, label: &str) -> Point {
    scene::primitives(doc)
        .into_iter()
        .find_map(|p| match p {
            Primitive::Text { text, position, .. } if text == label => Some(position),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing label {label}"))
}

fn assert_direction(doc: &Document, expected: HydrogenPosition) {
    let n = position(doc, "N");
    let h = position(doc, "H");
    match expected {
        HydrogenPosition::Above => assert!(h.y < n.y - 10. && (h.x - n.x).abs() < 0.1),
        HydrogenPosition::Below => assert!(h.y > n.y + 10. && (h.x - n.x).abs() < 0.1),
        HydrogenPosition::Left => assert!(h.x < n.x - 10. && (h.y - n.y).abs() < 0.1),
        HydrogenPosition::Right => assert!(h.x > n.x + 10. && (h.y - n.y).abs() < 0.1),
        HydrogenPosition::Auto => panic!("Expected a resolved direction"),
    }
}

#[test]
fn internal_hydrogens_use_open_sector_in_each_orientation_and_at_unequal_lengths() {
    use HydrogenPosition as H;
    for lengths in [[42., 42.], [20., 90.], [90., 20.]] {
        for (degrees, expected) in [
            (0., H::Above),
            (90., H::Right),
            (180., H::Below),
            (270., H::Left),
        ] {
            let doc = amine(degrees, lengths);
            let before = doc.clone();
            assert_direction(&doc, expected);
            assert_eq!(
                doc, before,
                "Rendering must not change chemistry or saved preferences"
            );
        }
    }
}

#[test]
fn manual_positions_override_automatic_placement_and_terminal_labels_stay_inline() {
    use HydrogenPosition as H;
    let mut doc = amine(0., [42., 42.]);
    for position in [H::Left, H::Right, H::Above, H::Below] {
        doc.atoms[0].display.hydrogen_position = position;
        assert_direction(&doc, position);
    }
    doc.atoms[0].display.hydrogen_position = H::Auto;
    doc.bonds.pop();
    assert_direction(&doc, H::Left);
    doc.bonds.clear();
    assert_direction(&doc, H::Right);
}

#[test]
fn explicit_hydrogen_count_is_stacked_without_losing_subscript_charge_or_isotope() {
    let mut doc = amine(0., [42., 42.]);
    let n = &mut doc.atoms[0];
    n.no_implicit = true;
    n.explicit_h = 2;
    n.charge = 1;
    n.isotope = 15;
    let h = position(&doc, "H");
    let count = position(&doc, "2");
    assert!(h.y < position(&doc, "N").y - 10.);
    assert!(count.x > h.x && count.y > h.y);
    position(&doc, "+");
    position(&doc, "15");
    doc.validate().unwrap();
}

#[tokio::test]
async fn paracetamol_formula_and_editable_round_trip_are_unchanged() {
    let doc: Document = serde_json::from_str(include_str!(
        "../docs/changes/fixtures/automatic-hydrogen.rsk"
    ))
    .unwrap();
    let engine = LocalEngine::default();
    let original = engine
        .request(Request::molecule("analyze", doc))
        .await
        .unwrap();
    let drawing = original.document.unwrap();
    let n = position(&drawing, "N");
    assert!(scene::primitives(&drawing).iter().any(|p| matches!(p,
        Primitive::Text { position, text, .. } if text == "H" && position.y < n.y - 10. && (position.x-n.x).abs()<0.1
    )));
    let mut export = Request::molecule("export", drawing);
    export.format = Some("cdxml".into());
    let xml = engine.request(export).await.unwrap().output.unwrap();
    let back = engine
        .request(Request::import("cdxml", &xml))
        .await
        .unwrap();
    let a = original.analysis.unwrap();
    let b = back.analysis.unwrap();
    assert_eq!(a.formula, "C8H9NO2");
    assert_eq!(a.formula, b.formula);
    assert_eq!(a.inchikey, b.inchikey);
    let back = back.document.unwrap();
    let n = position(&back, "N");
    assert!(scene::primitives(&back).iter().any(|p| matches!(p,
        Primitive::Text { position, text, .. } if text == "H" && position.y < n.y - 10. && (position.x-n.x).abs()<0.1
    )));
}

#[test]
fn internal_condensed_labels_keep_the_attachment_atom_and_subscripts_in_every_direction() {
    use reshiki::atom_text::{self, Mode};
    for label in [
        "CH2", "SiH2", "CCl2", "CBr2", "CF2", "NMe", "NMe2+", "SiMe2", "PPh", "C(OH)2", "CCl₂",
    ] {
        for (degrees, expected) in [
            (0., HydrogenPosition::Above),
            (90., HydrogenPosition::Right),
            (180., HydrogenPosition::Below),
            (270., HydrogenPosition::Left),
        ] {
            let source = amine(degrees, [42., 75.]);
            let id = source.atoms[0].id;
            let doc = atom_text::apply(&source, id, label, Mode::Auto).unwrap();
            let before = doc.clone();
            let core = if label.starts_with("Si") {
                "Si"
            } else {
                &label[..1]
            };
            let runs = scene::primitives(&doc);
            let core_pos = position(&doc, core);
            let suffix = runs
                .iter()
                .filter_map(|p| match p {
                    Primitive::Text {
                        position,
                        text,
                        size,
                        ..
                    } if text != core => Some((*position, text, *size)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(!suffix.is_empty(), "{label} lost its appendage");
            let first = suffix[0].0;
            match expected {
                HydrogenPosition::Above => assert!(
                    first.y < core_pos.y && (first.x - core_pos.x).abs() < 0.01,
                    "{label}"
                ),
                HydrogenPosition::Below => assert!(
                    first.y > core_pos.y && (first.x - core_pos.x).abs() < 0.01,
                    "{label}"
                ),
                HydrogenPosition::Left => assert!(first.x < core_pos.x, "{label}"),
                HydrogenPosition::Right => assert!(first.x > core_pos.x, "{label}"),
                _ => unreachable!(),
            }
            if label.contains('2') {
                let count = suffix
                    .iter()
                    .find(|(_, text, _)| text.as_str() == "2")
                    .unwrap();
                assert!(
                    count.0.y > first.y && count.2 < suffix[0].2,
                    "{label} lost its subscript"
                );
            }
            let encoded = serde_json::to_string(&doc).unwrap();
            let back: Document = serde_json::from_str(&encoded).unwrap();
            assert_eq!(back, before);
            assert_eq!(doc, before, "Layout must not reinterpret chemical data");
        }
    }
}

#[test]
fn free_names_remain_unsplit_and_horizontal_bonds_leave_the_core_centered() {
    use reshiki::atom_text::{self, Mode};
    let source = amine(0., [42., 42.]);
    for label in [
        "Boc",
        "Cp*",
        "custom ligand",
        "Name",
        "R1",
        "MyGroup",
        "C2H5",
        "C(OH",
        "CCl2 word",
    ] {
        let doc = atom_text::apply(&source, source.atoms[0].id, label, Mode::Text).unwrap();
        position(&doc, label);
    }
    for label in ["CH2", "CCl2", "NMe"] {
        let mut doc = atom_text::apply(&source, source.atoms[0].id, label, Mode::Auto).unwrap();
        doc.atoms[1].position = Point::new(-42., 0.);
        doc.atoms[2].position = Point::new(42., 0.);
        let core = &label[..1];
        let core_pos = position(&doc, core);
        let parts = scene::primitives(&doc);
        assert!(parts.iter().any(|p| matches!(p,
            Primitive::Text { text, position, .. } if text != core && position.y < core_pos.y
        )));
    }
}

#[test]
fn oblique_label_placement_matches_observed_full_rotation_and_boundary_cases() {
    use HydrogenPosition::{Above as A, Below as B, Left as L, Right as R};
    // Independent desktop observations at 15-degree increments; the second
    // row has collinear bonds rather than a 120-degree internal angle.
    let bent = [
        A, A, R, R, R, R, R, R, R, R, R, B, B, B, L, L, L, L, L, L, L, L, L, A,
    ];
    let straight = [
        A, A, R, R, R, R, R, L, L, L, L, A, A, A, R, R, R, R, R, L, L, L, L, A,
    ];
    for (collinear, expected) in [(false, bent), (true, straight)] {
        for (i, expected) in expected.into_iter().enumerate() {
            let mut doc = amine(i as f32 * 15., [42., 85.]);
            if collinear {
                let (sin, cos) = (i as f32 * 15.).to_radians().sin_cos();
                doc.atoms[1].position = Point::new(cos * 42., sin * 42.);
                doc.atoms[2].position = Point::new(-cos * 85., -sin * 85.);
            }
            assert_direction(&doc, expected);
            doc.bonds.reverse();
            for bond in &mut doc.bonds {
                std::mem::swap(&mut bond.a, &mut bond.b);
            }
            assert_direction(&doc, expected);
        }
    }
    for (angle, expected) in [
        (21.9, A),
        (22., A),
        (23., R),
        (23.1, R),
        (157., R),
        (158., B),
    ] {
        assert_direction(&amine(angle, [42., 85.]), expected);
    }
}
