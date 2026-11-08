//! SMILES reading differences deliberately added after the pinned RDKit (#1037).
use serde_json::Value;

/// #918 records whether the input's slashes specified a cumulene E/Z that
/// import drops. RDKit has no such field; a `true` flag is still compared.
pub fn without_cumulene_flag(mut value: Value) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if map.get("dropped_cumulene_ez") == Some(&Value::Bool(false)) {
                    map.remove("dropped_cumulene_ez");
                }
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut value);
    value
}

/// #892 refuses `@AL` on an atom that is not an allene centre, where RDKit
/// silently drops the class (see `smiles::tests` for "F[C@AL1](Cl)(Br)I").
/// The refusal counts only when the structure confirms it: the same SMILES
/// read without the classes has some `@AL` atom that `allene::supported`
/// rejects (no allene centre there, or no winding 1/2). A supported allene
/// such as `OC=[C@AL1]=CF` is compared as usual.
pub fn refused_allene(text: &str, error: &str) -> bool {
    use reshiki::chemistry::{smiles, stereo::allene};
    const MARK: i32 = 9000;
    if !error.contains("invalid allene stereochemistry") {
        return false;
    }
    // Reaction participants keep their atom order when joined as one graph.
    let body = text.split_whitespace().next().unwrap_or_default();
    let body = body
        .split('>')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(".");
    // Replace each class with a map number that finds the atom after reading.
    let (mut marked, mut windings, mut rest) = (String::new(), Vec::new(), body.as_str());
    while let Some(at) = rest.find("@AL") {
        let digits = rest[at + 3..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        let Some(close) = rest[at..].find(']').map(|i| at + i) else {
            return false;
        };
        if rest[at..close].contains(':') {
            return false;
        }
        windings.push(rest[at + 3..at + 3 + digits].parse::<u32>().ok());
        marked.push_str(&rest[..at]);
        marked.push_str(&rest[at + 3 + digits..close]);
        marked.push_str(&format!(":{}]", MARK + windings.len() as i32 - 1));
        rest = &rest[close + 1..];
    }
    marked.push_str(rest);
    let Ok(parsed) = smiles::parse(&marked) else {
        return false;
    };
    // Every marker must find exactly one atom; a map number the input
    // already used cannot identify the atom, so the refusal is not accepted.
    let atoms: Option<Vec<usize>> = (0..windings.len())
        .map(|k| {
            let mut found = (0..parsed.metadata.atoms.len())
                .filter(|&i| parsed.metadata.atoms[i].map_number == MARK + k as i32);
            match (found.next(), found.next()) {
                (Some(i), None) => Some(i),
                _ => None,
            }
        })
        .collect();
    atoms.is_some_and(|atoms| {
        atoms
            .iter()
            .zip(&windings)
            .any(|(&i, &winding)| !allene::supported(&parsed.graph, i, 5, winding))
    })
}

/// #992: a terminal H beside an implicit H defines no E/Z (P-92.3). RDKit
/// assigns one from the slashes and keeps their directions; take RDKit's
/// state only for double bonds whose stereo atoms include such an H.
pub fn phantom_hydrogen_ez(actual: &mut Value, state: &Value) {
    let atoms = &state["graph"]["atoms"];
    let hydrogens = |i: u64| {
        let atom = &atoms[i as usize];
        atom["explicit_hydrogens"].as_u64().unwrap_or(0)
            + state["valences"][i as usize]["implicit_hydrogens"]
                .as_u64()
                .unwrap_or(0)
    };
    // A terminal H whose own neighbour also carries an H.
    let hydrogen = |i: &Value| {
        let i = i.as_u64().unwrap_or(u64::MAX);
        let atom = &atoms[i as usize];
        let bonds: Vec<_> = state["graph"]["bonds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["a"] == i || b["b"] == i)
            .collect();
        atom["atomic_number"] == 1
            && atom["isotope"].as_u64().is_some_and(|mass| mass <= 1)
            && bonds.len() == 1
            && hydrogens(
                bonds[0]["a"].as_u64().unwrap_or(0) + bonds[0]["b"].as_u64().unwrap_or(0) - i,
            ) > 0
    };
    for (i, bond) in state["metadata"]["bonds"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if !(bond["stereo_atoms"]
            .as_array()
            .is_some_and(|atoms| atoms.iter().any(hydrogen))
            && actual["metadata"]["bonds"][i]["stereo"] == 0)
        {
            continue;
        }
        for field in ["stereo", "stereo_atoms"] {
            actual["metadata"]["bonds"][i][field] = bond[field].clone();
        }
        // The slashes on the bonds around this double bond.
        let ends = &state["graph"]["bonds"][i];
        for (j, other) in state["graph"]["bonds"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if j != i
                && [&other["a"], &other["b"]]
                    .iter()
                    .any(|atom| **atom == ends["a"] || **atom == ends["b"])
            {
                actual["directions"][j] = state["directions"][j].clone();
            }
        }
    }
}
