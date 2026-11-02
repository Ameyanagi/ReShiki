use super::*;
use crate::{
    abbreviations::{Abbreviation as DrawingAbbreviation, LabelAlignment},
    document::Document,
    engine::{LocalEngine, Request},
};

// A reduced ChemDraw multi-attachment nickname, using the same explicit
// ConnectionOrder/BondOrdering pairing as atom-to-fragment.cdxml. Both node
// and external-bond storage order differ from the specified attachment order.
const TWO_TARGET_NICKNAME: &str = r#"<CDXML BondLength="30"><page><fragment id="1">
<n id="2" p="-30 0"/>
<n id="3" p="0 0" NodeType="Nickname" BondOrdering="13 14">
  <fragment id="30" ConnectionOrder="9 10">
    <n id="4" p="0 0" Element="7"/>
    <n id="5" p="30 0"/><n id="6" p="30 -30"/>
    <n id="7" p="60 0"/><n id="8" p="60 -30" Element="8"/>
    <n id="10" p="90 0" NodeType="ExternalConnectionPoint" ExternalConnectionNum="2"/>
    <n id="9" p="-30 0" NodeType="ExternalConnectionPoint" ExternalConnectionNum="1"/>
    <b id="15" B="4" E="5"/><b id="16" B="5" E="6"/>
    <b id="17" B="5" E="7"/><b id="18" B="7" E="8" Order="2"/>
    <b id="19" B="9" E="4"/><b id="20" B="10" E="7"/>
  </fragment><t><s>Ala</s></t>
</n>
<n id="11" p="90 0" Element="8"/><n id="12" p="120 0"/>
<b id="14" B="3" E="11"/><b id="13" B="2" E="3"/>
<b id="21" B="11" E="12"/>
</fragment></page></CDXML>"#;

fn neighbors(document: &Document, id: u64) -> Vec<u64> {
    document
        .bonds
        .iter()
        .filter_map(|bond| {
            if bond.a == id {
                Some(bond.b)
            } else if bond.b == id {
                Some(bond.a)
            } else {
                None
            }
        })
        .collect()
}

fn carbonyl_oxygen(document: &Document, id: u64) -> Option<u64> {
    document.bonds.iter().find_map(|bond| {
        let other = if bond.a == id {
            bond.b
        } else if bond.b == id {
            bond.a
        } else {
            return None;
        };
        (bond.order == 2 && document.atom(other).unwrap().element == "O").then_some(other)
    })
}

fn assert_residue_boundaries(document: &Document) {
    let [group] = document.abbreviations.as_slice() else {
        panic!("Expected one contracted alanine residue");
    };
    assert_eq!(group.label, "Ala");
    assert_eq!(document.atom(group.anchor).unwrap().element, "N");
    let exit = group.exit.unwrap();
    assert_eq!(document.atom(exit).unwrap().element, "C");
    assert!(carbonyl_oxygen(document, exit).is_some());
    assert_eq!(group.members.len(), 5);
    for attachment in [group.anchor, exit] {
        assert_eq!(
            neighbors(document, attachment)
                .into_iter()
                .filter(|id| !group.members.contains(id))
                .count(),
            1
        );
    }
    document.validate().unwrap();
}

#[test]
fn chemdraw_two_target_nickname_imports_in_explicit_connection_order() {
    for source in [
        TWO_TARGET_NICKNAME.to_owned(),
        crate::exchange::from_cdx(&crate::exchange::to_cdx(TWO_TARGET_NICKNAME).unwrap()).unwrap(),
    ] {
        let document = import_cdxml(&source).unwrap().document;
        assert_residue_boundaries(&document);
        assert_eq!(document.atoms.len(), 8);
        assert_eq!(document.bonds.len(), 7);
        let carbonyl = document.abbreviations[0].exit.unwrap();
        let outside = neighbors(&document, carbonyl)
            .into_iter()
            .find(|id| !document.abbreviations[0].members.contains(id))
            .unwrap();
        assert_eq!(document.atom(outside).unwrap().element, "O");
        let smiles = crate::chemistry::smiles::write::write(
            &crate::chemistry::document::prepare(&document)
                .unwrap()
                .state,
            Default::default(),
        )
        .unwrap()
        .text;
        let expected = crate::chemistry::smiles::write::write(
            &crate::chemistry::smiles::prepare("CNC(C)C(=O)OC")
                .unwrap()
                .state,
            Default::default(),
        )
        .unwrap()
        .text;
        assert_eq!(smiles, expected);
    }
}

