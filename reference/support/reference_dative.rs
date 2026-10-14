//! Narrow reference expectations for #300 (`8fbf166c`).
//!
//! `docs/fragment-joining.md` documents that dative B/E are donor/acceptor,
//! independently of projection paint. These adapters retain the original
//! capture and check its old values before changing only dative endpoints,
//! direction/CFG, End paint/position, and editable-bond projection. They never
//! use a current parser or importer to manufacture the expected result.
//!
//! The separately approved two-carbon End-paint predicate also derives cached
//! valences and hydrogen labels from source-only pinned RDKit. Every old value
//! is checked before adaptation; all other fields remain exactly compared.

// Each integration target uses only its own stage-specific entry points.
#![allow(dead_code)]

use anyhow::{Context, bail, ensure};
use reshiki::chemistry::{ELEMENTS, cdxml};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Clone, Debug)]
struct SourceAtom {
    id: Option<String>,
    element: String,
    position: String,
}

#[derive(Clone, Debug)]
struct SourceBond {
    order: String,
    display: String,
    secondary: Option<String>,
    begin: Option<String>,
    end: Option<String>,
    position: String,
}

impl SourceBond {
    fn dative(&self) -> bool {
        self.order == "dative"
    }

    fn projected(&self) -> bool {
        self.dative()
            && self.secondary.as_deref() != Some("DottedHydrogen")
            && !matches!(self.display.as_str(), "Solid" | "Dash")
    }

    fn paint(&self) -> anyhow::Result<&'static str> {
        Ok(match self.display.as_str() {
            "Solid" => "plain",
            "Dash" => "dashed",
            "Dot" => "dotted",
            "Bold" => "bold",
            "Hash" => "hashed",
            "WedgeBegin" | "WedgeEnd" => "wedge",
            "WedgedHashBegin" | "WedgedHashEnd" => "hash",
            "HollowWedgeBegin" | "HollowWedgeEnd" => "hollow_wedge",
            "Wavy" => "wavy",
            other => bail!("Unsupported legacy dative paint {other:?}"),
        })
    }

    fn end_paint(&self) -> Option<&'static str> {
        match self.display.as_str() {
            "WedgeEnd" => Some("wedge_end"),
            "WedgedHashEnd" => Some("hash_end"),
            "HollowWedgeEnd" => Some("hollow_wedge_end"),
            _ => None,
        }
    }

    // Independently reproduce the original source normalization, only for
    // explicit dative orders. In particular Bold stays Bold for this order.
    fn old_chemical(&self, normalized: bool) -> (&'static str, Option<u8>, bool) {
        match self.display.as_str() {
            "WedgeBegin" => ("wedge", Some(1), false),
            "WedgeEnd" => ("wedge", Some(1), true),
            "WedgedHashBegin" => ("hash", Some(3), false),
            "WedgedHashEnd" => ("hash", Some(3), true),
            "HollowWedgeBegin" if normalized => ("wedge", Some(1), false),
            "HollowWedgeEnd" if normalized => ("wedge", Some(1), true),
            "Hash" if normalized => ("hash", Some(3), false),
            _ => ("none", None, false),
        }
    }
}

#[derive(Clone, Debug)]
struct SourceFragment {
    id: Option<String>,
    atoms: Vec<SourceAtom>,
    bonds: Vec<SourceBond>,
}

impl SourceFragment {
    fn chemical_indices(&self) -> anyhow::Result<HashMap<u64, usize>> {
        let mut indices = HashMap::new();
        for (index, atom) in self.atoms.iter().enumerate() {
            let id: u64 = atom
                .id
                .as_deref()
                .context("Dative source atom has no ID")?
                .parse()
                .context("Dative source atom ID is not an unsigned integer")?;
            ensure!(
                indices.insert(id, index).is_none(),
                "Duplicate dative source atom ID"
            );
        }
        Ok(indices)
    }

    fn chemical_pair(
        &self,
        bond: &SourceBond,
        indices: &HashMap<u64, usize>,
    ) -> anyhow::Result<(usize, usize)> {
        let endpoint = |text: Option<&str>| -> anyhow::Result<usize> {
            let id: u64 = text
                .context("Dative source bond endpoint is absent")?
                .parse()
                .context("Dative source bond endpoint is not an unsigned integer")?;
            indices
                .get(&id)
                .copied()
                .context("Dative source bond endpoint has no atom")
        };
        let pair = (
            endpoint(bond.begin.as_deref())?,
            endpoint(bond.end.as_deref())?,
        );
        ensure!(pair.0 != pair.1, "Self dative source bond");
        Ok(pair)
    }
}

struct Source {
    fragments: Vec<SourceFragment>,
    bond_length: String,
}

