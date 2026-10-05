#[path = "common/depict_linux.rs"]
mod depict_linux;

#[path = "common/depict_windows.rs"]
mod depict_windows;

use anyhow::Context;
use reshiki::chemistry::{
    depict::{attachment::AtomData, rings::Fragment, templates},
    stereo::perception::State,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
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
struct Trace {
    slot: i64,
    mapping: Vec<[usize; 2]>,
    fragment: Option<Native>,
}
#[derive(Deserialize)]
struct Expected {
    r#match: Option<Trace>,
    core: Vec<usize>,
    core_match: Option<Trace>,
    embedded: Option<Native>,
    later_compatible: Vec<i64>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    state: State,
    atom_data: Vec<AtomData>,
    selected: Vec<usize>,
    atoms: Vec<usize>,
    bond_length: String,
    expected: Expected,
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
#[test]
fn builtin_templates_match_direct_native_order_and_construction() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }));
    command.arg(root.join("tests/depict_templates_reference.py"));
    let windows_fixture = depict_windows::fixture("templates")?;
    let fixture = if let Some(fixture) = windows_fixture.as_deref() {
        fixture
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "depict-templates-macos-native.json.gz"
    } else {
        "depict-templates-linux-native.json.gz"
    };
    command.arg("--fixture").arg(
        root.join("tests/fixtures")
            .join(depict_linux::fixture(fixture)?),
    );
    let live = std::env::var_os("RESHIKI_DEPICT_TEMPLATES_ORACLE").is_some();
    if live {
        command
            .arg("--oracle")
            .arg(std::env::var_os("RESHIKI_DEPICT_TEMPLATES_ORACLE").context("native oracle")?)
            .arg("--rdkit-source")
            .arg(std::env::var_os("RESHIKI_RDKIT_SOURCE").context("native source")?)
            .arg("--replay");
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("fixture")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("header")??)?;
    assert_eq!(
        header["provenance"]["commit"],
        "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
    );
    let catalog: Value = serde_json::from_str(include_str!(
        "../src/chemistry/depict/templates/builtin.json"
    ))?;
    let queries = catalog["templates"].as_array().context("catalog")?;
    assert_eq!(queries.len(), 578);
    assert_eq!(
        catalog["source_sha256"],
        header["provenance"]["source_sha256"]["Code/GraphMol/Depictor/TemplateSmarts.h"]
    );
    let ordinal = |size: usize, slot: i64| -> anyhow::Result<usize> {
        queries
            .iter()
            .enumerate()
            .filter(|(_, q)| q["degrees"].as_array().is_some_and(|a| a.len() == size))
            .nth(usize::try_from(slot)?)
            .map(|(i, _)| i)
            .context("native catalog slot")
    };
    let mut audit = Audit {
        exact: live
            || std::env::var_os("RESHIKI_DEPICT_REQUIRE_EXACT").is_some()
            || cfg!(any(
                windows,
                all(target_os = "linux", target_arch = "x86_64"),
                all(target_os = "macos", target_arch = "aarch64")
            )),
        ..Default::default()
    };
    let (mut matched, mut unmatched, mut embedded, mut rejected, mut core) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut nonfinite_restrictions = 0;
    let mut first_mapping_witnesses = 0;
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        if case.expected.r#match.as_ref().is_some_and(|m| {
            case.expected
                .later_compatible
                .iter()
                .any(|&slot| m.slot < 0 || slot < m.slot)
        }) {
            first_mapping_witnesses += 1;
        }
        let before = serde_json::to_value((&case.state, &case.atom_data))?;
        let input = templates::Input::new(
            &case.state.graph,
            &case.state.metadata,
            &case.state.rings,
            &case.atom_data,
        )?;
        let result = input.match_system(&case.atoms);
        match (result, &case.expected.r#match) {
            (Ok(Some(actual)), Some(expected)) if expected.slot >= 0 => {
                matched += 1;
                assert_eq!(
                    actual.ordinal,
                    ordinal(case.atoms.len(), expected.slot)?,
                    "{} catalog",
                    case.name
                );
                let mapping = expected.mapping.iter().map(|p| p[1]).collect::<Vec<_>>();
                assert_eq!(actual.mapping, mapping, "{} mapping", case.name);
                if let Err(e) = audit.fragment(
                    &actual.fragment,
                    expected.fragment.as_ref().context("matched fragment")?,
                    &case.name,
                ) {
                    failures.push(e.to_string());
                }
            }
            (Ok(None), Some(expected)) if expected.slot < 0 => unmatched += 1,
            (Err(_), None) => rejected += 1,
            (result, _) => failures.push(format!("{} native match differs: {result:?}", case.name)),
        }
        let result = input.embed(&case.selected, decode(&case.bond_length)?);
        let native_nonfinite = case.expected.embedded.as_ref().is_some_and(|f| {
            f.bounds
                .iter()
                .chain(f.atoms.iter().flat_map(|a| a.values.iter()))
                .any(|v| decode(v).is_ok_and(|n| !n.is_finite()))
        });
        if native_nonfinite {
            assert!(
                matches!(
                    result,
                    Err(templates::Error::Rings(
                        reshiki::chemistry::depict::rings::Error::Geometry(
                            reshiki::chemistry::depict::geometry::Error::Numeric
                        )
                    ))
                ),
                "{} explicit nonfinite boundary",
                case.name
            );
            nonfinite_restrictions += 1;
            assert_eq!(
                serde_json::to_value((&case.state, &case.atom_data))?,
                before
            );
            continue;
        }
        match (result, &case.expected.embedded) {
            (Ok(actual), Some(expected)) => {
                embedded += 1;
                if let Err(e) = audit.fragment(&actual.fragment, expected, &case.name) {
                    failures.push(e.to_string());
                }
                let full = case.selected.len() > 1
                    || case
                        .selected
                        .first()
                        .and_then(|&i| case.state.rings.atoms.get(i))
                        .is_some_and(|r| r.len() > 8);
                let full_trace = case
                    .expected
                    .r#match
                    .as_ref()
                    .filter(|t| full && t.slot >= 0);
                let core_trace = case.expected.core_match.as_ref().filter(|t| t.slot >= 0);
                if let Some(t) = full_trace {
                    assert_eq!(
                        actual.template,
                        Some(ordinal(case.atoms.len(), t.slot)?),
                        "{} full",
                        case.name
                    );
                    assert!(!actual.core);
                } else if let Some(t) = core_trace {
                    let mut atoms = std::collections::BTreeSet::new();
                    for &i in &case.expected.core {
                        for &a in case
                            .state
                            .rings
                            .atoms
                            .get(*case.selected.get(i).context("core index")?)
                            .context("core ring")?
                        {
                            atoms.insert(a);
                        }
                    }
                    assert_eq!(
                        actual.template,
                        Some(ordinal(atoms.len(), t.slot)?),
                        "{} core",
                        case.name
                    );
                    assert!(actual.core);
                    core += 1;
                } else {
                    assert_eq!(actual.template, None, "{} none", case.name);
                    assert!(!actual.core);
                }
            }
            (Err(_), None) => rejected += 1,
            (result, _) => failures.push(format!("{} native embed differs: {result:?}", case.name)),
        }
        assert_eq!(
            serde_json::to_value((&case.state, &case.atom_data))?,
            before,
            "{} immutable",
            case.name
        );
    }
    assert!(child.wait()?.success());
    eprintln!(
        "Templates live={live}: {matched} matches, {unmatched} nonmatches, {embedded} embeddings ({core} core), {rejected} native exceptions; {audit:?}; {} failures",
        failures.len()
    );
    for failure in failures.iter().take(20) {
        eprintln!("{failure}");
    }
    assert!(failures.is_empty());
    assert_eq!(
        (matched, unmatched, embedded, core, rejected),
        (1712, 209, 1920, 6, 0)
    );
    assert_eq!(first_mapping_witnesses, 8);
    assert_eq!(audit.scalars, 695876);
    if audit.exact {
        assert_eq!(audit.unequal, 0);
        assert_eq!(audit.projected, 0);
    }
    eprintln!("Separate native-nonfinite geometry restrictions: {nonfinite_restrictions}");
    assert_eq!(nonfinite_restrictions, 1);
    assert!(
        core > 0,
        "Corpus must exercise actual core-template continuation"
    );
    Ok(())
}

