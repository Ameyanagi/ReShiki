//! Differential chemistry/figure tests guard the boundary between local Rust
//! conversion and the retained RDKit backend. Neither oracle uses the new codec.
#[path = "support/axial_extension.rs"]
mod axial_extension;
#[path = "support/cx_lenience.rs"]
mod cx_lenience;
#[path = "support/drawing_basis.rs"]
mod drawing_basis;
#[path = "support/rdkit_parity.rs"]
mod rdkit_parity;
#[path = "support/reference_response.rs"]
mod reference_response;
#[path = "support/smiles_extensions.rs"]
#[allow(dead_code)]
mod smiles_extensions;
#[path = "support/unspecified_ez.rs"]
mod unspecified_ez;
use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki::engine::{ChemistryEngine, LocalEngine, PythonEngine, Request, Response};

type TestResult = anyhow::Result<()>;

#[path = "support/coordinate_free_wedges.rs"]
mod coordinate_free_wedges;
#[path = "support/coordinate_wedges.rs"]
mod coordinate_wedges;
#[path = "support/flagged_ez.rs"]
mod flagged_ez;
#[path = "support/import_template_retry.rs"]
mod import_template_retry;
#[path = "support/longest_lived_mass.rs"]
mod longest_lived_mass;
#[path = "support/molecule_reading.rs"]
mod molecule_reading;
#[path = "support/molfile_restrictions.rs"]
mod molfile_restrictions;
#[path = "support/nci_2808_template_layout.rs"]
mod nci_2808_template_layout;
#[path = "support/partial_import.rs"]
mod partial_import;
#[path = "support/reaction_presentation.rs"]
mod reaction_presentation;
#[path = "support/reference_mol.rs"]
mod reference_mol;
#[path = "support/reference_presentation.rs"]
mod reference_presentation;
#[path = "support/superatoms.rs"]
mod superatoms;

#[tokio::test]
async fn rust_mol_import_preserves_complete_reference_responses() -> TestResult {
    rdkit_parity::enable();
    import_responses("mol", "reference/molfile_engine_reference.py").await
}

#[tokio::test]
async fn rust_smiles_import_preserves_complete_reference_responses() -> TestResult {
    rdkit_parity::enable();
    import_responses("smiles", "reference/smiles_engine_reference.py").await
}

#[tokio::test]
async fn rust_rxn_import_preserves_complete_reference_responses() -> TestResult {
    rdkit_parity::enable();
    import_responses("rxn", "reference/reaction_engine_reference.py").await
}

#[tokio::test]
async fn rust_reaction_smiles_import_preserves_complete_reference_responses() -> TestResult {
    rdkit_parity::enable();
    import_responses("rsmi", "reference/reaction_smiles_engine_reference.py").await
}

