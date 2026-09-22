//! Complete-response differential tests against the unchanged PythonEngine.
//! Reference requests use Request -> Value -> JSON and JSON -> Value -> Response
//! through the real engine transport, including both f32 conversion boundaries.
use anyhow::Context;
use reshiki::{
    chemistry::{document as molecular, inchi::generator},
    document::{Annotation, Document, Point},
    engine::{
        ChemistryEngine, PythonEngine, Request, Response,
        native_response::{self, Config, Error, Prepared},
    },
};
use std::{path::PathBuf, sync::Arc, time::Duration};

fn helper(name: &str) -> anyhow::Result<Option<PathBuf>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts/inchi-helper")
        .join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
    if path.is_file() {
        return Ok(Some(path));
    }
    anyhow::ensure!(
        std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
        "Build the pinned standalone InChI helper first"
    );
    eprintln!("Skipping optional native response tests; build scripts/build_inchi_helper.py first");
    Ok(None)
}

async fn prepare(request: Arc<Request>) -> anyhow::Result<Option<Arc<Prepared>>> {
    tokio::task::spawn_blocking(move || {
        let Some(document) = &request.document else {
            return Ok(None);
        };
        if document.atoms.is_empty() && request.operation != "finish_abbreviation" {
            return Ok(None);
        }
        let molecule = molecular::prepare(document)?;
        let drawing = molecular::for_drawing(&molecule, document)?;
        Ok(Some(Arc::new(Prepared { molecule, drawing })))
    })
    .await?
}

fn equal(actual: Response, expected: Response) -> anyhow::Result<()> {
    let mut actual = serde_json::to_value(actual)?;
    let mut expected = serde_json::to_value(expected)?;
    for field in ["mass", "exact_mass", "logp", "tpsa"] {
        if let (Some(a), Some(e)) = (
            actual["analysis"][field].as_f64(),
            expected["analysis"][field].as_f64(),
        ) {
            anyhow::ensure!(
                (a - e).abs() <= e.abs().max(1.) * 1e-12,
                "{field}: {a} != {e}"
            );
            actual["analysis"][field] = serde_json::Value::Null;
            expected["analysis"][field] = serde_json::Value::Null;
        }
    }
    anyhow::ensure!(
        actual == expected,
        "Response mismatch:\nactual={actual}\nexpected={expected}"
    );
    Ok(())
}

async fn compare(
    reference: &PythonEngine,
    config: &Config,
    request: Request,
) -> anyhow::Result<bool> {
    let original = serde_json::to_value(&request)?;
    let expected = reference.execute(request.clone()).await;
    let request = Arc::new(request);
    let prepared = prepare(Arc::clone(&request)).await?;
    let before = prepared
        .as_ref()
        .map(|p| serde_json::to_value(&p.molecule))
        .transpose()?;
    let actual =
        native_response::build(Arc::clone(&request), prepared.clone(), config.clone()).await;
    anyhow::ensure!(
        serde_json::to_value(&*request)? == original,
        "Request changed"
    );
    anyhow::ensure!(
        prepared
            .as_ref()
            .map(|p| serde_json::to_value(&p.molecule))
            .transpose()?
            == before,
        "Prepared chemistry changed"
    );
    match (actual, expected) {
        (Ok(a), Ok(e)) => {
            equal(a, e)?;
            Ok(true)
        }
        (Err(a), Err(e)) => {
            anyhow::ensure!(a.to_string() == e, "Error changed: {a} != {e}");
            Ok(false)
        }
        (a, e) => anyhow::bail!("Outcome changed: {a:?} != {e:?}"),
    }
}

async fn imported(reference: &PythonEngine, text: &str) -> anyhow::Result<Document> {
    reference
        .execute(Request::import_smiles(text))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing imported drawing")
}

