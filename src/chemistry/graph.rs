//! Bounded chemical graph, explicit valence, implicit H and radical assignment.
//!
//! These are property-cache operations, not a replacement for sanitization:
//! resonance cleanup, kekulization and aromaticity run before the final cache.
//! Adapted from RDKit Atom.cpp, Bond.cpp and MolOps::assignRadicals (2026.03.6).
//! Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{AtomFacts, ELEMENTS, Element};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Atom {
    pub atomic_number: u8,
    pub isotope: u16,
    pub charge: i8,
    pub explicit_hydrogens: u8,
    pub no_implicit: bool,
    pub aromatic: bool,
    pub radical_electrons: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bond {
    /// Dense atom indices; dative bonds run from donor `a` to acceptor `b`.
    pub a: usize,
    pub b: usize,
    /// Same order codes as the drawing: 0 hydrogen, 4 aromatic, 5 dative,
    /// 6 quadruple and 7 nonaromatic partial (1.5).
    pub order: u8,
    pub aromatic: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Valence {
    pub explicit_valence: u32,
    pub implicit_hydrogens: u32,
}

struct Environment {
    // Half-bond units retain partial/aromatic valence exactly without rounding.
    twice_valence: i32,
    degree: usize,
    aromatic: bool,
}

fn element(number: u8) -> Result<&'static Element, String> {
    ELEMENTS
        .get(usize::from(number))
        .ok_or_else(|| "Unknown atomic number".into())
}

impl Atom {
    fn effective_number(&self, original: &Element) -> u8 {
        if original.valences == [-1] {
            self.atomic_number
        } else {
            // RDKit's strict property cache clamps the effective atomic number.
            (i32::from(self.atomic_number) - i32::from(self.charge)).clamp(0, 118) as u8
        }
    }

    fn hypervalent(&self, effective: u8) -> bool {
        (effective > 16 && matches!(self.atomic_number, 15 | 16))
            || (effective > 34 && matches!(self.atomic_number, 33 | 34))
    }

    fn valence(&self, env: &Environment, strict: bool) -> Result<Valence, String> {
        let original = element(self.atomic_number)?;
        let effective_number = self.effective_number(original);
        let effective = element(effective_number)?;
        let default = *effective
            .valences
            .first()
            .ok_or("Missing default valence")?;
        let original_max = *original.valences.last().ok_or("Missing maximum valence")?;
        let mut twice = env.twice_valence;
        if default >= 0 && twice > default * 2 && env.aromatic {
            let mut previous = default;
            for &valence in effective.valences {
                if valence < 0 || valence * 2 > twice {
                    break;
                }
                previous = valence;
            }
            if twice - previous * 2 <= 3 {
                twice = previous * 2;
            }
        }
        // RDKit adds 0.1 then rounds; for nonnegative half units this is ceil.
        let explicit = (twice + 1) / 2;
        let mut maximum = *effective.valences.last().ok_or("Missing maximum valence")?;
        let mut offset = 0;
        if self.hypervalent(effective_number) {
            maximum = original_max;
            offset = -i32::from(self.charge);
        }
        if self.atomic_number == 1 && self.charge == -1 {
            maximum = 2;
        }
        if strict && maximum >= 0 && original_max >= 0 && explicit + offset > maximum {
            return Err(format!(
                "Explicit valence {explicit} is too large for {}",
                original.symbol
            ));
        }
        let implicit =
            self.implicit_valence(env.aromatic, explicit, original, effective_number, strict)?;
        Ok(Valence {
            explicit_valence: explicit as u32,
            implicit_hydrogens: implicit as u32,
        })
    }

    fn implicit_valence(
        &self,
        aromatic: bool,
        explicit: i32,
        original: &Element,
        effective_number: u8,
        strict: bool,
    ) -> Result<i32, String> {
        if self.no_implicit || self.atomic_number == 0 {
            return Ok(0);
        }
        if explicit == 0 && self.radical_electrons == 0 && self.atomic_number == 1 {
            return match self.charge {
                -1 | 1 => Ok(0),
                0 => Ok(1),
                _ if strict => Err("Unreasonable formal charge on hydrogen".into()),
                _ => Ok(0),
            };
        }
        if effective_number == 0 {
            return Ok(0);
        }
        let effective = element(effective_number)?;
        let default = *effective
            .valences
            .first()
            .ok_or("Missing default valence")?;
        if default == -1 {
            return Ok(0);
        }
        let mut explicit_radical = explicit + i32::from(self.radical_electrons);
        let valences = if self.hypervalent(effective_number) {
            explicit_radical -= i32::from(self.charge);
            original.valences
        } else {
            effective.valences
        };
        if aromatic {
            if explicit_radical <= default {
                return Ok(default - explicit_radical);
            }
            if valences
                .iter()
                .take_while(|&&v| v > 0)
                .any(|&v| v == explicit_radical)
            {
                return Ok(0);
            }
            return if strict {
                Err("Aromatic atom has no matching allowed valence".into())
            } else {
                Ok(0)
            };
        }
        if let Some(&allowed) = valences
            .iter()
            .take_while(|&&v| v >= 0)
            .find(|&&v| explicit_radical <= v)
        {
            return Ok(allowed - explicit_radical);
        }
        if strict
            && valences.last() != Some(&-1)
            && original.valences.last().is_some_and(|&v| v > 0)
        {
            return Err(format!(
                "Valence including radicals is too large for {}",
                original.symbol
            ));
        }
        Ok(0)
    }
}