#[tokio::test]
async fn reaction_coordinate_free_wedge_exceptions_require_changed_responses() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for (text, changed) in [
        ("c1ccccc1>>O |wU:0.0|", true),
        ("c1(ncc[nH]1)-c1c(F)cccc1>>O |wU:0.0|", true),
        ("(c1(ncc[nH]1)-c1c(F)cccc1.CC)>>O |wU:0.0,w:12.13|", true),
        ("c1ncc[nH]1>>O |wU:0.0|", false),
        ("(c1(-c2ccccc2)ccccc1.CC)>>O |wU:1.1,w:12.13|", false),
        ("(c1(-c2ccccc2)ccccc1.CC)>>O |wD:1.1,w:12.13|", false),
        ("(c1ccccc1-c1ccccc1.CC)>>O |wU:5.4,w:12.13|", true),
        ("CO>N->[Cu]<-N>CC |wD:3.1|", true),
        ("C/C=C/C>N>CC |wD:2.1|", true),
        ("N->[Cu]<-N>N>CC |wD:1.0,SgD:0:name:data::::,SgH:0:0|", true),
    ] {
        let request = Request::import("rsmi", text);
        let actual = local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let comparison = match reference.execute(request).await {
            Ok(expected) => assert_import_response_matches(actual, expected, "rsmi"),
            Err(error) if coordinate_free_wedges::cleared_undrawable("rsmi", text) => {
                assert!(
                    error.contains("Unsupported bond appearance for this order"),
                    "{text}: {error}"
                );
                Err(anyhow::anyhow!(error))
            }
            // RDKit 2026.09 perceives the wedged biaryl axis (rdkit#9190),
            // which the original worker refuses. ReShiki's axis for these
            // inputs is checked by the reaction SMILES unit tests.
            Err(error) => {
                assert_eq!(
                    error, "This bond stereochemistry is not supported yet.",
                    "{text}"
                );
                continue;
            }
        };
        // Matching inputs stay under strict comparison, including symmetric axes (#1048).
        assert_eq!(comparison.is_err(), changed, "{text}: {comparison:?}");
        assert_eq!(
            coordinate_free_wedges::changed("rsmi", text, &comparison),
            changed,
            "{text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn qualified_bridged_imports_match_independent_template_responses() -> TestResult {
    rdkit_parity::enable();
    // Exercise both deliberate #538 differences on every platform, regardless
    // of the larger corpus's sampling offset or optional source fixtures.
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for (format, name, text) in [
        ("smiles", "NCI/2808", "CC1(C)[CH]2CC[CH](CC(O)=O)[CH]1C2"),
        (
            "rsmi",
            "molecule/NCI/2808/1",
            "O>CC1(C)[CH]2CC[CH](CC(O)=O)[CH]1C2>O",
        ),
    ] {
        let request = Request::import(format, text);
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let expected = nci_2808_template_layout::adapt_expected(format, name, text, expected)?;
        let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
        assert_import_response_matches(actual, expected, format)
            .with_context(|| format!("Qualified bridged import {name}"))?;
    }
    Ok(())
}

#[tokio::test]
async fn reaction_cx_cached_properties_preserve_aromatic_stereo_drawings() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for graph in ["c1ccccc1[C@H]1CCCCC1", "c1ccccc1[C@H]2C[C@@H](N)CCC2"] {
        for atom in 0..14 {
            for key in ["_ringStereoAtoms", "_ringStereochemCand", "_CIPRank"] {
                for value in ["0", "1", "bad"] {
                    let text = format!("{graph}>>O |atomProp:{atom}.{key}.{value}|");
                    let request = Request::import("rsmi", &text);
                    match (
                        local.execute(request.clone()).await,
                        reference.execute(request).await,
                    ) {
                        (Ok(actual), Ok(expected)) => {
                            assert_import_response_matches(actual, expected, "rsmi")
                                .with_context(|| text)?
                        }
                        (Err(_), Err(_)) => (),
                        (actual, expected) => anyhow::bail!("{text}: {actual:?} != {expected:?}"),
                    }
                }
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn overlapping_reaction_imports_keep_labels_and_analysis_with_their_drawing() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let mut tasks = tokio::task::JoinSet::new();
    for text in [
        "CCO.O>O>CC=O.O",
        "F[C@](Cl)(Br)I>>F[C@@](Cl)(Br)I",
        "c1ccccc1>C[C@@H](N)C(=O)O>c1ccccc1O",
        "F/C=C/F>>F/C=C\\F",
    ] {
        for text in [text, "invalid", "C>>O |(bad)|"] {
            let request = Request::import("rsmi", text);
            let expected = reference.execute(request.clone()).await;
            let engine = local.clone();
            tasks.spawn(async move { (engine.execute(request).await, expected, text) });
        }
        let doc = reference
            .execute(Request::import("rsmi", text))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing reaction document")?;
        let mut export = Request::molecule("export", doc);
        export.format = Some("rxn".into());
        let block = reference
            .execute(export)
            .await
            .map_err(anyhow::Error::msg)?
            .output
            .context("Missing reaction file")?;
        for text in [
            block.as_str(),
            "invalid",
            block
                .get(..block.len() / 2)
                .context("Truncation boundary")?,
        ] {
            let request = Request::import("rxn", text);
            let expected = reference.execute(request.clone()).await;
            let engine = local.clone();
            tasks.spawn(async move { (engine.execute(request).await, expected, "") });
        }
    }
    while let Some(result) = tasks.join_next().await {
        match result? {
            (Ok(actual), Ok(expected), text) => {
                let format = if text.is_empty() { "rxn" } else { "rsmi" };
                assert_import_response_matches(actual, expected, format)?
            }
            (Err(_), Err(_), _) => (),
            // RDKit 2026.09 reads `(bad)` as a zero coordinate (rdkit#9273).
            (Err(_), Ok(_), text) if cx_lenience::lenient_number(text) => (),
            (actual, expected, _) => {
                anyhow::bail!("Reaction import changed: {actual:?} != {expected:?}")
            }
        }
    }
    let request = Request::import_smiles("N[C@@H](C)C(=O)O");
    assert_response_matches(
        local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?,
        reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?,
    )
}

#[tokio::test]
async fn overlapping_imports_keep_their_own_layout_and_recover_after_errors() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let mut tasks = tokio::task::JoinSet::new();
    for text in [
        "CCO",
        "c1ccccc1",
        "N[C@@H](C)C(=O)O",
        "C/C=C/C",
        "C(=O)O",
        "C1CC",
        "[CH5]",
        "F[C@H](Cl)[C@@H](Br)I |o1:1,3|",
        "[13CH3:90][NH3+]",
        "CC |(nan,inf,-inf)|",
    ] {
        let request = Request::import_smiles(text);
        let expected = reference.execute(request.clone()).await;
        let engine = local.clone();
        tasks.spawn(async move { (engine.execute(request).await, expected) });
    }
    while let Some(result) = tasks.join_next().await {
        match result? {
            (Ok(actual), Ok(expected)) => assert_response_matches(actual, expected)?,
            (Err(_), Err(_)) => (),
            (actual, expected) => {
                anyhow::bail!("Import result changed: {actual:?} != {expected:?}")
            }
        }
    }
    let request = Request::import_smiles("F[C@](Cl)(Br)I");
    assert_response_matches(
        local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?,
        reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?,
    )
}

async fn import_responses(format: &str, script: &str) -> TestResult {
    use std::{
        io::{BufRead, BufReader},
        path::Path,
        process::{Command, Stdio},
    };
    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        text: String,
        expected: Option<serde_json::Value>,
        failure: Option<String>,
        restriction: Option<molfile_restrictions::GenericRestriction>,
        display: Option<serde_json::Value>,
        generic: Option<serde_json::Value>,
        generic_extension: Option<serde_json::Value>,
        unspecified_ez: Option<unspecified_ez::UnspecifiedEz>,
        #[serde(default)]
        coordinate_wedges: Option<coordinate_wedges::CoordinateWedges>,
        #[serde(default)]
        molecule_reading: Option<molecule_reading::MoleculeReading>,
        #[serde(default)]
        sssr: Option<Vec<Vec<usize>>>,
        superatoms: Option<Vec<superatoms::Group>>,
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join(script))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines =
        BufReader::new(child.stdout.take().context("Missing import reference")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], reshiki::chemistry::RDKIT_VERSION);
    let local = LocalEngine::default();
    let (mut accepted, mut rejected, mut lenient, mut basis_layouts) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut categories = std::collections::BTreeMap::<String, usize>::new();
    let mut restrictions = std::collections::BTreeMap::<String, usize>::new();
    let (mut xyz, mut folded, mut generic, mut extensions) = (0, 0, 0, 0);
    let (mut tag_wins, mut kept_ez, mut template_layouts) = (Vec::new(), Vec::new(), Vec::new());
    let mut partial_imports = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let actual = local.execute(Request::import(format, &case.text)).await;
        if let Some(restriction) = &case.restriction {
            if let Some(error) = molfile_restrictions::rejection_difference(
                restriction,
                case.expected.is_some() && case.failure.is_none(),
                &actual,
            ) {
                *categories.entry("Restriction contract".into()).or_default() += 1;
                failures.push(format!("{}: {error}; input {:?}", case.name, case.text));
            } else {
                *restrictions
                    .entry(restriction.category.clone())
                    .or_default() += 1;
            }
            continue;
        }
        if let Some(extension) = &case.generic_extension {
            let result = (|| -> TestResult {
                anyhow::ensure!(
                    format == "mol" && case.expected.is_none(),
                    "Generic extension lacks legacy rejection"
                );
                anyhow::ensure!(
                    case.failure.as_deref() == extension["failure"].as_str(),
                    "Generic extension legacy reason changed"
                );
                let expected: Response = serde_json::from_value(extension["expected"].clone())?;
                assert_response_matches(actual.map_err(anyhow::Error::msg)?, expected)
            })();
            if let Err(error) = result {
                *categories
                    .entry("Generic acceptance extension".into())
                    .or_default() += 1;
                failures.push(format!("{}: {error:#}; input {:?}", case.name, case.text));
            } else {
                extensions += 1;
            }
            continue;
        }
        let expected = if format == "mol" {
            case.expected
                .map(|value| {
                    reference_mol::adapt_response(
                        value,
                        case.display.as_ref(),
                        case.generic.as_ref(),
                    )
                })
                .transpose()
        } else {
            Ok(case.expected)
        };
        let expected = match expected {
            Ok(expected) => expected,
            Err(error) => {
                *categories
                    .entry("MOL expectation contract".into())
                    .or_default() += 1;
                failures.push(format!("{}: {error:#}; input {:?}", case.name, case.text));
                continue;
            }
        };
        if (case.display.is_some() || case.generic.is_some()) && format == "mol" {
            let result = (|| -> TestResult {
                let response: Response = serde_json::from_value(
                    expected.clone().context("Missing adapted MOL response")?,
                )?;
                response
                    .document
                    .context("Missing adapted MOL document")?
                    .validate()
                    .map_err(anyhow::Error::msg)
            })();
            if let Err(error) = result {
                *categories
                    .entry("MOL expectation contract".into())
                    .or_default() += 1;
                failures.push(format!(
                    "{}: Invalid independently adapted response: {error:#}",
                    case.name
                ));
                continue;
            }
        }
        let expected = expected
            // The original Rust bridge also rejects native values outside the
            // editable document's types, such as negative atom-map numbers.
            .and_then(|value| serde_json::from_value::<Response>(value).ok())
            .filter(|r| r.document.as_ref().is_none_or(|d| d.validate().is_ok()));
        if actual.is_ok() && expected.is_some() && flagged_ez::flagged(format, &case.text) {
            accepted += 1;
            kept_ez.push(case.name);
            continue;
        }
        // A `~` bond that CX redefines needs no assumption; it must import
        // exactly as the same text without `~` does.
        let plain = if matches!(format, "smiles" | "rsmi") && case.text.contains('~') {
            let plain = local
                .execute(Request::import(format, &case.text.replace('~', "")))
                .await;
            plain
                .ok()
                .and_then(|plain| serde_json::to_value(plain).ok())
        } else {
            None
        };
        let failure = match (actual, expected) {
            // Partial import draws an atom whose invalid `@AL` tag the strict
            // reader refuses (#892) and the frozen worker silently drops.
            (Ok(actual), Some(_))
                if matches!(format, "smiles" | "rsmi")
                    && strict_error(format, &case.text).is_some_and(|error| {
                        smiles_extensions::refused_allene(&case.text, &error)
                    })
                    && partial_import::drops_allene_tag(&actual.warnings) =>
            {
                accepted += 1;
                partial_imports.push(case.name.clone());
                let text = partial_import::without_allene_tags(&case.text);
                local
                    .execute(Request::import(format, &text))
                    .await
                    .map_err(anyhow::Error::msg)
                    .and_then(|plain| partial_import::matches_without_allene_tags(&actual, &plain))
                    .err()
                    .map(|e| e.to_string())
            }
            (Ok(actual), Some(mut expected)) => {
                accepted += 1;
                let other_basis =
                    drawing_basis::differs(case.sssr.as_ref(), actual.document.as_ref());
                if let Some(open) = &case.unspecified_ez {
                    unspecified_ez::adapt(open, &actual, &mut expected);
                }
                if let Some(input) = &case.coordinate_wedges {
                    coordinate_wedges::adapt(&case.name, input, &actual, &mut expected);
                }
                if let Some(reading) = &case.molecule_reading {
                    molecule_reading::adapt(reading, &actual, &mut expected);
                }
                let superatoms = case.superatoms.as_deref().map_or(Ok(()), |groups| {
                    superatoms::adapt(groups, &actual, &mut expected)
                });
                let expected = if let Err(error) = superatoms {
                    Err(error)
                } else if format == "mol" {
                    let before = serde_json::to_value(&expected)?;
                    reference_mol::mol_source_xyz(expected, &case.text).inspect(|response| {
                        if serde_json::to_value(response).ok().as_ref() != Some(&before) {
                            xyz += 1;
                        }
                    })
                } else {
                    Ok(expected)
                };
                folded += usize::from(case.display.is_some());
                generic += usize::from(case.generic.is_some());
                expected
                    .and_then(|expected| {
                        nci_2808_template_layout::adapt_expected(
                            format, &case.name, &case.text, expected,
                        )
                    })
                    .and_then(|expected| {
                        let drawings = actual.document.clone().zip(expected.document.clone());
                        let comparison = assert_import_response_matches(actual, expected, format);
                        if coordinate_free_wedges::changed(format, &case.text, &comparison) {
                            tag_wins.push(case.name.clone());
                            Ok(())
                        } else if comparison.is_err() && other_basis {
                            basis_layouts += 1;
                            Ok(())
                        } else if let (Err(error), Some((ours, theirs))) = (&comparison, &drawings)
                            && import_template_retry::redrawn(ours, theirs, &error.to_string())
                        {
                            template_layouts.push(case.name.clone());
                            Ok(())
                        } else {
                            comparison
                        }
                    })
                    .err()
                    .map(|e| e.to_string())
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_))
                if matches!(format, "smiles" | "rsmi")
                    && smiles_extensions::refused_allene(&case.text, &error) =>
            {
                rejected += 1;
                None
            }
            // RDKit 2026.09 reads CX numbers leniently (rdkit#9273).
            (Err(_), Some(_))
                if matches!(format, "smiles" | "rsmi")
                    && cx_lenience::lenient_number(&case.text) =>
            {
                lenient += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("Unexpected rejection: {error}")),
            (Ok(_), None) if cx_lenience::lenient_query(&case.text) => {
                lenient += 1;
                None
            }
            (Ok(actual), None)
                if actual.document.as_ref().is_some_and(|doc| {
                    axial_extension::accepted(
                        &case.name,
                        case.failure.as_deref(),
                        format,
                        &case.text,
                        axial_extension::Checked::Document(doc),
                    )
                }) =>
            {
                accepted += 1;
                None
            }
            (Ok(_), None)
                if coordinate_free_wedges::cleared_undrawable(format, &case.text)
                    && case.failure.as_deref().is_none_or(|error| {
                        error.contains("Unsupported bond appearance for this order")
                    }) =>
            {
                accepted += 1;
                tag_wins.push(case.name.clone());
                None
            }
            (Ok(actual), None)
                if partial_import::replaces_failure(
                    format,
                    case.failure.as_deref(),
                    &actual.warnings,
                ) =>
            {
                accepted += 1;
                partial_imports.push(case.name.clone());
                None
            }
            (Ok(actual), None)
                if plain.is_some() && serde_json::to_value(&actual).ok() == plain =>
            {
                accepted += 1;
                partial_imports.push(case.name.clone());
                None
            }
            (Ok(_), None) => Some(format!("Accepted reference rejection: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            *categories
                .entry(
                    error
                        .lines()
                        .next()
                        .unwrap_or("Unknown mismatch")
                        .to_owned(),
                )
                .or_default() += 1;
            failures.push(format!("{}: {error}; input {:?}", case.name, case.text));
        }
    }
    assert!(child.wait()?.success(), "Import reference failed");
    eprintln!(
        "{format} wedge flags following the tag or cleared for unsupported orders (#1044, #1048): {tag_wins:?}"
    );
    eprintln!("{format} E/Z kept under wedge flags (#1042): {kept_ez:?}");
    eprintln!(
        "{format} layouts redrawn by the import template retry (#538, #1114): {template_layouts:?}"
    );
    eprintln!(
        "{format} partial imports replacing frozen rejections (#1138): {} {partial_imports:?}",
        partial_imports.len()
    );
    eprintln!(
        "{format} engine responses: {accepted} accepted, {rejected} rejected, {lenient} RDKit lenient CX numbers, {basis_layouts} responses from a different RDKit SSSR basis, {} mismatches",
        failures.len()
    );
    if !failures.is_empty() {
        std::fs::create_dir_all(root.join("artifacts"))?;
        std::fs::write(
            root.join(format!("artifacts/{format}-engine-failures.txt")),
            failures.join("\n"),
        )?;
    }
    eprintln!("{format} engine mismatch categories: {categories:?}");
    eprintln!(
        "{format} engine restrictions: {restrictions:?}; source XYZ {xyz}, H folding {folded}, generic drawings {generic}, acceptance extensions {extensions}"
    );
    reference_presentation::report_mismatches(&format!("{format}-engine"), &failures)?;
    assert!(
        accepted > 500 && rejected > 20,
        "Insufficient import response coverage"
    );
    Ok(())
}

