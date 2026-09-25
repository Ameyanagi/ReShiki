use reshiki::{
    attachments, crossings,
    document::{Document, Point},
    graphics::{PathCommand, flattened},
    hotkeys, projection,
    scene::{self, Primitive},
};

const BLUE: [u8; 3] = [32, 80, 145];

#[test]
fn crossings_interpolate_depth_at_the_intersection_not_the_bond_midpoint() -> Result<(), String> {
    let mut doc = Document::default();
    for (p, z) in [
        (Point::new(-80., 0.), -10.),
        (Point::new(20., 0.), 90.),
        (Point::new(0., -20.), 60.),
        (Point::new(0., 80.), 60.),
    ] {
        let id = doc.add_atom("C", p);
        doc.atom_mut(id).ok_or("atom")?.depth = z;
    }
    doc.add_bond(1, 2, 1, "plain");
    doc.add_bond(3, 4, 1, "plain");
    for reverse in [false, true] {
        if reverse {
            for b in &mut doc.bonds {
                b.reverse();
            }
        }
        let gaps = crossings::gaps(&doc);
        assert!(gaps.first().ok_or("first")?.is_empty());
        assert_eq!(gaps.get(1).ok_or("second")?.len(), 1);
    }
    Ok(())
}
fn curves(doc: &Document) -> Vec<Vec<PathCommand>> {
    scene::primitives(doc)
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Path {
                commands,
                style,
                filled: false,
            } if style.stroke == BLUE
                && commands.iter().any(|c| matches!(c, PathCommand::Cubic(..))) =>
            {
                Some(commands)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn moving_a_ligand_across_the_metal_updates_outline_and_circle_occlusion() -> Result<(), String> {
    for key in ["j", "J"] {
        for legacy in [false, true] {
            let mut source = Document::default();
            let metal = source.add_atom("Ru", Point::default());
            let mut doc = hotkeys::atom_edit(&source, metal, key, 42.)
                .ok_or("shortcut")??
                .0;
            let anchor = doc
                .atoms
                .iter()
                .find(|a| a.attachment.is_some())
                .ok_or("anchor")?
                .clone();
            let contact = doc
                .bonds
                .iter()
                .position(|b| b.a == metal || b.b == metal)
                .ok_or("contact")?;
            for (i, b) in doc.bonds.iter_mut().enumerate() {
                if i == contact {
                    b.z_order = if legacy { -1 } else { 0 };
                } else {
                    b.color = BLUE;
                }
            }
            let mut bare = doc.clone();
            bare.bonds.remove(contact);
            assert_eq!(
                curves(&doc),
                curves(&bare),
                "near side must remain unbroken"
            );
            assert!(!crossings::gaps(&doc).get(contact).ok_or("gap")?.is_empty());
            let before = doc.clone();
            let ids = attachments::movement_selection(&doc, &[anchor.id]);
            doc.translate(&ids, -2. * anchor.position.x, -2. * anchor.position.y);
            assert_eq!(doc.atom(metal), before.atom(metal));
            for atom in &doc.atoms {
                assert_eq!(Some(atom.depth), before.atom(atom.id).map(|a| a.depth));
            }
            let gaps = crossings::gaps(&doc);
            assert!(
                gaps.get(contact).ok_or("gap")?.is_empty(),
                "far side must not cut the contact: {key}/{legacy}"
            );
            assert_eq!(
                gaps.iter().filter(|g| !g.is_empty()).count(),
                if key == "j" { 2 } else { 1 }
            );
            for angle in [30., 60., 90., 120., 180., 210., 270.] {
                let mut rotated = doc.clone();
                let all = rotated.all_ids();
                reshiki::editing::transform_about(&mut rotated, &all, Point::default(), 1., angle);
                let gaps = crossings::gaps(&rotated);
                assert!(gaps.get(contact).ok_or("contact gaps")?.is_empty());
                assert_eq!(
                    gaps.iter().filter(|g| !g.is_empty()).count(),
                    if key == "j" { 2 } else { 1 },
                    "vertex clearance: {key}/{angle}"
                );
            }
            let mut bare = doc.clone();
            bare.bonds.remove(contact);
            assert_ne!(
                curves(&doc),
                curves(&bare),
                "far ellipse must clear the contact"
            );
            let saved: Document =
                serde_json::from_str(&serde_json::to_string(&doc).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            assert_eq!(saved, doc);
            assert_eq!(scene::svg(&saved), scene::svg(&doc));
            // User-requested layers remain authoritative even with retained XYZ.
            doc.bonds.get_mut(contact).ok_or("contact")?.z_order = -2;
            assert!(!crossings::gaps(&doc).get(contact).ok_or("gap")?.is_empty());
            assert_eq!(curves(&doc), curves(&bare));
        }
    }
    Ok(())
}

#[test]
fn one_contact_can_cross_in_front_of_the_far_side_and_behind_the_near_side() -> Result<(), String> {
    let mut doc = Document::default();
    let ring = reshiki::editing::ring(&mut doc, Point::default(), 6, true, 0.);
    projection::tilt(&mut doc, &ring, 60., false);
    for b in &mut doc.bonds {
        b.color = BLUE;
    }
    let a = doc.add_atom("C", Point::new(-100., 0.));
    let b = doc.add_atom("C", Point::new(100., 0.));
    doc.add_bond(a, b, 1, "plain");
    let paths = curves(&doc);
    assert!(!paths.is_empty());
    let points: Vec<_> = paths.iter().flat_map(|p| flattened(p)).flatten().collect();
    let clearance = doc.drawing_style.line_width() / 2. + reshiki::style::DEFAULT.world(1.1);
    assert!(
        points
            .iter()
            .filter(|p| p.x > 0.)
            .all(|p| p.y.abs() >= clearance - 0.01)
    );
    assert!(
        points.iter().any(|p| p.x < 0. && p.y.abs() < 0.01),
        "near side stays continuous"
    );
    let lines: Vec<_> = scene::primitives(&doc)
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Line(a, b, _) if a.y.abs() < 0.001 && b.y.abs() < 0.001 => Some((a, b)),
            _ => None,
        })
        .collect();
    assert!(
        lines.iter().any(|(a, b)| a.x < 0. && b.x > 90.),
        "front half of contact is continuous"
    );
    assert!(lines.len() >= 2, "back half is interrupted");
    assert!(!scene::svg(&doc).contains("rgb(255,255,255)"));
    Ok(())
}

#[test]
fn later_tilts_restyle_projected_edges_without_changing_chemistry_or_stereo_wedges()
-> Result<(), String> {
    let mut doc = Document::default();
    let metal = doc.add_atom("Ru", Point::default());
    doc = hotkeys::atom_edit(&doc, metal, "J", 42.)
        .ok_or("shortcut")??
        .0;
    let anchor = doc
        .atoms
        .iter()
        .find(|a| a.attachment.is_some())
        .ok_or("anchor")?
        .id;
    let ids = attachments::movement_selection(&doc, &[anchor]);
    let other_metal = doc.add_atom("Fe", Point::new(500., 0.));
    doc = hotkeys::atom_edit(&doc, other_metal, "j", 42.)
        .ok_or("other shortcut")??
        .0;
    // A tilt of the first ligand must not restyle a different ligand that the
    // user has emphasized manually.
    for b in &mut doc.bonds {
        if b.projection && !ids.contains(&b.a) {
            b.display = "bold".into();
        }
    }
    let a = doc.add_atom("C", Point::new(200., 0.));
    let b = doc.add_atom("C", Point::new(240., 0.));
    doc.add_bond(a, b, 1, "wedge");
    let original = doc.clone();
    for _ in 0..2 {
        projection::tilt(&mut doc, &ids, 75., false);
    }
    assert_ne!(doc.bonds, original.bonds, "perspective must update");
    assert_eq!(
        doc.bonds.last(),
        original.bonds.last(),
        "stereochemical wedge remains unchanged"
    );
    assert_eq!(
        doc.bonds
            .iter()
            .filter(|b| !ids.contains(&b.a))
            .collect::<Vec<_>>(),
        original
            .bonds
            .iter()
            .filter(|b| !ids.contains(&b.a))
            .collect::<Vec<_>>()
    );
    let mean = ids
        .iter()
        .filter_map(|id| doc.atom(*id))
        .map(|a| a.depth)
        .sum::<f32>()
        / ids.len() as f32;
    for b in doc
        .bonds
        .iter()
        .filter(|b| b.projection && ids.contains(&b.a))
    {
        assert_eq!(b.order, 4);
        let a = doc.atom(b.a).ok_or("a")?;
        let z = doc.atom(b.b).ok_or("b")?;
        match b.display.as_str() {
            "bold" => assert!(a.depth > mean && z.depth > mean),
            "wedge" => assert!(a.depth < mean && z.depth > mean),
            "plain" => assert!(a.depth <= mean && z.depth <= mean),
            _ => return Err("Unexpected perspective style".into()),
        }
    }
    for _ in 0..2 {
        projection::tilt(&mut doc, &ids, -75., false);
    }
    assert_eq!(doc.bonds, original.bonds);
    for a in &doc.atoms {
        let old = original.atom(a.id).ok_or("original")?;
        assert!(a.position.distance(old.position) < 0.001);
        assert!((a.depth - old.depth).abs() < 0.001);
        assert_eq!(
            (a.charge, a.explicit_h, &a.stereo),
            (old.charge, old.explicit_h, &old.stereo)
        );
    }
    Ok(())
}
