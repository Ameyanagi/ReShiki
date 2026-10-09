//! Versioned, native HOSE-style spherical environments and observed-shift lookup.
//!
//! The encoder is specific to ReShiki: CDK/OpenChemLib HOSE strings are not
//! interchangeable. Predictions are medians of experimental reference groups,
//! with longest-sphere fallback. Observed spread is not calibrated uncertainty.
use crate::{
    graph::{Atom, Bond, Graph},
    ranking::{self, Metadata},
    rings,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

pub const ENCODER: &str = "reshiki-hose-v1";
pub const METHOD: &str = "HOSE spherical environments · median · ReShiki v1";
pub const MAX_ATOMS: usize = 128;
pub const MIN_RADIUS: u8 = 2;
pub const MAX_RADIUS: u8 = 4;
pub const MIN_SUPPORT: u32 = 2;

pub fn identity_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Nucleus {
    H1,
    C13,
}
impl Nucleus {
    pub fn label(self) -> &'static str {
        match self {
            Self::H1 => "¹H",
            Self::C13 => "¹³C",
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::H1 => "1H",
            Self::C13 => "13C",
        }
    }
}

/// One proton group attached to an original atom, or one original carbon.
/// Explicit H drawing atoms are linked as well. No diastereotopic assignment is
/// inferred; multiple attached protons are an unresolved group, not one peak.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub parent: usize,
    pub explicit_hydrogens: Vec<usize>,
    pub count: u8,
    pub exchangeable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    pub support: u32,
    pub median: f64,
    pub standard_deviation: f64,
    pub minimum: f64,
    pub maximum: f64,
}
impl Statistics {
    /// Each input is one independent molecule's environment-group median.
    pub fn from_values(mut values: Vec<f64>) -> Result<Self, String> {
        if values.is_empty()
            || values.len() > 1_000_000
            || values.iter().any(|x| !x.is_finite() || x.abs() > 1_000.)
        {
            return Err("Invalid observed NMR values".into());
        }
        values.sort_by(f64::total_cmp);
        let n = values.len();
        let median = if n.is_multiple_of(2) {
            (*values.get(n / 2 - 1).ok_or("Missing shift")?
                + *values.get(n / 2).ok_or("Missing shift")?)
                / 2.
        } else {
            *values.get(n / 2).ok_or("Missing shift")?
        };
        let mean = values.iter().sum::<f64>() / n as f64;
        let variance = if n > 1 {
            values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64
        } else {
            0.
        };
        Ok(Self {
            support: u32::try_from(n).map_err(|_| "NMR support exceeds limit")?,
            median,
            standard_deviation: variance.sqrt(),
            minimum: *values.first().ok_or("Missing shift")?,
            maximum: *values.last().ok_or("Missing shift")?,
        })
    }
    pub fn validate(&self) -> bool {
        self.support > 0
            && [
                self.median,
                self.standard_deviation,
                self.minimum,
                self.maximum,
            ]
            .iter()
            .all(|x| x.is_finite())
            && self.standard_deviation >= 0.
            && self.minimum <= self.median
            && self.median <= self.maximum
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    pub site: Site,
    pub radius: Option<u8>,
    pub statistics: Option<Statistics>,
    pub limitation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Index {
    pub entries: BTreeMap<(Nucleus, u8, String), Statistics>,
}
impl Index {
    pub fn predict(
        &self,
        environment: &Environment,
        nucleus: Nucleus,
        deadline: Instant,
    ) -> Result<Vec<Prediction>, String> {
        let mut output = Vec::new();
        for site in environment.sites(nucleus)? {
            if site.exchangeable {
                output.push(Prediction {
                    site,
                    radius: None,
                    statistics: None,
                    limitation: Some(
                        "Exchangeable proton; solvent, concentration and exchange are not modeled"
                            .into(),
                    ),
                });
                continue;
            }
            let mut prediction = Prediction {
                site: site.clone(),
                radius: None,
                statistics: None,
                limitation: None,
            };
            for radius in (MIN_RADIUS..=MAX_RADIUS).rev() {
                let code = environment.code(&site, nucleus, radius, deadline)?;
                if let Some(statistics) = self
                    .entries
                    .get(&(nucleus, radius, code))
                    .filter(|s| s.support >= MIN_SUPPORT)
                {
                    prediction.radius = Some(radius);
                    prediction.statistics = Some(statistics.clone());
                    break;
                }
            }
            if prediction.statistics.is_none() {
                prediction.limitation = Some(
                    "No compatible environment with at least two independent reference molecules"
                        .into(),
                );
            }
            output.push(prediction);
        }
        Ok(output)
    }
}

pub struct Environment {
    graph: Graph,
    originals: Vec<usize>,
    original_to_heavy: BTreeMap<usize, usize>,
    hydrogens: Vec<Vec<usize>>,
}
impl Environment {
    /// Input must already be chemically prepared (aromaticity and valence).
    pub fn new(input: &Graph) -> Result<Self, String> {
        input.validate()?;
        if input.atoms.len() > MAX_ATOMS {
            return Err("NMR prediction supports at most 128 atoms".into());
        }
        if input.atoms.is_empty() {
            return Err("Select a molecular structure for NMR prediction".into());
        }
        if input.atoms.iter().any(|a| {
            !matches!(a.atomic_number, 1 | 6 | 7 | 8 | 9 | 15 | 16 | 17 | 35 | 53)
                || a.charge != 0
                || a.radical_electrons != 0
        }) || input.bonds.iter().any(|b| !(1..=4).contains(&b.order))
        {
            return Err("NMR supports neutral closed-shell organic molecules with ordinary bonds (H, C, N, O, F, P, S, Cl, Br, I)".into());
        }
        let facts = input.atom_facts()?;
        let originals: Vec<_> = input
            .atoms
            .iter()
            .enumerate()
            .filter(|(_, a)| a.atomic_number != 1)
            .map(|(i, _)| i)
            .collect();
        let original_to_heavy: BTreeMap<_, _> =
            originals.iter().enumerate().map(|(i, &a)| (a, i)).collect();
        let mut graph = Graph {
            atoms: Vec::new(),
            bonds: Vec::new(),
        };
        let mut hydrogens = vec![Vec::new(); originals.len()];
        for &index in &originals {
            let mut atom = input.atoms.get(index).ok_or("Missing atom")?.clone();
            atom.explicit_hydrogens = facts.get(index).ok_or("Missing atom facts")?.hydrogens;
            atom.no_implicit = true;
            graph.atoms.push(atom);
        }
        for bond in &input.bonds {
            match (
                original_to_heavy.get(&bond.a),
                original_to_heavy.get(&bond.b),
            ) {
                (Some(&a), Some(&b)) => graph.bonds.push(Bond {
                    a,
                    b,
                    ..bond.clone()
                }),
                (Some(&a), None) | (None, Some(&a)) => {
                    let hydrogen = if original_to_heavy.contains_key(&bond.a) {
                        bond.b
                    } else {
                        bond.a
                    };
                    let h = input.atoms.get(hydrogen).ok_or("Missing H")?;
                    if h.isotope != 0 && h.isotope != 1 {
                        return Err("Isotope-labeled hydrogen environments are not supported by this reference dataset".into());
                    }
                    let atom = graph.atoms.get_mut(a).ok_or("Missing parent")?;
                    atom.explicit_hydrogens = atom
                        .explicit_hydrogens
                        .checked_add(1)
                        .ok_or("Too many hydrogen sites")?;
                    hydrogens
                        .get_mut(a)
                        .ok_or("Missing H mapping")?
                        .push(hydrogen);
                }
                _ => return Err("Unattached hydrogen molecules are not supported".into()),
            }
        }
        if graph.atoms.iter().any(|a| {
            a.isotope != 0
                && a.isotope
                    != crate::ELEMENTS
                        .get(usize::from(a.atomic_number))
                        .map(|e| e.common_isotope)
                        .unwrap_or(0)
        }) {
            return Err(
                "Isotope-labeled environments are not supported by this reference dataset".into(),
            );
        }
        for atom in &mut graph.atoms {
            atom.isotope = 0;
        }
        Ok(Self {
            graph,
            originals,
            original_to_heavy,
            hydrogens,
        })
    }
    pub fn sites(&self, nucleus: Nucleus) -> Result<Vec<Site>, String> {
        self.graph
            .atoms
            .iter()
            .enumerate()
            .filter(|(_, a)| match nucleus {
                Nucleus::H1 => a.explicit_hydrogens != 0,
                Nucleus::C13 => a.atomic_number == 6,
            })
            .map(|(i, a)| {
                Ok(Site {
                    parent: *self.originals.get(i).ok_or("Missing atom mapping")?,
                    explicit_hydrogens: self.hydrogens.get(i).ok_or("Missing H mapping")?.clone(),
                    count: if nucleus == Nucleus::H1 {
                        a.explicit_hydrogens
                    } else {
                        1
                    },
                    exchangeable: nucleus == Nucleus::H1 && a.atomic_number != 6,
                })
            })
            .collect()
    }
    pub fn code(
        &self,
        site: &Site,
        nucleus: Nucleus,
        radius: u8,
        deadline: Instant,
    ) -> Result<String, String> {
        if Instant::now() > deadline {
            return Err("NMR prediction exceeded its work deadline".into());
        }
        if !(MIN_RADIUS..=MAX_RADIUS).contains(&radius) {
            return Err("Unsupported NMR sphere radius".into());
        }
        let parent = *self
            .original_to_heavy
            .get(&site.parent)
            .ok_or("Invalid NMR site")?;
        let mut graph = self.graph.clone();
        let root = if nucleus == Nucleus::H1 {
            let a = graph.atoms.get_mut(parent).ok_or("Missing H parent")?;
            a.explicit_hydrogens = a
                .explicit_hydrogens
                .checked_sub(1)
                .ok_or("Missing proton")?;
            let root = graph.atoms.len();
            graph.atoms.push(Atom {
                atomic_number: 1,
                no_implicit: true,
                ..Atom::default()
            });
            graph.bonds.push(Bond {
                a: root,
                b: parent,
                order: 1,
                aromatic: false,
            });
            root
        } else {
            parent
        };
        let mut adjacent = vec![Vec::new(); graph.atoms.len()];
        for b in &graph.bonds {
            adjacent
                .get_mut(b.a)
                .ok_or("Invalid sphere bond")?
                .push(b.b);
            adjacent
                .get_mut(b.b)
                .ok_or("Invalid sphere bond")?
                .push(b.a);
        }
        let mut distances = vec![u8::MAX; graph.atoms.len()];
        *distances.get_mut(root).ok_or("Missing root")? = 0;
        let mut pending = VecDeque::from([root]);
        while let Some(a) = pending.pop_front() {
            let d = *distances.get(a).ok_or("Missing distance")?;
            if d == radius {
                continue;
            }
            for &b in adjacent.get(a).ok_or("Missing neighbors")? {
                if *distances.get(b).ok_or("Missing distance")? == u8::MAX {
                    *distances.get_mut(b).ok_or("Missing distance")? = d + 1;
                    pending.push_back(b);
                }
            }
        }
        let old: Vec<_> = distances
            .iter()
            .enumerate()
            .filter(|(_, d)| **d <= radius)
            .map(|(i, _)| i)
            .collect();
        let indices: BTreeMap<_, _> = old.iter().enumerate().map(|(i, &a)| (a, i)).collect();
        let sub = Graph {
            atoms: old
                .iter()
                .map(|&a| graph.atoms.get(a).cloned().ok_or("Missing sphere atom"))
                .collect::<Result<_, _>>()?,
            bonds: graph
                .bonds
                .iter()
                .filter_map(|b| {
                    Some(Bond {
                        a: *indices.get(&b.a)?,
                        b: *indices.get(&b.b)?,
                        ..b.clone()
                    })
                })
                .collect(),
        };
        let mut meta = Metadata::unspecified(&sub);
        meta.atoms
            .get_mut(*indices.get(&root).ok_or("Missing sphere root")?)
            .ok_or("Missing sphere metadata")?
            .map_number = 1;
        let ring_atoms = rings::perceive(&sub, Default::default())
            .map_err(|e| e.to_string())?
            .atoms;
        let ranks = ranking::rank(
            &sub,
            &ring_atoms,
            &meta,
            ranking::Options {
                include_chirality: false,
                ..Default::default()
            },
        )?;
        let mut order: Vec<_> = (0..sub.atoms.len()).collect();
        order.sort_by_key(|&a| ranks.get(a).copied().unwrap_or(0));
        let canonical: BTreeMap<_, _> = order.iter().enumerate().map(|(i, &a)| (a, i)).collect();
        let mut code = format!("{ENCODER}|{}|{radius}|", nucleus.code());
        for a in order {
            let atom = sub.atoms.get(a).ok_or("Missing canonical atom")?;
            let original = *old.get(a).ok_or("Missing sphere original")?;
            code.push_str(&format!(
                "{}:{}:{}:{}:{};",
                atom.atomic_number,
                atom.explicit_hydrogens,
                u8::from(atom.aromatic),
                distances.get(original).ok_or("Missing shell")?,
                u8::from(original == root)
            ));
        }
        let mut bonds: Vec<_> = sub
            .bonds
            .iter()
            .map(|b| {
                let a = *canonical.get(&b.a).ok_or("Missing canonical bond")?;
                let c = *canonical.get(&b.b).ok_or("Missing canonical bond")?;
                Ok((a.min(c), a.max(c), if b.aromatic { 4 } else { b.order }))
            })
            .collect::<Result<_, String>>()?;
        bonds.sort_unstable();
        for (a, b, o) in bonds {
            code.push_str(&format!("{a}-{b}:{o};"));
        }
        if Instant::now() > deadline {
            return Err("NMR prediction exceeded its work deadline".into());
        }
        Ok(format!("{:x}", Sha256::digest(code.as_bytes())))
    }
    pub fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(3)
    }
}

#[cfg(test)]
mod tests;