#[test]
fn template_bounds_and_outside_degree_predicates_are_atomic() -> anyhow::Result<()> {
    use reshiki::chemistry::{
        electronic::Hybridization,
        graph::{Atom, Bond, Graph},
        ranking::Metadata,
        stereo::perception::{RingCache, RingKind},
    };
    let catalog: Value = serde_json::from_str(include_str!(
        "../src/chemistry/depict/templates/builtin.json"
    ))?;
    let query = catalog["templates"]
        .as_array()
        .and_then(|q| q.get(1))
        .context("degree regression template")?;
    let n = query["degrees"].as_array().context("degrees")?.len();
    let mut graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                no_implicit: true,
                ..Default::default()
            };
            n
        ],
        bonds: Vec::new(),
    };
    for edge in query["edges"].as_array().context("edges")? {
        graph.bonds.push(Bond {
            a: usize::try_from(edge[0].as_u64().context("endpoint")?)?,
            b: usize::try_from(edge[1].as_u64().context("endpoint")?)?,
            order: 1,
            aromatic: false,
        });
    }
    let metadata = Metadata::unspecified(&graph);
    let cache = RingCache {
        kind: RingKind::Symmetric,
        atoms: Vec::new(),
    };
    let data = vec![
        AtomData {
            hybridization: Hybridization::Unspecified,
            cip_rank: None,
            chiral_rank: None
        };
        n
    ];
    let before = serde_json::to_value(&graph)?;
    let ids = (0..n).collect::<Vec<_>>();
    let input = templates::Input::new(&graph, &metadata, &cache, &data)?;
    assert!(input.match_system(&ids)?.is_some());
    assert!(matches!(
        input.match_system(&[0, 0]),
        Err(templates::Error::Invalid(_))
    ));
    assert!(matches!(
        input.match_system(&[usize::MAX]),
        Err(templates::Error::Invalid(_))
    ));
    assert!(matches!(
        input.match_system(&vec![0; 100_001]),
        Err(templates::Error::Limit)
    ));
    assert!(input.embed(&[0], 1.5).is_err());
    let limited = templates::Input::new(&graph, &metadata, &cache, &data)?.with_work_limit(0);
    assert!(matches!(
        limited.match_system(&ids),
        Err(templates::Error::Limit)
    ));
    assert_eq!(serde_json::to_value(&graph)?, before);
    let connection = query["degrees"]
        .as_array()
        .context("degrees")?
        .iter()
        .position(|d| d.as_u64() == Some(2))
        .context("exact-degree query")?;
    graph.atoms.push(Atom {
        atomic_number: 8,
        ..Default::default()
    });
    graph.bonds.push(Bond {
        a: connection,
        b: n,
        order: 1,
        aromatic: false,
    });
    // Every internal vertex and edge is unchanged; the outside atom only raises
    // one full degree. The independent native corpus pins this exact contrast.
    let metadata = Metadata::unspecified(&graph);
    let mut data = data;
    data.push(AtomData {
        hybridization: Hybridization::Unspecified,
        cip_rank: None,
        chiral_rank: None,
    });
    assert!(
        templates::Input::new(&graph, &metadata, &cache, &data)?
            .match_system(&ids)?
            .is_none()
    );
    Ok(())
}
