//! Properties and valence calculated in Rust from the backend's sanitized graph.
//!
//! Aromaticity, resonance cleanup and other sanitization still belong to the
//! backend. Never use cached drawing labels: they may predate the latest edit.
//! Mass/formula semantics adapted from RDKit MolProps.cpp and Atom::getMass.
//! Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
pub mod aromaticity;
mod atomic_data;
pub mod descriptors;
pub mod electronic;
pub mod graph;
pub mod kekulize;
pub mod normalize;
pub mod ranking;
pub mod rings;
pub mod sanitize;
pub mod stereo;

pub use atomic_data::RDKIT_VERSION;
use atomic_data::{ELECTRON_MASS, ELEMENTS, ISOTOPES};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomFacts {
    pub atomic_number: u8,
    pub isotope: u16,
    pub charge: i8,
    /// Explicit attached + implicit H, excluding H represented by graph atoms.
    pub hydrogens: u8,
    pub radical_electrons: u8,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Properties {
    pub formula: String,
    pub mass: f64,
    pub exact_mass: f64,
    pub unpaired_electrons: u32,
}

struct Element {
    symbol: &'static str,
    average: f64,
    exact: f64,
    common_isotope: u16,
    outer_electrons: i32,
    valences: &'static [i32],
}

/// RDKit's default formula merges isotopes; masses retain their isotope values.
pub fn properties(atoms: &[AtomFacts]) -> Result<Properties, String> {
    // Together with the bounded atom fields this keeps all sums below u32/i32
    // limits, including malicious worker responses. No unchecked graph indexing.
    if atoms.len() > 100_000 {
        return Err("Molecular properties exceed the 100,000 atom limit".into());
    }
    let hydrogen = ELEMENTS.get(1).ok_or("Missing hydrogen mass data")?;
    let mut counts = BTreeMap::<&str, u32>::new();
    let (mut mass, mut exact_mass) = (0., 0.);
    let (mut hydrogens, mut charge, mut radicals) = (0u32, 0i32, 0u32);
    for atom in atoms {
        let element = ELEMENTS
            .get(usize::from(atom.atomic_number))
            .ok_or("Unknown atomic number in molecular properties")?;
        *counts.entry(element.symbol).or_default() += 1;
        hydrogens += u32::from(atom.hydrogens);
        charge += i32::from(atom.charge);
        radicals += u32::from(atom.radical_electrons);
        let isotope_mass = if atom.isotope == 0 {
            element.average
        } else {
            ISOTOPES
                .binary_search_by_key(&(atom.atomic_number, atom.isotope), |&(n, i, _)| (n, i))
                .ok()
                .and_then(|index| ISOTOPES.get(index))
                .map(|&(_, _, mass)| mass)
                // Match RDKit's unknown-isotope fallback, including dummy atoms.
                .unwrap_or_else(|| {
                    if atom.atomic_number == 0 {
                        0.
                    } else {
                        f64::from(atom.isotope)
                    }
                })
        };
        // Keep RDKit's operation order; grouping sums changes low bits in f64.
        mass += isotope_mass;
        mass += f64::from(atom.hydrogens) * hydrogen.average;
        exact_mass += if atom.isotope == 0 {
            element.exact
        } else {
            isotope_mass
        };
        exact_mass -= ELECTRON_MASS * f64::from(atom.charge);
    }
    exact_mass += f64::from(hydrogens) * hydrogen.exact;
    if hydrogens != 0 {
        *counts.entry("H").or_default() += hydrogens;
    }
    let mut formula = String::new();
    let mut append = |symbol: &str, count: u32| {
        formula.push_str(symbol);
        if count > 1 {
            formula.push_str(&count.to_string());
        }
    };
    // RDKit puts H immediately after C, even when no carbon is present.
    for symbol in ["C", "H"] {
        if let Some(count) = counts.remove(symbol) {
            append(symbol, count);
        }
    }
    for (symbol, count) in counts {
        append(symbol, count);
    }
    if charge != 0 {
        formula.push(if charge > 0 { '+' } else { '-' });
        if charge.unsigned_abs() > 1 {
            formula.push_str(&charge.unsigned_abs().to_string());
        }
    }
    Ok(Properties {
        formula,
        mass,
        exact_mass,
        unpaired_electrons: radicals,
    })
}

/// Complete the private worker response before exposing the public engine API.
pub(crate) fn complete_analysis(result: &mut serde_json::Value) -> Result<(), String> {
    let Some(analysis) = result.get_mut("analysis").filter(|a| !a.is_null()) else {
        // Figure-only and reaction-export responses may have no analysis.
        return Ok(());
    };
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        rdkit_version: String,
        graph: graph::Graph,
        // Temporary fallback for platform-dependent legacy ring pruning.
        reference_rings: Vec<Vec<usize>>,
    }
    let input: Input = serde_json::from_value(
        analysis
            .get("property_input")
            .ok_or("Missing molecular property input")?
            .clone(),
    )
    .map_err(|error| format!("Invalid molecular property input: {error}"))?;
    if input.rdkit_version != RDKIT_VERSION {
        return Err(format!(
            "Molecular property data requires RDKit {RDKIT_VERSION}; found {}. Restore the locked chemistry environment.",
            input.rdkit_version
        ));
    }
    let ring_atoms = match rings::perceive(&input.graph, rings::Options::default()) {
        Ok(rings) => rings.atoms,
        Err(rings::RingError::UnresolvedOrdering) => input.reference_rings,
        Err(error) => return Err(error.to_string()),
    };
    let descriptors = descriptors::calculate(&input.graph, &ring_atoms)?;
    let ring_count =
        u32::try_from(ring_atoms.len()).map_err(|_| "Ring count exceeds supported range")?;
    let mut derived = serde_json::to_value(properties(&input.graph.atom_facts()?)?)
        .map_err(|error| format!("Invalid molecular properties: {error}"))?;
    let fields = derived
        .as_object_mut()
        .ok_or("Invalid molecular properties")?;
    fields.insert("rings".into(), ring_count.into());
    fields.insert("logp".into(), descriptors.logp.into());
    fields.insert("tpsa".into(), descriptors.tpsa.into());
    fields.insert("donors".into(), descriptors.donors.into());
    fields.insert("acceptors".into(), descriptors.acceptors.into());
    let fields = derived.as_object().ok_or("Invalid molecular properties")?;
    let analysis = analysis
        .as_object_mut()
        .ok_or("Invalid chemistry analysis")?;
    analysis.remove("property_input");
    analysis.extend(fields.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn worker_completion_is_atomic_and_requires_matching_data() -> Result<(), String> {
        let input = json!({"rdkit_version": RDKIT_VERSION, "reference_rings": [[0, 1, 2]], "graph": {"atoms": [{
            "atomic_number": 6, "isotope": 0, "charge": 0,
            "explicit_hydrogens": 0, "radical_electrons": 0,
            "no_implicit": false, "aromatic": false,
        }], "bonds": []}});
        let mut response = json!({"analysis": {"smiles": "C", "property_input": input}});
        complete_analysis(&mut response)?;
        assert_eq!(response["analysis"]["formula"], "CH4");
        assert_eq!(response["analysis"]["mass"], 16.043);
        // Ordinary graphs use the Rust result, not the reference fallback.
        assert_eq!(response["analysis"]["rings"], 0);
        assert_eq!(response["analysis"]["smiles"], "C");
        assert!(response["analysis"].get("property_input").is_none());
        for mut bad_input in [
            json!(null),
            json!({"rdkit_version": "different", "graph": {"atoms": [], "bonds": []}}),
            json!({"rdkit_version": RDKIT_VERSION, "graph": {"atoms": [{"atomic_number": 6}]}}),
            json!({"rdkit_version": RDKIT_VERSION, "graph": {"atoms": [{
                "atomic_number": 119, "isotope": 0, "charge": 0,
                "explicit_hydrogens": 0, "radical_electrons": 0,
                "no_implicit": false, "aromatic": false,
            }], "bonds": []}}),
            // Reject stale transport payloads and impossible graph valences.
            json!({"rdkit_version": RDKIT_VERSION, "atoms": []}),
            json!({"rdkit_version": RDKIT_VERSION, "graph": {"atoms": [{
                "atomic_number": 6, "explicit_hydrogens": 5, "isotope": 0, "charge": 0,
                "radical_electrons": 0, "no_implicit": false, "aromatic": false,
            }], "bonds": []}}),
        ] {
            if let Some(input) = bad_input.as_object_mut() {
                input.insert("reference_rings".into(), json!([]));
            }
            let mut bad = json!({"analysis": {"smiles": "C", "property_input": bad_input}});
            let original = bad.clone();
            assert!(complete_analysis(&mut bad).is_err());
            assert_eq!(bad, original);
        }
        assert!(complete_analysis(&mut response).is_err()); // Missing facts, no silent fallback.
        for mut no_analysis in [json!({}), json!({"analysis": null})] {
            complete_analysis(&mut no_analysis)?;
        }
        Ok(())
    }

    #[test]
    fn unresolved_ring_ordering_retains_reference_and_missing_fallback_is_rejected()
    -> Result<(), String> {
        let graph: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ring-order-dependent.json"
        ))
        .map_err(|e| e.to_string())?;
        let input = json!({"rdkit_version": RDKIT_VERSION, "reference_rings": [[0, 8, 16]], "graph": graph});
        let mut response = json!({"analysis": {"property_input": input}});
        complete_analysis(&mut response)?;
        assert_eq!(response["analysis"]["rings"], 1);
        assert!(response["analysis"].get("property_input").is_none());
        for bad in [
            json!(null),
            json!(-1),
            json!(1.5),
            json!(4294967296u64),
            json!([[0, 999, 16]]),
            json!([[0, 0, 0]]),
        ] {
            let mut input = input.clone();
            input["reference_rings"] = bad;
            let mut response = json!({"analysis": {"property_input": input}});
            let before = response.clone();
            assert!(complete_analysis(&mut response).is_err());
            assert_eq!(response, before);
        }
        let mut input = input;
        input
            .as_object_mut()
            .ok_or("Missing fixture")?
            .remove("reference_rings");
        let mut response = json!({"analysis": {"property_input": input}});
        assert!(complete_analysis(&mut response).is_err());
        Ok(())
    }
}
