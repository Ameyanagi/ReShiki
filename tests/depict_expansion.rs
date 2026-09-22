use anyhow::Context;
use reshiki::chemistry::{
    depict::{
        attachment::AtomData,
        expansion,
        geometry::{Bounds, Coordinates, Point},
        rings::{EmbeddedAtom, Fragment},
    },
    stereo::perception::State,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};
#[derive(Deserialize)]
struct Atom {
    ints: Vec<i64>,
    values: Vec<String>,
    neighbors: Vec<usize>,
}
#[derive(Deserialize)]
struct Native {
    done: bool,
    bounds: Vec<String>,
    atoms: Vec<Atom>,
    attachment_points: Vec<usize>,
}
#[derive(Deserialize)]
struct Merge {
    before: Vec<Native>,
    common: Vec<usize>,
    target: i64,
    neighbor: i64,
    after: Option<Vec<Native>>,
    common_after: Vec<usize>,
}
#[derive(Deserialize)]
struct Step {
    before: Vec<Native>,
    remaining: Vec<usize>,
    master: usize,
    after: Vec<Native>,
    remaining_after: Vec<usize>,
}
#[derive(Deserialize)]
struct Expected {
    state: State,
    fragments: Vec<Native>,
    seeded: Vec<Native>,
    unembedded: Vec<usize>,
    steps: Vec<Step>,
    merges: Vec<Merge>,
    final_boundary: Vec<String>,
    depict_ranks: Vec<i32>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    state: State,
    chiral_ranks: Vec<Option<u32>>,
    coordinates: Option<Vec<(usize, String, String)>>,
    bond_length: String,
    templates: bool,
    expected: Option<Expected>,
    #[serde(default)]
    public_native: Option<Vec<serde_json::Value>>,
}
fn decode(text: &str) -> anyhow::Result<f64> {
    Ok(f64::from_bits(u64::from_str_radix(text, 16)?))
}
fn id(value: Option<usize>) -> anyhow::Result<i64> {
    value.map_or(Ok(-1), |i| Ok(i64::try_from(i)?))
}
#[derive(Default, Debug)]
struct Audit {
    exact: bool,
    fragments: usize,
    scalars: usize,
    unequal: usize,
    projected: usize,
}
impl Audit {
    fn number(
        &mut self,
        value: f64,
        expected: &str,
        projection: Option<f64>,
        name: &str,
    ) -> anyhow::Result<()> {
        let expected = decode(expected)?;
        self.scalars += 1;
        anyhow::ensure!(
            value.is_finite() && expected.is_finite(),
            "Nonfinite {name}"
        );
        if value.to_bits() != expected.to_bits() {
            self.unequal += 1;
            if self.exact {
                anyhow::bail!("f64 {name}: {value:.17e} != {expected:.17e}");
            }
        }
        if let Some(sign) = projection
            && ((sign * value * 28.) as f32).to_bits() != ((sign * expected * 28.) as f32).to_bits()
        {
            self.projected += 1;
        }
        Ok(())
    }
    fn fragment(&mut self, value: &Fragment, expected: &Native, name: &str) -> anyhow::Result<()> {
        self.fragments += 1;
        assert_eq!(value.done, expected.done, "{name}");
        assert_eq!(
            value.attachment_points, expected.attachment_points,
            "{name} attachment"
        );
        assert_eq!(value.atoms.len(), expected.atoms.len(), "{name} count");
        for (v, e) in [
            value.bounds.positive_x,
            value.bounds.negative_x,
            value.bounds.positive_y,
            value.bounds.negative_y,
        ]
        .into_iter()
        .zip(&expected.bounds)
        {
            self.number(v, e, None, name)?;
        }
        for ((&key, a), e) in value.atoms.iter().zip(&expected.atoms) {
            let ints = [
                i64::try_from(key)?,
                i64::try_from(a.id)?,
                id(a.neighbor1)?,
                id(a.neighbor2)?,
                id(a.cis_trans_neighbor)?,
                i64::from(a.counter_clockwise),
                i64::from(a.rotation_direction),
                i64::from(a.fixed),
            ];
            assert_eq!(ints.as_slice(), e.ints, "{name}/{key} metadata");
            assert_eq!(a.neighbors, e.neighbors, "{name}/{key} neighbors");
            for (i, (v, e)) in [
                a.location.x,
                a.location.y,
                a.normal.x,
                a.normal.y,
                a.angle,
                a.density,
            ]
            .into_iter()
            .zip(&e.values)
            .enumerate()
            {
                self.number(
                    v,
                    e,
                    match i {
                        0 => Some(1.),
                        1 => Some(-1.),
                        _ => None,
                    },
                    &format!("{name}/{key}/{i}"),
                )?;
            }
        }
        Ok(())
    }
}

