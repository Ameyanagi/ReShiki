use anyhow::Context;
use reshiki::chemistry::depict::geometry::{self, Coordinates, Error, MAX_POINTS, Point};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, process::Command};

#[derive(Debug, Deserialize)]
struct Native {
    ids: Vec<usize>,
    values: Vec<String>,
}
#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    op: String,
    ids: Vec<usize>,
    values: Vec<String>,
    expected: Native,
}
type Output = (Vec<usize>, Vec<f64>);
fn floats(values: &[String]) -> anyhow::Result<Vec<f64>> {
    values
        .iter()
        .map(|v| Ok(f64::from_bits(u64::from_str_radix(v, 16)?)))
        .collect()
}
fn point(values: &[f64], offset: usize) -> anyhow::Result<Point> {
    Ok(Point {
        x: *values.get(offset).context("Missing x")?,
        y: *values.get(offset + 1).context("Missing y")?,
    })
}
fn coordinates(ids: &[usize], values: &[f64]) -> anyhow::Result<Coordinates> {
    anyhow::ensure!(values.len() == ids.len() * 2, "Coordinate count differs");
    ids.iter()
        .enumerate()
        .map(|(i, &id)| Ok((id, point(values, i * 2)?)))
        .collect()
}
fn flatten(points: Coordinates) -> Output {
    let mut ids = Vec::new();
    let mut values = Vec::new();
    for (id, p) in points {
        ids.push(id);
        values.extend([p.x, p.y]);
    }
    (ids, values)
}
fn run(case: &Case) -> anyhow::Result<Result<Output, Error>> {
    let v = floats(&case.values)?;
    Ok(match case.op.as_str() {
        "ring" => {
            geometry::embed_ring(&case.ids, *v.first().context("Missing bond length")?).map(flatten)
        }
        "canonical" | "box" => {
            let points = coordinates(&case.ids, &v)?;
            let before = points.clone();
            let result = if case.op == "canonical" {
                geometry::canonical_orientation(&points).map(flatten)
            } else {
                geometry::compute_box(&points).map(|b| {
                    (
                        vec![],
                        vec![b.positive_x, b.negative_x, b.positive_y, b.negative_y],
                    )
                })
            };
            anyhow::ensure!(points == before, "Input mutated");
            result
        }
        "bisect" => geometry::bisect_point(
            point(&v, 0)?,
            *v.get(2).context("Missing angle")?,
            point(&v, 3)?,
            point(&v, 5)?,
        )
        .map(|p| (vec![], vec![p.x, p.y])),
        "reflect" => geometry::reflect_point(point(&v, 0)?, point(&v, 2)?, point(&v, 4)?)
            .map(|p| (vec![], vec![p.x, p.y])),
        _ => anyhow::bail!("Unknown operation"),
    })
}
#[derive(Default, Debug)]
struct Differences {
    cases: usize,
    values: usize,
    differing: usize,
    projected: usize,
    max_relative: f64,
    max_scaled_error: f64,
    max_absolute: f64,
    max_projected: f64,
    native_differing: usize,
    native_projected: usize,
    max_native_projected: f64,
    worst: String,
}

#[test]
fn direct_native_geometry() -> anyhow::Result<()> {
    compare("depict-geometry-linux-native.json.gz", true)?;
    compare("depict-geometry-native.json.gz", false)?;
    compare("depict-geometry-windows-native.json.gz", false)
}

