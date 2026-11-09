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
    assert_eq!(data.entries.len(), 85);
    assert_eq!(
        data.entries
            .iter()
            .map(|entry| entry.aliases.len())
            .sum::<usize>(),
        97
    );
    let parser = nazuchem_n2s::Parser::new().map_err(|error| error.to_string())?;
    for entry in &data.entries {
        for alias in &entry.aliases {
            for evidence in &alias.evidence {
                let xml = pinned_resource(&evidence.file);
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
            let result = parser.parse(interpreted, &nazuchem_n2s::ParseOptions::strict());
            assert_eq!(
                result.status,
                nazuchem_n2s::Status::Success,
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

fn pinned_resource(file: &str) -> &'static str {
    match file {
        "alkanes.xml" => include_str!("../../../tests/fixtures/naming/opsin/xml/alkanes.xml"),
        "aminoAcids.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/aminoAcids.xml")
        }
        "arylSubstituents.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/arylSubstituents.xml")
        }
        "arylGroups.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/arylGroups.xml")
        }
        "carbohydrateSuffixes.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/carbohydrateSuffixes.xml")
        }
        "carbohydrates.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/carbohydrates.xml")
        }
        "carboxylicAcids.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/carboxylicAcids.xml")
        }
        "functionalTerms.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/functionalTerms.xml")
        }
        "groupStemsAllowingInlineSuffixes.xml" => {
            include_str!(
                "../../../tests/fixtures/naming/opsin/xml/groupStemsAllowingInlineSuffixes.xml"
            )
        }
        "inlineSuffixes.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/inlineSuffixes.xml")
        }
        "miscTokens.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/miscTokens.xml")
        }
        "multipliers.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/multipliers.xml")
        }
        "simpleCyclicGroups.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/simpleCyclicGroups.xml")
        }
        "simpleGroups.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/simpleGroups.xml")
        }
        "simpleSubstituents.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/simpleSubstituents.xml")
        }
        "suffixes.xml" => include_str!("../../../tests/fixtures/naming/opsin/xml/suffixes.xml"),
        "unsaturators.xml" => {
            include_str!("../../../tests/fixtures/naming/opsin/xml/unsaturators.xml")
        }
        _ => panic!("Unpinned resource {file}"),
    }
}

#[test]
fn catalog_provenance_pins_the_shipping_data_and_all_cited_resources() {
    use sha2::{Digest, Sha256};

    let sha256 = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let provenance: serde_json::Value =
        serde_json::from_str(include_str!("provenance.json")).unwrap();
    let data: Catalog = serde_json::from_str(DATA).unwrap();
    assert_eq!(provenance["entry_count"], data.entries.len());
    assert_eq!(
        provenance["alias_count"],
        data.entries
            .iter()
            .map(|entry| entry.aliases.len())
            .sum::<usize>()
    );
    assert_eq!(provenance["data_sha256"], sha256(DATA.as_bytes()));
    assert_eq!(
        provenance["opsin"]["version"],
        nazuchem_n2s::UPSTREAM_VERSION
    );
    assert_eq!(provenance["opsin"]["commit"], nazuchem_n2s::UPSTREAM_COMMIT);
    assert_eq!(
        provenance["opsin"]["resource_fingerprint"],
        nazuchem_n2s::RESOURCE_FINGERPRINT
    );
    let resources = provenance["opsin"]["resources"].as_array().unwrap();
    for resource in resources {
        let file = resource["path"]
            .as_str()
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap();
        assert_eq!(
            resource["sha256"],
            sha256(pinned_resource(file).as_bytes()),
            "{file}"
        );
    }
    for entry in &data.entries {
        for alias in &entry.aliases {
            assert!(alias.source != Source::Opsin || !alias.evidence.is_empty());
            for evidence in &alias.evidence {
                assert!(
                    resources.iter().any(|resource| {
                        resource["path"].as_str().unwrap().rsplit('/').next()
                            == Some(evidence.file.as_str())
                    }),
                    "Unhashed evidence file {}",
                    evidence.file
                );
            }
        }
    }
    for (path, content) in [
        (
            "assets/template-catalog.json",
            include_str!("../../../assets/template-catalog.json"),
        ),
        (
            "assets/haworth-sugars.json",
            include_str!("../../../assets/haworth-sugars.json"),
        ),
    ] {
        let reference = provenance["reference_graphs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|reference| reference["path"] == path)
            .unwrap();
        assert_eq!(reference["sha256"], sha256(content.as_bytes()), "{path}");
    }
}

#[test]
fn biochemical_aliases_match_the_exact_template_and_haworth_asset_graphs() -> Result<(), String> {
    let templates: serde_json::Value =
        serde_json::from_str(include_str!("../../../assets/template-catalog.json")).unwrap();
    let sugars: serde_json::Value =
        serde_json::from_str(include_str!("../../../assets/haworth-sugars.json")).unwrap();
    let templates: Vec<_> = templates
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["group"] == "Amino acids" || entry["group"] == "Nucleobases")
        .collect();
    assert_eq!(templates.len(), 26);
    assert_eq!(sugars.as_array().unwrap().len(), 24);
    for entry in templates.into_iter().chain(sugars.as_array().unwrap()) {
        let asset_name = entry["name"].as_str().unwrap();
        let alias = match asset_name {
            "Adenine (9H)" => "9H-adenine",
            "Guanine (9H)" => "9H-guanine",
            "Cytosine" => "3H-cytosine",
            "L-Histidine" => "3-(1H-imidazol-5-yl)-L-alanine",
            "L-Arginine" => "N(5)-(diaminomethylidene)-L-ornithine",
            // Drop a trailing abbreviation such as " (GlcNAc)".
            _ => asset_name.split(" (").next().unwrap(),
        };
        let smiles = entry["tautomer_smiles"]
            .as_str()
            .or_else(|| entry["smiles"].as_str())
            .unwrap();
        let found = names(smiles)?;
        assert!(
            found.iter().any(|name| name.eq_ignore_ascii_case(alias)),
            "{asset_name}: {found:?}"
        );
    }
    Ok(())
}

