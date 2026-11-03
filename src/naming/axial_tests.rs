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

// Independent expectations: RDKit 2026.03.6 read each complete CXSMILES below
// with Chem.MolFromSmiles, then rdCIPLabeler.AssignCIPLabels confirmed M/P.
// Each pair is [M/Ra, P/Sa]. Coordinates and wedges specify the axial hand.
// OH/D, Me/D, methyl/iPr, hydroxymethyl, BINOL and ester fixtures come from the
// independent s865 audit inputs; Br/D, ethyl/iPr and methyl/phenyl were built
// from independent SMILES with RDKit Compute2DCoords and MolToCXSmiles, then
// re-read and labelled. Both ester fixtures also retain the independent S centre.
const INDEPENDENT_AXIAL_CASES: [(&str, &str, [&str; 2]); 9] = [
    (
        "1",
        "[1,1'-biphenyl]-2-ol-2'-d",
        [
            "[2H]c1ccccc1-c1ccccc1O |(0.75,-2.22692,;1.5,-0.927884,;3,-0.927884,;3.75,0.371154,;3,1.67019,;1.5,1.67019,;0.75,0.371154,;-0.75,0.371154,;-1.5,1.67019,;-3,1.67019,;-3.75,0.371154,;-3,-0.927884,;-1.5,-0.927884,;-0.75,-2.22692,),wU:6.6|",
            "[2H]c1ccccc1-c1ccccc1O |(0.75,-2.22692,;1.5,-0.927884,;3,-0.927884,;3.75,0.371154,;3,1.67019,;1.5,1.67019,;0.75,0.371154,;-0.75,0.371154,;-1.5,1.67019,;-3,1.67019,;-3.75,0.371154,;-3,-0.927884,;-1.5,-0.927884,;-0.75,-2.22692,),wU:6.5|",
        ],
    ),
    (
        "1",
        "2-methyl[1,1'-biphenyl]-2'-d",
        [
            "[2H]c1ccccc1-c1ccccc1C |(0.75,-2.22692,;1.5,-0.927884,;3,-0.927884,;3.75,0.371154,;3,1.67019,;1.5,1.67019,;0.75,0.371154,;-0.75,0.371154,;-1.5,1.67019,;-3,1.67019,;-3.75,0.371154,;-3,-0.927884,;-1.5,-0.927884,;-0.75,-2.22692,),wU:6.6|",
            "[2H]c1ccccc1-c1ccccc1C |(0.75,-2.22692,;1.5,-0.927884,;3,-0.927884,;3.75,0.371154,;3,1.67019,;1.5,1.67019,;0.75,0.371154,;-0.75,0.371154,;-1.5,1.67019,;-3,1.67019,;-3.75,0.371154,;-3,-0.927884,;-1.5,-0.927884,;-0.75,-2.22692,),wU:6.5|",
        ],
    ),
    (
        "1",
        "2-bromo[1,1'-biphenyl]-2'-d",
        [
            "[2H]c1ccccc1-c1ccccc1Br |(-0.954907,-2.52995,;-1.59887,-1.17521,;-3.09408,-1.05553,;-3.73804,0.299209,;-2.88679,1.53426,;-1.39157,1.41458,;-0.747609,0.0598419,;0.747609,-0.0598419,;1.39157,-1.41458,;2.88679,-1.53426,;3.73804,-0.299209,;3.09408,1.05553,;1.59887,1.17521,;0.954907,2.52995,),wU:6.6|",
            "[2H]c1ccccc1-c1ccccc1Br |(-0.954907,-2.52995,;-1.59887,-1.17521,;-3.09408,-1.05553,;-3.73804,0.299209,;-2.88679,1.53426,;-1.39157,1.41458,;-0.747609,0.0598419,;0.747609,-0.0598419,;1.39157,-1.41458,;2.88679,-1.53426,;3.73804,-0.299209,;3.09408,1.05553,;1.59887,1.17521,;0.954907,2.52995,),wU:6.5|",
        ],
    ),
    (
        "2",
        "2-[2-methyl-6-(propan-2-yl)pyridin-4-yl]phenol",
        [
            "Cc1cc(-c2ccccc2O)cc(C(C)C)n1 |(1.92469,-3.53733,;1.58356,-2.07664,;0.147993,-1.64171,;-0.193136,-0.181019,;-1.6287,0.253903,;-1.96983,1.7146,;-3.40539,2.14952,;-4.49983,1.12375,;-4.1587,-0.33695,;-2.72313,-0.771871,;-2.38201,-2.23257,;0.901299,0.844755,;2.33686,0.409834,;3.4313,1.43561,;4.86686,1.00069,;3.09017,2.8963,;2.67799,-1.05086,),wU:3.11|",
            "Cc1cc(-c2ccccc2O)cc(C(C)C)n1 |(1.92469,-3.53733,;1.58356,-2.07664,;0.147993,-1.64171,;-0.193136,-0.181019,;-1.6287,0.253903,;-1.96983,1.7146,;-3.40539,2.14952,;-4.49983,1.12375,;-4.1587,-0.33695,;-2.72313,-0.771871,;-2.38201,-2.23257,;0.901299,0.844755,;2.33686,0.409834,;3.4313,1.43561,;4.86686,1.00069,;3.09017,2.8963,;2.67799,-1.05086,),wU:3.2|",
        ],
    ),
    (
        "2",
        "2-[2-ethyl-6-(propan-2-yl)pyridin-4-yl]phenol",
        [
            "CCc1cc(-c2ccccc2O)cc(C(C)C)n1 |(1.01252,-4.26173,;2.05077,-3.17913,;1.63234,-1.73867,;0.175651,-1.38081,;-0.242781,0.0596425,;-1.69947,0.417498,;-2.1179,1.85795,;-3.57459,2.21581,;-4.61285,1.13321,;-4.19441,-0.307249,;-2.73772,-0.665104,;-2.31929,-2.10556,;0.795474,1.14224,;2.25216,0.784389,;3.29042,1.86699,;4.74711,1.50913,;2.87199,3.30745,;2.67059,-0.656068,),wU:4.12|",
            "CCc1cc(-c2ccccc2O)cc(C(C)C)n1 |(1.01252,-4.26173,;2.05077,-3.17913,;1.63234,-1.73867,;0.175651,-1.38081,;-0.242781,0.0596425,;-1.69947,0.417498,;-2.1179,1.85795,;-3.57459,2.21581,;-4.61285,1.13321,;-4.19441,-0.307249,;-2.73772,-0.665104,;-2.31929,-2.10556,;0.795474,1.14224,;2.25216,0.784389,;3.29042,1.86699,;4.74711,1.50913,;2.87199,3.30745,;2.67059,-0.656068,),wU:4.3|",
        ],
    ),
    (
        "2",
        "2-(2-methyl-6-phenylpyridin-4-yl)phenol",
        [
            "Cc1cc(-c2ccccc2O)cc(-c2ccccc2)n1 |(-0.00691215,-4.16705,;0.0657146,-2.66881,;-1.19549,-1.85679,;-1.12286,-0.358554,;-2.38406,0.453463,;-2.31143,1.9517,;-3.57264,2.76372,;-4.90646,2.0775,;-4.97909,0.579256,;-3.71789,-0.232761,;-3.79052,-1.731,;0.210968,0.32767,;1.47217,-0.484347,;2.806,0.201876,;4.0672,-0.610141,;5.40103,0.0760831,;5.47365,1.57432,;4.21245,2.38634,;2.87862,1.70012,;1.39954,-1.98259,),wU:3.11|",
            "Cc1cc(-c2ccccc2O)cc(-c2ccccc2)n1 |(-0.00691215,-4.16705,;0.0657146,-2.66881,;-1.19549,-1.85679,;-1.12286,-0.358554,;-2.38406,0.453463,;-2.31143,1.9517,;-3.57264,2.76372,;-4.90646,2.0775,;-4.97909,0.579256,;-3.71789,-0.232761,;-3.79052,-1.731,;0.210968,0.32767,;1.47217,-0.484347,;2.806,0.201876,;4.0672,-0.610141,;5.40103,0.0760831,;5.47365,1.57432,;4.21245,2.38634,;2.87862,1.70012,;1.39954,-1.98259,),wU:3.2|",
        ],
    ),
    (
        "2",
        "2-[2-(hydroxymethyl)naphthalen-1-yl]-3,5-dimethylphenol",
        [
            "Cc1cc(C)c(-c2c(CO)ccc3ccccc23)c(O)c1 |(-5.16413,1.81269,;-3.77913,1.23669,;-2.5878,2.14814,;-1.2028,1.57215,;-0.0114718,2.48359,;-1.00913,0.0847008,;0.375875,-0.491295,;0.569548,-1.97874,;-0.62178,-2.89019,;-0.428106,-4.37763,;1.95455,-2.55474,;3.14588,-1.64329,;2.9522,-0.155843,;4.14353,0.755605,;3.94986,2.24305,;2.56486,2.81905,;1.37353,1.9076,;1.5672,0.420153,;-2.20045,-0.826747,;-2.00678,-2.31419,;-3.58546,-0.250751,),wU:5.4|",
            "Cc1cc(C)c(-c2c(CO)ccc3ccccc23)c(O)c1 |(-5.16413,1.81269,;-3.77913,1.23669,;-2.5878,2.14814,;-1.2028,1.57215,;-0.0114718,2.48359,;-1.00913,0.0847008,;0.375875,-0.491295,;0.569548,-1.97874,;-0.62178,-2.89019,;-0.428106,-4.37763,;1.95455,-2.55474,;3.14588,-1.64329,;2.9522,-0.155843,;4.14353,0.755605,;3.94986,2.24305,;2.56486,2.81905,;1.37353,1.9076,;1.5672,0.420153,;-2.20045,-0.826747,;-2.00678,-2.31419,;-3.58546,-0.250751,),wU:5.19|",
        ],
    ),
    (
        "1",
        "[1,1'-binaphthalene]-2,2'-diol",
        [
            "Oc1ccc2ccccc2c1-c1c(O)ccc2ccccc12 |(1.1711,2.43742,;-0.253568,1.96805,;-1.37239,2.96716,;-2.79706,2.49778,;-3.10291,1.0293,;-4.52758,0.559921,;-4.83343,-0.908567,;-3.7146,-1.90768,;-2.28993,-1.43831,;-1.98408,0.0301822,;-0.559413,0.499557,;0.559413,-0.499557,;0.253568,-1.96805,;-1.1711,-2.43742,;1.37239,-2.96716,;2.79706,-2.49778,;3.10291,-1.0293,;4.52758,-0.559921,;4.83343,0.908567,;3.7146,1.90768,;2.28993,1.43831,;1.98408,-0.0301822,),wU:10.10|",
            "Oc1ccc2ccccc2c1-c1c(O)ccc2ccccc12 |(1.1711,2.43742,;-0.253568,1.96805,;-1.37239,2.96716,;-2.79706,2.49778,;-3.10291,1.0293,;-4.52758,0.559921,;-4.83343,-0.908567,;-3.7146,-1.90768,;-2.28993,-1.43831,;-1.98408,0.0301822,;-0.559413,0.499557,;0.559413,-0.499557,;0.253568,-1.96805,;-1.1711,-2.43742,;1.37239,-2.96716,;2.79706,-2.49778,;3.10291,-1.0293,;4.52758,-0.559921,;4.83343,0.908567,;3.7146,1.90768,;2.28993,1.43831,;1.98408,-0.0301822,),wU:10.11|",
        ],
    ),
    (
        "1",
        "2'-hydroxy[1,1'-binaphthalen]-2-yl (1S)-2,2-dimethylcyclopropane-1-carboxylate",
        [
            "CC1(C)C[C@@H]1C(=O)Oc1ccc2ccccc2c1-c1c(O)ccc2ccccc12 |(5.20976,2.74177,;5.16707,1.24238,;6.63627,0.939978,;4.61414,-0.151988,;3.68305,1.02405,;2.28868,1.57698,;2.07035,3.06101,;1.11264,0.645891,;-0.281731,1.19882,;-0.500062,2.68285,;-1.89443,3.23578,;-3.07047,2.30469,;-4.46484,2.85762,;-5.64088,1.92653,;-5.42255,0.442505,;-4.02818,-0.110428,;-2.85214,0.820665,;-1.45777,0.267732,;-1.23944,-1.21629,;-2.41548,-2.14739,;-3.80985,-1.59445,;-2.19715,-3.63141,;-0.802779,-4.18435,;0.37326,-3.25325,;1.76763,-3.80619,;2.94367,-2.87509,;2.72534,-1.39107,;1.33097,-0.838135,;0.15493,-1.76923,),wD:4.5,wU:17.18|",
            "CC1(C)C[C@@H]1C(=O)Oc1ccc2ccccc2c1-c1c(O)ccc2ccccc12 |(5.20976,2.74177,;5.16707,1.24238,;6.63627,0.939978,;4.61414,-0.151988,;3.68305,1.02405,;2.28868,1.57698,;2.07035,3.06101,;1.11264,0.645891,;-0.281731,1.19882,;-0.500062,2.68285,;-1.89443,3.23578,;-3.07047,2.30469,;-4.46484,2.85762,;-5.64088,1.92653,;-5.42255,0.442505,;-4.02818,-0.110428,;-2.85214,0.820665,;-1.45777,0.267732,;-1.23944,-1.21629,;-2.41548,-2.14739,;-3.80985,-1.59445,;-2.19715,-3.63141,;-0.802779,-4.18435,;0.37326,-3.25325,;1.76763,-3.80619,;2.94367,-2.87509,;2.72534,-1.39107,;1.33097,-0.838135,;0.15493,-1.76923,),wD:4.5,wU:17.19|",
        ],
    ),
];

#[test]
fn axial_names_match_independent_cxsmiles_identities() {
    for (locant, body, fixtures) in INDEPENDENT_AXIAL_CASES {
        let expected = fixtures.map(|cxsmiles| {
            let state = chemistry::smiles::read(cxsmiles).unwrap().prepared.state;
            canonical_state(&state).unwrap()
        });
        assert_ne!(expected[0], expected[1], "Distinct fixture hands: {body}");
        for (descriptor, hand) in [("M", 0), ("P", 1), ("Ra", 0), ("Sa", 1)] {
            let name = format!("({locant}{descriptor})-{body}");
            let reply = worker::respond(&local::parse_request(&name).unwrap()).unwrap();
            let record = local::parse_result(&name, &reply)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(record.canonical_smiles, expected[hand], "{name}");
        }
    }
}

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
            unknown_stereo_smiles: None,
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
        unknown_stereo_smiles: None,
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
