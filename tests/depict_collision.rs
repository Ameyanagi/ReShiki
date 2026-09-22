use anyhow::Context;
use reshiki::chemistry::{
    depict::{
        collision::{Error, Input, Stage},
        geometry::{Bounds, Point},
        rings::{EmbeddedAtom, Fragment},
    },
    stereo::perception::State,
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, process::Command};
#[derive(Debug, Deserialize)]
struct NativeAtom {
    ints: Vec<i64>,
    values: Vec<String>,
    neighbors: Vec<usize>,
}
#[derive(Debug, Deserialize)]
struct NativeFragment {
    done: bool,
    bounds: Vec<String>,
    atoms: Vec<NativeAtom>,
    attachment_points: Vec<usize>,
}
#[derive(Deserialize)]
struct Found {
    fragment: NativeFragment,
    density: String,
    pairs: Vec<(usize, usize)>,
}
#[derive(Deserialize)]
struct Repaired {
    fragment: NativeFragment,
    error: bool,
}
#[derive(Deserialize)]
struct Expected {
    initial: NativeFragment,
    find_atoms: Found,
    find_bonds: Found,
    flip: Repaired,
    open: Repaired,
    shorten: Repaired,
    combined: Repaired,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    state: State,
    fragment: NativeFragment,
    expected: Expected,
}
fn decode(value: &str) -> anyhow::Result<f64> {
    Ok(f64::from_bits(u64::from_str_radix(value, 16)?))
}
fn same(actual: &Fragment, expected: &NativeFragment, name: &str) -> anyhow::Result<usize> {
    let expected = restore(expected)?;
    anyhow::ensure!(
        actual.done == expected.done && actual.attachment_points == expected.attachment_points,
        "Fragment metadata {name}"
    );
    anyhow::ensure!(
        actual.atoms.len() == expected.atoms.len(),
        "Atom count {name}"
    );
    for (i, (a, b)) in [
        actual.bounds.positive_x,
        actual.bounds.negative_x,
        actual.bounds.positive_y,
        actual.bounds.negative_y,
    ]
    .into_iter()
    .zip([
        expected.bounds.positive_x,
        expected.bounds.negative_x,
        expected.bounds.positive_y,
        expected.bounds.negative_y,
    ])
    .enumerate()
    {
        anyhow::ensure!(a.to_bits() == b.to_bits(), "Bounds {name}/{i}");
    }
    for ((ak, a), (bk, b)) in actual.atoms.iter().zip(&expected.atoms) {
        anyhow::ensure!(
            ak == bk
                && a.id == b.id
                && a.neighbor1 == b.neighbor1
                && a.neighbor2 == b.neighbor2
                && a.cis_trans_neighbor == b.cis_trans_neighbor
                && a.counter_clockwise == b.counter_clockwise
                && a.rotation_direction == b.rotation_direction
                && a.fixed == b.fixed
                && a.neighbors == b.neighbors,
            "Atom metadata {name}/{ak}"
        );
        for (i, (a, b)) in [
            a.location.x,
            a.location.y,
            a.normal.x,
            a.normal.y,
            a.angle,
            a.density,
        ]
        .into_iter()
        .zip([
            b.location.x,
            b.location.y,
            b.normal.x,
            b.normal.y,
            b.angle,
            b.density,
        ])
        .enumerate()
        {
            anyhow::ensure!(
                a.to_bits() == b.to_bits(),
                "Scalar {name}/{ak}/{i}: {a:.17e} vs {b:.17e}"
            );
        }
    }
    Ok(4 + 6 * actual.atoms.len())
}
fn restore(native: &NativeFragment) -> anyhow::Result<Fragment> {
    let mut atoms = BTreeMap::new();
    for native in &native.atoms {
        let ints: [i64; 8] = native
            .ints
            .clone()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Native integer count"))?;
        let values = native
            .values
            .iter()
            .map(|v| decode(v))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let [x, y, nx, ny, angle, density]: [f64; 6] = values
            .try_into()
            .map_err(|_| anyhow::anyhow!("Native scalar count"))?;
        let optional = |value| {
            if value < 0 {
                Ok(None)
            } else {
                usize::try_from(value).map(Some)
            }
        };
        atoms.insert(
            usize::try_from(ints[0])?,
            EmbeddedAtom {
                id: usize::try_from(ints[1])?,
                location: Point { x, y },
                normal: Point { x: nx, y: ny },
                angle,
                neighbor1: optional(ints[2])?,
                neighbor2: optional(ints[3])?,
                cis_trans_neighbor: optional(ints[4])?,
                counter_clockwise: ints[5] != 0,
                rotation_direction: i32::try_from(ints[6])?,
                fixed: ints[7] != 0,
                neighbors: native.neighbors.clone(),
                density,
            },
        );
    }
    let values = native
        .bounds
        .iter()
        .map(|v| decode(v))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let [positive_x, negative_x, positive_y, negative_y]: [f64; 4] = values
        .try_into()
        .map_err(|_| anyhow::anyhow!("Native bound count"))?;
    Ok(Fragment {
        atoms,
        done: native.done,
        bounds: Bounds {
            positive_x,
            negative_x,
            positive_y,
            negative_y,
        },
        attachment_points: native.attachment_points.clone(),
    })
}

