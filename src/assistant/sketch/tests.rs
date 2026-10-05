use super::*;
fn sandwich() -> Sketch {
    let mut atoms = vec![Atom {
        color: None,
        variable: None,
        element: "Fe".into(),
        x: 0.,
        y: 0.,
        charge: 0,
        isotope: 0,
        hydrogens: 0,
    }];
    let mut bonds = Vec::new();
    let mut shapes = Vec::new();
    for cy in [-1.5, 1.5] {
        let first = atoms.len();
        for i in 0..5 {
            let angle = (-90. + i as f32 * 72.).to_radians();
            atoms.push(Atom {
                color: None,
                variable: None,
                element: "C".into(),
                x: angle.cos(),
                y: cy + angle.sin() * 0.65,
                charge: 0,
                isotope: 0,
                hydrogens: 1,
            });
            bonds.push(Bond {
                ring_arc: false,
                a: first + i,
                b: first + (i + 1) % 5,
                order: 1,
                display: if i == 2 { "bold" } else { "plain" }.into(),
            });
        }
        shapes.push(Shape {
            kind: ShapeKind::Ellipse,
            start: Point::new(-0.6, cy - 0.35),
            end: Point::new(0.6, cy + 0.35),
            dashed: false,
        });
        shapes.push(Shape {
            kind: ShapeKind::Line,
            start: Point::new(0., cy),
            end: Point::new(0., cy.signum() * 0.35),
            dashed: true,
        });
    }
    Sketch {
        atoms,
        bonds,
        shapes,
        tilts: vec![],
        centroids: vec![],
        arrows: vec![],
        captions: vec![],
        abbreviations: vec![],
        ligands: vec![],
    }
}
#[test]
fn flat_scheme_keeps_variables_colors_arrows_and_ring_curves() {
    let mut sketch = sandwich();
    sketch.shapes.clear();
    sketch.atoms[0].element = "Cu".into();
    sketch.atoms[0].color = Some([0, 0, 255]);
    sketch.atoms[1].element = "*".into();
    sketch.atoms[1].variable = Some("E".into());
    sketch.atoms[1].color = Some([220, 40, 40]);
    sketch.bonds[0].ring_arc = true;
    sketch.bonds[1].ring_arc = true;
    sketch.arrows.push(Arrow {
        start: Point::new(2., 0.),
        end: Point::new(5., 0.),
    });
    sketch.captions.push(Caption {
        text: "Cu(acac)₂".into(),
        position: Point::new(2., -0.8),
        color: None,
    });
    let doc = sketch.render(&Default::default()).unwrap();
    assert!(doc.atoms.iter().all(|a| a.depth == 0.));
    assert_eq!(doc.arrows.len(), 1);
    assert_eq!(doc.annotations[0].text, "Cu(acac)₂");
    assert_eq!(doc.atoms[1].element, "*");
    assert_eq!(doc.atoms[1].display.variable.as_deref(), Some("E"));
    assert_eq!(crate::ring_arcs::render(&doc).bonds.len(), 2);
    let svg = crate::scene::svg(&doc);
    assert!(svg.contains(">E</text>"));
    assert!(svg.contains("rgb(0,0,255)"));
    assert_eq!(
        serde_json::from_str::<Document>(&serde_json::to_string(&doc).unwrap()).unwrap(),
        doc
    );
    sketch.atoms[1].element = "O".into();
    assert!(sketch.validate().is_err());
}

#[test]
fn native_ring_centroids_and_tilts_render_valid_contacts() -> Result<(), String> {
    for kind in [
        None,
        Some(crate::attachments::Kind::MultiCenter),
        Some(crate::attachments::Kind::Variable),
    ] {
        let mut sketch = sandwich();
        sketch
            .shapes
            .retain(|s| matches!(s.kind, ShapeKind::Ellipse));
        for (index, atoms) in [
            (0, (1..6).collect::<Vec<_>>()),
            (1, (6..11).collect::<Vec<_>>()),
        ] {
            sketch.tilts.push(Tilt {
                atoms: atoms.clone(),
                shapes: vec![index],
                x_degrees: 30.,
                y_degrees: 0.,
                depth_bonds: true,
            });
            sketch.centroids.push(Centroid {
                kind,
                atoms,
                contact: Some(0),
                contact_style: None,
            });
        }
        let doc = sketch.render(&Default::default())?;
        doc.validate()?;
        assert_eq!(doc.atoms.len(), 13);
        assert_eq!(doc.bonds.len(), 12);
        for centroid in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
            assert_eq!(centroid.centroid.len(), 5);
            let contact = doc
                .bonds
                .iter()
                .find(|b| b.a == centroid.id || b.b == centroid.id)
                .ok_or("Missing ring contact")?;
            assert_eq!(centroid.attachment, kind);
            let (order, display) = if kind.is_some() {
                (1, "plain")
            } else {
                (5, "dashed")
            };
            assert_eq!(contact.display, display);
            assert_eq!(contact.order, order);
            assert!(contact.a == 1 || contact.b == 1);
        }
        assert!(crate::assistant::canvas_tools::image(&doc).is_ok());
        assert!(
            crate::chemistry::document::prepare(&doc).is_err(),
            "No fabricated haptic molecular data"
        );
    }
    Ok(())
}

#[test]
fn sandwich_stays_editable_grouped_and_requires_chemical_review() {
    let sketch = sandwich();
    let doc = sketch.render(&Default::default()).unwrap();
    assert_eq!(doc.atoms.len(), 11);
    assert_eq!(doc.bonds.len(), 10);
    assert_eq!(doc.graphics.len(), 4);
    assert!(
        doc.bonds.iter().all(|b| b.a != 1 && b.b != 1),
        "No invented Fe-carbon sigma bonds"
    );
    assert!(doc.graphics.iter().all(|g| g.picture.is_none()));
    let targets = super::super::review::targets(&doc);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].kind, "diagram");
    let moved = super::super::review::apply(
        &doc,
        &[super::super::review::Edit::Move {
            target: targets[0].name.clone(),
            dx_pt: 18.,
            dy_pt: 0.,
        }],
        false,
    )
    .unwrap();
    let dx = moved.atoms[0].position.x - doc.atoms[0].position.x;
    assert!(dx > 0.);
    assert!((moved.graphics[0].origin.x - doc.graphics[0].origin.x - dx).abs() < 0.001);
    let mut straightened = doc.clone();
    assert_eq!(
        super::super::composition::straighten_all(&mut straightened),
        0
    );
    assert_eq!(doc, straightened);
    let roundtrip: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(doc, roundtrip);
    assert!(super::super::review::quality(&doc, &Default::default()).contains(&REVIEW_NOTE.into()));
    assert!(
        super::super::canvas_tools::image(&doc)
            .unwrap()
            .starts_with(b"\x89PNG")
    );
}
#[test]
fn malformed_diagrams_are_rejected_without_panics() {
    let mut s = sandwich();
    s.atoms[0].charge = i32::MIN;
    assert!(s.validate().is_err());
    s = sandwich();
    s.atoms[0].x = f32::NAN;
    assert!(s.validate().is_err());
    s = sandwich();
    s.bonds[0].b = usize::MAX;
    assert!(s.validate().is_err());
    s = sandwich();
    s.bonds.push(s.bonds[0].clone());
    assert!(s.validate().is_err());
    s = sandwich();
    s.shapes[0].end = s.shapes[0].start;
    assert!(s.validate().is_err());
    let proposal = super::super::Proposal {
        sketch: Some(sandwich()),
        molecules: vec![super::super::Molecule {
            smiles: "O".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(proposal.validate().is_err());
}