#[test]
fn biochemical_aliases_keep_configuration_and_purine_tautomers_distinct() -> Result<(), String> {
    assert_eq!(names("C[C@@H](C(=O)O)N")?, ["L-alanine"]);
    assert_eq!(names("N[C@@H](C)C(=O)O")?, ["L-alanine"]);
    assert!(names("C[C@H](C(=O)O)N")?.is_empty());
    assert!(names("CC(C(=O)O)N")?.is_empty());
    assert!(names("C[C@@H](C(=O)[O-])[NH3+]")?.is_empty());
    assert_eq!(names("Nc1ncnc2[nH]cnc12")?, ["9H-adenine"]);
    assert_eq!(names("C1=NC2=NC=NC(=C2N1)N")?, ["adenine", "7H-adenine"]);
    assert_eq!(names("Nc1nc2[nH]cnc2c(=O)[nH]1")?, ["9H-guanine"]);
    assert_eq!(
        names("C1=NC2=C(N1)C(=O)NC(=N2)N")?,
        ["guanine", "7H-guanine"]
    );
    let alpha = "C([C@@H]1[C@H]([C@@H]([C@H]([C@H](O1)O)O)O)O)O";
    let beta = "C([C@@H]1[C@H]([C@@H]([C@H]([C@@H](O1)O)O)O)O)O";
    assert_eq!(names(alpha)?, ["α-D-glucopyranose"]);
    assert_eq!(names(beta)?, ["β-D-glucopyranose"]);
    assert_ne!(canonical_smiles(alpha)?, canonical_smiles(beta)?);
    assert!(names("C([C@@H]1[C@H]([C@@H]([C@H](C(O1)O)O)O)O)O")?.is_empty());
    Ok(())
}

#[test]
fn biochemical_plain_opsin_defaults_keep_template_tautomers_distinct() -> Result<(), String> {
    let parser = nazuchem_n2s::Parser::new().map_err(|error| error.to_string())?;
    for (name, independent_graph, template_graph, template_alias) in [
        (
            "cytosine",
            "N1C(=O)N=C(N)C=C1",
            "C1=C(NC(=O)N=C1)N",
            "3H-cytosine",
        ),
        (
            "L-histidine",
            "N[C@@H](CC1=CNC=N1)C(=O)O",
            "C1=C(NC=N1)C[C@@H](C(=O)O)N",
            "3-(1H-imidazol-5-yl)-L-alanine",
        ),
        (
            "L-arginine",
            "N[C@@H](CCCNC(N)=N)C(=O)O",
            "C(C[C@@H](C(=O)O)N)CN=C(N)N",
            "N(5)-(diaminomethylidene)-L-ornithine",
        ),
    ] {
        let result = parser.parse(name, &nazuchem_n2s::ParseOptions::strict());
        assert_eq!(
            result.status,
            nazuchem_n2s::Status::Success,
            "{name}: {}",
            result.message
        );
        assert!(result.warnings.is_empty(), "{name}: {:?}", result.warnings);
        let actual = result
            .structure
            .ok_or("Missing biochemical default graph")?
            .semantic_cxsmiles()
            .map_err(|error| error.to_string())?;
        super::super::verify_identity(independent_graph, &actual)?;
        assert_eq!(names(independent_graph)?, [name]);
        assert_eq!(names(&actual)?, [name]);
        assert_eq!(names(template_graph)?, [template_alias]);
        assert_ne!(
            canonical_smiles(independent_graph)?,
            canonical_smiles(template_graph)?,
            "Merged the {name} default and template tautomers"
        );
    }
    for absent in [
        "N[C@H](CC1=CNC=N1)C(=O)O",
        "NC(CC1=CNC=N1)C(=O)O",
        "[NH3+][C@@H](CC1=CNC=N1)C(=O)[O-]",
        "N[C@H](CCCNC(N)=N)C(=O)O",
        "NC(CCCNC(N)=N)C(=O)O",
        "[NH3+][C@@H](CCCNC(N)=N)C(=O)[O-]",
    ] {
        assert!(names(absent)?.is_empty(), "Assigned an alias to {absent}");
    }
    Ok(())
}
