#[path = "common/depict_windows.rs"]
mod depict_windows;

use anyhow::Context;
use reshiki::chemistry::{
    depict::{
        geometry,
        rings::{Error, Fragment, Input, MAX_RING_ATOMS, MAX_RINGS, NextRing},
    },
    graph::{Atom, Bond, Graph},
    ranking::Metadata,
    stereo::perception::{RingCache, RingKind, State},
};
use serde::Deserialize;
use std::{path::Path, process::Command};

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
}
#[derive(Debug, Deserialize)]
struct Expected {
    first: i64,
    core: Vec<usize>,
    next: Option<NextRing>,
    fragment: Option<NativeFragment>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    state: State,
    selected: Vec<usize>,
    done: Vec<usize>,
    bond_length: String,
    construct: bool,
    expected: Expected,
}
fn decode(value: &str) -> anyhow::Result<f64> {
    Ok(f64::from_bits(u64::from_str_radix(value, 16)?))
}
fn id(value: Option<usize>) -> anyhow::Result<i64> {
    value.map_or(Ok(-1), |v| Ok(i64::try_from(v)?))
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
    nonfinite: usize,
    worst: String,
}
impl Audit {
    fn value(
        &mut self,
        a: f64,
        bits: &str,
        name: &str,
        exact: bool,
        projection: Option<f64>,
    ) -> anyhow::Result<()> {
        let b = decode(bits)?;
        anyhow::ensure!(
            a.is_finite() && b.is_finite(),
            "Unexpected nonfinite {name}"
        );
        self.scalars += 1;
        if a.to_bits() != b.to_bits() {
            self.differing += 1;
            if exact {
                anyhow::bail!("Native f64 difference {name}: {a:.17e} vs {b:.17e}");
            }
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
            let pa = (sign * a * 28.0) as f32;
            let pb = (sign * b * 28.0) as f32;
            if pa.to_bits() != pb.to_bits() {
                self.projected += 1;
            }
            self.max_projected = self
                .max_projected
                .max((f64::from(pa) - f64::from(pb)).abs());
        }
        Ok(())
    }
    fn fragment(
        &mut self,
        value: &Fragment,
        expected: &NativeFragment,
        name: &str,
        exact: bool,
    ) -> anyhow::Result<()> {
        self.fragments += 1;
        anyhow::ensure!(
            value.done == expected.done && value.attachment_points.is_empty(),
            "Constructor defaults {name}"
        );
        anyhow::ensure!(
            value.atoms.len() == expected.atoms.len(),
            "Atom count {name}"
        );
        for (index, (actual, bits)) in [
            value.bounds.positive_x,
            value.bounds.negative_x,
            value.bounds.positive_y,
            value.bounds.negative_y,
        ]
        .into_iter()
        .zip(&expected.bounds)
        .enumerate()
        {
            self.value(actual, bits, &format!("{name}/bounds/{index}"), true, None)?;
        }
        anyhow::ensure!(expected.bounds.len() == 4, "Native bound count");
        for ((&key, atom), expected) in value.atoms.iter().zip(&expected.atoms) {
            self.atoms += 1;
            let integers = vec![
                i64::try_from(key)?,
                i64::try_from(atom.id)?,
                id(atom.neighbor1)?,
                id(atom.neighbor2)?,
                id(atom.cis_trans_neighbor)?,
                i64::from(atom.counter_clockwise),
                i64::from(atom.rotation_direction),
                i64::from(atom.fixed),
            ];
            anyhow::ensure!(
                integers == expected.ints && atom.neighbors == expected.neighbors,
                "Native constructor metadata {name}/atom {key}: {integers:?} vs {:?}",
                expected.ints
            );
            anyhow::ensure!(expected.values.len() == 6, "Native atom scalar count");
            for (index, (actual, bits)) in [
                atom.location.x,
                atom.location.y,
                atom.normal.x,
                atom.normal.y,
                atom.angle,
                atom.density,
            ]
            .into_iter()
            .zip(&expected.values)
            .enumerate()
            {
                self.value(
                    actual,
                    bits,
                    &format!("{name}/atom {key}/scalar {index}"),
                    exact,
                    match index {
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

#[test]
fn native_ring_selection_and_constructor() -> anyhow::Result<()> {
    compare("depict-rings-linux-native.json.gz", true)?;
    compare("depict-rings-macos-native.json.gz", false)?;
    compare("depict-rings-windows-native.json.gz", false)?;
    compare("depict-rings-windows-no-fma3-native.json.gz", false)?;
    compare("depict-rings-windows-server2022-native.json.gz", false)
}
fn compare(fixture: &str, baseline: bool) -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut command = Command::new(python);
    command
        .arg(root.join("tests/depict_rings_reference.py"))
        .arg("--fixture")
        .arg(root.join("tests/fixtures").join(fixture));
    let live = baseline && std::env::var_os("RESHIKI_DEPICT_RINGS_ORACLE").is_some();
    if live {
        command
            .arg("--oracle")
            .arg(std::env::var_os("RESHIKI_DEPICT_RINGS_ORACLE").context("Missing oracle")?)
            .arg("--replay")
            .arg("--rdkit-source")
            .arg(std::env::var_os("RESHIKI_RDKIT_SOURCE").context("Missing native source")?);
    }
    let output = command.output()?;
    anyhow::ensure!(
        output.status.success(),
        "Reference reader: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)?;
    let mut lines = text.lines();
    let header: serde_json::Value =
        serde_json::from_str(lines.next().context("Missing provenance")?)?;
    anyhow::ensure!(
        header["provenance"]["commit"] == "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
            && header["provenance"]["templates"] == false,
        "Wrong native source or mode"
    );
    // Same-platform fixtures and direct native replay always require every
    // f64 bit. The other ABI remains a descriptive audit, never an epsilon.
    let exact = live
        || (baseline && cfg!(all(target_os = "linux", target_arch = "x86_64")))
        || (fixture.ends_with("-macos-native.json.gz")
            && cfg!(all(target_os = "macos", target_arch = "aarch64")))
        || depict_windows::fixture("rings")?.as_deref() == Some(fixture);
    let mut audit = Audit::default();
    for line in lines {
        let case: Case = serde_json::from_str(line)?;
        audit.cases += 1;
        let before = serde_json::to_value(&case.state)?;
        let input = Input::new(
            &case.state.graph,
            &case.state.metadata,
            &case.state.rings,
            &case.selected,
        )
        .with_context(|| case.name.clone())?;
        anyhow::ensure!(
            input.source_ring_ids() == case.selected,
            "Source selection changed"
        );
        anyhow::ensure!(
            id(input.pick_first()?)? == case.expected.first,
            "First ring {}",
            case.name
        );
        anyhow::ensure!(
            input.core()? == case.expected.core,
            "Core rings {}",
            case.name
        );
        match &case.expected.next {
            Some(next) => {
                anyhow::ensure!(input.next(&case.done)? == *next, "Next ring {}", case.name)
            }
            None => anyhow::ensure!(
                matches!(input.next(&case.done), Err(Error::Invalid(_))),
                "Missing native next-ring failure {}",
                case.name
            ),
        }
        if case.construct {
            let actual = input.embed_without_templates(decode(&case.bond_length)?)?;
            anyhow::ensure!(
                actual == input.embed_without_templates(decode(&case.bond_length)?)?,
                "Repeated constructor {}",
                case.name
            );
            let expected = case
                .expected
                .fragment
                .as_ref()
                .context("Native constructor failed")?;
            if expected
                .atoms
                .iter()
                .flat_map(|a| &a.values)
                .any(|s| decode(s).is_ok_and(|v| !v.is_finite()))
            {
                audit.nonfinite += 1;
                anyhow::bail!("Unclassified native nonfinite {}", case.name);
            }
            audit.fragment(&actual, expected, &case.name, exact)?;
        }
        anyhow::ensure!(
            serde_json::to_value(&case.state)? == before,
            "Input changed {}",
            case.name
        );
    }
    anyhow::ensure!(audit.cases == 1338, "Unexpected native coverage");
    eprintln!("{fixture}, live={live}, exact={exact}: {audit:?}");
    Ok(())
}

fn triangle_graph() -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            3
        ],
        bonds: vec![
            Bond {
                a: 0,
                b: 1,
                order: 1,
                aromatic: false,
            },
            Bond {
                a: 1,
                b: 2,
                order: 1,
                aromatic: false,
            },
            Bond {
                a: 2,
                b: 0,
                order: 1,
                aromatic: false,
            },
        ],
    }
}
#[test]
fn checked_ring_inputs_and_work_limits() -> anyhow::Result<()> {
    let graph = triangle_graph();
    let metadata = Metadata::unspecified(&graph);
    let cache = RingCache {
        kind: RingKind::Symmetric,
        atoms: vec![vec![0, 1, 2]],
    };
    let input = Input::new(&graph, &metadata, &cache, &[0])?.with_work_limit(0);
    assert_eq!(input.pick_first(), Err(Error::Limit));
    assert_eq!(input.core(), Err(Error::Limit));
    assert_eq!(input.embed_without_templates(1.5), Err(Error::Limit));
    let input = Input::new(&graph, &metadata, &cache, &[0])?;
    assert_eq!(
        input.embed_without_templates(f64::NAN),
        Err(Error::Geometry(geometry::Error::NonFinite))
    );
    assert!(matches!(input.next(&[]), Err(Error::Invalid(_))));
    assert!(matches!(
        Input::new(&graph, &metadata, &cache, &[1]),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        Input::new(&graph, &metadata, &cache, &vec![0; MAX_RINGS + 1]),
        Err(Error::Limit)
    ));
    for bad in [vec![0, 1], vec![0, 1, 1], vec![0, 1, usize::MAX]] {
        let cache = RingCache {
            kind: RingKind::Symmetric,
            atoms: vec![bad],
        };
        assert!(matches!(
            Input::new(&graph, &metadata, &cache, &[0]),
            Err(Error::Invalid(_))
        ));
    }
    let oversized = RingCache {
        kind: RingKind::Symmetric,
        atoms: vec![vec![0; MAX_RING_ATOMS + 1]],
    };
    assert!(matches!(
        Input::new(&graph, &metadata, &oversized, &[0]),
        Err(Error::Limit)
    ));
    let empty = Input::new(&graph, &metadata, &cache, &[])?;
    assert_eq!(empty.pick_first()?, None);
    assert!(empty.core()?.is_empty());
    assert!(matches!(
        empty.embed_without_templates(1.5),
        Err(Error::Invalid(_))
    ));
    let mut disconnected = graph.clone();
    disconnected.atoms.extend(graph.atoms.clone());
    disconnected.bonds.extend(graph.bonds.iter().map(|b| Bond {
        a: b.a + 3,
        b: b.b + 3,
        order: 1,
        aromatic: false,
    }));
    let metadata = Metadata::unspecified(&disconnected);
    let cache = RingCache {
        kind: RingKind::Symmetric,
        atoms: vec![vec![0, 1, 2], vec![3, 4, 5]],
    };
    let input = Input::new(&disconnected, &metadata, &cache, &[0, 1])?;
    assert!(matches!(
        input.embed_without_templates(1.5),
        Err(Error::Invalid(_))
    ));
    Ok(())
}
