use super::*;
use crate::{
    atom_labels::{Owner, indicators, refresh::Refresh, test_support::with_fonts},
    document::Point,
    molecule_names::{IdentityRejection, canonical_identity},
};

// BINOL, with the 1--11 axis horizontal in chemistry coordinates (Y up).
pub(crate) fn binol(display: &str) -> Document {
    let mut doc = Document::default();
    let positions = [
        (0., 0.),
        (-0.75, 1.299),
        (-2.25, 1.299),
        (-3., 0.),
        (-2.25, -1.299),
        (-0.75, -1.299),
        (-4.5, 0.),
        (-5.25, 1.299),
        (-4.5, 2.598),
        (-3., 2.598),
    ];
    for (offset, sign) in [(0., 1.), (1.5, -1.)] {
        for (x, y) in positions {
            doc.add_atom("C", Point::new((offset + sign * x) * 28., -sign * y * 28.));
        }
    }
    doc.add_atom("O", Point::new(0., -2.598 * 28.));
    doc.add_atom("O", Point::new(1.5 * 28., 2.598 * 28.));
    for offset in [0, 10] {
        for (a, b, order) in [
            (1, 2, 1),
            (2, 3, 2),
            (3, 4, 1),
            (4, 5, 2),
            (5, 6, 1),
            (6, 1, 2),
            (4, 7, 1),
            (7, 8, 2),
            (8, 9, 1),
            (9, 10, 2),
            (10, 3, 1),
        ] {
            doc.add_bond(a + offset, b + offset, order, "plain");
        }
    }
    doc.add_bond(1, 11, 1, "plain");
    doc.add_bond(2, 21, 1, "plain");
    doc.add_bond(12, 22, 1, "plain");
    doc.bonds[0].display = display.into();
    doc
}

fn labeled(doc: &Document) -> anyhow::Result<Document> {
    let molecule = prepare(doc)?;
    let drawing = for_drawing(&molecule, doc)?;
    let labels = drawing.labels()?;
    Ok(drawing.finish(labels)?)
}

#[test]
fn binol_drawing_perceives_mirrored_single_axis_wedges() -> anyhow::Result<()> {
    // Independently checked with RDKit 2026.03.6's MOL reader and CIP labeler.
    for (display, expected) in [("wedge", "P"), ("hash", "M")] {
        let doc = binol(display);
        let before = doc.clone();
        let molecule = prepare(&doc)?;
        assert!(matches!(molecule.state.metadata.bonds[22].stereo, 6 | 7));
        assert_eq!(
            canonical_identity(&molecule.state),
            Err(IdentityRejection::NonclassicalStereo)
        );
        let output = labeled(&doc)?;
        assert_eq!(output.bonds[22].cip_label.as_deref(), Some(expected));
        assert_eq!(output.bonds[0].display, display);
        assert!(output.bonds[22].stereo.is_none());
        assert_eq!(doc, before);

        // Reconstruction and cleanup's full-precision prepare both read
        // the author's visible wedges again, without a stored axial tag.
        let checked = prepare_at(&output, &molecule.positions)?;
        assert_eq!(
            checked.state.metadata.bonds[22].stereo,
            molecule.state.metadata.bonds[22].stereo
        );
        assert_eq!(
            labeled(&output)?.bonds[22].cip_label.as_deref(),
            Some(expected)
        );
        let saved = serde_json::to_string(&output)?;
        let reopened: Document = serde_json::from_str(&saved)?;
        assert_eq!(
            labeled(&reopened)?.bonds[22].cip_label.as_deref(),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn axial_stereo_requires_wedges_originating_at_the_axis() -> anyhow::Result<()> {
    let plain = binol("plain");
    let mut distant = plain.clone();
    distant.bonds[23].display = "wedge".into(); // C2--OH, away from the axis.
    let mut reversed = binol("wedge");
    let bond = &mut reversed.bonds[0];
    std::mem::swap(&mut bond.a, &mut bond.b);
    let mut removed = labeled(&binol("wedge"))?;
    removed.bonds[0].display = "plain".into();
    for doc in [plain, distant, reversed, removed] {
        let molecule = prepare(&doc)?;
        assert!(molecule.state.metadata.bonds.iter().all(|b| b.stereo == 0));
        assert!(labeled(&doc)?.bonds.iter().all(|b| b.cip_label.is_none()));
    }
    Ok(())
}

#[test]
fn unwedged_biphenyl_has_no_axial_stereo() -> anyhow::Result<()> {
    let mut doc = binol("plain");
    doc.atoms
        .retain(|a| [1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 16].contains(&a.id));
    let ids: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    doc.bonds
        .retain(|b| ids.contains(&b.a) && ids.contains(&b.b));
    let molecule = prepare(&doc)?;
    assert!(molecule.state.metadata.bonds.iter().all(|b| b.stereo == 0));
    assert_eq!(labeled(&doc)?.bonds, doc.bonds);
    Ok(())
}

#[test]
fn small_ring_wedges_do_not_create_an_axis() -> anyhow::Result<()> {
    // C1=C(O)C(O)=CCC1: the detector's provisional tag on the single bond
    // between the two sp2 carbons must be removed, as on sanitized MOL import.
    let mut doc = Document::default();
    for (element, x, y) in [
        ("C", 0.6495, -1.5),
        ("C", -0.6495, -0.75),
        ("O", -1.9486, -1.5),
        ("C", -0.6495, 0.75),
        ("O", -1.9486, 1.5),
        ("C", 0.6495, 1.5),
        ("C", 1.9486, 0.75),
        ("C", 1.9486, -0.75),
    ] {
        doc.add_atom(element, Point::new(x * 28., -y * 28.));
    }
    for (a, b, order) in [
        (1, 2, 2),
        (2, 3, 1),
        (2, 4, 1),
        (4, 5, 1),
        (4, 6, 2),
        (6, 7, 1),
        (7, 8, 1),
        (8, 1, 1),
    ] {
        doc.add_bond(
            a,
            b,
            order,
            if (a, b) == (2, 3) { "wedge" } else { "plain" },
        );
    }
    let molecule = prepare(&doc)?;
    assert!(
        molecule
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    Ok(())
}

#[test]
fn binol_axis_refresh_displays_helicity_bond_indicator() {
    with_fonts(|| {
        for (display, expected) in [("wedge", "(P)"), ("hash", "(M)")] {
            let mut doc = binol(display);
            doc.drawing_style.font_family = "Liberation Sans".into();
            doc.atom_labels.stereo = true;
            for bond in &mut doc.bonds {
                bond.indicator.style.family = "Liberation Sans".into();
            }
            let refresh = Refresh::calculate(&doc, &Refresh::default()).unwrap();
            assert_eq!(refresh.notice, None);
            refresh.apply(&mut doc);
            let labels = indicators(&doc);
            let [axis] = labels.as_slice() else {
                panic!("Expected one axis indicator: {labels:?}")
            };
            assert_eq!(axis.owner, Owner::BondStereo(1, 11));
            assert_eq!(axis.text, expected);
            doc.atom_labels.stereo = false;
            assert!(indicators(&doc).is_empty());
        }
    });
}
