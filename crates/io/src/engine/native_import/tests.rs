use super::*;
use crate::chemistry::smiles;

const SPHOS: &str = "COc1cccc(OC)c1-c1ccccc1P(C1CCCCC1)C1CCCCC1";
const XPHOS: &str = "CC(C)c1cc(C(C)C)c(-c2ccccc2P(C2CCCCC2)C2CCCCC2)c(C(C)C)c1";
const SPIRO_INDANONE: &str = "O=C1Cc2ccccc2C12CCC(=O)c1ccccc12";

#[test]
fn only_fully_specified_odd_cumulene_smiles_warn_about_lost_ez() -> anyhow::Result<()> {
    for (text, warned) in [
        ("C/C=C=C=C/C", true),
        (r"C/C=C=C=C\C", true),
        ("C/C=C=C=C=C=C/C", true),
        (r"C/C=C=C=C=C=C\C", true),
        ("[2H]/C=C=C=C/C", true),
        (r"[2H]/C=C=C=C\C", true),
        ("[3H]/C=C=C=C/C", true),
        ("[2H]/C=C=C=C=C=C/C", true),
        ("[2H]/C([H])=C=C=C/C", true),
        ("F/C=C=C=C/Cl", true),
        ("C/C(Cl)=C=C=C(/C)F", true),
        ("C(=C=C=C/C)/C", true),
        (r"C/C=C/C(=C=C=C/F)/C=C\C", true),
        (r"C/C=C/C(=C=C=C\F)/C=C\C", true),
        ("[H]/N=C=C=C/C", true),
        (r"[H]/N=C=C=C\C", true),
        ("C/N=C=C=C/C", true),
        ("C/C=C=C=C/C.CC", true),
        ("C/C=C=C=C/C.C/C=C=C=C/C", true),
        ("C/1=C=C=C/CCCC1", true),
        ("C/C=C=C=c1/cccc[nH]1", true),
        ("C/C=C=C=c1cccc[nH]/1", true),
        ("CC=C=C=CC", false),
        ("C/C=C=C=CC", false),
        ("CC=C=C=C/C", false),
        ("CC=C=CC", false),
        ("C/C=C/C", false),
        (r"C/C=C\C", false),
        ("C/C=C=C/C", false),
        ("C/C=C=C=C=C/C", false),
        ("C/C=C/C=C/C", false),
        ("C/C=C=C=CC.CC=C=C=C/C", false),
        ("CC=C=C=CC C/C=C=C=C/C", false),
        ("C/C(C)=C=C=C/C", false),
        ("C/C(C)=C=C=C(/C)C", false),
        ("[2H]/C([2H])=C=C=C/C", false),
        ("[1H]/C=C=C=C/C", false),
        (r"[1H]/C=C=C=C\C", false),
        ("[1H]/C([H])=C=C=C/C", false),
        ("[CH3:1]/C([CH3:2])=C=C=C/C", false),
        ("C/C=C/C(/C=C/C)=C=C=C/C=C/C", false),
    ] {
        let Preparation::Complete(prepared) = prepare(Request::import_smiles(text))? else {
            panic!("{text}: import deferred");
        };
        assert!(!prepared.document.atoms.is_empty(), "{text}");
        assert_eq!(
            prepared
                .warnings
                .iter()
                .filter(|notice| notice.as_str() == CUMULENE_EZ_NOTICE)
                .count(),
            usize::from(warned),
            "{text}: {:?}",
            prepared.warnings,
        );
    }
    Ok(())
}

#[tokio::test]
async fn cumulene_smiles_loss_warning_reaches_the_import_response() -> anyhow::Result<()> {
    for text in ["C/C=C=C=C/C", r"C/C=C=C=C\C"] {
        let Outcome::Complete(response) = execute(
            Request::import_smiles(text),
            Some(native_response::Config::in_process()),
        )
        .await?
        else {
            panic!("{text}: import deferred");
        };
        assert!(response.document.is_some());
        assert!(response.analysis.is_some());
        assert!(
            response
                .warnings
                .iter()
                .any(|notice| notice == CUMULENE_EZ_NOTICE)
        );
    }
    Ok(())
}

#[tokio::test]
async fn cx_smiles_imports_groups_and_keeps_relative_controls() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    for (text, members) in [
        ("C[C@H](O)C(=O)O |r|", 1),
        ("C[C@@H](O)C(=O)O |r|", 1),
        ("C[C@H](O)[C@@H](C)Cl |r|", 2),
        ("[H][C@](C)(O)C(=O)O |r|", 1),
        ("C[C@H](O)F.C[C@H](O)Cl |r:1|", 1),
        ("C[C@H](O)F.C[C@H](O)Cl |r:0|", 1),
        ("C[C@H](O)[C@@H](C)Cl |&1:1,3|", 2),
        ("C[C@H](O)[C@@H](C)Cl |o2:1,3|", 2),
        ("C[C@H](O)[C@H](F)Cl |r,o1:1,a:3|", 1),
        ("C[C@H](O)[C@H](F)Cl |a:1,&2:3|", 1),
        ("C[C@H](O)C(=O)O |a:1|", 0),
        ("CCO |r|", 0),
        ("CC(O)C(=O)O |r|", 0),
        ("O[C@H]1CC[C@@H](O)CC1 |r|", 0),
        ("C[C@H](O)[C@H](O)C |r|", 0),
        ("C[C@H](O)F.CCO |r:1|", 0),
    ] {
        let actual = engine
            .request(Request::import_smiles(text))
            .await
            .map_err(anyhow::Error::msg)?;
        assert!(actual.analysis.is_some(), "{text}");
        assert!(
            actual
                .warnings
                .iter()
                .all(|notice| !notice.contains("stated enantiomer")),
            "{text}: {:?}",
            actual.warnings
        );
        let document = actual.document.unwrap();
        assert_eq!(
            document
                .atoms
                .iter()
                .filter(|atom| atom.stereo_group.is_some())
                .count(),
            members,
            "{text}"
        );
        let source = smiles::read(text)?;
        let rebuilt = molecular::prepare(&document)?;
        let source_members: usize = source
            .prepared
            .state
            .metadata
            .groups
            .iter()
            .filter(|group| matches!(group.kind, 1 | 2))
            .map(|group| group.atoms.len())
            .sum();
        let rebuilt_members: usize = rebuilt
            .state
            .metadata
            .groups
            .iter()
            .map(|group| group.atoms.len())
            .sum();
        assert_eq!(source_members, rebuilt_members, "{text}");
    }
    Ok(())
}

#[tokio::test]
async fn stereo_group_bond_loss_reaches_import_response_once() -> anyhow::Result<()> {
    const BINOL: &str = include_str!("../../../../../tests/fixtures/mol-atrop-import/binol-2d.mol");
    let mut input = molfile::read(BINOL)?;
    let axis = input
        .molecule
        .state
        .metadata
        .bonds
        .iter()
        .position(|bond| matches!(bond.stereo, 6 | 7))
        .unwrap();
    input.molecule.state.metadata.groups = vec![chemistry::ranking::StereoGroup {
        kind: 2,
        bonds: vec![axis],
        read_id: 7,
        ..Default::default()
    }];
    let text = molfile::write_absolute(&input.molecule, Default::default())?;
    let prepared = smiles::Prepared {
        dummy_labels: vec![None; input.molecule.state.graph.atoms.len()],
        state: input.molecule.state,
    };
    let engine = crate::engine::LocalEngine::in_process();
    for request in [
        Request::import("mol", &text),
        Request::import_prepared(prepared),
    ] {
        let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
        assert_eq!(
            response
                .warnings
                .iter()
                .filter(|notice| notice.as_str() == chemistry::STEREO_GROUP_BONDS_NOTICE)
                .count(),
            1
        );
        assert!(
            response
                .document
                .unwrap()
                .atoms
                .iter()
                .all(|atom| atom.stereo_group.is_none())
        );
    }
    Ok(())
}

#[tokio::test]
async fn cumulene_inchi_ez_loss_uses_the_same_import_warning() -> anyhow::Result<()> {
    for (inchi, warned) in [
        ("InChI=1S/C6H8/c1-3-5-6-4-2/h3-4H,1-2H3/b4-3+", true),
        ("InChI=1S/C6H8/c1-3-5-6-4-2/h3-4H,1-2H3/b4-3-", true),
        ("InChI=1S/C6H8/c1-3-5-6-4-2/h3-4H,1-2H3", false),
        ("InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3/b4-3+", false),
    ] {
        let Outcome::Complete(response) = execute(
            Request::import("inchi", inchi),
            Some(native_response::Config::in_process()),
        )
        .await?
        else {
            panic!("{inchi}: import deferred");
        };
        assert!(response.document.is_some());
        assert!(response.analysis.is_some());
        assert_eq!(
            response
                .warnings
                .iter()
                .any(|notice| notice == CUMULENE_EZ_NOTICE),
            warned,
            "{inchi}: {:?}",
            response.warnings,
        );
    }
    Ok(())
}