impl Source {
    fn parse(text: &str) -> anyhow::Result<Self> {
        let xml = roxmltree::Document::parse_with_options(
            text,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                nodes_limit: 400_000,
            },
        )?;
        let named = |node: roxmltree::Node<'_, '_>, name| {
            node.is_element()
                && node.tag_name().namespace().is_none()
                && node.tag_name().name() == name
        };
        let fragments = xml
            .descendants()
            .filter(|node| named(*node, "fragment"))
            .map(|fragment| SourceFragment {
                id: fragment.attribute("id").map(str::to_owned),
                atoms: fragment
                    .children()
                    .filter(|node| named(*node, "n"))
                    .map(|node| SourceAtom {
                        id: node.attribute("id").map(str::to_owned),
                        element: node.attribute("Element").unwrap_or("6").to_owned(),
                        position: node.attribute("p").unwrap_or("").to_owned(),
                    })
                    .collect(),
                bonds: fragment
                    .children()
                    .filter(|node| named(*node, "b"))
                    .map(|node| SourceBond {
                        order: node.attribute("Order").unwrap_or("1").to_owned(),
                        display: node.attribute("Display").unwrap_or("Solid").to_owned(),
                        secondary: node.attribute("Display2").map(str::to_owned),
                        begin: node.attribute("B").map(str::to_owned),
                        end: node.attribute("E").map(str::to_owned),
                        position: node
                            .attribute("DoublePosition")
                            .unwrap_or("auto")
                            .to_lowercase(),
                    })
                    .collect(),
            })
            .collect();
        Ok(Self {
            fragments,
            bond_length: xml
                .root_element()
                .attribute("BondLength")
                .unwrap_or("14.4")
                .to_owned(),
        })
    }

    fn has_dative(&self) -> bool {
        self.fragments
            .iter()
            .flat_map(|fragment| &fragment.bonds)
            .any(SourceBond::dative)
    }

    fn has_projection(&self) -> bool {
        self.fragments
            .iter()
            .flat_map(|fragment| &fragment.bonds)
            .any(SourceBond::projected)
    }

    fn fragment(&self, id: i64, numeric: bool) -> anyhow::Result<&SourceFragment> {
        let mut matches = self.fragments.iter().filter(|fragment| {
            fragment.id.as_deref().is_some_and(|source| {
                if numeric {
                    source.parse::<i64>().ok() == Some(id)
                } else {
                    source == id.to_string()
                }
            })
        });
        // Appearance follows the original dictionary's last-fragment-wins
        // rule. Chemical parsing instead rejects ambiguous source identity.
        if numeric {
            let fragment = matches.next().context("Missing dative source fragment")?;
            ensure!(matches.next().is_none(), "Ambiguous dative source fragment");
            Ok(fragment)
        } else {
            matches
                .next_back()
                .context("Missing dative appearance fragment")
        }
    }
}

fn array<'a>(value: &'a Value, name: &str) -> anyhow::Result<&'a [Value]> {
    value
        .get(name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .with_context(|| format!("Missing original {name} array"))
}

fn assert_field(value: &Value, field: &str, old: Value) -> anyhow::Result<()> {
    ensure!(
        value.get(field) == Some(&old),
        "Legacy dative {field} is {:?}, source requires {old}",
        value.get(field)
    );
    Ok(())
}

fn update_field(value: &mut Value, field: &str, old: Value, new: Value) -> anyhow::Result<()> {
    assert_field(value, field, old)?;
    if value[field] != new {
        value[field] = new;
    }
    Ok(())
}

fn point(text: &str) -> anyhow::Result<[f64; 2]> {
    let coordinates = text
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()?;
    ensure!(
        coordinates.len() == 2 && coordinates.iter().all(|value| value.is_finite()),
        "Dative appearance source must have two finite coordinates"
    );
    Ok([coordinates[0], coordinates[1]])
}

fn reverse_position(position: &str) -> &str {
    match position {
        "left" => "right",
        "right" => "left",
        other => other,
    }
}

fn adapt_paint(
    captured: &mut Value,
    source: &SourceBond,
    begin: u64,
    end: u64,
    editable: bool,
) -> anyhow::Result<()> {
    if !source.projected() {
        return Ok(());
    }
    ensure!(begin != end, "Self dative appearance source bond");
    let paint = source.paint()?;
    let narrowed_at_end = source.end_paint();
    let (old_begin, old_end) = if narrowed_at_end.is_some() {
        (end, begin)
    } else {
        (begin, end)
    };
    assert_field(captured, "order", json!(5))?;
    update_field(captured, "a", json!(old_begin), json!(begin))?;
    update_field(captured, "b", json!(old_end), json!(end))?;
    update_field(
        captured,
        "display",
        json!(paint),
        json!(narrowed_at_end.unwrap_or(paint)),
    )?;
    ensure!(
        matches!(
            source.position.as_str(),
            "auto" | "left" | "right" | "center"
        ),
        "Unsupported source dative double position"
    );
    let old_position = if narrowed_at_end.is_some() {
        reverse_position(&source.position)
    } else {
        &source.position
    };
    update_field(
        captured,
        "double_position",
        json!(old_position),
        json!(source.position),
    )?;
    if editable {
        // The worker's raw transport omitted default-valued projection;
        // serde's editable Bond capture serializes the default explicitly.
        ensure!(
            captured
                .get("projection")
                .is_none_or(|value| value == &json!(false)),
            "Original dative projection was already enabled"
        );
        captured["projection"] = json!(true);
    }
    Ok(())
}

