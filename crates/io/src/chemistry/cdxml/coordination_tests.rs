use super::{assemble_cdxml, prepare_cdxml};
use crate::chemistry;
use crate::{
    bonds::BondPreset,
    document::{Document, Point},
    exchange::drawing,
    joining,
};

fn covalent_frame() -> Document {
    let mut doc = Document::default();
    let metal = doc.add_atom("Pt", Point::default());
    for (element, point, display) in [
        ("F", Point::new(42., 0.), "wedge"),
        ("Cl", Point::new(0., 42.), "hash"),
        ("Br", Point::new(-42., 0.), "plain"),
        ("I", Point::new(0., -42.), "plain"),
    ] {
        let ligand = doc.add_atom(element, point);
        doc.add_bond(metal, ligand, 1, display);
        doc.bonds.last_mut().unwrap().projection = display != "plain";
    }
    doc
}

fn assert_frame(doc: &Document, expected: &Document) {
    assert!(doc.atoms.iter().all(|a| a.stereo.is_none()));
    assert_eq!(doc.bonds.len(), expected.bonds.len());
    for (bond, source) in doc.bonds.iter().zip(&expected.bonds) {
        assert_eq!(bond.order, 1);
        assert_eq!(bond.display, source.display);
        assert_eq!(bond.projection, source.projection);
        assert_eq!(
            doc.atom(bond.a).unwrap().element,
            expected.atom(source.a).unwrap().element
        );
        assert!(bond.stereo.is_none());
    }
}

#[tokio::test]
async fn placed_coordination_templates_export_and_analyze_without_tetrahedral_stereo()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::templates::{Connection, LIBRARY};

    let frames: Vec<_> = LIBRARY
        .iter()
        .filter(|template| template.group == "Coordination geometry")
        .collect();
    assert_eq!(frames.len(), 3);
    for template in frames {
        let (doc, _) = template.place(
            &Document::default(),
            Point::new(120., 84.),
            None,
            8.,
            template.anchor,
            Connection::Connect,
        )?;
        let molecule = chemistry::document::prepare(&doc)?;
        assert!(
            molecule
                .state
                .metadata
                .atoms
                .iter()
                .all(|a| a.chiral_tag == 0)
        );
        let smiles = chemistry::smiles::write::write(&molecule.state, Default::default())?.text;
        assert!(!smiles.contains('@'));
        let response = crate::engine::native_response::execute_with_config(
            crate::engine::Request::molecule("analyze", doc.clone()),
            Some(crate::engine::native_response::Config::in_process()),
        )
        .await?;
        assert_eq!(response.analysis.unwrap().smiles, smiles);
        assert!(
            response
                .document
                .unwrap()
                .atoms
                .iter()
                .all(|a| a.stereo.is_none())
        );
        let file = drawing::write(&doc, Default::default())?;
        let (clipboard, notices) = drawing::write_clipboard(&doc)?;
        assert!(!notices.iter().any(|n| n.contains("plain lines")));
        for xml in [file, clipboard] {
            assert_eq!(xml.matches("AS=\"N\"").count(), 1);
            assert_eq!(
                xml.matches("Display=\"WedgeBegin\"").count(),
                doc.bonds.iter().filter(|b| b.display == "wedge").count()
            );
            assert_eq!(
                xml.matches("Display=\"WedgedHashBegin\"").count(),
                doc.bonds.iter().filter(|b| b.display == "hash").count()
            );
            for decoded in [
                xml.clone(),
                crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml)?)?,
            ] {
                let imported = assemble_cdxml(&prepare_cdxml(&decoded)?)?.into_document()?;
                assert_frame(&imported.document, &doc);
                assert_eq!(
                    chemistry::smiles::write::write(&imported.molecule.state, Default::default())?
                        .text,
                    smiles
                );
            }
        }
        let mol = super::super::molfile::write_document(&doc)?;
        let imported = super::super::molfile::read(&mol)?;
        assert!(
            imported
                .molecule
                .state
                .metadata
                .atoms
                .iter()
                .all(|a| a.chiral_tag == 0)
        );
        assert_eq!(
            chemistry::smiles::write::write(&imported.molecule.state, Default::default())?.text,
            smiles
        );
    }
    Ok(())
}