impl Graph {
    /// Validate topology and field ranges without imposing chemical valences.
    pub fn validate(&self) -> Result<(), String> {
        self.environments().map(|_| ())
    }

    fn environments(&self) -> Result<Vec<Environment>, String> {
        if self.atoms.len() > 100_000 || self.bonds.len() > 300_000 {
            return Err("Chemical graph exceeds 100,000 atoms or 300,000 bonds".into());
        }
        let mut environments = self
            .atoms
            .iter()
            .map(|a| {
                element(a.atomic_number)?;
                Ok(Environment {
                    twice_valence: i32::from(a.explicit_hydrogens) * 2,
                    degree: 0,
                    aromatic: a.aromatic,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut seen = HashSet::new();
        for bond in &self.bonds {
            if bond.a == bond.b || !seen.insert((bond.a.min(bond.b), bond.a.max(bond.b))) {
                return Err("Self bond or duplicate chemical bond".into());
            }
            let (a, b) = match bond.order {
                0 => (0, 0),
                1..=3 => (i32::from(bond.order) * 2, i32::from(bond.order) * 2),
                4 | 7 => (3, 3),
                5 => (0, 2),
                6 => (8, 8),
                _ => return Err("Unsupported chemical bond order".into()),
            };
            for (index, twice) in [(bond.a, a), (bond.b, b)] {
                let env = environments
                    .get_mut(index)
                    .ok_or("Missing chemical bond endpoint")?;
                env.twice_valence += twice;
                env.degree += 1;
                env.aromatic |= bond.aromatic || bond.order == 4;
            }
        }
        Ok(environments)
    }

    /// Strict RDKit property-cache semantics. This does not perceive aromaticity
    /// or normalize functional groups: callers must supply that chemistry first.
    pub fn valences(&self) -> Result<Vec<Valence>, String> {
        self.calculate_valences(true)
    }

    /// Intermediate cache used during sanitization. Topology and field limits
    /// still apply, but excessive valences are retained for normalization.
    /// This must never replace the final strict valence check.
    pub fn provisional_valences(&self) -> Result<Vec<Valence>, String> {
        self.calculate_valences(false)
    }

    fn calculate_valences(&self, strict: bool) -> Result<Vec<Valence>, String> {
        let environments = self.environments()?;
        self.atoms
            .iter()
            .zip(environments)
            .enumerate()
            .map(|(i, (atom, env))| {
                atom.valence(&env, strict)
                    .map_err(|e| format!("Atom {}: {e}", i + 1))
            })
            .collect()
    }

    /// RDKit's radical-assignment pass, to be called after kekulization. Return
    /// complete results without mutating the input if any field is invalid.
    pub fn assign_radicals(&self) -> Result<Vec<u8>, String> {
        let environments = self.environments()?;
        self.atoms
            .iter()
            .zip(environments)
            .map(|(atom, env)| {
                if !atom.no_implicit || atom.atomic_number == 0 {
                    return Ok(atom.radical_electrons);
                }
                let element = element(atom.atomic_number)?;
                let charge = i32::from(atom.charge);
                let outer = element.outer_electrons;
                let radicals = if element.valences != [-1] {
                    let total = env.twice_valence / 2;
                    let base = if atom.atomic_number <= 2 { 2 } else { 8 };
                    let mut radicals = base - outer - total + charge;
                    if radicals < 0 {
                        radicals = 0;
                        if element.valences.len() > 1
                            && let Some(&valence) =
                                element.valences.iter().find(|&&v| v - total + charge >= 0)
                        {
                            radicals = valence - total + charge;
                        }
                    }
                    let earlier = outer - total - charge;
                    if earlier >= 0 {
                        radicals = radicals.min(earlier);
                    }
                    radicals
                } else if env.degree > 0 {
                    0
                } else {
                    (outer - charge).max(0) % 2
                };
                u8::try_from(radicals)
                    .map_err(|_| "Radical count is outside the supported range".into())
            })
            .collect()
    }

    /// Derive attached H counts from the graph, never from cached drawing labels.
    pub fn atom_facts(&self) -> Result<Vec<AtomFacts>, String> {
        self.atoms
            .iter()
            .zip(self.valences()?)
            .map(|(a, v)| {
                let hydrogens = u32::from(a.explicit_hydrogens) + v.implicit_hydrogens;
                Ok(AtomFacts {
                    atomic_number: a.atomic_number,
                    isotope: a.isotope,
                    charge: a.charge,
                    hydrogens: u8::try_from(hydrogens)
                        .map_err(|_| "Too many attached hydrogens")?,
                    radical_electrons: a.radical_electrons,
                })
            })
            .collect()
    }
}
