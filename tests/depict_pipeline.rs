//! Exact complete-layout comparisons against the original public native API.
use anyhow::Context;
use reshiki::chemistry::{
    depict::{self, attachment::AtomData, geometry::Coordinates},
    stereo::{perception::State, wedging::Conformer},
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct LayoutOptions {
    canon_orient: bool,
    use_ring_templates: bool,
    bond_length: Option<String>,
    coordinates: Option<Vec<(usize, String, String)>>,
    clear_confs: bool,
    force_rdkit: bool,
    n_samples: usize,
    n_flips_per_sample: usize,
}
#[derive(Deserialize)]
struct NativeConformer {
    id: u32,
    is_3d: bool,
    positions: Vec<[String; 3]>,
}
#[derive(Deserialize)]
struct Expected {
    success: bool,
    finite: bool,
    rejection_kind: Option<String>,
    conformer_id: Option<u32>,
    conformers: Vec<NativeConformer>,
    state: Value,
    properties: Value,
    atom_data: Value,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    state: State,
    atom_data: Vec<AtomData>,
    options: LayoutOptions,
    before_properties: Value,
    expected: Expected,
}
fn decode(text: &str) -> anyhow::Result<f64> {
    anyhow::ensure!(text.len() == 16, "Invalid f64 bit string");
    Ok(f64::from_bits(u64::from_str_radix(text, 16)?))
}
fn coordinates(options: &LayoutOptions) -> anyhow::Result<Option<Coordinates>> {
    options
        .coordinates
        .as_ref()
        .map(|values| {
            values
                .iter()
                .map(|(id, x, y)| {
                    Ok((
                        *id,
                        depict::geometry::Point {
                            x: decode(x)?,
                            y: decode(y)?,
                        },
                    ))
                })
                .collect()
        })
        .transpose()
}
fn equal_conformer(actual: &Conformer, expected: &NativeConformer) -> anyhow::Result<usize> {
    anyhow::ensure!(
        expected.id == 0 && actual.is_3d == expected.is_3d,
        "Conformer metadata"
    );
    anyhow::ensure!(
        actual.positions.len() == expected.positions.len(),
        "Atom count"
    );
    let mut count = 0;
    for (id, (actual, expected)) in actual.positions.iter().zip(&expected.positions).enumerate() {
        for (axis, (actual, expected)) in [actual.x, actual.y, actual.z]
            .into_iter()
            .zip(expected)
            .enumerate()
        {
            let expected = decode(expected)?;
            anyhow::ensure!(
                actual.is_finite() && expected.is_finite(),
                "Nonfinite atom {id}/{axis}"
            );
            anyhow::ensure!(
                actual.to_bits() == expected.to_bits(),
                "atom {id}/{axis}: {actual:.17e} ({:016x}) != {expected:.17e} ({:016x})",
                actual.to_bits(),
                expected.to_bits()
            );
            count += 1;
        }
    }
    Ok(count)
}
fn compare(case: &Case) -> anyhow::Result<(bool, usize)> {
    let options = &case.options;
    anyhow::ensure!(
        options.clear_confs && options.force_rdkit,
        "Unsupported native options"
    );
    anyhow::ensure!(
        options.n_samples == 0 && options.n_flips_per_sample == 0,
        "Sampling changed"
    );
    let coordinates = coordinates(options)?;
    let ranks = case
        .atom_data
        .iter()
        .map(|a| a.chiral_rank)
        .collect::<Vec<_>>();
    let before = serde_json::to_value(&case.state)?;
    let native_defaults =
        options.bond_length.is_none() && options.canon_orient && !options.use_ring_templates;
    let solver_options = if native_defaults {
        depict::Options::default()
    } else {
        depict::Options {
            bond_length: options
                .bond_length
                .as_deref()
                .map(decode)
                .transpose()?
                .unwrap_or(1.5),
            canonical_orientation: options.canon_orient,
            use_ring_templates: options.use_ring_templates,
            ..Default::default()
        }
    };
    let actual = depict::compute(&case.state, &ranks, coordinates.as_ref(), solver_options);
    anyhow::ensure!(
        serde_json::to_value(&case.state)? == before,
        "Input state mutated"
    );
    anyhow::ensure!(
        before == case.expected.state,
        "Original chemical state changed"
    );
    anyhow::ensure!(
        case.before_properties == case.expected.properties,
        "Original properties changed"
    );
    anyhow::ensure!(
        serde_json::to_value(&case.atom_data)? == case.expected.atom_data,
        "Original atom data changed"
    );
    match (actual, case.expected.success && case.expected.finite) {
        (Ok(actual), true) => {
            anyhow::ensure!(
                case.expected.conformer_id == Some(0) && case.expected.conformers.len() == 1,
                "Expected one replacing conformer"
            );
            let expected = case
                .expected
                .conformers
                .first()
                .context("Missing conformer")?;
            Ok((true, equal_conformer(&actual, expected)?))
        }
        (Err(error), false) => {
            let matching = matches!(
                (case.expected.rejection_kind.as_deref(), &error),
                (
                    Some("zero_length_vector"),
                    depict::Error::Initial(depict::expansion::Error::Seeds(
                        depict::seeds::Error::Geometry(depict::geometry::Error::Numeric)
                    ))
                ) | (
                    Some("missing_fragment_neighbor"),
                    depict::Error::Initial(depict::expansion::Error::Invalid(
                        "missing single-anchor merge neighbors"
                    ))
                )
            );
            anyhow::ensure!(
                matching,
                "Native {:?} became a different failure: {error:?}",
                case.expected.rejection_kind
            );
            Ok((false, 0))
        }
        (Err(error), true) => anyhow::bail!("Rust rejected native layout: {error}"),
        (Ok(_), false) => anyhow::bail!("Rust accepted a native layout failure"),
    }
}

#[test]
fn complete_layout_matches_public_native_api() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut command = Command::new(python);
    command.arg(root.join("tests/depict_pipeline_reference.py"));
    if let Some(fixture) = std::env::var_os("RESHIKI_DEPICT_PIPELINE_REFERENCE") {
        command.arg("--fixture").arg(fixture);
    } else {
        command.arg("--live");
    }
    let mut process = command
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .spawn()?;
    let output = process.stdout.take().context("No reference output")?;
    let mut lines = BufReader::new(output).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing provenance")??)?;
    let provenance = &header["provenance"];
    anyhow::ensure!(
        provenance["version"] == reshiki::chemistry::RDKIT_VERSION,
        "Native version"
    );
    anyhow::ensure!(
        provenance["commit"] == "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
        "Native source revision changed"
    );
    let observer = std::fs::read(root.join("tests/depict_pipeline_reference.py"))?;
    anyhow::ensure!(
        provenance["observer_sha256"] == format!("{:x}", Sha256::digest(observer)),
        "Reference observer changed; recapture expected results"
    );
    let inputs = std::fs::read(root.join("tests/fixtures/depict-pipeline-inputs.json.gz"))?;
    anyhow::ensure!(
        provenance["input_sha256"] == format!("{:x}", Sha256::digest(inputs)),
        "Input fixture differs"
    );
    let platform = provenance["platform"]
        .as_str()
        .context("Missing native platform")?;
    anyhow::ensure!(
        platform.starts_with(if cfg!(windows) {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        }),
        "Exact replay requires a capture from this platform: {platform}"
    );
    let machine = provenance["machine"]
        .as_str()
        .context("Missing reference machine")?;
    let matching_machine = if cfg!(target_arch = "aarch64") {
        matches!(machine.to_ascii_lowercase().as_str(), "arm64" | "aarch64")
    } else {
        matches!(machine.to_ascii_lowercase().as_str(), "amd64" | "x86_64")
    };
    // The pinned Windows reference wheel is x64, executed under emulation on
    // ARM CI. Report that boundary explicitly; do not call it native ARM parity.
    let windows_reference = cfg!(all(windows, target_arch = "aarch64"))
        && matches!(machine.to_ascii_lowercase().as_str(), "amd64" | "x86_64");
    anyhow::ensure!(
        matching_machine || windows_reference,
        "Reference architecture differs: {machine}"
    );
    if cfg!(all(windows, target_arch = "aarch64")) {
        eprintln!("Comparing Windows ARM Rust with the pinned x64 reference wheel");
    }
    let (mut successes, mut rejections, mut scalars) = (0, 0, 0);
    let mut failures = Vec::new();
    let mut coordination = Vec::new();
    let mut cases = 0;
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        cases += 1;
        match compare(&case) {
            Ok((true, count)) => {
                successes += 1;
                scalars += count;
            }
            Ok((false, _)) => rejections += 1,
            Err(error) => failures.push(format!("{}: {error:#}", case.name)),
        }
        if case
            .state
            .metadata
            .atoms
            .iter()
            .any(|a| matches!(a.chiral_tag, 6..=8))
        {
            coordination.push(case);
        }
    }
    anyhow::ensure!(process.wait()?.success(), "Native reference failed");
    // Each reference result used a fresh native seed cache. Reverse requests
    // with different lengths as well: earlier styles must never leak forward.
    anyhow::ensure!(!coordination.is_empty(), "Missing coordination seed cases");
    for case in coordination.iter().rev() {
        if let Err(error) = compare(case) {
            failures.push(format!("reverse/{}: {error:#}", case.name));
        }
    }
    eprintln!(
        "Public pipeline: {cases} cases, {successes} successes, {rejections} rejections, {scalars} exact coordinate scalars; {} failures",
        failures.len()
    );
    for failure in failures.iter().take(40) {
        eprintln!("{failure}");
    }
    anyhow::ensure!(cases == 2854, "Incomplete public reference corpus");
    anyhow::ensure!(
        failures.is_empty(),
        "{} complete-layout differences",
        failures.len()
    );
    Ok(())
}