async fn all_operations(
    reference: &PythonEngine,
    config: &Config,
    document: &Document,
    name: &str,
) -> anyhow::Result<usize> {
    let mut count = 0;
    for (operation, format) in [
        ("analyze", None),
        ("finish_abbreviation", None),
        ("export", Some("smiles")),
        ("export", Some("mol")),
        ("export", Some("inchi")),
        ("export", Some("cdxml")),
        ("export", Some("cdx")),
    ] {
        let mut request = Request::molecule(operation, document.clone());
        request.format = format.map(str::to_owned);
        compare(reference, config, request)
            .await
            .with_context(|| format!("{name}: {operation}/{format:?}"))?;
        count += 1;
    }
    Ok(count)
}

#[tokio::test]
async fn complete_responses_match_original_worker_for_molecules_and_exports() -> anyhow::Result<()>
{
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let config = Config::new(path);
    let reference = PythonEngine::default();
    let mut texts = reshiki::templates::LIBRARY
        .iter()
        .map(|t| t.smiles.to_string())
        .collect::<Vec<_>>();
    texts.extend(
        [
            "C",
            "O",
            "*",
            "[CH3]",
            "[CH2]",
            "[O]",
            "[He]",
            "[H][H]",
            "[13CH3:90][NH3+]",
            "[2H]O[3H]",
            "[Na+].[Cl-]",
            "[Fe+2].[Cl-].[Cl-]",
            "F/C=C/F",
            "F/C=C\\F",
            "F/C=C/C=C/Cl",
            "F[C@](Cl)(Br)I",
            "C[C@H]1CCC[C@@H](C)C1",
            "C[S@](=O)CC",
            "C1CCC2(CC1)CCCC2",
            "C12C3C4C1C5C2C3C45",
            "[NH3]->[Cu+2]<-[NH3]",
            "[Mo]$[Mo]",
            "C[N+]([O-])=O",
            "O=S(=O)(O)O",
            "O=Cl(=O)(=O)O",
            "CC(=O)C=C(O)C",
            "COc1ccc(NC(=O)OC(C)(C)C)cc1",
            "F[C@H](Cl)[C@@H](Br)I",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    texts.sort();
    texts.dedup();
    let mut count = 0;
    for text in texts {
        let mut document = imported(&reference, &text)
            .await
            .with_context(|| text.clone())?;
        document.atoms.reverse();
        document.bonds.reverse();
        for atom in &mut document.atoms {
            atom.position.x = -atom.position.x;
            atom.label_h = 99;
            atom.cip_label = Some("R".into());
        }
        count += all_operations(&reference, &config, &document, &text).await?;
    }
    eprintln!("Compared {count} complete molecular responses with the original worker");
    Ok(())
}

#[tokio::test]
async fn empty_exotic_and_unsupported_requests_keep_native_rules() -> anyhow::Result<()> {
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let config = Config::new(path);
    let reference = PythonEngine::default();
    all_operations(&reference, &config, &Document::default(), "empty").await?;
    let mut figure = Document::default();
    figure.annotations.push(Annotation {
        id: 10,
        position: Point::new(0.1, -0.2),
        text: "αβ\n試料".into(),
        format: Default::default(),
    });
    all_operations(&reference, &config, &figure, "figure only").await?;
    for (order, display) in [(0, "dotted"), (5, "plain"), (6, "plain"), (7, "plain")] {
        let mut document = Document::default();
        let a = document.add_atom(
            if order == 5 {
                "N"
            } else if order == 0 {
                "H"
            } else {
                "*"
            },
            Point::new(-5.25, 0.1),
        );
        let b = document.add_atom(
            if order == 5 {
                "Cu"
            } else if order == 0 {
                "O"
            } else {
                "*"
            },
            Point::new(37.25, 0.1),
        );
        document.add_bond(a, b, order, display);
        if order == 0 {
            let donor = document.add_atom("N", Point::new(-47.25, 0.1));
            document.add_bond(donor, a, 1, "plain");
        }
        all_operations(&reference, &config, &document, &format!("order={order}")).await?;
        // These bonds suppress InChI before helper lookup, even for valid SMILES.
        let request = Arc::new(Request::molecule("analyze", document));
        let prepared = prepare(Arc::clone(&request)).await?;
        let response = native_response::build(
            request,
            prepared,
            Config::new(PathBuf::from("missing-helper")),
        )
        .await?;
        let analysis = response.analysis.context("Missing analysis")?;
        assert!(analysis.inchi.is_empty() && analysis.inchikey.is_empty());
        if matches!(order, 0 | 7) {
            assert!(analysis.smiles.is_empty());
        }
    }
    let document = imported(&reference, "CO").await?;
    for (operation, format, protocol) in [
        ("export", Some("invalid"), 1),
        ("export", None, 1),
        ("invalid", None, 1),
        ("analyze", None, 2),
    ] {
        let mut request = Request::molecule(operation, document.clone());
        request.format = format.map(str::to_owned);
        request.protocol = protocol;
        compare(&reference, &config, request).await?;
    }
    let mut missing = Request::molecule("analyze", document);
    missing.document = None;
    compare(&reference, &config, missing).await?;
    Ok(())
}

#[tokio::test]
async fn styles_high_ids_groups_abbreviations_and_reaction_warnings_survive() -> anyhow::Result<()>
{
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let config = Config::new(path);
    let reference = PythonEngine::default();
    let original = imported(&reference, "CCOC").await?;
    let abbreviated = reference
        .execute(Request::molecule("abbreviate", original))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing abbreviation")?;
    all_operations(&reference, &config, &abbreviated, "abbreviations").await?;
    let reaction = reference
        .execute(Request::import("rsmi", "CCO.O>O>CC=O.O"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing reaction")?;
    all_operations(&reference, &config, &reaction, "reaction warnings").await?;
    let mut document = imported(&reference, "N[C@@H](C)C(=O)O").await?;
    let offset = (1u64 << 53) + 100;
    for atom in &mut document.atoms {
        atom.id += offset;
        if let Some(stereo) = &mut atom.stereo {
            for id in &mut stereo.neighbors {
                *id += offset;
            }
        }
        atom.position.x += 0.1;
        atom.position.y -= 0.2;
        atom.text_style = Some(reshiki::typography::TextStyle {
            size_pt: 11.3,
            color: [14, 83, 167],
            ..Default::default()
        });
        atom.cip_label = Some("R".into());
    }
    for bond in &mut document.bonds {
        bond.a += offset;
        bond.b += offset;
        for id in &mut bond.stereo_atoms {
            *id += offset;
        }
        bond.color = [35, 156, 103];
        bond.z_order = -3;
    }
    document.drawing_style.name = "User style".into();
    document.drawing_style.set_bond_length(16.3);
    let id = document.next_id();
    document.annotations.push(Annotation {
        id,
        position: Point::new(-10.1, 77.7),
        text: "試料 α\nH₂O".into(),
        format: Default::default(),
    });
    document.groups.push(reshiki::grouping::Group {
        id: id + 1,
        members: document.atoms.iter().map(|a| a.id).chain([id]).collect(),
        integral: true,
    });
    all_operations(&reference, &config, &document, "styled high IDs").await?;
    Ok(())
}

#[tokio::test]
async fn helper_resource_failures_are_typed_atomic_and_recoverable() -> anyhow::Result<()> {
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let reference = PythonEngine::default();
    let request = Arc::new(Request::molecule(
        "analyze",
        imported(&reference, "CCO").await?,
    ));
    let prepared = prepare(Arc::clone(&request)).await?;
    let before = serde_json::to_value(&*request)?;
    let mut config = Config::new(path);
    config.limits.kernel_heap_bytes = 1;
    let result =
        native_response::build(Arc::clone(&request), prepared.clone(), config.clone()).await;
    assert!(matches!(
        result,
        Err(Error::Helper(generator::Error::ResourceLimit { .. }))
    ));
    assert_eq!(before, serde_json::to_value(&*request)?);
    config.limits.kernel_heap_bytes = generator::DEFAULT_KERNEL_HEAP_BYTES;
    let response =
        native_response::build(Arc::clone(&request), prepared.clone(), config.clone()).await?;
    equal(
        response,
        reference
            .execute((*request).clone())
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    config.limits.timeout = Duration::ZERO;
    assert!(matches!(
        native_response::build(request, prepared, config).await,
        Err(Error::Helper(generator::Error::Input(_)))
    ));
    Ok(())
}

#[tokio::test]
async fn overlapping_builds_retain_their_immutable_snapshots() -> anyhow::Result<()> {
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let reference = PythonEngine::default();
    let mut tasks = tokio::task::JoinSet::new();
    for text in ["C", "O", "c1ccccc1", "C[C@H](N)C(=O)O", "F/C=C/F", "*"] {
        let request = Arc::new(Request::molecule(
            "analyze",
            imported(&reference, text).await?,
        ));
        let expected = reference
            .execute((*request).clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let prepared = prepare(Arc::clone(&request)).await?;
        let config = Config::new(path.clone());
        tasks.spawn(async move {
            equal(
                native_response::build(request, prepared, config).await?,
                expected,
            )
        });
    }
    while let Some(result) = tasks.join_next().await {
        result??;
    }
    Ok(())
}

#[tokio::test]
async fn shared_prepared_analysis_preserves_chemical_state_and_native_results() -> anyhow::Result<()>
{
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let reference = PythonEngine::default();
    for text in [
        "C",
        "*",
        "F[C@](Cl)(Br)I",
        "F/C=C/F",
        "[13CH3:41][NH3+]",
        "[NH3]->[Cu+2]",
        "[Mo]$[Mo]",
    ] {
        let document = imported(&reference, text).await?;
        let request = Request::molecule("analyze", document.clone());
        let expected = reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?;
        let molecule = Arc::new(molecular::prepare(&document)?);
        let before = serde_json::to_value(&*molecule)?;
        let analysis = native_response::analyze_prepared(
            Arc::clone(&molecule),
            Some(Config::new(path.clone())),
        )
        .await?;
        let actual = Response {
            analysis: Some(analysis),
            ..expected.clone()
        };
        equal(actual, expected)?;
        assert_eq!(before, serde_json::to_value(&*molecule)?);
    }
    Ok(())
}

#[tokio::test]
async fn native_adapter_early_empty_result_does_not_require_a_helper() -> anyhow::Result<()> {
    let reference = PythonEngine::default();
    let mut document = Document::default();
    let center = document.add_atom("*", Point::default());
    for index in 0..21 {
        let neighbor = document.add_atom("F", Point::new(index as f32 * 28., 42.));
        document.add_bond(center, neighbor, 1, "plain");
    }
    let request = Arc::new(Request::molecule("analyze", document));
    let prepared = prepare(Arc::clone(&request)).await?;
    let molecule = &prepared
        .as_ref()
        .context("Missing prepared state")?
        .molecule;
    assert!(matches!(
        reshiki::chemistry::inchi::input::prepare(&molecule.state, Some(&molecule.positions)),
        Err(reshiki::chemistry::inchi::input::Error::NativeEmpty(_))
    ));
    let expected = reference
        .execute((*request).clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let actual = native_response::build(
        request,
        prepared,
        Config::new(PathBuf::from("missing-helper")),
    )
    .await?;
    assert!(
        actual
            .analysis
            .as_ref()
            .context("Missing analysis")?
            .inchi
            .is_empty()
    );
    equal(actual, expected)
}

#[tokio::test]
async fn native_drawing_precision_and_measurements_survive_the_real_transport() -> anyhow::Result<()>
{
    use reshiki::{
        graphics::{Graphic, GraphicKind, GraphicStyle},
        scientific::{AtomMark, MarkKind, OrbitalKind, SymbolKind},
        typography::{TextAlign, TextSpan, TextStyle},
    };
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let config = Config::new(path);
    let reference = PythonEngine::default();
    for x in [
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        0.1,
        -0.2,
        -234_567.13,
        456_789.25,
    ] {
        let mut document = Document::default();
        document.add_atom("C", Point::new(x, -x));
        all_operations(&reference, &config, &document, &format!("f32/{x:?}")).await?;
    }
    let mut document = imported(&reference, "N[C@@H](C)C(=O)O").await?;
    document.page_layout = Some(Default::default());
    document
        .atoms
        .first_mut()
        .context("Missing atom")?
        .marks
        .push(AtomMark {
            kind: MarkKind::LonePair,
            offset: Point::new(-9.1, -13.7),
            angle: 31.3,
            size_pt: Some(3.7),
        });
    let text = "試料 α\nH2O";
    let annotation = document.next_id();
    document.annotations.push(Annotation {
        id: annotation,
        position: Point::new(-45.25, 85.75),
        text: text.into(),
        format: reshiki::typography::TextFormat {
            alignment: TextAlign::Right,
            line_spacing: 1.3,
            width_pt: Some(74.25),
            spans: vec![TextSpan {
                start: 0,
                end: "試料 α".len(),
                style: TextStyle {
                    bold: true,
                    color: [70, 20, 30],
                    ..Default::default()
                },
            }],
            ..Default::default()
        },
    });
    for kind in [
        GraphicKind::Rectangle,
        GraphicKind::Curve,
        GraphicKind::Symbol(SymbolKind::CirclePlus),
        GraphicKind::Orbital(OrbitalKind::P),
    ] {
        document.graphics.push(Graphic::dragged(
            document.next_id(),
            kind,
            Point::new(120.5, 70.25),
            Point::new(191.5, 95.75),
            GraphicStyle::default(),
            Default::default(),
            false,
        ));
    }
    all_operations(&reference, &config, &document, "rich scene").await?;
    // The builder must consume the supplied renderer measurements, including
    // deliberately different values; regenerating them changes CDXML bounds.
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", document.clone());
        request.format = Some(format.into());
        if let Some(metrics) = request
            .text_layout
            .as_mut()
            .and_then(|m| m.get_mut(&annotation))
        {
            metrics.width = 63.123_455;
            metrics.height = 27.654_322;
            metrics.baseline = 5.125;
        }
        compare(&reference, &config, request).await?;
    }
    Ok(())
}

#[tokio::test]
async fn malformed_prepared_inputs_and_stage_order_do_not_publish_partial_results()
-> anyhow::Result<()> {
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let reference = PythonEngine::default();
    let document = imported(&reference, "CO").await?;
    let request = Arc::new(Request::molecule("analyze", document.clone()));
    let mut molecule = molecular::prepare(&document)?;
    let drawing = molecular::for_drawing(&molecule, &document)?;
    *molecule.ids.first_mut().context("Missing identity")? += 100;
    assert!(matches!(
        native_response::build(
            Arc::clone(&request),
            Some(Arc::new(Prepared { molecule, drawing })),
            Config::new(path.clone())
        )
        .await,
        Err(Error::Identity)
    ));
    let mut molecule = molecular::prepare(&document)?;
    let drawing = molecular::for_drawing(&molecule, &document)?;
    molecule.state.hybridizations.clear();
    assert!(matches!(
        native_response::build(
            Arc::clone(&request),
            Some(Arc::new(Prepared { molecule, drawing })),
            Config::new(path.clone())
        )
        .await,
        Err(Error::Drawing(_))
    ));
    assert!(matches!(
        native_response::build(request, None, Config::new(path.clone())).await,
        Err(Error::MissingPrepared)
    ));
    let mut request = Request::molecule("export", document);
    request.format = Some("invalid".into());
    let request = Arc::new(request);
    let prepared = prepare(Arc::clone(&request)).await?;
    // Native analysis occurs before the export-format check. A real helper
    // process error must not be hidden by moving that check ahead of analysis.
    assert!(matches!(
        native_response::build(
            request,
            prepared,
            Config::new(path.with_file_name("missing-helper"))
        )
        .await,
        Err(Error::Helper(generator::Error::Io(_)))
    ));
    Ok(())
}

#[tokio::test]
async fn picture_drawings_preserve_the_complete_document_and_exports() -> anyhow::Result<()> {
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let reference = PythonEngine::default();
    let config = Config::new(path);
    let picture =
        reshiki::pictures::Picture::import(include_bytes!("fixtures/jpeg-subsampled-3x2.jpg"))
            .map_err(anyhow::Error::msg)?;
    let mut document = imported(&reference, "CO").await?;
    document
        .graphics
        .push(picture.graphic(document.next_id(), Point::new(123.25, -31.125)));
    for reflected in [false, true] {
        if reflected {
            document
                .graphics
                .first_mut()
                .context("Missing picture")?
                .axis_y
                .y *= -1.;
        }
        for (operation, format) in [
            ("analyze", None),
            ("finish_abbreviation", None),
            ("export", Some("smiles")),
            ("export", Some("mol")),
            ("export", Some("inchi")),
            ("export", Some("cdxml")),
            ("export", Some("cdx")),
        ] {
            let mut request = Request::molecule(operation, document.clone());
            request.format = format.map(str::to_owned);
            let mut expected = reference
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?;
            let request = Arc::new(request);
            let prepared = prepare(Arc::clone(&request)).await?;
            let mut actual = native_response::build(request, prepared, config.clone()).await?;
            if let Some(format @ ("cdxml" | "cdx")) = format {
                let normalize = |output: &str| -> anyhow::Result<String> {
                    use base64::{Engine, engine::general_purpose::STANDARD};
                    let xml = if format == "cdx" {
                        reshiki::exchange::from_cdx(&STANDARD.decode(output)?)
                            .map_err(anyhow::Error::msg)?
                    } else {
                        output.into()
                    };
                    normalized_picture_payloads(&xml)
                };
                actual.output = actual.output.as_deref().map(normalize).transpose()?;
                expected.output = expected.output.as_deref().map(normalize).transpose()?;
            }
            equal(actual, expected)
                .with_context(|| format!("picture/reflected={reflected}/{operation}/{format:?}"))?;
        }
    }
    Ok(())
}

fn normalized_picture_payloads(xml: &str) -> anyhow::Result<String> {
    let tree = roxmltree::Document::parse(xml)?;
    let mut result = xml.to_owned();
    for node in tree.descendants() {
        if let Some(hex) = node.attribute("PNG") {
            anyhow::ensure!(hex.len() % 2 == 0, "Invalid PNG hex");
            let bytes = hex
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let image = image::load_from_memory(&bytes)?.to_rgba8();
            // PNG filtering/compression is an established lossless difference
            // in the existing writer. Compare every channel exactly; preserve
            // every nonpayload XML byte and the complete surrounding response.
            let pixels = image
                .as_raw()
                .iter()
                .map(|value| format!("{value:02x}"))
                .collect::<String>();
            result = result.replace(
                &format!("PNG=\"{hex}\""),
                &format!("PNG=\"{}x{}:{pixels}\"", image.width(), image.height()),
            );
        }
    }
    Ok(result)
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn cancelling_a_running_response_kills_helper_and_preserves_snapshots() -> anyhow::Result<()>
{
    use std::process::{Command, Stdio};
    let Some(stub) = helper("inchi-helper-stub")? else {
        return Ok(());
    };
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let directory = tempfile::tempdir()?;
    let executable = directory.path().join("hang");
    std::fs::copy(stub, &executable)?;
    let reference = PythonEngine::default();
    let request = Arc::new(Request::molecule(
        "analyze",
        imported(&reference, "F[C@](Cl)(Br)I").await?,
    ));
    let prepared = prepare(Arc::clone(&request)).await?;
    let before = serde_json::to_value(&*request)?;
    let task = tokio::spawn(native_response::build(
        Arc::clone(&request),
        prepared.clone(),
        Config::new(executable),
    ));
    let pid = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(directory.path().join("pid"))
                && let Ok(pid) = text.parse::<u32>()
            {
                break pid;
            }
            // A single-thread executor remains available during response work.
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    anyhow::ensure!(pid > 0, "Invalid helper PID");
    task.abort();
    anyhow::ensure!(task.await.is_err(), "Cancelled response was published");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let live = Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !live.success() {
                return Ok::<_, std::io::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    assert_eq!(serde_json::to_value(&*request)?, before);
    equal(
        native_response::build(Arc::clone(&request), prepared, Config::new(path)).await?,
        reference
            .execute((*request).clone())
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    Ok(())
}
