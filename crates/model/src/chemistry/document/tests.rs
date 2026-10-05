use super::*;
use crate::document::{AtomStereo, Point};
use anyhow::Context;

#[test]
fn winding_uses_neighbor_order_without_losing_parity() -> anyhow::Result<()> {
    assert_eq!(winding(&[1, 2, 3, 4], &[1, 2, 3, 4], true)?, 1);
    assert_eq!(winding(&[1, 2, 3, 4], &[2, 1, 3, 4], true)?, 2);
    assert_eq!(winding(&[1, 2, 3, 4], &[4, 1, 2, 3], false)?, 1);
    assert!(winding(&[1, 2], &[1, 1], true).is_err());
    assert!(winding(&[1, 2], &[1, 3], true).is_err());
    Ok(())
}

#[test]
fn rejected_drawings_are_atomic_and_keep_typed_causes() -> anyhow::Result<()> {
    let mut base = Document::default();
    let a = base.add_atom("C", Point::default());
    let b = base.add_atom("O", Point::new(28., 0.));
    base.add_bond(a, b, 1, "plain");
    let mut invalid = Vec::new();
    for kind in 0..9 {
        let mut doc = base.clone();
        let atom = doc.atoms.first_mut().context("Missing atom")?;
        match kind {
            0 => atom.element = "Xx".into(),
            1 => atom.charge = 128,
            2 => atom.isotope = 65536,
            3 => atom.explicit_h = 256,
            4 => atom.map_num = u32::MAX,
            5 => atom.position.x = f32::INFINITY,
            6 => {
                atom.stereo = Some(AtomStereo {
                    winding: "cw".into(),
                    neighbors: vec![999],
                })
            }
            7 => atom.id = b,
            _ => doc.bonds.first_mut().context("Missing bond")?.b = 999,
        }
        invalid.push(doc);
    }
    for doc in invalid {
        let before = doc.clone();
        assert!(matches!(prepare(&doc), Err(Error::Drawing(_))));
        assert_eq!(doc, before);
    }
    let mut valence = base.clone();
    valence
        .atoms
        .first_mut()
        .context("Missing atom")?
        .explicit_h = 5;
    assert!(matches!(
        prepare(&valence),
        Err(Error::Sanitization(sanitize::Error {
            stage: sanitize::Stage::Properties,
            ..
        }))
    ));
    let mut radical = base;
    let atom = radical.atoms.get_mut(1).context("Missing oxygen")?;
    atom.explicit_h = 1;
    atom.radical_electrons = 1;
    assert!(prepare(&radical).is_err());
    Ok(())
}

#[test]
fn hydrogen_bonds_require_a_bound_donor_and_compatible_acceptor() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let o = doc.add_atom("O", Point::default());
    let h = doc.add_atom("H", Point::new(28., 0.));
    let acceptor = doc.add_atom("N", Point::new(56., 0.));
    doc.add_bond(o, h, 1, "plain");
    doc.add_bond(h, acceptor, 0, "dotted");
    assert_eq!(
        prepare(&doc)?.state.graph.bonds.get(1).map(|b| b.order),
        Some(0)
    );
    let mut detached = doc.clone();
    detached.bonds.remove(0);
    assert!(prepare(&detached).is_err());
    let mut positive = doc.clone();
    positive
        .atoms
        .get_mut(2)
        .context("Missing acceptor")?
        .charge = 1;
    assert!(prepare(&positive).is_err());
    let bond = doc.bonds.get_mut(1).context("Missing hydrogen bond")?;
    std::mem::swap(&mut bond.a, &mut bond.b);
    assert!(prepare(&doc).is_err());
    Ok(())
}

#[test]
fn controls_must_belong_to_the_correct_bond_endpoints() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let a = doc.add_atom("F", Point::new(-14., 24.));
    let b = doc.add_atom("C", Point::default());
    let c = doc.add_atom("C", Point::new(28., 0.));
    let d = doc.add_atom("Cl", Point::new(42., -24.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 2, "plain");
    doc.add_bond(c, d, 1, "plain");
    let bond = doc.bonds.get_mut(1).context("Missing double bond")?;
    bond.stereo = Some("e".into());
    bond.stereo_atoms = vec![a, d];
    assert_eq!(
        prepare(&doc)?.state.metadata.bonds.get(1).map(|b| b.stereo),
        Some(3)
    );
    doc.bonds
        .get_mut(1)
        .context("Missing double bond")?
        .stereo_atoms = vec![d, a];
    let before = doc.clone();
    assert!(prepare(&doc).is_err());
    assert_eq!(doc, before);
    Ok(())
}

#[test]
fn large_drawing_validation_uses_indexed_endpoints_and_neighbors() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let id = doc.add_atom("C", Point::default());
    let prototype = doc.atoms.first().context("Missing prototype")?.clone();
    doc.atoms = (1..=20_000)
        .map(|id| crate::document::Atom {
            id,
            stereo: Some(AtomStereo {
                winding: "cw".into(),
                neighbors: [
                    id.checked_sub(1).filter(|&n| n > 0),
                    (id < 20_000).then_some(id + 1),
                ]
                .into_iter()
                .flatten()
                .collect(),
            }),
            ..prototype.clone()
        })
        .collect();
    doc.add_bond(id, id + 1, 1, "plain");
    let bond = doc.bonds.first().context("Missing prototype bond")?.clone();
    doc.bonds = (1..20_000)
        .map(|id| crate::document::Bond {
            a: id,
            b: id + 1,
            ..bond.clone()
        })
        .collect();
    let input = build(&doc)?;
    assert_eq!(input.graph.atoms.len(), 20_000);
    assert_eq!(input.metadata.atoms.len(), 20_000);
    doc.atoms.resize(100_001, prototype);
    assert!(matches!(build(&doc), Err(Error::Drawing(message)) if message.contains("limit")));
    Ok(())
}
