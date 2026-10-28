use super::*;

fn crystal() -> Document {
    let mut doc = Document::default();
    let ids = ["C", "O", "H", "N", "S"].map(|s| doc.add_atom(s, Point::default()));
    let atoms = ids
        .iter()
        .enumerate()
        .map(|(i, id)| Placed {
            id: *id,
            xyz: [i as f32 * 2., 0., 0.],
        })
        .collect();
    let adps = [
        Some(Adp::Aniso([0.04, 0.09, 0.16, 0., 0., 0.])),
        Some(Adp::Iso(0.09)),
        Some(Adp::Iso(1.)),
        Some(Adp::Aniso([-0.1, 0.1, 0.1, 0., 0., 0.])),
        None,
    ];
    let sites = ids
        .into_iter()
        .zip(adps)
        .map(|(id, adp)| Site {
            id,
            label: String::new(),
            adp,
            asymmetric: true,
        })
        .collect();
    let mut model = Model3d::new(&doc, atoms, vec![], ForceField::default()).with_cell(
        &doc,
        Some(Cell {
            vectors: [[12., 0., 0.], [0., 12., 0.], [0., 0., 12.]],
            crossing: vec![],
        }),
        sites,
    );
    model.display = Display::ThreeD;
    model.rotation = [1., 0., 0., 0.];
    model.perspective = 0.;
    doc.models.push(model);
    require_version(&mut doc);
    doc
}

#[test]
fn ortep_settings_round_trip_and_defaults_keep_old_drawings_unchanged() {
    let mut doc = crystal();
    let default = serde_json::to_value(&doc).unwrap();
    for name in ["probability", "molecule_only", "include_solvent"] {
        assert!(default["models"][0].get(name).is_none());
    }
    assert_eq!(doc.file_version(), crate::document::DISTANCE_BONDS);
    let old = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(old.models, doc.models);
    let model = &mut doc.models[0];
    model.style = Style::Ellipsoids;
    model.probability = Probability::Seventy;
    model.molecule_only = true;
    model.include_solvent = false;
    assert_eq!(doc.file_version(), crate::document::ORTEP);
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.models, doc.models);
    assert_eq!(reopened.version, crate::document::ORTEP);
    doc.version = crate::document::ARROW_LABEL_GAPS;
    assert!(validate(&doc).unwrap_err().contains("version 32"));
    for update in [0, 1, 2, 3] {
        let mut doc = crystal();
        let model = &mut doc.models[0];
        match update {
            0 => model.style = Style::Ellipsoids,
            1 => model.probability = Probability::Thirty,
            2 => model.molecule_only = true,
            _ => model.include_solvent = false,
        }
        assert_eq!(required_version(&doc.models), Some(crate::document::ORTEP));
        assert_eq!(doc.file_version(), crate::document::ORTEP);
    }
}

#[test]
fn ortep_uses_adps_and_fixed_hydrogen_missing_and_invalid_fallbacks() {
    let mut doc = crystal();
    doc.models[0].style = Style::Ellipsoids;
    doc.models[0].molecule_only = true;
    require_version(&mut doc);
    let before = (doc.atoms.clone(), doc.bonds.clone());
    let parts = primitives(&doc);
    let ellipsoids: Vec<_> = parts
        .iter()
        .filter_map(|p| match p {
            Primitive::Ellipsoid { axes, section, .. } => Some((axes, section)),
            _ => None,
        })
        .collect();
    assert_eq!(ellipsoids.len(), 2);
    assert_eq!(ellipsoids.iter().filter(|(_, s)| s.is_some()).count(), 1);
    let fixed: Vec<_> = parts
        .iter()
        .filter_map(|p| match p {
            Primitive::Sphere { radius, .. } => Some(*radius),
            _ => None,
        })
        .collect();
    assert_eq!(fixed.len(), 3);
    assert!(fixed.iter().all(|r| (*r - fixed[0]).abs() < 1e-5));
    assert!(
        !parts
            .iter()
            .any(|p| matches!(p, Primitive::Text { .. } | Primitive::Wire { .. }))
    );
    let before_scale = View::new(&doc, &doc.models[0]).unwrap().scale;
    let before_radii: Vec<_> = ellipsoids
        .iter()
        .map(|(a, _)| a[0].distance(Point::default()))
        .collect();
    doc.models[0].probability = Probability::Ninety;
    let parts = primitives(&doc);
    let after_radii: Vec<_> = parts
        .iter()
        .filter_map(|p| match p {
            Primitive::Ellipsoid { axes, .. } => Some(axes[0].distance(Point::default())),
            _ => None,
        })
        .collect();
    let after_scale = View::new(&doc, &doc.models[0]).unwrap().scale;
    for (a, b) in before_radii.iter().zip(after_radii) {
        assert!(
            (b / a
                - Probability::Ninety.scale() / Probability::Fifty.scale() * after_scale
                    / before_scale)
                .abs()
                < 1e-4
        );
    }
    assert_eq!(before, (doc.atoms, doc.bonds));
}

