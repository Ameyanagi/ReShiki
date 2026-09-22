use super::{Error, Fragment, Parsed, Result, stereo};
use crate::chemistry::{
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::{Metadata, StereoGroup},
    stereo::Point3,
};
use roxmltree::Node;
use std::collections::{BTreeMap, HashMap, HashSet};

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn integer(node: Node<'_, '_>, name: &str, default: i64) -> Result<i64> {
    node.attribute(name).map_or(Ok(default), |s| {
        s.trim()
            .parse()
            .map_err(|_| invalid(format!("Invalid {name}")))
    })
}
fn id(node: Node<'_, '_>) -> Result<u32> {
    u32::try_from(integer(node, "id", 0)?)
        .map_err(|_| invalid("Object ID is outside the unsigned 32-bit range"))
}
fn coordinate(value: &str) -> Result<f64> {
    let n: f64 = value.parse().map_err(|_| invalid("Invalid coordinate"))?;
    let fixed = (n * 65536.).trunc();
    if !fixed.is_finite() || fixed < f64::from(i32::MIN) || fixed > f64::from(i32::MAX) {
        return Err(invalid("Coordinate exceeds the signed 16.16 range"));
    }
    // The intermediate native integer also removes the sign of truncated zero.
    Ok(f64::from(fixed as i32))
}
fn position(node: Node<'_, '_>) -> Result<(Point3, bool)> {
    let (text, is_3d) = if let Some(xyz) = node.attribute("xyz") {
        (xyz, true)
    } else {
        (node.attribute("p").unwrap_or("0 0"), false)
    };
    let mut values = text.split_whitespace();
    let mut next = || coordinate(values.next().ok_or_else(|| invalid("Missing coordinate"))?);
    let x = next()?;
    let y = -next()?;
    let z = if is_3d { next()? } else { 0. };
    if values.next().is_some() {
        return Err(invalid("Unexpected coordinate component"));
    }
    Ok((Point3 { x, y, z }, is_3d))
}

fn predicates(node: Node<'_, '_>) -> Result<()> {
    for (name, defaults) in [
        ("RingBondCount", &["Unspecified", "-1"][..]),
        ("UnsaturatedBonds", &["Unspecified", "0"][..]),
        ("SubstituentsUpTo", &[][..]),
        ("SubstituentsExactly", &[][..]),
        ("FreeSites", &["0"][..]),
        ("LinkCountLow", &[][..]),
        ("LinkCountHigh", &[][..]),
        ("IsotopicAbundance", &["Unspecified", "0"][..]),
        ("Topology", &["Unspecified", "0"][..]),
        ("RxnChange", &["no", "0"][..]),
        ("RxnStereo", &["Unspecified", "0"][..]),
        ("RxnParticipation", &["Unspecified", "0"][..]),
    ] {
        if node.attribute(name).is_some_and(|v| !defaults.contains(&v)) {
            return Err(Error::Unsupported("query or reaction predicate"));
        }
    }
    Ok(())
}

