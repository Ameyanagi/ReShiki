//! Molecular preparation, drawing reconstruction and properties in safe Rust.
//!
//! Migrated operations use local sanitization and stereo perception. Remaining
//! imports, full CIP labels and identifiers still use the backend. Never use cached
//! drawing labels: they may predate the latest edit.
//! Mass/formula semantics adapted from RDKit MolProps.cpp and Atom::getMass.
//! Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]

#[cfg(test)]
pub(crate) use reshiki_process_heap::allocation_metrics;
#[cfg(test)]
#[global_allocator]
static ALLOCATOR: allocation_metrics::MeasuredAllocator<std::alloc::System> =
    allocation_metrics::MeasuredAllocator::new(std::alloc::System);

pub mod aromaticity;
mod atomic_data;
pub mod cx;
pub mod depict;
pub mod descriptors;
pub mod electronic;
pub mod graph;
pub mod hydrogens;
pub mod inchi;
pub mod kekulize;
mod native_order;
pub mod normalize;
pub mod ranking;
pub mod rings;
pub mod sanitize;
pub mod smarts;
pub mod smiles;
pub mod stereo;
#[doc(hidden)]
pub mod windows_trigonometry;

use atomic_data::ELECTRON_MASS;
pub use atomic_data::{ELEMENTS, ISOTOPES, RDKIT_VERSION};
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

pub struct Element {
    pub symbol: &'static str,
    average: f64,
    exact: f64,
    pub common_isotope: u16,
    outer_electrons: i32,
    pub valences: &'static [i32],
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
#[cfg(any(test, feature = "rdkit-reference"))]
pub fn complete_analysis(result: &mut serde_json::Value) -> Result<(), String> {
    let Some(analysis) = result.get_mut("analysis").filter(|a| !a.is_null()) else {
        // Figure-only and reaction-export responses may have no analysis.
        return Ok(());
    };
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        rdkit_version: String,
        graph: graph::Graph,
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
    if analysis.get("inchikey").is_some() {
        return Err("Unexpected native InChIKey".into());
    }
    let inchi = analysis
        .get("inchi")
        .and_then(serde_json::Value::as_str)
        .ok_or("Missing molecular InChI input")?;
    // Empty molecules and unsupported identifier chemistry keep empty keys.
    // Every nonempty identifier must pass the bounded Rust key parser.
    let inchikey = if inchi.is_empty() {
        String::new()
    } else {
        inchi::key::from_inchi(inchi).map_err(|e| format!("Invalid molecular InChI: {e}"))?
    };
    let ring_atoms = rings::perceive(&input.graph, rings::Options::default())
        .map_err(|e| e.to_string())?
        .atoms;
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
    fields.insert("inchikey".into(), inchikey.into());
    let fields = derived.as_object().ok_or("Invalid molecular properties")?;
    let analysis = analysis
        .as_object_mut()
        .ok_or("Invalid chemistry analysis")?;
    analysis.remove("property_input");
    analysis.extend(fields.clone());
    Ok(())
}

#[cfg(test)]
mod tests;
