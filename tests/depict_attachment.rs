use anyhow::Context;
use reshiki::chemistry::{
    depict::{
        attachment::{AtomData, Error, Input},
        geometry::{self, Bounds, Point},
        rings::{EmbeddedAtom, Fragment},
    },
    electronic::Hybridization,
    graph::{Atom, Bond, Graph},
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
struct Step {
    op: String,
    atom: usize,
    target: usize,
    fragment: NativeFragment,
    #[serde(default)]
    error: bool,
}
#[derive(Deserialize)]
struct Expected {
    ascending: Vec<usize>,
    descending: Vec<usize>,
    initial: NativeFragment,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    state: State,
    atom_data: Vec<AtomData>,
    mode: String,
    seed: usize,
    patch: u32,
    bond_length: String,
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
            value.done == expected.done && value.attachment_points == expected.attachment_points,
            "Fragment defaults {name}"
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
                "Native attachment metadata {name}/atom {key}: {integers:?} vs {:?}",
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
#[test]
fn native_neighbor_setup_and_attachment() -> anyhow::Result<()> {
    compare("depict-attachment-linux-native.json.gz", true)?;
    compare("depict-attachment-macos-native.json.gz", false)?;
    compare("depict-attachment-windows-native.json.gz", false)
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
        .arg(root.join("tests/depict_attachment_reference.py"))
        .arg("--fixture")
        .arg(root.join("tests/fixtures").join(fixture));
    let live = baseline && std::env::var_os("RESHIKI_DEPICT_ATTACHMENT_ORACLE").is_some();
    if live {
        command
            .arg("--oracle")
            .arg(std::env::var_os("RESHIKI_DEPICT_ATTACHMENT_ORACLE").context("Missing oracle")?)
            .arg("--replay")
            .arg("--rdkit-source")
            .arg(std::env::var_os("RESHIKI_RDKIT_SOURCE").context("Missing native source")?);
    }
    let output = command.output()?;
    anyhow::ensure!(
        output.status.success(),
        "Oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)?;
    let mut lines = text.lines();
    let header: serde_json::Value =
        serde_json::from_str(lines.next().context("Missing provenance")?)?;
    anyhow::ensure!(
        header["provenance"]["commit"] == "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
        "Wrong source"
    );
    // Same-platform fixtures and direct native replay always require every
    // f64 bit. The other ABI remains a descriptive audit, never an epsilon.
    let exact = live
        || (baseline && cfg!(all(target_os = "linux", target_arch = "x86_64")))
        || (fixture.ends_with("-macos-native.json.gz")
            && cfg!(all(target_os = "macos", target_arch = "aarch64")))
        || (fixture.ends_with("-windows-native.json.gz") && cfg!(windows));
    let mut audit = Audit::default();
    let mut steps = 0usize;
    let mut native_errors = 0usize;
    for line in lines {
        let case: Case = serde_json::from_str(line)?;
        audit.cases += 1;
        let unchanged = serde_json::to_value((&case.state, &case.atom_data))?;
        let input = Input::new(&case.state.graph, &case.atom_data)?;
        let ids: Vec<_> = (0..case.state.graph.atoms.len()).collect();
        anyhow::ensure!(
            input.ranked_atoms(&ids, true)? == case.expected.ascending,
            "Ascending {}",
            case.name
        );
        anyhow::ensure!(
            input.ranked_atoms(&ids, false)? == case.expected.descending,
            "Descending {}",
            case.name
        );
        let mut fragment = if case.mode == "single" && case.patch == 0 {
            let result = input.single_atom(case.seed)?;
            audit.fragment(
                &result,
                &case.expected.initial,
                &format!("{}/single", case.name),
                exact,
            )?;
            result
        } else {
            restore(&case.expected.initial)?
        };
        for (index, step) in case.expected.steps.iter().enumerate() {
            let before = serde_json::to_value(&fragment)?;
            let apply = || match step.op.as_str() {
                "setup" => input.setup_neighbors(&fragment),
                "update" => input.update_neighbors(&fragment, step.atom),
                "add" => input.add_non_ring_atom(
                    &fragment,
                    step.atom,
                    step.target,
                    decode(&case.bond_length).map_err(|_| Error::Invalid("test bits"))?,
                ),
                _ => Err(Error::Invalid("test operation")),
            };
            if step.error {
                native_errors += 1;
                let expected = if case.patch == 5 {
                    Error::Invalid("attachment normal is too short")
                } else {
                    Error::Geometry(geometry::Error::Numeric)
                };
                anyhow::ensure!(
                    apply() == Err(expected.clone()),
                    "Native exception {}",
                    case.name
                );
                anyhow::ensure!(
                    apply() == Err(expected),
                    "Repeated native exception {}",
                    case.name
                );
                anyhow::ensure!(
                    serde_json::to_value(&fragment)? == before,
                    "Failure changed input {}",
                    case.name
                );
                break;
            }
            let actual = apply().with_context(|| format!("{}/{index}/{}", case.name, step.op))?;
            anyhow::ensure!(actual == apply()?, "Repeat {}", case.name);
            anyhow::ensure!(
                serde_json::to_value(&fragment)? == before,
                "Fragment changed {}",
                case.name
            );
            audit.fragment(
                &actual,
                &step.fragment,
                &format!("{}/{index}/{}", case.name, step.op),
                exact,
            )?;
            fragment = actual;
            steps += 1;
        }
        anyhow::ensure!(
            serde_json::to_value((&case.state, &case.atom_data))? == unchanged,
            "Inputs changed {}",
            case.name
        );
    }
    anyhow::ensure!(audit.cases == 970, "Coverage changed");
    anyhow::ensure!(native_errors == 3, "Native exception coverage changed");
    eprintln!(
        "{fixture}, live={live}, exact={exact}, steps={steps}, native_errors={native_errors}: {audit:?}"
    );
    Ok(())
}

fn chain() -> (Graph, Vec<AtomData>) {
    (
        Graph {
            atoms: vec![
                Atom {
                    atomic_number: 6,
                    ..Atom::default()
                };
                4
            ],
            bonds: (0..3)
                .map(|a| Bond {
                    a,
                    b: a + 1,
                    order: 1,
                    aromatic: false,
                })
                .collect(),
        },
        vec![
            AtomData {
                hybridization: Hybridization::Sp3,
                cip_rank: None,
                chiral_rank: None
            };
            4
        ],
    )
}
#[test]
fn checked_attachment_errors_are_atomic() -> anyhow::Result<()> {
    let (graph, data) = chain();
    let input = Input::new(&graph, &data)?;
    let initial = input.single_atom(0)?;
    let unchanged = serde_json::to_value(&initial)?;
    for (id, target) in [(0, 0), (2, 0), (4, 0), (1, 3), (1, usize::MAX)] {
        assert!(matches!(
            input.add_non_ring_atom(&initial, id, target, 1.5),
            Err(Error::Invalid(_))
        ));
    }
    assert!(matches!(input.single_atom(4), Err(Error::Invalid(_))));
    assert!(matches!(
        input.update_neighbors(&initial, 4),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        input.ranked_atoms(&[4], true),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        Input::new(&graph, &data[..3]),
        Err(Error::Invalid(_))
    ));
    for nonfinite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            input.add_non_ring_atom(&initial, 1, 0, nonfinite),
            Err(Error::Geometry(geometry::Error::NonFinite))
        );
    }
    assert_eq!(
        input.add_non_ring_atom(&initial, 1, 0, 0.0),
        Err(Error::Geometry(geometry::Error::Numeric))
    );
    for mutation in 0..9 {
        let mut invalid = initial.clone();
        let a = invalid.atoms.get_mut(&0).context("Missing seed")?;
        match mutation {
            0 => a.normal = Point::default(),
            1 => a.neighbors.clear(),
            2 => a.neighbors.push(1),
            3 => a.neighbor1 = Some(usize::MAX),
            4 => a.id = usize::MAX,
            5 => a.rotation_direction = 2,
            6 => {
                a.angle = 1.0;
                a.neighbor1 = Some(2);
                a.neighbor2 = Some(3);
            }
            7 => invalid.attachment_points.push(0),
            _ => invalid.attachment_points = vec![3],
        }
        let before = serde_json::to_value(&invalid)?;
        assert!(matches!(
            input.add_non_ring_atom(&invalid, 1, 0, 1.5),
            Err(Error::Invalid(_))
        ));
        assert_eq!(serde_json::to_value(&invalid)?, before);
    }
    let limited = Input::new(&graph, &data)?.with_work_limit(0);
    assert_eq!(limited.single_atom(0), Err(Error::Limit));
    assert_eq!(limited.setup_neighbors(&initial), Err(Error::Limit));
    assert_eq!(limited.update_neighbors(&initial, 0), Err(Error::Limit));
    assert_eq!(
        limited.add_non_ring_atom(&initial, 1, 0, 1.5),
        Err(Error::Limit)
    );
    assert_eq!(limited.ranked_atoms(&[0], true), Err(Error::Limit));
    assert_eq!(serde_json::to_value(&initial)?, unchanged);
    Ok(())
}
#[test]
fn stale_attachment_and_default_aid_are_preserved() -> anyhow::Result<()> {
    let (graph, data) = chain();
    let input = Input::new(&graph, &data)?;
    let value = input.add_non_ring_atom(&input.single_atom(1)?, 0, 1, 1.5)?;
    assert_eq!(value.atoms.get(&0).context("Added atom")?.id, 0);
    let value = input.add_non_ring_atom(&value, 2, 1, 1.5)?;
    assert_eq!(value.atoms.get(&2).context("Added atom")?.id, 0);
    assert!(value.attachment_points.contains(&1));
    assert!(
        value
            .atoms
            .get(&1)
            .context("Reference atom")?
            .neighbors
            .is_empty()
    );
    assert!(
        input
            .update_neighbors(&value, 1)?
            .attachment_points
            .contains(&1)
    );
    assert!(
        !input
            .setup_neighbors(&value)?
            .attachment_points
            .contains(&1)
    );
    Ok(())
}

#[test]
fn attachment_storage_and_empty_inputs() -> anyhow::Result<()> {
    let empty_graph = Graph {
        atoms: Vec::new(),
        bonds: Vec::new(),
    };
    let empty = Input::new(&empty_graph, &[])?;
    let fragment = Fragment {
        atoms: BTreeMap::new(),
        done: false,
        bounds: Bounds {
            positive_x: 0.0,
            negative_x: 0.0,
            positive_y: 0.0,
            negative_y: 0.0,
        },
        attachment_points: Vec::new(),
    };
    assert_eq!(empty.setup_neighbors(&fragment)?, fragment);
    assert_eq!(empty.ranked_atoms(&[], true)?, Vec::<usize>::new());
    assert!(matches!(empty.single_atom(0), Err(Error::Invalid(_))));
    let (mut graph, data) = chain();
    graph
        .atoms
        .resize(geometry::MAX_POINTS + 1, Atom::default());
    assert!(matches!(Input::new(&graph, &data), Err(Error::Limit)));
    let (mut graph, data) = chain();
    graph.bonds[0].b = usize::MAX;
    assert!(matches!(Input::new(&graph, &data), Err(Error::Invalid(_))));
    let (graph, data) = chain();
    let input = Input::new(&graph, &data)?;
    assert_eq!(input.ranked_atoms(&[1, 1, 0, 0], true)?, vec![0, 0, 1, 1]);
    assert_eq!(
        input.ranked_atoms(&vec![0; 600_001], true),
        Err(Error::Limit)
    );
    let mut initial = input.single_atom(0)?;
    initial.atoms.get_mut(&0).context("Missing seed")?.neighbors = vec![1; 600_001];
    assert_eq!(input.setup_neighbors(&initial), Err(Error::Limit));
    Ok(())
}