#[test]
fn copying_a_residue_without_both_neighbors_expands_for_editable_exchange() {
    let source = import_cdxml(TWO_TARGET_NICKNAME).unwrap().document;
    let group = &source.abbreviations[0];
    for retained_attachment in [None, Some(group.anchor), group.exit] {
        let mut selected = group.members.clone();
        if let Some(attachment) = retained_attachment {
            selected.extend(
                neighbors(&source, attachment)
                    .into_iter()
                    .filter(|id| !group.members.contains(id)),
            );
        }
        let part = crate::editing::selection(&source, &selected);
        part.validate().unwrap();
        assert_eq!(part.abbreviations.len(), 1);
        assert_eq!(part.abbreviations[0].exit, group.exit);
        let expected = crate::chemistry::smiles::write::write(
            &crate::chemistry::document::prepare(&part).unwrap().state,
            Default::default(),
        )
        .unwrap()
        .text;
        let xml = crate::exchange::drawing::write(&part, Default::default()).unwrap();
        for xml in [
            xml.clone(),
            crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap(),
        ] {
            let reopened = import_cdxml(&xml).unwrap().document;
            assert!(reopened.abbreviations.is_empty());
            assert_eq!(reopened.atoms.len(), part.atoms.len());
            assert_eq!(reopened.bonds.len(), part.bonds.len());
            let actual = crate::chemistry::smiles::write::write(
                &crate::chemistry::document::prepare(&reopened)
                    .unwrap()
                    .state,
                Default::default(),
            )
            .unwrap()
            .text;
            assert_eq!(actual, expected);
        }
        // Export operates on a detached view; the native copy keeps its exit.
        assert_eq!(part.abbreviations[0].exit, group.exit);
    }
}

fn assert_exported_attachment_order(xml: &str) {
    let tree = roxmltree::Document::parse(xml).unwrap();
    let wrapper = tree
        .descendants()
        .find(|node| node.has_tag_name("n") && node.attribute("NodeType") == Some("Nickname"))
        .unwrap();
    let inner = wrapper
        .children()
        .find(|node| node.has_tag_name("fragment"))
        .unwrap();
    let connections: Vec<_> = inner
        .attribute("ConnectionOrder")
        .unwrap()
        .split_whitespace()
        .collect();
    let bonds: Vec<_> = wrapper
        .attribute("BondOrdering")
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(connections.len(), 2);
    assert_eq!(bonds.len(), 2);
    for (connection, expected_element) in connections.iter().zip(["7", "6"]) {
        let bond = inner
            .children()
            .find(|node| {
                node.has_tag_name("b")
                    && (node.attribute("B") == Some(*connection)
                        || node.attribute("E") == Some(*connection))
            })
            .unwrap();
        let target = bond
            .attribute(if bond.attribute("B") == Some(*connection) {
                "E"
            } else {
                "B"
            })
            .unwrap();
        let atom = inner
            .children()
            .find(|node| node.has_tag_name("n") && node.attribute("id") == Some(target))
            .unwrap();
        assert_eq!(atom.attribute("Element").unwrap_or("6"), expected_element);
    }
    for bond in bonds {
        let outside = tree
            .descendants()
            .find(|node| node.has_tag_name("b") && node.attribute("id") == Some(bond))
            .unwrap();
        assert!(
            outside.attribute("B") == wrapper.attribute("id")
                || outside.attribute("E") == wrapper.attribute("id")
        );
    }
}