#[test]
fn private_naming_smiles_import_draws_and_reperceives_biaryl_axes() -> anyhow::Result<()> {
    use crate::molecule_names::{apply_axis_markers, canonical_identity};
    for (marker, expected) in [(900_004, "M"), (900_008, "P")] {
        let text = format!("Oc1ccc2ccccc2[c:{marker}]1-[c:{marker}]1c(O)ccc2ccccc12");
        let source = apply_axis_markers(&smiles::read(&text)?.prepared.state).unwrap();
        let identity = canonical_identity(&source).unwrap();
        let Preparation::Complete(prepared) = prepare(Request::import("naming_smiles", &text))?
        else {
            panic!("Private naming import must complete natively");
        };
        assert!(
            prepared
                .document
                .bonds
                .iter()
                .any(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
        );
        assert!(prepared.document.atoms.iter().all(|atom| atom.map_num == 0));
        let reconstructed = molecular::prepare(&prepared.document)?;
        assert_eq!(canonical_identity(&reconstructed.state).unwrap(), identity);
        let assigned =
            chemistry::stereo::cip::label::assign(&reconstructed.state, &Default::default())?;
        assert_eq!(
            assigned
                .state
                .properties
                .bond_codes
                .iter()
                .filter_map(Option::as_deref)
                .filter(|code| matches!(*code, "M" | "P"))
                .collect::<Vec<_>>(),
            [expected]
        );

        let Preparation::Complete(public) = prepare(Request::import("smiles", &text))? else {
            panic!("Public SMILES import must complete natively");
        };
        assert!(
            public
                .molecule
                .state
                .metadata
                .bonds
                .iter()
                .all(|bond| bond.stereo == 0)
        );
        assert!(
            public
                .document
                .bonds
                .iter()
                .all(|bond| !matches!(bond.display.as_str(), "wedge" | "hash"))
        );
    }
    Ok(())
}

#[test]
fn private_naming_smiles_import_rejects_malformed_axis_maps() {
    for text in ["[CH3:900004][CH3:900004]", "[c:900004]1ccccc1-c1ccccc1"] {
        assert!(matches!(
            prepare(Request::import("naming_smiles", text)),
            Err(Error::AxisMarkers)
        ));
    }
}

#[test]
fn generated_unknown_alkene_depictions_roundtrip_mol_and_cdxml() -> anyhow::Result<()> {
    for (input, known) in [("CC=CCC", 0), ("C/C=C/C=CC", 1)] {
        for wavy in [false, true] {
            let imported = smiles::read(input)?;
            let mut molecule = molecule(imported.prepared.state)?;
            layout(&mut molecule, &imported.reaction_properties)?;
            // SMILES cannot say "unknown"; mark the unspecified double as such.
            let (graph, metadata) = (&molecule.state.graph, &mut molecule.state.metadata);
            for (bond, meta) in graph.bonds.iter().zip(&mut metadata.bonds) {
                if bond.order == 2 && meta.stereo == 0 {
                    meta.stereo = 1;
                }
            }
            let options = molecular::DrawingOptions {
                wavy_unknown_alkenes: wavy,
                ..Default::default()
            };
            let drawing =
                molecular::for_generated(&molecule, &imported.prepared.dummy_labels, options)?;
            let labels = drawing.labels()?;
            let document = drawing.finish(labels)?;
            assert_eq!(
                document
                    .bonds
                    .iter()
                    .filter(|b| b.order == 1 && b.display == "wavy")
                    .count(),
                usize::from(wavy)
            );
            assert_eq!(
                document
                    .bonds
                    .iter()
                    .filter(|b| b.order == 2 && b.display == "wavy")
                    .count(),
                usize::from(!wavy)
            );
            let molecule = molecular::prepare(&document)?;
            assert_eq!(
                molecule
                    .state
                    .metadata
                    .bonds
                    .iter()
                    .filter(|b| b.stereo == 1)
                    .count(),
                1
            );
            assert_eq!(
                molecule
                    .state
                    .metadata
                    .bonds
                    .iter()
                    .filter(|b| b.stereo > 1)
                    .count(),
                known
            );
            for force_v3000 in [false, true] {
                let (text, fallback) = molfile::write_drawing_absolute(
                    &molecule,
                    &document,
                    molfile::Options { force_v3000 },
                )?;
                assert!(!fallback);
                let imported = molfile::read(&text)?;
                assert_eq!(
                    imported
                        .molecule
                        .state
                        .metadata
                        .bonds
                        .iter()
                        .filter(|b| b.stereo == 1)
                        .count(),
                    1,
                    "input={input}, wavy={wavy}, v3000={force_v3000}: {text}"
                );
                assert_eq!(
                    imported
                        .molecule
                        .state
                        .metadata
                        .bonds
                        .iter()
                        .filter(|b| b.stereo > 1)
                        .count(),
                    known
                );
            }
            let xml = crate::exchange::drawing::write(&document, Default::default())?;
            let imported = cdxml::import_cdxml(&xml)?;
            assert_eq!(
                molecular::prepare(&imported.document)?
                    .state
                    .metadata
                    .bonds
                    .iter()
                    .filter(|b| b.stereo == 1)
                    .count(),
                1
            );
        }
    }
    Ok(())
}

const MOLECULES: &str = "\
ibuprofen CC(C)Cc1ccc(cc1)C(C)C(=O)O
aspirin CC(=O)Oc1ccccc1C(=O)O
paracetamol CC(=O)Nc1ccc(O)cc1
caffeine Cn1cnc2c1c(=O)n(C)c(=O)n2C
toluene Cc1ccccc1
benzoic_acid OC(=O)c1ccccc1
ethyl_cinnamate CCOC(=O)/C=C/c1ccccc1
stearic_acid CCCCCCCCCCCCCCCCCC(=O)O
naproxen COc1ccc2cc(ccc2c1)[C@H](C)C(=O)O
biphenyl c1ccc(cc1)-c1ccccc1
tyrosine N[C@@H](Cc1ccc(O)cc1)C(=O)O
menthol CC(C)[C@@H]1CC[C@@H](C)C[C@H]1O
cholesterol C[C@H](CCCC(C)C)[C@H]1CC[C@@H]2[C@@]1(CC[C@H]3[C@H]2CC=C4[C@@]3(CC[C@@H](C4)O)C)C
diphenhydramine CN(C)CCOC(c1ccccc1)c1ccccc1
lidocaine CCN(CC)CC(=O)Nc1c(C)cccc1C
glucose OC[C@H]1OC(O)[C@H](O)[C@@H](O)[C@@H]1O";

fn grid_deviations(state: &State, points: &[Point3]) -> Vec<f64> {
    state
        .graph
        .bonds
        .iter()
        .filter(|bond| {
            !state.rings.atoms.iter().any(|ring| {
                ring.len() != 6
                    && ring
                        .iter()
                        .zip(ring.iter().cycle().skip(1))
                        .take(ring.len())
                        .any(|(&a, &b)| {
                            (a == bond.a && b == bond.b) || (a == bond.b && b == bond.a)
                        })
            })
        })
        .map(|bond| {
            let (a, b) = (points[bond.a], points[bond.b]);
            let angle = (b.y - a.y).atan2(b.x - a.x).to_degrees().rem_euclid(30.);
            angle.min(30. - angle)
        })
        .collect()
}

pub(super) fn assert_rigid_rotation(before: &[Point3], after: &[Point3]) {
    assert_eq!(before.len(), after.len());
    let center = Point3 {
        x: before.iter().map(|p| p.x).sum::<f64>() / before.len() as f64,
        y: before.iter().map(|p| p.y).sum::<f64>() / before.len() as f64,
        z: 0.,
    };
    let (&a, &b) = before
        .iter()
        .zip(after)
        .find(|(a, _)| (a.x - center.x).hypot(a.y - center.y) > 1e-6)
        .unwrap();
    let (ax, ay, bx, by) = (
        a.x - center.x,
        a.y - center.y,
        b.x - center.x,
        b.y - center.y,
    );
    let angle = (ax * by - ay * bx).atan2(ax * bx + ay * by);
    assert!(angle.abs() <= 15_f64.to_radians() + 1e-9);
    let (sin, cos) = angle.sin_cos();
    for (a, b) in before.iter().zip(after) {
        let (x, y) = (a.x - center.x, a.y - center.y);
        let (x, y) = (center.x + cos * x - sin * y, center.y + sin * x + cos * y);
        assert!((x - b.x).hypot(y - b.y) < 1e-10);
        assert_eq!(a.z, b.z);
    }
}

#[test]
fn common_native_imports_straighten_six_rings_and_chains() -> anyhow::Result<()> {
    for entry in MOLECULES.lines() {
        let (name, text) = entry.split_once(' ').unwrap();
        let Preparation::Complete(prepared) = prepare(Request::import("smiles", text))? else {
            panic!("{name}: import deferred");
        };
        let molecule = prepared.molecule;
        let deviations = grid_deviations(&molecule.state, &molecule.positions);
        let fitted = deviations
            .iter()
            .filter(|&&angle| angle <= 1.5 + 1e-7)
            .count();
        // Cholesterol's fused rings leave some bonds off the grid; the
        // rotation still brings most of them onto it.
        let expected = match name {
            "cholesterol" => deviations.len() / 2 + 1,
            // One caffeine bond is distorted by its fused five-ring geometry.
            "caffeine" => deviations.len() - 1,
            _ => deviations.len(),
        };
        assert!(
            fitted >= expected,
            "{name}: {fitted}/{} on grid: {deviations:?}",
            deviations.len()
        );
        if name == "paracetamol" {
            let ring = molecule
                .state
                .rings
                .atoms
                .iter()
                .find(|ring| ring.len() == 6)
                .unwrap();
            let vertical = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .take(6)
                .filter(|&(&a, &b)| {
                    (molecule.positions[a].x - molecule.positions[b].x).abs() < 1e-7
                })
                .count();
            assert_eq!(vertical, 2, "paracetamol's ring sides must be vertical");
        }
    }
    Ok(())
}

#[test]
fn disconnected_imports_turn_as_one_body() -> anyhow::Result<()> {
    // Rotating each component alone would make these two overlap.
    let input = smiles::read("CCOC(=O)/C=C/c1ccccc1.CCCC(CCCC)CCCC")?;
    let state = input.prepared.state;
    let original = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    // This regression belongs to the bounded straightening stage. Fresh
    // imports subsequently orient and repack disconnected components.
    let mut positions = original.clone();
    chemistry::layout_refinement::straighten(&state, &mut positions, None);
    assert_ne!(positions, original);
    assert_rigid_rotation(&original, &positions);
    Ok(())
}

fn imported_smiles(text: &str) -> anyhow::Result<Box<Prepared>> {
    let Preparation::Complete(prepared) = prepare(Request::import("smiles", text))? else {
        anyhow::bail!("{text}: import deferred");
    };
    Ok(prepared)
}

#[tokio::test]
async fn imported_butenes_retain_specified_or_unspecified_stereo_through_exports()
-> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    const UNSPECIFIED_INCHI: &str = "InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3";
    for (format, text, expected_smiles, expected_inchi, unspecified) in [
        ("smiles", "CC=CC", "CC=CC", UNSPECIFIED_INCHI, true),
        (
            "smiles",
            "C/C=C/C",
            "C/C=C/C",
            "InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3/b4-3+",
            false,
        ),
        (
            "smiles",
            "C/C=C\\C",
            "C/C=C\\C",
            "InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3/b4-3-",
            false,
        ),
        ("inchi", UNSPECIFIED_INCHI, "CC=CC", UNSPECIFIED_INCHI, true),
    ] {
        let imported = engine
            .request(Request::import(format, text))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = imported.analysis.unwrap();
        assert_eq!(analysis.smiles, expected_smiles, "{format}/{text}");
        assert_eq!(analysis.inchi, expected_inchi, "{format}/{text}");
        let mut document = imported.document.unwrap();
        if unspecified {
            let double = document.bonds.iter().find(|bond| bond.order == 2).unwrap();
            assert!(double.stereo_authoritative, "{format}/{text}");
            assert!(double.stereo.is_none(), "{format}/{text}");
            // Use visibly different planar coordinates so a fresh depiction
            // cannot accidentally satisfy the coordinate preservation check.
            for (atom, (x, y)) in
                document
                    .atoms
                    .iter_mut()
                    .zip([(11., 17.), (67., 3.), (121., 3.), (173., -39.)])
            {
                atom.position = crate::document::Point::new(x, y);
            }
        }
        let before = document.clone();
        for output_format in ["smiles", "inchi", "mol"] {
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(output_format.into());
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            let analysis = response.analysis.unwrap();
            assert_eq!(
                analysis.smiles, expected_smiles,
                "{format}/{text}/{output_format}"
            );
            assert_eq!(
                analysis.inchi, expected_inchi,
                "{format}/{text}/{output_format}"
            );
            let output = response.output.unwrap();
            match output_format {
                "smiles" => assert_eq!(output.trim(), expected_smiles),
                "inchi" => assert_eq!(output.trim(), expected_inchi),
                "mol" => {
                    assert_eq!(
                        v2000_double_flags(&output),
                        [if unspecified { 3 } else { 0 }]
                    );
                    let reopened = molfile::read(&output)?;
                    assert_eq!(
                        smiles::write::write(&reopened.molecule.state, Default::default())?.text,
                        expected_smiles,
                        "{format}/{text}: MOL roundtrip"
                    );
                    if unspecified {
                        let prepared = molecular::prepare(&document)?;
                        for (original, written) in
                            prepared.positions.iter().zip(&reopened.molecule.positions)
                        {
                            assert!(
                                (original.x - written.x).abs() < 1e-4,
                                "{format}/{text}: x changed"
                            );
                            assert!(
                                (original.y - written.y).abs() < 1e-4,
                                "{format}/{text}: y changed"
                            );
                            assert_eq!(written.z, 0.);
                        }
                        assert_eq!(
                            reopened
                                .molecule
                                .state
                                .metadata
                                .bonds
                                .iter()
                                .filter(|bond| bond.stereo == 1)
                                .count(),
                            1
                        );
                    }
                    let reopened = engine
                        .request(Request::import("mol", &output))
                        .await
                        .map_err(anyhow::Error::msg)?;
                    let analysis = reopened.analysis.unwrap();
                    assert_eq!(
                        analysis.smiles, expected_smiles,
                        "{format}/{text}: reopened MOL"
                    );
                    assert_eq!(
                        analysis.inchi, expected_inchi,
                        "{format}/{text}: reopened MOL"
                    );
                }
                _ => unreachable!(),
            }
            if unspecified {
                assert_eq!(response.document.unwrap(), before);
            }
        }
        assert_eq!(document, before);
    }
    Ok(())
}

