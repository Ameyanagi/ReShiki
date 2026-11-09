//! One-off Phase B gates. The legacy fixture is deliberately external so the
//! equivalence check cannot accidentally compare a regenerated fixture to itself.
//!
//! Preserve it before updating the snapshot:
//! `git show origin/main:tests/fixtures/s2n/worker_snapshot.jsonl > /tmp/reshiki-10b-old-worker_snapshot.jsonl`
//! Run either ignored test with `--ignored --exact --nocapture` and its full path.
//! The size gate additionally needs `RESHIKI_NAZUCHEM_CHECKOUT=/path/to/nazuchem`.
use super::{Response, Snapshot, respond};
use crate::{chemistry, molecule_names, naming::worker};
use nazuchem_ir::{AxialWinding, Molecule, Ref, Stereo};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    time::Instant,
};

type State = chemistry::stereo::perception::State;

#[derive(Default)]
struct Sizes {
    requests: BTreeMap<String, (usize, String)>,
    responses: BTreeMap<String, (usize, String)>,
    errors: BTreeMap<String, usize>,
    count: usize,
}

impl Sizes {
    fn record(&mut self, request: &[u8], response: Option<&Response>) {
        let value: Value = serde_json::from_slice(request).expect("request JSON");
        let operation = value["operation"].as_str().expect("operation");
        let input = value["name"]
            .as_str()
            .or_else(|| value["smiles"].as_str())
            .expect("request input");
        let largest = self.requests.entry(operation.to_owned()).or_default();
        if request.len() > largest.0 {
            *largest = (request.len(), input.to_owned());
        }
        self.count += 1;
        match response {
            Some(Response::Bytes(bytes)) => {
                let largest = self.responses.entry(operation.to_owned()).or_default();
                if bytes.len() > largest.0 {
                    *largest = (bytes.len(), input.to_owned());
                }
            }
            Some(Response::Error { error }) => *self.errors.entry(error.clone()).or_default() += 1,
            None => {}
        }
    }

    fn finish(&self, label: &str) {
        eprintln!("{label}: {} requests", self.count);
        for (operation, (bytes, input)) in &self.requests {
            eprintln!(
                "  largest {operation} request: {bytes}/{} bytes ({:.2}%): {input}",
                worker::WIRE_LIMIT,
                100.0 * *bytes as f64 / worker::WIRE_LIMIT as f64
            );
            assert!(*bytes <= worker::WIRE_LIMIT, "request exceeds WIRE_LIMIT");
        }
        for (operation, (bytes, input)) in &self.responses {
            eprintln!(
                "  largest {operation} response: {bytes}/{} bytes ({:.2}%): {input}",
                worker::OUTPUT_LIMIT,
                100.0 * *bytes as f64 / worker::OUTPUT_LIMIT as f64
            );
            assert!(
                *bytes <= worker::OUTPUT_LIMIT,
                "response exceeds OUTPUT_LIMIT"
            );
        }
        eprintln!("  explicit worker error counts: {:?}", self.errors);
        assert!(
            !self.errors.keys().any(|error| error.contains("wire limit")),
            "a wire limit hid the serialized size of a response"
        );
    }
}

fn baseline() -> PathBuf {
    std::env::var_os("RESHIKI_OLD_S2N_SNAPSHOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/reshiki-10b-old-worker_snapshot.jsonl"))
}

fn current_request(request: worker::Request) -> Vec<u8> {
    let mut request: Value = serde_json::to_value(request).expect("frozen request JSON");
    request["protocol"] = worker::PROTOCOL.into();
    // Serialize the real request type to measure the production field order.
    let request: worker::Request = serde_json::from_value(request).expect("current request");
    serde_json::to_vec(&request).expect("current request bytes")
}

fn bare(text: &str) -> &str {
    text.split_once(" |").map_or(text, |(smiles, _)| smiles)
}

