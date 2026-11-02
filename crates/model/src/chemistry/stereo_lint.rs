//! Advisory checks for stereochemical drawing conventions. These inspect a
//! detached perception result and never change how the chemistry is read.
use super::{
    document::{self, Molecule},
    kekulize::Direction,
    ranking::Metadata,
    stereo::{self, perception, wedging},
};
use crate::document::{Bond, Document};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    NoEffect,
    Reversed,
    Ambiguous,
    Conflicting,
    Undefined,
    BetweenCentres,
    MixedHashes,
    NonWedgeOnly,
    WavyAlkeneCentre,
    ReEntrant,
    TShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub kind: Kind,
    pub atoms: Vec<u64>,
    pub bonds: Vec<(u64, u64)>,
    pub message: String,
}

/// Unspecified stereogenic units, excluding explicit unknown (wavy/any) units.
/// IDs and endpoint pairs retain document order, independently of atom maps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Undefined {
    pub atoms: Vec<u64>,
    pub bonds: Vec<(u64, u64)>,
}

impl Undefined {
    fn document_order(&mut self, document: &Document) {
        let atoms: HashSet<_> = self.atoms.drain(..).collect();
        let bonds: HashSet<_> = self
            .bonds
            .drain(..)
            .map(|(a, b)| (a.min(b), a.max(b)))
            .collect();
        self.atoms.extend(
            document
                .atoms
                .iter()
                .filter_map(|a| atoms.contains(&a.id).then_some(a.id)),
        );
        self.bonds.extend(document.bonds.iter().filter_map(|b| {
            bonds
                .contains(&(b.a.min(b.b), b.a.max(b.b)))
                .then_some((b.a, b.b))
        }));
    }
}

/// Use the author's attached number when present, otherwise the element and
/// its one-based position in the complete drawing.
pub fn atom_name(document: &Document, id: u64) -> String {
    document
        .atoms
        .iter()
        .enumerate()
        .find(|(_, atom)| atom.id == id)
        .map(|(index, atom)| {
            atom.display.number.as_ref().map_or_else(
                || format!("{}{index}", atom.element, index = index + 1),
                |number| number.text.clone(),
            )
        })
        .unwrap_or_else(|| format!("atom {id}"))
}

fn stereobond(bond: &Bond) -> bool {
    !bond.projection
        && bond.order == 1
        && matches!(
            bond.display.as_str(),
            "wedge" | "hash" | "hollow_wedge" | "bold" | "hashed"
        )
}

fn unknown(bond: &Bond) -> bool {
    !bond.projection && (bond.display == "wavy" || bond.stereo.as_deref() == Some("any"))
}

fn direction(bond: &Bond) -> Direction {
    if stereobond(bond) {
        if matches!(bond.display.as_str(), "hash" | "hashed") {
            Direction::Hash
        } else {
            Direction::Wedge
        }
    } else if unknown(bond) {
        Direction::Unknown
    } else {
        Direction::None
    }
}

// Generic structures wait for expansion and a crystal's bonds come from
// distances: neither is checked. Parts carry chemistry only, not display.
fn components(source: &Document) -> impl Iterator<Item = Document> {
    let distance = crate::models3d::crystal::analysis_atoms(source);
    crate::atom_labels::refresh::components(source)
        .0
        .into_iter()
        .filter(move |part| !part.atoms.iter().any(|a| distance.contains(&a.id)))
}

struct Checked {
    source: Document,
    molecule: Molecule,
    possible: perception::State,
    adjacent: Vec<Vec<(usize, usize)>>,
    unspecified_bonds: HashSet<usize>,
}

