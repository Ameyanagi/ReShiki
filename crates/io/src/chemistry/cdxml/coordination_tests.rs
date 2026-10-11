use super::{assemble_cdxml, prepare_cdxml};
use crate::chemistry;
use crate::{
    bonds::BondPreset,
    document::{Document, Point},
    exchange::drawing,
    templates,
};

fn chelate() -> (Document, u64, Vec<u64>) {
    let mut doc = Document::default();
    let co = doc.add_atom("Co", Point::default());
    doc.atom_mut(co).unwrap().charge = 3;
    let mut donors = vec![];
    for angle in [0_f32, 120., 240.] {
        let point = |radius: f32, offset: f32| {
            let a = (angle + offset).to_radians();
            Point::new(radius * a.cos(), radius * a.sin())
        };
        let n = doc.add_atom("N", point(60., -25.));
        let c = doc.add_atom("C", point(96., -18.));
        let c2 = doc.add_atom("C", point(96., 18.));
        let n2 = doc.add_atom("N", point(60., 25.));
        for (a, b) in [(n, c), (c, c2), (c2, n2)] {
            doc.add_bond(a, b, 1, "plain");
        }
        donors.extend([n, n2]);
    }
    for donor in &donors {
        doc = templates::coordinate_atoms(&doc, *donor, co).unwrap();
    }
    (doc, co, donors)
}

#[test]
fn coordination_projection_cdxml_and_mol_keep_direction_charge_and_donor_hydrogens()
-> Result<(), Box<dyn std::error::Error>> {
    let (source, co, donors) = chelate();
    let identity = |doc: &Document| -> Result<String, Box<dyn std::error::Error>> {
        let m = chemistry::document::prepare(doc)?;
        for id in &donors {
            let index = m.ids.iter().position(|x| x == id).unwrap();
            assert_eq!(m.state.valences[index].implicit_hydrogens, 2);
            assert_eq!(m.state.metadata.atoms[index].chiral_tag, 0);
        }
        Ok(chemistry::smiles::write::write(&m.state, Default::default())?.text)
    };
    let expected = identity(&source)?;
    for (preset, narrow_at_metal) in [
        (BondPreset::Dative, false),
        (BondPreset::Dashed, false),
        (BondPreset::Wedge, false),
        (BondPreset::Wedge, true),
        (BondPreset::HashedWedge, false),
        (BondPreset::HashedWedge, true),
        (BondPreset::HollowWedge, false),
        (BondPreset::HollowWedge, true),
    ] {
        let mut doc = source.clone();
        for bond in doc.bonds.iter_mut().filter(|b| b.order == 5) {
            preset.apply(bond);
        }
        assert_eq!(identity(&doc)?, expected);
        if narrow_at_metal {
            for bond in doc.bonds.iter_mut().filter(|b| b.order == 5) {
                assert!(bond.reverse_projection());
            }
        }
        let xml = drawing::write(&doc, Default::default())?;
        assert_eq!(xml.matches("Order=\"dative\"").count(), 6);
        let (clipboard_xml, clipboard_notices) = drawing::write_clipboard(&doc)?;
        assert_eq!(clipboard_xml.matches("Order=\"dative\"").count(), 6);
        assert!(!clipboard_notices.iter().any(|n| n.contains("plain lines")));
        if doc.bonds.iter().any(|b| b.order == 5 && b.projection) {
            assert!(clipboard_notices.iter().any(|n| n.contains("ChemDraw 26")));
        }
        let clipboard = assemble_cdxml(&prepare_cdxml(&clipboard_xml)?)?.into_document()?;
        assert_eq!(
            clipboard
                .document
                .bonds
                .iter()
                .find(|b| b.order == 5)
                .unwrap()
                .display,
            doc.bonds.iter().find(|b| b.order == 5).unwrap().display
        );
        let cdx = crate::exchange::to_cdx(&xml)?;
        let decoded = crate::exchange::from_cdx(&cdx)?;
        let prepared = prepare_cdxml(&decoded)?;
        let imported = assemble_cdxml(&prepared)?.into_document()?;
        let received = &imported.document;
        assert_eq!(
            received
                .atoms
                .iter()
                .find(|a| a.element == "Co")
                .unwrap()
                .charge,
            3
        );
        assert_eq!(received.bonds.iter().filter(|b| b.order == 5).count(), 6);
        for bond in received.bonds.iter().filter(|b| b.order == 5) {
            assert_eq!(received.atom(bond.a).unwrap().element, "N");
            assert_eq!(received.atom(bond.b).unwrap().element, "Co");
            assert_eq!(
                bond.display,
                doc.bonds.iter().find(|b| b.order == 5).unwrap().display
            );
            assert_eq!(
                bond.projection,
                !matches!(bond.display.as_str(), "plain" | "dashed")
            );
            assert!(received.atom(bond.a).unwrap().stereo.is_none());
        }
        assert_eq!(
            chemistry::smiles::write::write(&imported.molecule.state, Default::default())?.text,
            expected
        );
        let mol = super::super::molfile::write_document(&doc)?;
        assert!(mol.contains("V3000"));
        let reparsed = super::super::molfile::read(&mol)?;
        assert_eq!(
            reparsed
                .molecule
                .state
                .graph
                .bonds
                .iter()
                .filter(|b| b.order == 5)
                .count(),
            6
        );
        for bond in reparsed
            .molecule
            .state
            .graph
            .bonds
            .iter()
            .filter(|b| b.order == 5)
        {
            assert_eq!(reparsed.molecule.state.graph.atoms[bond.a].atomic_number, 7);
            assert_eq!(
                reparsed.molecule.state.graph.atoms[bond.b].atomic_number,
                27
            );
            assert_eq!(reparsed.molecule.state.graph.atoms[bond.b].charge, 3);
        }
        assert_eq!(doc.atom(co).unwrap().charge, 3);
    }
    if let Some(dir) = std::env::var_os("RESHIKI_COORDINATION_FIXTURES") {
        let dir = std::path::PathBuf::from(dir);
        let mut projected = source.clone();
        for (i, bond) in projected
            .bonds
            .iter_mut()
            .filter(|b| b.order == 5)
            .enumerate()
        {
            match i {
                0 | 1 => BondPreset::Wedge.apply(bond),
                2 | 3 => BondPreset::HashedWedge.apply(bond),
                _ => {}
            }
            if matches!(i, 1 | 3) {
                assert!(bond.reverse_projection());
            }
        }
        let prepared = chemistry::document::prepare(&projected)?;
        for (&id, valence) in prepared.ids.iter().zip(&prepared.state.valences) {
            projected.atom_mut(id).unwrap().label_h = valence.implicit_hydrogens;
        }
        let xml = drawing::write(&projected, Default::default())?;
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("co-en3-both-tips.cdxml"), &xml)?;
        std::fs::write(
            dir.join("co-en3-both-tips.cdx"),
            crate::exchange::to_cdx(&xml)?,
        )?;
        std::fs::write(dir.join("co-en3-both-tips.rsk"), projected.file_json()?)?;
        std::fs::write(
            dir.join("co-en3-both-tips.svg"),
            crate::scene::svg(&projected),
        )?;
    }
    Ok(())
}

