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
pub fn refused_allene(text: &str, error: &str) -> bool {
    text.contains("@AL") && error.contains("invalid allene stereochemistry")
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
