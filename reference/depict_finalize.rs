#[path = "common/depict_windows.rs"]
mod depict_windows;

use anyhow::Context;
use reshiki::chemistry::{
    depict::{
        finalize::{self, Error, Options},
        geometry::{self, Bounds, Coordinates, Point},
        rings::{EmbeddedAtom, Fragment},
    },
    stereo::Point3,
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, process::Command};

#[derive(Debug, Deserialize)]
struct NativeAtom {
    ints: [i64; 8],
    values: [String; 6],
    neighbors: Vec<usize>,
}
#[derive(Debug, Deserialize)]
struct NativeFragment {
    done: bool,
    bounds: [String; 4],
    atoms: Vec<NativeAtom>,
    attachment_points: Vec<usize>,
}
#[derive(Deserialize)]
struct Expected {
    initial: Vec<NativeFragment>,
    packed: Vec<NativeFragment>,
    conformer_id: u32,
    conformer_count: usize,
    is_3d: bool,
    positions: Vec<[String; 3]>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    atom_count: usize,
    fragments: Vec<NativeFragment>,
    coordinates: Option<Vec<(usize, String, String)>>,
    canonical: bool,
    expected: Expected,
}
fn decode(value: &str) -> anyhow::Result<f64> {
    Ok(f64::from_bits(u64::from_str_radix(value, 16)?))
}
fn optional(id: i64) -> anyhow::Result<Option<usize>> {
    if id == -1 {
        Ok(None)
    } else {
        Ok(Some(usize::try_from(id)?))
    }
}
fn native_id(id: Option<usize>) -> anyhow::Result<i64> {
    id.map_or(Ok(-1), |id| Ok(i64::try_from(id)?))
}
fn restore(f: &NativeFragment) -> anyhow::Result<Fragment> {
    let mut atoms = BTreeMap::new();
    for a in &f.atoms {
        atoms.insert(
            usize::try_from(a.ints[0])?,
            EmbeddedAtom {
                id: usize::try_from(a.ints[1])?,
                location: Point {
                    x: decode(&a.values[0])?,
                    y: decode(&a.values[1])?,
                },
                normal: Point {
                    x: decode(&a.values[2])?,
                    y: decode(&a.values[3])?,
                },
                angle: decode(&a.values[4])?,
                density: decode(&a.values[5])?,
                neighbor1: optional(a.ints[2])?,
                neighbor2: optional(a.ints[3])?,
                cis_trans_neighbor: optional(a.ints[4])?,
                counter_clockwise: a.ints[5] != 0,
                rotation_direction: i32::try_from(a.ints[6])?,
                fixed: a.ints[7] != 0,
                neighbors: a.neighbors.clone(),
            },
        );
    }
    Ok(Fragment {
        atoms,
        done: f.done,
        bounds: Bounds {
            positive_x: decode(&f.bounds[0])?,
            negative_x: decode(&f.bounds[1])?,
            positive_y: decode(&f.bounds[2])?,
            negative_y: decode(&f.bounds[3])?,
        },
        attachment_points: f.attachment_points.clone(),
    })
}
#[derive(Default, Debug)]
struct Audit {
    cases: usize,
    fragments: usize,
    atoms: usize,
    scalars: usize,
    differing: usize,
    projected: usize,
    max_absolute: f64,
    max_relative: f64,
    max_projected: f64,
    worst: String,
}
impl Audit {
    fn value(
        &mut self,
        a: f64,
        encoded: &str,
        name: &str,
        exact: bool,
        projection: Option<f64>,
    ) -> anyhow::Result<()> {
        let b = decode(encoded)?;
        anyhow::ensure!(a.is_finite() && b.is_finite(), "Nonfinite {name}");
        self.scalars += 1;
        if a.to_bits() != b.to_bits() {
            self.differing += 1;
            anyhow::ensure!(!exact, "Native f64 difference {name}: {a:.17e} vs {b:.17e}");
        }
        let difference = (a - b).abs();
        if difference > self.max_absolute {
            self.max_absolute = difference;
            self.worst = format!("{name}: {a:.17e} vs {b:.17e}");
        }
        let magnitude = a.abs().max(b.abs());
        if magnitude > 0.0 {
            self.max_relative = self.max_relative.max(difference / magnitude);
        }
        if let Some(sign) = projection {
            let ap = (sign * a * 28.0) as f32;
            let bp = (sign * b * 28.0) as f32;
            if ap.to_bits() != bp.to_bits() {
                self.projected += 1;
            }
            self.max_projected = self
                .max_projected
                .max((f64::from(ap) - f64::from(bp)).abs());
            anyhow::ensure!(
                !exact || ap.to_bits() == bp.to_bits(),
                "Drawing projection {name}"
            );
        }
        Ok(())
    }
    fn fragment(
        &mut self,
        actual: &Fragment,
        expected: &NativeFragment,
        name: &str,
        exact: bool,
    ) -> anyhow::Result<()> {
        self.fragments += 1;
        anyhow::ensure!(
            actual.done == expected.done && actual.attachment_points == expected.attachment_points,
            "Fragment state {name}"
        );
        anyhow::ensure!(
            actual.atoms.len() == expected.atoms.len(),
            "Fragment atom count {name}"
        );
        for (a, b) in [
            actual.bounds.positive_x,
            actual.bounds.negative_x,
            actual.bounds.positive_y,
            actual.bounds.negative_y,
        ]
        .into_iter()
        .zip(&expected.bounds)
        {
            self.value(a, b, &format!("{name}/box"), exact, None)?;
        }
        for ((&id, a), b) in actual.atoms.iter().zip(&expected.atoms) {
            self.atoms += 1;
            let ints = [
                i64::try_from(id)?,
                i64::try_from(a.id)?,
                native_id(a.neighbor1)?,
                native_id(a.neighbor2)?,
                native_id(a.cis_trans_neighbor)?,
                i64::from(a.counter_clockwise),
                i64::from(a.rotation_direction),
                i64::from(a.fixed),
            ];
            anyhow::ensure!(
                ints == b.ints && a.neighbors == b.neighbors,
                "Atom metadata {name}/{id}"
            );
            for (i, (value, expected)) in [
                a.location.x,
                a.location.y,
                a.normal.x,
                a.normal.y,
                a.angle,
                a.density,
            ]
            .into_iter()
            .zip(&b.values)
            .enumerate()
            {
                self.value(
                    value,
                    expected,
                    &format!("{name}/atom {id}/{i}"),
                    exact,
                    match i {
                        0 => Some(1.0),
                        1 => Some(-1.0),
                        _ => None,
                    },
                )?;
            }
        }
        Ok(())
    }
}
fn nonfinite(expected: &Expected) -> anyhow::Result<bool> {
    for f in &expected.packed {
        for encoded in f
            .bounds
            .iter()
            .chain(f.atoms.iter().flat_map(|a| a.values.iter()))
        {
            if !decode(encoded)?.is_finite() {
                return Ok(true);
            }
        }
    }
    for p in &expected.positions {
        for encoded in p {
            if !decode(encoded)?.is_finite() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
#[test]
fn native_final_fragment_and_conformer_stages() -> anyhow::Result<()> {
    compare("depict-finalize-linux-native.json.gz", true)?;
    compare("depict-finalize-macos-native.json.gz", false)?;
    compare("depict-finalize-windows-native.json.gz", false)
}
fn compare(fixture: &str, baseline: bool) -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = match std::env::var_os("RESHIKI_REFERENCE_PYTHON") {
        Some(python) => python,
        None => {
            let local = root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            });
            if local.exists() {
                local.into_os_string()
            } else if cfg!(windows) {
                "python".into()
            } else {
                "python3".into()
            }
        }
    };
    let mut command = Command::new(python);
    command
        .arg(root.join("tests/depict_finalize_reference.py"))
        .arg("--fixture")
        .arg(root.join("tests/fixtures").join(fixture));
    let live = baseline && std::env::var_os("RESHIKI_DEPICT_FINALIZE_ORACLE").is_some();
    if live {
        command
            .arg("--oracle")
            .arg(std::env::var_os("RESHIKI_DEPICT_FINALIZE_ORACLE").context("Native helper")?)
            .arg("--rdkit-source")
            .arg(std::env::var_os("RESHIKI_RDKIT_SOURCE").context("Native source")?)
            .arg("--replay");
    }
    let output = command.output()?;
    anyhow::ensure!(
        output.status.success(),
        "Native reader: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data = String::from_utf8(output.stdout)?;
    let mut lines = data.lines();
    let header: serde_json::Value =
        serde_json::from_str(lines.next().context("Missing provenance")?)?;
    anyhow::ensure!(
        header["provenance"]["commit"] == "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
        "Wrong source"
    );
    let exact = live
        || (baseline && cfg!(all(target_os = "linux", target_arch = "x86_64")))
        || (fixture.ends_with("macos-native.json.gz")
            && cfg!(all(target_os = "macos", target_arch = "aarch64")))
        || depict_windows::fixture("finalize")?.as_deref() == Some(fixture);
    let mut audit = Audit::default();
    let mut numeric = 0;
    for line in lines {
        let case: Case = serde_json::from_str(line)?;
        audit.cases += 1;
        let fragments = case
            .fragments
            .iter()
            .map(restore)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let original = fragments.clone();
        let coordinates = case
            .coordinates
            .as_ref()
            .map(|c| {
                c.iter()
                    .map(|(id, x, y)| {
                        Ok((
                            *id,
                            Point {
                                x: decode(x)?,
                                y: decode(y)?,
                            },
                        ))
                    })
                    .collect::<anyhow::Result<Coordinates>>()
            })
            .transpose()?;
        let options = Options {
            canonical_orientation: case.canonical,
            ..Options::default()
        };
        let apply = || finalize::finish(case.atom_count, &fragments, coordinates.as_ref(), options);
        let result = apply();
        if nonfinite(&case.expected)? {
            numeric += 1;
            anyhow::ensure!(
                matches!(result, Err(Error::Geometry(geometry::Error::Numeric))),
                "Native nonfinite {}: {result:?}",
                case.name
            );
        } else {
            let result = result.with_context(|| case.name.clone())?;
            let repeated = apply()?;
            anyhow::ensure!(
                result.fragments == repeated.fragments,
                "Repeated fragment {}",
                case.name
            );
            anyhow::ensure!(
                result.conformer_id == case.expected.conformer_id
                    && case.expected.conformer_count == 1
                    && result.conformer.is_3d == case.expected.is_3d
                    && !result.conformer.is_3d,
                "Conformer metadata {}",
                case.name
            );
            anyhow::ensure!(
                result.fragments.len() == case.expected.packed.len(),
                "Packed fragment count"
            );
            for (i, (a, b)) in result
                .fragments
                .iter()
                .zip(&case.expected.packed)
                .enumerate()
            {
                audit.fragment(a, b, &format!("{}/fragment {i}", case.name), exact)?;
            }
            anyhow::ensure!(
                result.conformer.positions.len() == case.expected.positions.len(),
                "Conformer dimensions"
            );
            for (i, ((p, b), r)) in result
                .conformer
                .positions
                .iter()
                .zip(&case.expected.positions)
                .zip(&repeated.conformer.positions)
                .enumerate()
            {
                for (j, ((a, b), r)) in [p.x, p.y, p.z]
                    .into_iter()
                    .zip(b)
                    .zip([r.x, r.y, r.z])
                    .enumerate()
                {
                    anyhow::ensure!(a.to_bits() == r.to_bits(), "Repeated conformer");
                    audit.value(
                        a,
                        b,
                        &format!("{}/position {i}/{j}", case.name),
                        exact,
                        match j {
                            0 => Some(1.0),
                            1 => Some(-1.0),
                            _ => None,
                        },
                    )?;
                }
            }
        }
        anyhow::ensure!(fragments == original, "Caller changed {}", case.name);
        // Observe constructor/input transfer independently of all algorithms.
        for (a, b) in fragments.iter().zip(&case.expected.initial) {
            Audit::default().fragment(a, b, &case.name, true)?;
        }
    }
    anyhow::ensure!(
        audit.cases == 2312 && numeric == 4,
        "Corpus coverage: {}, {numeric}",
        audit.cases
    );
    eprintln!("{fixture}, live={live}, exact={exact}, numeric={numeric}: {audit:?}");
    Ok(())
}
fn empty() -> Fragment {
    Fragment {
        atoms: BTreeMap::new(),
        done: false,
        bounds: Bounds {
            positive_x: 0.0,
            negative_x: 0.0,
            positive_y: 0.0,
            negative_y: 0.0,
        },
        attachment_points: Vec::new(),
    }
}
fn one() -> Fragment {
    let mut f = empty();
    f.atoms.insert(
        0,
        EmbeddedAtom {
            id: 0,
            location: Point { x: 2.0, y: -3.0 },
            normal: Point { x: 0.6, y: 0.8 },
            angle: -1.0,
            neighbor1: None,
            neighbor2: None,
            cis_trans_neighbor: None,
            counter_clockwise: true,
            rotation_direction: 0,
            neighbors: Vec::new(),
            density: -1.0,
            fixed: true,
        },
    );
    f
}
#[test]
fn checked_indices_storage_and_work_preserve_inputs() -> anyhow::Result<()> {
    let f = one();
    let before = f.clone();
    for count in [geometry::MAX_POINTS + 1, usize::MAX] {
        assert!(matches!(
            finalize::finish(count, &[], None, Options::default()),
            Err(Error::Limit)
        ));
    }
    assert!(matches!(
        finalize::finish(0, std::slice::from_ref(&f), None, Options::default()),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        finalize::finish(
            1,
            &vec![empty(); finalize::MAX_FRAGMENTS + 1],
            None,
            Options::default()
        ),
        Err(Error::Limit)
    ));
    let invalid = Coordinates::from([(1, Point::default())]);
    assert!(matches!(
        finalize::finish(
            1,
            std::slice::from_ref(&f),
            Some(&invalid),
            Options::default()
        ),
        Err(Error::Invalid(_))
    ));
    for limit in [0, 12, 20, 23] {
        assert!(matches!(
            finalize::finish(
                1,
                std::slice::from_ref(&f),
                None,
                Options {
                    work_limit: limit,
                    ..Options::default()
                }
            ),
            Err(Error::Limit)
        ));
    }
    let mut excessive = f.clone();
    excessive.attachment_points = vec![0; finalize::MAX_METADATA_ENTRIES + 1];
    assert!(matches!(
        finalize::finish(1, &[excessive], None, Options::default()),
        Err(Error::Limit)
    ));
    let mut bad = f.clone();
    bad.atoms.get_mut(&0).context("Atom")?.neighbor1 = Some(1);
    assert!(matches!(
        finalize::finish(1, &[bad], None, Options::default()),
        Err(Error::Invalid(_))
    ));
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut bad = f.clone();
        bad.atoms.get_mut(&0).context("Atom")?.normal.x = invalid;
        assert!(matches!(
            finalize::finish(1, &[bad], None, Options::default()),
            Err(Error::Geometry(geometry::Error::NonFinite))
        ));
    }
    assert_eq!(f, before);
    Ok(())
}
#[test]
fn single_anchor_only_translates_conformer_and_preserves_dimensions() -> anyhow::Result<()> {
    let f = one();
    let coordinates = Coordinates::from([(1, Point { x: 7.0, y: 9.0 })]);
    let out = finalize::finish(
        3,
        std::slice::from_ref(&f),
        Some(&coordinates),
        Options::default(),
    )?;
    assert_eq!(
        out.fragments
            .first()
            .context("Fragment")?
            .atoms
            .get(&0)
            .context("Atom")?
            .location,
        f.atoms.get(&0).context("Original atom")?.location
    );
    let expected = [
        Point3 {
            x: 9.0,
            y: 6.0,
            z: 0.0,
        },
        Point3 {
            x: 7.0,
            y: 9.0,
            z: 0.0,
        },
        Point3 {
            x: 7.0,
            y: 9.0,
            z: 0.0,
        },
    ];
    for (a, b) in out.conformer.positions.iter().zip(expected) {
        assert_eq!((a.x, a.y, a.z), (b.x, b.y, b.z));
    }
    assert!(!out.conformer.is_3d);
    assert_eq!(out.conformer_id, 0);
    Ok(())
}