fn strict_error(format: &str, text: &str) -> Option<String> {
    use reshiki::chemistry::{reaction, smiles};
    if format == "smiles" {
        smiles::read(text).err().map(|e| e.to_string())
    } else {
        reaction::read_smiles(text).err().map(|e| e.to_string())
    }
}

#[test]
fn partial_import_exception_requires_replaced_failure_format_and_warning() {
    let warnings = vec!["Bond C1–C2 has any order (~); bond order assumed single".into()];
    let bond_failure = Some("This bond type is not supported yet.");
    for (format, failure, accepted) in [
        ("smiles", bond_failure, true),
        (
            "rsmi",
            Some("Query reaction atoms are not supported yet"),
            true,
        ),
        ("rsmi", Some("bad any cast"), true),
        ("rsmi", Some("bad any_cast"), true),
        ("rsmi", Some("Bad any_cast"), true),
        ("smiles", Some("bad any cast"), false),
        ("smiles", Some("bad any_cast"), false),
        ("rsmi", Some("bad cast"), false),
        ("mol", bond_failure, false),
        ("rsmi", None, false),
        (
            "rsmi",
            Some("This bond type is not supported yet. Extra failure"),
            false,
        ),
        (
            "rsmi",
            Some("Enhanced stereo groups are not supported yet; import was cancelled."),
            false,
        ),
    ] {
        assert_eq!(
            partial_import::replaces_failure(format, failure, &warnings),
            accepted,
            "{format}: {failure:?}"
        );
    }
    assert!(!partial_import::replaces_failure("rsmi", bond_failure, &[]));
    assert!(!partial_import::replaces_failure(
        "rsmi",
        bond_failure,
        &["Extended double-bond stereochemistry (e.g. C=C=C=C) ignored".into()],
    ));
}

