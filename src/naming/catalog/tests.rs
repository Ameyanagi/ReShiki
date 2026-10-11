use super::*;

fn names(smiles: &str) -> Result<Vec<String>, String> {
    Ok(aliases_for_smiles(smiles)?
        .into_iter()
        .map(|a| a.name)
        .collect())
}

#[test]
fn catalog_identity_is_order_invariant_without_relaxing_stereo_charge_or_isotopes()
-> Result<(), String> {
    assert_eq!(names("CCO")?, ["ethanol", "ethyl alcohol"]);
    assert_eq!(names("OCC")?, names("[CH3:7][CH2:8][OH:9]")?);
    assert_eq!(names("OCC")?, names("CCO")?);
    assert_eq!(
        names("C[C@H](O)C(=O)O")?,
        ["L-lactic acid", "(+)-lactic acid"]
    );
    assert_eq!(names("O=C(O)[C@@H](O)C")?, names("C[C@H](O)C(=O)O")?);
    assert_eq!(names("C[C@@H](O)C(=O)O")?, ["D-lactic acid"]);
    assert_eq!(names("CC(O)C(=O)O")?, ["lactic acid"]);
    assert_eq!(names("C/C=C/C(=O)O")?, ["crotonic acid"]);
    assert_eq!(names("C/C=C\\C(=O)O")?, ["isocrotonic acid"]);
    for absent in [
        "[13CH3]CO",
        "CC[O-]",
        "C[C@H](O)C(=O)[O-]",
        "[13CH3][C@H](O)C(=O)O",
        "CC=CC(=O)O",
        "CCCCC(O)",
    ] {
        assert!(names(absent)?.is_empty(), "Assigned an alias to {absent}");
    }
    assert!(aliases_for_smiles("CCO.C").is_err());
    assert!(aliases_for_smiles("C[C@H](O)C").is_err());
    Ok(())
}

#[test]
fn every_curated_alias_has_pinned_evidence_and_decodes_to_its_independent_graph()
-> Result<(), String> {
    let data: Catalog = serde_json::from_str(DATA).unwrap();
    assert_eq!(data.entries.len(), 30);
    assert_eq!(
        data.entries
            .iter()
            .map(|entry| entry.aliases.len())
            .sum::<usize>(),
        40
    );
    let parser = opsin::Parser::new().map_err(|error| error.to_string())?;
    for entry in &data.entries {
        for alias in &entry.aliases {
            for evidence in &alias.evidence {
                let xml = match evidence.file.as_str() {
                    "alkanes.xml" => {
                        include_str!("../../../crates/opsin/resources/xml/alkanes.xml")
                    }
                    "arylGroups.xml" => {
                        include_str!("../../../crates/opsin/resources/xml/arylGroups.xml")
                    }
                    "carboxylicAcids.xml" => {
                        include_str!("../../../crates/opsin/resources/xml/carboxylicAcids.xml")
                    }
                    "functionalTerms.xml" => {
                        include_str!("../../../crates/opsin/resources/xml/functionalTerms.xml")
                    }
                    "groupStemsAllowingInlineSuffixes.xml" => include_str!(
                        "../../../crates/opsin/resources/xml/groupStemsAllowingInlineSuffixes.xml"
                    ),
                    "inlineSuffixes.xml" => {
                        include_str!("../../../crates/opsin/resources/xml/inlineSuffixes.xml")
                    }
                    "simpleCyclicGroups.xml" => {
                        include_str!("../../../crates/opsin/resources/xml/simpleCyclicGroups.xml")
                    }
                    "simpleGroups.xml" => {
                        include_str!("../../../crates/opsin/resources/xml/simpleGroups.xml")
                    }
                    "suffixes.xml" => {
                        include_str!("../../../crates/opsin/resources/xml/suffixes.xml")
                    }
                    _ => panic!("Unpinned resource {}", evidence.file),
                };
                let resource = roxmltree::Document::parse_with_options(
                    xml,
                    roxmltree::ParsingOptions {
                        allow_dtd: true,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert!(
                    resource.descendants().any(|node| {
                        node.has_tag_name("token")
                            && node.text().is_some_and(|text| {
                                text.split('|').any(|token| token == evidence.token)
                            })
                    }),
                    "Missing source token {}: {}",
                    evidence.file,
                    evidence.token
                );
            }
            let interpreted = if alias.source == Source::Chebi422 {
                "(2S)-2-hydroxypropanoic acid"
            } else {
                &alias.name
            };
            let result = parser.parse(interpreted, &opsin::ParseOptions::strict());
            assert_eq!(
                result.status,
                opsin::Status::Success,
                "{}: {}",
                alias.name,
                result.message
            );
            assert!(
                result.warnings.is_empty(),
                "{}: {:?}",
                alias.name,
                result.warnings
            );
            let actual = result
                .structure
                .ok_or("Missing catalog alias graph")?
                .semantic_cxsmiles()
                .map_err(|error| error.to_string())?;
            super::super::verify_identity(&entry.smiles, &actual)
                .map_err(|error| format!("{}: {error}", alias.name))?;
            let found = aliases_for_smiles(&entry.smiles)?;
            let metadata = found.iter().find(|a| a.name == alias.name).unwrap();
            assert!(!metadata.source.url.is_empty());
            assert_eq!(metadata.source.record, entry.id);
            assert_eq!(
                metadata.source.license,
                if alias.source == Source::Opsin {
                    "MIT"
                } else {
                    "CC BY 4.0"
                }
            );
        }
    }
    Ok(())
}