#[test]
fn invalid_requests_are_atomic_and_do_not_change_the_next_layout() -> anyhow::Result<()> {
    let molecule = reshiki::chemistry::smiles::prepare("CCCOC1CCCCC1")?;
    let state = molecule.state;
    let ranks = vec![None; state.graph.atoms.len()];
    let before = serde_json::to_value(&state)?;
    let baseline = depict::compute(&state, &ranks, None, Default::default())?;
    for bond_length in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            depict::compute(
                &state,
                &ranks,
                None,
                depict::Options {
                    bond_length,
                    ..Default::default()
                }
            ),
            Err(depict::Error::BondLength)
        ));
    }
    for work_limit in [0, 1, 10] {
        assert!(
            depict::compute(
                &state,
                &ranks,
                None,
                depict::Options {
                    work_limit,
                    bond_length: 2.75,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    let invalid = [(
        state.graph.atoms.len(),
        depict::geometry::Point { x: 1.0, y: 2.0 },
    )]
    .into_iter()
    .collect();
    assert!(depict::compute(&state, &ranks, Some(&invalid), Default::default()).is_err());
    assert!(depict::compute(&state, &[], None, Default::default()).is_err());
    assert_eq!(serde_json::to_value(&state)?, before);
    let after = depict::compute(&state, &ranks, None, Default::default())?;
    assert_eq!(baseline.is_3d, after.is_3d);
    assert_eq!(baseline.positions.len(), after.positions.len());
    for (left, right) in baseline.positions.iter().zip(&after.positions) {
        assert_eq!(
            [left.x.to_bits(), left.y.to_bits(), left.z.to_bits()],
            [right.x.to_bits(), right.y.to_bits(), right.z.to_bits()]
        );
    }
    Ok(())
}