/// IR identity is the legacy semantic writer order, followed by protected H
/// vertices (pinned nazuchem-n2s/src/ir.rs::to_ir_with_atom_mapping). Validate
/// that identity map chemically; canonical traversal can exchange symmetric
/// atoms and therefore cannot establish the frozen parent-locant association.
fn validate_identity_map(old: &State, new: &State, surviving: &[usize]) -> Result<(), String> {
    let count = old.graph.atoms.len();
    if surviving != (0..count).collect::<Vec<_>>() || new.graph.atoms.len() != count {
        return Err(format!(
            "native H projection changed legacy atom identities: {surviving:?}; expected 0..{count}"
        ));
    }
    for (index, (old_atom, new_atom)) in old.graph.atoms.iter().zip(&new.graph.atoms).enumerate() {
        let facts = |atom: &chemistry::graph::Atom, state: &State| {
            (
                atom.atomic_number,
                atom.isotope,
                atom.charge,
                atom.radical_electrons,
                u32::from(atom.explicit_hydrogens) + state.valences[index].implicit_hydrogens,
            )
        };
        if facts(old_atom, old) != facts(new_atom, new) {
            return Err(format!(
                "chemical identity at frozen atom {index} changed: {:?} != {:?}",
                facts(old_atom, old),
                facts(new_atom, new)
            ));
        }
    }
    let edges = |state: &State| {
        let mut edges: Vec<_> = state
            .graph
            .bonds
            .iter()
            .map(|bond| {
                (
                    bond.a.min(bond.b),
                    bond.a.max(bond.b),
                    bond.order,
                    bond.aromatic,
                )
            })
            .collect();
        edges.sort_unstable();
        edges
    };
    if edges(old) != edges(new) {
        return Err("chemical edges in frozen atom identity order changed".into());
    }
    Ok(())
}

/// Semantic CX layers are compared after graph mapping, including cases whose
/// canonical molecular identity is unsupported. Unknown layers fail closed.
fn layers(text: &str, order: &[u32]) -> Result<Value, String> {
    let Some((_, extension)) = text.split_once(" |") else {
        return Ok(Value::Array(vec![]));
    };
    let extension = extension
        .strip_suffix('|')
        .ok_or("unterminated CX extension")?;
    let (mut start, mut labels) = (0, false);
    let mut segments = vec![];
    for (index, byte) in extension.bytes().enumerate() {
        if byte == b'$' {
            labels = !labels;
        } else if byte == b','
            && !labels
            && extension
                .as_bytes()
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_alphabetic() || matches!(*next, b'$' | b'&'))
        {
            segments.push(&extension[start..index]);
            start = index + 1;
        }
    }
    segments.push(&extension[start..]);
    let atom = |text: &str| -> Result<u32, String> {
        let index = text
            .parse::<usize>()
            .map_err(|_| format!("invalid CX index {text}"))?;
        order
            .get(index)
            .copied()
            .ok_or_else(|| format!("CX index {index} outside atoms"))
    };
    let atoms = |text: &str, separator| -> Result<Vec<u32>, String> {
        let mut members = text
            .split(separator)
            .map(&atom)
            .collect::<Result<Vec<_>, _>>()?;
        members.sort_unstable();
        Ok(members)
    };
    let mut result = vec![];
    for segment in segments {
        if segment == "r" {
            result.push(serde_json::json!(["relative"]));
        } else if let Some(body) = segment.strip_prefix("m:") {
            for position in body.split(',') {
                let (source, targets) = position
                    .split_once(':')
                    .ok_or("invalid position variation")?;
                result.push(serde_json::json!([
                    "position",
                    atom(source)?,
                    atoms(targets, '.')?
                ]));
            }
        } else if let Some(body) = segment.strip_prefix("Sg:n:") {
            let (members, details) = body.split_once(':').ok_or("invalid polymer unit")?;
            result.push(serde_json::json!([
                "polymer",
                atoms(members, ',')?,
                details
            ]));
        } else if let Some((group, members)) = segment.split_once(':') {
            let kind = group.chars().next().ok_or("empty stereo group")?;
            if !matches!(kind, 'a' | 'o' | '&')
                || (kind == 'a' && group != "a")
                || (kind != 'a' && group[1..].parse::<u32>().is_err())
            {
                return Err(format!("unsupported semantic CX layer {segment}"));
            }
            result.push(serde_json::json!([
                "stereo",
                kind.to_string(),
                atoms(members, ',')?
            ]));
        } else {
            // The frozen migration corpus currently has no atom labels. Never
            // silently accept a future label format without testing its escapes.
            return Err(format!("unsupported semantic CX layer {segment}"));
        }
    }
    result.sort_by_key(Value::to_string);
    Ok(Value::Array(result))
}