// RDKit's MolToInchi reads a MOL file's own XYZ; expected values are its output
// (2026.03.6). The first input is RDKit's C[C@H]1CC[C@H](C)CC1 with out-of-plane Z.
#[tokio::test]
async fn mol_imports_give_inchi_their_own_xyz() -> anyhow::Result<()> {
    const RING: &str = "
     RDKit          2D

  8  8  0  0  0  0  0  0  0  0999 V2000
    3.0000    0.0000   -0.4000 C   0  0  0  0  0  0  0  0  0  0  0  0
    1.5000   -0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.7500   -1.2990    0.4000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.7500   -1.2990   -0.4000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -1.5000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -3.0000    0.0000    0.4000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.7500    1.2990   -0.4000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.7500    1.2990    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
  2  1  1  1
  2  3  1  0
  3  4  1  0
  4  5  1  0
  5  6  1  6
  5  7  1  0
  7  8  1  0
  8  2  1  0
M  END
";
    let butene = |stereo: u8| {
        format!(
            "
     RDKit          3D

  4  3  0  0  0  0  0  0  0  0999 V2000
    1.8942    0.1555   -0.0535 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.4521    0.2141   -0.4705 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.4317   -0.2179    0.4371 C   0  0  0  0  0  0  0  0  0  0  0  0
   -1.8978   -0.1863    0.0827 C   0  0  0  0  0  0  0  0  0  0  0  0
  1  2  1  0
  2  3  2  {stereo}
  3  4  1  0
M  END
"
        )
    };
    // A 3D header with flat coordinates is still the file's own geometry.
    const CUMULENE: &str = "
     RDKit          3D

  6  5  0  0  0  0  0  0  0  0999 V2000
   -1.0000    1.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    1.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    2.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    3.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    4.0000   -1.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
  1  2  1  0
  2  3  2  0
  3  4  2  0
  4  5  2  0
  5  6  1  0
M  END
";
    let engine = crate::engine::LocalEngine::in_process();
    for (text, expected) in [
        (
            CUMULENE.to_owned(),
            "InChI=1S/C6H8/c1-3-5-6-4-2/h3-4H,1-2H3/b4-3+",
        ),
        (
            RING.to_owned(),
            "InChI=1S/C8H16/c1-7-3-5-8(2)6-4-7/h7-8H,3-6H2,1-2H3/t7-,8?",
        ),
        (butene(0), "InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3/b4-3+"),
        // A crossed double bond stays unspecified with real XYZ too.
        (butene(3), "InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3"),
    ] {
        let imported = engine
            .request(Request::import("mol", &text))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(imported.analysis.unwrap().inchi, expected, "{text}");
    }
    Ok(())
}

const UNSPECIFIED_TRIENE_INCHI: &str = "InChI=1S/C6H8/c1-3-5-6-4-2/h3-4H,1-2H3";

#[tokio::test]
async fn generated_cumulenes_do_not_acquire_inchi_stereo_from_layout() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    for (format, text, expected_inchi, double_count) in [
        ("smiles", "CC=C=C=CC", UNSPECIFIED_TRIENE_INCHI, 3),
        ("smiles", "C/C=C=C=C/C", UNSPECIFIED_TRIENE_INCHI, 3),
        ("smiles", "C/C=C=C=C\\C", UNSPECIFIED_TRIENE_INCHI, 3),
        (
            "smiles",
            "CC=C=C=C=C=CC",
            "InChI=1S/C8H8/c1-3-5-7-8-6-4-2/h3-4H,1-2H3",
            5,
        ),
        (
            "inchi",
            "InChI=1S/C6H8/c1-3-5-6-4-2/h3-4H,1-2H3/b4-3+",
            UNSPECIFIED_TRIENE_INCHI,
            3,
        ),
    ] {
        let imported = engine
            .request(Request::import(format, text))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(imported.analysis.unwrap().inchi, expected_inchi, "{text}");
        if format == "inchi" {
            assert_eq!(
                imported.warnings,
                ["Extended double-bond stereochemistry (e.g. C=C=C=C) ignored"]
            );
        }
        let document = imported.document.unwrap();
        document.validate().map_err(anyhow::Error::msg)?;
        assert!(molfile::has_projected_coordinates(&document), "{text}");
        assert_eq!(
            document
                .bonds
                .iter()
                .filter(|bond| bond.stereo_authoritative)
                .count(),
            double_count,
            "{text}"
        );
        for bond in &document.bonds {
            assert_eq!(bond.stereo_authoritative, bond.order == 2, "{text}");
            assert!(bond.stereo.is_none(), "{text}");
            assert!(bond.stereo_atoms.is_empty(), "{text}");
            assert_eq!(bond.display, "plain", "{text}");
        }
        assert!(
            chemistry::stereo_lint::check(&document, true).is_empty(),
            "{text}: Check drawing changed"
        );
        for operation in ["analyze", "export"] {
            let mut request = Request::molecule(operation, document.clone());
            if operation == "export" {
                request.format = Some("inchi".into());
            }
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            assert_eq!(
                response.analysis.unwrap().inchi,
                expected_inchi,
                "{text}/{operation}"
            );
            if operation == "export" {
                assert_eq!(response.output.unwrap().trim(), expected_inchi, "{text}");
            }
            assert_eq!(response.document.unwrap(), document, "{text}/{operation}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn generated_allenes_and_terminal_cumulenes_keep_their_inchi() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    // Exact controls recorded with the pre-fix CLI and its InChI helper.
    for (text, expected_inchi) in [
        ("CC=C=CC", "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3"),
        (
            "CC=[C@AL1]=CC",
            "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m0/s1",
        ),
        ("C=C=C=CC", "InChI=1S/C5H6/c1-3-5-4-2/h4H,1H2,2H3"),
    ] {
        let imported = engine
            .request(Request::import("smiles", text))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(imported.analysis.unwrap().inchi, expected_inchi, "{text}");
        let document = imported.document.unwrap();
        assert!(
            document.bonds.iter().all(|bond| !bond.stereo_authoritative),
            "{text}"
        );
        assert!(!molfile::has_projected_coordinates(&document), "{text}");
        for operation in ["analyze", "export"] {
            let mut request = Request::molecule(operation, document.clone());
            if operation == "export" {
                request.format = Some("inchi".into());
            }
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            assert_eq!(
                response.analysis.unwrap().inchi,
                expected_inchi,
                "{text}/{operation}"
            );
            if operation == "export" {
                assert_eq!(response.output.unwrap().trim(), expected_inchi, "{text}");
            }
        }
    }
    Ok(())
}

fn v2000_double_flags(text: &str) -> Vec<u8> {
    let counts = text.lines().nth(3).unwrap();
    assert!(counts.ends_with("V2000"), "{text}");
    let atoms = counts[..3].trim().parse::<usize>().unwrap();
    let bonds = counts[3..6].trim().parse::<usize>().unwrap();
    text.lines()
        .skip(4 + atoms)
        .take(bonds)
        .filter(|line| line[6..9].trim() == "2")
        .map(|line| line[9..12].trim().parse().unwrap())
        .collect()
}

fn assert_mol_positions(document: &Document, mol: &str) -> anyhow::Result<()> {
    let before = molecular::prepare(document)?;
    let reopened = molfile::read(mol)?;
    assert_eq!(before.positions.len(), reopened.molecule.positions.len());
    for (before, after) in before.positions.iter().zip(&reopened.molecule.positions) {
        assert!((before.x - after.x).abs() < 1e-4);
        assert!((before.y - after.y).abs() < 1e-4);
        assert_eq!(after.z, 0.);
    }
    Ok(())
}

#[tokio::test]
async fn authoritative_unspecified_cumulene_mol_export_preserves_drawing_coordinates()
-> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    let imported = engine
        .request(Request::import("smiles", "CC=C=C=CC"))
        .await
        .map_err(anyhow::Error::msg)?;
    let mut document = imported.document.unwrap();
    for (atom, (x, y)) in document.atoms.iter_mut().zip([
        (-41., -23.),
        (7., 17.),
        (43., 17.),
        (79., 17.),
        (115., 17.),
        (139., 53.),
    ]) {
        atom.position = crate::document::Point::new(x, y);
    }
    assert!(
        document
            .bonds
            .iter()
            .filter(|bond| bond.order == 2)
            .all(|bond| { bond.stereo_authoritative && bond.stereo.is_none() })
    );
    let mut request = Request::molecule("export", document.clone());
    request.format = Some("mol".into());
    let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
    assert_eq!(response.analysis.unwrap().inchi, UNSPECIFIED_TRIENE_INCHI);
    assert_eq!(response.document.unwrap(), document);
    let output = response.output.unwrap();
    // Cumulene doubles do not get the ordinary alkene ANY flag.
    assert_eq!(v2000_double_flags(&output), [0, 0, 0]);
    assert_mol_positions(&document, &output)?;
    Ok(())
}

#[tokio::test]
async fn drawn_and_mol_cumulenes_retain_coordinate_defined_inchi_stereo() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    for (trans, layer) in [(true, "/b4-3+"), (false, "/b4-3-")] {
        let mut document = Document::default();
        let atoms = [
            (-20., -20.),
            (0., 0.),
            (30., 0.),
            (60., 0.),
            (90., 0.),
            (110., if trans { 20. } else { -20. }),
        ]
        .map(|(x, y)| document.add_atom("C", crate::document::Point::new(x, y)));
        for (pair, order) in atoms.windows(2).zip([1, 2, 2, 2, 1]) {
            document.add_bond(pair[0], pair[1], order, "plain");
        }
        document.validate().map_err(anyhow::Error::msg)?;
        assert!(!molfile::has_projected_coordinates(&document));
        let expected_inchi = format!("{UNSPECIFIED_TRIENE_INCHI}{layer}");
        let analyzed = engine
            .request(Request::molecule("analyze", document.clone()))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(analyzed.analysis.unwrap().inchi, expected_inchi);
        let mut request = Request::molecule("export", document.clone());
        request.format = Some("mol".into());
        let exported = engine.request(request).await.map_err(anyhow::Error::msg)?;
        assert_eq!(exported.analysis.unwrap().inchi, expected_inchi);
        let mol = exported.output.unwrap();
        assert_eq!(v2000_double_flags(&mol), [0, 0, 0]);
        assert_mol_positions(&document, &mol)?;
        let imported = engine
            .request(Request::import("mol", &mol))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(imported.analysis.unwrap().inchi, expected_inchi);
        let document = imported.document.unwrap();
        assert!(document.bonds.iter().all(|bond| !bond.stereo_authoritative));
        assert!(!molfile::has_projected_coordinates(&document));
        for operation in ["analyze", "export"] {
            let mut request = Request::molecule(operation, document.clone());
            if operation == "export" {
                request.format = Some("inchi".into());
            }
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            assert_eq!(response.analysis.unwrap().inchi, expected_inchi);
            if operation == "export" {
                assert_eq!(response.output.unwrap().trim(), expected_inchi);
            }
        }
    }
    Ok(())
}

fn document_atom(document: &Document, id: u64) -> &crate::document::Atom {
    document.atoms.iter().find(|atom| atom.id == id).unwrap()
}

fn average_document_bond_length(document: &Document) -> f32 {
    document
        .bonds
        .iter()
        .map(|bond| {
            document_atom(document, bond.a)
                .position
                .distance(document_atom(document, bond.b).position)
        })
        .sum::<f32>()
        / document.bonds.len() as f32
}

