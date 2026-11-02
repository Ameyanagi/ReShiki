use super::*;

const CASES: [(&str, &str); 7] = [
    ("(1M)-[1,1′-binaphthalene]-2,2′-diol", "M"),
    ("(1P)-[1,1′-binaphthalene]-2,2′-diol", "P"),
    (
        "(1M)-6,6′-dinitro[1,1′-biphenyl]-2,2′-dicarboxylic acid",
        "M",
    ),
    (
        "(1P)-2′,5′-dimethoxy-6-nitro[1,1′-biphenyl]-2-carboxylic acid",
        "P",
    ),
    (
        "(1M)-2′,6-diamino-6′-methoxy[1,1′-biphenyl]-2-carboxylic acid",
        "M",
    ),
    (
        "(2P)-2-[2-(hydroxymethyl)naphthalen-1-yl]-3,5-dimethylphenol",
        "P",
    ),
    (
        "(1M)-2′-hydroxy[1,1′-binaphthalen]-2-yl (1S)-2,2-dimethylcyclopropane-1-carboxylate",
        "M",
    ),
];

#[test]
fn axial_blue_book_names_and_general_forms_agree_with_native_cip() {
    for (name, descriptor) in CASES {
        let general = name.replacen(descriptor, if descriptor == "M" { "Ra" } else { "Sa" }, 1);
        let mut identities = Vec::new();
        for name in [name, general.as_str()] {
            let reply = worker::respond(&local::parse_request(name).unwrap()).unwrap();
            let record =
                local::parse_result(name, &reply).unwrap_or_else(|error| panic!("{name}: {error}"));
            let state = chemistry::smiles::read(&record.smiles)
                .unwrap()
                .prepared
                .state;
            let state = molecule_names::apply_axis_markers(&state).unwrap();
            let state = chemistry::stereo::cip::label::assign(&state, &Default::default())
                .unwrap()
                .state;
            let axes: Vec<_> = state
                .properties
                .bond_codes
                .iter()
                .filter_map(Option::as_deref)
                .filter(|code| matches!(*code, "M" | "P"))
                .collect();
            assert_eq!(axes, [descriptor], "{name}");
            assert_eq!(
                canonical_smiles(&record.smiles).unwrap(),
                record.canonical_smiles
            );
            assert_eq!(
                canonical_smiles(&record.canonical_smiles).unwrap(),
                record.canonical_smiles
            );
            identities.push(record.canonical_smiles);
        }
        assert_eq!(identities[0], identities[1], "{name}");
    }
}