fn legacy_state(old: &Value) -> Result<State, String> {
    let smiles = old["cxsmiles"]
        .as_str()
        .ok_or("no legacy molecular structure")?;
    let mut state = chemistry::smiles::read(smiles)
        .map_err(|error| error.to_string())?
        .prepared
        .state;
    for axis in old["axes"].as_array().ok_or("legacy axes")? {
        let pair = |field: &str| -> Result<[usize; 2], String> {
            serde_json::from_value(axis[field].clone()).map_err(|error| error.to_string())
        };
        let [a, b] = pair("atoms")?;
        let mut refs = pair("refs")?;
        let index = state
            .graph
            .bonds
            .iter()
            .position(|bond| (bond.a, bond.b) == (a, b) || (bond.a, bond.b) == (b, a))
            .ok_or("legacy axis missing bond")?;
        let bond = &state.graph.bonds[index];
        if bond.a != a {
            refs.reverse();
        }
        let mut clockwise = axis["clockwise"].as_bool().ok_or("axis winding")?;
        let mut carriers = vec![];
        for ((atom, other), reference) in [(bond.a, bond.b), (bond.b, bond.a)].into_iter().zip(refs)
        {
            let neighbors: Vec<_> = state
                .graph
                .bonds
                .iter()
                .filter_map(|bond| match (bond.a, bond.b) {
                    (x, y) | (y, x) if x == atom && y != other => Some(y),
                    _ => None,
                })
                .collect();
            let carrier = *neighbors.iter().min().ok_or("axis missing carrier")?;
            if neighbors.len() != 2 || !neighbors.contains(&reference) {
                return Err("invalid frozen axis references".into());
            }
            clockwise ^= carrier != reference;
            carriers.push(carrier);
        }
        state.metadata.bonds[index].stereo = if clockwise { 6 } else { 7 };
        state.metadata.bonds[index].stereo_atoms = carriers;
        let labelled = chemistry::stereo::cip::label::assign(&state, &Default::default())
            .map_err(|error| error.to_string())?;
        if labelled.state.properties.bond_codes[index].as_deref() != axis["descriptor"].as_str() {
            return Err("frozen axis descriptor disagrees with ReShiki CIP".into());
        }
    }
    Ok(state)
}

#[derive(Default, Debug)]
struct Counts {
    generate: usize,
    parse_verify: usize,
    no_structure: usize,
    canonical: usize,
    unsupported_canonical: BTreeMap<String, usize>,
    axes: usize,
}