fn assert_response_matches(actual: Response, expected: Response) -> TestResult {
    assert_response_matches_with_source(actual, expected, None)
}

fn assert_import_response_matches(
    actual: Response,
    expected: Response,
    format: &str,
) -> TestResult {
    assert_response_matches_contract(actual, expected, None, "import", Some(format))
}

fn assert_response_matches_with_source(
    actual: Response,
    expected: Response,
    source: Option<&reshiki::document::Document>,
) -> TestResult {
    assert_response_matches_contract(actual, expected, source, "", None)
}

fn assert_response_matches_contract(
    actual: Response,
    expected: Response,
    source: Option<&reshiki::document::Document>,
    operation: &str,
    format: Option<&str>,
) -> TestResult {
    let source = source.map(serde_json::to_value).transpose()?;
    let mut actual = serde_json::to_value(actual)?;
    let mut expected = reference_response::expected(expected)?;
    // The Python reference has no display formula or mass percentages.
    for side in [&mut actual, &mut expected] {
        if let Some(analysis) = side
            .get_mut("analysis")
            .and_then(|value| value.as_object_mut())
        {
            analysis.remove("display_formula");
            analysis.remove("mass_percent");
        }
    }
    // #878 keeps singlet spin on two-electron atoms; RDKit keeps only the count.
    for atom in actual["document"]["atoms"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        atom.as_object_mut().map(|atom| atom.remove("singlet"));
    }
    let mut failures = Vec::new();
    if let Err(error) =
        reaction_presentation::adapt_import(operation, format, &actual, &mut expected)
    {
        failures.push(format!("[reaction-ink] {error:#}"));
    }
    if let Err(error) =
        reference_presentation::compare_export_with_source(&actual, &mut expected, source.as_ref())
    {
        failures.push(format!("[presentation] {error:#}"));
    }
    let actual_atoms = actual["document"]["atoms"].clone();
    if let (Some(a), Some(e)) = (
        actual["analysis"].as_object_mut(),
        expected["analysis"].as_object_mut(),
    ) {
        for field in ["mass", "exact_mass", "logp", "tpsa"] {
            let av = a
                .remove(field)
                .and_then(|v| v.as_f64())
                .context("Missing mass")?;
            let ev = e
                .remove(field)
                .and_then(|v| v.as_f64())
                .context("Missing reference mass")?;
            // RDKit wheels may fuse floating-point operations on some targets.
            let ev = if field == "mass" {
                ev + longest_lived_mass::change(&actual_atoms)
            } else {
                ev
            };
            if (av - ev).abs() > ev.abs().max(1.) * 1e-12 {
                failures.push(format!("[analysis] {field}: {av} != {ev}"));
            }
        }
    }
    failures.extend(reference_presentation::response_differences(
        &actual, &expected,
    ));
    if !failures.is_empty() {
        std::fs::create_dir_all("artifacts")?;
        let name = std::thread::current().name().unwrap_or("engine").to_owned();
        std::fs::write(
            format!("artifacts/{name}-actual.json"),
            serde_json::to_vec_pretty(&actual)?,
        )?;
        std::fs::write(
            format!("artifacts/{name}-expected.json"),
            serde_json::to_vec_pretty(&expected)?,
        )?;
        anyhow::bail!(
            "Engine response mismatch\n{}; see artifacts/{name}-{{actual,expected}}.json",
            failures.join("\n")
        );
    }
    Ok(())
}

