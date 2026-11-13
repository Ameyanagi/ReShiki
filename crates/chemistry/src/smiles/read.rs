//! Default SMILES import annotations adapted from SmilesParse.cpp and
//! CXSmilesOps.cpp, Copyright (C) 2001-2022 Greg Landrum and contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Error, Parsed, Prepared, Result, parse_inner, prepare};
use crate::{
    cx,
    kekulize::Direction,
    ranking::StereoGroup,
    stereo::{self, Point3, perception::State, wedging::Conformer},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Input identity survives hydrogen removal and reaction component splitting.
#[derive(Clone, Copy, Debug)]
pub struct InputAtom {
    pub position: usize,
    pub element: &'static str,
}

#[derive(Debug)]
pub enum Configuration {
    SquarePlanar,
    TrigonalBipyramidal,
    Octahedral,
}

#[derive(Debug)]
pub enum Dropped {
    Configuration {
        atom: InputAtom,
        class: Configuration,
    },
    Radical {
        atom: InputAtom,
        electrons: u8,
        hydrogens: (u32, u32),
    },
    Allene(InputAtom),
    Queries,
    AnyBond(InputAtom, InputAtom),
    ZeroBond(InputAtom, InputAtom),
}

#[derive(Debug, Serialize)]
pub struct Imported {
    pub prepared: Prepared,
    /// Features discarded only by the application’s partial import reader.
    #[serde(skip)]
    pub dropped: Vec<Dropped>,
    pub conformers: Vec<Conformer>,
    pub name: Option<String>,
    /// SMILES directions specified odd-cumulene E/Z that preparation discarded.
    pub dropped_cumulene_ez: bool,
    /// Some fragment has a CX relative flag, normalized into stereo groups.
    #[serde(skip)]
    pub relative: bool,
    /// Index of the stereo group the relative flag added, if any.
    #[serde(skip)]
    pub relative_group: Option<usize>,
    /// Reaction CX attachment markers affect wedge selection before drawing.
    #[serde(skip)]
    pub reaction_attachments: Vec<bool>,
    /// Preserve native string types until the temporary layout pass reads them.
    #[serde(skip)]
    pub reaction_properties: Vec<Vec<(Vec<u8>, Vec<u8>)>>,
}

fn at<T>(items: &[T], id: usize) -> Result<&T> {
    items.get(id).ok_or(Error::Limit)
}
fn put<T>(items: &mut [T], id: usize, value: T) -> Result<()> {
    *items.get_mut(id).ok_or(Error::Limit)? = value;
    Ok(())
}

struct Annotations {
    context: prepare::Context,
    properties: Vec<HashMap<Vec<u8>, Vec<u8>>>,
    queries: Vec<bool>,
    // CX makes bare stars null queries for strict RDKit parity, without a
    // constraint for partial import to discard.
    implicit_queries: Vec<bool>,
    groups: HashMap<(u8, u32), usize>,
    labels_processed: bool,
    atom_stereo: bool,
    relative: HashSet<usize>,
    work: usize,
}
impl Annotations {
    fn new(parsed: &Parsed) -> Self {
        Self {
            context: prepare::Context::new(parsed),
            properties: vec![HashMap::new(); parsed.graph.atoms.len()],
            queries: vec![false; parsed.graph.atoms.len()],
            implicit_queries: vec![false; parsed.graph.atoms.len()],
            groups: HashMap::new(),
            labels_processed: false,
            atom_stereo: false,
            relative: HashSet::new(),
            work: 2_000_000,
        }
    }
    fn spend(&mut self, count: usize) -> Result<()> {
        self.work = self.work.checked_sub(count).ok_or(Error::Limit)?;
        Ok(())
    }
    fn conformer(&self) -> Option<&Conformer> {
        self.context
            .conformers
            .iter()
            .find(|c| !c.is_3d)
            .or_else(|| self.context.conformers.first())
    }
    fn labels(&mut self, parsed: &mut Parsed) -> Result<()> {
        if self.labels_processed {
            return Ok(());
        }
        self.labels_processed = true;
        for (id, (atom, props)) in parsed
            .graph
            .atoms
            .iter()
            .zip(&mut self.properties)
            .enumerate()
        {
            if let Some(label) = props.get(b"atomLabel".as_slice()).cloned() {
                put(&mut parsed.dummy_labels, id, None)?;
                props.remove(b"dummyLabel".as_slice());
                if [
                    b"star_e".as_slice(),
                    b"Q_e",
                    b"QH_p",
                    b"AH_p",
                    b"X_p",
                    b"XH_p",
                    b"M_p",
                    b"MH_p",
                ]
                .contains(&label.as_slice())
                {
                    put(&mut self.queries, id, true)?;
                } else if label == b"Pol_p" || label == b"Mod_p" {
                    let label = label.get(..3).ok_or(Error::Limit)?.to_vec();
                    props.insert(b"dummyLabel".to_vec(), label);
                    props.remove(b"atomLabel".as_slice());
                }
            } else if atom.atomic_number == 0
                && atom.isotope == 0
                && props
                    .get(b"dummyLabel".as_slice())
                    .map(Vec::as_slice)
                    .or_else(|| {
                        parsed
                            .dummy_labels
                            .get(id)
                            .and_then(|label| label.as_ref())
                            .map(|label| label.as_bytes())
                    })
                    .is_none_or(|symbol| symbol == b"*")
            {
                put(&mut self.implicit_queries, id, true)?;
            }
        }
        Ok(())
    }
    fn apply(&mut self, parsed: &mut Parsed, event: cx::Event) -> Result<()> {
        match event {
            cx::Event::AtomProperty { atom, name, value } => {
                self.properties
                    .get_mut(atom)
                    .ok_or(Error::Limit)?
                    .insert(name, value);
            }
            cx::Event::Coordinates(points) => {
                if points.is_empty() {
                    return Ok(());
                }
                self.spend(parsed.graph.atoms.len())?;
                let mut positions = vec![Point3::default(); parsed.graph.atoms.len()];
                let mut marked = false;
                for (point, fields) in positions.iter_mut().zip(points) {
                    for (slot, text) in [&mut point.x, &mut point.y, &mut point.z]
                        .into_iter()
                        .zip(&fields)
                    {
                        if !text.is_empty() {
                            *slot = cx::coordinate(
                                std::str::from_utf8(text)
                                    .map_err(|_| Error::Unsupported("non-UTF8 coordinate"))?,
                            )
                            .ok_or(Error::Unsupported("coordinate"))?;
                        }
                    }
                    marked |= fields.get(2).is_some_and(|s| !s.is_empty());
                }
                if positions.len() > 1 && positions.iter().all(|p| Some(p) == positions.first()) {
                    return Ok(());
                }
                let is_3d = marked && positions.iter().any(|p| p.z.abs() > 1e-3);
                self.context.conformers.push(Conformer { positions, is_3d });
            }
            cx::Event::BondKind { bond, begin, kind } => {
                put(
                    &mut self.context.unsupported_bonds,
                    bond,
                    matches!(kind, cx::BondKind::Zero),
                )?;
                let bond = parsed.graph.bonds.get_mut(bond).ok_or(Error::Limit)?;
                bond.order = if matches!(kind, cx::BondKind::Coordinate) {
                    5
                } else {
                    0
                };
                if begin.is_some_and(|a| a != bond.a) {
                    std::mem::swap(&mut bond.a, &mut bond.b);
                }
            }
            cx::Event::Radical { atom, electrons } => {
                parsed
                    .graph
                    .atoms
                    .get_mut(atom)
                    .ok_or(Error::Limit)?
                    .radical_electrons = electrons;
            }
            cx::Event::StereoGroup { kind, id, atoms } => {
                let key = (kind, id);
                if let Some(&index) = self.groups.get(&key) {
                    let group = parsed.metadata.groups.get_mut(index).ok_or(Error::Limit)?;
                    group.atoms.extend(atoms);
                    group.read_id = id;
                } else {
                    self.groups.insert(key, parsed.metadata.groups.len());
                    parsed.metadata.groups.push(StereoGroup {
                        kind,
                        read_id: id,
                        atoms,
                        ..StereoGroup::default()
                    });
                }
            }
            cx::Event::RelativeStereo(atoms) => self.relative.extend(atoms),
            cx::Event::QueryAtom(atom) => put(&mut self.queries, atom, true)?,
            cx::Event::SubstanceGroup { atoms, bonds } => {
                let members = atoms.iter().copied().collect::<HashSet<_>>();
                for id in bonds {
                    let bond = at(&parsed.graph.bonds, id)?;
                    if members.contains(&bond.a) != members.contains(&bond.b) {
                        self.context
                            .annotations
                            .protected_atoms
                            .extend([bond.a, bond.b]);
                    }
                }
                self.context.annotations.substance_groups.push(atoms);
            }
            cx::Event::Wedge { bond, begin, kind } => {
                let b = parsed.graph.bonds.get_mut(bond).ok_or(Error::Limit)?;
                if b.a != begin {
                    std::mem::swap(&mut b.a, &mut b.b);
                }
                put(
                    &mut parsed.directions,
                    bond,
                    match kind {
                        1 => Direction::Wedge,
                        3 => Direction::Hash,
                        _ => Direction::Unknown,
                    },
                )?;
                if matches!(b.order, 1 | 4) {
                    if kind == 2 {
                        parsed
                            .metadata
                            .atoms
                            .get_mut(begin)
                            .ok_or(Error::Limit)?
                            .chiral_tag = 0;
                        self.context.needs_bond_stereo = true;
                    } else {
                        self.atom_stereo = true;
                    }
                }
            }
            cx::Event::BondStereo { bond, stereo } => {
                let b = at(&parsed.graph.bonds, bond)?;
                let mut controls = [None::<usize>; 2];
                self.spend(parsed.graph.bonds.len())?;
                for edge in &parsed.graph.bonds {
                    for (slot, (atom, partner)) in controls.iter_mut().zip([(b.a, b.b), (b.b, b.a)])
                    {
                        let other = if edge.a == atom {
                            Some(edge.b)
                        } else if edge.b == atom {
                            Some(edge.a)
                        } else {
                            None
                        };
                        if let Some(other) = other.filter(|&other| other != partner) {
                            *slot = Some(slot.map_or(other, |old| old.min(other)));
                        }
                    }
                }
                if let [Some(a), Some(b)] = controls {
                    let meta = parsed.metadata.bonds.get_mut(bond).ok_or(Error::Limit)?;
                    meta.stereo = stereo;
                    meta.stereo_atoms = vec![a, b];
                    self.context.needs_bond_stereo = true;
                }
            }
            cx::Event::ProcessLabels => self.labels(parsed)?,
        }
        Ok(())
    }
    fn numeric_props(&mut self, parsed: &mut Parsed) -> Result<()> {
        for (id, props) in self.properties.iter().enumerate() {
            let error = self
                .context
                .property_errors
                .get_mut(id)
                .ok_or(Error::Limit)?;
            let meta = parsed.metadata.atoms.get_mut(id).ok_or(Error::Limit)?;
            if let Some(map) = int_property(props, b"molAtomMapNumber", error) {
                meta.map_number = map;
                meta.map_present = true;
            }
            if let Some(value) = props.get(b"_chiralPermutation".as_slice()) {
                let value = std::str::from_utf8(value).ok().and_then(|text| {
                    if let Some(negative) = text.strip_prefix('-') {
                        negative.parse::<u32>().ok().map(u32::wrapping_neg)
                    } else {
                        text.parse::<u32>().ok()
                    }
                });
                if let Some(value) = value {
                    meta.chiral_permutation = Some(value);
                } else {
                    put(&mut self.context.permutation_errors, id, true)?;
                }
            }
            // Default import and drawing ranking never enable non-stereo
            // ranks. Malformed values therefore remain unused native metadata.
            if let Some(value) = int_property(props, b"_CanonicalRankingNumber", &mut None) {
                meta.non_stereo_rank = value;
            }
            let mut unknown_error = None;
            if let Some(value) = int_property(props, b"_UnknownStereo", &mut unknown_error) {
                put(&mut self.context.unknown, id, value != 0)?;
            }
            put(
                &mut self.context.unknown_errors,
                id,
                unknown_error.is_some(),
            )?;
            if let Some(value) = props.get(b"dummyLabel".as_slice()) {
                match String::from_utf8(value.clone()) {
                    Ok(value) => put(&mut parsed.dummy_labels, id, Some(value))?,
                    Err(_) => *error = Some("non-UTF8 atom label"),
                }
            }
        }
        Ok(())
    }
}

fn int_property(
    props: &HashMap<Vec<u8>, Vec<u8>>,
    key: &[u8],
    error: &mut Option<&'static str>,
) -> Option<i32> {
    let text = props.get(key)?;
    let value = std::str::from_utf8(text)
        .ok()
        .and_then(|s| s.parse::<i32>().ok());
    if value.is_none() {
        *error = Some("invalid numeric atom property");
    }
    value
}

fn attachment_label_number(label: &[u8]) -> Option<u32> {
    let suffix = label.strip_prefix(b"_AP")?;
    if suffix.is_empty() || !suffix.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(suffix).ok()?.parse().ok()
}

fn relative_stereo(state: &mut State, relative: &HashSet<usize>) -> Result<bool> {
    let metadata = &state.metadata;
    // Explicit enhanced stereo labels take precedence over relative flags.
    let grouped: HashSet<_> = metadata
        .groups
        .iter()
        .flat_map(|group| group.atoms.iter().copied())
        .collect();
    let atoms: Vec<_> = metadata
        .atoms
        .iter()
        .enumerate()
        .filter_map(|(atom, meta)| {
            (relative.contains(&atom)
                && matches!(meta.chiral_tag, 1 | 2)
                && !grouped.contains(&atom))
            .then_some(atom)
        })
        .collect();
    if atoms.is_empty() {
        return Ok(false);
    }
    // Equivalence of the drawn member need not survive independent group inversions.
    if !metadata
        .groups
        .iter()
        .any(|group| matches!(group.kind, 1 | 2))
    {
        // Atom maps are not molecular identity.
        let mut unmapped = state.clone();
        for atom in &mut unmapped.metadata.atoms {
            atom.map_number = 0;
            atom.map_present = false;
        }
        let mut inverse = unmapped.clone();
        super::invert_tetrahedral(&mut inverse, &atoms.iter().copied().collect());
        if super::canonical_smiles(&unmapped)? == super::canonical_smiles(&inverse)? {
            return Ok(false);
        }
    }
    state.metadata.groups.push(StereoGroup {
        kind: 2,
        atoms,
        ..StereoGroup::default()
    });
    Ok(true)
}

/// Reactions apply CX annotations after bare parsing and keep explicit H.
/// They sanitize without the default molecule import's stereo perception.
/// Explicit enhanced stereo groups and normalized relative flags are retained.
pub fn reaction_part(parsed: Parsed, events: Vec<cx::Event>) -> Result<Imported> {
    reaction_part_with(parsed, events, None)
}

/// Read one participant for partial application import, using global input positions.
pub fn reaction_part_partial(
    parsed: Parsed,
    events: Vec<cx::Event>,
    positions: &[usize],
) -> Result<Imported> {
    reaction_part_with(parsed, events, Some(positions))
}

fn reaction_part_with(
    mut parsed: Parsed,
    events: Vec<cx::Event>,
    positions: Option<&[usize]>,
) -> Result<Imported> {
    use crate::{
        sanitize,
        stereo::perception::{Properties, RingCache, RingKind},
    };
    let mut annotations = Annotations::new(&parsed);
    for event in events {
        annotations.apply(&mut parsed, event)?;
    }
    annotations.numeric_props(&mut parsed)?;
    let mut dropped = Vec::new();
    if let Some(positions) = positions {
        partial_graph(&mut parsed, &mut annotations, positions, &mut dropped)?;
    }
    prepare::validate_allenes(&parsed)?;
    if annotations
        .queries
        .iter()
        .chain(&annotations.implicit_queries)
        .any(|&q| q)
        || annotations.context.unsupported_bonds.iter().any(|&b| b)
        || parsed.query_bonds.iter().any(|&b| b)
    {
        return Err(Error::Unsupported("reaction query or bond type"));
    }
    if let Some(error) = annotations.context.property_errors.iter().flatten().next() {
        return Err(Error::Unsupported(error));
    }
    // Read axial flags before kekulization and small-ring axis cleanup (#1048).
    if annotations.context.conformers.is_empty() {
        parsed.metadata = stereo::from_directions_with_bounds(
            &parsed.graph,
            &parsed.metadata,
            &parsed.directions,
            None,
            true,
            stereo::CoordinateBounds::NativeImport,
        )
        .map_err(Error::Stereo)?
        .metadata;
    }
    let clean = sanitize::sanitize(&parsed.graph, &parsed.metadata, &parsed.directions)?;
    let mut state = State {
        conformer: annotations.context.conformers.first().cloned(),
        properties: Properties::unspecified(&clean.graph),
        graph: clean.graph,
        metadata: clean.metadata,
        directions: clean.directions,
        valences: clean.valences,
        conjugated: clean.conjugated,
        hybridizations: clean.hybridizations,
        rings: RingCache {
            kind: RingKind::Symmetric,
            atoms: clean.rings,
        },
    };
    if let Some(positions) = positions {
        partial_state(&mut state, positions, &mut dropped)?;
    }
    if state.graph.atoms.iter().any(|a| a.radical_electrons > 2)
        || state.metadata.atoms.iter().enumerate().any(|(i, a)| {
            !stereo::allene::supported(&state.graph, i, a.chiral_tag, a.chiral_permutation)
        })
        || state.metadata.bonds.iter().any(|b| b.stereo > 7)
    {
        return Err(Error::Unsupported(
            "reaction stereochemistry or radical count",
        ));
    }
    let properties = &mut state.properties;
    properties.needs_detection = annotations.context.needs_bond_stereo.then_some(true);
    for (i, (props, atom)) in annotations
        .properties
        .iter()
        .zip(&mut properties.atoms)
        .enumerate()
    {
        atom.unknown = *at(&annotations.context.unknown, i)?;
        atom.invalid_unknown = *at(&annotations.context.unknown_errors, i)?;
        if let Some(code) = props.get(b"_CIPCode".as_slice()) {
            atom.cip_code = Some(
                String::from_utf8(code.clone())
                    .map_err(|_| Error::Unsupported("non-UTF8 CIP code"))?,
            );
        }
        if let Some(value) = props.get(b"_ChiralityPossible".as_slice()) {
            // Preserve the optional legacy flag. The native property dictionary
            // interprets numeric strings as numbers and leaves other text intact.
            atom.possible = Some(
                std::str::from_utf8(value)
                    .ok()
                    .and_then(|s| cx::coordinate(s.trim()))
                    .is_none_or(|n| n != 0.0),
            );
        }
    }
    let reaction_properties = annotations
        .properties
        .iter()
        .map(|props| {
            let mut entries = props
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>();
            entries.sort();
            entries
        })
        .collect();
    let mut degree = vec![0usize; state.graph.atoms.len()];
    for bond in &state.graph.bonds {
        *degree.get_mut(bond.a).ok_or(Error::Limit)? += 1;
        *degree.get_mut(bond.b).ok_or(Error::Limit)? += 1;
    }
    let mut imported = Imported {
        dropped,
        dropped_cumulene_ez: false,
        relative: !annotations.relative.is_empty(),
        relative_group: None,
        reaction_properties,
        reaction_attachments: annotations
            .properties
            .iter()
            .zip(&state.graph.atoms)
            .zip(degree)
            .map(|((props, atom), degree)| {
                // WedgeBonds tests the native property, not marked attachment
                // identity. CleanupAfterParsing (#9454) adds it only for labels 1/2.
                props.contains_key(b"_fromAttchpt".as_slice())
                    || atom.atomic_number == 0
                        && degree == 1
                        && props
                            .get(b"atomLabel".as_slice())
                            .and_then(|label| attachment_label_number(label))
                            .is_some_and(|number| matches!(number, 1 | 2))
            })
            .collect(),
        prepared: Prepared {
            state,
            dummy_labels: parsed.dummy_labels,
        },
        conformers: annotations.context.conformers,
        name: None,
    };
    if imported.relative {
        let index = imported.prepared.state.metadata.groups.len();
        if relative_stereo(&mut imported.prepared.state, &annotations.relative)? {
            imported.relative_group = Some(index);
        }
    }
    Ok(imported)
}

fn partial_state(
    state: &mut State,
    positions: &[usize],
    dropped: &mut Vec<Dropped>,
) -> Result<bool> {
    let mut changed = false;
    let bare_radicals = if state.graph.atoms.iter().any(|a| a.radical_electrons > 2) {
        let mut graph = state.graph.clone();
        for atom in &mut graph.atoms {
            if atom.radical_electrons > 2 {
                atom.no_implicit = true;
                atom.explicit_hydrogens = 0;
            }
        }
        crate::sanitize::sanitize(&graph, &state.metadata, &state.directions)?
            .graph
            .atoms
            .into_iter()
            .map(|atom| atom.radical_electrons)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut bonded_hydrogens = vec![0u32; state.graph.atoms.len()];
    for bond in &state.graph.bonds {
        for (atom, neighbour) in [(bond.a, bond.b), (bond.b, bond.a)] {
            if at(&state.graph.atoms, neighbour)?.atomic_number == 1 {
                *bonded_hydrogens.get_mut(atom).ok_or(Error::Limit)? += 1;
            }
        }
    }
    // A centre in an enhanced stereo group keeps its tag, so the import is
    // refused instead of silently dropping it from the group.
    let grouped: HashSet<usize> = state
        .metadata
        .groups
        .iter()
        .flat_map(|group| group.atoms.iter().copied())
        .collect();
    for (id, (atom, meta)) in state
        .graph
        .atoms
        .iter_mut()
        .zip(&mut state.metadata.atoms)
        .enumerate()
    {
        if grouped.contains(&id) {
            continue;
        }
        let input = input_atom(atom, *at(positions, id)?)?;
        let class = match meta.chiral_tag {
            6 => Some(Configuration::SquarePlanar),
            7 => Some(Configuration::TrigonalBipyramidal),
            8 => Some(Configuration::Octahedral),
            _ => None,
        };
        if let Some(class) = class {
            dropped.push(Dropped::Configuration { atom: input, class });
            meta.chiral_tag = 0;
            meta.chiral_permutation = Some(0);
            changed = true;
        }
        if atom.radical_electrons > 2 {
            let r0 = *at(&bare_radicals, id)?;
            if r0 < 2 {
                continue;
            }
            let before =
                u32::from(atom.explicit_hydrogens) + at(&state.valences, id)?.implicit_hydrogens;
            let after = u32::from(r0 - 2);
            dropped.push(Dropped::Radical {
                atom: input,
                electrons: atom.radical_electrons,
                hydrogens: (
                    before + at(&bonded_hydrogens, id)?,
                    after + at(&bonded_hydrogens, id)?,
                ),
            });
            // Store a valence-derived triplet that survives preparation and export.
            atom.no_implicit = true;
            atom.explicit_hydrogens = r0 - 2;
            changed = true;
        }
    }
    if changed {
        let clean = crate::sanitize::sanitize(&state.graph, &state.metadata, &state.directions)?;
        state.graph = clean.graph;
        state.metadata = clean.metadata;
        state.directions = clean.directions;
        state.valences = clean.valences;
        state.conjugated = clean.conjugated;
        state.hybridizations = clean.hybridizations;
        state.rings.atoms = clean.rings;
    }
    Ok(changed)
}

fn input_atom(atom: &crate::graph::Atom, position: usize) -> Result<InputAtom> {
    Ok(InputAtom {
        position,
        element: at(crate::ELEMENTS, usize::from(atom.atomic_number))?.symbol,
    })
}

fn retain_bonds<T>(items: &mut Vec<T>, mapping: &[Option<usize>]) {
    let mut indices = mapping.iter();
    items.retain(|_| indices.next().is_some_and(Option::is_some));
}

fn partial_graph(
    parsed: &mut Parsed,
    annotations: &mut Annotations,
    positions: &[usize],
    dropped: &mut Vec<Dropped>,
) -> Result<()> {
    if annotations.queries.iter().any(|&q| q) {
        dropped.push(Dropped::Queries);
        annotations.queries.fill(false);
    }
    annotations.implicit_queries.fill(false);
    let mut mapping = vec![None; parsed.graph.bonds.len()];
    let mut kept = 0;
    for (id, (bond, query)) in parsed
        .graph
        .bonds
        .iter_mut()
        .zip(&mut parsed.query_bonds)
        .enumerate()
    {
        let zero = *at(&annotations.context.unsupported_bonds, id)?;
        if zero || (*query && bond.order == 1) {
            let atom = |id| input_atom(at(&parsed.graph.atoms, id)?, *at(positions, id)?);
            let mut ends = [atom(bond.a)?, atom(bond.b)?];
            ends.sort_by_key(|atom| atom.position);
            if zero {
                dropped.push(Dropped::ZeroBond(ends[0], ends[1]));
            } else {
                dropped.push(Dropped::AnyBond(ends[0], ends[1]));
                bond.aromatic = false;
            }
        }
        *query = false;
        if !zero {
            put(&mut mapping, id, Some(kept))?;
            kept += 1;
        }
    }
    if kept != mapping.len() {
        retain_bonds(&mut parsed.graph.bonds, &mapping);
        retain_bonds(&mut parsed.metadata.bonds, &mapping);
        retain_bonds(&mut parsed.directions, &mapping);
        retain_bonds(&mut parsed.bond_indices, &mapping);
        retain_bonds(&mut parsed.ring_bonds, &mapping);
        retain_bonds(&mut parsed.query_bonds, &mapping);
        retain_bonds(&mut annotations.context.unsupported_bonds, &mapping);
        for group in &mut parsed.metadata.groups {
            group.bonds = group
                .bonds
                .iter()
                .filter_map(|&id| mapping.get(id).copied().flatten())
                .collect();
        }
    }
    // A tag in an enhanced stereo group stays, so the import is refused as before.
    let grouped: HashSet<usize> = parsed
        .metadata
        .groups
        .iter()
        .flat_map(|group| group.atoms.iter().copied())
        .collect();
    for (id, meta) in parsed.metadata.atoms.iter_mut().enumerate() {
        if meta.chiral_tag == 5
            && !grouped.contains(&id)
            && !stereo::allene::supported(
                &parsed.graph,
                id,
                meta.chiral_tag,
                meta.chiral_permutation,
            )
        {
            dropped.push(Dropped::Allene(input_atom(
                at(&parsed.graph.atoms, id)?,
                *at(positions, id)?,
            )?));
            meta.chiral_tag = 0;
            meta.chiral_permutation = Some(0);
        }
    }
    Ok(())
}

/// Keep stated E/Z under wU/wD on terminal H/C bonds to isolated C=C/C=N
/// ends with no other neighbour or chiral tag (#1042). These flags cannot
/// define a tetrahedral centre or contribute another bond to a chiral axis.
fn keep_stated_alkene_directions(parsed: &mut Parsed, stated: &[(usize, Direction)]) -> Result<()> {
    let graph = &parsed.graph;
    let mut degree = vec![0usize; graph.atoms.len()];
    let mut doubles = vec![Vec::new(); graph.atoms.len()];
    let mut directed = vec![false; graph.atoms.len()];
    for (bond, &(_, direction)) in graph.bonds.iter().zip(stated) {
        for (atom, partner) in [(bond.a, bond.b), (bond.b, bond.a)] {
            *degree.get_mut(atom).ok_or(Error::Limit)? += 1;
            if bond.order == 2 && !bond.aromatic {
                doubles.get_mut(atom).ok_or(Error::Limit)?.push(partner);
            }
            if bond.order == 1 && matches!(direction, Direction::Up | Direction::Down) {
                put(&mut directed, atom, true)?;
            }
        }
    }
    let terminal = |atom: usize| -> Result<bool> {
        Ok(*at(&degree, atom)? == 1 && matches!(at(&graph.atoms, atom)?.atomic_number, 1 | 6))
    };
    let alkene_end = |atom: usize| -> Result<bool> {
        let &[partner] = at(&doubles, atom)?.as_slice() else {
            return Ok(false);
        };
        let a = at(&graph.atoms, atom)?;
        let b = at(&graph.atoms, partner)?;
        Ok(*at(&degree, atom)? == 2
            && at(&doubles, partner)?.len() == 1
            // Require a slash at the other end before H removal (#1042).
            && *at(&directed, partner)?
            && !a.aromatic
            && !b.aromatic
            && matches!((a.atomic_number, b.atomic_number), (6, 6 | 7) | (7, 6)))
    };
    for ((bond, direction), &(stated_a, stated_direction)) in
        graph.bonds.iter().zip(&mut parsed.directions).zip(stated)
    {
        // Keep the flag's begin atom, reversing the slash when it swapped ends.
        let restored = match (stated_direction, stated_a == bond.a) {
            (Direction::Up, true) | (Direction::Down, false) => Direction::Up,
            (Direction::Down, true) | (Direction::Up, false) => Direction::Down,
            _ => continue,
        };
        if bond.order == 1
            && matches!(direction, Direction::Wedge | Direction::Hash)
            && at(&parsed.metadata.atoms, bond.a)?.chiral_tag == 0
            && at(&parsed.metadata.atoms, bond.b)?.chiral_tag == 0
            && (terminal(bond.a)? && alkene_end(bond.b)?
                || terminal(bond.b)? && alkene_end(bond.a)?)
        {
            *direction = restored;
        }
    }
    Ok(())
}

/// Read default SMILES/CXSMILES chemistry, retaining input conformers and name.
/// Drawing layout and the application's editable-chemistry checks follow this.
pub fn read(text: &str) -> Result<Imported> {
    read_with(text, true, false)
}

/// Read as RDKit does, letting wU/wD replace the E/Z direction on their bond.
/// Reference comparisons use this reading where `read` keeps stated E/Z (#1042).
pub fn read_dropping_flagged_ez(text: &str) -> Result<Imported> {
    read_with(text, false, false)
}

/// Import the representable structure and record discarded features for notices.
pub fn read_partial(text: &str) -> Result<Imported> {
    read_with(text, true, true)
}

fn read_with(text: &str, keep_stated_ez: bool, partial: bool) -> Result<Imported> {
    if text.len() > 1024 * 1024 {
        return Err(Error::Limit);
    }
    let split = text
        .bytes()
        .position(|b| matches!(b, b' ' | b'\t'))
        .filter(|&i| i != 0);
    let (body, suffix) = if let Some(i) = split {
        (
            text.get(..i).ok_or(Error::Limit)?,
            cx::trim(text.get(i..).ok_or(Error::Limit)?),
        )
    } else {
        (text, "")
    };
    let mut parsed = if partial {
        super::parse_for_import(body)?
    } else {
        parse_inner(body)?
    };
    // Record directions before H removal: a bond to a removed H still specifies
    // a terminal ligand's orientation, including H opposite an imine lone pair.
    let mut directed = vec![false; parsed.graph.atoms.len()];
    for (bond, direction) in parsed.graph.bonds.iter().zip(&parsed.directions) {
        if matches!(direction, Direction::Up | Direction::Down) {
            put(&mut directed, bond.a, true)?;
            put(&mut directed, bond.b, true)?;
        }
    }
    let mut annotations = Annotations::new(&parsed);
    let name = if suffix.starts_with('|') {
        let topology = cx::Topology {
            atoms: parsed.graph.atoms.len(),
            bonds: parsed
                .graph
                .bonds
                .iter()
                .zip(&parsed.bond_indices)
                .map(|(b, &index)| cx::ParseBond {
                    a: b.a,
                    b: b.b,
                    index: Some(index),
                })
                .collect(),
        };
        let extension = cx::read(suffix, &topology)?;
        let stated: Vec<_> = parsed
            .graph
            .bonds
            .iter()
            .zip(&parsed.directions)
            .map(|(bond, &direction)| (bond.a, direction))
            .collect();
        for event in extension.events {
            annotations.apply(&mut parsed, event)?;
        }
        if keep_stated_ez {
            keep_stated_alkene_directions(&mut parsed, &stated)?;
        }
        cx::trim(suffix.get(extension.end..).ok_or(Error::Limit)?)
    } else {
        suffix
    };
    if !partial
        && annotations
            .queries
            .iter()
            .chain(&annotations.implicit_queries)
            .any(|&q| q)
    {
        return Err(Error::Unsupported("query atom"));
    }
    annotations.numeric_props(&mut parsed)?;
    let mut dropped = Vec::new();
    if partial {
        let positions = (0..parsed.graph.atoms.len()).collect::<Vec<_>>();
        partial_graph(&mut parsed, &mut annotations, &positions, &mut dropped)?;
    }
    prepare::validate_allenes(&parsed)?;
    let conf = annotations.conformer();
    if annotations.atom_stereo {
        let drawn = stereo::from_directions_with_bounds(
            &parsed.graph,
            &parsed.metadata,
            &parsed.directions,
            conf,
            true,
            stereo::CoordinateBounds::NativeImport,
        )
        .map_err(Error::Stereo)?;
        parsed.graph = drawn.graph;
        parsed.metadata = drawn.metadata;
    }
    if let Some(conf) = conf.filter(|c| c.is_3d) {
        parsed.metadata = stereo::from_3d_with_bounds(
            &parsed.graph,
            &parsed.metadata,
            &parsed.directions,
            Some(conf),
            &stereo::SpatialAnnotations {
                non_explicit: vec![None; parsed.graph.atoms.len()],
                done: None,
            },
            stereo::SpatialOptions::default(),
            stereo::CoordinateBounds::NativeImport,
        )?
        .metadata;
    }
    let (mut prepared, conformers, kept_atoms) = prepare::finish(parsed, annotations.context)?;
    if partial && partial_state(&mut prepared.state, &kept_atoms, &mut dropped)? {
        prepared.state = stereo::perception::perceive(
            &prepared.state,
            stereo::perception::Options {
                clean: true,
                force: true,
                flag_possible: true,
            },
        )
        .map_err(Error::Stereo)?;
    }
    let mut dropped_cumulene_ez = false;
    for chain in stereo::stereogenic_odd_cumulenes(&prepared.state).map_err(Error::Stereo)? {
        let [(a, _), (b, _)] = chain.ends;
        if *at(&directed, *at(&kept_atoms, a)?)? && *at(&directed, *at(&kept_atoms, b)?)? {
            dropped_cumulene_ez = true;
            break;
        }
    }
    // After cumulene detection: an `r` group would make enantiomorphic ligands
    // rank equal and hide the dropped E/Z.
    let mut relative_group = None;
    if !annotations.relative.is_empty() {
        let relative = kept_atoms
            .iter()
            .enumerate()
            .filter_map(|(atom, original)| annotations.relative.contains(original).then_some(atom))
            .collect();
        let index = prepared.state.metadata.groups.len();
        if relative_stereo(&mut prepared.state, &relative)? {
            relative_group = Some(index);
        }
    }
    // Native RemoveHs preserves properties on retained atoms. Keep the raw
    // values for the later depiction stage, which converts rank properties
    // only when the selected fragment/neighbor ordering reads them.
    let reaction_properties = kept_atoms
        .iter()
        .map(|&id| {
            let mut entries = annotations
                .properties
                .get(id)
                .ok_or(Error::Limit)?
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>();
            entries.sort();
            Ok(entries)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Imported {
        dropped,
        dropped_cumulene_ez,
        relative: !annotations.relative.is_empty(),
        relative_group,
        prepared,
        conformers,
        name: (!name.is_empty()).then(|| name.to_owned()),
        reaction_attachments: Vec::new(),
        reaction_properties,
    })
}

#[cfg(test)]
mod retained_properties;