fn appearance(
    source: &Source,
    mut legacy: Value,
    parts: &[Value],
    scale: f64,
    editable: bool,
) -> anyhow::Result<Value> {
    if !source.has_projection() {
        return Ok(legacy);
    }
    ensure!(scale.is_finite(), "Nonfinite dative association scale");
    let captured = legacy
        .as_array_mut()
        .context("Original bonds are not an array")?;
    let (mut atom_offset, mut bond_offset) = (0usize, 0usize);
    for part in parts {
        let fragment =
            source.fragment(part["id"].as_i64().context("Original fragment ID")?, false)?;
        let atoms = array(&part["graph"], "atoms")?;
        let positions = array(part, "positions")?;
        ensure!(
            atoms.len() == positions.len(),
            "Original dative position count differs"
        );
        let end_offset = bond_offset + fragment.bonds.len();
        ensure!(
            end_offset <= captured.len(),
            "Original dative bond count differs"
        );
        if fragment.bonds.iter().any(SourceBond::projected) {
            let mut nodes = HashMap::new();
            for node in &fragment.atoms {
                let [x, y] = point(&node.position)?;
                let element: u64 = node.element.parse()?;
                let mut associated = None;
                for (index, (atom, position)) in atoms.iter().zip(positions).enumerate() {
                    let native_x = position["x"].as_f64().context("Original atom X")? * 28.0;
                    let native_y = -position["y"].as_f64().context("Original atom Y")? * 28.0;
                    if atom["atomic_number"].as_u64() == Some(element)
                        && (native_x - x * scale).abs() < 0.02
                        && (native_y - y * scale).abs() < 0.02
                    {
                        ensure!(
                            associated.is_none(),
                            "Ambiguous original dative atom association"
                        );
                        associated = Some(u64::try_from(atom_offset + index + 1)?);
                    }
                }
                nodes.insert(
                    node.id.as_deref(),
                    associated.context("Missing original dative atom association")?,
                );
            }
            for (index, bond) in fragment
                .bonds
                .iter()
                .enumerate()
                .filter(|(_, bond)| bond.projected())
            {
                let begin = nodes
                    .get(&bond.begin.as_deref())
                    .copied()
                    .context("Missing source donor association")?;
                let end = nodes
                    .get(&bond.end.as_deref())
                    .copied()
                    .context("Missing source acceptor association")?;
                adapt_paint(
                    &mut captured[bond_offset + index],
                    bond,
                    begin,
                    end,
                    editable,
                )?;
            }
        }
        atom_offset += atoms.len();
        bond_offset = end_offset;
    }
    ensure!(
        bond_offset == captured.len(),
        "Original dative bond count differs"
    );
    Ok(legacy)
}

/// Adapt original editable appearance bonds. Parts and scale are the caller's
/// independent oracle inputs, never fragments obtained from a current read.
pub fn document_bonds(
    text: &str,
    legacy: Value,
    parts: &[cdxml::Fragment],
    scale: f64,
) -> anyhow::Result<Value> {
    let parts = serde_json::to_value(parts)?;
    appearance(
        &Source::parse(text)?,
        legacy,
        parts.as_array().context("Original parts")?,
        scale,
        true,
    )
}

/// Adapt wide NativeBond captures, which do not have a projection field.
pub fn native_bonds(
    text: &str,
    legacy: Value,
    parts: &[cdxml::Fragment],
    scale: f64,
) -> anyhow::Result<Value> {
    let parts = serde_json::to_value(parts)?;
    appearance(
        &Source::parse(text)?,
        legacy,
        parts.as_array().context("Original parts")?,
        scale,
        false,
    )
}

fn chemical(
    captured: &mut Value,
    fragment: &SourceFragment,
    atom_offset: usize,
    bond_offset: usize,
    normalized: bool,
    restored: bool,
) -> anyhow::Result<()> {
    if !fragment.bonds.iter().any(SourceBond::dative) {
        return Ok(());
    }
    let indices = fragment.chemical_indices()?;
    let atoms = array(&captured["graph"], "atoms")?;
    ensure!(
        atom_offset + fragment.atoms.len() <= atoms.len(),
        "Original dative graph atom count differs"
    );
    for (index, source_atom) in fragment.atoms.iter().enumerate() {
        ensure!(
            atoms[atom_offset + index]["atomic_number"].as_u64()
                == Some(source_atom.element.parse()?),
            "Original dative graph atom order differs"
        );
    }
    let bonds = array(&captured["graph"], "bonds")?;
    ensure!(
        bond_offset + fragment.bonds.len() <= bonds.len(),
        "Original dative graph bond count differs"
    );
    ensure!(
        array(captured, "directions")?.len() == bonds.len(),
        "Original dative direction count differs"
    );
    let has_cfg = captured.get("bond_cfg").is_some();
    if has_cfg {
        ensure!(
            array(captured, "bond_cfg")?.len() == bonds.len(),
            "Original dative CFG count differs"
        );
    }
    for (index, source_bond) in fragment
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, bond)| bond.dative())
    {
        let (begin, end) = fragment.chemical_pair(source_bond, &indices)?;
        let (begin, end) = (atom_offset + begin, atom_offset + end);
        let (direction, cfg, reversed) = source_bond.old_chemical(normalized);
        let (old_begin, old_end) = if reversed { (end, begin) } else { (begin, end) };
        let index = bond_offset + index;
        let bond = &mut captured["graph"]["bonds"][index];
        let order = if restored && source_bond.secondary.as_deref() == Some("DottedHydrogen") {
            0
        } else {
            5
        };
        assert_field(bond, "order", json!(order))?;
        update_field(bond, "a", json!(old_begin), json!(begin))?;
        update_field(bond, "b", json!(old_end), json!(end))?;
        ensure!(
            captured["directions"][index] == json!(direction),
            "Original dative chemical direction differs: {} != {direction:?}",
            captured["directions"][index]
        );
        captured["directions"][index] = json!("none");
        if has_cfg {
            ensure!(
                captured["bond_cfg"][index] == json!(cfg),
                "Original dative CFG differs"
            );
            captured["bond_cfg"][index] = Value::Null;
        }
    }
    Ok(())
}

