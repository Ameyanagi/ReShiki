use super::*;
use crate::{
    document::Point,
    molecule_names::{IdentityRejection, canonical_identity},
};

pub(crate) fn pentadiene(display: &str) -> Document {
    let mut doc = Document::default();
    // Chemistry coordinates: axis left to right, far methyl up, near methyl
    // toward the viewer for a wedge. Viewed along +X, near methyl is on the
    // right (+Z), far methyl above (+Y): counterclockwise helicity M (Ra).
    for (element, x, y) in [
        ("C", -2., 1.),
        ("C", -1., 0.),
        ("C", 0., 0.),
        ("C", 1., 0.),
        ("C", 2., 1.),
        ("H", -2., -1.),
        ("H", 2., -1.),
    ] {
        doc.add_atom(element, Point::new(x * 28., -y * 28.));
    }
    for (a, b, order, style) in [
        (2, 1, 1, display),
        (2, 3, 2, "plain"),
        (3, 4, 2, "plain"),
        (4, 5, 1, "plain"),
        (2, 6, 1, "plain"),
        (4, 7, 1, "plain"),
    ] {
        doc.add_bond(a, b, order, style);
    }
    doc
}

fn labeled(doc: &Document) -> anyhow::Result<Document> {
    let molecule = prepare(doc)?;
    let drawing = for_drawing(&molecule, doc)?;
    let labels = drawing.labels()?;
    Ok(drawing.finish(labels)?)
}

#[test]
fn allene_wedge_and_hash_label_the_central_atom() -> anyhow::Result<()> {
    for (display, expected) in [("wedge", "M"), ("hash", "P")] {
        let doc = pentadiene(display);
        let molecule = prepare(&doc)?;
        assert_eq!(molecule.state.metadata.atoms[2].chiral_tag, 5);
        assert_eq!(
            canonical_identity(&molecule.state),
            Err(IdentityRejection::NonclassicalStereo)
        );
        let output = labeled(&doc)?;
        assert_eq!(output.atoms[2].cip_label.as_deref(), Some(expected));
        assert_eq!(output.bonds[0].display, display);
        let reopened: Document = serde_json::from_str(&serde_json::to_string(&output)?)?;
        assert_eq!(
            labeled(&reopened)?.atoms[2].cip_label.as_deref(),
            Some(expected)
        );
        assert!(crate::chemistry::stereo_lint::check(&doc, false).is_empty());
    }
    Ok(())
}

#[test]
fn allene_both_ends_wedged_are_ambiguous() -> anyhow::Result<()> {
    let mut doc = pentadiene("wedge");
    doc.bonds[3].display = "hash".into();
    assert_eq!(prepare(&doc)?.state.metadata.atoms[2].chiral_tag, 0);
    assert_eq!(labeled(&doc)?.atoms[2].cip_label, None);
    assert!(
        crate::chemistry::stereo_lint::check(&doc, false)
            .iter()
            .any(|note| note.message.contains("both ends"))
    );
    Ok(())
}

#[test]
fn allene_geometry_checks_both_substituents_independent_of_bond_order() -> anyhow::Result<()> {
    for (hydrogen, bonds) in [(5, [0, 4]), (6, [3, 5])] {
        for y in [-1., 0., 0.5] {
            let mut doc = pentadiene("wedge");
            doc.atoms[hydrogen].position.y = -y * 28.;
            for reordered in [false, true] {
                if reordered {
                    doc.bonds.swap(bonds[0], bonds[1]);
                }
                let output = labeled(&doc)?;
                assert_eq!(
                    output.atoms[2].cip_label.as_deref(),
                    (y < 0.).then_some("M"),
                    "hydrogen={hydrogen} y={y} reordered={reordered}"
                );
                assert_eq!(
                    prepare(&doc)?.state.metadata.atoms[2].chiral_tag,
                    if y < 0. { 5 } else { 0 }
                );
            }
        }
    }
    Ok(())
}

#[test]
fn allene_plain_or_reversed_wedges_do_not_specify_stereo() -> anyhow::Result<()> {
    let mut reversed = pentadiene("wedge");
    let bond = &mut reversed.bonds[0];
    std::mem::swap(&mut bond.a, &mut bond.b);
    for doc in [pentadiene("plain"), reversed] {
        assert_eq!(prepare(&doc)?.state.metadata.atoms[2].chiral_tag, 0);
        assert_eq!(labeled(&doc)?.atoms[2].cip_label, None);
    }
    Ok(())
}

#[test]
fn allene_implicit_hydrogens_and_opposite_end_wedges_agree() -> anyhow::Result<()> {
    let base = pentadiene("wedge");
    let mut implicit = base.clone();
    implicit.atoms.truncate(5);
    implicit.bonds.truncate(4);
    let mut far = base.clone();
    far.bonds[0].display = "plain".into();
    far.bonds[3].display = "hash".into();
    let mut hydrogen_wedge = base.clone();
    hydrogen_wedge.bonds[0].display = "plain".into();
    hydrogen_wedge.bonds[4].display = "hash".into();
    let mut paired = base.clone();
    paired.bonds[4].display = "hash".into();
    for doc in [base, implicit, far, hydrogen_wedge, paired] {
        assert_eq!(labeled(&doc)?.atoms[2].cip_label.as_deref(), Some("M"));
        assert!(crate::chemistry::stereo_lint::check(&doc, false).is_empty());
    }
    Ok(())
}

#[test]
fn allene_identical_substituents_and_removed_wedges_have_no_label() -> anyhow::Result<()> {
    let mut identical = pentadiene("wedge");
    identical.atoms[5].element = "C".into();
    assert_eq!(labeled(&identical)?.atoms[2].cip_label, None);
    let mut removed = labeled(&pentadiene("wedge"))?;
    removed.bonds[0].display = "plain".into();
    assert_eq!(labeled(&removed)?.atoms[2].cip_label, None);
    Ok(())
}

#[test]
fn allene_atom_refresh_displays_helicity_indicator() {
    use crate::atom_labels::{Owner, indicators, refresh::Refresh, test_support::with_fonts};
    with_fonts(|| {
        for (display, expected) in [("wedge", "(M)"), ("hash", "(P)")] {
            let mut doc = pentadiene(display);
            doc.drawing_style.font_family = "Liberation Sans".into();
            doc.atom_labels.stereo = true;
            for atom in &mut doc.atoms {
                atom.display.stereo.style.family = "Liberation Sans".into();
            }
            let refresh = Refresh::calculate(&doc, &Refresh::default()).unwrap();
            assert_eq!(refresh.notice, None);
            refresh.apply(&mut doc);
            let labels = indicators(&doc);
            let [axis] = labels.as_slice() else {
                panic!("Expected one allene indicator: {labels:?}");
            };
            assert_eq!(axis.owner, Owner::AtomStereo(3));
            assert_eq!(axis.text, expected);
        }
    });
}