fn index(value: i64) -> anyhow::Result<Option<usize>> {
    Ok(if value < 0 {
        None
    } else {
        Some(usize::try_from(value)?)
    })
}
fn fragment(n: &Native) -> anyhow::Result<Fragment> {
    let mut atoms = BTreeMap::new();
    for a in &n.atoms {
        let [key, id, first, second, control, ccw, rotation, fixed] = a.ints.as_slice() else {
            anyhow::bail!("native integer fields");
        };
        let [x, y, nx, ny, angle, density] = a.values.as_slice() else {
            anyhow::bail!("native scalar fields");
        };
        atoms.insert(
            usize::try_from(*key)?,
            EmbeddedAtom {
                id: usize::try_from(*id)?,
                location: Point {
                    x: decode(x)?,
                    y: decode(y)?,
                },
                normal: Point {
                    x: decode(nx)?,
                    y: decode(ny)?,
                },
                angle: decode(angle)?,
                neighbor1: index(*first)?,
                neighbor2: index(*second)?,
                cis_trans_neighbor: index(*control)?,
                counter_clockwise: *ccw != 0,
                rotation_direction: i32::try_from(*rotation)?,
                neighbors: a.neighbors.clone(),
                density: decode(density)?,
                fixed: *fixed != 0,
            },
        );
    }
    let [px, nx, py, ny] = n.bounds.as_slice() else {
        anyhow::bail!("native bounds");
    };
    Ok(Fragment {
        atoms,
        done: n.done,
        bounds: Bounds {
            positive_x: decode(px)?,
            negative_x: decode(nx)?,
            positive_y: decode(py)?,
            negative_y: decode(ny)?,
        },
        attachment_points: n.attachment_points.clone(),
    })
}
fn malformed(n: &Native, count: usize) -> bool {
    n.bounds
        .iter()
        .chain(n.atoms.iter().flat_map(|a| a.values.iter()))
        .any(|v| decode(v).is_ok_and(|v| !v.is_finite()))
        || n.atoms.iter().any(|a| {
            a.ints
                .first()
                .is_none_or(|&i| i < 0 || usize::try_from(i).is_ok_and(|i| i >= count))
        })
}
fn compare(
    audit: &mut Audit,
    actual: &[Fragment],
    expected: &[Native],
    name: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        actual.len() == expected.len(),
        "{name}: fragment count {} != {}",
        actual.len(),
        expected.len()
    );
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        audit.fragment(a, e, &format!("{name}/{i}"))?;
    }
    Ok(())
}
#[test]
fn native_initial_orchestration_and_merge_stages() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/depict_expansion_reference.py"))
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("fixture output")?).lines();
    let header: serde_json::Value = serde_json::from_str(&lines.next().context("header")??)?;
    assert_eq!(
        header["provenance"]["commit"],
        "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
    );
    let mut audit = Audit {
        exact: cfg!(all(target_os = "linux", target_arch = "x86_64"))
            || cfg!(all(target_os = "macos", target_arch = "aarch64"))
            || cfg!(windows)
            || std::env::var_os("DEPICT_EXPANSION_ORACLE").is_some(),
        ..Default::default()
    };
    let (mut accepted, mut rejected, mut merges, mut steps, mut restrictions) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let source_adapter = header["provenance"]["native_build"]["source_adapter"].is_object();
    if cfg!(windows) {
        assert!(
            source_adapter,
            "Windows requires its native source-adapter capture"
        );
    }
    let (mut public_success, mut public_errors, mut public_scalars) = (0, 0, 0);
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        if source_adapter {
            let outcomes = case
                .public_native
                .as_ref()
                .context("missing independent original public outcomes")?;
            assert_eq!(outcomes.len(), 2, "{} canonical options", case.name);
            for outcome in outcomes {
                match outcome["kind"].as_str() {
                    Some("error") => public_errors += 1,
                    Some("success") => {
                        public_success += 1;
                        assert_eq!(outcome["is3d"], false);
                        let coordinates = outcome["coordinates"]
                            .as_array()
                            .context("public coordinates")?;
                        assert_eq!(coordinates.len(), case.state.graph.atoms.len());
                        for point in coordinates {
                            let point = point.as_array().context("public point")?;
                            assert_eq!(point.len(), 3);
                            for scalar in point {
                                decode(scalar.as_str().context("public scalar")?)?;
                                public_scalars += 1;
                            }
                        }
                    }
                    _ => anyhow::bail!("Invalid independent native outcome {}", case.name),
                }
            }
        }
        let before = serde_json::to_value(&case.state)?;
        let coords = case
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
        let options = expansion::Options {
            bond_length: decode(&case.bond_length)?,
            use_ring_templates: case.templates,
            ..Default::default()
        };
        let result =
            expansion::compute_initial(&case.state, &case.chiral_ranks, coords.as_ref(), options);
        assert_eq!(
            serde_json::to_value(&case.state)?,
            before,
            "{} immutable",
            case.name
        );
        let Some(expected) = case.expected else {
            if result.is_ok() {
                failures.push(format!("{}: native rejects but Rust accepts", case.name));
            } else {
                rejected += 1;
            }
            continue;
        };
        if expected
            .fragments
            .iter()
            .any(|f| malformed(f, case.state.graph.atoms.len()))
        {
            restrictions += 1;
            assert_eq!(
                expected.final_boundary,
                ["error", "error"],
                "{} full native boundary",
                case.name
            );
            assert!(
                result.is_err(),
                "{} native-invalid output must not escape",
                case.name
            );
            continue;
        }
        let result = match result {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{}: initial {e}", case.name));
                continue;
            }
        };
        accepted += 1;
        assert_eq!(
            result.depict_ranks, expected.depict_ranks,
            "{} ranks",
            case.name
        );
        if serde_json::to_value(&result.state)? != serde_json::to_value(&expected.state)? {
            failures.push(format!("{}: prepared chemical state differs", case.name));
        }
        if let Err(e) = compare(
            &mut audit,
            &result.fragments,
            &expected.fragments,
            &case.name,
        ) {
            failures.push(e.to_string());
        }
        let data = expected
            .state
            .hybridizations
            .iter()
            .enumerate()
            .map(|(i, &hybridization)| {
                Ok(AtomData {
                    hybridization,
                    cip_rank: expected
                        .state
                        .properties
                        .atoms
                        .get(i)
                        .context("properties")?
                        .cip_rank,
                    chiral_rank: *case.chiral_ranks.get(i).context("chiral ranks")?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let input = expansion::Input::new(
            &expected.state.graph,
            &expected.state.metadata,
            &expected.state.rings,
            &data,
        )?;
        let seeded = input.seed(coords.as_ref(), options)?;
        assert_eq!(
            seeded.non_embedded, expected.unembedded,
            "{} nonembedded",
            case.name
        );
        if let Err(e) = compare(
            &mut audit,
            &seeded.fragments,
            &expected.seeded,
            &format!("{}/seeds", case.name),
        ) {
            failures.push(e.to_string());
        }
        for (i, m) in expected.merges.iter().enumerate() {
            let source = m
                .before
                .iter()
                .map(fragment)
                .collect::<anyhow::Result<Vec<_>>>()?;
            let first = source.first().context("first merge")?;
            let second = source.get(1).context("second merge")?;
            let r = if m.common.is_empty() {
                input.merge_no_common(
                    first,
                    second,
                    usize::try_from(m.target)?,
                    usize::try_from(m.neighbor)?,
                    options.bond_length,
                )
            } else {
                input.merge_with_common(first, second, &m.common, options.bond_length)
            };
            match (r, &m.after) {
                (Ok(a), Some(e)) => {
                    merges += 1;
                    if !m.common.is_empty() {
                        assert_eq!(a.common, m.common_after, "{} extended common", case.name);
                    }
                    if let Err(e) = compare(
                        &mut audit,
                        &[a.fragment, a.incoming],
                        e,
                        &format!("{}/merge/{i}", case.name),
                    ) {
                        failures.push(e.to_string());
                    }
                }
                (Err(_), None) => {}
                (Err(_), Some(e))
                    if e.iter().any(|f| malformed(f, case.state.graph.atoms.len())) =>
                {
                    restrictions += 1
                }
                (r, _) => failures.push(format!("{}/merge/{i}: parity differs {r:?}", case.name)),
            }
        }
        for (i, s) in expected.steps.iter().enumerate() {
            let source = s
                .before
                .iter()
                .map(fragment)
                .collect::<anyhow::Result<Vec<_>>>()?;
            let result = input.expand(&source, s.master, &s.remaining, options.bond_length);
            match result {
                Ok(a) => {
                    steps += 1;
                    assert_eq!(a.non_embedded, s.remaining_after, "{} remaining", case.name);
                    if let Err(e) = compare(
                        &mut audit,
                        &a.fragments,
                        &s.after,
                        &format!("{}/expand/{i}", case.name),
                    ) {
                        failures.push(e.to_string());
                    }
                }
                Err(e) => failures.push(format!("{}/expand/{i}: {e}", case.name)),
            }
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "Expansion: {accepted} accepted, {rejected} native errors, {merges} merges, {steps} expansions, {restrictions} separate raw restrictions; {audit:?}; {} failures",
        failures.len()
    );
    for failure in failures.iter().take(30) {
        eprintln!("{failure}");
    }
    eprintln!(
        "Independent full native boundary: {public_success} successes, {public_errors} errors, {public_scalars} scalar bits checked by the C++ observer"
    );
    if source_adapter {
        assert_eq!(public_success + public_errors, 2 * 1877);
    }
    assert!(failures.is_empty());
    assert_eq!((accepted, rejected, restrictions), (1698, 117, 62));
    assert_eq!((merges, steps), (1478, 1791));

    if audit.exact {
        assert_eq!((audit.unequal, audit.projected), (0, 0));
    }
    Ok(())
}

fn chain(count: usize) -> reshiki::chemistry::graph::Graph {
    use reshiki::chemistry::graph::{Atom, Bond, Graph};
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Default::default()
            };
            count
        ],
        bonds: (1..count)
            .map(|id| Bond {
                a: id - 1,
                b: id,
                order: 1,
                aromatic: false,
            })
            .collect(),
    }
}
fn properties(count: usize) -> Vec<AtomData> {
    vec![
        AtomData {
            hybridization: reshiki::chemistry::electronic::Hybridization::Sp3,
            cip_rank: None,
            chiral_rank: None
        };
        count
    ]
}
fn no_rings() -> reshiki::chemistry::stereo::perception::RingCache {
    reshiki::chemistry::stereo::perception::RingCache {
        kind: reshiki::chemistry::stereo::perception::RingKind::Symmetric,
        atoms: Vec::new(),
    }
}