fn fragment_captures(
    source: &Source,
    fragments: &mut Value,
    normalized: bool,
) -> anyhow::Result<()> {
    let fragments = fragments
        .as_array_mut()
        .context("Original fragment captures")?;
    for captured in fragments {
        let source = source.fragment(
            captured["id"].as_i64().context("Original fragment ID")?,
            true,
        )?;
        if !source.bonds.iter().any(SourceBond::dative) {
            continue;
        }
        ensure!(
            array(&captured["graph"], "atoms")?.len() == source.atoms.len(),
            "Expanded dative source atom count differs"
        );
        ensure!(
            array(&captured["graph"], "bonds")?.len() == source.bonds.len(),
            "Expanded dative source bond count differs"
        );
        chemical(captured, source, 0, 0, normalized, false)?;
    }
    Ok(())
}

/// Adapt a direct native parser observation. Pass the precise XML supplied to
/// the two raw parsers (normalized XML in the normalization target).
pub fn normalized_fragments(text: &str, mut legacy: Value) -> anyhow::Result<Value> {
    let source = Source::parse(text)?;
    if source.has_dative() {
        fragment_captures(&source, &mut legacy["fragments"], false)?;
    }
    Ok(legacy)
}

fn combined(source: &Source, state: &mut Value, fragments: Option<&[Value]>) -> anyhow::Result<()> {
    let ordered = if let Some(fragments) = fragments {
        fragments
            .iter()
            .map(|part| source.fragment(part["id"].as_i64().context("Original fragment ID")?, true))
            .collect::<anyhow::Result<Vec<_>>>()?
    } else {
        source.fragments.iter().collect::<Vec<_>>()
    };
    let (mut atoms, mut bonds) = (0usize, 0usize);
    for fragment in ordered {
        chemical(state, fragment, atoms, bonds, true, true)?;
        atoms += fragment.atoms.len();
        bonds += fragment.bonds.len();
    }
    ensure!(
        array(&state["graph"], "atoms")?.len() == atoms,
        "Expanded combined dative atom count differs"
    );
    ensure!(
        array(&state["graph"], "bonds")?.len() == bonds,
        "Expanded combined dative bond count differs"
    );
    Ok(())
}

// This helper receives only input XML. Neither a current Rust observation nor
// a legacy capture can contribute to the independent expected values.
fn endpoint_evidence(text: &str, source: &Source) -> anyhow::Result<Option<Value>> {
    if !source
        .fragments
        .iter()
        .flat_map(|f| &f.bonds)
        .any(|bond| bond.projected() && bond.end_paint().is_some() && bond.old_chemical(true).2)
    {
        return Ok(None);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    };
    let mut child = Command::new(root.join(python))
        .arg(root.join("reference/reference_dative_endpoint.py"))
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Starting independent dative endpoint oracle")?;
    child
        .stdin
        .take()
        .context("Dative oracle stdin")?
        .write_all(text.as_bytes())?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Independent dative endpoint oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let evidence: Value = serde_json::from_slice(&output.stdout)?;
    if evidence.is_null() {
        return Ok(None);
    }
    ensure!(
        evidence["rdkit_version"] == reshiki::chemistry::RDKIT_VERSION,
        "Independent dative endpoint RDKit version differs"
    );
    ensure!(
        evidence["import_sha256"]
            == "8e29ae0e691ee01c3cdf3e9121ff72a8249b89accfab1ef1604b288be875196d",
        "Original dative import prefix differs"
    );
    let fragment = source
        .fragments
        .first()
        .context("Dative evidence fragment")?;
    ensure!(
        source.fragments.len() == 1 && fragment.atoms.len() == 2 && fragment.bonds.len() == 1,
        "Independent dative evidence exceeds the two-endpoint bound"
    );
    let bond = &fragment.bonds[0];
    ensure!(
        bond.projected() && bond.end_paint().is_some() && bond.old_chemical(true).2,
        "Independent dative evidence is not an existing End-paint correction"
    );
    let (begin, end) = fragment.chemical_pair(bond, &fragment.chemical_indices()?)?;
    ensure!(
        evidence["endpoints"] == json!([begin, end]),
        "Independent dative endpoint association differs"
    );
    Ok(Some(evidence))
}