#[tokio::test]
async fn boc_ala_ome_keeps_both_attachments_and_chemistry_across_formats() {
    let engine = LocalEngine::in_process();
    let mut expanded = engine
        .request(Request::import_smiles("C[C@H](NC(=O)OC(C)(C)C)C(=O)OC"))
        .await
        .unwrap()
        .document
        .unwrap();
    // Reverse storage so export cannot accidentally pass by using bond order.
    expanded.bonds.reverse();
    let nitrogen = expanded
        .atoms
        .iter()
        .find(|atom| atom.element == "N")
        .unwrap()
        .id;
    let alpha = neighbors(&expanded, nitrogen)
        .into_iter()
        .find(|&id| carbonyl_oxygen(&expanded, id).is_none())
        .unwrap();
    let carbonyl = neighbors(&expanded, alpha)
        .into_iter()
        .find(|&id| carbonyl_oxygen(&expanded, id).is_some())
        .unwrap();
    let oxygen = carbonyl_oxygen(&expanded, carbonyl).unwrap();
    let methyl = neighbors(&expanded, alpha)
        .into_iter()
        .find(|&id| id != nitrogen && id != carbonyl)
        .unwrap();
    let mut collapsed = expanded.clone();
    collapsed.abbreviations.push(DrawingAbbreviation {
        label: "Ala".into(),
        reverse_label: String::new(),
        anchor: nitrogen,
        exit: Some(carbonyl),
        members: vec![nitrogen, alpha, methyl, carbonyl, oxygen],
        alignment: LabelAlignment::Auto,
        label_style: None,
        label_color_override: false,
        highlight: None,
    });
    collapsed.require_version(crate::document::RESIDUE_GROUPS);
    assert_residue_boundaries(&collapsed);

    for format in ["smiles", "mol", "inchi"] {
        let expected = crate::formats::write_with_engine(expanded.clone(), format, &engine)
            .await
            .unwrap();
        let actual = crate::formats::write_with_engine(collapsed.clone(), format, &engine)
            .await
            .unwrap();
        assert_eq!(actual, expected, "collapsed {format} chemistry");
    }
    let expected = engine
        .request(Request::molecule("analyze", expanded))
        .await
        .unwrap()
        .analysis
        .unwrap();
    assert_eq!(expected.formula, "C9H17NO4");
    for format in ["rsk", "cdxml", "cdx"] {
        let bytes = crate::formats::write_with_engine(collapsed.clone(), format, &engine)
            .await
            .unwrap();
        if format != "rsk" {
            let xml = if format == "cdx" {
                crate::exchange::from_cdx(&bytes).unwrap()
            } else {
                String::from_utf8(bytes.clone()).unwrap()
            };
            assert_exported_attachment_order(&xml);
        }
        let reopened = crate::formats::read_with_engine(bytes, format, &engine)
            .await
            .unwrap();
        assert_residue_boundaries(&reopened);
        if format == "rsk" {
            assert_eq!(reopened.abbreviations, collapsed.abbreviations);
        }
        let analysis = engine
            .request(Request::molecule("analyze", reopened))
            .await
            .unwrap()
            .analysis
            .unwrap();
        assert_eq!(
            analysis.smiles, expected.smiles,
            "{format} stereochemical SMILES"
        );
        assert_eq!(analysis.inchi, expected.inchi, "{format} InChI");
        assert_eq!(analysis.formula, expected.formula, "{format} formula");
    }
}

#[tokio::test]
async fn collapsed_boc_ala_gly_ome_preserves_every_atom_position_across_exchange() {
    use crate::chemistry::{abbreviations, document};

    let engine = LocalEngine::in_process();
    let mut source = engine
        .request(Request::import_smiles(
            "C[C@H](NC(=O)OC(C)(C)C)C(=O)NCC(=O)OC",
        ))
        .await
        .unwrap()
        .document
        .unwrap();
    source.bonds.reverse();
    for label in ["Boc", "Ala", "Gly", "OMe"] {
        source = abbreviations::find_with_policy(
            &source,
            &document::prepare(&source).unwrap(),
            &[],
            Some(label),
            abbreviations::AttachmentPolicy::SharedAnchor,
        )
        .unwrap();
        assert!(
            source
                .abbreviations
                .iter()
                .any(|group| group.label == label)
        );
    }
    assert_eq!(source.abbreviations.len(), 4);
    source.validate().unwrap();

    for format in ["cdxml", "cdx"] {
        let bytes = crate::formats::write_with_engine(source.clone(), format, &engine)
            .await
            .unwrap();
        let xml = if format == "cdx" {
            crate::exchange::from_cdx(&bytes).unwrap()
        } else {
            String::from_utf8(bytes.clone()).unwrap()
        };
        // Export assigns atom XML IDs from their source storage indices;
        // preparation retains those IDs even when wrapper flattening reorders atoms.
        let prepared = prepare_cdxml(&xml).unwrap();
        let original_indices: Vec<_> = prepared
            .fragments
            .iter()
            .flat_map(|fragment| &fragment.atom_ids)
            .map(|id| (id - 3) as usize)
            .collect();
        let reopened = crate::formats::read_with_engine(bytes, format, &engine)
            .await
            .unwrap();
        assert_eq!(reopened.abbreviations.len(), 4, "{format}");
        assert_eq!(reopened.atoms.len(), source.atoms.len(), "{format}");
        assert_eq!(original_indices.len(), source.atoms.len(), "{format}");
        let before_origin = source.atoms[original_indices[0]].position;
        let after_origin = reopened.atoms[0].position;
        for (&index, after) in original_indices.iter().zip(&reopened.atoms) {
            let before = &source.atoms[index];
            assert_eq!(after.element, before.element, "{format}: atom {index}");
            // Editable export adds a shared page-margin translation.
            let dx = (before.position.x - before_origin.x) - (after.position.x - after_origin.x);
            let dy = (before.position.y - before_origin.y) - (after.position.y - after_origin.y);
            assert!(
                dx.hypot(dy) < 0.003,
                "{format}: atom {index} moved by ({dx}, {dy})"
            );
        }
    }
}