impl Checked {
    fn new(source: Document) -> Option<Self> {
        let molecule = document::prepare(&source).ok()?;
        let mut input = molecule.state.clone();
        // Atom maps rank ligands; they must not make equivalent ones distinct.
        for atom in &mut input.properties.atoms {
            atom.cip_code = None;
            atom.cip_rank = None;
            atom.possible = None;
        }
        for atom in &mut input.metadata.atoms {
            atom.map_number = 0;
            atom.map_present = false;
        }
        let options = perception::Options {
            clean: false,
            force: true,
            flag_possible: true,
        };
        let possible = if source.bonds.iter().any(|b| b.stereo_authoritative) {
            let mask: Vec<_> = source
                .bonds
                .iter()
                .map(|b| b.stereo_authoritative)
                .collect();
            perception::perceive_preserving_bonds(&input, options, &mask)
        } else {
            perception::perceive(&input, options)
        }
        .ok()?;
        let mut adjacent = vec![Vec::new(); molecule.ids.len()];
        for (index, bond) in possible.graph.bonds.iter().enumerate() {
            adjacent.get_mut(bond.a)?.push((index, bond.b));
            adjacent.get_mut(bond.b)?.push((index, bond.a));
        }
        // Reuse the interchange predicate for crossed, unspecified alkenes.
        // It already excludes equivalent ligands, cumulenes and small rings.
        let ranks: Vec<_> = if possible
            .properties
            .atoms
            .iter()
            .all(|a| a.cip_rank.is_some())
        {
            possible
                .properties
                .atoms
                .iter()
                .map(|a| a.cip_rank)
                .collect()
        } else {
            stereo::atom_priorities(&possible.graph, &possible.metadata)
                .ok()?
                .into_iter()
                .map(Some)
                .collect()
        };
        let mut potential = possible.metadata.clone();
        for atom in &mut potential.atoms {
            atom.chiral_tag = 0;
        }
        for bond in &mut potential.bonds {
            // Geometry also uses "any" for degenerate XY. Only the source's
            // explicit unknown markers exclude a unit from this check.
            if bond.stereo <= 1 {
                bond.stereo = 0;
                bond.stereo_atoms.clear();
            }
        }
        let unspecified_bonds = wedging::file_bonds(
            &wedging::WedgeState {
                graph: possible.graph.clone(),
                metadata: potential,
                // Partial XY-derived alkene directions do not define a unit
                // whose perceived bond metadata remains unspecified.
                directions: possible
                    .directions
                    .iter()
                    .map(|&d| {
                        if matches!(d, Direction::Up | Direction::Down) {
                            Direction::None
                        } else {
                            d
                        }
                    })
                    .collect(),
                rings: possible.rings.clone(),
            },
            &wedging::WedgeProperties {
                valences: possible.valences.clone(),
                attachment_points: vec![false; molecule.ids.len()],
            },
            &wedging::Conformer {
                positions: molecule.positions.clone(),
                is_3d: false,
            },
            &ranks,
        )
        .ok()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, bond)| (bond.code == 3).then_some(index))
        .collect();
        Some(Self {
            source,
            molecule,
            possible,
            adjacent,
            unspecified_bonds,
        })
    }

    fn centre(&self, atom: usize) -> bool {
        self.possible
            .properties
            .atoms
            .get(atom)
            .is_some_and(|a| a.possible == Some(true))
    }

    fn explicit_unknown(&self, atom: usize) -> bool {
        self.adjacent
            .get(atom)
            .into_iter()
            .flatten()
            .any(|&(bond, _)| self.source.bonds.get(bond).is_some_and(unknown))
    }

    fn unspecified(&self) -> Undefined {
        let state = &self.possible;
        let atoms = self
            .molecule
            .ids
            .iter()
            .zip(&state.metadata.atoms)
            .enumerate()
            .filter_map(|(index, (&id, metadata))| {
                (self.centre(index) && metadata.chiral_tag == 0 && !self.explicit_unknown(index))
                    .then_some(id)
            })
            .collect();
        let bonds = state
            .graph
            .bonds
            .iter()
            .zip(&state.metadata.bonds)
            .enumerate()
            .filter_map(|(index, (bond, metadata))| {
                if bond.order != 2
                    || metadata.stereo > 1
                    || self.explicit_unknown(bond.a)
                    || self.explicit_unknown(bond.b)
                    || !self.unspecified_bonds.contains(&index)
                {
                    return None;
                }
                Some((
                    *self.molecule.ids.get(bond.a)?,
                    *self.molecule.ids.get(bond.b)?,
                ))
            })
            .collect();
        Undefined { atoms, bonds }
    }

    fn geometry(&self, directions: &[Direction]) -> Option<Metadata> {
        let mut metadata = self.molecule.state.metadata.clone();
        for atom in &mut metadata.atoms {
            atom.chiral_tag = 0;
        }
        stereo::from_directions(
            &self.molecule.state.graph,
            &metadata,
            directions,
            Some(&self.molecule.positions),
            true,
        )
        .ok()
        .map(|drawn| drawn.metadata)
    }

    fn reentrant(&self, atom: usize) -> bool {
        let positions = &self.molecule.positions;
        self.possible.rings.atoms.iter().any(|ring| {
            let Some(index) = ring.iter().position(|&a| a == atom) else {
                return false;
            };
            let n = ring.len();
            let Some((a, b, c)) = ring
                .get((index + n - 1) % n)
                .and_then(|a| positions.get(*a))
                .zip(positions.get(atom))
                .zip(ring.get((index + 1) % n).and_then(|a| positions.get(*a)))
                .map(|((a, b), c)| (a, b, c))
            else {
                return false;
            };
            let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
            let area: f64 = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .map(|(&a, &b)| {
                    positions
                        .get(a)
                        .zip(positions.get(b))
                        .map_or(0.0, |(a, b)| a.x * b.y - a.y * b.x)
                })
                .sum();
            cross * area < -1e-8
        })
    }

    fn tshape(&self, atom: usize) -> bool {
        let positions = &self.molecule.positions;
        let Some(center) = positions.get(atom) else {
            return false;
        };
        let plain: Vec<_> = self
            .adjacent
            .get(atom)
            .into_iter()
            .flatten()
            .filter_map(|&(index, other)| {
                let bond = self.source.bonds.get(index)?;
                if bond.order == 1 && bond.display == "plain" {
                    positions.get(other)
                } else {
                    None
                }
            })
            .collect();
        for (i, a) in plain.iter().enumerate() {
            for b in plain.iter().take(i) {
                let (ax, ay, bx, by) = (
                    a.x - center.x,
                    a.y - center.y,
                    b.x - center.x,
                    b.y - center.y,
                );
                let length = (ax * ax + ay * ay).sqrt() * (bx * bx + by * by).sqrt();
                if length > 0.0 && (ax * bx + ay * by) / length <= 170_f64.to_radians().cos() {
                    return true;
                }
            }
        }
        false
    }
}