#[test]
fn explicit_external_end_projection_retains_donor_metal_direction()
-> Result<(), Box<dyn std::error::Error>> {
    for (display, retained) in [
        ("WedgeEnd", "wedge_end"),
        ("WedgedHashEnd", "hash_end"),
        ("HollowWedgeEnd", "hollow_wedge_end"),
    ] {
        let xml = format!(
            "<CDXML BondLength='14.4'><page id='1'><fragment id='2'><n id='3' p='0 0' Element='7'/><n id='4' p='40 0' Element='27' Charge='3'/><b id='5' B='3' E='4' Order='dative' Display='{display}'/></fragment></page></CDXML>"
        );
        let prepared = prepare_cdxml(&xml)?;
        let imported = assemble_cdxml(&prepared)?.into_document()?;
        let bond = imported.document.bonds.first().unwrap();
        assert_eq!(bond.display, retained);
        assert_eq!(imported.document.atom(bond.a).unwrap().element, "N");
        assert_eq!(imported.document.atom(bond.b).unwrap().element, "Co");
        assert_eq!(bond.order, 5);
        assert!(bond.projection);
        assert!(imported.document.atoms.iter().all(|a| a.stereo.is_none()));
    }
    Ok(())
}

#[test]
fn actual_chemdraw_roundtrip_retains_complete_complexes_but_omits_projection_paint()
-> Result<(), Box<dyn std::error::Error>> {
    let xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/coordination/co-en3-chemdraw-roundtrip.cdxml"
    ));
    let prepared = prepare_cdxml(xml)?;
    let imported = assemble_cdxml(&prepared)?.into_document()?;
    let doc = &imported.document;
    assert_eq!(doc.atoms.len(), 26);
    assert_eq!(doc.bonds.len(), 30);
    let metals: Vec<_> = doc.atoms.iter().filter(|a| a.element == "Co").collect();
    assert_eq!(metals.len(), 2);
    for metal in metals {
        assert_eq!(metal.charge, 3);
        assert_eq!(
            doc.bonds
                .iter()
                .filter(|b| b.order == 5 && b.b == metal.id)
                .count(),
            6
        );
    }
    assert_eq!(doc.bonds.iter().filter(|b| b.order == 1).count(), 18);
    for bond in doc.bonds.iter().filter(|b| b.order == 5) {
        let donor = doc.atom(bond.a).unwrap();
        assert_eq!(donor.element, "N");
        assert_eq!(doc.atom(bond.b).unwrap().element, "Co");
        assert_eq!(bond.display, "plain");
        assert!(!bond.projection);
        assert!(donor.stereo.is_none());
        let index = imported
            .molecule
            .ids
            .iter()
            .position(|id| *id == donor.id)
            .unwrap();
        assert_eq!(
            u32::from(imported.molecule.state.graph.atoms[index].explicit_hydrogens)
                + imported.molecule.state.valences[index].implicit_hydrogens,
            2
        );
    }
    Ok(())
}
