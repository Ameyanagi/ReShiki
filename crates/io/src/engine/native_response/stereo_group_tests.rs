use super::*;
use crate::chemistry::{cdxml, depict};
use crate::stereo_groups::{Kind, StereoGroup};

const DISCONNECTED_V3000: &str =
    include_str!("../../../../../tests/fixtures/stereo-groups/disconnected-and.mol");

const TWO_FRAGMENT_CDXML: &str =
    include_str!("../../../../../tests/fixtures/stereo-groups/two-fragments-and.cdxml");

fn drawing(text: &str) -> anyhow::Result<Document> {
    let input = smiles::read(text)?;
    let state = input.prepared.state;
    let positions = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    let molecule = molecular::Molecule {
        rdkit_version: chemistry::RDKIT_VERSION,
        ids: (1..=positions.len() as u64).collect(),
        positions,
        state,
    };
    let draft = molecular::for_import(&molecule, false, &input.prepared.dummy_labels)?;
    let labels = draft.labels()?;
    Ok(draft.finish(labels)?)
}

fn groups(doc: &Document) -> Vec<Option<crate::stereo_groups::StereoGroup>> {
    doc.atoms.iter().map(|atom| atom.stereo_group).collect()
}

async fn disconnected_roundtrips(
    doc: &Document,
    expected: &[(StereoGroup, Vec<usize>)],
) -> anyhow::Result<()> {
    let molecule = molecular::prepare(doc)?;
    let collections: Vec<_> = molecule
        .state
        .metadata
        .groups
        .iter()
        .map(|group| (group.kind, group.read_id, group.atoms.clone()))
        .collect();
    let expected_collections: Vec<_> = expected
        .iter()
        .map(|(group, atoms)| {
            (
                match group.kind {
                    Kind::And => 2,
                    Kind::Or => 1,
                },
                group.number,
                atoms.clone(),
            )
        })
        .collect();
    assert_eq!(collections, expected_collections);
    let identifiers = prepare_identifiers(&molecule, false)?.0;
    let atom_order = smiles::write::write(&molecule.state, Default::default())?.atom_order;
    for format in ["smiles", "mol", "cdxml", "cdx"] {
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let response = execute(request).await?;
        assert!(response.warnings.is_empty(), "{format}");
        let output = response.output.unwrap();
        let restored = match format {
            "smiles" => {
                let fields = expected
                    .iter()
                    .map(|(group, atoms)| {
                        let mut indices = atoms
                            .iter()
                            .map(|atom| atom_order.iter().position(|index| index == atom).unwrap())
                            .collect::<Vec<_>>();
                        indices.sort_unstable();
                        let indices = indices
                            .into_iter()
                            .map(|index| index.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        let kind = if group.kind == Kind::And { "&" } else { "o" };
                        format!("{kind}{}:{indices}", group.number)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    output.split_once(" |").unwrap().1,
                    format!("{}|", fields.join(","))
                );
                drawing(&output)?
            }
            "mol" => {
                assert!(output.contains("V3000"));
                assert_eq!(output.matches("MDLV30/STER").count(), expected.len());
                for (group, atoms) in expected {
                    let kind = if group.kind == Kind::And {
                        "RAC"
                    } else {
                        "REL"
                    };
                    let members = atoms
                        .iter()
                        .map(|index| (index + 1).to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let record = format!(
                        "M  V30 MDLV30/STE{kind}{} ATOMS=({} {members})\n",
                        group.number,
                        atoms.len()
                    );
                    assert_eq!(output.matches(record.as_str()).count(), 1, "{record}");
                }
                molfile::read(&output)?.display_document()?.0
            }
            _ => {
                let xml = if format == "cdx" {
                    exchange::from_cdx(&STANDARD.decode(output)?).map_err(anyhow::Error::msg)?
                } else {
                    output
                };
                let parsed = roxmltree::Document::parse(&xml)?;
                for (group, atoms) in expected {
                    let kind = if group.kind == Kind::And { "And" } else { "Or" };
                    let number = group.number.to_string();
                    let members = parsed
                        .descendants()
                        .filter(|node| {
                            node.has_tag_name("n")
                                && node.attribute("EnhancedStereoType") == Some(kind)
                                && node.attribute("EnhancedStereoGroupNum") == Some(number.as_str())
                        })
                        .map(|node| node.attribute("id").unwrap().parse::<usize>())
                        .collect::<Result<Vec<_>, _>>()?;
                    assert_eq!(
                        members,
                        atoms.iter().map(|index| index + 3).collect::<Vec<_>>(),
                        "{format}"
                    );
                }
                let imported = cdxml::import_cdxml(&xml)?;
                assert!(imported.notices.is_empty());
                imported.document
            }
        };
        let prepared = molecular::prepare(&restored)?;
        assert_eq!(
            prepared.state.metadata.groups.len(),
            expected.len(),
            "{format}"
        );
        assert_eq!(
            prepared
                .state
                .metadata
                .groups
                .iter()
                .map(|group| group.atoms.len())
                .sum::<usize>(),
            2,
            "{format}"
        );
        if format != "smiles" {
            assert_eq!(groups(&restored), groups(doc), "{format}");
        }
        assert_eq!(
            prepare_identifiers(&prepared, false)?.0,
            identifiers,
            "{format}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn disconnected_cx_groups_preserve_correlation_and_independence() -> anyhow::Result<()> {
    for (text, kind, members) in [
        (
            include_str!("../../../../../tests/fixtures/stereo-groups/shared-and.cxsmiles"),
            Kind::And,
            vec![vec![1, 5]],
        ),
        (
            include_str!("../../../../../tests/fixtures/stereo-groups/shared-or.cxsmiles"),
            Kind::Or,
            vec![vec![1, 5]],
        ),
        (
            include_str!("../../../../../tests/fixtures/stereo-groups/independent-and.cxsmiles"),
            Kind::And,
            vec![vec![1], vec![5]],
        ),
    ] {
        let doc = drawing(text)?;
        let expected: Vec<_> = members
            .into_iter()
            .enumerate()
            .map(|(index, atoms)| {
                (
                    StereoGroup {
                        kind,
                        number: index as u32 + 1,
                    },
                    atoms,
                )
            })
            .collect();
        disconnected_roundtrips(&doc, &expected).await?;
    }
    Ok(())
}

#[tokio::test]
async fn disconnected_v3000_collections_keep_all_members_together() -> anyhow::Result<()> {
    for (text, kind) in [
        (DISCONNECTED_V3000, Kind::And),
        (
            include_str!("../../../../../tests/fixtures/stereo-groups/disconnected-or.mol"),
            Kind::Or,
        ),
    ] {
        let (doc, _, notices) = molfile::read(text)?.display_document()?;
        assert!(notices.is_empty());
        disconnected_roundtrips(&doc, &[(StereoGroup { kind, number: 1 }, vec![1, 5])]).await?;
    }
    Ok(())
}

#[tokio::test]
async fn cdxml_fragment_group_numbers_remain_independent() -> anyhow::Result<()> {
    for xml in [
        TWO_FRAGMENT_CDXML.to_string(),
        exchange::from_cdx(&exchange::to_cdx(TWO_FRAGMENT_CDXML).map_err(anyhow::Error::msg)?)
            .map_err(anyhow::Error::msg)?,
    ] {
        let prepared = cdxml::prepare_cdxml(&xml)?;
        let unchecked = cdxml::assemble_cdxml(&prepared)?.into_unchecked_drawing()?;
        let imported = cdxml::assemble_cdxml(&prepared)?.into_document()?;
        assert!(imported.notices.is_empty());
        assert_eq!(groups(&unchecked), groups(&imported.document));
        disconnected_roundtrips(
            &imported.document,
            &[
                (
                    StereoGroup {
                        kind: Kind::And,
                        number: 1,
                    },
                    vec![1],
                ),
                (
                    StereoGroup {
                        kind: Kind::And,
                        number: 2,
                    },
                    vec![5],
                ),
            ],
        )
        .await?;
    }
    Ok(())
}

#[test]
fn rxn_participant_groups_remain_independent_and_keep_disconnected_members() -> anyhow::Result<()> {
    use crate::chemistry::reaction;
    let source = molfile::read(DISCONNECTED_V3000)?;
    let imported = reaction::Imported {
        reactants: vec![source.clone()],
        products: vec![source],
        agents: Vec::new(),
    };
    let draft = imported.drawing()?;
    let labels = draft.labels()?;
    let doc = draft.finish(labels)?;
    let output = reaction::write_rxn(&doc, None)?;
    assert_eq!(output.matches("MDLV30/STERAC").count(), 2);
    let restored = reaction::read_rxn(&output)?.drawing()?;
    let labels = restored.labels()?;
    let restored = restored.finish(labels)?;
    assert_eq!(groups(&restored), groups(&doc));
    let prepared = molecular::prepare(&restored)?;
    assert_eq!(
        prepared
            .state
            .metadata
            .groups
            .iter()
            .map(|group| (group.kind, group.read_id, group.atoms.clone()))
            .collect::<Vec<_>>(),
        vec![(2, 1, vec![1, 5]), (2, 2, vec![9, 13])]
    );
    Ok(())
}

#[tokio::test]
async fn stereo_groups_analysis_and_inchi_export_need_no_helper() -> anyhow::Result<()> {
    for text in ["C[C@H](N)C(=O)O |&1:1|", "C[C@H](O)[C@H](F)Cl |o1:1,3|"] {
        let doc = drawing(text)?;
        let response = execute(Request::molecule("analyze", doc.clone())).await?;
        assert!(response.warnings.is_empty());
        assert_eq!(groups(response.document.as_ref().unwrap()), groups(&doc));
        let analysis = response.analysis.unwrap();
        assert!(analysis.inchi.is_empty());
        assert!(analysis.inchikey.is_empty());
        assert!(!analysis.formula.is_empty());
        assert!(analysis.mass > 0.);
        assert!(analysis.smiles.contains(" |"));
        assert!(
            response
                .document
                .unwrap()
                .atoms
                .iter()
                .any(|atom| atom.cip_label.is_some())
        );
        let mut request = Request::molecule("export", doc);
        request.format = Some("inchi".into());
        assert_eq!(
            execute(request).await.unwrap_err().to_string(),
            STEREO_GROUP_INCHI_NOTICE
        );
    }
    Ok(())
}

#[tokio::test]
async fn stereo_groups_molecular_exports_roundtrip() -> anyhow::Result<()> {
    for text in [
        "C[C@H](N)C(=O)O |&1:1|",
        "C[C@H](O)[C@H](F)Cl |a:1,&1:3|",
        "C[C@H](O)[C@H](F)Cl |o1:1,3|",
        "C[C@H](O)C(=O)O |r|",
    ] {
        let doc = drawing(text)?;
        let expected = prepare_identifiers(&molecular::prepare(&doc)?, false)?.0;
        for format in ["mol", "smiles", "cdxml", "cdx"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let response = execute(request).await?;
            assert!(
                response.warnings.is_empty(),
                "{text}/{format}: {:?}",
                response.warnings
            );
            let output = response.output.unwrap();
            let restored = match format {
                "smiles" => drawing(&output)?,
                "mol" => {
                    assert!(output.contains("V3000"));
                    assert!(output.contains("MDLV30/STER"));
                    molfile::read(&output)?.display_document()?.0
                }
                _ => {
                    let xml = if format == "cdx" {
                        exchange::from_cdx(&STANDARD.decode(output)?).map_err(anyhow::Error::msg)?
                    } else {
                        output
                    };
                    assert!(xml.contains("EnhancedStereoGroupNum=\"1\""));
                    let imported = cdxml::import_cdxml(&xml)?;
                    assert!(imported.notices.is_empty());
                    imported.document
                }
            };
            if format != "smiles" {
                assert_eq!(groups(&restored), groups(&doc), "{text}/{format}");
            }
            assert_eq!(
                prepare_identifiers(&molecular::prepare(&restored)?, false)?.0,
                expected,
                "{text}/{format}"
            );
        }
    }
    Ok(())
}

#[test]
fn stereo_groups_cdxml_numbers_follow_the_cdx_schema() -> anyhow::Result<()> {
    let mut doc = drawing("C[C@H](O)[C@H](F)Cl |a:1,&1:3|")?;
    doc.atoms[3].stereo_group.as_mut().unwrap().number = 65535;
    let xml = exchange::drawing::write(&doc, Default::default())?;
    assert!(xml.contains("EnhancedStereoType=\"Absolute\""));
    let bytes = exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
    let xml = exchange::from_cdx(&bytes).map_err(anyhow::Error::msg)?;
    assert_eq!(groups(&cdxml::import_cdxml(&xml)?.document), groups(&doc));
    doc.atoms[3].stereo_group.as_mut().unwrap().number = 65536;
    assert!(
        exchange::drawing::write(&doc, Default::default())
            .unwrap_err()
            .to_string()
            .contains("65535")
    );
    Ok(())
}

#[test]
fn stereo_groups_abs_only_identifiers_and_cdxml_stay_plain() -> anyhow::Result<()> {
    let doc = drawing("C[C@H](N)C(=O)O |a:1|")?;
    assert!(groups(&doc).iter().all(Option::is_none));
    let molecule = molecular::prepare(&doc)?;
    assert_eq!(
        prepare_identifiers(&molecule, false)?.0,
        smiles::write::write(&molecule.state, Default::default())?.text
    );
    assert!(!exchange::drawing::write(&doc, Default::default())?.contains("EnhancedStereo"));
    Ok(())
}

/// Any object group makes the CDXML writer put each molecule in its own
/// fragment; ChemDraw scopes stereo groups to a fragment, so two molecules
/// sharing a group stay in one. A group split by object grouping is refused.
#[test]
fn cdxml_object_groups_keep_a_shared_stereo_group_in_one_fragment() -> anyhow::Result<()> {
    let mut doc = drawing("C[C@H](O)F.C[C@H](O)Cl.CCO |&1:1,5|")?;
    let id = doc.next_id();
    doc.groups.push(crate::grouping::Group {
        id,
        members: vec![9, 10, 11],
        integral: true,
    });
    let xml = exchange::drawing::write(&doc, Default::default())?;
    let imported = cdxml::import_cdxml(&xml)?.document;
    let and1 = StereoGroup {
        kind: Kind::And,
        number: 1,
    };
    let centres: Vec<_> = imported
        .atoms
        .iter()
        .filter_map(|a| a.stereo_group)
        .collect();
    assert_eq!(centres, [and1; 2]);
    doc.groups[0].members = vec![1, 2, 3, 4];
    let error = exchange::drawing::write(&doc, Default::default()).unwrap_err();
    assert!(error.to_string().contains("stereo group"), "{error}");
    Ok(())
}

/// RXN gives each participant its own groups, so one group across two
/// participants can't be written; reaction SMILES keeps it as one field.
#[test]
fn rxn_refuses_a_group_spanning_participants() -> anyhow::Result<()> {
    use crate::chemistry::reaction;
    let source = molfile::read(DISCONNECTED_V3000)?;
    let imported = reaction::Imported {
        reactants: vec![source.clone()],
        products: vec![source],
        agents: Vec::new(),
    };
    let draft = imported.drawing()?;
    let labels = draft.labels()?;
    let mut doc = draft.finish(labels)?;
    for group in doc
        .atoms
        .iter_mut()
        .filter_map(|atom| atom.stereo_group.as_mut())
    {
        group.number = 1;
    }
    let error = reaction::write_rxn(&doc, None).unwrap_err();
    assert!(error.to_string().contains("stereo group"), "{error}");
    assert!(reaction::write_smiles(&doc, None)?.contains("&1:"));
    Ok(())
}