#[test]
fn axial_transport_rejects_reference_and_atom_count_mismatches() {
    let name = CASES[0].0;
    let bytes = worker::respond(&local::parse_request(name).unwrap()).unwrap();
    let reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut reversed = reply.clone();
    reversed["axes"][0]["atoms"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reversed["axes"][0]["refs"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(
        local::parse_result(name, &serde_json::to_vec(&reversed).unwrap())
            .unwrap()
            .canonical_smiles,
        local::parse_result(name, &bytes).unwrap().canonical_smiles,
    );

    let graph = chemistry::smiles::parse(reply["cxsmiles"].as_str().unwrap())
        .unwrap()
        .graph;
    let atom = reply["axes"][0]["atoms"][0].as_u64().unwrap() as usize;
    let partner = reply["axes"][0]["atoms"][1].as_u64().unwrap() as usize;
    let reference = reply["axes"][0]["refs"][0].as_u64().unwrap() as usize;
    let other = graph
        .bonds
        .iter()
        .find_map(|bond| {
            let neighbour = if bond.a == atom {
                Some(bond.b)
            } else if bond.b == atom {
                Some(bond.a)
            } else {
                None
            }?;
            (neighbour != partner && neighbour != reference).then_some(neighbour)
        })
        .unwrap();
    let mut wrong_reference = reply.clone();
    wrong_reference["axes"][0]["refs"][0] = other.into();
    let error =
        local::parse_result(name, &serde_json::to_vec(&wrong_reference).unwrap()).unwrap_err();
    assert!(
        error.to_string().contains("CIP assignments disagree"),
        "{error}"
    );

    for field in ["atoms", "refs"] {
        let mut invalid = reply.clone();
        invalid["axes"][0][field] = serde_json::json!([usize::MAX, usize::MAX]);
        assert!(local::parse_result(name, &serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let mut duplicate = reply.clone();
    duplicate["axes"]
        .as_array_mut()
        .unwrap()
        .push(reply["axes"][0].clone());
    assert!(local::parse_result(name, &serde_json::to_vec(&duplicate).unwrap()).is_err());

    let mut removed_hydrogen = reply;
    let smiles = removed_hydrogen["cxsmiles"].as_str().unwrap().to_owned();
    removed_hydrogen["cxsmiles"] = smiles.replacen('O', "O[H]", 1).into();
    removed_hydrogen["locants"]
        .as_array_mut()
        .unwrap()
        .insert(0, serde_json::Value::Null);
    let error =
        local::parse_result(name, &serde_json::to_vec(&removed_hydrogen).unwrap()).unwrap_err();
    assert!(
        error.to_string().contains("changed the atom count"),
        "{error}"
    );
}

#[test]
fn axial_transport_applies_indices_before_salt_component_reordering() {
    let name = CASES[0].0;
    let bytes = worker::respond(&local::parse_request(name).unwrap()).unwrap();
    let mut reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let smiles = reply["cxsmiles"].as_str().unwrap().to_owned();
    reply["cxsmiles"] = format!("[Cl-].{smiles}.[Na+]").into();
    for field in ["atoms", "refs"] {
        for index in reply["axes"][0][field].as_array_mut().unwrap() {
            *index = (index.as_u64().unwrap() + 1).into();
        }
    }
    let locants = reply["locants"].as_array_mut().unwrap();
    locants.insert(0, serde_json::Value::Null);
    locants.push(serde_json::Value::Null);
    let salt = local::parse_result(name, &serde_json::to_vec(&reply).unwrap()).unwrap();
    assert!(salt.smiles.starts_with("[Na+]."));
    assert!(salt.locants.as_ref().unwrap().smiles.starts_with("[Cl-]."));
    let original = local::parse_result(name, &bytes).unwrap();
    verify_identity(&format!("[Na+].{}.[Cl-]", original.smiles), &salt.smiles).unwrap();
}

#[tokio::test]
async fn axial_generated_names_and_ra_sa_alternatives_round_trip() {
    for (name, descriptor) in CASES {
        let parsed = resolve_name(name, Cancel::default())
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let identity = Identity {
            smiles: parsed.smiles.clone(),
            naming_smiles: None,
            write_unknown_stereo: false,
            warnings: vec![],
        };
        let generated = generate_name(&identity, Cancel::default())
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            generated.title.contains(&format!("{descriptor})")),
            "{}",
            generated.title
        );
        verify_identity(&parsed.smiles, &generated.smiles).unwrap();
        let round_trip = resolve_name(&generated.title, Cancel::default())
            .await
            .unwrap();
        assert_eq!(parsed.canonical_smiles, round_trip.canonical_smiles);
        let alternative = generated
            .alternatives
            .iter()
            .find(|alternative| {
                alternative.status == NameStatus::NotPreferred(NameReason::AxialAlternative)
            })
            .expect("Verified Ra/Sa alternative");
        assert!(
            alternative
                .name
                .contains(if descriptor == "M" { "Ra)" } else { "Sa)" })
        );
        assert!(alternative.status.detail().contains("P-93.5.7.1"));
        let general = resolve_name(&alternative.name, Cancel::default())
            .await
            .unwrap();
        assert_eq!(parsed.canonical_smiles, general.canonical_smiles);
        if name.contains("binaphthalene]-2") {
            assert_eq!(
                generated.title,
                format!("(1{descriptor})-1-(2-hydroxynaphthalen-1-yl)naphthalen-2-ol")
            );
            assert_eq!(
                generated.name_status,
                Some(NameStatus::NotPreferred(NameReason::RingAssembly))
            );
        }
        println!(
            "AXIAL RESULT {name}\nNAME: {}\nGENERAL: {}",
            generated.title, alternative.name
        );
    }
}

#[tokio::test]
async fn axial_ester_candidate_without_its_axis_descriptor_is_refused() {
    let name = CASES[6].0;
    let bytes = worker::respond(&local::parse_request(name).unwrap()).unwrap();
    let parsed = local::parse_result(name, &bytes).unwrap();
    let identity = Identity {
        smiles: parsed.smiles,
        naming_smiles: None,
        write_unknown_stereo: false,
        warnings: vec![],
    };
    let mut candidate = openclatura::name_smiles(&identity.smiles).unwrap();
    assert!(candidate.text.starts_with("(1M)-"));
    candidate.text = candidate.text.replacen("(1M)-", "", 1);
    candidate.alternatives.clear();
    let error = reverse::reconstruct(
        &identity,
        parsed.canonical_smiles,
        candidate,
        &Cancel::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Rejected(_)), "{error}");
    assert!(error.to_string().contains("stereochemistry"), "{error}");
    println!("AXIAL OMITTED-DESCRIPTOR REFUSAL: {error}");
}

#[tokio::test]
async fn axial_drawing_identity_keeps_unknown_stereo_markers_and_the_axis() {
    let input = "C[C@H](O)c1cccc[c:900004]1-[c:900004]1ccccc1O";
    let response = crate::engine::LocalEngine::in_process()
        .request(crate::engine::Request::import("naming_smiles", input))
        .await
        .unwrap();
    let mut document = response.document.unwrap();
    let molecule = chemistry::document::prepare(&document).unwrap();
    let atom = molecule
        .state
        .metadata
        .atoms
        .iter()
        .position(|atom| is_tetrahedral(atom.chiral_tag))
        .unwrap();
    let id = molecule.ids[atom];
    let bond = document
        .bonds
        .iter_mut()
        .find(|bond| bond.a == id && matches!(bond.display.as_str(), "wedge" | "hash"))
        .unwrap();
    bond.display = "wavy".into();
    let identity = document_identity(&document).unwrap();
    let naming_smiles = identity.naming_smiles.as_deref().unwrap();
    assert_eq!(naming_smiles.matches(":900004]").count(), 2);
    assert_eq!(naming_smiles.matches(":900001]").count(), 1);
    verify_identity(&identity.smiles, naming_smiles).unwrap();
    let candidate = openclatura::name_smiles(naming_smiles).unwrap();
    assert!(candidate.text.contains("M)"), "{}", candidate.text);
    assert_eq!(candidate.omitted_units, 1);
    assert!(
        candidate
            .alternatives
            .iter()
            .any(|alternative| alternative.name.contains("Ra)"))
    );
    let mirror = identity.smiles.replace(":900004]", ":900008]");
    assert!(verify_identity(&identity.smiles, &mirror).is_err());
}