fn endpoint_state(text: &str, source: &Source, state: &mut Value) -> anyhow::Result<()> {
    let Some(evidence) = endpoint_evidence(text, source)? else {
        return Ok(());
    };
    // Guard the entire independent old state first, including both old valence
    // fields and every other atom/property. No partial old match is accepted.
    ensure!(
        *state == evidence["original_state"],
        "Original End-paint dative state differs from pinned source evidence"
    );
    for endpoint in array(&evidence, "endpoints")? {
        let index = usize::try_from(endpoint.as_u64().context("Dative endpoint index")?)?;
        for field in ["explicit_valence", "implicit_hydrogens"] {
            update_field(
                &mut state["valences"][index],
                field,
                evidence["original_state"]["valences"][index][field].clone(),
                evidence["intended_state"]["valences"][index][field].clone(),
            )?;
        }
    }
    Ok(())
}

fn endpoint_labels(text: &str, source: &Source, document: &mut Value) -> anyhow::Result<()> {
    let Some(evidence) = endpoint_evidence(text, source)? else {
        return Ok(());
    };
    // Only the three already-comparable End/None Documents are approved.
    // Secondary paint retains its existing rejection and exact label boundary.
    if evidence["labels"] != json!(true) {
        return Ok(());
    }
    ensure!(
        document["atoms"] == evidence["original_atoms"],
        "Original End-paint dative atoms differ from pinned source evidence"
    );
    for endpoint in array(&evidence, "endpoints")? {
        let index = usize::try_from(endpoint.as_u64().context("Dative endpoint index")?)?;
        update_field(
            &mut document["atoms"][index],
            "label_h",
            evidence["original_atoms"][index]["label_h"].clone(),
            evidence["intended_atoms"][index]["label_h"].clone(),
        )?;
    }
    Ok(())
}

/// Adapt the original preparation capture without changing its expanded XML
/// tree. Only source-qualified End endpoints receive independently derived
/// cached valences; every other chemical property remains the original value.
pub fn preparation(text: &str, mut legacy: Value) -> anyhow::Result<Value> {
    let source = Source::parse(text)?;
    if !source.has_dative() {
        return Ok(legacy);
    }
    let fragments = array(&legacy, "fragments")?.to_vec();
    let scale = legacy["source_scale"]
        .as_f64()
        .context("Original source scale")?;
    endpoint_state(text, &source, &mut legacy["molecule"]["state"])?;
    legacy["bonds"] = appearance(&source, legacy["bonds"].clone(), &fragments, scale, false)?;
    fragment_captures(&source, &mut legacy["fragments"], true)?;
    combined(&source, &mut legacy["molecule"]["state"], Some(&fragments))?;
    Ok(legacy)
}

fn document_appearance(source: &Source, mut legacy: Value, native: bool) -> anyhow::Result<Value> {
    if !source.has_projection() {
        return Ok(legacy);
    }
    let source_length: f64 = source.bond_length.parse()?;
    let style = legacy
        .get("drawing_style")
        .context("Original drawing style")?;
    let point_length = style["bond_length_pt"]
        .as_f64()
        .context("Original point bond length")?;
    let world_length = style["bond_length_world"]
        .as_f64()
        .context("Original world bond length")?;
    ensure!(
        [source_length, point_length, world_length]
            .iter()
            .all(|length| length.is_finite() && *length > 0.0),
        "Invalid original/source dative bond length"
    );
    // The original native reader normalizes the source BondLength to its
    // default bond, and the worker then applies the captured style's point
    // length relative to the 14.4-point default. Derive both steps from the
    // source/capture; do not infer a new endpoint from the current Document.
    let scale = world_length / source_length * (point_length / 14.4);
    let atoms = array(&legacy, "atoms")?;
    let mut ids = HashSet::new();
    for atom in atoms {
        ensure!(
            ids.insert(atom["id"].as_u64().context("Original document atom ID")?),
            "Duplicate original document atom ID"
        );
    }
    // NativeScene atoms do not include coordinates. Its atom IDs, like the
    // original combined molecule IDs, follow direct fragment node order.
    // The molecule adapter separately validates the associated graph elements.
    let mut pairs = Vec::new();
    let mut offset = 0usize;
    for fragment in &source.fragments {
        let end_offset = offset + fragment.atoms.len();
        ensure!(
            end_offset <= atoms.len(),
            "Expanded dative document atom count differs"
        );
        if fragment.bonds.iter().any(SourceBond::projected) {
            let indices = fragment.chemical_indices()?;
            for bond in &fragment.bonds {
                if !bond.projected() {
                    pairs.push(None);
                    continue;
                }
                let (begin, end) = fragment.chemical_pair(bond, &indices)?;
                let (begin_atom, end_atom) = (&atoms[offset + begin], &atoms[offset + end]);
                if !native {
                    for (index, atom) in [(begin, begin_atom), (end, end_atom)] {
                        let number: usize = fragment.atoms[index].element.parse()?;
                        let symbol = ELEMENTS
                            .get(number)
                            .context("Source dative element")?
                            .symbol;
                        assert_field(atom, "element", json!(symbol))?;
                        let [x, y] = point(&fragment.atoms[index].position)?;
                        ensure!(
                            (atom["position"]["x"].as_f64().context("Original atom X")?
                                - x * scale)
                                .abs()
                                < 0.02
                                && (atom["position"]["y"].as_f64().context("Original atom Y")?
                                    - y * scale)
                                    .abs()
                                    < 0.02,
                            "Original dative document atom association differs"
                        );
                    }
                }
                pairs.push(Some((
                    bond,
                    begin_atom["id"].as_u64().context("Original donor ID")?,
                    end_atom["id"].as_u64().context("Original acceptor ID")?,
                )));
            }
        } else {
            pairs.extend(fragment.bonds.iter().map(|_| None));
        }
        offset = end_offset;
    }
    ensure!(
        offset == atoms.len(),
        "Expanded dative document atom count differs"
    );
    let bonds = legacy["bonds"]
        .as_array_mut()
        .context("Original document bonds")?;
    ensure!(
        bonds.len() == pairs.len(),
        "Expanded dative document bond count differs"
    );
    for (bond, pair) in bonds.iter_mut().zip(pairs) {
        if let Some((source, begin, end)) = pair {
            adapt_paint(bond, source, begin, end, !native)?;
        }
    }
    Ok(legacy)
}