fn compare_structure(old: &Value, molecule: &Molecule, counts: &mut Counts) -> Result<(), String> {
    molecule.validate().map_err(|error| error.to_string())?;
    let old_smiles = old["cxsmiles"]
        .as_str()
        .ok_or("legacy molecule absent but IR present")?;
    let old_locants: Vec<Option<String>> =
        serde_json::from_value(old["locants"].clone()).map_err(|error| error.to_string())?;
    // Axes have no portable CXSMILES representation. Compare the typed unit
    // explicitly and remove it only from the compatibility writer projection.
    let mut projection = molecule.clone();
    projection
        .stereo
        .retain(|stereo| !matches!(stereo, Stereo::Atropisomer { .. }));
    if projection.stereo.len() != molecule.stereo.len() && !projection.stereo_groups.is_empty() {
        return Err("axis removal needs an explicit stereo-group index map".into());
    }
    let (current_smiles, order) = projection
        .semantic_cxsmiles_with_atom_order_for_tests()
        .map_err(|error| error.to_string())?;
    let old_state = legacy_state(old)?;
    let prepared = molecule_names::from_ir(molecule)?;
    let (prepared, surviving) = molecule_names::name_import_projection(&prepared)?;
    validate_identity_map(&old_state, &prepared.state, &surviving)?;
    let ir_to_old: Vec<_> = (0..molecule.atoms.len())
        .map(|index| (index < old_locants.len()).then_some(index as u32))
        .collect();
    let displayed = molecule.displayed_locants();
    let actual_locants: Vec<_> = surviving
        .iter()
        .map(|&index| displayed[index].clone())
        .collect();
    if old_locants != actual_locants {
        return Err(format!(
            "parent-only first locants changed: {old_locants:?} != {actual_locants:?}"
        ));
    }
    let identity: Vec<_> = (0..old_locants.len() as u32).collect();
    // The convenience writer's traversal maps back to the validated stable IR
    // identities, including any extra H it keeps for its own export metadata.
    if layers(old_smiles, &identity)? != layers(&current_smiles, &order)? {
        return Err(format!(
            "semantic CX layers changed: {old_smiles} != {current_smiles}"
        ));
    }
    let mut actual_axes = vec![];
    let mapped = |reference: Ref| -> Result<u32, String> {
        match reference {
            Ref::Atom(atom) => ir_to_old
                .get(atom as usize)
                .copied()
                .flatten()
                .ok_or("unmapped axial atom".into()),
            _ => Err("legacy axes cannot represent virtual axial references".into()),
        }
    };
    for stereo in &molecule.stereo {
        if let Stereo::Atropisomer {
            bond,
            refs,
            winding,
        } = stereo
        {
            let bond = &molecule.bonds[*bond as usize];
            let atoms = [
                mapped(Ref::Atom(bond.atoms[0]))?,
                mapped(Ref::Atom(bond.atoms[1]))?,
            ];
            let refs = [mapped(refs[0])?, mapped(refs[2])?];
            let clockwise = match winding {
                AxialWinding::Clockwise => true,
                AxialWinding::Anticlockwise => false,
                _ => return Err("unrepresentable axial winding".into()),
            };
            actual_axes.push(serde_json::json!({"atoms":atoms,"refs":refs,"clockwise":clockwise}));
        }
    }
    let old_axes = old["axes"].as_array().ok_or("legacy axes array")?;
    if actual_axes.len() != old_axes.len() {
        return Err("axis count changed".into());
    }
    for (actual, old) in actual_axes.iter().zip(old_axes) {
        let mut expected = old.clone();
        expected
            .as_object_mut()
            .ok_or("legacy axis object")?
            .remove("descriptor");
        if actual != &expected {
            return Err(format!(
                "mapped axis references/winding changed: {expected} != {actual}"
            ));
        }
    }
    counts.axes += actual_axes.len();
    let expected = crate::naming::canonical_state(&old_state);
    let actual = crate::naming::canonical_state(&prepared.state);
    match (expected, actual) {
        (Ok(expected), Ok(actual)) if expected == actual => counts.canonical += 1,
        (Err(expected), Err(actual)) if expected == actual => {
            // Enhanced groups are an intentional canonical-identity refusal,
            // not a skipped structure. Their mapped memberships were compared
            // above; also require equal underlying specified stereochemistry.
            let mut old_absolute = old_state.clone();
            old_absolute.metadata.groups.clear();
            let mut new_absolute = prepared.state.clone();
            new_absolute.metadata.groups.clear();
            match (
                crate::naming::canonical_state(&old_absolute),
                crate::naming::canonical_state(&new_absolute),
            ) {
                (Ok(old), Ok(new)) if old == new => {}
                (Err(old), Err(new)) if old == new && bare(old_smiles) == bare(&current_smiles) => {
                }
                (old, new) => {
                    return Err(format!(
                        "underlying identity of rejected semantic structure changed: {old:?} != {new:?}"
                    ));
                }
            }
            *counts
                .unsupported_canonical
                .entry(format!("legacy: {expected}; IR: {actual}"))
                .or_default() += 1;
        }
        (expected, actual) => {
            return Err(format!(
                "ReShiki canonical identity changed: {expected:?} != {actual:?}"
            ));
        }
    }
    Ok(())
}