fn corpus() -> anyhow::Result<Vec<Case>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }));
    command
        .arg(root.join("tests/depict_collision_reference.py"))
        .arg("--fixture")
        .arg(
            root.join("tests/fixtures")
                .join(if cfg!(target_os = "macos") {
                    "depict-collision-macos-native.json.gz"
                } else if cfg!(windows) {
                    "depict-collision-windows-native.json.gz"
                } else {
                    "depict-collision-linux-native.json.gz"
                }),
        );
    if let Some(oracle) = std::env::var_os("RESHIKI_DEPICT_COLLISION_ORACLE") {
        command
            .arg("--oracle")
            .arg(oracle)
            .arg("--replay")
            .arg("--rdkit-source")
            .arg(std::env::var_os("RESHIKI_RDKIT_SOURCE").context("Missing native source")?);
    }
    let output = command.output()?;
    anyhow::ensure!(
        output.status.success(),
        "Fixture loader: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)?;
    let mut lines = text.lines();
    let provenance: serde_json::Value =
        serde_json::from_str(lines.next().context("Missing provenance")?)?;
    anyhow::ensure!(
        provenance["provenance"]["commit"] == "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
        "Wrong pinned source"
    );
    lines.map(|line| Ok(serde_json::from_str(line)?)).collect()
}
#[test]
fn independent_native_collision_stages() -> anyhow::Result<()> {
    let cases = corpus()?;
    let (mut scalars, mut errors, mut changed, mut pairs) = (0, 0, 0, 0);
    for case in &cases {
        let original = restore(&case.fragment)?;
        let before = serde_json::to_value((&case.state, &original))?;
        let input = Input::new(&case.state.graph, &case.state.metadata, &case.state.rings)?;
        scalars += same(
            &original,
            &case.expected.initial,
            &format!("{}/initial", case.name),
        )?;
        for (include, expected) in [
            (false, &case.expected.find_atoms),
            (true, &case.expected.find_bonds),
        ] {
            let result = input.find(&original, include)?;
            anyhow::ensure!(
                result.pairs == expected.pairs,
                "Pairs {} / {include}: {:?} vs {:?}",
                case.name,
                result.pairs,
                expected.pairs
            );
            pairs += result.pairs.len();
            anyhow::ensure!(
                result.total_density.to_bits() == decode(&expected.density)?.to_bits(),
                "Total density {} / {include}",
                case.name
            );
            scalars += same(
                &result.fragment,
                &expected.fragment,
                &format!("{}/find/{include}", case.name),
            )?;
        }
        for (stage, expected) in [
            (Some(Stage::BondAndSpiroFlip), &case.expected.flip),
            (Some(Stage::OpenAngles), &case.expected.open),
            (Some(Stage::ShortenBonds), &case.expected.shorten),
            (None, &case.expected.combined),
        ] {
            let result = stage.map_or_else(
                || input.repair(&original),
                |stage| input.apply(&original, stage),
            );
            if expected.error {
                errors += 1;
                anyhow::ensure!(result.is_err(), "Native error {} / {stage:?}", case.name);
            } else {
                let value = result.with_context(|| format!("{} / {stage:?}", case.name))?;
                changed += usize::from(value != original);
                scalars += same(
                    &value,
                    &expected.fragment,
                    &format!("{}/{stage:?}", case.name),
                )?;
            }
        }
        anyhow::ensure!(
            before == serde_json::to_value((&case.state, &original))?,
            "Input mutated {}",
            case.name
        );
    }
    eprintln!(
        "{} cases; {scalars} exact scalars; {pairs} ordered pairs; {errors} native errors; {changed} changed states",
        cases.len()
    );
    anyhow::ensure!(
        cases.len() == 1634 && pairs > 1000 && errors > 0 && changed > 100,
        "Insufficient coverage"
    );
    Ok(())
}
#[test]
fn bounded_atomic_failure_and_invalid_state() -> anyhow::Result<()> {
    let cases = corpus()?;
    let case = cases
        .iter()
        .find(|c| c.name.contains("CCCCCC/") && c.name.ends_with("/compressed/none"))
        .context("Missing chain")?;
    let original = restore(&case.fragment)?;
    let state = &case.state;
    let input = Input::new(&state.graph, &state.metadata, &state.rings)?.with_work_limit(1);
    for _ in 0..2 {
        anyhow::ensure!(input.find(&original, true) == Err(Error::Limit));
        anyhow::ensure!(input.repair(&original) == Err(Error::Limit));
        for stage in [
            Stage::BondAndSpiroFlip,
            Stage::OpenAngles,
            Stage::ShortenBonds,
        ] {
            anyhow::ensure!(input.apply(&original, stage) == Err(Error::Limit));
        }
    }
    // This budget passes validation and fails after detached density updates.
    let late = Input::new(&state.graph, &state.metadata, &state.rings)?.with_work_limit(50);
    anyhow::ensure!(late.repair(&original) == Err(Error::Limit));
    same(&original, &case.fragment, "late budget failure")?;
    let input = Input::new(&state.graph, &state.metadata, &state.rings)?;
    let mut malformed = original.clone();
    malformed.atoms.remove(&0);
    anyhow::ensure!(input.repair(&malformed).is_err());
    let mut malformed = original.clone();
    malformed
        .atoms
        .get_mut(&0)
        .context("Missing atom")?
        .location
        .x = f64::NAN;
    anyhow::ensure!(input.find(&malformed, true).is_err());
    let mut overflow = original.clone();
    overflow
        .atoms
        .get_mut(&0)
        .context("Missing atom")?
        .location
        .x = f64::MAX;
    anyhow::ensure!(input.find(&overflow, true).is_err());
    let mut rings = state.rings.clone();
    rings.atoms.push(vec![0, 1, 0]);
    anyhow::ensure!(Input::new(&state.graph, &state.metadata, &rings).is_err());
    same(&original, &case.fragment, "atomic original")?;
    Ok(())
}