/// Adapt an original complete native scene; native paint has no projection
/// flag, while the chemical state clears dative direction and preserves B/E.
pub fn scene(text: &str, mut legacy: Value) -> anyhow::Result<Value> {
    let source = Source::parse(text)?;
    if source.has_dative() {
        endpoint_state(text, &source, &mut legacy["molecule"]["state"])?;
        combined(&source, &mut legacy["molecule"]["state"], None)?;
        legacy["base"] = document_appearance(&source, legacy["base"].clone(), true)?;
    }
    Ok(legacy)
}

/// Adapt raw legacy Document JSON before its normal serde/validation boundary.
/// In particular this retains the evidence for the projected-dative acceptance
/// extension instead of discarding a legacy validation error as `None`.
pub fn document(text: &str, legacy: Value) -> anyhow::Result<Value> {
    let source = Source::parse(text)?;
    let mut legacy = document_appearance(&source, legacy, false)?;
    endpoint_labels(text, &source, &mut legacy)?;
    Ok(legacy)
}

/// Convenience for complete import responses; all other response fields remain
/// the exact original capture, including analysis and warnings.
pub fn response(text: &str, mut legacy: Value) -> anyhow::Result<Value> {
    let source = Source::parse(text)?;
    if source.has_projection() {
        let document = legacy
            .get_mut("document")
            .context("Missing original response document")?;
        *document = self::document(text, document.clone())?;
    }
    Ok(legacy)
}