pub(super) fn read(text: &str) -> Result<Parsed> {
    if text.len() > 16 * 1024 * 1024 {
        return Err(Error::Limit);
    }
    if text.contains("<!ENTITY") {
        return Err(Error::Unsupported("DTD entities"));
    }
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            nodes_limit: 400_000,
            allow_dtd: true,
        },
    )
    .map_err(|e| invalid(e.to_string()))?;
    let root = document.root_element();
    if root.tag_name().name() != "CDXML" || root.tag_name().namespace().is_some() {
        return Err(invalid("Expected CDXML root"));
    }
    let mut objects = 0usize;
    let mut properties = 0usize;
    let mut chemical_nodes = 0usize;
    let mut chemical_bonds = 0usize;
    let mut validation = vec![(root, 0usize)];
    while let Some((node, depth)) = validation.pop() {
        if depth > 64 {
            return Err(Error::Limit);
        }
        objects += 1;
        properties += node.attributes().len();
        if objects > 100_000 || properties > 1_000_000 {
            return Err(Error::Limit);
        }
        if node.tag_name().namespace().is_some()
            || node.attributes().any(|a| a.namespace().is_some())
        {
            return Err(invalid("Unsupported namespace"));
        }
        match node.tag_name().name() {
            "n" => {
                predicates(node)?;
                chemical_nodes += 1;
            }
            "b" => {
                predicates(node)?;
                chemical_bonds += 1;
            }
            "CDXML" | "page" | "fragment" | "group" | "t" | "s" | "fonttable" | "font"
            | "colortable" | "color" | "arrow" | "graphic" | "curve" | "represent"
            | "objecttag" | "embeddedobject" => (),
            _ => return Err(Error::Unsupported("drawing object")),
        }
        if matches!(node.tag_name().name(), "fragment" | "n" | "b") {
            id(node)?;
        }
        validation.extend(
            node.children()
                .filter(|n| n.is_element())
                .map(|n| (n, depth + 1)),
        );
    }
    let bond_length = coordinate(root.attribute("BondLength").unwrap_or("0"))?;
    let mut fragments = Vec::new();
    let mut stack = root
        .children()
        .filter(|n| n.has_tag_name("page"))
        .rev()
        .collect::<Vec<_>>();
    let mut total_atoms = 0usize;
    let mut total_bonds = 0usize;
    while let Some(node) = stack.pop() {
        if node.has_tag_name("fragment") {
            let part = fragment(node, bond_length)?;
            total_atoms += part.graph.atoms.len();
            total_bonds += part.graph.bonds.len();
            if total_atoms > 100_000 || total_bonds > 300_000 {
                return Err(Error::Limit);
            }
            fragments.push(part);
        } else {
            // Native ContainedObjects is a stable multimap ordered by object
            // type: groups precede fragments even when XML orders them later.
            stack.extend(node.children().filter(|n| n.has_tag_name("fragment")).rev());
            stack.extend(node.children().filter(|n| n.has_tag_name("group")).rev());
        }
    }
    if total_atoms != chemical_nodes || total_bonds != chemical_bonds {
        return Err(invalid("Molecular objects outside parsed fragments"));
    }
    Ok(Parsed { fragments })
}

fn atom(node: Node<'_, '_>) -> Result<Atom> {
    let node_type = node.attribute("NodeType").unwrap_or("Element");
    let mut number = integer(node, "Element", 6)?;
    match node_type {
        "Element"
        | "Unspecified"
        | "ElementListNickname"
        | "Formula"
        | "AnonymousAlternativeGroup"
        | "NamedAlternativeGroup"
        | "MultiAttachment"
        | "VariableAttachment"
        | "LinkNode"
        | "Monomer" => (),
        "ExternalConnectionPoint" => {
            if node.attribute("ExternalConnectionType") == Some("Diamond") {
                number = 0;
            }
        }
        "Nickname" | "Fragment" => {
            if node.children().any(|n| n.has_tag_name("fragment")) {
                return Err(Error::Unsupported(
                    "abbreviation must be flattened before molecular parsing",
                ));
            }
            return Err(Error::Unsupported("unexpanded nickname"));
        }
        "GenericNickname" => {
            let label = node.attribute("GenericNickname").unwrap_or("");
            if label.starts_with('R') || matches!(label, "A" | "Q" | "M" | "MH" | "X") {
                return Err(Error::Unsupported("query atom"));
            }
            if label.starts_with(['A', 'Q', 'X', 'M']) {
                number = 0;
            }
        }
        "ElementList" => {
            if node
                .attribute("ElementList")
                .is_some_and(|v| !v.trim().is_empty())
            {
                return Err(Error::Unsupported("element-list query"));
            }
        }
        _ => return Err(Error::Unsupported("non-element node type")),
    }
    let atomic_number = u8::try_from(number)
        .ok()
        .filter(|n| *n <= 118)
        .ok_or_else(|| invalid("Invalid atomic number"))?;
    let mut isotope = integer(node, "Isotope", 0)?;
    // ChemDraw labels beginning with R override isotope even on ordinary atoms.
    for text in node.children().filter(|n| n.has_tag_name("t")) {
        let text = text
            .children()
            .filter(|n| n.has_tag_name("s"))
            .flat_map(|n| n.descendants().filter(|n| n.is_text()))
            .filter_map(|n| n.text())
            .collect::<String>();
        if let Some(suffix) = text.strip_prefix('R') {
            if suffix.is_empty() {
                isotope = 0;
            } else {
                let suffix = suffix.trim_start_matches(|c: char| c.is_ascii_whitespace());
                let sign = usize::from(suffix.starts_with(['-', '+']));
                let digits = suffix
                    .bytes()
                    .skip(sign)
                    .take_while(u8::is_ascii_digit)
                    .count();
                if let Some(value) = suffix
                    .get(..sign + digits)
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    isotope = i64::from(value as u16);
                }
            }
        }
    }
    let isotope = u16::try_from(isotope).map_err(|_| invalid("Invalid isotope"))?;
    let charge =
        i8::try_from(integer(node, "Charge", 0)?).map_err(|_| invalid("Invalid charge"))?;
    let hydrogens = integer(node, "NumHydrogens", 65535)?;
    let explicit = hydrogens != 65535;
    let explicit_hydrogens = if explicit {
        u8::try_from(hydrogens).map_err(|_| invalid("Invalid hydrogen count"))?
    } else {
        0
    };
    let radical_electrons = match node.attribute("Radical").unwrap_or("None") {
        "None" => 0,
        "Doublet" => 1,
        "Singlet" | "Triplet" => 2,
        _ => return Err(invalid("Unknown radical type")),
    };
    Ok(Atom {
        atomic_number,
        isotope,
        charge,
        explicit_hydrogens,
        no_implicit: explicit || matches!(node.attribute("AbnormalValence"), Some("yes" | "true")),
        aromatic: false,
        radical_electrons,
    })
}

