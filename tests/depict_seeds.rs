#[path = "common/depict_windows.rs"]
mod depict_windows;

use anyhow::Context;
use reshiki::chemistry::{
    depict::{
        attachment::AtomData,
        geometry::{self, Coordinates, Point},
        rings::Fragment,
        seeds::{Error, IdealLengths, Input},
    },
    electronic::Hybridization,
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
    attachment_points: Vec<usize>,
}
#[derive(Deserialize)]
struct Expected {
    fragment: Option<NativeFragment>,
    error: bool,
    ranked: Option<Vec<usize>>,
    across: Option<Vec<i64>>,
    axial: Option<Vec<i64>>,
    angles: Option<Vec<String>>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    state: State,
    atom_data: Vec<AtomData>,
    mode: String,
    seed: usize,
    ranks: Vec<i32>,
    coordinates: Vec<(usize, String, String)>,
    current_length: String,
    ideal_lengths: [String; 3],
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

#[test]
fn native_coordination_coordinate_and_bond_seeds() -> anyhow::Result<()> {
    compare("depict-seeds-linux-native.json.gz", true)?;
    compare("depict-seeds-macos-native.json.gz", false)?;
    compare("depict-seeds-windows-native.json.gz", false)
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
        .arg(root.join("tests/depict_seeds_reference.py"))
        .arg("--fixture")
        .arg(root.join("tests/fixtures").join(fixture));
    let live = baseline && std::env::var_os("RESHIKI_DEPICT_SEEDS_ORACLE").is_some();
    if live {
        command
            .arg("--oracle")
            .arg(std::env::var_os("RESHIKI_DEPICT_SEEDS_ORACLE").context("Missing oracle")?)
            .arg("--replay")
            .arg("--rdkit-source")
            .arg(std::env::var_os("RESHIKI_RDKIT_SOURCE").context("Missing source")?);
    }
    let output = command.output()?;
    anyhow::ensure!(
        output.status.success(),
        "Native reader: {}",
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
    // Same-platform fixtures and direct native replay always require every
    // f64 bit. The other ABI remains a descriptive audit, never an epsilon.
    let exact = live
        || (baseline && cfg!(all(target_os = "linux", target_arch = "x86_64")))
        || (fixture.ends_with("-macos-native.json.gz")
            && cfg!(all(target_os = "macos", target_arch = "aarch64")))
        || depict_windows::fixture("seeds")?.as_deref() == Some(fixture);
    let mut audit = Audit::default();
    let mut errors = 0usize;
    let mut queries = 0usize;
    for line in lines {
        let case: Case = serde_json::from_str(line)?;
        audit.cases += 1;
        let before =
            serde_json::to_value((&case.state, &case.atom_data, &case.ranks, &case.coordinates))?;
        let input = Input::new(
            &case.state.graph,
            &case.state.metadata,
            &case.atom_data,
            &case.state.rings,
        )?;
        let lengths = IdealLengths {
            square_planar: decode(&case.ideal_lengths[0])?,
            trigonal_bipyramidal: decode(&case.ideal_lengths[1])?,
            octahedral: decode(&case.ideal_lengths[2])?,
        };
        let coordinates = case
            .coordinates
            .iter()
            .map(|(id, x, y)| {
                Ok((
                    *id,
                    Point {
                        x: decode(x)?,
                        y: decode(y)?,
                    },
                ))
            })
            .collect::<anyhow::Result<Coordinates>>()?;
        let current = decode(&case.current_length)?;
        let apply = || match case.mode.as_str() {
            "bond" => input.cis_trans(case.seed, current).map(Some),
            "coordinates" => input.from_coordinates(&coordinates).map(Some),
            _ => input.coordination(case.seed, &case.ranks, &lengths),
        };
        let actual = apply();
        if case.expected.error {
            errors += 1;
            if case.mode == "bond" || case.name.starts_with("missing-cache/") {
                anyhow::ensure!(
                    matches!(actual, Err(Error::Invalid(_))),
                    "Native precondition {}: {actual:?}",
                    case.name
                );
            } else {
                anyhow::ensure!(
                    actual == Err(Error::Geometry(geometry::Error::Numeric)),
                    "Native normalization {}: {actual:?}",
                    case.name
                );
            }
        } else {
            let actual = actual.with_context(|| case.name.clone())?;
            match (&actual, &case.expected.fragment) {
                (Some(actual), Some(expected)) => {
                    audit.fragment(actual, expected, &case.name, exact)?
                }
                (None, None) => (),
                _ => anyhow::bail!("Fragment presence {}", case.name),
            }
        }
        anyhow::ensure!(apply() == apply(), "Repeated seed {}", case.name);
        if let Some(expected) = &case.expected.ranked {
            anyhow::ensure!(
                input.ranked_neighbors(case.seed, &case.ranks)? == *expected,
                "Ranked neighbors {}",
                case.name
            );
            let mut adjacent = Vec::new();
            for bond in &case.state.graph.bonds {
                if bond.a == case.seed {
                    adjacent.push(bond.b);
                } else if bond.b == case.seed {
                    adjacent.push(bond.a);
                }
            }
            let across = adjacent
                .iter()
                .map(|&ligand| id(input.across(case.seed, ligand)?))
                .collect::<anyhow::Result<Vec<_>>>()?;
            anyhow::ensure!(Some(across) == case.expected.across, "Across {}", case.name);
            let axial = vec![
                id(input.axial(case.seed, false)?)?,
                id(input.axial(case.seed, true)?)?,
            ];
            anyhow::ensure!(Some(axial) == case.expected.axial, "Axial {}", case.name);
            let mut angles = Vec::new();
            for &first in &adjacent {
                for &second in &adjacent {
                    angles.push(format!(
                        "{:016x}",
                        input.ideal_angle(case.seed, first, second)?.to_bits()
                    ));
                    queries += 1;
                }
            }
            anyhow::ensure!(
                Some(angles) == case.expected.angles,
                "Ideal angles {}",
                case.name
            );
        }
        anyhow::ensure!(
            serde_json::to_value((&case.state, &case.atom_data, &case.ranks, &case.coordinates))?
                == before,
            "Inputs changed {}",
            case.name
        );
    }
    anyhow::ensure!(audit.cases == 991, "Coverage changed");
    anyhow::ensure!(
        errors == 75 && queries == 9478,
        "Native error/query coverage changed"
    );
    eprintln!(
        "{fixture}, live={live}, exact={exact}, errors={errors}, queries={queries}: {audit:?}"
    );
    Ok(())
}
fn star(
    degree: usize,
    tag: u8,
    permutation: Option<u32>,
) -> (Graph, Metadata, Vec<AtomData>, RingCache) {
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            degree + 1
        ],
        bonds: (1..=degree)
            .map(|b| Bond {
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    let mut metadata = Metadata::unspecified(&graph);
    if let Some(center) = metadata.atoms.get_mut(0) {
        center.chiral_tag = tag;
        center.chiral_permutation = permutation;
    }
    let data = vec![
        AtomData {
            hybridization: Hybridization::Sp3,
            cip_rank: None,
            chiral_rank: None
        };
        degree + 1
    ];
    (
        graph,
        metadata,
        data,
        RingCache {
            kind: RingKind::Symmetric,
            atoms: Vec::new(),
        },
    )
}
fn lengths() -> IdealLengths {
    IdealLengths {
        square_planar: 1.5,
        trigonal_bipyramidal: 1.5,
        octahedral: 1.5,
    }
}
#[test]
fn malformed_native_ub_is_checked_without_mutation() -> anyhow::Result<()> {
    // Native square-planar nbrs[0] with no neighbor and TBP idealPoints[5]
    // with four equatorial ligands are unchecked source accesses. Do not run
    // those native UB cases; require a typed Rust error and unchanged input.
    for (degree, tag, permutation) in [(0, 6, Some(1)), (4, 7, None), (5, 7, Some(0))] {
        let (graph, metadata, data, rings) = star(degree, tag, permutation);
        let before = serde_json::to_value((&graph, &metadata, &data, &rings))?;
        let input = Input::new(&graph, &metadata, &data, &rings)?;
        assert!(matches!(
            input.coordination(0, &vec![0; degree + 1], &lengths()),
            Err(Error::Invalid(_))
        ));
        assert_eq!(
            serde_json::to_value((&graph, &metadata, &data, &rings))?,
            before
        );
    }
    Ok(())
}
#[test]
fn seed_bounds_indices_and_finite_inputs() -> anyhow::Result<()> {
    let (graph, metadata, data, rings) = star(4, 6, Some(1));
    let input = Input::new(&graph, &metadata, &data, &rings)?;
    assert!(matches!(
        input.ranked_neighbors(0, &[0]),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        input.coordination(99, &[0; 5], &lengths()),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(input.across(0, 99), Err(Error::Invalid(_))));
    assert!(matches!(input.axial(99, false), Err(Error::Invalid(_))));
    assert!(matches!(
        input.ideal_angle(0, 1, 99),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(input.cis_trans(0, 1.5), Err(Error::Invalid(_))));
    assert!(matches!(
        input.from_coordinates(&[(99, Point::default())].into()),
        Err(Error::Invalid(_))
    ));
    assert_eq!(
        input.from_coordinates(
            &[(
                0,
                Point {
                    x: f64::NAN,
                    y: 0.0
                }
            )]
            .into()
        ),
        Err(Error::Geometry(geometry::Error::NonFinite))
    );
    let invalid = IdealLengths {
        square_planar: f64::INFINITY,
        ..lengths()
    };
    assert_eq!(
        input.coordination(0, &[0; 5], &invalid),
        Err(Error::Geometry(geometry::Error::NonFinite))
    );
    let limited = Input::new(&graph, &metadata, &data, &rings)?.with_work_limit(0);
    assert_eq!(
        limited.from_coordinates(&[(0, Point::default())].into()),
        Err(Error::Limit)
    );
    assert_eq!(
        limited.coordination(0, &[0; 5], &lengths()),
        Err(Error::Limit)
    );
    assert_eq!(limited.cis_trans(0, 1.5), Err(Error::Limit));
    let oversized = RingCache {
        kind: RingKind::Symmetric,
        atoms: vec![Vec::new(); 100_001],
    };
    assert!(matches!(
        Input::new(&graph, &metadata, &data, &oversized),
        Err(Error::Limit)
    ));
    let oversized = RingCache {
        kind: RingKind::Symmetric,
        atoms: vec![vec![0; 1_000_001]],
    };
    assert!(matches!(
        Input::new(&graph, &metadata, &data, &oversized),
        Err(Error::Limit)
    ));
    let invalid = RingCache {
        kind: RingKind::Symmetric,
        atoms: vec![vec![99]],
    };
    assert!(matches!(
        Input::new(&graph, &metadata, &data, &invalid),
        Err(Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn quadratic_angle_storage_is_bounded_before_allocation() -> anyhow::Result<()> {
    let (graph, metadata, data, rings) = star(1416, 0, None);
    let input = Input::new(&graph, &metadata, &data, &rings)?;
    let mut coordinates: Coordinates = (1..1416).map(|id| (id, Point { x: 1.0, y: 0.0 })).collect();
    coordinates.insert(0, Point::default());
    let before = serde_json::to_value(&coordinates)?;
    assert_eq!(input.from_coordinates(&coordinates), Err(Error::Limit));
    assert_eq!(serde_json::to_value(&coordinates)?, before);
    Ok(())
}

#[test]
fn cis_trans_controls_and_constructor_state_are_checked() -> anyhow::Result<()> {
    let graph = Graph {
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
                order: if a == 1 { 2 } else { 1 },
                aromatic: false,
            })
            .collect(),
    };
    let mut metadata = Metadata::unspecified(&graph);
    let stereo = metadata.bonds.get_mut(1).context("Missing stereo bond")?;
    stereo.stereo = 2;
    stereo.stereo_atoms = vec![0, 3];
    let data = vec![
        AtomData {
            hybridization: Hybridization::Sp2,
            cip_rank: None,
            chiral_rank: None
        };
        4
    ];
    let rings = RingCache::default();
    let input = Input::new(&graph, &metadata, &data, &rings)?;
    let value = input.cis_trans(1, 0.0)?;
    assert!(value.attachment_points.is_empty());
    assert!(
        value
            .atoms
            .values()
            .all(|a| a.neighbors.is_empty() && !a.fixed)
    );
    assert_eq!(
        value.atoms.get(&2).context("Missing second atom")?.location,
        Point::default()
    );
    assert_eq!(
        input.cis_trans(1, f64::NAN),
        Err(Error::Geometry(geometry::Error::NonFinite))
    );
    for controls in [vec![3, 0], vec![2, 1], vec![1, 2], vec![0]] {
        let mut invalid = metadata.clone();
        invalid
            .bonds
            .get_mut(1)
            .context("Missing stereo bond")?
            .stereo_atoms = controls;
        let before = serde_json::to_value(&invalid)?;
        let input = Input::new(&graph, &invalid, &data, &rings)?;
        assert!(matches!(input.cis_trans(1, 1.5), Err(Error::Invalid(_))));
        assert_eq!(serde_json::to_value(&invalid)?, before);
    }
    Ok(())
}