fn compare(fixture: &str, source_order: bool) -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = if cfg!(windows) {
        root.join(".venv/Scripts/python.exe")
    } else {
        root.join(".venv/bin/python")
    };
    let mut command = Command::new(python);
    command.arg(root.join("tests/depict_geometry_reference.py"));
    command
        .arg("--fixture")
        .arg(root.join("tests/fixtures").join(fixture));
    let live = source_order && std::env::var_os("RESHIKI_DEPICT_ORACLE").is_some();
    if live {
        command
            .arg("--oracle")
            .arg(std::env::var_os("RESHIKI_DEPICT_ORACLE").context("Missing oracle")?);
        command.arg("--rdkit-source").arg(
            std::env::var_os("RESHIKI_RDKIT_SOURCE").context("Missing source for live replay")?,
        );
        command.arg("--replay");
    }
    let output = command.output()?;
    anyhow::ensure!(
        output.status.success(),
        "Native fixture reader failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)?;
    let mut lines = text.lines();
    let header: serde_json::Value =
        serde_json::from_str(lines.next().context("Missing provenance")?)?;
    anyhow::ensure!(
        header["provenance"]["commit"] == "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
        "Wrong native source"
    );
    let mut stats: BTreeMap<String, Differences> = BTreeMap::new();
    let mut numeric = 0;
    let mut count = 0;
    for line in lines {
        count += 1;
        let case: Case = serde_json::from_str(line)?;
        let expected = floats(&case.expected.values)?;
        let actual = run(&case)?;
        anyhow::ensure!(
            actual == run(&case)?,
            "Repeated output differs: {}",
            case.name
        );
        if expected.iter().any(|v| !v.is_finite()) {
            anyhow::ensure!(
                actual == Err(Error::Numeric),
                "Native nonfinite not classified: {case:?} {actual:?}"
            );
            numeric += 1;
            continue;
        }
        let (ids, values) = actual.with_context(|| case.name.clone())?;
        anyhow::ensure!(
            ids == case.expected.ids,
            "Native atom order differs: {}",
            case.name
        );
        anyhow::ensure!(values.len() == expected.len(), "Native value count differs");
        let stat = stats.entry(case.op.clone()).or_default();
        stat.cases += 1;
        for (index, (a, b)) in values.into_iter().zip(expected).enumerate() {
            // Native replay and the fixture for this ABI require exact bits.
            // Cross-platform results remain an explicit descriptive audit.
            let exact = live
                || (source_order && cfg!(all(target_os = "linux", target_arch = "x86_64")))
                || (fixture == "depict-geometry-native.json.gz"
                    && cfg!(all(target_os = "macos", target_arch = "aarch64")))
                || (fixture == "depict-geometry-windows-native.json.gz" && cfg!(windows));
            if exact {
                anyhow::ensure!(
                    a.to_bits() == b.to_bits(),
                    "Exact native difference: {} {} value {index}: {a:.17e} vs {b:.17e}",
                    case.op,
                    case.name
                );
            }
            stat.values += 1;
            if a.to_bits() != b.to_bits() {
                stat.differing += 1;
                if case.name.starts_with("native/") {
                    stat.native_differing += 1;
                }
            }
            let error = (a - b).abs();
            let magnitude = a.abs().max(b.abs());
            if magnitude > 0.0 {
                stat.max_relative = stat.max_relative.max(error / magnitude);
            }
            let scaled = error / magnitude.max(1.0);
            if scaled > stat.max_scaled_error {
                stat.max_scaled_error = scaled;
                stat.worst = format!("{} value {index}: {a:.17e} vs {b:.17e}", case.name);
            }
            stat.max_absolute = stat.max_absolute.max(error);
            // Independent drawing projection: worker x*28, -y*28, then f32.
            let sign = if index % 2 == 0 { 1.0 } else { -1.0 };
            let projected_a = (sign * a * 28.0) as f32;
            let projected_b = (sign * b * 28.0) as f32;
            let projected_error = (f64::from(projected_a) - f64::from(projected_b)).abs();
            stat.max_projected = stat.max_projected.max(projected_error);
            if case.name.starts_with("native/") {
                stat.max_native_projected = stat.max_native_projected.max(projected_error);
            }
            if projected_a.to_bits() != projected_b.to_bits() {
                stat.projected += 1;
                if case.name.starts_with("native/") {
                    stat.native_projected += 1;
                }
            }
        }
    }
    for (op, stat) in &stats {
        eprintln!("{fixture}, live={live}, {op}: {stat:?}");
    }
    anyhow::ensure!(count == 5310 && numeric == 24, "Unexpected corpus coverage");
    eprintln!("native nonfinite cases: {numeric}");
    Ok(())
}

#[test]
fn checked_geometry_limits_and_invalid_numbers() {
    let zero = Point::default();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(geometry::embed_ring(&[0, 1], bad), Err(Error::NonFinite));
        assert_eq!(
            geometry::bisect_point(zero, bad, zero, zero),
            Err(Error::NonFinite)
        );
        let point = Point { x: bad, y: 0.0 };
        assert_eq!(
            geometry::reflect_point(point, zero, zero),
            Err(Error::NonFinite)
        );
        let points = Coordinates::from([(0, point)]);
        assert_eq!(
            geometry::canonical_orientation(&points),
            Err(Error::NonFinite)
        );
        assert_eq!(geometry::compute_box(&points), Err(Error::NonFinite));
    }
    assert_eq!(
        geometry::embed_ring(&vec![0; MAX_POINTS + 1], 1.5),
        Err(Error::Limit)
    );
    assert_eq!(geometry::embed_ring(&[usize::MAX], 1.5), Err(Error::Limit));
    let invalid = Coordinates::from([(usize::MAX, zero)]);
    assert_eq!(geometry::canonical_orientation(&invalid), Err(Error::Limit));
    let large: Coordinates = (0..=MAX_POINTS).map(|i| (i, zero)).collect();
    assert_eq!(geometry::compute_box(&large), Err(Error::Limit));
    let huge = Point {
        x: f64::MAX,
        y: f64::MAX,
    };
    assert_eq!(
        geometry::bisect_point(zero, 0.0, huge, huge),
        Err(Error::Numeric)
    );
    let overflow = Coordinates::from([(0, huge), (1, huge)]);
    assert_eq!(
        geometry::canonical_orientation(&overflow),
        Err(Error::Numeric)
    );
    assert_eq!(overflow.get(&0), Some(&huge));
}