fn compare(old: &Response, actual: &Response, counts: &mut Counts) -> Result<(), String> {
    let (Response::Bytes(old), Response::Bytes(actual)) = (old, actual) else {
        return (old == actual)
            .then_some(())
            .ok_or_else(|| format!("worker error changed: {old:?} != {actual:?}"));
    };
    let mut frozen: Value = serde_json::from_str(old).map_err(|error| error.to_string())?;
    let mut current: Value = serde_json::from_str(actual).map_err(|error| error.to_string())?;
    if frozen["operation"] == "generate" {
        counts.generate += 1;
        let protocol = frozen["protocol"].as_u64().ok_or("frozen protocol")?;
        let adjusted = old.replacen(
            &format!("\"protocol\":{protocol}"),
            &format!("\"protocol\":{}", worker::PROTOCOL),
            1,
        );
        return (adjusted == *actual)
            .then_some(())
            .ok_or("Generate bytes changed beyond protocol".into());
    }
    counts.parse_verify += 1;
    frozen["protocol"] = worker::PROTOCOL.into();
    let molecular = current
        .as_object_mut()
        .ok_or("current response object")?
        .remove("molecule")
        .ok_or("IR response missing molecule field")?;
    let molecule: Option<Molecule> =
        serde_json::from_value(molecular).map_err(|error| error.to_string())?;
    if let Some(molecule) = molecule {
        compare_structure(&frozen, &molecule, counts)?;
    } else {
        counts.no_structure += 1;
        if !frozen["cxsmiles"].is_null()
            || frozen["locants"] != serde_json::json!([])
            || frozen["axes"] != serde_json::json!([])
        {
            return Err("molecular structure was dropped".into());
        }
    }
    let object = frozen.as_object_mut().ok_or("legacy response object")?;
    object.remove("cxsmiles");
    object.remove("locants");
    object.remove("axes");
    if frozen != current {
        return Err(format!(
            "status/message/warnings/metadata changed: {frozen} != {current}"
        ));
    }
    Ok(())
}

fn audit_snapshot(equivalence: bool, selected: Option<&BTreeSet<usize>>) -> Sizes {
    let start = Instant::now();
    let text = std::fs::read_to_string(baseline())
        .expect("preserve the OLD origin/main snapshot before running this migration gate");
    let mut counts = Counts::default();
    let mut sizes = Sizes::default();
    let mut differences = vec![];
    for (index, line) in text.lines().enumerate() {
        if selected.is_some_and(|selected| !selected.contains(&(index + 1))) {
            continue;
        }
        let snapshot: Snapshot = serde_json::from_str(line)
            .unwrap_or_else(|error| panic!("old fixture line {}: {error}", index + 1));
        let request_only =
            !equivalence && matches!(&snapshot.request, worker::Request::Generate { .. });
        let request = current_request(snapshot.request);
        if request_only {
            // Generation is already checked by the equivalence gate. Size
            // measurement needs every request, but only Parse/Verify replies.
            sizes.record(&request, None);
            continue;
        }
        let actual = respond(&request);
        sizes.record(&request, Some(&actual));
        if equivalence && let Err(error) = compare(&snapshot.response, &actual, &mut counts) {
            differences.push(format!(
                "line {} request {}: {error}",
                index + 1,
                String::from_utf8_lossy(&request)
            ));
        }
        if (index + 1) % 1000 == 0 {
            eprintln!(
                "snapshot: {} requests checked in {:.1?}",
                index + 1,
                start.elapsed()
            );
        }
    }
    sizes.finish("legacy snapshot, current IR worker");
    if let Some(selected) = selected {
        assert_eq!(
            sizes.count,
            selected.len(),
            "selected fixture lines must all exist"
        );
        eprintln!(
            "FOCUSED audit only; selected {} fixture rows",
            selected.len()
        );
    }
    if equivalence {
        eprintln!(
            "equivalence coverage: {counts:?}; {} differences",
            differences.len()
        );
        for difference in &differences {
            eprintln!("{difference}");
        }
        assert!(
            differences.is_empty(),
            "{} worker equivalence differences (all printed above)",
            differences.len()
        );
    }
    sizes
}

#[test]
#[ignore = "one-off legacy CXSMILES to IR migration gate; needs preserved origin/main fixture"]
fn s2n_worker_ir_equivalence() {
    audit_snapshot(true, None);
}

#[test]
#[ignore = "focused migration debugging; requires explicit RESHIKI_S2N_EQUIVALENCE_LINES"]
fn s2n_worker_ir_selected_equivalence() {
    let selected: BTreeSet<usize> = std::env::var("RESHIKI_S2N_EQUIVALENCE_LINES")
        .expect("set comma-separated 1-based legacy fixture line numbers")
        .split(',')
        .map(|line| line.parse().expect("fixture line number"))
        .collect();
    assert!(!selected.is_empty());
    audit_snapshot(true, Some(&selected));
}