#[tokio::test]
async fn rust_properties_match_reference_across_editor_operations() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for smiles in [
        "CN",
        "[2H]O[3H]",
        "[NH4+]",
        "[Fe+3].[Cl-].[Cl-].[Cl-]",
        "[CH3]",
        "*CC",
        "C12C3C4C1C5C2C3C45", // Cubane: six symmetric rings, five basis rings.
        "C1CCC2(CC1)CCCC2",   // Spiro junction.
        "c1ccc2occc2c1",      // Fused aromatic rings.
        "CC(=O)N",            // Exclude the amide nitrogen from acceptors.
        "O1CC1",              // Three-membered-ring polar surface contribution.
        "CS(=O)(=O)N",        // Sulfur and nitrogen descriptor rules.
    ] {
        let request = Request::import_smiles(smiles);
        assert_response_matches(
            local
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?,
            reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?,
        )?;
    }
    let document = reference
        .execute(Request::import_smiles("COc1ccccc1"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing reference document")?;
    for operation in ["analyze", "clean", "aromatic", "abbreviate"] {
        let mut request = Request::molecule(operation, document.clone());
        request.selected_ids = Some(document.atoms.iter().map(|a| a.id).collect());
        if operation == "abbreviate" {
            request.text = Some("OMe".into());
        }
        assert_response_matches(
            local
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?,
            reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?,
        )?;
    }
    for format in ["smiles", "inchi", "mol", "cdxml", "cdx"] {
        let mut request = Request::molecule("export", document.clone());
        request.format = Some(format.into());
        assert_response_matches(
            local
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?,
            reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?,
        )?;
    }
    let request = Request::import("rsmi", "[CH3:1][OH:2]>>[CH2:1]=[O:2]");
    assert_import_response_matches(
        local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?,
        reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?,
        "rsmi",
    )?;
    // Cached labels must not affect properties following a topology edit.
    let mut document = reference
        .execute(Request::import_smiles("CC"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing reference document")?;
    for atom in &mut document.atoms {
        atom.label_h = 99;
    }
    for bond in &mut document.bonds {
        bond.order = 2;
    }
    let request = Request::molecule("analyze", document);
    let actual = local
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        actual.analysis.as_ref().map(|a| a.formula.as_str()),
        Some("C2H4")
    );
    assert_response_matches(
        actual,
        reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    Ok(())
}

#[tokio::test]
async fn rust_ring_display_preserves_selected_scope_and_complete_response() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for text in [
        "c1ccccc1.CCO",
        "c1cc[nH]c1",
        "c1ncc[nH]1",
        "c1ccc2ccccc2c1",
        "c1ccc2[nH]ccc2c1",
        "[13cH:90]1ccccc1.[2H]O[3H]",
        "C[C@H](O)c1ccccc1",
        "c1ccccc1/C=C/Cl",
    ] {
        let mut original = reference
            .execute(Request::import_smiles(text))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing ring drawing")?;
        for bond in &mut original.bonds {
            bond.color = reshiki::palette::Color::Custom([25, 60, 190]);
        }
        original.annotations.push(reshiki::document::Annotation {
            id: 987654,
            position: reshiki::document::Point::new(10., 100.),
            text: "試料".into(),
            format: Default::default(),
        });
        let chemical = reshiki::chemistry::document::prepare(&original)?;
        let ring = chemical
            .state
            .rings
            .atoms
            .first()
            .context("Missing aromatic ring")?
            .iter()
            .map(|&i| chemical.ids.get(i).copied().context("Missing ring atom"))
            .collect::<anyhow::Result<Vec<_>>>()?;
        for selected in [original.atoms.iter().map(|a| a.id).collect(), ring] {
            let mut document = original.clone();
            for _ in 0..2 {
                let mut request = Request::molecule("aromatic", document);
                request.selected_ids = Some(selected.clone());
                let expected = reference
                    .execute(request.clone())
                    .await
                    .map_err(anyhow::Error::msg)?;
                let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
                document = actual.document.clone().context("Missing ring display")?;
                assert_response_matches(actual, expected)?;
            }
        }
        for selected in [vec![], vec![987654]] {
            let mut request = Request::molecule("aromatic", original.clone());
            request.selected_ids = Some(selected);
            assert!(reference.execute(request.clone()).await.is_err());
            assert!(local.execute(request).await.is_err());
        }
    }
    Ok(())
}

#[tokio::test]
async fn aromatic_edits_retain_blank_analysis_smiles_for_partial_and_hydrogen_bonds() -> TestResult
{
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for order in [0, 7] {
        let mut document = reference
            .execute(Request::import_smiles("c1ccccc1"))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing ring drawing")?;
        let selected = document.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
        if order == 0 {
            let o = document.add_atom("O", reshiki::document::Point::new(200., 0.));
            let h = document.add_atom("H", reshiki::document::Point::new(228., 0.));
            let acceptor = document.add_atom("O", reshiki::document::Point::new(256., 0.));
            document.add_bond(o, h, 1, "plain");
            document.add_bond(h, acceptor, 0, "dotted");
        } else {
            let a = document.add_atom("C", reshiki::document::Point::new(200., 0.));
            let b = document.add_atom("C", reshiki::document::Point::new(228., 0.));
            document.add_bond(a, b, order, "plain");
        }
        for _ in 0..2 {
            let mut request = Request::molecule("aromatic", document);
            request.selected_ids = Some(selected.clone());
            let expected = reference
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?;
            let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
            assert!(
                actual
                    .analysis
                    .as_ref()
                    .context("Missing analysis")?
                    .smiles
                    .is_empty()
            );
            document = actual.document.clone().context("Missing ring display")?;
            assert_response_matches(actual, expected)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn rust_prepared_drawings_preserve_full_analysis_and_molecular_exports() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let templates: serde_json::Value =
        serde_json::from_str(include_str!("../assets/templates.json"))?;
    let mut texts: Vec<_> = templates
        .as_array()
        .context("Missing templates")?
        .iter()
        .filter_map(|t| t["smiles"].as_str())
        .collect();
    texts.extend([
        "[2H]O[3H]",
        "[13CH3][NH3+]",
        "[CH3]",
        "[CH2]",
        "[O]",
        "[Na+].[Cl-]",
        "F/C=C/F",
        "F/C=C\\F",
        "F/C=C/C=C/Cl",
        "F[C@](Cl)(Br)I",
        "C[C@H]1CCC[C@@H](C)C1",
        "C[S@](=O)CC",
        "C1CCC2(CC1)CCCC2",
        "C12C3C4C1C5C2C3C45",
        "[NH3]->[Cu+2]<-[NH3]",
        "[CH3:1][OH:9]",
    ]);
    for text in texts {
        let mut doc = reference
            .execute(Request::import_smiles(text))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing reference drawing")?;
        // Reordering changes toolkit indices; stable IDs and explicit winding
        // must survive, while stale display labels must have no chemical effect.
        if text == "[CH3:1][OH:9]" {
            doc.atoms.last_mut().context("Missing mapped atom")?.map_num = i32::MAX as u32;
        }
        doc.atoms.reverse();
        doc.bonds.reverse();
        for a in &mut doc.atoms {
            a.position.x = -a.position.x;
            a.label_h = 99;
        }
        for b in &mut doc.bonds {
            b.color = reshiki::palette::Color::Custom([75, 125, 160]);
        }
        for format in [None, Some("smiles"), Some("mol")] {
            let mut request = Request::molecule(
                if format.is_some() {
                    "export"
                } else {
                    "analyze"
                },
                doc.clone(),
            );
            request.format = format.map(String::from);
            let expected = reference.execute(request.clone()).await;
            let actual = local.execute(request).await;
            match (actual, expected) {
                (Ok(a), Ok(e)) => {
                    assert_response_matches(a, e).with_context(|| format!("{text}/{format:?}"))?
                }
                (Err(_), Err(_)) => anyhow::bail!("Unexpected rejection of {text}/{format:?}"),
                (a, e) => anyhow::bail!("Response mismatch for {text}/{format:?}: {a:?} vs {e:?}"),
            }
        }
    }
    // A chemistry failure must leave the input unchanged and the next request usable.
    let mut invalid = reshiki::document::Document::default();
    let center = invalid.add_atom("C", Default::default());
    for i in 0..5 {
        let other = invalid.add_atom("F", reshiki::document::Point::new(i as f32 * 28., 28.));
        invalid.add_bond(center, other, 1, "plain");
    }
    let before = invalid.clone();
    assert!(
        local
            .execute(Request::molecule("analyze", invalid.clone()))
            .await
            .is_err()
    );
    assert_eq!(before, invalid);
    let valid = reference
        .execute(Request::import_smiles("CO"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing valid drawing")?;
    let req = Request::molecule("analyze", valid);
    assert_response_matches(
        local
            .execute(req.clone())
            .await
            .map_err(anyhow::Error::msg)?,
        reference.execute(req).await.map_err(anyhow::Error::msg)?,
    )?;
    Ok(())
}

#[tokio::test]
async fn dense_ring_pruning_matches_complete_reference_responses() -> TestResult {
    rdkit_parity::enable();
    use reshiki::{chemistry::graph::Graph, document::Document};
    let graph: Graph =
        serde_json::from_str(include_str!("../tests/fixtures/ring-order-dependent.json"))?;
    let mut document = Document::default();
    let ids = (0..graph.atoms.len())
        .map(|i| {
            document.add_atom(
                "*",
                reshiki::document::Point {
                    x: i as f32 * 42.,
                    y: 0.,
                },
            )
        })
        .collect::<Vec<_>>();
    for bond in &graph.bonds {
        document.add_bond(ids[bond.a], ids[bond.b], 1, "plain");
    }
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for (operation, format) in [
        ("analyze", None),
        ("export", Some("smiles")),
        ("export", Some("mol")),
        ("export", Some("cdxml")),
        ("export", Some("cdx")),
    ] {
        let mut request = Request::molecule(operation, document.clone());
        request.format = format.map(str::to_owned);
        let mut expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        // RDKit 2026.09 takes a 21-ring SSSR basis from RingDecomposerLib
        // (rdkit#9105) and symmetrizes it to 34 rings; ReShiki keeps the
        // 20-ring Figueras basis, which symmetrizes to 33 rings now that its
        // candidates are ranked with a stable sort (32 with the old libc++
        // and libstdc++ tie order, 33 with MSVC's). The rest must match.
        if let Some(analysis) = &mut expected.analysis {
            assert_eq!(analysis.rings, 34);
            analysis.rings = 33;
        }
        assert_response_matches(
            local.execute(request).await.map_err(anyhow::Error::msg)?,
            expected,
        )?;
    }
    // Use a supported element with unrestricted valence: native RXN export
    // writes dummy atoms as R labels, which the original analysis cannot read.
    for atom in &mut document.atoms {
        atom.element = "Fe".into();
    }
    let product = document.add_atom("O", reshiki::document::Point::new(600., 0.));
    let arrow = product.checked_add(1).context("Arrow ID overflow")?;
    document.arrows.push(reshiki::document::Arrow::new(
        arrow,
        reshiki::document::Point::new(420., 0.),
        reshiki::document::Point::new(560., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut roles = reshiki::reactions::Reaction::new(arrow);
    roles.reactants.push(reshiki::reactions::Participant {
        atoms: ids,
        coefficient: 1,
    });
    roles.products.push(reshiki::reactions::Participant {
        atoms: vec![product],
        coefficient: 1,
    });
    document.reactions.push(roles);
    let mut request = Request::molecule("export", document);
    request.format = Some("rxn".into());
    let block = reference
        .execute(request)
        .await
        .map_err(anyhow::Error::msg)?
        .output
        .context("Missing dense-ring reaction")?;
    reshiki::chemistry::reaction::read_rxn(&block)?.drawing()?;
    let request = Request::import("rxn", &block);
    let mut expected = reference
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    // The same ring graph as above, so the same basis difference.
    if let Some(analysis) = &mut expected.analysis {
        assert_eq!(analysis.rings, 34);
        analysis.rings = 33;
    }
    assert_import_response_matches(
        local.execute(request).await.map_err(anyhow::Error::msg)?,
        expected,
        "rxn",
    )?;
    Ok(())
}

#[tokio::test]
async fn rust_mol_files_roundtrip_stereo_isotopes_charges_and_maps() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for text in [
        "CCO",
        "[13CH3:90][NH3+]",
        "[2H]O[3H]",
        "[Na+].[Cl-]",
        "[CH3]",
        "N[C@H](C)C(=O)O",
        "N[C@@H](C)C(=O)O",
        "F/C=C/Cl",
        "F/C=C\\Cl",
        "F[C@](Cl)(Br)I",
        "c1ccc2[nH]ccc2c1",
        "N->[Cu+2]",
    ] {
        let original = reference
            .execute(Request::import_smiles(text))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = original.analysis.context("Missing starting identity")?;
        let mut request =
            Request::molecule("export", original.document.context("Missing drawing")?);
        request.format = Some("mol".into());
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
        assert_response_matches(actual.clone(), expected)?;
        let output = actual.output.context("Missing MOL output")?;
        assert_eq!(output.contains("V3000"), text.contains("->"));
        let back = reference
            .execute(Request::import("mol", &output))
            .await
            .map_err(anyhow::Error::msg)?;
        let identity = back.analysis.context("Missing reimported identity")?;
        assert_eq!(identity.smiles, analysis.smiles, "{text}");
        assert_eq!(identity.inchikey, analysis.inchikey, "{text}");
        assert_eq!(identity.formula, analysis.formula, "{text}");
        assert_eq!(identity.exact_mass, analysis.exact_mass, "{text}");
        assert_eq!(
            identity.unpaired_electrons, analysis.unpaired_electrons,
            "{text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn mol_dummy_atoms_retain_the_reference_query_import_rejection() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let document = reference
        .execute(Request::import_smiles("*CC"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing dummy drawing")?;
    let mut request = Request::molecule("export", document);
    request.format = Some("mol".into());
    let expected = reference
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
    assert_response_matches(actual.clone(), expected)?;
    let text = actual.output.context("Missing dummy MOL output")?;
    // The reference writes a generic R atom, which its reader treats as a
    // query. The drawing model currently rejects such queries on import.
    for result in [
        local.execute(Request::import("mol", &text)).await,
        reference.execute(Request::import("mol", &text)).await,
    ] {
        assert!(result.is_err_and(|error| error.to_ascii_lowercase().contains("query")));
    }
    Ok(())
}

#[tokio::test]
async fn native_drawings_match_the_original_python_importer() -> TestResult {
    rdkit_parity::enable();
    use reshiki::{abbreviations::LabelAlignment, typography::TextStyle};
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for (data, labels) in [
        (
            include_bytes!("../tests/fixtures/native-ethyl-clipboard.cdx").as_slice(),
            vec![],
        ),
        (
            include_bytes!("../tests/fixtures/abbreviations-native.cdx").as_slice(),
            vec![
                ("Boc", LabelAlignment::Left),
                ("OMe", LabelAlignment::Right),
            ],
        ),
        (
            include_bytes!("../tests/fixtures/picture-group-native.cdx").as_slice(),
            vec![],
        ),
    ] {
        let request = Request::import("cdx", &STANDARD.encode(data));
        let mut expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        // This fixture explicitly uses Arial 10 black for Boc/OMe, independent
        // of its Helvetica 10 black inner atoms, plus Left/Right justification.
        // The historical Python importer discarded these presentations. Extend
        // only these known fields from the source fixture; every other response
        // field still has to match the original oracle. The exact fixture's
        // import and editable roundtrips are also checked in
        // abbreviation_style_exchange::native_boc_and_ome_keep_independent_fonts.
        let document = expected
            .document
            .as_mut()
            .context("Missing reference drawing")?;
        assert_eq!(document.abbreviations.len(), labels.len());
        for (group, (label, alignment)) in document.abbreviations.iter_mut().zip(labels) {
            assert_eq!(group.label, label);
            group.alignment = alignment;
            group.label_style = Some(TextStyle {
                family: "Arial".into(),
                size_pt: 10.,
                ..Default::default()
            });
            group.label_color_override = true;
            let anchor = document
                .atoms
                .iter_mut()
                .find(|atom| atom.id == group.anchor)
                .context("Missing reference abbreviation anchor")?;
            anchor.text_style = Some(TextStyle {
                family: "Helvetica".into(),
                size_pt: 10.,
                ..Default::default()
            });
            anchor.display.color_override = true;
        }
        let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
        assert_response_matches(actual, expected)?;
    }
    Ok(())
}

#[tokio::test]
async fn exports_and_reimports_preserve_rdkit_chemistry_and_document_metadata() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let mut failures = Vec::new();
    for smiles in [
        "CCO",
        "N[C@@H](C)C(=O)O",
        "N[C@H](C)C(=O)O",
        "[13CH3][NH3+]",
        "[Na+].[Cl-]",
        "F/C=C/F",
        "F/C=C\\F",
        "[CH3]",
        "c1ccc2occc2c1",
        "C1CC2CCC1C2",
    ] {
        let initial = reference
            .execute(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = initial.analysis.context("Missing reference analysis")?;
        let mut document = initial.document.context("Missing reference drawing")?;
        for bond in &mut document.bonds {
            bond.color = reshiki::palette::Color::Custom([180, 50, 55]);
        }
        let mut request = Request::molecule("export", document);
        request.format = Some("cdx".into());
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let source = request.document.clone();
        let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
        if let Err(error) =
            assert_response_matches_with_source(actual.clone(), expected, source.as_ref())
        {
            failures.push(format!("{smiles}/export/cdx: {error:#}"));
        }
        let data = actual.output.context("Missing binary export")?;
        let request = Request::import("cdx", &data);
        let back = local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let oracle = reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?;
        if let Err(error) = assert_response_matches(back.clone(), oracle) {
            failures.push(format!("{smiles}/reimport/cdx: {error:#}"));
        }
        let identity = back.analysis.context("Missing round-trip analysis")?;
        assert_eq!(analysis.smiles, identity.smiles, "{smiles}");
        assert_eq!(analysis.inchikey, identity.inchikey, "{smiles}");
        assert_eq!(analysis.formula, identity.formula, "{smiles}");
        assert_eq!(analysis.exact_mass, identity.exact_mass, "{smiles}");
    }
    eprintln!(
        "exports_and_reimports_preserve_rdkit_chemistry_and_document_metadata: {} export/response mismatches",
        failures.len()
    );
    reference_presentation::report_mismatches(
        "exports_and_reimports_preserve_rdkit_chemistry_and_document_metadata",
        &failures,
    )?;
    Ok(())
}

#[tokio::test]
async fn supported_figure_exports_are_byte_identical_to_python() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let mut failures = Vec::new();
    for name in [
        "bond-styles-chemdraw",
        "formatted-label-chemdraw",
        "graphics-chemdraw",
        "symbols-chemdraw",
        "arrows-chemdraw",
        "atom-labels-chemdraw",
        "attached-symbols-chemdraw",
        "grouped-aspirin-chemdraw",
        "ring-presets-chemdraw",
        "aromatic-circle-native",
    ] {
        let xml = std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}.cdxml",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        let doc = reference
            .execute(Request::import("cdxml", &xml))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing drawing")?;
        if !doc.atoms.is_empty() {
            let analyze = Request::molecule("analyze", doc.clone());
            assert_response_matches(
                local
                    .execute(analyze.clone())
                    .await
                    .map_err(anyhow::Error::msg)?,
                reference
                    .execute(analyze)
                    .await
                    .map_err(anyhow::Error::msg)?,
            )?;
        }
        let mut request = Request::molecule("export", doc);
        request.format = Some("cdx".into());
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let source = request.document.clone();
        let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
        if let Err(error) = assert_response_matches_with_source(actual, expected, source.as_ref()) {
            failures.push(format!("{name}/export/cdx: {error:#}"));
        }
    }
    eprintln!(
        "supported_figure_exports_are_byte_identical_to_python: {} export/response mismatches",
        failures.len()
    );
    reference_presentation::report_mismatches(
        "supported_figure_exports_are_byte_identical_to_python",
        &failures,
    )?;
    Ok(())
}

#[tokio::test]
async fn query_predicates_still_fail_chemistry_validation() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for (name, value) in [
        ("RingBondCount", "NoRingBonds"),
        ("SubstituentsExactly", "0"),
        ("RxnStereo", "Inversion"),
    ] {
        let xml = format!(
            "<CDXML><page id=\"1\"><fragment id=\"2\"><n id=\"3\" p=\"0 0\" Element=\"6\" {name}=\"{value}\"/></fragment></page></CDXML>"
        );
        let bytes = reshiki::exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
        let request = Request::import("cdx", &STANDARD.encode(bytes));
        let expected = reference.execute(request.clone()).await;
        let actual = local.execute(request).await;
        assert!(expected.is_err());
        assert!(actual.is_err());
        assert_eq!(expected.err(), actual.err());
    }
    Ok(())
}

#[tokio::test]
async fn rust_smiles_export_preserves_analysis_and_complete_drawing() -> TestResult {
    rdkit_parity::enable();
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for text in [
        "CC(=O)Oc1ccccc1C(=O)O",
        "O.[Na+].[Cl-]",
        "F[C@](Cl)(Br)I",
        "F/C=C/F",
        "F/C=C\\F",
        "C[C@H]1CC[C@@H](C)CC1.O",
        "C[C@H](F)Cl.C[C@@H](F)Cl",
        "[13CH3:0][NH3+]",
        "N->[Cu]<-N",
        "[Pt@SP1](Cl)(F)(Br)I",
        "[P@TB5](F)(Cl)(Br)(I)N",
        "[Co@OH23](F)(Cl)(Br)(I)(N)O",
    ] {
        let partial_configuration = match text {
            "[Pt@SP1](Cl)(F)(Br)I" => Some((
                "Pt",
                "ReShiki cannot draw the square-planar configuration (@SP) at Pt1; it imported that atom without a configuration",
            )),
            "[P@TB5](F)(Cl)(Br)(I)N" => Some((
                "P",
                "ReShiki cannot draw the trigonal-bipyramidal configuration (@TB) at P1; it imported that atom without a configuration",
            )),
            "[Co@OH23](F)(Cl)(Br)(I)(N)O" => Some((
                "Co",
                "ReShiki cannot draw the octahedral configuration (@OH) at Co1; it imported that atom without a configuration",
            )),
            _ => None,
        };
        let imported = if let Some((element, warning)) = partial_configuration {
            let failure = reference
                .execute(Request::import_smiles(text))
                .await
                .expect_err("Frozen worker accepted unsupported coordination stereo");
            assert_eq!(
                failure, "This stereochemistry class is not supported yet.",
                "{text}"
            );
            let actual = local
                .execute(Request::import_smiles(text))
                .await
                .map_err(anyhow::Error::msg)?;
            assert_eq!(actual.warnings, [warning], "{text}");
            let center = actual
                .document
                .as_ref()
                .context("Missing partial-import drawing")?
                .atoms
                .iter()
                .find(|atom| atom.element == element)
                .context("Missing coordination center")?;
            assert!(center.stereo.is_none(), "{text}: configuration retained");
            assert!(
                center.cip_label.is_none(),
                "{text}: configuration label retained"
            );
            actual
        } else {
            match reference.execute(Request::import_smiles(text)).await {
                Ok(response) => response,
                Err(_) => {
                    let actual = local.execute(Request::import_smiles(text)).await;
                    assert!(
                        actual.is_err(),
                        "Unsupported native drawing was accepted: {text}"
                    );
                    continue;
                }
            }
        };
        let document = imported.document.context("Missing drawing")?;
        let before = document.clone();
        for (operation, format) in [("analyze", None), ("export", Some("smiles"))] {
            let mut request = Request::molecule(operation, document.clone());
            request.format = format.map(str::to_owned);
            let actual = local
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?;
            let expected = reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?;
            assert_response_matches(actual, expected)
                .with_context(|| format!("{text}/{operation}"))?;
        }
        assert_eq!(document, before);
    }
    Ok(())
}

#[tokio::test]
async fn overlapping_smiles_exports_keep_each_analysis_with_its_snapshot() -> TestResult {
    rdkit_parity::enable();
    let reference = PythonEngine::default();
    let local = LocalEngine::default();
    let mut tasks = tokio::task::JoinSet::new();
    for text in ["CCO", "F/C=C\\F", "F[C@](Cl)(Br)I", "c1ccccc1.O.[Na+]"] {
        let document = reference
            .execute(Request::import_smiles(text))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing drawing")?;
        let mut request = Request::molecule("export", document);
        request.format = Some("smiles".into());
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let engine = local.clone();
        tasks.spawn(async move { (engine.execute(request).await, expected) });
    }
    while let Some(result) = tasks.join_next().await {
        let (actual, expected) = result?;
        assert_response_matches(actual.map_err(anyhow::Error::msg)?, expected)?;
    }
    Ok(())
}