#[test]
fn ordinary_metal_wedges_keep_captured_and_drawn_stereo_in_editable_interchange()
-> Result<(), Box<dyn std::error::Error>> {
    let mut source = covalent_frame();
    for bond in &mut source.bonds {
        bond.projection = false;
    }
    source.bonds[1].display = "plain".into();
    for display in ["wedge", "hash", "hollow_wedge", "bold", "hashed"] {
        let mut drawn = source.clone();
        drawn.bonds[0].display = display.into();
        let expected = chemistry::document::prepare(&drawn)?;
        let tag = expected.state.metadata.atoms[0].chiral_tag;
        assert!(matches!(tag, 1 | 2));
        let smiles = chemistry::smiles::write::write(&expected.state, Default::default())?.text;
        assert!(smiles.contains('@'));
        for captured in [false, true] {
            let mut doc = drawn.clone();
            if captured {
                doc.atoms[0].stereo = Some(crate::document::AtomStereo {
                    winding: if tag == 1 { "cw" } else { "ccw" }.into(),
                    neighbors: doc.bonds.iter().map(|b| b.b).collect(),
                });
            }
            let before = doc.clone();
            let file = drawing::write(&doc, Default::default())?;
            let (clipboard, _) = drawing::write_clipboard(&doc)?;
            for xml in [file, clipboard] {
                for decoded in [
                    xml.clone(),
                    crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml)?)?,
                ] {
                    let imported = assemble_cdxml(&prepare_cdxml(&decoded)?)?.into_document()?;
                    assert!(imported.document.atoms[0].stereo.is_some());
                    assert!(imported.document.bonds.iter().all(|b| !b.projection));
                    assert_eq!(
                        chemistry::smiles::write::write(
                            &imported.molecule.state,
                            Default::default()
                        )?
                        .text,
                        smiles
                    );
                }
                assert!(!xml.contains("AS=\"N\""));
            }
            assert_eq!(doc, before);
        }
    }
    Ok(())
}

fn ordinary_metal_xml(stereo: &str) -> String {
    format!(
        "<CDXML BondLength='14.4'><page id='1'><fragment id='2'><n id='3' p='0 0' Element='78' {stereo}/><n id='4' p='40 0' Element='9'/><n id='5' p='0 40' Element='17'/><n id='6' p='-40 0' Element='35'/><n id='7' p='0 -40' Element='53'/><b id='8' B='3' E='4' Order='1' Display='WedgeBegin'/><b id='9' B='3' E='5' Order='1'/><b id='10' B='3' E='6' Order='1'/><b id='11' B='3' E='7' Order='1'/></fragment></page></CDXML>"
    )
}

#[test]
fn metal_wedges_without_a_no_stereocentre_marker_stay_tetrahedral()
-> Result<(), Box<dyn std::error::Error>> {
    for stereo in ["", "AS='R'", "AS='U'"] {
        let xml = ordinary_metal_xml(stereo);
        for decoded in [
            xml.clone(),
            crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml)?)?,
        ] {
            let prepared = prepare_cdxml(&decoded)?;
            assert!(matches!(
                prepared.molecule.state.metadata.atoms[0].chiral_tag,
                1 | 2
            ));
            let imported = assemble_cdxml(&prepared)?.into_document()?;
            assert!(imported.document.atoms[0].stereo.is_some());
            assert!(imported.document.bonds.iter().all(|b| !b.projection));
        }
    }
    Ok(())
}