fn oracle_names(checkout: &Path) -> BTreeSet<String> {
    let fixtures = checkout.join("crates/nazuchem-n2s/tests/fixtures");
    let mut names = BTreeSet::new();
    // These fixtures contain names. Graph-resource, suffix-rule and
    // fused-building inputs contain SMILES/XML operations, not name requests.
    for file in [
        "frontend-inputs.txt",
        "configuration-inputs.txt",
        "unambiguous.txt",
        "ambiguous.txt",
        "uninterpretable.txt",
    ] {
        let text = std::fs::read_to_string(fixtures.join(file))
            .unwrap_or_else(|error| panic!("oracle {file}: {error}"));
        for line in text.lines() {
            // Tabs can be intentional name whitespace (the lexical oracle
            // includes sodium + mixed Unicode whitespace + chloride).
            names.insert(line.to_owned());
        }
    }
    let text = std::fs::read_to_string(fixtures.join("chalcogenAcidLocants.txt"))
        .expect("chalcogen acid name/locant fixture");
    for line in text.lines() {
        names.insert(
            line.split_once('\t')
                .expect("name/locants TSV")
                .0
                .to_owned(),
        );
    }
    for file in ["structure-inputs.tsv", "parse-tree-inputs.tsv"] {
        let text = std::fs::read_to_string(fixtures.join(file))
            .unwrap_or_else(|error| panic!("oracle {file}: {error}"));
        for line in text.lines() {
            names.insert(line.split_once('\t').expect("family/name TSV").1.to_owned());
        }
    }
    let text = std::fs::read_to_string(fixtures.join("fused-numbering-inputs.tsv"))
        .expect("numbering names");
    for line in text.lines() {
        names.insert(line.split('\t').next().unwrap().to_owned());
    }
    for entry in
        std::fs::read_dir(fixtures.join("upstream-inchi")).expect("upstream InChI directory")
    {
        let entry = entry.expect("InChI fixture entry");
        let text = std::fs::read_to_string(entry.path()).expect("upstream InChI names");
        for line in text.lines() {
            names.insert(line.split('\t').next().unwrap().to_owned());
        }
    }
    // Include input/name fields from every frozen JSON oracle too. Some are
    // repeated configurations; the set makes coverage reproducible and unique.
    for entry in std::fs::read_dir(&fixtures).expect("oracle fixture directory") {
        let entry = entry.expect("oracle fixture entry");
        let path = entry.path();
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        // Here "input" is a hand-written scenario identifier for an XML graph
        // operation, not a chemical name consumed by the parser.
        if filename == "fused-building-golden.jsonl" {
            continue;
        }
        let reader: Box<dyn Read> = if filename.ends_with(".jsonl.gz") {
            Box::new(flate2::read::GzDecoder::new(
                std::fs::File::open(&path).expect("gzip oracle"),
            ))
        } else if filename.ends_with(".jsonl") {
            Box::new(std::fs::File::open(&path).expect("JSON oracle"))
        } else {
            continue;
        };
        for line in BufReader::new(reader).lines() {
            let row: Value =
                serde_json::from_str(&line.expect("oracle JSON line")).expect("oracle JSON");
            if let Some(input) = row["input"].as_str().or_else(|| row["name"].as_str()) {
                names.insert(input.to_owned());
            }
        }
    }
    names
}

#[test]
#[ignore = "full OPSIN oracle wire measurement; set RESHIKI_NAZUCHEM_CHECKOUT"]
fn s2n_worker_ir_wire_sizes() {
    audit_snapshot(false, None);
    let checkout = std::env::var_os("RESHIKI_NAZUCHEM_CHECKOUT")
        .expect("set RESHIKI_NAZUCHEM_CHECKOUT to the pinned nazuchem dependency checkout");
    let names = oracle_names(Path::new(&checkout));
    let mut sizes = Sizes::default();
    let start = Instant::now();
    eprintln!(
        "measuring Parse + Verify for {} distinct OPSIN oracle names",
        names.len()
    );
    for (index, name) in names.into_iter().enumerate() {
        for request in [
            worker::Request::Parse {
                protocol: worker::PROTOCOL,
                name: name.clone(),
            },
            worker::Request::Verify {
                protocol: worker::PROTOCOL,
                name: name.clone(),
            },
        ] {
            let request = serde_json::to_vec(&request).expect("oracle request");
            let response = respond(&request);
            sizes.record(&request, Some(&response));
        }
        if (index + 1) % 1000 == 0 {
            eprintln!(
                "oracle: {} names measured in {:.1?}",
                index + 1,
                start.elapsed()
            );
        }
    }
    sizes.finish("all OPSIN oracle names");
}
