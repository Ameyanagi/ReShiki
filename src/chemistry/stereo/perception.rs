//! Legacy stereo perception adapted from RDKit Chirality.cpp (2026.03.6).
//! Copyright (C) 2004-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
mod atoms;
mod bonds;
mod cycles;
#[cfg(test)]
mod tests;
use super::priority::{self, Work};
use crate::chemistry::{
    electronic::Hybridization,
    graph::{Graph, Valence},
    kekulize::Direction,
    ranking::Metadata,
    rings,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtomProperties {
    /// Computed legacy labels and flags, separate from structural winding.
    pub cip_code: Option<String>,
    pub cip_rank: Option<u32>,
    pub possible: Option<bool>,
    pub ring_candidate: Option<bool>,
    pub ring_members: Option<Vec<i32>>,
    pub unknown: bool,
    /// CX properties are untyped text. An invalid value matters only when
    /// stereo assignment actually reads it; it is not drawing metadata.
    #[serde(skip)]
    pub(crate) invalid_unknown: bool,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Properties {
    pub atoms: Vec<AtomProperties>,
    pub bond_codes: Vec<Option<String>>,
    /// Presence, including `Some(false)`, suppresses an unforced assignment.
    pub done: Option<bool>,
    pub needs_detection: Option<bool>,
}
impl Properties {
    pub fn unspecified(graph: &Graph) -> Self {
        Self {
            atoms: vec![AtomProperties::default(); graph.atoms.len()],
            bond_codes: vec![None; graph.bonds.len()],
            done: None,
            needs_detection: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RingKind {
    #[default]
    None,
    Fast,
    Basis,
    Symmetric,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RingCache {
    pub kind: RingKind,
    pub atoms: Vec<Vec<usize>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Graph and annotations captured together; caches must describe this graph.
pub struct State {
    pub graph: Graph,
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
    pub valences: Vec<Valence>,
    pub conjugated: Vec<bool>,
    pub hybridizations: Vec<Hybridization>,
    pub rings: RingCache,
    pub properties: Properties,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub clean: bool,
    pub force: bool,
    pub flag_possible: bool,
}

fn at<T>(v: &[T], i: usize) -> Result<&T, String> {
    v.get(i)
        .ok_or_else(|| "Invalid stereo perception index".into())
}
fn put<T>(v: &mut [T], i: usize, x: T) -> Result<(), String> {
    *v.get_mut(i).ok_or("Invalid stereo perception index")? = x;
    Ok(())
}
fn directed(d: Direction) -> bool {
    matches!(d, Direction::Up | Direction::Down)
}
fn opposite(d: Direction) -> Result<Direction, String> {
    match d {
        Direction::Up => Ok(Direction::Down),
        Direction::Down => Ok(Direction::Up),
        _ => Err("Invalid directional stereo bond".into()),
    }
}

/// Assign the pinned legacy atom/bond stereo and repair annotations as requested.
/// Legacy R/S and E/Z rules are approximate; full CIP labeling is separate.
/// Supplied caches must belong to this graph. Errors leave the input unchanged.
pub fn perceive(input: &State, options: Options) -> Result<State, String> {
    with_work(input, options, None, &mut Work(50_000_000))
}

/// Depiction supplies freshly perceived symmetric rings. Requiring that cache
/// excludes the independent fallback ring searches used by the public API.
pub(crate) fn perceive_prepared_with_work(
    input: &State,
    options: Options,
    remaining: &mut usize,
) -> Result<State, String> {
    if input.rings.kind != RingKind::Symmetric {
        return Err("Shared stereo work requires a symmetric ring cache".into());
    }
    let initial = (*remaining).min(50_000_000);
    let mut work = Work(initial);
    let result = (|| {
        work.spend(input.graph.atoms.len())?;
        work.spend(input.graph.bonds.len())?;
        // Charge before the detached copy, even when an existing done property
        // permits the native early return. Callers validated annotation sizes.
        for ring in &input.rings.atoms {
            work.spend(1)?;
            work.spend(ring.len())?;
        }
        for group in &input.metadata.groups {
            work.spend(1)?;
            work.spend(group.atoms.len())?;
            work.spend(group.bonds.len())?;
        }
        for properties in &input.properties.atoms {
            work.spend(1)?;
            work.spend(properties.ring_members.as_ref().map_or(0, Vec::len))?;
            work.spend(properties.cip_code.as_ref().map_or(0, String::len))?;
        }
        for label in &input.properties.bond_codes {
            work.spend(1)?;
            work.spend(label.as_ref().map_or(0, String::len))?;
        }
        with_work(input, options, None, &mut work)
    })();
    let used = initial
        .checked_sub(work.0)
        .ok_or("Stereo work accounting overflow")?;
    *remaining = remaining
        .checked_sub(used)
        .ok_or("Stereo work accounting overflow")?;
    result
}

/// FindStereo's center predicate differs from legacy perception for low-degree
/// phosphorus/arsenic. SMILES winding uses this predicate after perception.
pub(crate) fn potential_tetrahedral_centers(input: &State) -> Result<Vec<bool>, String> {
    input.graph.cached_valences(Some(&input.valences))?;
    input.metadata.validate(&input.graph)?;
    if input.hybridizations.len() != input.graph.atoms.len()
        || input.conjugated.len() != input.graph.bonds.len()
    {
        return Err("Invalid potential-center annotations".into());
    }
    let mut work = Work(50_000_000);
    let mut ctx = Context::new(input.clone(), &mut work)?;
    (0..input.graph.atoms.len())
        .map(|atom| ctx.potential_tetrahedral(atom, &mut work))
        .collect()
}
/// Exact atomic-number file queries remain queries during the first stereo pass.
/// Their coordinated H atoms do not contribute duplicate priority entries.
pub(crate) fn perceive_file_queries(
    input: &State,
    options: Options,
    queries: &[bool],
) -> Result<State, String> {
    with_work(input, options, Some(queries), &mut Work(50_000_000))
}
fn with_work(
    input: &State,
    options: Options,
    queries: Option<&[bool]>,
    work: &mut Work,
) -> Result<State, String> {
    input.graph.cached_valences(Some(&input.valences))?;
    input.metadata.validate(&input.graph)?;
    let (n, e) = (input.graph.atoms.len(), input.graph.bonds.len());
    if input.directions.len() != e
        || input.conjugated.len() != e
        || input.hybridizations.len() != n
        || input.properties.atoms.len() != n
        || input.properties.bond_codes.len() != e
        || queries.is_some_and(|q| q.len() != n)
    {
        return Err("Invalid stereo perception annotation count".into());
    }
    let mut stored = 0usize;
    for (props, meta) in input.properties.atoms.iter().zip(&input.metadata.atoms) {
        if props.cip_code.as_ref().is_some_and(|s| s.len() > 1024)
            || props.ring_members.is_some() != meta.ring_stereo
        {
            return Err("Invalid stereo perception atom properties".into());
        }
        if let Some(members) = &props.ring_members {
            stored = stored
                .checked_add(members.len())
                .ok_or("Stereo member storage overflow")?;
            if stored > 2_000_000
                || members
                    .iter()
                    .any(|&v| v == 0 || v.unsigned_abs() as usize > n)
            {
                return Err("Invalid stereo ring members".into());
            }
        }
    }
    if input
        .properties
        .bond_codes
        .iter()
        .flatten()
        .any(|s| s.len() > 1024)
    {
        return Err("Invalid stereo bond label".into());
    }
    let mut ctx = Context::new(input.clone(), work)?;
    ctx.queries = queries.map(<[bool]>::to_vec);
    if !options.force && ctx.state.properties.done.is_some() {
        return Ok(ctx.state);
    }
    ctx.state.properties.needs_detection = None;
    if ctx.state.rings.kind == RingKind::None {
        ctx.state.rings = RingCache {
            kind: RingKind::Fast,
            atoms: rings::fast(&ctx.state.graph)?.atoms,
        };
        ctx.index_rings(work)?;
    }
    let (mut has_atoms, mut potential_atoms) = (false, false);
    for i in 0..n {
        work.spend(1)?;
        if options.clean {
            let props = ctx
                .state
                .properties
                .atoms
                .get_mut(i)
                .ok_or("Missing stereo atom properties")?;
            props.cip_code = None;
            props.possible = None;
        }
        let tag = at(&ctx.state.metadata.atoms, i)?.chiral_tag;
        if !has_atoms && !matches!(tag, 0 | 3) {
            has_atoms = true;
        } else if !potential_atoms {
            potential_atoms = ctx.legal_center(i, work)?;
        }
    }
    let (mut has_bonds, mut potential_bonds) = (false, false);
    for i in 0..e {
        work.spend(1)?;
        let order = at(&ctx.state.graph.bonds, i)?.order;
        if options.clean {
            put(&mut ctx.state.properties.bond_codes, i, None)?;
            if matches!(order, 2 | 4) && !ctx.detectable(i)? {
                if *at(&ctx.state.directions, i)? == Direction::EitherDouble {
                    put(&mut ctx.state.directions, i, Direction::None)?;
                }
                let meta = ctx
                    .state
                    .metadata
                    .bonds
                    .get_mut(i)
                    .ok_or("Missing stereo bond")?;
                if meta.stereo != 0 {
                    meta.stereo = 0;
                    meta.stereo_atoms.clear();
                }
                continue;
            } else if order == 2 {
                let dir = *at(&ctx.state.directions, i)?;
                let meta = ctx
                    .state
                    .metadata
                    .bonds
                    .get_mut(i)
                    .ok_or("Missing stereo bond")?;
                if dir == Direction::EitherDouble {
                    meta.stereo = 1;
                    meta.stereo_atoms.clear();
                    put(&mut ctx.state.directions, i, Direction::None)?;
                } else if meta.stereo != 1 {
                    meta.stereo = 0;
                    meta.stereo_atoms.clear();
                }
            }
        }
        if !has_bonds && order == 2 {
            let bond = at(&ctx.state.graph.bonds, i)?;
            let mut specified = false;
            for atom in [bond.a, bond.b] {
                for &other in at(&ctx.neighbors, atom)? {
                    work.spend(1)?;
                    if directed(*at(&ctx.state.directions, other)?) {
                        has_bonds = true;
                        specified = true;
                        break;
                    }
                }
                if has_bonds {
                    break;
                }
            }
            if !potential_bonds && !specified && ctx.detectable(i)? {
                potential_bonds = true;
            }
        }
        if !options.clean && has_bonds && potential_bonds {
            break;
        }
    }
    let mut go =
        has_atoms || has_bonds || (options.flag_possible && (potential_atoms || potential_bonds));
    while go {
        work.spend(1)?;
        let changed_atoms;
        if has_atoms || potential_atoms {
            (has_atoms, changed_atoms) = ctx.assign_atoms(options.flag_possible, work)?;
        } else {
            changed_atoms = false;
        }
        let changed_bonds;
        if has_bonds || potential_bonds {
            (has_bonds, changed_bonds) = ctx.assign_bonds(work)?;
        } else {
            changed_bonds = false;
        }
        go = (has_atoms || has_bonds) && (changed_atoms || changed_bonds);
        if go {
            let labels = ctx
                .state
                .properties
                .atoms
                .iter()
                .map(|p| p.cip_code.clone())
                .collect::<Vec<_>>();
            ctx.ranks = priority::rerank(
                &ctx.state.graph,
                &ctx.state.metadata,
                &ctx.state.valences,
                &ctx.ranks,
                &labels,
                ctx.queries.as_deref(),
                work,
            )?;
            ctx.write_ranks()?;
        }
    }
    if options.clean {
        ctx.cleanup(work)?;
    }
    ctx.state.properties.done = Some(true);
    Ok(ctx.state)
}

struct Context {
    state: State,
    queries: Option<Vec<bool>>,
    neighbors: Vec<Vec<usize>>,
    pairs: HashMap<(usize, usize), usize>,
    ranks: Vec<u32>,
    ring_bonds: Vec<Vec<usize>>,
    atom_rings: Vec<Vec<usize>>,
    bond_rings: Vec<Vec<usize>>,
    minimum: Vec<usize>,
    bridgeheads: Vec<Option<bool>>,
    member_storage: usize,
}
impl Context {
    fn unknown_atom(&self, atom: usize) -> Result<bool, String> {
        let property = at(&self.state.properties.atoms, atom)?;
        if property.invalid_unknown {
            return Err("Invalid unknown-stereo atom property".into());
        }
        Ok(property.unknown)
    }
    fn new(state: State, work: &mut Work) -> Result<Self, String> {
        let mut neighbors = vec![Vec::new(); state.graph.atoms.len()];
        let mut pairs = HashMap::new();
        for (i, b) in state.graph.bonds.iter().enumerate() {
            for a in [b.a, b.b] {
                neighbors
                    .get_mut(a)
                    .ok_or("Missing stereo endpoint")?
                    .push(i);
            }
            pairs.insert((b.a.min(b.b), b.a.max(b.b)), i);
        }
        let mut result = Self {
            state,
            queries: None,
            neighbors,
            pairs,
            ranks: Vec::new(),
            ring_bonds: Vec::new(),
            atom_rings: Vec::new(),
            bond_rings: Vec::new(),
            minimum: Vec::new(),
            bridgeheads: Vec::new(),
            member_storage: 0,
        };
        result.index_rings(work)?;
        Ok(result)
    }
    fn other(&self, bond: usize, atom: usize) -> Result<usize, String> {
        let b = at(&self.state.graph.bonds, bond)?;
        if b.a == atom {
            Ok(b.b)
        } else if b.b == atom {
            Ok(b.a)
        } else {
            Err("Unconnected stereo endpoint".into())
        }
    }
    fn detectable(&self, bond: usize) -> Result<bool, String> {
        Ok(*at(&self.minimum, bond)? >= 8)
    }
    fn hydrogens(&self, atom: usize) -> Result<usize, String> {
        Ok(
            usize::from(at(&self.state.graph.atoms, atom)?.explicit_hydrogens)
                + at(&self.state.valences, atom)?.implicit_hydrogens as usize,
        )
    }
    fn ensure_ranks(&mut self, work: &mut Work) -> Result<(), String> {
        if self.ranks.is_empty() {
            self.ranks = priority::priorities(
                &self.state.graph,
                &self.state.metadata,
                Some(&self.state.valences),
                self.queries.as_deref(),
                work,
            )?;
            self.write_ranks()?;
        }
        Ok(())
    }
    fn write_ranks(&mut self) -> Result<(), String> {
        if self.ranks.len() != self.state.properties.atoms.len() {
            return Err("Invalid assigned CIP rank count".into());
        }
        for (prop, &rank) in self.state.properties.atoms.iter_mut().zip(&self.ranks) {
            prop.cip_rank = Some(rank);
        }
        Ok(())
    }
    fn cleanup(&mut self, work: &mut Work) -> Result<(), String> {
        for (props, meta) in self
            .state
            .properties
            .atoms
            .iter_mut()
            .zip(&mut self.state.metadata.atoms)
        {
            props.ring_candidate = None;
            props.ring_members = None;
            meta.ring_stereo = false;
        }
        let special = self.ring_special_cases(work)?;
        let mut refreshed = Vec::new();
        for i in 0..self.state.graph.atoms.len() {
            work.spend(1)?;
            let meta = self
                .state
                .metadata
                .atoms
                .get_mut(i)
                .ok_or("Missing stereo atom")?;
            if meta.chiral_tag != 0
                && !matches!(meta.chiral_tag, 6..=8)
                && at(&self.state.properties.atoms, i)?.cip_code.is_none()
                && (!*at(&special, i)? || !meta.ring_stereo)
            {
                meta.chiral_tag = 0;
                let atom = self
                    .state
                    .graph
                    .atoms
                    .get_mut(i)
                    .ok_or("Missing graph atom")?;
                if atom.explicit_hydrogens == 1 && atom.charge == 0 && !atom.aromatic {
                    atom.explicit_hydrogens = 0;
                    atom.no_implicit = false;
                    refreshed.push(i);
                }
            }
        }
        if !refreshed.is_empty() {
            self.state.valences =
                self.state
                    .graph
                    .refresh_atoms(&self.state.valences, &refreshed, false)?;
        }
        self.cleanup_bonds(work)?;
        self.cleanup_groups(work)
    }
    fn cleanup_groups(&mut self, work: &mut Work) -> Result<(), String> {
        let mut converted_storage = 0usize;
        if self
            .state
            .metadata
            .bonds
            .iter()
            .any(|m| matches!(m.stereo, 6 | 7))
        {
            for group in &mut self.state.metadata.groups {
                converted_storage += 1;
                let mut atoms = Vec::new();
                let mut bonds = Vec::new();
                let mut seen = HashSet::new();
                for &a in &group.atoms {
                    let mut found = false;
                    for &b in at(&self.neighbors, a)? {
                        work.spend(1)?;
                        if matches!(at(&self.state.metadata.bonds, b)?.stereo, 6 | 7) {
                            found = true;
                            if seen.insert(b) {
                                converted_storage += 1;
                                if converted_storage > 2_000_000 {
                                    return Err("Stereo group output storage limit exceeded".into());
                                }
                                bonds.push(b);
                            }
                        }
                    }
                    if !found {
                        converted_storage += 1;
                        if converted_storage > 2_000_000 {
                            return Err("Stereo group output storage limit exceeded".into());
                        }
                        atoms.push(a);
                    }
                }
                if !bonds.is_empty() {
                    group.atoms = atoms;
                    group.bonds = bonds;
                    group.read_id = 0;
                    group.write_id = 0;
                }
            }
        }
        let mut groups = Vec::new();
        let mut stored = 0usize;
        for group in &self.state.metadata.groups {
            work.spend(1 + group.atoms.len() + group.bonds.len())?;
            stored += 1 + group.atoms.len() + group.bonds.len();
            if stored > 2_000_000 {
                return Err("Stereo group output storage limit exceeded".into());
            }
            let mut next = group.clone();
            next.atoms.retain(|&a| {
                self.state
                    .metadata
                    .atoms
                    .get(a)
                    .is_some_and(|v| v.chiral_tag != 0)
            });
            next.bonds.retain(|&b| {
                self.state
                    .metadata
                    .bonds
                    .get(b)
                    .is_some_and(|v| matches!(v.stereo, 6 | 7))
            });
            if next.atoms.len() == group.atoms.len() && next.bonds.len() == group.bonds.len() {
                groups.push(next);
            } else if !next.atoms.is_empty() {
                next.write_id = 0;
                groups.push(next);
            }
        }
        self.state.metadata.groups = groups;
        Ok(())
    }
}
