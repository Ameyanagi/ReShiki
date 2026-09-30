//! Toolkit adapter for the pure Rust `cosmolkit-inchi` dependency.
//!
//! The dependency owns InChI and its RDKit adapter. ReShiki supplies the same
//! property cache, Kekulé assignment, hydrogen removal and stereo perception
//! used by the rest of the application. No C library or foreign calls are used.
use super::{input, output};
use crate::chemistry::{
    ELEMENTS,
    electronic::Hybridization,
    graph::{self, Graph, Valence},
    kekulize::{self, Direction},
    ranking::Metadata,
    rings,
    stereo::{
        self, Point3,
        perception::{self, Properties, RingCache, RingKind, State},
    },
};
use cosmolkit_inchi::{
    InchiAtom, InchiBond, InchiBondDirection as BD, InchiBondStereo as BS, InchiBondType as BT,
    InchiChiralTag as CT, InchiMolecule, InchiToMolToolkit, InchiToolkitError, MolToInchiToolkit,
};
use serde::{Deserialize, Serialize};

pub const VERSION: &str = "1.07.5";
pub const CRATE_VERSION: &str = "0.3.0";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Molecule {
    pub state: State,
    pub positions: Option<Vec<Point3>>,
}
impl Molecule {
    pub fn prepare(state: &State, positions: Option<&[Point3]>) -> Result<Self, input::Error> {
        // Retain the application's established validation and explicit bounds.
        input::prepare(state, positions)?;
        Ok(Self {
            state: state.clone(),
            positions: positions.map(<[Point3]>::to_vec),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generated {
    pub status: i32,
    pub inchi: String,
    pub message: String,
    pub log: String,
    pub auxiliary: String,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Imported {
    pub status: i32,
    pub message: String,
    pub log: String,
    pub state: Option<State>,
    pub unspecified_bonds: Vec<bool>,
    pub diagnostics: Vec<String>,
}

fn error(message: impl ToString) -> InchiToolkitError {
    InchiToolkitError {
        kind: "ReShiki chemistry",
        message: message.to_string(),
    }
}
fn at<T>(items: &[T], i: usize) -> Result<&T, InchiToolkitError> {
    items
        .get(i)
        .ok_or_else(|| error("Missing molecular annotation"))
}
fn integer<T: TryFrom<U>, U>(value: U) -> Result<T, InchiToolkitError> {
    T::try_from(value).map_err(|_| error("Molecular field exceeds its supported range"))
}
fn chiral(value: u8) -> Result<CT, InchiToolkitError> {
    at(
        &[
            CT::Unspecified,
            CT::TetrahedralCw,
            CT::TetrahedralCcw,
            CT::Other,
            CT::Tetrahedral,
            CT::Allene,
            CT::SquarePlanar,
            CT::TrigonalBipyramidal,
            CT::Octahedral,
        ],
        usize::from(value),
    )
    .copied()
}
fn bond_stereo(value: u8) -> Result<BS, InchiToolkitError> {
    at(
        &[
            BS::None,
            BS::Any,
            BS::Z,
            BS::E,
            BS::Cis,
            BS::Trans,
            BS::AtropCw,
            BS::AtropCcw,
        ],
        usize::from(value),
    )
    .copied()
}
fn direction(value: Direction) -> BD {
    match value {
        Direction::None => BD::None,
        Direction::Wedge => BD::BeginWedge,
        Direction::Hash => BD::BeginDash,
        Direction::Down => BD::EndDownRight,
        Direction::Up => BD::EndUpRight,
        Direction::EitherDouble => BD::EitherDouble,
        Direction::Unknown => BD::Unknown,
    }
}
fn from_direction(value: BD) -> Direction {
    match value {
        BD::None => Direction::None,
        BD::BeginWedge => Direction::Wedge,
        BD::BeginDash => Direction::Hash,
        BD::EndDownRight => Direction::Down,
        BD::EndUpRight => Direction::Up,
        BD::EitherDouble => Direction::EitherDouble,
        BD::Unknown => Direction::Unknown,
    }
}
fn to_adapter(
    state: &State,
    conformers: Vec<Vec<[f64; 3]>>,
    unspecified: &[bool],
) -> Result<InchiMolecule, InchiToolkitError> {
    state
        .graph
        .cached_valences(Some(&state.valences))
        .map_err(error)?;
    state.metadata.validate(&state.graph).map_err(error)?;
    let atoms = state
        .graph
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| {
            Ok(InchiAtom {
                atomic_number: a.atomic_number.into(),
                formal_charge: a.charge.into(),
                num_explicit_hydrogens: a.explicit_hydrogens.into(),
                is_aromatic: a.aromatic,
                isotope: a.isotope.into(),
                num_radical_electrons: a.radical_electrons.into(),
                no_implicit: a.no_implicit,
                chiral_tag: chiral(at(&state.metadata.atoms, i)?.chiral_tag)?,
                cip_rank: at(&state.properties.atoms, i)?.cip_rank,
                cached_explicit_valence: Some(integer(at(&state.valences, i)?.explicit_valence)?),
                cached_implicit_valence: Some(integer(at(&state.valences, i)?.implicit_hydrogens)?),
            })
        })
        .collect::<Result<Vec<_>, InchiToolkitError>>()?;
    let bonds = state
        .graph
        .bonds
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let kind = if unspecified.get(i).copied().unwrap_or(false) {
                BT::Unspecified
            } else {
                match b.order {
                    0 => BT::Hydrogen,
                    1 => BT::Single,
                    2 => BT::Double,
                    3 => BT::Triple,
                    4 => BT::Aromatic,
                    5 => BT::Dative,
                    6 => BT::Quadruple,
                    7 => BT::OneAndAHalf,
                    _ => return Err(error("Unsupported bond order")),
                }
            };
            let mut result = InchiBond::new(integer(b.a)?, integer(b.b)?, kind);
            result.is_aromatic = b.aromatic;
            result.direction = direction(*at(&state.directions, i)?);
            let meta = at(&state.metadata.bonds, i)?;
            result.stereo = bond_stereo(meta.stereo)?;
            result.stereo_atoms = meta
                .stereo_atoms
                .iter()
                .map(|&a| integer(a))
                .collect::<Result<_, _>>()?;
            Ok(result)
        })
        .collect::<Result<_, InchiToolkitError>>()?;
    InchiMolecule::try_from_graph(atoms, bonds, conformers).map_err(|e| error(format!("{e:?}")))
}

#[derive(Default)]
struct Toolkit {
    state: Option<State>,
    unspecified: Vec<bool>,
}
impl Toolkit {
    // Preserve caches, rings and computed properties between callbacks while
    // accepting graph edits performed by the dependency's own cleanup pass.
    fn refresh(&mut self, molecule: &InchiMolecule) -> Result<State, InchiToolkitError> {
        let atoms = molecule
            .atoms()
            .iter()
            .map(|a| {
                Ok(graph::Atom {
                    atomic_number: integer(a.atomic_number)?,
                    isotope: integer(a.isotope)?,
                    charge: integer(a.formal_charge)?,
                    explicit_hydrogens: integer(a.num_explicit_hydrogens)?,
                    no_implicit: a.no_implicit,
                    aromatic: a.is_aromatic,
                    radical_electrons: integer(a.num_radical_electrons)?,
                })
            })
            .collect::<Result<Vec<_>, InchiToolkitError>>()?;
        self.unspecified = molecule
            .bonds()
            .iter()
            .map(|b| b.bond_type == BT::Unspecified)
            .collect();
        let bonds = molecule
            .bonds()
            .iter()
            .map(|b| {
                Ok(graph::Bond {
                    a: integer(b.begin_atom_index())?,
                    b: integer(b.end_atom_index())?,
                    aromatic: b.is_aromatic,
                    order: match b.bond_type {
                        BT::Unspecified | BT::Hydrogen => 0,
                        BT::Single => 1,
                        BT::Double => 2,
                        BT::Triple => 3,
                        BT::Aromatic => 4,
                        BT::Dative => 5,
                        BT::Quadruple => 6,
                        BT::OneAndAHalf => 7,
                        _ => return Err(error("Unsupported InChI bond type")),
                    },
                })
            })
            .collect::<Result<Vec<_>, InchiToolkitError>>()?;
        let graph = Graph { atoms, bonds };
        graph.validate().map_err(error)?;
        let mut state = match &self.state {
            Some(state)
                if state.graph.atoms.len() == graph.atoms.len()
                    && state.graph.bonds.len() == graph.bonds.len() =>
            {
                state.clone()
            }
            _ => State {
                metadata: Metadata::unspecified(&graph),
                directions: vec![],
                valences: vec![],
                conjugated: vec![false; graph.bonds.len()],
                hybridizations: vec![Hybridization::Unspecified; graph.atoms.len()],
                rings: RingCache::default(),
                properties: Properties::unspecified(&graph),
                graph: graph.clone(),
            },
        };
        state.graph = graph;
        state.directions = molecule
            .bonds()
            .iter()
            .map(|b| from_direction(b.direction))
            .collect();
        state.valences = if molecule
            .atoms()
            .iter()
            .all(|a| a.cached_explicit_valence.is_some() && a.cached_implicit_valence.is_some())
        {
            molecule
                .atoms()
                .iter()
                .map(|a| {
                    Ok(Valence {
                        explicit_valence: integer(
                            a.cached_explicit_valence
                                .ok_or_else(|| error("Missing explicit valence"))?,
                        )?,
                        implicit_hydrogens: integer(
                            a.cached_implicit_valence
                                .ok_or_else(|| error("Missing implicit valence"))?,
                        )?,
                    })
                })
                .collect::<Result<_, InchiToolkitError>>()?
        } else {
            state.graph.provisional_valences().map_err(error)?
        };
        for ((a, meta), props) in molecule
            .atoms()
            .iter()
            .zip(&mut state.metadata.atoms)
            .zip(&mut state.properties.atoms)
        {
            meta.chiral_tag = a.chiral_tag as u8;
            props.cip_rank = a.cip_rank;
        }
        for (b, meta) in molecule.bonds().iter().zip(&mut state.metadata.bonds) {
            meta.stereo = b.stereo as u8;
            meta.stereo_atoms = b
                .stereo_atoms
                .iter()
                .map(|&i| integer(i))
                .collect::<Result<_, _>>()?;
        }
        Ok(state)
    }
    fn save(
        &mut self,
        molecule: &mut InchiMolecule,
        state: State,
    ) -> Result<(), InchiToolkitError> {
        *molecule = to_adapter(&state, molecule.conformers().to_vec(), &self.unspecified)?;
        self.state = Some(state);
        Ok(())
    }
    fn cache(
        &mut self,
        molecule: &mut InchiMolecule,
        strict: bool,
    ) -> Result<(), InchiToolkitError> {
        let mut state = self.refresh(molecule)?;
        state.valences = if strict {
            state.graph.valences()
        } else {
            state.graph.provisional_valences()
        }
        .map_err(error)?;
        self.save(molecule, state)
    }
    fn prepare(
        &mut self,
        molecule: &mut InchiMolecule,
        remove: bool,
    ) -> Result<(), InchiToolkitError> {
        let state = self.refresh(molecule)?;
        let result = output::prepare(
            &output::Assembly {
                state: Some(state),
                warnings: vec![],
                unspecified_bonds: self.unspecified.clone(),
            },
            output::Options {
                sanitize: true,
                remove_hydrogens: remove,
            },
        )
        .map_err(error)?;
        self.unspecified = result.unspecified_bonds;
        self.save(
            molecule,
            result
                .state
                .ok_or_else(|| error("Missing prepared molecule"))?,
        )
    }
}
impl MolToInchiToolkit for Toolkit {
    fn needs_update_property_cache(
        &mut self,
        molecule: &InchiMolecule,
    ) -> Result<bool, InchiToolkitError> {
        Ok(molecule
            .atoms()
            .iter()
            .any(|a| a.cached_explicit_valence.is_none() || a.cached_implicit_valence.is_none()))
    }
    fn update_property_cache(
        &mut self,
        molecule: &mut InchiMolecule,
        strict: bool,
    ) -> Result<(), InchiToolkitError> {
        self.cache(molecule, strict)
    }
    fn kekulize(
        &mut self,
        molecule: &mut InchiMolecule,
        mark_atoms_bonds: bool,
    ) -> Result<(), InchiToolkitError> {
        let mut state = self.refresh(molecule)?;
        if state.rings.kind == RingKind::None {
            let found = rings::perceive(&state.graph, rings::Options::default()).map_err(error)?;
            state.rings = RingCache {
                kind: RingKind::Basis,
                atoms: found.atoms.into_iter().take(found.basis_count).collect(),
            };
        }
        let assignment = kekulize::assign_cached(
            &state.graph,
            &state.rings.atoms,
            &state.directions,
            kekulize::Options {
                clear_aromaticity: mark_atoms_bonds,
                ..Default::default()
            },
            &state.valences,
        )
        .map_err(error)?;
        state.graph = assignment.assignment.graph;
        state.directions = assignment.assignment.directions;
        state.valences = assignment.cache;
        self.save(molecule, state)
    }
    fn element_symbol(&mut self, number: i32) -> Result<Vec<u8>, InchiToolkitError> {
        Ok(at(ELEMENTS, integer(number)?)?.symbol.as_bytes().to_vec())
    }
    fn atomic_weight(&mut self, number: i32) -> Result<f64, InchiToolkitError> {
        Ok(at(ELEMENTS, integer(number)?)?.average)
    }
    fn total_num_hydrogens(
        &mut self,
        molecule: &InchiMolecule,
        index: u32,
    ) -> Result<u32, InchiToolkitError> {
        let a = at(molecule.atoms(), integer(index)?)?;
        Ok(a.num_explicit_hydrogens
            + integer::<u32, _>(
                a.cached_implicit_valence
                    .ok_or_else(|| error("Missing hydrogen cache"))?,
            )?)
    }
    fn calc_implicit_valence(
        &mut self,
        molecule: &mut InchiMolecule,
        index: u32,
    ) -> Result<i32, InchiToolkitError> {
        let mut state = self.refresh(molecule)?;
        let refreshed = state
            .graph
            .refresh_implicit(&state.valences)
            .map_err(error)?;
        let i = integer(index)?;
        let value = *at(&refreshed, i)?;
        *state
            .valences
            .get_mut(i)
            .ok_or_else(|| error("Missing valence"))? = value;
        self.save(molecule, state)?;
        integer(value.implicit_hydrogens)
    }
    fn total_degree(
        &mut self,
        molecule: &InchiMolecule,
        index: u32,
    ) -> Result<u32, InchiToolkitError> {
        Ok(integer::<u32, _>(
            molecule
                .bonds()
                .iter()
                .filter(|b| b.begin_atom_index() == index || b.end_atom_index() == index)
                .count(),
        )? + self.total_num_hydrogens(molecule, index)?)
    }
}
impl InchiToMolToolkit for Toolkit {
    fn atomic_number(&mut self, symbol: &[u8]) -> Result<i32, InchiToolkitError> {
        integer(
            ELEMENTS
                .iter()
                .position(|e| e.symbol.as_bytes() == symbol)
                .ok_or_else(|| error("Unknown element symbol"))?,
        )
    }
    fn average_atomic_weight(&mut self, number: i32) -> Result<f64, InchiToolkitError> {
        self.atomic_weight(number)
    }
    fn update_property_cache(
        &mut self,
        molecule: &mut InchiMolecule,
        strict: bool,
    ) -> Result<(), InchiToolkitError> {
        self.cache(molecule, strict)
    }
    fn assign_atom_cip_ranks(
        &mut self,
        molecule: &mut InchiMolecule,
    ) -> Result<Vec<u32>, InchiToolkitError> {
        let mut state = self.refresh(molecule)?;
        let ranks = stereo::atom_priorities(&state.graph, &state.metadata).map_err(error)?;
        for (p, &rank) in state.properties.atoms.iter_mut().zip(&ranks) {
            p.cip_rank = Some(rank);
        }
        self.save(molecule, state)?;
        Ok(ranks)
    }
    fn remove_hydrogens(&mut self, molecule: &mut InchiMolecule) -> Result<(), InchiToolkitError> {
        self.prepare(molecule, true)
    }
    fn sanitize_molecule(&mut self, molecule: &mut InchiMolecule) -> Result<(), InchiToolkitError> {
        self.prepare(molecule, false)
    }
    fn synchronize_after_cleanup(
        &mut self,
        molecule: &InchiMolecule,
    ) -> Result<(), InchiToolkitError> {
        self.state = Some(self.refresh(molecule)?);
        Ok(())
    }
    fn assign_stereochemistry(
        &mut self,
        molecule: &mut InchiMolecule,
        clean_it: bool,
        force: bool,
    ) -> Result<(), InchiToolkitError> {
        let state = self.refresh(molecule)?;
        let state = perception::perceive(
            &state,
            perception::Options {
                clean: clean_it,
                force,
                flag_possible: false,
            },
        )
        .map_err(error)?;
        self.save(molecule, state)
    }
}
fn text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Run only in the isolated helper: the caller enforces time and heap limits.
pub fn generate(input: &Molecule) -> Result<Generated, String> {
    input::prepare(&input.state, input.positions.as_deref()).map_err(|e| e.to_string())?;
    let conformers = input
        .positions
        .as_ref()
        .map(|p| vec![p.iter().map(|p| [p.x, p.y, p.z]).collect()])
        .unwrap_or_default();
    let molecule = to_adapter(&input.state, conformers, &[]).map_err(|e| e.message)?;
    let mut toolkit = Toolkit {
        state: Some(input.state.clone()),
        unspecified: vec![],
    };
    let result =
        cosmolkit_inchi::mol_to_inchi(&mut toolkit, &molecule, None).map_err(|e| e.to_string())?;
    Ok(Generated {
        status: result.return_values.return_code,
        inchi: text(result.inchi)?,
        message: text(result.return_values.message)?,
        log: text(result.return_values.log)?,
        auxiliary: text(result.return_values.aux_info)?,
        diagnostics: result.diagnostics.into_iter().map(|d| d.message).collect(),
    })
}
/// Return the dependency's molecular result; raw C records are intentionally
/// not fabricated because the dependency does not expose that API.
pub fn read(inchi: &str, options: output::Options) -> Result<Imported, String> {
    let mut toolkit = Toolkit::default();
    let result = cosmolkit_inchi::mol_from_inchi(
        &mut toolkit,
        inchi.as_bytes(),
        options.sanitize,
        options.remove_hydrogens,
    )
    .map_err(|e| e.to_string())?;
    let state = if result.molecule.is_some() {
        toolkit.state
    } else {
        None
    };
    Ok(Imported {
        status: result.return_values.return_code,
        message: text(result.return_values.message)?,
        log: text(result.return_values.log)?,
        state,
        unspecified_bonds: toolkit.unspecified,
        diagnostics: result.diagnostics.into_iter().map(|d| d.message).collect(),
    })
}
