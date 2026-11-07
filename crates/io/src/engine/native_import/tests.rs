use super::*;
use crate::chemistry::smiles;

const SPHOS: &str = "COc1cccc(OC)c1-c1ccccc1P(C1CCCCC1)C1CCCCC1";
const XPHOS: &str = "CC(C)c1cc(C(C)C)c(-c2ccccc2P(C2CCCCC2)C2CCCCC2)c(C(C)C)c1";

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
    let source = finish_molecule(molecule, &input.prepared.dummy_labels, Default::default())?;
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
    // A wavy flag next to a wedge flag keeps the flags on the layout (#1049):
    // the drawing must not lose the tagged centre's stereobond.
    let text = "C[C@H](N)C(=O)O |w:0.0,wD:1.1|";
    let mut visible = drawn(&import("smiles", text).await?);
    for atom in &mut visible.atoms {
        atom.stereo = None;
    }
    assert_eq!(
        smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?.text,
        "C[C@H](N)C(=O)O",
        "{text}"
    );
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
    let stereo =
        |text: &str, read: fn(&str) -> std::result::Result<smiles::Imported, smiles::Error>| {
            anyhow::Ok(serde_json::to_value(read(text)?.prepared.state.metadata)?)
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
        assert_eq!(analysis(text).await?, (smiles.into(), inchi), "{text}");
        assert_eq!(analysis(text).await?, analysis(bare).await?, "{text}");
        assert_ne!(
            stereo(text, smiles::read)?,
            stereo(text, dropped)?,
            "{text}"
        );
    }
    // The flag can mean a centre or an axis, the double-bond end has another
    // neighbour, or the double bond is cumulated: the flag still replaces the
    // direction, as on main.
    for (text, smiles, inchi) in [
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
    Ok(())
}
