use anyhow::Context;
use reshiki::{
    bonds::BondPreset,
    document::{Document, Point},
    graphics::PathCommand,
    scene::{self, Primitive},
};

const APPEARANCES: [BondPreset; 7] = [
    BondPreset::Wedge,
    BondPreset::HashedWedge,
    BondPreset::HollowWedge,
    BondPreset::Bold,
    BondPreset::Hashed,
    BondPreset::Wavy,
    BondPreset::Single,
];

fn ring(size: u8, tilt: f32) -> Document {
    let mut doc = Document::default();
    let ids = reshiki::editing::ring(&mut doc, Point::default(), size, true, 0.);
    reshiki::projection::tilt(&mut doc, &ids, tilt, true);
    doc
}

fn curves(doc: &Document) -> Vec<Vec<PathCommand>> {
    scene::primitives(doc)
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Path { commands, .. }
                if commands.iter().any(|c| matches!(c, PathCommand::Cubic(..))) =>
            {
                Some(commands)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn aromatic_ring_appearance_keeps_bond_orders_and_its_projected_circle() -> anyhow::Result<()> {
    for size in [5, 6] {
        for tilt in [0., 70.] {
            let source = ring(size, tilt);
            let circle = curves(&source);
            assert_eq!(circle.len(), 1);
            for preset in APPEARANCES {
                let mut doc = source.clone();
                preset.apply(doc.bonds.first_mut().context("Missing ring bond")?);
                assert!(doc.bonds.iter().all(|b| b.order == 4), "{preset:?}");
                assert_eq!(doc.atoms, source.atoms, "{preset:?}");
                assert!(curves(&doc).contains(&circle[0]), "{preset:?}");
                doc.validate().map_err(anyhow::Error::msg)?;
                if preset != BondPreset::Single {
                    assert_eq!(BondPreset::of(&doc.bonds[0]), Some(preset));
                    assert!(doc.bonds[0].projection);
                }
                let restored: Document = serde_json::from_slice(&serde_json::to_vec(&doc)?)?;
                assert_eq!(restored, doc);
                assert_eq!(scene::svg(&restored), scene::svg(&doc));
            }
        }
    }
    Ok(())
}

#[test]
fn styling_and_reversing_aromatic_edges_does_not_change_chemical_identity() -> anyhow::Result<()> {
    let source = ring(6, 65.);
    let chemistry = serde_json::to_value(reshiki::chemistry::document::prepare(&source)?.state)?;
    for preset in APPEARANCES {
        let mut doc = source.clone();
        for bond in &mut doc.bonds {
            preset.apply(bond);
            bond.reverse();
        }
        let prepared = reshiki::chemistry::document::prepare(&doc)?;
        assert!(
            prepared
                .state
                .directions
                .iter()
                .all(|d| *d == reshiki::chemistry::kekulize::Direction::None)
        );
        // Endpoint order is a drawing choice; restore it for a graph-order comparison.
        for bond in &mut doc.bonds {
            bond.reverse();
        }
        assert_eq!(
            serde_json::to_value(reshiki::chemistry::document::prepare(&doc)?.state)?,
            chemistry
        );
    }
    Ok(())
}

#[test]
fn dragging_an_existing_aromatic_edge_uses_the_requested_direction_without_replacing_it()
-> anyhow::Result<()> {
    let source = ring(5, 70.);
    let bond = source.bonds.first().context("Missing edge")?;
    let (a, b) = (bond.a, bond.b);
    for preset in APPEARANCES {
        let mut doc = source.clone();
        preset.place(&mut doc, b, a);
        assert_eq!((doc.bonds[0].a, doc.bonds[0].b), (b, a));
        assert!(doc.bonds.iter().all(|b| b.order == 4));
        assert_eq!(doc.bonds.len(), source.bonds.len());
        assert_eq!(doc.atoms, source.atoms);
        BondPreset::Single.place(&mut doc, a, b);
        assert_eq!(
            doc, source,
            "Restoring plain appearance should restore the ring"
        );
    }
    Ok(())
}

#[test]
fn partial_ring_curves_survive_projected_wedge_styles() -> anyhow::Result<()> {
    let mut source = ring(6, 70.);
    let ids: Vec<_> = source.atoms.iter().take(3).map(|a| a.id).collect();
    reshiki::ring_arcs::toggle(&mut source, &ids).map_err(anyhow::Error::msg)?;
    let curve = curves(&source);
    assert_eq!(curve.len(), 1);
    for preset in APPEARANCES {
        let mut doc = source.clone();
        for bond in &mut doc.bonds {
            preset.apply(bond);
        }
        assert_eq!(doc.bonds.iter().filter(|b| b.ring_arc).count(), 2);
        assert!(curves(&doc).contains(&curve[0]), "{preset:?}");
        assert!(doc.bonds.iter().all(|b| b.order == 4));
    }
    Ok(())
}

#[test]
fn cp_star_dimer_wedges_keep_both_ellipses_ligands_and_contacts() -> anyhow::Result<()> {
    // Visual fixture: the metal/halide charge assignments are not validated here.
    let sketch: reshiki::assistant::sketch::Sketch =
        serde_json::from_str(include_str!("fixtures/assistant-cp-star-dimer.json"))?;
    let mut doc = sketch
        .render(&Default::default())
        .map_err(anyhow::Error::msg)?;
    let source = doc.clone();
    let circles = curves(&source);
    assert_eq!(circles.len(), 2);
    for bond in &mut doc.bonds {
        if bond.order == 4 {
            BondPreset::Wedge.apply(bond);
            bond.reverse();
        }
    }
    assert_eq!(curves(&doc), circles);
    assert_eq!(doc.atoms, source.atoms);
    assert_eq!(doc.bonds.iter().filter(|b| b.order == 4).count(), 10);
    assert_eq!(
        doc.bonds
            .iter()
            .filter(|b| b.order != 4)
            .collect::<Vec<_>>(),
        source
            .bonds
            .iter()
            .filter(|b| b.order != 4)
            .collect::<Vec<_>>()
    );
    doc.validate().map_err(anyhow::Error::msg)?;
    for format in ["svg", "png", "pdf"] {
        assert!(
            !reshiki::export::drawing(&doc, format)
                .map_err(anyhow::Error::msg)?
                .is_empty()
        );
    }
    assert!(
        !reshiki::export::clipboard_png(&doc)
            .map_err(anyhow::Error::msg)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn projected_styles_are_not_silently_exported_as_stereochemical_wedges() -> anyhow::Result<()> {
    for preset in APPEARANCES
        .into_iter()
        .filter(|p| !matches!(p, BondPreset::Single | BondPreset::Bold))
    {
        let mut doc = ring(6, 65.);
        preset.apply(doc.bonds.first_mut().context("Missing edge")?);
        let error = reshiki::exchange::drawing::write(&doc, Default::default())
            .err()
            .context("Expected explicit exchange limitation")?;
        assert!(
            error.to_string().contains("projected wedge styles"),
            "{error}"
        );
        BondPreset::Single.apply(doc.bonds.first_mut().context("Missing edge")?);
        let xml = reshiki::exchange::drawing::write(&doc, Default::default())?;
        let imported = reshiki::chemistry::cdxml::import_cdxml(&xml)?.document;
        assert_eq!(imported.bonds.iter().filter(|b| b.order == 4).count(), 6);
        assert!(imported.atoms.iter().all(|a| a.stereo.is_none()));
    }
    Ok(())
}

#[test]
fn explicit_order_changes_and_new_stereo_bonds_still_work() -> anyhow::Result<()> {
    for preset in [
        BondPreset::Double,
        BondPreset::Triple,
        BondPreset::Dashed,
        BondPreset::Dative,
    ] {
        let mut doc = ring(6, 65.);
        let bond = doc.bonds.first_mut().context("Missing edge")?;
        BondPreset::Wedge.apply(bond);
        preset.apply(bond);
        assert_eq!(bond.order, preset.parts().0);
        assert!(!bond.projection);
        assert!(curves(&doc).is_empty());
    }
    for preset in APPEARANCES {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        preset.place(&mut doc, a, b);
        let bond = doc.bonds.first().context("Missing new bond")?;
        assert_eq!(bond.order, 1);
        assert!(!bond.projection);
        assert_eq!(BondPreset::of(bond), Some(preset));
        doc.validate().map_err(anyhow::Error::msg)?;
    }
    Ok(())
}