#[test]
fn explicit_mol_and_cdxml_imports_preserve_nonconventional_coordinates() -> anyhow::Result<()> {
    let input = smiles::read("[Na+].CC(=O)[O-]")?;
    let mut molecule = molecule(input.prepared.state)?;
    // Draw the carboxylate on the left, its carbonyl oxygen down, and sodium
    // far to the left of the entire acetate. Fresh-layout refinements would
    // change both the molecular orientation and the component placement.
    molecule.positions = [
        (-6., 2.),
        (2., 0.),
        (0.5, 0.),
        (-0.25, -1.299),
        (-0.25, 1.299),
    ]
    .map(|(x, y)| Point3 { x, y, z: 0. })
    .to_vec();
    let source = finish_molecule(
        molecule,
        &input.prepared.dummy_labels,
        Default::default(),
        &ComponentRow::default(),
    )?;
    let (mol, fallback) =
        molfile::write_drawing_absolute(&source.molecule, &source.document, Default::default())?;
    assert!(!fallback);
    let cdxml = crate::exchange::drawing::write(&source.document, Default::default())?;
    for (format, text) in [("mol", mol), ("cdxml", cdxml)] {
        let Preparation::Complete(imported) = prepare(Request::import(format, &text))? else {
            anyhow::bail!("{format}: explicit-coordinate import deferred");
        };
        assert_eq!(imported.document.atoms.len(), source.document.atoms.len());
        let before_origin = source.document.atoms[0].position;
        let after_origin = imported.document.atoms[0].position;
        for (before, after) in source.document.atoms.iter().zip(&imported.document.atoms) {
            assert_eq!(
                (&before.element, before.charge),
                (&after.element, after.charge),
                "{format}"
            );
            // CDXML export adds a page-margin translation. Comparing every
            // atom relative to sodium retains the exact drawing geometry and
            // counterion placement while allowing that shared translation.
            let dx = (before.position.x - before_origin.x) - (after.position.x - after_origin.x);
            let dy = (before.position.y - before_origin.y) - (after.position.y - after_origin.y);
            assert!(
                dx.hypot(dy) < 0.003,
                "{format}: atom {} moved by ({dx}, {dy}) relative to sodium",
                before.id
            );
        }
    }
    Ok(())
}

#[test]
fn imported_component_rows_leave_a_bond_length_between_label_edges() -> anyhow::Result<()> {
    for text in [
        "C1C=CCC2C1CNC2.Cl",
        "C1CCNCC1.Cl",
        "C1CCNCC1.OC(=O)C(F)(F)F",
    ] {
        let prepared = imported_smiles(text)?;
        let doc = &prepared.document;
        let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
        let mut bounds: Vec<_> = crate::reactions::molecules(doc, &ids)
            .iter()
            .map(|ids| {
                doc.atoms
                    .iter()
                    .filter(|a| ids.contains(&a.id))
                    .map(|a| (a.position, a.position))
                    .chain(crate::scene::participant_label_ink_boxes(doc, ids))
                    .fold(
                        (f32::INFINITY, f32::NEG_INFINITY),
                        |(left, right), (lo, hi)| (left.min(lo.x), right.max(hi.x)),
                    )
            })
            .collect();
        bounds.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(bounds.len(), 2, "{text}");
        let gap = bounds[1].0 - bounds[0].1;
        let length = average_document_bond_length(doc);
        assert!(gap >= length - 1e-4, "{text}: label gap {gap} < {length}");
    }
    Ok(())
}

#[test]
fn sodium_benzoate_import_places_counterion_right_at_one_bond_length() -> anyhow::Result<()> {
    for text in ["[Na+].[O-]C(=O)c1ccccc1", "[O-]C(=O)c1ccccc1.[Na+]"] {
        let prepared = imported_smiles(text)?;
        let document = &prepared.document;
        let sodium = document
            .atoms
            .iter()
            .find(|atom| atom.element == "Na")
            .unwrap();
        let oxygen = document
            .atoms
            .iter()
            .find(|atom| atom.element == "O" && atom.charge == -1)
            .unwrap();
        assert!(
            document
                .atoms
                .iter()
                .filter(|atom| atom.id != sodium.id)
                .all(|atom| atom.position.x < sodium.position.x),
            "{text}: the entire benzoate must be left of sodium"
        );
        let separation = sodium.position.distance(oxygen.position);
        let bond_length = average_document_bond_length(document);
        assert!(
            (separation - bond_length).abs() < bond_length * 1e-4,
            "{text}: Na–O− separation {separation} must equal a real bond length {bond_length}"
        );
    }
    Ok(())
}

