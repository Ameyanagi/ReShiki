//! #877 weighs unlabelled Tc, Db, Hs, Nh, Mc and Ts with the IUPAC 2023
//! longest-lived mass numbers; labelled atoms keep their nuclide mass.
use serde_json::Value;

/// The change from the pinned RDKit weights (98, 268, 269, 284, 288, 292)
/// for a document's atoms.
pub fn change(atoms: &Value) -> f64 {
    atoms
        .as_array()
        .into_iter()
        .flatten()
        .filter(|atom| atom["isotope"].as_u64().unwrap_or(0) == 0)
        .map(|atom| match atom["element"].as_str() {
            Some("Tc") => -1.,
            Some("Db" | "Nh") => 2.,
            Some("Hs" | "Mc" | "Ts") => 1.,
            _ => 0.,
        })
        .sum()
}