#[test]
fn large_chain_and_shared_work_budget_are_atomic() -> anyhow::Result<()> {
    use reshiki::chemistry::{depict::attachment, ranking::Metadata};
    let graph = chain(20_000);
    let metadata = Metadata::unspecified(&graph);
    let data = properties(graph.atoms.len());
    let rings = no_rings();
    let input = expansion::Input::new(&graph, &metadata, &rings, &data)?;
    let fragments = input.initial(None, expansion::Options::default())?;
    assert_eq!(fragments.len(), 1);
    let result = fragments.first().context("completed chain")?;
    assert_eq!(result.atoms.len(), graph.atoms.len());
    assert!(result.done && result.attachment_points.is_empty());
    assert!(
        result.atoms.values().all(|a| a.location.x.is_finite()
            && a.location.y.is_finite()
            && a.neighbors.is_empty())
    );
    let mut seed = attachment::Input::new(&graph, &data)?.single_atom(0)?;
    seed.done = true;
    let source = vec![seed];
    let before = serde_json::to_value(&source)?;
    let remaining = (1..graph.atoms.len()).collect::<Vec<_>>();
    let limited = input.with_work_limit(1000);
    assert!(limited.expand(&source, 0, &remaining, 1.5).is_err());
    assert_eq!(serde_json::to_value(&source)?, before);
    assert_eq!(remaining.len(), 19_999);
    Ok(())
}

