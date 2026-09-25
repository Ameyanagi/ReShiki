use anyhow::Context;
use reshiki::{
    assistant::{
        self, DrawingSettings,
        sketch::{Atom, ContactStyle, Ligand, LigandKind, Sketch},
    },
    attachments,
    document::{Document, Point},
};

fn sketch(kind: LigandKind, tilted: bool) -> Sketch {
    Sketch {
        atoms: vec![Atom {
            element: "Rh".into(),
            x: 0.,
            y: 2.5,
            charge: 3,
            hydrogens: 0,
            isotope: 0,
            color: None,
            variable: None,
        }],
        bonds: vec![],
        shapes: vec![],
        tilts: vec![],
        centroids: vec![],
        arrows: vec![],
        captions: vec![],
        abbreviations: vec![],
        ligands: vec![Ligand {
            kind,
            center: Point::new(0., 0.),
            phase_degrees: 0.,
            x_degrees: if tilted { 65. } else { 0. },
            y_degrees: 0.,
            rotation_degrees: if tilted { 40. } else { 0. },
            depth_bonds: tilted,
            show_charge: true,
            contact: Some(0),
            contact_style: Some(ContactStyle::Dashed),
            contact_in_front: None,
        }],
    }
}

#[test]
fn generated_dimer_uses_each_ligands_depth_unless_explicitly_overridden() -> anyhow::Result<()> {
    let source: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/assistant-cp-star-dimer.json"))?;
    let base: Sketch = serde_json::from_value(source.clone())?;
    assert!(base.ligands.iter().all(|l| l.contact_in_front.is_none()));
    let original = base
        .render(&Default::default())
        .map_err(anyhow::Error::msg)?;
    // Both ligand planes have the same orientation. Their metals sit on
    // opposite sides on screen, so one contact crosses a far edge and the
    // other a near edge. A common hardcoded layer cannot describe this.
    for override_depth in [None, Some(false), Some(true)] {
        let mut value = source.clone();
        for ligand in value["ligands"].as_array_mut().context("Ligands")? {
            ligand["contact_in_front"] = serde_json::to_value(override_depth)?;
        }
        let sketch: Sketch = serde_json::from_value(value)?;
        let serialized = serde_json::to_string(&sketch)?;
        let restored: Sketch = serde_json::from_str(&serialized)?;
        assert!(
            restored
                .ligands
                .iter()
                .all(|l| l.contact_in_front == override_depth)
        );
        let doc = restored
            .render(&Default::default())
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            doc.atoms, original.atoms,
            "Only contact layering may change"
        );
        for (a, b) in doc.bonds.iter().zip(&original.bonds) {
            let mut normalized = a.clone();
            normalized.z_order = b.z_order;
            assert_eq!(normalized, *b);
        }
        let contacts: Vec<_> = doc
            .bonds
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                doc.atom(b.a).is_some_and(|a| a.attachment.is_some())
                    || doc.atom(b.b).is_some_and(|a| a.attachment.is_some())
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(contacts.len(), 2);
        for angle in [0., 90., 180.] {
            let mut rotated = doc.clone();
            let ids = rotated.all_ids();
            reshiki::editing::transform_about(&mut rotated, &ids, Point::default(), 1., angle);
            let gaps = reshiki::crossings::gaps(&rotated);
            for (side, index) in contacts.iter().enumerate() {
                let in_front = override_depth.unwrap_or(side == 0);
                assert_eq!(
                    gaps.get(*index).context("Contact gaps")?.is_empty(),
                    in_front,
                    "side {side}, override {override_depth:?}, rotation {angle}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn cp_star_is_an_aromatic_ligand_before_projection_not_a_traced_polygon() -> anyhow::Result<()> {
    let settings = DrawingSettings::default();
    for (kind, carbons, hydrogens, formula) in [
        (LigandKind::Cp, 5, 5, "C5H5Rh+2"),
        (LigandKind::CpStar, 10, 15, "C10H15Rh+2"),
    ] {
        let flat = sketch(kind, false)
            .render(&settings)
            .map_err(anyhow::Error::msg)?;
        let tilted = sketch(kind, true)
            .render(&settings)
            .map_err(anyhow::Error::msg)?;
        let point = tilted
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .context("Missing haptic attachment")?;
        let members: Vec<_> = tilted.atoms.iter().filter(|a| a.element == "C").collect();
        assert_eq!(members.len(), carbons);
        assert_eq!(members.iter().map(|a| a.explicit_h).sum::<u32>(), hydrogens);
        assert_eq!(members.iter().map(|a| a.charge).sum::<i32>(), -1);
        assert_eq!(point.centroid.len(), 5);
        assert_eq!(point.attachment, Some(attachments::Kind::MultiCenter));
        assert_eq!(tilted.bonds.iter().filter(|b| b.order == 4).count(), 5);
        assert_eq!(tilted.atoms.iter().filter(|a| a.aromatic).count(), 5);
        assert_eq!(
            members.iter().filter(|a| a.explicit_h == 3).count(),
            carbons - 5
        );
        assert!(
            tilted.graphics.is_empty(),
            "The circle is derived from the aromatic ring, not a loose ellipse"
        );
        assert_eq!(flat.atoms.iter().filter(|a| a.depth != 0.).count(), 0);
        assert!(members.iter().any(|a| a.depth.abs() > 1.));
        assert!(
            tilted
                .bonds
                .iter()
                .any(|b| b.order == 4 && b.display == "bold" && b.projection)
        );
        assert!(
            tilted
                .bonds
                .iter()
                .all(|b| !matches!(b.display.as_str(), "wedge" | "hash"))
        );
        for a in &tilted.atoms {
            let original = flat.atom(a.id).context("Lost atom")?;
            assert_eq!(
                (a.element.as_str(), a.explicit_h, a.charge, a.aromatic),
                (
                    original.element.as_str(),
                    original.explicit_h,
                    original.charge,
                    original.aromatic
                )
            );
        }
        for b in &tilted.bonds {
            let original = flat
                .bonds
                .iter()
                .find(|f| f.a == b.a && f.b == b.b)
                .context("Changed connectivity")?;
            assert_eq!(b.order, original.order);
            if b.a == point.id {
                continue;
            }
            let a = tilted.atom(b.a).context("Missing endpoint")?;
            let z = tilted.atom(b.b).context("Missing endpoint")?;
            let length_3d = a.position.distance(z.position).hypot(a.depth - z.depth);
            assert!((length_3d - settings.bond_length).abs() < 0.001);
        }
        let circles = reshiki::aromatic::circles(&tilted);
        assert_eq!(circles.len(), 1);
        assert!(
            circles
                .first()
                .context("Missing aromatic circle")?
                .projected_axes
                .is_some()
        );
        let chemistry = attachments::composition(&tilted).map_err(anyhow::Error::msg)?;
        assert_eq!(chemistry.formula, formula);
        let restored: Document = serde_json::from_str(&serde_json::to_string(&tilted)?)?;
        assert_eq!(restored, tilted);
        let mut untilted = tilted.clone();
        let ids: Vec<_> = untilted
            .atoms
            .iter()
            .filter(|a| a.element != "Rh")
            .map(|a| a.id)
            .collect();
        reshiki::editing::transform_about(&mut untilted, &ids, Point::default(), 1., -40.);
        reshiki::projection::tilt(&mut untilted, &ids, -65., true);
        for atom in &untilted.atoms {
            let original = flat.atom(atom.id).context("Lost atom after inverse tilt")?;
            assert!(atom.position.distance(original.position) < 0.001);
            assert!((atom.depth - original.depth).abs() < 0.001);
        }
        assert!(
            !assistant::canvas_tools::image(&tilted)
                .map_err(anyhow::Error::msg)?
                .is_empty()
        );
        let issues = assistant::review::quality(&tilted, &Default::default());
        assert!(
            !issues
                .iter()
                .any(|s| s.starts_with("Chemical assignments need review:")),
            "{issues:?}"
        );
    }
    Ok(())
}

#[test]
fn defined_ligands_reject_invalid_tilts_contacts_and_expanded_atom_counts() -> anyhow::Result<()> {
    let original = sketch(LigandKind::CpStar, true);
    for invalid in [f32::NAN, f32::INFINITY, 86., -86.] {
        let mut s = original.clone();
        s.ligands.first_mut().context("Missing ligand")?.x_degrees = invalid;
        assert!(s.validate().is_err());
    }
    let mut s = original.clone();
    s.ligands.first_mut().context("Missing ligand")?.contact = Some(99);
    assert!(s.validate().is_err());
    let mut s = original.clone();
    s.ligands = vec![s.ligands.first().context("Missing ligand")?.clone(); 17];
    assert!(s.validate().is_err());
    let mut s = original.clone();
    s.atoms = vec![s.atoms.first().context("Missing atom")?.clone(); 295];
    assert!(s.validate().is_err());
    let mut s = original.clone();
    let base = s.atoms.first().context("Missing atom")?.clone();
    s.atoms = (0..35)
        .map(|i| Atom {
            x: i as f32,
            ..base.clone()
        })
        .collect();
    for a in 0..35 {
        for b in 0..a {
            s.bonds.push(assistant::sketch::Bond {
                a,
                b,
                order: 1,
                display: "plain".into(),
                ring_arc: false,
            });
        }
    }
    assert!(
        s.validate()
            .err()
            .is_some_and(|e| e.contains("600 bond limit"))
    );
    let mut s = original;
    s.atoms.clear();
    let ligand = s.ligands.first_mut().context("Missing ligand")?;
    ligand.contact = None;
    ligand.contact_style = None;
    assert!(
        s.render(&Default::default()).is_ok(),
        "A standalone defined ligand needs no explicit atoms"
    );
    Ok(())
}