/// Classify a source-qualified projected-dative document independently of any
/// current result. Callers still verify original failure and current success.
pub fn has_projection(text: &str) -> anyhow::Result<bool> {
    Ok(Source::parse(text)?.has_projection())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carbon_source() -> String {
        "<CDXML BondLength='14.4'><page><fragment id='30'><n id='101' p='0 0'/><n id='109' p='14.4 0'/><b id='201' Order='dative' Display='WedgeEnd' B='101' E='109'/></fragment></page></CDXML>".to_owned()
    }

    #[test]
    fn endpoint_consequences_follow_source_roles_and_preserve_all_other_fields()
    -> anyhow::Result<()> {
        for text in [
            carbon_source(),
            carbon_source().replace("B='101' E='109'", "B='109' E='101'"),
        ] {
            let source = Source::parse(&text)?;
            let evidence =
                endpoint_evidence(&text, &source)?.context("Missing bounded evidence")?;
            let mut state = evidence["original_state"].clone();
            endpoint_state(&text, &source, &mut state)?;
            combined(&source, &mut state, None)?;
            assert_eq!(state, evidence["intended_state"]);
            let mut document =
                json!({"atoms":evidence["original_atoms"],"extra":"compared exactly"});
            endpoint_labels(&text, &source, &mut document)?;
            assert_eq!(
                document,
                json!({"atoms":evidence["intended_atoms"],"extra":"compared exactly"})
            );
        }
        Ok(())
    }

    #[test]
    fn tampered_original_endpoint_valence_hydrogens_or_labels_fail() -> anyhow::Result<()> {
        let text = carbon_source();
        let source = Source::parse(&text)?;
        let evidence = endpoint_evidence(&text, &source)?.context("Missing bounded evidence")?;
        for index in 0..2 {
            for field in ["explicit_valence", "implicit_hydrogens"] {
                let mut state = evidence["original_state"].clone();
                state["valences"][index][field] = json!(99);
                assert!(endpoint_state(&text, &source, &mut state).is_err());
            }
            let mut document = json!({"atoms":evidence["original_atoms"]});
            document["atoms"][index]["label_h"] = json!(99);
            assert!(endpoint_labels(&text, &source, &mut document).is_err());
        }
        Ok(())
    }

    #[test]
    fn endpoint_allowance_rejects_extra_captured_atoms_or_fields() -> anyhow::Result<()> {
        let text = carbon_source();
        let source = Source::parse(&text)?;
        let evidence = endpoint_evidence(&text, &source)?.context("Missing bounded evidence")?;
        let mut extra_atom = evidence["original_state"].clone();
        extra_atom["graph"]["atoms"]
            .as_array_mut()
            .unwrap()
            .push(json!({"atomic_number":6}));
        let mut extra_field = evidence["original_state"].clone();
        extra_field["valences"][0]["extra"] = json!(1);
        let mut changed_field = evidence["original_state"].clone();
        changed_field["graph"]["atoms"][0]["charge"] = json!(1);
        for mut state in [extra_atom, extra_field, changed_field] {
            assert!(endpoint_state(&text, &source, &mut state).is_err());
        }
        let mut extra_atom = json!({"atoms":evidence["original_atoms"]});
        extra_atom["atoms"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":3}));
        let mut extra_field = json!({"atoms":evidence["original_atoms"]});
        extra_field["atoms"][0]["extra"] = json!(1);
        let mut changed_field = json!({"atoms":evidence["original_atoms"]});
        changed_field["atoms"][1]["charge"] = json!(1);
        for mut document in [extra_atom, extra_field, changed_field] {
            assert!(endpoint_labels(&text, &source, &mut document).is_err());
        }
        Ok(())
    }

    #[test]
    fn unproved_sources_keep_valences_and_labels_exact() -> anyhow::Result<()> {
        for text in [
            carbon_source().replace("WedgeEnd", "WedgeBegin"),
            carbon_source().replace("Order='dative'", "Order='1'"),
            carbon_source().replace(
                "Display='WedgeEnd'",
                "Display='WedgeEnd' Display2='DottedHydrogen'",
            ),
            carbon_source().replace("id='101' p", "id='101' Element='7' p"),
            carbon_source().replace("id='101' p", "id='101' NumHydrogens='0' p"),
            carbon_source().replace("</fragment>", "<n id='110' p='28.8 0'/></fragment>"),
        ] {
            let source = Source::parse(&text)?;
            assert!(endpoint_evidence(&text, &source)?.is_none());
            let original = json!({"valences":[{"explicit_valence":99}],"atoms":[{"label_h":99}],"unrelated":true});
            let mut state = original.clone();
            endpoint_state(&text, &source, &mut state)?;
            assert_eq!(state, original);
            endpoint_labels(&text, &source, &mut state)?;
            assert_eq!(state, original);
            // A proposed new value on these sources still fails equality.
            let mut different = original.clone();
            different["valences"][0]["explicit_valence"] = json!(0);
            assert_ne!(state, different);
            different = original.clone();
            different["atoms"][0]["label_h"] = json!(0);
            assert_ne!(state, different);
        }
        Ok(())
    }

    #[test]
    fn secondary_end_paint_has_no_hydrogen_label_allowance() -> anyhow::Result<()> {
        for secondary in ["Solid", "Dash", "Bold"] {
            let text = carbon_source().replace(
                "Display='WedgeEnd'",
                &format!("Display='WedgeEnd' Display2='{secondary}'"),
            );
            let source = Source::parse(&text)?;
            let evidence =
                endpoint_evidence(&text, &source)?.context("Missing valence evidence")?;
            let mut state = evidence["original_state"].clone();
            endpoint_state(&text, &source, &mut state)?;
            combined(&source, &mut state, None)?;
            assert_eq!(state, evidence["intended_state"]);
            let original = json!({"atoms":evidence["original_atoms"]});
            let mut document = original.clone();
            endpoint_labels(&text, &source, &mut document)?;
            assert_eq!(document, original);
            assert_ne!(document["atoms"], evidence["intended_atoms"]);
        }
        Ok(())
    }

    fn source(display: &str, secondary: Option<&str>) -> String {
        let secondary = secondary
            .map(|value| format!(" Display2='{value}'"))
            .unwrap_or_default();
        format!(
            "<CDXML><page><fragment id='30'><n id='101' Element='7' p='0 0'/><n id='109' Element='27' p='14.4 0'/><b Order='dative' Display='{display}' B='101' E='109' DoublePosition='Left'{secondary}/></fragment></page></CDXML>"
        )
    }

    #[test]
    fn all_end_paints_require_exact_old_values_and_preserve_unrelated_fields() -> anyhow::Result<()>
    {
        for (display, old, new) in [
            ("WedgeEnd", "wedge", "wedge_end"),
            ("WedgedHashEnd", "hash", "hash_end"),
            ("HollowWedgeEnd", "hollow_wedge", "hollow_wedge_end"),
        ] {
            let source = Source::parse(&source(display, None))?;
            let source = &source.fragments[0].bonds[0];
            let original = json!({"a":9,"b":4,"order":5,"display":old,"double_position":"right","projection":false,"color":[1,2,3],"secondary_display":"bold","z_order":-7,"stereo":null});
            let mut adapted = original.clone();
            adapt_paint(&mut adapted, source, 4, 9, true)?;
            let mut intended = original.clone();
            intended["a"] = json!(4);
            intended["b"] = json!(9);
            intended["display"] = json!(new);
            intended["double_position"] = json!("left");
            intended["projection"] = json!(true);
            assert_eq!(adapted, intended);
            for (field, changed) in [
                ("a", json!(4)),
                ("display", json!(new)),
                ("double_position", json!("left")),
                ("projection", json!(true)),
                ("order", json!(1)),
            ] {
                let mut invalid = original.clone();
                invalid[field] = changed;
                assert!(
                    adapt_paint(&mut invalid, source, 4, 9, true).is_err(),
                    "Unexpected old {field} accepted"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn plain_dashed_and_secondary_hydrogen_are_not_projection_exceptions() -> anyhow::Result<()> {
        for (display, secondary) in [
            ("Solid", None),
            ("Dash", None),
            ("WedgeEnd", Some("DottedHydrogen")),
        ] {
            let text = source(display, secondary);
            let legacy = json!([{"unrelated":"kept exactly"}]);
            assert!(!has_projection(&text)?);
            assert_eq!(document_bonds(&text, legacy.clone(), &[], 1.0)?, legacy);
        }
        Ok(())
    }

    #[test]
    fn raw_parser_direction_cfg_and_endpoints_have_independent_guards() -> anyhow::Result<()> {
        let text = source("WedgedHashEnd", None);
        let legacy = json!({"fragments":[{"id":30,"graph":{"atoms":[{"atomic_number":7},{"atomic_number":27}],"bonds":[{"a":1,"b":0,"order":5,"aromatic":false}]},"directions":["hash"],"bond_cfg":[3],"metadata":{"untouched":"yes"},"positions":[{"x":0.0,"y":0.0,"z":4.0},{"x":1.5,"y":0.0,"z":8.0}]}]});
        let mut intended = legacy.clone();
        intended["fragments"][0]["graph"]["bonds"][0]["a"] = json!(0);
        intended["fragments"][0]["graph"]["bonds"][0]["b"] = json!(1);
        intended["fragments"][0]["directions"][0] = json!("none");
        intended["fragments"][0]["bond_cfg"][0] = Value::Null;
        assert_eq!(normalized_fragments(&text, legacy.clone())?, intended);
        let mut wrong = legacy.clone();
        wrong["fragments"][0]["bond_cfg"][0] = json!(1);
        assert!(normalized_fragments(&text, wrong).is_err());
        let mut wrong = legacy;
        wrong["fragments"][0]["directions"][0] = json!("wedge");
        assert!(normalized_fragments(&text, wrong).is_err());
        Ok(())
    }

    #[test]
    fn appearance_donor_and_acceptor_follow_independent_native_association() -> anyhow::Result<()> {
        let source = Source::parse(&source("WedgeEnd", None))?;
        let parts = json!([{"id":30,"graph":{"atoms":[{"atomic_number":27},{"atomic_number":7}]},"positions":[{"x":1.5,"y":0.0,"z":0.0},{"x":0.0,"y":0.0,"z":0.0}]}]);
        let original = json!([{"a":1,"b":2,"order":5,"display":"wedge","double_position":"right","color":[0,0,0],"z_order":"0"}]);
        let mut intended = original.clone();
        intended[0]["a"] = json!(2);
        intended[0]["b"] = json!(1);
        intended[0]["display"] = json!("wedge_end");
        intended[0]["double_position"] = json!("left");
        assert_eq!(
            appearance(
                &source,
                original,
                parts.as_array().unwrap(),
                42.0 / 14.4,
                false
            )?,
            intended
        );
        Ok(())
    }

    #[test]
    fn document_scale_uses_source_length_and_original_conformer_scaling() -> anyhow::Result<()> {
        let text = source("HollowWedgeEnd", None).replace("<CDXML>", "<CDXML BondLength='28.8'>");
        let original = json!({"drawing_style":{"bond_length_pt":28.8,"bond_length_world":42.0},"atoms":[{"id":55,"element":"N","position":{"x":0.0,"y":0.0},"label_h":2},{"id":66,"element":"Co","position":{"x":42.0,"y":0.0},"label_h":0}],"bonds":[{"a":66,"b":55,"order":5,"display":"hollow_wedge","double_position":"right","color":[0,0,0],"z_order":0}]});
        let mut intended = original.clone();
        intended["bonds"][0]["a"] = json!(55);
        intended["bonds"][0]["b"] = json!(66);
        intended["bonds"][0]["display"] = json!("hollow_wedge_end");
        intended["bonds"][0]["double_position"] = json!("left");
        intended["bonds"][0]["projection"] = json!(true);
        assert_eq!(document(&text, original.clone())?, intended);
        let mut wrong = original;
        wrong["atoms"][1]["position"]["x"] = json!(21.0);
        assert!(document(&text, wrong).is_err());
        Ok(())
    }

    #[test]
    fn ignored_raw_dative_paint_keeps_whole_capture_exact() -> anyhow::Result<()> {
        for display in ["Hash", "Bold", "HollowWedgeBegin", "HollowWedgeEnd"] {
            let original = json!({"fragments":[{"id":30,"graph":{"atoms":[{"atomic_number":7},{"atomic_number":27}],"bonds":[{"a":0,"b":1,"order":5,"aromatic":false}]},"directions":["none"],"bond_cfg":[null],"metadata":{"untouched":"yes"}}]});
            assert_eq!(
                normalized_fragments(&source(display, None), original.clone())?,
                original
            );
        }
        Ok(())
    }
}