#[test]
fn seeds_share_a_single_budget_and_validate_coordinate_ids() -> anyhow::Result<()> {
    use reshiki::chemistry::{graph::Bond, ranking::Metadata};
    let mut graph = chain(120);
    graph.bonds.clear();
    let mut rings = no_rings();
    for first in (0..120).step_by(6) {
        let ring = (first..first + 6).collect::<Vec<_>>();
        for id in first..first + 6 {
            graph.bonds.push(Bond {
                a: id,
                b: if id == first + 5 { first } else { id + 1 },
                order: 1,
                aromatic: false,
            });
        }
        rings.atoms.push(ring);
    }
    let metadata = Metadata::unspecified(&graph);
    let data = properties(graph.atoms.len());
    let options = expansion::Options {
        use_ring_templates: false,
        ..Default::default()
    };
    let input = expansion::Input::new(&graph, &metadata, &rings, &data)?;
    assert_eq!(input.seed(None, options)?.fragments.len(), 20);
    assert!(input.with_work_limit(4000).seed(None, options).is_err());
    let input = expansion::Input::new(&graph, &metadata, &rings, &data)?;
    for coordinates in [
        Coordinates::from([(120, Point::default())]),
        Coordinates::from([(0, Point { x: f64::NAN, y: 0. })]),
    ] {
        assert!(input.seed(Some(&coordinates), options).is_err());
    }
    Ok(())
}