#[test]
fn ortep_bonds_start_at_ellipsoid_surfaces_and_end_at_the_shared_midpoint() {
    let mut doc = crystal();
    doc.models[0].style = Style::Ellipsoids;
    let view = View::new(&doc, &doc.models[0]).unwrap();
    let (a, m, ..) = view.half_bond(0, 1).unwrap();
    let (b, n, ..) = view.half_bond(1, 0).unwrap();
    assert!(m.distance(n) < 1e-5);
    assert!(
        (a.x - view.project(view.rotated[0]).0.x - 0.2 * Probability::Fifty.scale() * view.scale)
            .abs()
            < 1e-4
    );
    assert!(
        (view.project(view.rotated[1]).0.x - b.x - 0.3 * Probability::Fifty.scale() * view.scale)
            .abs()
            < 1e-4
    );
}

#[test]
fn ortep_omits_bonds_with_coincident_or_overlapping_surfaces() {
    let mut doc = crystal();
    doc.models[0].style = Style::Ellipsoids;
    let ids: Vec<_> = doc.models[0].atoms.iter().map(|a| a.id).collect();
    doc.add_bond(ids[0], ids[1], 1, "plain");
    for x in [0., 0.1] {
        doc.models[0].atoms[1].xyz = [x, 0., 0.];
        let view = View::new(&doc, &doc.models[0]).unwrap();
        assert!(view.half_bond(0, 1).is_none());
        assert!(
            primitives(&doc)
                .iter()
                .all(|p| !matches!(p, Primitive::ShadedQuad { .. }))
        );
        assert!(balls_and_sticks(&doc, &doc.models[0]).unwrap().1.is_empty());
    }
    doc.models[0].hydrogens.push(Hydrogen {
        parent: ids[0],
        xyz: [1., 0., 0.],
    });
    doc.models[0].morph = Some(0.);
    assert!(
        primitives(&doc)
            .iter()
            .all(|p| !matches!(p, Primitive::ShadedQuad { .. }))
    );
}

#[test]
fn molecule_only_filters_packing_before_fitting_the_3d_view() {
    let mut doc = crystal();
    let ids: Vec<_> = doc.models[0].atoms.iter().map(|a| a.id).collect();
    doc.add_bond(ids[0], ids[1], 1, "plain");
    let model = &mut doc.models[0];
    model.style = Style::Ellipsoids;
    model.molecule_only = true;
    for site in &mut model.sites {
        site.asymmetric = site.id == ids[0];
    }
    let view = View::new(&doc, &doc.models[0]).unwrap();
    assert_eq!(view.shown, [true, true, false, false, false]);
    assert_eq!(view.center, [1., 0., 0.]);
    let frame_before = frame(&doc, &doc.models[0]);
    let ink_before =
        crate::scene::svg_part(&primitives(&doc), Point::default(), Point::new(400., 400.));
    let model = &mut doc.models[0];
    for (i, atom) in model.atoms.iter_mut().enumerate().skip(2) {
        atom.xyz = [i as f32 * 1000., 0., 0.];
    }
    model.repeat = [4; 3];
    assert_eq!(frame(&doc, &doc.models[0]), frame_before);
    assert_eq!(
        crate::scene::svg_part(&primitives(&doc), Point::default(), Point::new(400., 400.)),
        ink_before
    );
    assert_eq!(doc.models[0].atoms.len(), 5);
}