#[test]
fn covalent_coordination_frames_keep_projection_in_editable_interchange()
-> Result<(), Box<dyn std::error::Error>> {
    let doc = covalent_frame();
    let before = doc.clone();
    let expected = chemistry::document::prepare(&doc)?;
    assert!(
        expected
            .state
            .metadata
            .atoms
            .iter()
            .all(|a| a.chiral_tag == 0)
    );
    let smiles = chemistry::smiles::write::write(&expected.state, Default::default())?.text;
    assert!(!smiles.contains('@'));
    let file = drawing::write(&doc, Default::default())?;
    let (clipboard, notices) = drawing::write_clipboard(&doc)?;
    assert!(!notices.iter().any(|n| n.contains("plain lines")));
    for xml in [file, clipboard] {
        assert_eq!(xml.matches("AS=\"N\"").count(), 1);
        assert!(xml.contains("Order=\"1\" Display=\"WedgeBegin\""));
        assert!(xml.contains("Order=\"1\" Display=\"WedgedHashBegin\""));
        for decoded in [
            xml.clone(),
            crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml)?)?,
        ] {
            let prepared = prepare_cdxml(&decoded)?;
            assert!(
                prepared
                    .molecule
                    .state
                    .metadata
                    .atoms
                    .iter()
                    .all(|a| a.chiral_tag == 0)
            );
            let imported = assemble_cdxml(&prepared)?.into_document()?;
            assert_frame(&imported.document, &doc);
            assert_eq!(
                chemistry::smiles::write::write(&imported.molecule.state, Default::default())?.text,
                smiles
            );
        }
    }
    let mol = super::super::molfile::write_document(&doc)?;
    let reparsed = super::super::molfile::read(&mol)?;
    assert!(
        reparsed
            .molecule
            .state
            .metadata
            .atoms
            .iter()
            .all(|a| a.chiral_tag == 0)
    );
    assert_eq!(
        chemistry::smiles::write::write(&reparsed.molecule.state, Default::default())?.text,
        smiles
    );
    assert_eq!(doc, before);
    Ok(())
}

#[test]
fn covalent_frame_export_exception_requires_an_unassigned_metal_at_the_narrow_end()
-> Result<(), Box<dyn std::error::Error>> {
    for display in ["wedge", "hash", "hollow_wedge", "bold", "hashed"] {
        let mut doc = covalent_frame();
        doc.bonds[0].display = display.into();
        for boundary in ["nonmetal", "wide_metal"] {
            let mut source = doc.clone();
            if boundary == "nonmetal" {
                source.atoms[0].element = "C".into();
            } else {
                let bond = &mut source.bonds[0];
                std::mem::swap(&mut bond.a, &mut bond.b);
            }
            let error = drawing::write(&source, Default::default()).unwrap_err();
            assert!(
                error.to_string().contains("projected wedge styles"),
                "{error}"
            );
            let (clipboard, notices) = drawing::write_clipboard(&source)?;
            assert!(notices.iter().any(|n| n.contains("plain lines")));
            let imported = assemble_cdxml(&prepare_cdxml(&clipboard)?)?.into_document()?;
            assert_eq!(imported.document.bonds[0].display, "plain");
        }
    }
    let mut doc = covalent_frame();
    doc.atoms[0].stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: doc.bonds.iter().map(|b| b.b).collect(),
    });
    let error = drawing::write(&doc, Default::default()).unwrap_err();
    assert!(
        error.to_string().contains("projected wedge styles"),
        "{error}"
    );
    let error = drawing::write_clipboard(&doc).unwrap_err();
    assert!(
        error.to_string().contains("assigned stereocenter"),
        "{error}"
    );
    Ok(())
}

#[test]
fn metal_frame_normalization_preserves_other_equivalent_stereo_displays()
-> Result<(), Box<dyn std::error::Error>> {
    for (display, expected, ordinary) in [
        ("WedgeBegin", "Solid", "WedgeBegin"),
        ("WedgedHashBegin", "Solid", "WedgedHashBegin"),
        ("HollowWedgeBegin", "WedgeBegin", "WedgeBegin"),
        ("Bold", "WedgeBegin", "WedgeBegin"),
        ("Hash", "WedgedHashBegin", "WedgedHashBegin"),
    ] {
        let xml = format!(
            "<CDXML><page><fragment><n id='3' Element='78' AS='N'/><n id='4' Element='9'/><b B='3' E='4' Order='1' Display='{display}'/></fragment><fragment><n id='3' Element='6' AS='N'/><n id='4' Element='9'/><b B='3' E='4' Order='1' Display='{display}'/></fragment><fragment><n id='3' Element='78' AS='N'/><n id='4' Element='9'/><b B='4' E='3' Order='1' Display='{display}'/></fragment></page></CDXML>"
        );
        let chemical = super::chemistry_xml(&xml)?;
        let parsed = roxmltree::Document::parse(&chemical)?;
        let displays: Vec<_> = parsed
            .descendants()
            .filter(|n| n.has_tag_name("b"))
            .map(|n| n.attribute("Display").unwrap())
            .collect();
        assert_eq!(displays, [expected, ordinary, ordinary]);
    }
    Ok(())
}