#[test]
fn detached_merge_and_expand_failures_leave_sources_unchanged() -> anyhow::Result<()> {
    use reshiki::chemistry::{depict::seeds, ranking::Metadata};
    let graph = chain(4);
    let metadata = Metadata::unspecified(&graph);
    let data = properties(4);
    let rings = no_rings();
    let seed = seeds::Input::new(&graph, &metadata, &data, &rings)?;
    let first = seed.from_coordinates(&Coordinates::from([
        (0, Point { x: 0., y: 0. }),
        (1, Point { x: 1.5, y: 0. }),
    ]))?;
    let second = seed.from_coordinates(&Coordinates::from([
        (2, Point { x: 0., y: 0. }),
        (3, Point { x: 1.5, y: 0. }),
    ]))?;
    let input = expansion::Input::new(&graph, &metadata, &rings, &data)?;
    let before = serde_json::to_value((&first, &second))?;
    assert!(input.merge_no_common(&first, &second, 1, 2, 1.5).is_ok());
    assert!(
        input
            .merge_with_common(&first, &first, &[0, 0], 1.5)
            .is_err()
    );
    assert!(input.merge_no_common(&first, &second, 1, 4, 1.5).is_err());
    assert!(
        input
            .merge_no_common(&first, &second, 1, 2, f64::INFINITY)
            .is_err()
    );
    assert!(
        input
            .expand(std::slice::from_ref(&first), 0, &[2, 3], 1.5)
            .is_err()
    );
    assert!(
        input
            .expand(std::slice::from_ref(&first), 1, &[2, 2], 1.5)
            .is_err()
    );
    assert_eq!(serde_json::to_value((&first, &second))?, before);
    let mut invalid = first.clone();
    invalid
        .atoms
        .values_mut()
        .next()
        .context("first atom")?
        .location
        .x = f64::INFINITY;
    assert!(input.merge_no_common(&invalid, &second, 1, 2, 1.5).is_err());
    Ok(())
}

#[test]
fn aggregate_storage_is_checked_before_source_cloning() -> anyhow::Result<()> {
    use reshiki::chemistry::{depict::attachment, graph::Bond, ranking::Metadata};
    let mut graph = chain(1000);
    graph.bonds = (1..1000)
        .map(|b| Bond {
            a: 0,
            b,
            order: 1,
            aromatic: false,
        })
        .collect();
    let metadata = Metadata::unspecified(&graph);
    let data = properties(1000);
    let rings = no_rings();
    let mut seed = attachment::Input::new(&graph, &data)?.single_atom(0)?;
    seed.done = true;
    let fragments = vec![seed; 2100];
    let before_first = serde_json::to_value(fragments.first())?;
    let input = expansion::Input::new(&graph, &metadata, &rings, &data)?;
    assert!(matches!(
        input.expand(&fragments, 0, &[], 1.5),
        Err(expansion::Error::Limit)
    ));
    assert_eq!(serde_json::to_value(fragments.first())?, before_first);
    assert_eq!(fragments.len(), 2100);
    Ok(())
}

#[test]
fn chemical_annotation_dimensions_fail_before_copying() -> anyhow::Result<()> {
    use reshiki::chemistry::{
        graph::Valence,
        ranking::Metadata,
        stereo::perception::{Properties, RingCache},
    };
    let graph = chain(1);
    let mut state = State {
        metadata: Metadata::unspecified(&graph),
        properties: Properties::unspecified(&graph),
        graph,
        directions: Vec::new(),
        valences: vec![Valence {
            explicit_valence: 0,
            implicit_hydrogens: 4,
        }],
        conjugated: Vec::new(),
        hybridizations: vec![reshiki::chemistry::electronic::Hybridization::Sp3],
        rings: RingCache::default(),
    };
    let before = serde_json::to_value(&state)?;
    assert!(expansion::compute_initial(&state, &[], None, Default::default()).is_err());
    assert_eq!(serde_json::to_value(&state)?, before);
    state.properties.atoms.clear();
    assert!(matches!(
        expansion::compute_initial(&state, &[None], None, Default::default()),
        Err(expansion::Error::Invalid("chemical annotation count"))
    ));
    Ok(())
}