#[test]
fn counterion_is_not_placed_across_a_bond() -> anyhow::Result<()> {
    // The amide N's free bisector points into the bicycle, where the spot one
    // bond length away clears every atom but lies on a ring bond.
    let prepared = imported_smiles("[Na+].[N-]1CC2CCC1CC2")?;
    let document = &prepared.document;
    let sodium = document
        .atoms
        .iter()
        .find(|atom| atom.element == "Na")
        .unwrap()
        .position;
    let length = average_document_bond_length(document);
    for bond in &document.bonds {
        let (a, b) = (
            document_atom(document, bond.a).position,
            document_atom(document, bond.b).position,
        );
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let t =
            (((sodium.x - a.x) * dx + (sodium.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
        assert!(
            (a.x + t * dx - sodium.x).hypot(a.y + t * dy - sodium.y) >= length * 0.5,
            "Na+ sits on bond {}–{}",
            bond.a,
            bond.b
        );
    }
    Ok(())
}

#[test]
fn disconnected_imports_put_larger_components_first_then_cations_on_ties() -> anyhow::Result<()> {
    // The size rule takes precedence for a neutral acid and for an organic
    // cation whose small chloride counterion appeared first in the input.
    for (text, small_element) in [
        ("OC(=O)c1ccccc1.[Na+]", "Na"),
        ("[Na+].OC(=O)c1ccccc1", "Na"),
        ("[Cl-].C[N+](C)(C)C", "Cl"),
        ("C[N+](C)(C)C.[Cl-]", "Cl"),
    ] {
        let prepared = imported_smiles(text)?;
        let small = prepared
            .document
            .atoms
            .iter()
            .find(|atom| atom.element == small_element)
            .unwrap();
        assert!(
            prepared
                .document
                .atoms
                .iter()
                .filter(|atom| atom.id != small.id)
                .all(|atom| atom.position.x < small.position.x),
            "{text}: the larger component must be entirely left of {small_element}"
        );
    }
    for text in ["[Na+].[Cl-]", "[Cl-].[Na+]"] {
        let prepared = imported_smiles(text)?;
        let atoms = &prepared.document.atoms;
        let sodium = atoms.iter().find(|atom| atom.element == "Na").unwrap();
        let chloride = atoms.iter().find(|atom| atom.element == "Cl").unwrap();
        assert!(sodium.position.x < chloride.position.x, "{text}");
        assert!(
            (sodium.position.y - chloride.position.y).abs() < 1e-5,
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn ethyl_benzoate_import_orients_ring_bottom_left_and_carbonyl_up() -> anyhow::Result<()> {
    let prepared = imported_smiles("CCOC(=O)c1ccccc1")?;
    let document = &prepared.document;
    let molecule = &prepared.molecule;
    let ring = molecule
        .state
        .rings
        .atoms
        .iter()
        .find(|ring| ring.len() == 6)
        .unwrap();
    let ring_center = ring.iter().fold((0., 0.), |(x, y), &index| {
        let point = document_atom(document, molecule.ids[index]).position;
        (x + point.x, y + point.y)
    });
    let ring_center = (
        ring_center.0 / ring.len() as f32,
        ring_center.1 / ring.len() as f32,
    );
    let oxygens: Vec<_> = document
        .atoms
        .iter()
        .filter(|atom| atom.element == "O")
        .collect();
    let oxygen_center = (
        oxygens.iter().map(|atom| atom.position.x).sum::<f32>() / oxygens.len() as f32,
        oxygens.iter().map(|atom| atom.position.y).sum::<f32>() / oxygens.len() as f32,
    );
    let molecule_y = document
        .atoms
        .iter()
        .map(|atom| atom.position.y)
        .sum::<f32>()
        / document.atoms.len() as f32;
    assert!(
        ring_center.0 < oxygen_center.0,
        "ring must be left of the ester oxygens"
    );
    // Document coordinates increase downwards, opposite to depiction's y axis.
    assert!(
        ring_center.1 > oxygen_center.1,
        "ring must be visually below the ester oxygens"
    );
    assert!(
        ring_center.1 > molecule_y,
        "ring must be visually below the molecule centroid"
    );
    let carbonyl = document
        .bonds
        .iter()
        .find(|bond| {
            bond.order == 2
                && [bond.a, bond.b]
                    .iter()
                    .any(|&id| document_atom(document, id).element == "O")
        })
        .unwrap();
    let (a, b) = (
        document_atom(document, carbonyl.a),
        document_atom(document, carbonyl.b),
    );
    let (oxygen, carbon) = if a.element == "O" { (a, b) } else { (b, a) };
    assert!(
        oxygen.position.y < carbon.position.y,
        "carbonyl oxygen must be visually above carbon"
    );
    Ok(())
}

#[test]
fn conventionally_reoriented_chiral_import_keeps_visible_stereochemistry() -> anyhow::Result<()> {
    let mut selected_reflection = false;
    for text in ["C[C@H](N)C(=O)O", "O=C(O)[C@@H](O)c1ccccc1"] {
        let input = smiles::read(text)?;
        let state = &input.prepared.state;
        let expected = smiles::write::write(state, Default::default())?.text;
        // Reproduce the import pipeline through straighten, immediately before
        // conventional orientation, rather than comparing to raw depiction.
        let ranks: Vec<_> = input
            .reaction_properties
            .iter()
            .map(|pairs| depict::ranks::AtomProperties::from_pairs(pairs))
            .collect();
        let original =
            depict::compute_with_rank_properties(state, &ranks, None, Default::default())?;
        let points = template_retry::improve(state, &ranks, original.positions, 1.5);
        let mut before = chain_layout::improve(state, &ranks, points).positions;
        chemistry::layout_refinement::straighten(state, &mut before, None);
        let prepared = imported_smiles(text)?;
        let after = &prepared.molecule.positions;
        // A non-collinear triple distinguishes a reflection from translation
        // or a 180-degree rotation, without assuming which atom owns a wedge.
        let triple = (1..before.len())
            .find_map(|a| {
                (a + 1..before.len()).find_map(|b| {
                    (cross(&before[0], &before[a], &before[b]).abs() > 1e-6).then_some((a, b))
                })
            })
            .unwrap();
        let (a, b) = triple;
        selected_reflection |=
            cross(&before[0], &before[a], &before[b]) * cross(&after[0], &after[a], &after[b]) < 0.;
        assert_eq!(
            smiles::write::write(
                &molecular::prepare(&prepared.document)?.state,
                Default::default()
            )?
            .text,
            expected,
            "{text}: generated document stereochemistry"
        );
        let mut visible = prepared.document;
        for atom in &mut visible.atoms {
            atom.stereo = None;
        }
        for bond in &mut visible.bonds {
            bond.stereo = None;
            bond.stereo_atoms.clear();
        }
        assert!(
            visible
                .bonds
                .iter()
                .any(|bond| matches!(bond.display.as_str(), "wedge" | "hash")),
            "{text}: the generated drawing must display its stereocenter"
        );
        assert_eq!(
            smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?.text,
            expected,
            "{text}: visible wedges/hash must preserve stereo after reorientation"
        );
    }
    assert!(
        selected_reflection,
        "the regression must exercise an actually selected reflection after straighten"
    );
    Ok(())
}

fn cross(o: &Point3, a: &Point3, b: &Point3) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// CX wedge flags carry configuration only at the coordinates they are read
/// against: the input coordinates when given, else the fresh layout, which
/// conventional orientation must then not mirror (#1037).
const WEDGE_FLAGGED_RING: &str = "C1[C@@H](F)CC[C@H]1O |(0,-1.03088,;1.21353,-0.149206,;2.64011,-0.612731,;0.75,1.27738,;-0.75,1.27738,;-1.21353,-0.149206,;-2.64011,-0.612731,),wD:1.1,5.6|";
/// The same stereoisomer drawn mirrored in y, with wedges instead of hashes.
const MIRRORED_WEDGE_FLAGGED_RING: &str = "C1[C@@H](F)CC[C@H]1O |(0,1.03088,;1.21353,0.149206,;2.64011,0.612731,;0.75,-1.27738,;-0.75,-1.27738,;-1.21353,0.149206,;-2.64011,0.612731,),wU:1.1,5.6|";

#[test]
fn wedge_flagged_smiles_import_keeps_the_stated_enantiomer() -> anyhow::Result<()> {
    let stated = smiles::write::write(
        &smiles::read("O[C@@H]1CC[C@H](F)C1")?.prepared.state,
        Default::default(),
    )?
    .text;
    for text in [
        WEDGE_FLAGGED_RING,
        MIRRORED_WEDGE_FLAGGED_RING,
        "O[C@@H]1CC[C@H](F)C1 |(2.64011,-0.612731,;1.21353,-0.149206,;0.75,1.27738,;-0.75,1.27738,;-1.21353,-0.149206,;-2.64011,-0.612731,;0,-1.03088,),wU:1.0,4.4|",
    ] {
        let mut visible = imported_smiles(text)?.document;
        for atom in &mut visible.atoms {
            atom.stereo = None;
        }
        assert_eq!(
            smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?.text,
            stated,
            "{text}: visible wedges must keep the stated enantiomer"
        );
    }
    Ok(())
}

#[tokio::test]
async fn wedge_flagged_smiles_import_keeps_stated_stereo_identifiers() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    const RING: &str = "InChI=1S/C5H9FO/c6-4-1-2-5(7)3-4/h4-5,7H,1-3H2";
    for (text, inchi) in [
        // Without coordinates the @/@@ tags state m0 whatever the flag (#1044).
        ("C1[C@@H](F)CC[C@H]1O |wU:1.0|", "/t4-,5+/m0/s1"),
        ("C1[C@@H](F)CC[C@H]1O |wD:1.0|", "/t4-,5+/m0/s1"),
        // With coordinates, tags, wedges and coordinates all state m0.
        (WEDGE_FLAGGED_RING, "/t4-,5+/m0/s1"),
        (MIRRORED_WEDGE_FLAGGED_RING, "/t4-,5+/m0/s1"),
        // With 3D coordinates the flags are again read on the fresh layout.
        (
            "O[C@@H]1CC[C@H](F)C1 |(2.64011,-0.612731,-0.75;1.21353,-0.149206,;0.75,1.27738,0.75;-0.75,1.27738,-0.75;-1.21353,-0.149206,0.75;-2.64011,-0.612731,;0,-1.03088,-0.75),wU:1.0,4.4|",
            "/t4-,5+/m0/s1",
        ),
        (
            "O[C@@H]1CC[C@H](F)C1 |(2.64011,-0.612731,;1.21353,-0.149206,;0.75,1.27738,;-0.75,1.27738,;-1.21353,-0.149206,;-2.64011,-0.612731,;0,-1.03088,),wU:1.0,4.4|",
            "/t4-,5+/m0/s1",
        ),
    ] {
        let imported = engine
            .request(Request::import("smiles", text))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            imported.analysis.unwrap().inchi,
            format!("{RING}{inchi}"),
            "{text}"
        );
    }
    Ok(())
}

/// A CX wedge flag without coordinates has no geometric meaning, so the
/// @/@@ tag (or its absence) states the configuration: the flagged import
/// draws and identifies exactly as the bare SMILES, whether the flag read on
/// the layout would agree with the tag (wD here) or not (wU) (#1044).
#[tokio::test]
async fn coordinate_free_wedge_flags_follow_the_tag() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    let import = async |format: &str, text: &str| {
        engine
            .request(Request::import(format, text))
            .await
            .map_err(anyhow::Error::msg)
    };
    // The flag also names a bond's begin atom, which orders a plain bond's ends.
    let drawn = |response: &Response| {
        let mut document = response.document.clone().unwrap();
        for bond in &mut document.bonds {
            if bond.display == "plain" && bond.a > bond.b {
                std::mem::swap(&mut bond.a, &mut bond.b);
            }
        }
        document
    };
    const ALANINE: &str = "InChI=1S/C3H7NO2/c1-2(4)3(5)6/h2H,4H2,1H3,(H,5,6)/t2-/m0/s1";
    for (format, bare, inchi) in [
        ("smiles", "C[C@H](N)C(=O)O", Some(ALANINE)),
        ("naming_smiles", "C[C@H](N)C(=O)O", Some(ALANINE)),
        (
            "smiles",
            "C1[C@@H](F)CC[C@H]1O",
            Some("InChI=1S/C5H9FO/c6-4-1-2-5(7)3-4/h4-5,7H,1-3H2/t4-,5+/m0/s1"),
        ),
        (
            "smiles",
            "CC=[C@]=CC",
            Some("InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m0/s1"),
        ),
        ("smiles", "FC(Cl)(Br)I", None),
    ] {
        let stated =
            smiles::write::write(&smiles::read(bare)?.prepared.state, Default::default())?.text;
        let expected = import(format, bare).await?;
        for flag in ["wU", "wD"] {
            let text = format!("{bare} |{flag}:1.0|");
            let imported = import(format, &text).await?;
            if let Some(analysis) = &expected.analysis {
                let actual = imported.analysis.as_ref().unwrap();
                assert_eq!(actual.inchi, analysis.inchi, "{text}");
                assert_eq!(actual.smiles, analysis.smiles, "{text}");
            }
            if let Some(inchi) = inchi {
                assert_eq!(imported.analysis.as_ref().unwrap().inchi, inchi, "{text}");
            }
            let mut visible = drawn(&imported);
            assert_eq!(visible, drawn(&expected), "{text}");
            for atom in &mut visible.atoms {
                atom.stereo = None;
            }
            assert_eq!(
                smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?
                    .text,
                stated,
                "{text}: visible wedges must state the tag"
            );
        }
    }
    // An axial flag has no tag to defer to: the molecule reader takes the
    // configuration from it, and the drawing is wedged from that.
    let binol = "c1(-c2c(O)c3c(cc2)cccc3)c(O)c2c(cc1)cccc2";
    let mut axes = Vec::new();
    for flag in ["wU", "wD"] {
        let document = import("smiles", &format!("{binol} |{flag}:0.13|"))
            .await?
            .document
            .unwrap();
        let labels = document
            .bonds
            .iter()
            .filter_map(|bond| bond.cip_label.clone())
            .collect::<Vec<_>>();
        assert_eq!(labels.len(), 1, "{flag}: {labels:?}");
        axes.push(labels);
    }
    assert_ne!(axes[0], axes[1]);
    Ok(())
}

#[tokio::test]
async fn coordinate_free_wedge_flags_respect_wavy_stereo_semantics() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    let import = async |format: &str, text: &str| {
        engine
            .request(Request::import(format, text))
            .await
            .map_err(anyhow::Error::msg)
    };
    const ALANINE: &str = "C[C@H](N)C(=O)O";
    for (source, wave, wedge, bare, inchi, wavy_single) in [
        // A wave ending at the tagged atom preserves its configuration.
        (
            ALANINE,
            "w:0.0",
            "1.1",
            ALANINE,
            Some("InChI=1S/C3H7NO2/c1-2(4)3(5)6/h2H,4H2,1H3,(H,5,6)/t2-/m0/s1"),
            Some((1, 2)),
        ),
        // The wave disappears with its explicit hydrogen during preparation.
        (
            "C1[C@@H](F)CC[C@H]1O[H]",
            "w:7.7",
            "1.0",
            "C1[C@@H](F)CC[C@H]1O",
            Some("InChI=1S/C5H9FO/c6-4-1-2-5(7)3-4/h4-5,7H,1-3H2/t4-,5+/m0/s1"),
            None,
        ),
        // A wave beginning at the tagged atom explicitly clears that tag.
        (
            ALANINE,
            "w:1.0",
            "1.2",
            "CC(N)C(=O)O",
            Some("InChI=1S/C3H7NO2/c1-2(4)3(5)6/h2H,4H2,1H3,(H,5,6)"),
            Some((2, 1)),
        ),
        // The wave makes the adjacent alkene unknown, leaving the far-end tag.
        (
            "C[C@H](N)/C=C/C",
            "w:3.2",
            "1.0",
            "C[C@H](N)C=CC",
            None,
            Some((4, 2)),
        ),
    ] {
        let stated =
            smiles::write::write(&smiles::read(bare)?.prepared.state, Default::default())?.text;
        for format in ["smiles", "naming_smiles"] {
            let expected = import(format, bare).await?;
            let expected_analysis = expected.analysis.as_ref().unwrap();
            let wave_only = import(format, &format!("{source} |{wave}|")).await?;
            let wave_analysis = wave_only.analysis.as_ref().unwrap();
            assert_eq!(
                wave_analysis.inchi, expected_analysis.inchi,
                "{source}/{wave}/{format}"
            );
            assert_eq!(
                wave_analysis.smiles, expected_analysis.smiles,
                "{source}/{wave}/{format}"
            );
            if let Some(inchi) = inchi {
                assert_eq!(wave_analysis.inchi, inchi, "{source}/{wave}/{format}");
            } else {
                assert!(
                    !wave_analysis.inchi.contains("/b"),
                    "{source}/{wave}/{format}"
                );
            }
            let mut visible = wave_only.document.unwrap();
            for atom in &mut visible.atoms {
                atom.stereo = None;
            }
            assert_eq!(
                smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?
                    .text,
                wave_analysis.smiles,
                "{source}/{wave}/{format}: wave-only visible drawing stereochemistry"
            );
            for flag in ["wU", "wD"] {
                let text = format!("{source} |{wave},{flag}:{wedge}|");
                let imported = import(format, &text).await?;
                let analysis = imported.analysis.as_ref().unwrap();
                assert_eq!(analysis.inchi, wave_analysis.inchi, "{text}/{format}");
                assert_eq!(analysis.smiles, wave_analysis.smiles, "{text}/{format}");
                let mut visible = imported.document.unwrap();
                assert_eq!(
                    visible
                        .bonds
                        .iter()
                        .filter(|bond| bond.order == 1 && bond.display == "wavy")
                        .map(|bond| (bond.a, bond.b))
                        .collect::<Vec<_>>(),
                    wavy_single.into_iter().collect::<Vec<_>>(),
                    "{text}/{format}: wavy single-bond direction"
                );
                for atom in &mut visible.atoms {
                    atom.stereo = None;
                }
                assert_eq!(
                    smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?
                        .text,
                    stated,
                    "{text}/{format}: visible drawing stereochemistry"
                );
                if source == ALANINE && bare == ALANINE {
                    let xml = crate::exchange::drawing::write(&visible, Default::default())?;
                    let reopened = import("cdxml", &xml).await?;
                    let analysis = reopened.analysis.as_ref().unwrap();
                    assert_eq!(
                        analysis.inchi, wave_analysis.inchi,
                        "{text}/{format}: CDXML"
                    );
                    assert_eq!(
                        analysis.smiles, wave_analysis.smiles,
                        "{text}/{format}: CDXML"
                    );
                }
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn coordinate_free_wavy_flags_keep_visible_fused_and_adjacent_stereo() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    for (bare, flags, waves) in [
        // Add fusion H before choosing wedges so their directions stay correct.
        (
            "C1CC[C@H]2CCCC[C@@H]2C1",
            "w:2.2,4.3,wU:8.9",
            [(3, 4), (5, 4)],
        ),
        // Leave the only available bond for the wave-constrained centre.
        (
            "F[C@H](Cl)[C@]([C@H](F)Br)([C@H](Cl)Br)[C@H](F)I",
            "w:0.0,2.1,wU:1.2,3.3",
            [(1, 2), (3, 2)],
        ),
    ] {
        let stated =
            smiles::write::write(&smiles::read(bare)?.prepared.state, Default::default())?.text;
        let text = format!("{bare} |{flags}|");
        for format in ["smiles", "naming_smiles"] {
            let imported = engine
                .request(Request::import(format, &text))
                .await
                .map_err(anyhow::Error::msg)?;
            let analysis = imported.analysis.unwrap();
            assert_eq!(analysis.smiles, stated, "{text}/{format}: analysis");
            let mut visible = imported.document.unwrap();
            assert_eq!(
                visible
                    .bonds
                    .iter()
                    .filter(|bond| bond.order == 1 && bond.display == "wavy")
                    .map(|bond| (bond.a, bond.b))
                    .collect::<Vec<_>>(),
                waves,
                "{text}/{format}: wavy single-bond direction"
            );
            for atom in &mut visible.atoms {
                atom.stereo = None;
            }
            // Normalize the graph H added for the drawing to the implicit H
            // representation used for analysis, retaining the read-back stereo.
            let visible_smiles =
                smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?
                    .text;
            let normalized = smiles::prepare(&visible_smiles)?.state;
            assert_eq!(
                smiles::write::write(&normalized, Default::default())?.text,
                analysis.smiles,
                "{text}/{format}: visible drawing stereochemistry"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn wavy_imports_keep_all_five_visible_sulfoxide_configurations() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    let bare = "O=[S@]([C@H](F)[C@]([2H])(Cl)Br)[C@H](Cl)[C@]([2H])(F)I";
    let text = format!("{bare} |w:3.2,9.8|");
    let stated = smiles::read(bare)?.prepared.state;
    assert_eq!(
        stated
            .metadata
            .atoms
            .iter()
            .filter(|atom| matches!(atom.chiral_tag, 1 | 2))
            .count(),
        5
    );
    let expected = smiles::write::write(&stated, Default::default())?.text;
    for format in ["smiles", "naming_smiles", "prepared"] {
        let request = if format == "prepared" {
            Request::import_prepared(smiles::read(&text)?.prepared)
        } else {
            Request::import(format, &text)
        };
        let imported = engine.request(request).await.map_err(anyhow::Error::msg)?;
        assert_eq!(
            imported.analysis.unwrap().smiles,
            expected,
            "{format}: analysis"
        );
        let mut visible = imported.document.unwrap();
        assert_eq!(
            visible
                .bonds
                .iter()
                .filter(|bond| bond.order == 1 && bond.display == "wavy")
                .map(|bond| (bond.a, bond.b))
                .collect::<Vec<_>>(),
            [(4, 3), (10, 9)],
            "{format}: incoming waves"
        );
        assert_eq!(
            visible
                .bonds
                .iter()
                .filter(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
                .count(),
            5,
            "{format}: every centre needs a visible stereobond"
        );
        assert_eq!(
            visible
                .bonds
                .iter()
                .filter(|bond| { bond.a == 2 && matches!(bond.display.as_str(), "wedge" | "hash") })
                .count(),
            1,
            "{format}: sulfur needs an outgoing stereobond"
        );
        for atom in &mut visible.atoms {
            atom.stereo = None;
        }
        let read_back = molecular::prepare(&visible)?.state;
        assert_eq!(
            read_back
                .metadata
                .atoms
                .iter()
                .filter(|atom| matches!(atom.chiral_tag, 1 | 2))
                .count(),
            5,
            "{format}: all five configurations must be visible"
        );
        assert_eq!(
            smiles::write::write(&read_back, Default::default())?.text,
            expected,
            "{format}: visible drawing stereochemistry"
        );
    }
    Ok(())
}

#[tokio::test]
async fn prepared_import_keeps_visible_wavy_bridgehead_stereo() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    let bare = "O=C1C[C@H]2CC[C@@H]1C2";
    let text = format!("{bare} |w:7.7|");
    let source = smiles::read(&text)?;
    let imported = engine
        .request(Request::import_prepared(source.prepared))
        .await
        .map_err(anyhow::Error::msg)?;
    let analysis = imported.analysis.unwrap();
    let stated =
        smiles::write::write(&smiles::read(bare)?.prepared.state, Default::default())?.text;
    assert_eq!(analysis.smiles, stated, "{text}: analysis");
    let mut visible = imported.document.unwrap();
    assert_eq!(
        visible
            .bonds
            .iter()
            .filter(|bond| bond.order == 1 && bond.display == "wavy")
            .map(|bond| (bond.a, bond.b))
            .collect::<Vec<_>>(),
        [(8, 7)],
        "{text}: wavy single-bond direction"
    );
    for atom in &mut visible.atoms {
        atom.stereo = None;
    }
    assert_eq!(
        smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?.text,
        analysis.smiles,
        "{text}: visible drawing stereochemistry"
    );
    Ok(())
}

/// No non-bonded atom pair is inside the depictor's 0.7-unit collision
/// threshold (bonds are 1.5 units), and no bond outside a ring crosses that
/// ring's edges or ends inside its polygon.
fn assert_separated(text: &str, state: &State, p: &[Point3]) {
    let bonded = |a: usize, b: usize| {
        state
            .graph
            .bonds
            .iter()
            .any(|bond| (bond.a, bond.b) == (a, b) || (bond.a, bond.b) == (b, a))
    };
    for a in 0..p.len() {
        for b in a + 1..p.len() {
            let d = (p[a].x - p[b].x).hypot(p[a].y - p[b].y);
            assert!(
                bonded(a, b) || d >= 0.7,
                "{text}: atoms {a} and {b} are {d:.3} apart"
            );
        }
    }
    for ring in &state.rings.atoms {
        let edges = (0..ring.len()).map(|i| (ring[i], ring[(i + 1) % ring.len()]));
        for bond in &state.graph.bonds {
            let (a, b) = (bond.a, bond.b);
            if ring.contains(&a) && ring.contains(&b) {
                continue;
            }
            for (c, d) in edges.clone() {
                if [c, d].contains(&a) || [c, d].contains(&b) {
                    continue;
                }
                let crosses = cross(&p[a], &p[b], &p[c]) * cross(&p[a], &p[b], &p[d]) < 0.0
                    && cross(&p[c], &p[d], &p[a]) * cross(&p[c], &p[d], &p[b]) < 0.0;
                assert!(!crosses, "{text}: bond {a}-{b} crosses ring edge {c}-{d}");
            }
            for end in [a, b].into_iter().filter(|end| !ring.contains(end)) {
                let mut sides = edges
                    .clone()
                    .map(|(c, d)| cross(&p[c], &p[d], &p[end]) > 0.0);
                let first = sides.next();
                let inside = sides.all(|side| Some(side) == first);
                assert!(!inside, "{text}: atom {end} lies inside ring {ring:?}");
            }
        }
    }
}

fn assert_regular_spiro_rings(text: &str, state: &State, positions: &[Point3]) {
    for bond in &state.graph.bonds {
        let a = positions[bond.a];
        let b = positions[bond.b];
        let length = (a.x - b.x).hypot(a.y - b.y);
        assert!(
            (length / 1.5 - 1.).abs() < 1e-3,
            "{text}: bond {}-{} has length {length:.6}",
            bond.a,
            bond.b,
        );
    }
    for ring in &state.rings.atoms {
        let expected = std::f64::consts::PI * (ring.len() - 2) as f64 / ring.len() as f64;
        for i in 0..ring.len() {
            let a = positions[ring[(i + ring.len() - 1) % ring.len()]];
            let b = positions[ring[i]];
            let c = positions[ring[(i + 1) % ring.len()]];
            let (ax, ay, cx, cy) = (a.x - b.x, a.y - b.y, c.x - b.x, c.y - b.y);
            let angle = ((ax * cx + ay * cy) / (ax.hypot(ay) * cx.hypot(cy)))
                .clamp(-1., 1.)
                .acos();
            assert!(
                (angle - expected).abs() < 1e-3,
                "{text}: ring {ring:?} angle at {} is {:.6}, expected {:.6}",
                ring[i],
                angle.to_degrees(),
                expected.to_degrees(),
            );
        }
    }
    assert_separated(text, state, positions);
}

#[test]
fn spiro_imports_keep_regular_rings_and_bond_lengths() -> anyhow::Result<()> {
    for text in [
        SPIRO_INDANONE,
        "C1Cc2ccccc2C12CCCc1ccccc12",
        "O=C1Cc2cc3ccccc3cc2C12CCC(=O)c1ccccc12",
        "O=C1Cc2ccccc2C12CCCc1ccccc12",
        // Fluorescein diacetate (NCI): RDKit squashes the phthalide benzene.
        "CC(=O)OC1=CC2=C(C=C1)C3(OC(=O)C4=C3C=CC=C4)C5=C(O2)C=C(OC(C)=O)C=C5",
        "C1CCC2(C1)CCCCC2",
    ] {
        let input = smiles::read(text)?;
        assert_regular_spiro_rings(
            text,
            &input.prepared.state,
            &coordinates(&input.prepared.state, &input.reaction_properties)?.positions,
        );
        let imported = imported_smiles(text)?;
        assert_regular_spiro_rings(text, &imported.molecule.state, &imported.molecule.positions);
    }
    Ok(())
}

#[test]
fn spiro_cleanup_keeps_regular_rings_and_bond_lengths() -> anyhow::Result<()> {
    use crate::cleanup::{Options, Scope};
    let imported = imported_smiles(SPIRO_INDANONE)?;
    let state = &imported.molecule.state;
    let world = f64::from(imported.document.drawing_style.bond_length_world);
    // RDKit's own layout, with the tetralone benzene ring (atoms 14–19) squashed.
    let squashed = depict::compute(
        state,
        &vec![None; state.graph.atoms.len()],
        None,
        depict::Options::default(),
    )?
    .positions;
    let short = (squashed[15].x - squashed[16].x).hypot(squashed[15].y - squashed[16].y);
    assert!((short - 0.8).abs() < 1e-6, "source ring bond is {short}");
    let ids: Vec<u64> = imported.document.atoms.iter().map(|atom| atom.id).collect();
    assert_eq!(ids.len(), squashed.len());
    for mirror in [1., -1.] {
        let mut source = imported.document.clone();
        for (atom, point) in source.atoms.iter_mut().zip(&squashed) {
            atom.position.x = (mirror * point.x * world / 1.5) as f32;
            atom.position.y = (-point.y * world / 1.5) as f32;
        }
        for (label, scope, selected) in [
            ("whole drawing", Scope::Drawing, None),
            (
                "the molecule",
                Scope::SelectedMolecules,
                Some(ids[..1].to_vec()),
            ),
            // Selecting every atom has no fixed atoms.
            ("every atom", Scope::SelectedAtoms, Some(ids.clone())),
            // Only the indanone's carbonyl oxygen stays fixed, so the layout
            // runs with a fixed atom.
            (
                "all but one atom",
                Scope::SelectedAtoms,
                Some(ids[1..].to_vec()),
            ),
        ] {
            for (keep_orientation, iupac_orientation) in
                [(true, false), (false, false), (false, true)]
            {
                let mut request = Request::molecule("clean", source.clone());
                request.cleanup = Some(Options {
                    scope,
                    keep_orientation,
                    iupac_orientation,
                });
                request.selected_ids = selected.clone();
                let cleaned = crate::engine::native_cleanup::prepare(&request)?;
                let molecule = cleaned.molecule.expect("cleanup analysis");
                let normalized = molecule
                    .positions
                    .iter()
                    .map(|p| Point3 {
                        x: p.x * 1.5 * 28. / world,
                        y: p.y * 1.5 * 28. / world,
                        z: p.z,
                    })
                    .collect::<Vec<_>>();
                assert_regular_spiro_rings(
                    &format!(
                        "{label}, keep {keep_orientation}, IUPAC {iupac_orientation}, mirror {mirror}"
                    ),
                    &molecule.state,
                    &normalized,
                );
            }
        }
    }
    Ok(())
}

#[test]
fn buchwald_ligands_lay_out_without_ring_overlap() -> anyhow::Result<()> {
    for text in [SPHOS, XPHOS] {
        let input = smiles::read(text)?;
        let state = &input.prepared.state;
        assert_separated(
            text,
            state,
            &coordinates(state, &input.reaction_properties)?.positions,
        );
        // Clean up computes with ring templates.
        let cleaned = depict::compute(
            state,
            &vec![None; state.graph.atoms.len()],
            None,
            depict::Options {
                use_ring_templates: true,
                ..Default::default()
            },
        )?;
        assert_separated(text, state, &cleaned.positions);
    }
    Ok(())
}

#[tokio::test]
async fn reaction_imports_keep_stated_alkene_stereo() -> anyhow::Result<()> {
    // Reaction participants keep `/` `\` (or RXN geometry) as directions
    // without assigned stereo. Those state E/Z; an alkene the input leaves
    // open stays unspecified (#1043, #909).
    const AGENT_RXN: &str = "$RXN\n\n      RDKit\n\n  1  1  1\n\
        $MOL\n\n     RDKit          2D\n\n  1  0  0  0  0  0  0  0  0  0999 V2000\n\
        \x20   0.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\nM  END\n\
        $MOL\n\n     RDKit          2D\n\n  1  0  0  0  0  0  0  0  0  0999 V2000\n\
        \x20   0.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\nM  END\n\
        $MOL\n\n     RDKit          2D\n\n  4  3  0  0  0  0  0  0  0  0999 V2000\n\
        \x20  -1.9796   -0.1365    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n\
        \x20  -0.5994    0.4508    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n\
        \x20   0.5994   -0.4508    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n\
        \x20   1.9796    0.1365    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n\
        \x20 1  2  1  0\n  2  3  2  0\n  3  4  1  0\nM  END\n";
    let engine = crate::engine::LocalEngine::in_process();
    for (format, text, smiles, inchi, unspecified) in [
        (
            "rsmi",
            "C/C=C/C>>C",
            "C.C/C=C/C",
            Some("InChI=1S/C4H8.CH4/c1-3-4-2;/h3-4H,1-2H3;1H4/b4-3+;"),
            0,
        ),
        (
            "rsmi",
            r"C/C=C\C>>C",
            r"C.C/C=C\C",
            Some("InChI=1S/C4H8.CH4/c1-3-4-2;/h3-4H,1-2H3;1H4/b4-3-;"),
            0,
        ),
        (
            "rsmi",
            "C/C=C/C>O>N",
            "C/C=C/C.N.O",
            Some("InChI=1S/C4H8.H3N.H2O/c1-3-4-2;;/h3-4H,1-2H3;1H3;1H2/b4-3+;;"),
            0,
        ),
        (
            "rsmi",
            "CC=CC>>C",
            "C.CC=CC",
            Some("InChI=1S/C4H8.CH4/c1-3-4-2;/h3-4H,1-2H3;1H4"),
            1,
        ),
        ("rsmi", "C/C=C/CC=CCC>>C", "C.C/C=C/CC=CCC", None, 1),
        ("rsmi", "CC=CCC/C=C/CC>>C", "C.CC=CCC/C=C/CC", None, 1),
        // Input coordinates take precedence over directions, as in RDKit.
        (
            "rsmi",
            "C/C=C/C>>C |(0,1,;1,0,;2,0,;3,1,;4,0,)|",
            r"C.C/C=C\C",
            None,
            0,
        ),
        // Source perception fails on the invalid property: keep #909's rule.
        (
            "rsmi",
            "(C/C=C/C.CC=CC)>>C |(0,1,;1,0,;2,0,;3,-1,;5,1,;6,0,;7,0,;8,-1,;10,0,),atomProp:1._UnknownStereo.x|",
            "C.CC=CC.CC=CC",
            None,
            2,
        ),
        ("rxn", AGENT_RXN, "C.C.C/C=C/C", None, 0),
    ] {
        let response = engine
            .request(Request::import(format, text))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = response.analysis.unwrap();
        assert_eq!(analysis.smiles, smiles, "{text}");
        if let Some(inchi) = inchi {
            assert_eq!(analysis.inchi, inchi, "{text}");
        }
        let document = response.document.unwrap();
        let marked = document
            .bonds
            .iter()
            .filter(|bond| bond.order == 2 && bond.stereo_authoritative)
            .count();
        assert_eq!(marked, unspecified, "{text}");
        assert!(
            document
                .bonds
                .iter()
                .all(|bond| bond.stereo.is_none() && bond.stereo_atoms.is_empty()),
            "{text}"
        );
    }
    Ok(())
}

/// A wU/wD flag replaces the `/` or `\` on its bond. Where the flag cannot mean
/// a wedge, import keeps the E/Z the direction states, as the bare SMILES does,
/// although RDKit drops it (#1042). Other flagged directions keep main's result.
#[tokio::test]
async fn wedge_flags_keep_stated_alkene_stereo() -> anyhow::Result<()> {
    use anyhow::Context as _;
    let engine = crate::engine::LocalEngine::in_process();
    let analysis = async |text: &str| -> anyhow::Result<(String, String)> {
        let analysis = engine
            .request(Request::import("smiles", text))
            .await
            .map_err(anyhow::Error::msg)?
            .analysis
            .context("Missing analysis")?;
        Ok((analysis.smiles, analysis.inchi))
    };
    let dropped = smiles::read_dropping_flagged_ez;
    // The whole prepared molecule, so a restore cannot change anything else.
    let stereo =
        |text: &str, read: fn(&str) -> std::result::Result<smiles::Imported, smiles::Error>| {
            anyhow::Ok(serde_json::to_value(read(text)?.prepared)?)
        };
    const FLUOROIMINE: &str = "InChI=1S/CH2FN/c2-1-3/h1,3H";
    const BUTENE: &str = "InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3";
    for (text, smiles, inchi) in [
        ("[H]/N=C/F |wU:1.0|", "[H]/N=C/F", "/b3-1+"),
        ("[H]/N=C/F |wD:1.0|", "[H]/N=C/F", "/b3-1+"),
        (r"[H]\N=C/F |wU:1.0|", r"[H]/N=C\F", "/b3-1-"),
        (
            "[H]/N=C/F |(0,0,;1,0,;0,1,;-1,-1,;1,1,;0,-1,),wU:1.0|",
            "[H]/N=C/F",
            "/b3-1+",
        ),
        (
            "[H]/N=C/F |(-1,1,;0,0,;1,0,;2,-1,),wU:1.0|",
            "[H]/N=C/F",
            "/b3-1+",
        ),
        // The flag begins at a terminal atom or at a double-bond end.
        ("C/C=C/C |wU:0.0|", "C/C=C/C", "/b4-3+"),
        ("C/C=C/C |wU:0.0,wU:3.2|", "C/C=C/C", "/b4-3+"),
        (r"C/C=C\C |wU:0.0|", r"C/C=C\C", "/b4-3-"),
        ("C/C=C/C |wU:1.0|", "C/C=C/C", "/b4-3+"),
    ] {
        let bare = text.split_once(' ').context("Missing CX suffix")?.0;
        let inchi = format!(
            "{}{inchi}",
            if text.contains('N') {
                FLUOROIMINE
            } else {
                BUTENE
            }
        );
        let actual = analysis(text).await?;
        assert_eq!(actual, (smiles.into(), inchi), "{text}");
        assert_eq!(actual, analysis(bare).await?, "{text}");
        assert_ne!(
            stereo(text, smiles::read)?,
            stereo(text, dropped)?,
            "{text}"
        );
    }
    // The flag can mean a centre or an axis, the double-bond end has another
    // neighbour, or the double bond is cumulated: the flag still replaces the
    // direction, as on main. Where the double bond has no E/Z, or CX states
    // it, the kept direction changes nothing.
    for (text, smiles, inchi) in [
        ("C/C=C |wU:1.0|", "C=CC", "InChI=1S/C3H6/c1-3-2/h3H,1H2,2H3"),
        (
            "F/C=C(F)F |wU:1.0|",
            "FC=C(F)F",
            "InChI=1S/C2HF3/c3-1-2(4)5/h1H",
        ),
        ("C/C=C/C |wU:1.0,ctu:1|", "CC=CC", BUTENE),
        ("[H]/CC=C/C |wU:3.3|", "CC=CC", BUTENE),
        ("[H]/CC=C/C |wU:4.3|", "CC=CC", BUTENE),
        // Halogen oxides are outside the C=C/C=N restoration rule.
        ("O/Cl=O |wU:1.0|", "[O-][Cl+]O", "InChI=1S/ClHO2/c2-1-3/h2H"),
        (
            "F/C(=C/F)C(Cl)=C/Br |wU:0.0,wU:4.4|",
            "FC=C(F)C(Cl)=CBr",
            "InChI=1S/C4H2BrClF2/c5-1-3(6)4(8)2-7/h1-2H",
        ),
        (
            "C/C=C=CC |(0,1,;0,0,;1,0,;2,0,;2,1,),wU:1.0,wU:3.3|",
            "CC=C=CC",
            "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3",
        ),
        (
            "c1/[p@](F)(C)cccc1 |wU:1.0|",
            "C[P@]1(F)=CC=CC=C1",
            "InChI=1S/C6H8FP/c1-8(7)5-3-2-4-6-8/h2-6H,1H3",
        ),
        (
            "[H]/CC=C/F |wU:1.0|",
            "CC=CF",
            "InChI=1S/C3H5F/c1-2-3-4/h2-3H,1H3",
        ),
        (
            "F/C=C/[C@H](Cl)Br |wU:3.2|",
            "FC=C[C@H](Cl)Br",
            "InChI=1S/C3H3BrClF/c4-3(5)1-2-6/h1-3H/t3-/m0/s1",
        ),
        (
            "C/C(F)=C/F |wU:1.0|",
            "CC(F)=CF",
            "InChI=1S/C3H4F2/c1-3(5)2-4/h2H,1H3",
        ),
    ] {
        assert_eq!(
            analysis(text).await?,
            (smiles.into(), inchi.into()),
            "{text}"
        );
        assert_eq!(
            stereo(text, smiles::read)?,
            stereo(text, dropped)?,
            "{text}"
        );
    }
    // Existing E/Z from another slash and metal normalization must not differ.
    for text in [
        "F/C(/Cl)=C/F |wU:0.0|",
        "[Na]/O=C |wU:1.0|",
        "[Na]/O=C/F |wU:0.0|",
        "[Na]/O=C/F |wU:1.0|",
    ] {
        assert_eq!(
            stereo(text, smiles::read)?,
            stereo(text, dropped)?,
            "{text}"
        );
    }
    Ok(())
}

async fn reaction_wedges_match_smiles(
    text: &str,
    molecule_text: &str,
    role: crate::reactions::Role,
) -> anyhow::Result<(Document, String)> {
    let expected = prepare(Request::import("smiles", molecule_text));
    let actual = prepare(Request::import("rsmi", text));
    assert_eq!(actual.is_ok(), expected.is_ok(), "{text}: acceptance");
    let Preparation::Complete(expected) = expected? else {
        panic!("{molecule_text}: import deferred");
    };
    let Preparation::Complete(actual) = actual? else {
        panic!("{text}: import deferred");
    };
    let ids = &actual.document.reactions[0].participants(role)[0].atoms;
    let document = Document {
        atoms: actual
            .document
            .atoms
            .into_iter()
            .filter(|a| ids.contains(&a.id))
            .collect(),
        bonds: actual
            .document
            .bonds
            .into_iter()
            .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
            .collect(),
        ..Document::default()
    };
    let labels = |doc: &Document| {
        doc.bonds
            .iter()
            .filter_map(|b| b.cip_label.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(&document),
        labels(&expected.document),
        "{text}: axes"
    );
    let identity = crate::molecule_names::canonical_identity(&expected.molecule.state).unwrap();
    let expected = native_response::analyze_prepared(
        Arc::new(expected.molecule),
        false,
        Some(native_response::Config::in_process()),
    )
    .await?;
    for clear_tags in [false, true] {
        let mut visible = document.clone();
        if clear_tags {
            for atom in &mut visible.atoms {
                atom.stereo = None;
            }
        }
        let molecule = molecular::prepare(&visible)?;
        assert_eq!(
            crate::molecule_names::canonical_identity(&molecule.state).unwrap(),
            identity,
            "{text}: clear_tags={clear_tags}"
        );
        let actual = native_response::analyze_prepared(
            Arc::new(molecule),
            false,
            Some(native_response::Config::in_process()),
        )
        .await?;
        assert_eq!(
            actual.smiles, expected.smiles,
            "{text}: clear_tags={clear_tags}"
        );
        assert_eq!(
            actual.inchi, expected.inchi,
            "{text}: clear_tags={clear_tags}"
        );
    }
    Ok((document, expected.inchi))
}

#[tokio::test]
async fn coordinate_free_reaction_wedge_flags_follow_the_tag() -> anyhow::Result<()> {
    use crate::reactions::Role;
    for flags in ["wU:1.0", "wD:1.0", "wU:0.0,wD:1.1"] {
        let (doc, inchi) = reaction_wedges_match_smiles(
            &format!("C[C@H](N)C(=O)O>>O |{flags}|"),
            &format!("C[C@H](N)C(=O)O |{flags}|"),
            Role::Reactant,
        )
        .await?;
        assert!(inchi.ends_with("/t2-/m0/s1"), "{flags}: {inchi}");
        assert!(
            doc.bonds
                .iter()
                .any(|b| matches!(b.display.as_str(), "wedge" | "hash"))
        );
    }
    // CX indices traverse reactants, agents, then products (#1048).
    for (text, role) in [
        ("O>>C[C@H](N)C(=O)O |wU:2.1|", Role::Product),
        ("CC>O>C[C@H](N)C(=O)O |wU:4.2|", Role::Product),
        ("CC>C[C@H](N)C(=O)O>O |wU:3.2|", Role::Agent),
    ] {
        let (_, inchi) =
            reaction_wedges_match_smiles(text, "C[C@H](N)C(=O)O |wU:1.1|", role).await?;
        assert!(inchi.ends_with("/t2-/m0/s1"), "{text}: {inchi}");
    }
    for flags in ["wU:1.0", "wD:1.0"] {
        let (doc, _) = reaction_wedges_match_smiles(
            &format!("FC(Cl)(Br)I>>O |{flags}|"),
            &format!("FC(Cl)(Br)I |{flags}|"),
            Role::Reactant,
        )
        .await?;
        assert!(doc.bonds.iter().all(|b| b.display == "plain"));
    }
    Ok(())
}

#[tokio::test]
async fn coordinate_free_reaction_aromatic_wedges_are_plain() -> anyhow::Result<()> {
    let (document, _) = reaction_wedges_match_smiles(
        "c1ccccc1>>O |wU:0.0|",
        "c1ccccc1 |wU:0.0|",
        crate::reactions::Role::Reactant,
    )
    .await?;
    assert_eq!(document.bonds.len(), 6);
    assert!(document.bonds.iter().all(|bond| bond.display == "plain"));
    Ok(())
}

#[tokio::test]
async fn coordinate_free_reaction_wedges_keep_axes_before_sanitization() -> anyhow::Result<()> {
    use crate::reactions::Role;
    let binol = "c1(-c2c(O)c3c(cc2)cccc3)c(O)c2c(cc1)cccc2";
    let mut axes = Vec::new();
    for flags in ["wU:0.13", "wD:0.13"] {
        let (doc, _) = reaction_wedges_match_smiles(
            &format!("{binol}>>O |{flags}|"),
            &format!("{binol} |{flags}|"),
            Role::Reactant,
        )
        .await?;
        let labels = doc
            .bonds
            .iter()
            .filter_map(|b| b.cip_label.clone())
            .collect::<Vec<_>>();
        assert_eq!(labels.len(), 1, "{flags}: {labels:?}");
        axes.push(labels);
    }
    assert_ne!(axes[0], axes[1]);
    // Kekulization clears the first flag; sanitization removes the second axis.
    for (bare, flags) in [
        ("c1(ncc[nH]1)-c1c(F)cccc1", "wU:0.0"),
        ("C1=C(F)C=CCC1", "wU:1.1"),
    ] {
        reaction_wedges_match_smiles(
            &format!("{bare}>>O |{flags}|"),
            &format!("{bare} |{flags}|"),
            Role::Reactant,
        )
        .await?;
    }
    Ok(())
}

#[tokio::test]
async fn reaction_wedge_flags_keep_coordinates_and_wavy_behavior() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    let ring = WEDGE_FLAGGED_RING.replacen(" |", ">>O |", 1);
    // These coordinate-bearing results are unchanged from origin/main (#1048).
    for (text, smiles, inchi) in [
        (
            ring.clone(),
            "O.O[C@@H]1CC[C@H](F)C1",
            "InChI=1S/C5H9FO.H2O/c6-4-1-2-5(7)3-4;/h4-5,7H,1-3H2;1H2/t4-,5+;/m0./s1",
        ),
        (
            ring.replace("wD:", "wU:"),
            "O.O[C@@H]1CC[C@H](F)C1",
            "InChI=1S/C5H9FO.H2O/c6-4-1-2-5(7)3-4;/h4-5,7H,1-3H2;1H2/t4-,5+;/m1./s1",
        ),
        (
            "C[C@H](N)C(=O)O>>O |w:0.0,wD:1.1|".into(),
            "C[C@H](N)C(=O)O.O",
            "InChI=1S/C3H7NO2.H2O/c1-2(4)3(5)6;/h2H,4H2,1H3,(H,5,6);1H2/t2-;/m0./s1",
        ),
    ] {
        let imported = engine
            .request(Request::import("rsmi", &text))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = imported.analysis.unwrap();
        assert_eq!(analysis.smiles, smiles, "{text}");
        assert_eq!(analysis.inchi, inchi, "{text}");
    }
    reaction_wedges_match_smiles(
        "C[C@H](N)C(=O)O>>O |w:0.0,wD:1.1|",
        "C[C@H](N)C(=O)O |w:0.0,wD:1.1|",
        crate::reactions::Role::Reactant,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn coordinate_bearing_reaction_wedges_keep_main_results() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    // Identifiers, absent axis labels, and wedge counts match origin/main (#1048).
    for (text, smiles, inchi, wedge_count, axes) in [
        (
            "FC=C(F)C(F)=CF>>O |(-2,1,0;-1,1,0;0,0,0;-1,-1,0;1.5,0,0;2.5,1,1;2.5,-1,-1;3.5,-1,-1;5,0,0),wU:2.2|",
            "FC=C(F)C(F)=CF.O",
            "InChI=1S/C4H2F4.H2O/c5-1-3(7)4(8)2-6;/h1-2H;1H2",
            1,
            vec![6],
        ),
        (
            "c1(ncc[nH]1)-c1c(F)cccc1>>O |(0,0,;-1,1,;-2,0.5,;-2,-0.5,;-1,-1,;1.5,0,;2.25,-1.299,;1.5,-2.598,;3.75,-1.299,;4.5,0,;3.75,1.299,;2.25,1.299,;8,0,),wU:0.0|",
            "Fc1ccccc1-c1ncc[nH]1.O",
            "InChI=1S/C9H7FN2.H2O/c10-8-4-2-1-3-7(8)9-11-5-6-12-9;/h1-6H,(H,11,12);1H2",
            0,
            vec![],
        ),
    ] {
        let imported = engine
            .request(Request::import("rsmi", text))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = imported.analysis.unwrap();
        assert_eq!(analysis.smiles, smiles, "{text}");
        assert_eq!(analysis.inchi, inchi, "{text}");
        let document = imported.document.unwrap();
        assert!(
            document.bonds.iter().all(|b| b.cip_label.is_none()),
            "{text}"
        );
        assert_eq!(
            document
                .bonds
                .iter()
                .filter(|b| b.display == "wedge")
                .count(),
            wedge_count,
            "{text}"
        );
        assert!(document.bonds.iter().all(|b| b.display != "hash"), "{text}");
        let molecule = molecular::prepare(&document)?;
        assert_eq!(
            molecule
                .state
                .metadata
                .bonds
                .iter()
                .filter_map(|b| matches!(b.stereo, 6 | 7).then_some(b.stereo))
                .collect::<Vec<_>>(),
            axes,
            "{text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn reaction_wedges_on_nonsingle_bonds_match_silent_molecule_imports() -> anyhow::Result<()> {
    use crate::reactions::Role;
    let engine = crate::engine::LocalEngine::in_process();
    for (
        reaction,
        molecule,
        role,
        reaction_flag,
        molecule_flag,
        extra,
        reaction_atoms,
        molecule_atoms,
    ) in [
        (
            "CO>N->[Cu]<-N>CC",
            "N->[Cu]<-N",
            Role::Agent,
            "3.1",
            "1.0",
            "",
            7,
            3,
        ),
        (
            "C/C=C/C>N>CC",
            "C/C=C/C",
            Role::Reactant,
            "2.1",
            "2.1",
            "",
            7,
            4,
        ),
        (
            "N->[Cu]<-N>N>CC",
            "N->[Cu]<-N",
            Role::Reactant,
            "1.0",
            "1.0",
            ",SgD:0:name:data::::,SgH:0:0",
            6,
            3,
        ),
        ("CC>N>C#N", "C#N", Role::Product, "4.1", "1.0", "", 5, 2),
    ] {
        // Molecular import clears the same paint with no notice, including
        // retained CX conformers. Reaction import must do so for every role.
        for dimension in [None, Some(false), Some(true)] {
            let coordinates = |count| match dimension {
                None => String::new(),
                Some(is_3d) => format!(
                    "({}),",
                    (0..count)
                        .map(|i| format!("{},{},{}", i, i % 2, if is_3d { "1" } else { "" }))
                        .collect::<Vec<_>>()
                        .join(";")
                ),
            };
            for flag in ["wU", "wD"] {
                let reaction_text = format!(
                    "{reaction} |{}{flag}:{reaction_flag}{extra}|",
                    coordinates(reaction_atoms)
                );
                let molecule_text = format!(
                    "{molecule} |{}{flag}:{molecule_flag}{extra}|",
                    coordinates(molecule_atoms)
                );
                let expected = engine
                    .request(Request::import("smiles", &molecule_text))
                    .await
                    .map_err(anyhow::Error::msg)?;
                assert!(
                    expected.warnings.is_empty(),
                    "{molecule_text}: {:?}",
                    expected.warnings
                );
                let actual = engine
                    .request(Request::import("rsmi", &reaction_text))
                    .await
                    .map_err(anyhow::Error::msg)?;
                assert!(
                    actual.warnings.is_empty(),
                    "{reaction_text}: {:?}",
                    actual.warnings
                );
                assert!(actual.analysis.is_some(), "{reaction_text}");
                let document = actual.document.unwrap();
                let ids = &document.reactions[0].participants(role)[0].atoms;
                let participant = Document {
                    atoms: document
                        .atoms
                        .iter()
                        .filter(|a| ids.contains(&a.id))
                        .cloned()
                        .collect(),
                    bonds: document
                        .bonds
                        .iter()
                        .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                        .cloned()
                        .collect(),
                    ..Document::default()
                };
                assert!(
                    participant.bonds.iter().all(|b| b.display == "plain"),
                    "{reaction_text}"
                );
                let expected = molecular::prepare(&expected.document.unwrap())?;
                let actual = molecular::prepare(&participant)?;
                assert_eq!(
                    smiles::write::write(&actual.state, Default::default())?.text,
                    smiles::write::write(&expected.state, Default::default())?.text,
                    "{reaction_text}: {molecule_text}"
                );
            }
        }
    }
    Ok(())
}