fn fragment(node: Node<'_, '_>, bond_length: f64) -> Result<Fragment> {
    if node
        .attribute("SequenceType")
        .is_some_and(|v| v != "Unknown")
    {
        return Err(Error::Unsupported("biopolymer sequence"));
    }
    let mut result = Fragment {
        id: id(node)? as i32,
        atom_ids: Vec::new(),
        bond_ids: Vec::new(),
        fuse_labels: Vec::new(),
        graph: Graph {
            atoms: Vec::new(),
            bonds: Vec::new(),
        },
        metadata: Metadata::default(),
        directions: Vec::new(),
        positions: Vec::new(),
        is_3d: false,
        bond_cfg: Vec::new(),
        non_explicit_3d_chirality: Vec::new(),
        bond_cip: Vec::new(),
        atom_cip_ranks: Vec::new(),
    };
    let mut indices = HashMap::new();
    let mut groups: BTreeMap<(i8, u8), Vec<usize>> = BTreeMap::new();
    for child in node.children().filter(|n| n.has_tag_name("n")) {
        let identity = id(child)?;
        let index = result.graph.atoms.len();
        if indices.insert(identity, index).is_some() {
            return Err(invalid("Duplicate atom ID within a fragment"));
        }
        result.atom_ids.push(identity);
        result.fuse_labels.push(
            if child.attribute("NodeType") == Some("ExternalConnectionPoint") {
                Some(identity)
            } else {
                None
            },
        );
        result.graph.atoms.push(atom(child)?);
        let (position, is_3d) = position(child)?;
        result.positions.push(position);
        result.is_3d |= is_3d;
        let kind = match child.attribute("EnhancedStereoType") {
            Some("Absolute") => Some(0),
            Some("Or") => Some(1),
            Some("And") => Some(2),
            None | Some("Unspecified") => None,
            Some(_) => return Err(invalid("Unknown enhanced stereo type")),
        };
        if let Some(kind) = kind {
            let number = i8::try_from(integer(child, "EnhancedStereoGroupNum", 0)?)
                .map_err(|_| invalid("Enhanced stereo group number exceeds signed 8-bit range"))?;
            groups.entry((number, kind)).or_default().push(index);
        }
    }
    let mut pairs = HashSet::new();
    for child in node.children().filter(|n| n.has_tag_name("b")) {
        let endpoint = |name| -> Result<usize> {
            let id = u32::try_from(integer(child, name, 0)?)
                .map_err(|_| invalid("Invalid bond endpoint"))?;
            indices
                .get(&id)
                .copied()
                .ok_or_else(|| invalid("Missing bond endpoint in fragment"))
        };
        let (mut a, mut b) = (endpoint("B")?, endpoint("E")?);
        if a == b || !pairs.insert((a.min(b), a.max(b))) {
            return Err(invalid("Self or duplicate bond"));
        }
        let order = match child.attribute("Order").unwrap_or("1") {
            "1" => 1,
            "2" => 2,
            "3" => 3,
            "1.5" => 4,
            "dative" => 5,
            "4" => 6,
            _ => return Err(Error::Unsupported("query or unsupported bond order")),
        };
        let display = child.attribute("Display").unwrap_or("Solid");
        if matches!(display, "WedgeEnd" | "WedgedHashEnd") {
            std::mem::swap(&mut a, &mut b);
        }
        let (direction, cfg) = match display {
            "WedgeBegin" | "WedgeEnd" => (Direction::Wedge, Some(1)),
            "WedgedHashBegin" | "WedgedHashEnd" => (Direction::Hash, Some(3)),
            "Wavy" if order == 1 => (Direction::Unknown, Some(2)),
            "Wavy" if order == 2 => (Direction::EitherDouble, None),
            _ => (Direction::None, None),
        };
        let aromatic = order == 4;
        if aromatic {
            for i in [a, b] {
                result
                    .graph
                    .atoms
                    .get_mut(i)
                    .ok_or_else(|| invalid("Missing aromatic atom"))?
                    .aromatic = true;
            }
        }
        result.graph.bonds.push(Bond {
            a,
            b,
            order,
            aromatic,
        });
        result.bond_ids.push(id(child)?);
        result.directions.push(direction);
        result.bond_cfg.push(cfg);
        result.bond_cip.push(if order == 2 {
            match child.attribute("BS") {
                Some("E") => Some(2),
                Some("Z") => Some(3),
                _ => None,
            }
        } else {
            None
        });
    }
    result.graph.validate().map_err(invalid)?;
    result.metadata = Metadata::unspecified(&result.graph);
    let merge_absolute = groups.keys().filter(|(_, kind)| *kind == 0).count() > 1;
    let mut absolute = Vec::new();
    for ((number, kind), atoms) in groups {
        if kind == 0 && merge_absolute {
            absolute.push(atoms);
            continue;
        }
        result.metadata.groups.push(StereoGroup {
            kind,
            atoms,
            bonds: Vec::new(),
            read_id: if kind != 0 && number > 0 {
                number as u32
            } else {
                0
            },
            write_id: 0,
        });
    }
    if merge_absolute {
        result.metadata.groups.push(StereoGroup {
            kind: 0,
            atoms: absolute.into_iter().rev().flatten().collect(),
            bonds: Vec::new(),
            read_id: 0,
            write_id: 0,
        });
    }
    for (metadata, direction) in result.metadata.bonds.iter_mut().zip(&result.directions) {
        if *direction == Direction::EitherDouble {
            metadata.stereo = 1;
        }
    }
    if !result.is_3d {
        let length = if bond_length < 0. {
            let mut sum = 0.;
            for bond in &result.graph.bonds {
                let a = result
                    .positions
                    .get(bond.a)
                    .ok_or_else(|| invalid("Missing position"))?;
                let b = result
                    .positions
                    .get(bond.b)
                    .ok_or_else(|| invalid("Missing position"))?;
                sum += ((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)).sqrt();
            }
            sum / result.graph.bonds.len() as f64
        } else {
            bond_length
        };
        if length > 0. {
            let scale = 1.5 / length;
            for p in &mut result.positions {
                p.x *= scale;
                p.y *= scale;
                p.z *= scale;
            }
        }
    }
    stereo::finish(&mut result)?;
    Ok(result)
}