fn checked(document: &Document) -> impl Iterator<Item = Checked> {
    components(document).filter_map(Checked::new)
}

/// Compute only when requested by a naming notice or the optional editor aid.
pub fn undefined(document: &Document) -> Undefined {
    let mut result = Undefined::default();
    for component in checked(document) {
        let units = component.unspecified();
        result.atoms.extend(units.atoms);
        result.bonds.extend(units.bonds);
    }
    result.document_order(document);
    result
}

/// Inspect ordinary connected molecules; unsupported or invalid components do
/// not conceal notices on independent molecules. Style advice is opt-in.
pub fn check(document: &Document, style: bool) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut missing = Undefined::default();
    let mut tapered = Vec::new();
    let mut parallel = Vec::new();
    for component in checked(document) {
        let source = &component.source;
        let ids = &component.molecule.ids;
        let directions: Vec<_> = source.bonds.iter().map(direction).collect();
        let geometry = component.geometry(&directions);
        let mut groups = vec![Vec::new(); ids.len()];
        for (index, (bond, graph_bond)) in source
            .bonds
            .iter()
            .zip(&component.possible.graph.bonds)
            .enumerate()
        {
            let (a, b) = (graph_bond.a, graph_bond.b);
            if stereobond(bond) {
                if let Some(group) = groups.get_mut(a) {
                    group.push(index);
                }
                if !component.centre(a) {
                    let valid_reverse = component.centre(b) && {
                        let mut reversed = component.molecule.state.graph.clone();
                        let Some(edge) = reversed.bonds.get_mut(index) else {
                            continue;
                        };
                        std::mem::swap(&mut edge.a, &mut edge.b);
                        let mut alone = vec![Direction::None; source.bonds.len()];
                        if let Some(target) = alone.get_mut(index) {
                            *target = direction(bond);
                        }
                        stereo::from_directions(
                            &reversed,
                            &Metadata::unspecified(&reversed),
                            &alone,
                            Some(&component.molecule.positions),
                            true,
                        )
                        .is_ok_and(|drawn| {
                            drawn
                                .metadata
                                .atoms
                                .get(b)
                                .is_some_and(|atom| atom.chiral_tag != 0)
                        })
                    };
                    let kind = if valid_reverse {
                        Kind::Reversed
                    } else {
                        Kind::NoEffect
                    };
                    let message = if valid_reverse {
                        format!(
                            "Probably reversed stereo bond: narrow end should be at {}",
                            atom_name(document, bond.b)
                        )
                    } else {
                        format!(
                            "Stereo bond at {} has no effect: its narrow end is not a stereocentre",
                            atom_name(document, bond.a)
                        )
                    };
                    notes.push(Note {
                        kind,
                        atoms: vec![],
                        bonds: vec![(bond.a, bond.b)],
                        message,
                    });
                }
                if style && component.centre(a) && component.centre(b) {
                    let message = if matches!(bond.display.as_str(), "hash" | "hashed") {
                        "Hashed bonds between two stereocentres are always ambiguous; redraw stereo bonds"
                    } else {
                        "Stereo bond between two stereocentres; redraw stereo bonds onto a substituent"
                    };
                    notes.push(Note {
                        kind: Kind::BetweenCentres,
                        atoms: vec![],
                        bonds: vec![(bond.a, bond.b)],
                        message: message.into(),
                    });
                }
                if style {
                    match bond.display.as_str() {
                        "hash" => tapered.push((bond.a, bond.b)),
                        "hashed" => parallel.push((bond.a, bond.b)),
                        _ => {}
                    }
                }
            }
            if style && bond.display == "wavy" && !bond.projection {
                let alkene = |atom: usize| {
                    component
                        .adjacent
                        .get(atom)
                        .into_iter()
                        .flatten()
                        .any(|&(i, _)| source.bonds.get(i).is_some_and(|bond| bond.order == 2))
                };
                if component.centre(a) && alkene(b) || component.centre(b) && alkene(a) {
                    notes.push(Note {
                        kind: Kind::WavyAlkeneCentre,
                        atoms: vec![],
                        bonds: vec![(bond.a, bond.b)],
                        message: "Wavy bond joins a double-bond atom to a stereocentre; \
                                  redraw stereo bonds and use another bond to indicate \
                                  unknown configuration"
                            .into(),
                    });
                }
            }
        }
        for (atom, (bonds, &id)) in groups.iter().zip(ids).enumerate() {
            if bonds.is_empty() || !component.centre(atom) || component.explicit_unknown(atom) {
                continue;
            }
            let named = atom_name(document, id);
            let selected: Vec<_> = bonds
                .iter()
                .filter_map(|&i| source.bonds.get(i).map(|b| (b.a, b.b)))
                .collect();
            let mut tags = HashSet::new();
            if bonds.len() > 1 {
                for &bond in bonds {
                    let mut alone = vec![Direction::None; source.bonds.len()];
                    if let Some(target) = alone.get_mut(bond).zip(directions.get(bond)) {
                        *target.0 = *target.1;
                    }
                    if let Some(metadata) = component.geometry(&alone) {
                        let tag = metadata.atoms.get(atom).map_or(0, |a| a.chiral_tag);
                        if tag != 0 {
                            tags.insert(tag);
                        }
                    }
                }
            }
            if tags.len() > 1 {
                notes.push(Note {
                    kind: Kind::Conflicting,
                    atoms: vec![id],
                    bonds: selected.clone(),
                    message: format!(
                        "Conflicting wedges at {named}: stereo bonds imply opposite configurations"
                    ),
                });
            } else if geometry
                .as_ref()
                .is_none_or(|metadata| metadata.atoms.get(atom).is_none_or(|a| a.chiral_tag == 0))
            {
                notes.push(Note {
                    kind: Kind::Ambiguous,
                    atoms: vec![id],
                    bonds: selected.clone(),
                    message: format!("Ambiguous wedge geometry at {named}"),
                });
            }
            if style {
                if component
                    .possible
                    .metadata
                    .atoms
                    .get(atom)
                    .is_some_and(|a| a.chiral_tag != 0)
                    && bonds.iter().all(|&i| {
                        source.bonds.get(i).is_some_and(|b| {
                            matches!(b.display.as_str(), "bold" | "hashed" | "hollow_wedge")
                        })
                    })
                {
                    notes.push(Note {
                        kind: Kind::NonWedgeOnly,
                        atoms: vec![id],
                        bonds: selected.clone(),
                        message: format!(
                            "Configuration at {named} uses only bold, parallel-hash or hollow \
                             bonds; redraw stereo bonds"
                        ),
                    });
                }
                for (condition, kind, message) in [
                    (
                        component.reentrant(atom),
                        Kind::ReEntrant,
                        format!(
                            "Stereo bond at re-entrant ring atom {named}; redraw stereo bonds or choose another stereochemical depiction"
                        ),
                    ),
                    (
                        component.tshape(atom),
                        Kind::TShape,
                        format!(
                            "T-shaped wedge geometry at {named}: two plain bonds are nearly linear; redraw stereo bonds"
                        ),
                    ),
                ] {
                    if condition {
                        notes.push(Note {
                            kind,
                            atoms: vec![id],
                            bonds: selected.clone(),
                            message,
                        });
                    }
                }
            }
        }
        let units = component.unspecified();
        missing.atoms.extend(units.atoms);
        missing.bonds.extend(units.bonds);
    }
    if style && !tapered.is_empty() && !parallel.is_empty() {
        tapered.extend(parallel);
        notes.push(Note {
            kind: Kind::MixedHashes,
            atoms: vec![],
            bonds: tapered,
            message: "Drawing mixes tapered and parallel hashes; use one hash style or redraw \
                      stereo bonds"
                .into(),
        });
    }
    let covered: HashSet<_> = notes
        .iter()
        .filter(|note| {
            matches!(
                note.kind,
                Kind::NoEffect | Kind::Reversed | Kind::Ambiguous | Kind::Conflicting
            )
        })
        .flat_map(|note| {
            note.atoms
                .iter()
                .copied()
                .chain(note.bonds.iter().flat_map(|&(a, b)| [a, b]))
        })
        .collect();
    missing.atoms.retain(|id| !covered.contains(id));
    if !missing.atoms.is_empty() || !missing.bonds.is_empty() {
        missing.document_order(document);
        let count = |n, singular, plural| format!("{n} {}", if n == 1 { singular } else { plural });
        let mut summaries = Vec::new();
        if !missing.atoms.is_empty() {
            summaries.push(format!(
                "{} ({})",
                count(
                    missing.atoms.len(),
                    "undefined stereocentre",
                    "undefined stereocentres"
                ),
                missing
                    .atoms
                    .iter()
                    .map(|&id| atom_name(document, id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !missing.bonds.is_empty() {
            summaries.push(format!(
                "{} ({})",
                count(
                    missing.bonds.len(),
                    "undefined stereogenic double bond",
                    "undefined stereogenic double bonds"
                ),
                missing
                    .bonds
                    .iter()
                    .map(|&(a, b)| format!("{}={}", atom_name(document, a), atom_name(document, b)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        notes.push(Note {
            kind: Kind::Undefined,
            atoms: missing.atoms,
            bonds: missing.bonds,
            message: summaries.join(", "),
        });
    }
    notes
}

/// Re-choose the stereo bonds on whole touched molecules. Only bond appearance
/// and the selected wedge's endpoint orientation change; all edits are detached
/// until every eligible component has passed a configuration round trip.
pub fn redraw(source: &Document, selected: &[u64]) -> Result<(Document, usize), document::Error> {
    let selected: HashSet<_> = selected.iter().copied().collect();
    let mut result = source.clone();
    let bond_indices: HashMap<_, _> = result
        .bonds
        .iter()
        .enumerate()
        .map(|(i, b)| ((b.a.min(b.b), b.a.max(b.b)), i))
        .collect();
    let mut count = 0;
    for part in components(source) {
        if !selected.is_empty() && !part.atoms.iter().any(|a| selected.contains(&a.id)) {
            continue;
        }
        let Ok(molecule) = document::prepare(&part) else {
            continue;
        };
        let state = &molecule.state;
        let directions = state
            .directions
            .iter()
            .map(|&d| {
                if matches!(d, Direction::Wedge | Direction::Hash) {
                    Direction::None
                } else {
                    d
                }
            })
            .collect();
        let wedged = wedging::wedge_molecule(
            &wedging::WedgeState {
                graph: state.graph.clone(),
                metadata: state.metadata.clone(),
                directions,
                rings: state.rings.clone(),
            },
            &wedging::WedgeProperties {
                valences: state.valences.clone(),
                attachment_points: vec![false; molecule.ids.len()],
            },
            Some(&wedging::Conformer {
                positions: molecule.positions.clone(),
                is_3d: false,
            }),
            source.drawing_style.two_wedge_stereocentres,
        )
        .map_err(document::Error::WedgeAssignment)?;
        let mut edited = part.clone();
        for ((bond, &direction), edge) in edited
            .bonds
            .iter_mut()
            .zip(&wedged.directions)
            .zip(&wedged.graph.bonds)
        {
            if bond.projection {
                continue;
            }
            if stereobond(bond) {
                bond.display = "plain".into();
            }
            if matches!(direction, Direction::Wedge | Direction::Hash) {
                // The chooser orients each new stereobond from its centre.
                if molecule.ids.get(edge.a) != Some(&bond.a) {
                    bond.reverse();
                }
                bond.display = if direction == Direction::Wedge {
                    "wedge"
                } else {
                    "hash"
                }
                .into();
            }
        }
        // Removing explicit winding here tests that the new visible wedges
        // themselves retain every assigned tetrahedral configuration.
        let mut visible = edited.clone();
        for atom in &mut visible.atoms {
            atom.stereo = None;
        }
        let after = document::prepare(&visible)?;
        if state
            .metadata
            .atoms
            .iter()
            .zip(&after.state.metadata.atoms)
            .any(|(a, b)| a.chiral_tag != b.chiral_tag)
            || state
                .metadata
                .bonds
                .iter()
                .zip(&after.state.metadata.bonds)
                .any(|(a, b)| a.stereo != b.stereo || a.stereo_atoms != b.stereo_atoms)
        {
            return Err(document::Error::WedgeAssignment(
                "Redrawn stereo bonds would change a configuration".into(),
            ));
        }
        count += state
            .metadata
            .atoms
            .iter()
            .filter(|a| matches!(a.chiral_tag, 1 | 2))
            .count();
        for bond in edited.bonds {
            if let Some(&index) = bond_indices.get(&(bond.a.min(bond.b), bond.a.max(bond.b)))
                && let Some(target) = result.bonds.get_mut(index)
            {
                if target.a != bond.a {
                    target.reverse();
                }
                target.display = bond.display;
            }
        }
    }
    Ok((result, count))
}