#[test]
fn external_covalent_metal_wedges_restore_projection_at_either_xml_endpoint()
-> Result<(), Box<dyn std::error::Error>> {
    for (display, begin, end, retained) in [
        ("WedgeBegin", 3, 4, "wedge"),
        ("WedgeEnd", 4, 3, "wedge"),
        ("WedgedHashBegin", 3, 4, "hash"),
        ("WedgedHashEnd", 4, 3, "hash"),
    ] {
        let xml = format!(
            "<CDXML BondLength='14.4'><page id='1'><fragment id='2'><n id='3' p='0 0' Element='78' AS='N'/><n id='4' p='40 0' Element='9'/><n id='5' p='0 40' Element='17'/><n id='6' p='-40 0' Element='35'/><n id='7' p='0 -40' Element='53'/><b id='8' B='{begin}' E='{end}' Order='1' Display='{display}'/><b id='9' B='3' E='5' Order='1'/><b id='10' B='3' E='6' Order='1'/><b id='11' B='3' E='7' Order='1'/></fragment></page></CDXML>"
        );
        for decoded in [
            xml.clone(),
            crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml)?)?,
        ] {
            let prepared = prepare_cdxml(&decoded)?;
            assert!(
                prepared
                    .molecule
                    .state
                    .metadata
                    .atoms
                    .iter()
                    .all(|a| a.chiral_tag == 0)
            );
            let imported = assemble_cdxml(&prepared)?.into_document()?;
            let bond = imported.document.bonds.first().unwrap();
            assert_eq!(bond.order, 1);
            assert_eq!(bond.display, retained);
            assert!(bond.projection, "{display}");
            assert_eq!(imported.document.atom(bond.a).unwrap().element, "Pt");
            assert!(imported.document.atoms.iter().all(|a| a.stereo.is_none()));
        }
    }
    Ok(())
}

#[test]
fn frame_projection_ignores_ligand_stereo_and_never_masks_ordinary_metal_wedges()
-> Result<(), Box<dyn std::error::Error>> {
    // A chiral ligand carbon at the wide end keeps the metal's wedge projected.
    let mut doc = covalent_frame();
    let carbon = doc.bonds[0].b;
    doc.atom_mut(carbon).unwrap().element = "C".into();
    let position = doc.atom(carbon).unwrap().position;
    let mut neighbors = vec![doc.bonds[0].a];
    for (element, dx, dy) in [("F", 36., -21.), ("Cl", 36., 21.), ("Br", 0., 42.)] {
        let id = doc.add_atom(element, Point::new(position.x + dx, position.y + dy));
        doc.add_bond(carbon, id, 1, "plain");
        neighbors.push(id);
    }
    doc.atom_mut(carbon).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors,
    });
    let xml = drawing::write(&doc, Default::default())?;
    assert_eq!(xml.matches("AS=\"N\"").count(), 1);
    let imported = assemble_cdxml(&prepare_cdxml(&xml)?)?.into_document()?;
    assert!(imported.document.atoms[0].stereo.is_none());
    assert!(imported.document.bonds[..2].iter().all(|b| b.projection));

    // An ordinary wedge beside a projected hash keeps the metal unflagged.
    let mut doc = covalent_frame();
    doc.bonds[0].projection = false;
    let expected = chemistry::document::prepare(&doc)?;
    assert!(matches!(expected.state.metadata.atoms[0].chiral_tag, 1 | 2));
    let xml = drawing::write(&doc, Default::default())?;
    assert!(!xml.contains("AS=\"N\""));
    let imported = assemble_cdxml(&prepare_cdxml(&xml)?)?.into_document()?;
    assert!(imported.document.atoms[0].stereo.is_some());
    Ok(())
}

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
        doc = joining::coordinate_atoms(&doc, *donor, co)
            .unwrap()
            .into_document(&doc);
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
