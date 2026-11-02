//! Convert verified chemistry back to an editable drawing. Full CIP labeling
//! is an explicit dependency; legacy CIP codes must never become display labels.
use super::{Error, Molecule, WORLD_PER_DEPICT_UNIT, at, invalid};
use crate::{
    chemistry::{
        ELEMENTS, RDKIT_VERSION, kekulize, ranking,
        stereo::{cip::label, perception::RingKind, wedging},
    },
    document::{Atom, AtomStereo, Bond, Document, Point},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod fusion_hydrogens;

#[cfg(test)]
mod reconstruct_parity_tests;
#[cfg(test)]
mod tests;

/// Depiction preferences for newly generated structures, independent of page metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct DrawingOptions {
    pub two_wedge_stereocentres: bool,
    pub ring_fusion_hydrogens: bool,
    pub wavy_unknown_alkenes: bool,
}
impl Default for DrawingOptions {
    fn default() -> Self {
        Self {
            two_wedge_stereocentres: false,
            ring_fusion_hydrogens: true,
            wavy_unknown_alkenes: false,
        }
    }
}
impl From<&crate::style::DrawingStyle> for DrawingOptions {
    fn from(style: &crate::style::DrawingStyle) -> Self {
        Self {
            two_wedge_stereocentres: style.two_wedge_stereocentres,
            ring_fusion_hydrogens: style.ring_fusion_hydrogens,
            wavy_unknown_alkenes: style.wavy_unknown_alkenes,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub rdkit_version: String,
    pub atoms: Vec<Option<String>>,
    pub bonds: Vec<BondLabel>,
}

/// Full CIP labeling may normalize E/Z to cis/trans and choose new controls.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BondLabel {
    pub code: Option<String>,
    pub stereo: u8,
    pub stereo_atoms: Vec<usize>,
}

/// Detached drawing plus the exact molecular state that needs full CIP labels.
/// Finishing consumes the draft; a partial result never changes the input.
#[derive(Clone, Debug)]
pub struct Drawing {
    molecule: Molecule,
    document: Document,
    bond_indices: Vec<usize>,
}
impl Drawing {
    /// Restore file spin states without changing the chemistry graph.
    pub fn with_singlets(mut self, singlets: &[bool]) -> Result<Self, Error> {
        if singlets.len() != self.document.atoms.len() {
            return Err(invalid("Drawing spin state dimensions changed"));
        }
        for (atom, &singlet) in self.document.atoms.iter_mut().zip(singlets) {
            atom.singlet = singlet;
        }
        Ok(self)
    }
    pub fn with_generic(mut self, generic: &crate::generic::Generic) -> Result<Self, Error> {
        self.document.generic = generic.clone();
        for site in &generic.alternatives {
            let label = site.label();
            let atom = self
                .document
                .atom_mut(site.atom)
                .ok_or_else(|| invalid("Missing alternative atom"))?;
            atom.element = "*".into();
            atom.no_implicit = true;
            atom.display.set_variable(Some(label));
        }
        for link in &generic.links {
            let atom = self
                .document
                .atom_mut(link.atom)
                .ok_or_else(|| invalid("Missing link atom"))?;
            atom.display.set_variable(Some(link.label(&atom.element)));
        }
        for index in 0..self.document.generic.repeats.len() {
            let unit = self
                .document
                .generic
                .repeats
                .get(index)
                .ok_or_else(|| invalid("Missing repeat unit"))?
                .clone();
            if unit.bracket.is_none() {
                let points = unit
                    .members
                    .iter()
                    .filter_map(|id| self.document.atom(*id).map(|a| a.position));
                let bounds = points.fold(None, |bounds: Option<(Point, Point)>, p| {
                    Some(match bounds {
                        None => (p, p),
                        Some((lo, hi)) => (
                            Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                            Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                        ),
                    })
                });
                if let Some((lo, hi)) = bounds {
                    let id = self.document.next_id();
                    self.document
                        .graphics
                        .push(crate::graphics::Graphic::dragged(
                            id,
                            crate::graphics::GraphicKind::Brackets,
                            lo.offset(-14., -24.),
                            hi.offset(14., 24.),
                            Default::default(),
                            crate::graphics::BracketSides::Both,
                            false,
                        ));
                    self.document
                        .generic
                        .repeats
                        .get_mut(index)
                        .ok_or_else(|| invalid("Missing repeat unit"))?
                        .bracket = Some(id);
                }
            }
        }
        self.document.validate().map_err(invalid)?;
        Ok(self)
    }
    pub fn with_attachments(
        mut self,
        attachments: &[crate::attachments::Attachment],
    ) -> Result<Self, Error> {
        for attachment in attachments {
            let atom = self
                .document
                .atom_mut(attachment.id)
                .ok_or_else(|| invalid("Missing attachment atom"))?;
            if atom.attachment.is_some()
                && (atom.attachment != Some(attachment.kind) || atom.centroid != attachment.members)
            {
                return Err(invalid("Conflicting attachment target lists"));
            }
            atom.attachment = Some(attachment.kind);
            atom.element = "*".into();
            atom.centroid.clone_from(&attachment.members);
            atom.no_implicit = true;
            atom.label_h = 0;
        }
        self.document.validate().map_err(invalid)?;
        Ok(self)
    }
    pub fn molecule(&self) -> &Molecule {
        &self.molecule
    }
    /// Compute display labels from the prepared drawing. Clear legacy or stale
    /// codes first; only the complete CIP pass may supply drawing labels.
    pub fn labels(&self) -> Result<Labels, Error> {
        let mut state = self.molecule.state.clone();
        for atom in &mut state.properties.atoms {
            atom.cip_code = None;
        }
        state.properties.bond_codes.fill(None);
        let state = label::assign(&state, &Default::default())?.state;
        Ok(Labels {
            rdkit_version: RDKIT_VERSION.into(),
            atoms: state
                .properties
                .atoms
                .into_iter()
                .map(|atom| atom.cip_code)
                .collect(),
            bonds: state
                .metadata
                .bonds
                .into_iter()
                .zip(state.properties.bond_codes)
                .map(|(bond, code)| BondLabel {
                    code,
                    stereo: bond.stereo,
                    stereo_atoms: bond.stereo_atoms,
                })
                .collect(),
        })
    }
    /// Reaction layout uses full-precision file coordinates until every row is
    /// placed. Replace canvas positions before validating the finished drawing;
    /// the conformer used for stereochemistry remains unchanged.
    pub fn finish_at(mut self, labels: Labels, positions: &[Point]) -> Result<Document, Error> {
        if positions.len() != self.document.atoms.len() {
            return Err(invalid("Reaction drawing position dimensions changed"));
        }
        for (atom, position) in self.document.atoms.iter_mut().zip(positions) {
            atom.position = *position;
        }
        self.finish(labels)
    }
    pub fn finish(self, labels: Labels) -> Result<Document, Error> {
        self.finish_with(labels, |_| Ok(()))
    }
    /// Analyze/export may refresh display labels on a projection, while its
    /// stored XYZ, captured chemical stereo and all paint remain drawing data.
    pub fn finish_preserving(self, labels: Labels, source: &Document) -> Result<Document, Error> {
        let labeled = self.labeled(labels)?;
        if labeled.atoms.len() != source.atoms.len()
            || labeled.bonds.len() != source.bonds.len()
            || !labeled
                .atoms
                .iter()
                .zip(&source.atoms)
                .all(|(a, b)| a.id == b.id)
            || !labeled
                .bonds
                .iter()
                .zip(&source.bonds)
                .all(|(a, b)| (a.a, a.b) == (b.a, b.b))
        {
            return Err(invalid("Projected drawing label identities changed"));
        }
        let mut document = source.clone();
        for (atom, labeled) in document.atoms.iter_mut().zip(labeled.atoms) {
            atom.label_h = labeled.label_h;
            atom.cip_label = labeled.cip_label;
        }
        for (bond, labeled) in document.bonds.iter_mut().zip(labeled.bonds) {
            bond.cip_label = labeled.cip_label;
        }
        document.validate().map_err(Error::Drawing)?;
        Ok(document)
    }
    /// Restore import-owned appearance after CIP labels and before final
    /// validation. The callback only receives a detached draft; an error
    /// publishes no partial drawing and leaves the caller's source unchanged.
    pub fn finish_with<E: From<Error>>(
        self,
        labels: Labels,
        restore: impl FnOnce(&mut Document) -> Result<(), E>,
    ) -> Result<Document, E> {
        let mut document = self.labeled(labels)?;
        restore(&mut document)?;
        document.validate().map_err(Error::Drawing)?;
        Ok(document)
    }
    fn labeled(mut self, labels: Labels) -> Result<Document, Error> {
        if labels.rdkit_version != RDKIT_VERSION
            || labels.atoms.len() != self.document.atoms.len()
            || labels.bonds.len() != self.document.bonds.len()
        {
            return Err(invalid("Drawing label version or dimensions changed"));
        }
        for (atom, label) in self.document.atoms.iter_mut().zip(labels.atoms) {
            atom.cip_label = label;
        }
        let graph = &self.molecule.state.graph;
        let edges: HashSet<_> = graph
            .bonds
            .iter()
            .flat_map(|b| [(b.a, b.b), (b.b, b.a)])
            .collect();
        for (bond, index) in self.document.bonds.iter_mut().zip(&self.bond_indices) {
            let label = at(&labels.bonds, *index)?;
            let chemical = at(&graph.bonds, *index)?;
            if label.stereo > 5
                || !matches!(label.stereo_atoms.len(), 0 | 2)
                || label.stereo_atoms.len() == 2
                    && (!edges.contains(&(chemical.a, *at(&label.stereo_atoms, 0)?))
                        || !edges.contains(&(chemical.b, *at(&label.stereo_atoms, 1)?)))
            {
                return Err(invalid("Invalid labeled bond stereochemistry"));
            }
            bond.cip_label = label.code.clone();
            bond.stereo = stereo_name(label.stereo);
            bond.stereo_atoms = label
                .stereo_atoms
                .iter()
                .map(|&i| at(&self.molecule.ids, i).copied())
                .collect::<Result<Vec<_>, _>>()?;
            if bond.a != *at(&self.molecule.ids, chemical.a)? {
                bond.stereo_atoms.reverse();
            }
        }
        Ok(self.document)
    }
}
fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}
fn stereo_name(code: u8) -> Option<String> {
    match code {
        2 => Some("z"),
        3 => Some("e"),
        4 => Some("cis"),
        5 => Some("trans"),
        _ => None,
    }
    .map(String::from)
}

/// Generate Kekulé and wedge bonds, then reconstruct drawing-owned styles and
/// stable IDs. This path updates an existing drawing without moving its atoms.
pub fn for_drawing(molecule: &Molecule, base: &Document) -> Result<Drawing, Error> {
    validate_molecule(molecule)?;
    base.validate().map_err(Error::Drawing)?;
    let state = &molecule.state;
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    let previous: HashMap<_, _> = base.atoms.iter().map(|a| (a.id, a)).collect();
    if previous.len() != n
        || base.bonds.len() != e
        || molecule.ids.iter().any(|id| !previous.contains_key(id))
    {
        return Err(invalid("Drawing molecule dimensions or identities changed"));
    }
    let options = DrawingOptions::from(&base.drawing_style);
    let work = wedge(molecule, false, vec![false; n], options, false)?;
    let drawing = reconstruct(work, molecule, Some(base), &previous, None, options)?;
    drawing.document.validate().map_err(Error::Drawing)?;
    Ok(drawing)
}

impl Molecule {
    /// Build a label/wedge preparation from chemical tags instead of a possibly
    /// collapsed projection. Publishing the drawing requires finish_preserving.
    pub fn projected_drawing(&self, base: &Document) -> Result<Drawing, Error> {
        validate_molecule(self)?;
        let ranks = self
            .state
            .properties
            .atoms
            .iter()
            .map(|atom| atom.cip_rank)
            .collect::<Vec<_>>();
        let conformer =
            crate::chemistry::depict::compute(&self.state, &ranks, None, Default::default())
                .map_err(|error| invalid(format!("Projected label depiction: {error}")))?;
        let mut detached = self.clone();
        detached.positions = conformer.positions;
        for_drawing(&detached, base)
    }
}

/// Construct a new drawing from an imported molecular state. File coordinates
/// determine the generated wedges; no previous drawing styles override those
/// directions or endpoints. Unexpanded file attachment markers are not the
/// generated attachment atoms that receive different wedge priorities.
pub fn for_import(
    molecule: &Molecule,
    is_3d: bool,
    dummy_labels: &[Option<String>],
) -> Result<Drawing, Error> {
    for_import_with_attachments(
        molecule,
        is_3d,
        dummy_labels,
        vec![false; molecule.ids.len()],
    )
}

pub fn for_import_with_attachments(
    molecule: &Molecule,
    is_3d: bool,
    dummy_labels: &[Option<String>],
    attachments: Vec<bool>,
) -> Result<Drawing, Error> {
    validate_molecule(molecule)?;
    if dummy_labels.len() != molecule.state.graph.atoms.len() {
        return Err(invalid("Imported label dimensions changed"));
    }
    let options = DrawingOptions::default();
    let work = wedge(molecule, is_3d, attachments, options, false)?;
    let drawing = reconstruct(
        work,
        molecule,
        None,
        &HashMap::new(),
        Some(dummy_labels),
        options,
    )?;
    drawing.document.validate().map_err(Error::Drawing)?;
    Ok(drawing)
}

/// CDXML restores the native bond appearance after full CIP reconstruction.
/// Temporary order-zero bonds have plain appearance and cannot be validated as
/// editable bonds yet. Only finish/finish_with may publish this detached draft.
pub fn for_import_scene(molecule: &Molecule, is_3d: bool) -> Result<Drawing, Error> {
    validate_molecule(molecule)?;
    let options = DrawingOptions::default();
    let work = wedge(
        molecule,
        is_3d,
        vec![false; molecule.ids.len()],
        options,
        false,
    )?;
    reconstruct(work, molecule, None, &HashMap::new(), None, options)
}

/// Generate publication stereobonds after layout. Unlike reconstruction of an
/// existing drawing, this may make implicit fusion hydrogens visible.
pub fn for_generated(
    molecule: &Molecule,
    dummy_labels: &[Option<String>],
    options: DrawingOptions,
) -> Result<Drawing, Error> {
    validate_molecule(molecule)?;
    if dummy_labels.len() != molecule.ids.len() {
        return Err(invalid("Imported label dimensions changed"));
    }
    let mut molecule = molecule.clone();
    let mut dummy_labels = dummy_labels.to_vec();
    if options.ring_fusion_hydrogens {
        let expanded =
            wedging::generated::add_fusion_hydrogens(&molecule.state, &molecule.positions, 1.5)
                .map_err(Error::WedgeAssignment)?;
        let mut next = molecule.ids.iter().copied().max().unwrap_or(0);
        for _ in &expanded.parents {
            next = next
                .checked_add(1)
                .ok_or_else(|| invalid("Atom ID overflow"))?;
            molecule.ids.push(next);
            dummy_labels.push(None);
        }
        molecule.state = expanded.state;
        molecule.positions = expanded.positions;
        if !expanded.parents.is_empty() {
            fusion_hydrogens::place(&mut molecule, &expanded.parents, &dummy_labels, options)?;
        }
    }
    let work = wedge(
        &molecule,
        false,
        vec![false; molecule.ids.len()],
        options,
        true,
    )?;
    let drawing = reconstruct(
        work,
        &molecule,
        None,
        &HashMap::new(),
        Some(&dummy_labels),
        options,
    )?;
    drawing.document.validate().map_err(Error::Drawing)?;
    Ok(drawing)
}

pub fn validate_molecule(molecule: &Molecule) -> Result<(), Error> {
    let state = &molecule.state;
    state.graph.validate().map_err(Error::Drawing)?;
    state
        .metadata
        .validate(&state.graph)
        .map_err(Error::Drawing)?;
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    if molecule.rdkit_version != RDKIT_VERSION
        || molecule.ids.len() != n
        || molecule.positions.len() != n
        || molecule.ids.iter().collect::<HashSet<_>>().len() != n
        || state.hybridizations.len() != n
        || state.conjugated.len() != e
        || state.properties.atoms.len() != n
        || state.properties.bond_codes.len() != e
        || state.rings.kind != RingKind::Symmetric
    {
        return Err(invalid("Drawing molecule dimensions or identities changed"));
    }
    if state.graph.atoms.iter().any(|a| a.radical_electrons > 2)
        || state.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
        || state.metadata.bonds.iter().any(|b| b.stereo > 5)
        || !state.metadata.groups.is_empty()
    {
        return Err(invalid(
            "Unsupported drawing stereochemistry or radical count",
        ));
    }
    Ok(())
}

fn wedge(
    molecule: &Molecule,
    is_3d: bool,
    attachment_points: Vec<bool>,
    options: DrawingOptions,
    generated: bool,
) -> Result<Molecule, Error> {
    let state = &molecule.state;
    let kekule = kekule(molecule)?;
    let properties = wedging::WedgeProperties {
        valences: kekule.cache.clone(),
        attachment_points,
    };
    let conformer = wedging::Conformer {
        positions: molecule.positions.clone(),
        is_3d,
    };
    let wedged = wedging::wedge_molecule(
        &wedging::WedgeState {
            graph: kekule.assignment.graph,
            metadata: state.metadata.clone(),
            directions: kekule.assignment.directions,
            rings: state.rings.clone(),
        },
        &properties,
        Some(&conformer),
        options.two_wedge_stereocentres,
    )
    .map_err(Error::WedgeAssignment)?;
    let wedged = if generated && !is_3d {
        wedging::generated::prefer_ring_wedges(&wedged, &properties, &conformer)
            .map_err(Error::WedgeAssignment)?
    } else {
        wedged
    };
    let mut work = molecule.clone();
    work.state.graph = wedged.graph;
    work.state.metadata = wedged.metadata;
    work.state.directions = wedged.directions;
    work.state.rings = wedged.rings;
    work.state.valences = kekule.cache;
    Ok(work)
}

/// Shared canonical bond assignment for drawing reconstruction and MOL output.
pub fn kekule(molecule: &Molecule) -> Result<kekulize::Attempt, Error> {
    let state = &molecule.state;
    let cache = state
        .graph
        .refresh_implicit(&state.valences)
        .map_err(Error::BondAssignment)?;
    let ranks = if state.graph.atoms.iter().any(|a| a.aromatic)
        || state.graph.bonds.iter().any(|b| b.aromatic)
    {
        Some(
            ranking::rank_cached(
                &state.graph,
                &state.rings.atoms,
                &state.metadata,
                ranking::Options {
                    fragment: true,
                    ..Default::default()
                },
                &cache,
            )
            .map_err(Error::BondAssignment)?,
        )
    } else {
        None
    };
    kekulize::assign_cached(
        &state.graph,
        &state.rings.atoms,
        &state.directions,
        kekulize::Options {
            ranks: ranks.as_deref(),
            ..Default::default()
        },
        &state.valences,
    )
    .map_err(Error::BondAssignment)
}

fn reconstruct(
    mut work: Molecule,
    molecule: &Molecule,
    base: Option<&Document>,
    previous: &HashMap<u64, &crate::document::Atom>,
    dummy_labels: Option<&[Option<String>]>,
    options: DrawingOptions,
) -> Result<Drawing, Error> {
    let state = &molecule.state;
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    for properties in &mut work.state.properties.atoms {
        properties.cip_code = None;
    }
    work.state.properties.bond_codes.fill(None);
    let WorkGraph {
        adjacent,
        bonds_by_pair,
    } = work_graph(&work, n)?;
    let circular: HashSet<_> = base
        .into_iter()
        .flat_map(|d| &d.bonds)
        .filter(|b| b.order == 4)
        .flat_map(|b| [b.a, b.b])
        .collect();
    let atoms = reconstruct_atoms(
        &work,
        molecule,
        previous,
        dummy_labels,
        &circular,
        &adjacent,
    )?;
    let ordered = ordered_bonds(base, &bonds_by_pair, e)?;
    let (mut bonds, bond_indices) = reconstruct_bonds(&work, molecule, ordered, e)?;
    if base.is_none() && options.wavy_unknown_alkenes {
        for (i, bond) in work.state.graph.bonds.iter().enumerate() {
            if bond.order == 2 && at(&work.state.metadata.bonds, i)?.stereo == 1 {
                depict_unknown_alkene(&work, i, &mut bonds, true, |_| true)?;
            }
        }
    }
    let mut document = Document {
        version: crate::document::EDITOR_BASELINE,
        atoms,
        bonds,
        ..base.cloned().unwrap_or_default()
    };
    // Kept 3D models still need the version that validates them.
    crate::models3d::require_version(&mut document);
    Ok(Drawing {
        molecule: work,
        document,
        bond_indices,
    })
}

/// Neighbor IDs per atom and the bond index of each unordered ID pair.
struct WorkGraph {
    adjacent: Vec<Vec<u64>>,
    bonds_by_pair: HashMap<(u64, u64), usize>,
}

fn work_graph(work: &Molecule, n: usize) -> Result<WorkGraph, Error> {
    let mut adjacent = vec![Vec::new(); n];
    let mut bonds_by_pair = HashMap::new();
    for (i, bond) in work.state.graph.bonds.iter().enumerate() {
        let a = *at(&work.ids, bond.a)?;
        let b = *at(&work.ids, bond.b)?;
        bonds_by_pair.insert(pair(a, b), i);
        for (index, neighbor) in [(bond.a, b), (bond.b, a)] {
            adjacent
                .get_mut(index)
                .ok_or_else(|| invalid("Missing drawing neighbors"))?
                .push(neighbor);
        }
    }
    Ok(WorkGraph {
        adjacent,
        bonds_by_pair,
    })
}

/// Rebuild each drawing atom from the wedged work state, keeping the previous
/// drawing's display, marks and text style.
fn reconstruct_atoms(
    work: &Molecule,
    molecule: &Molecule,
    previous: &HashMap<u64, &Atom>,
    dummy_labels: Option<&[Option<String>]>,
    circular: &HashSet<u64>,
    adjacent: &[Vec<u64>],
) -> Result<Vec<Atom>, Error> {
    let state = &molecule.state;
    work.state
        .graph
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let id = *at(&work.ids, i)?;
            let metadata = at(&work.state.metadata.atoms, i)?;
            let p = at(&work.positions, i)?;
            let old = previous.get(&id).copied();
            let element = if a.atomic_number == 0 {
                dummy_labels
                    .and_then(|labels| labels.get(i))
                    .and_then(Option::as_deref)
            } else {
                None
            }
            .unwrap_or(at(ELEMENTS, usize::from(a.atomic_number))?.symbol);
            let mut atom = Atom {
                depth: 0.,
                centroid: vec![],
                attachment: None,
                id,
                element: element.into(),
                position: Point {
                    x: (p.x * WORLD_PER_DEPICT_UNIT) as f32,
                    y: (-p.y * WORLD_PER_DEPICT_UNIT) as f32,
                },
                charge: i32::from(a.charge),
                isotope: u32::from(a.isotope),
                radical_electrons: a.radical_electrons,
                singlet: a.radical_electrons == 2 && old.is_some_and(|a| a.singlet),
                explicit_h: u32::from(a.explicit_hydrogens),
                no_implicit: a.no_implicit,
                aromatic: a.aromatic,
                map_num: u32::try_from(metadata.map_number)
                    .map_err(|_| invalid("Negative atom map"))?,
                stereo: None,
                label_h: 0,
                cip_label: None,
                display: old.map(|a| a.display.clone()).unwrap_or_default(),
                marks: old.map(|a| a.marks.clone()).unwrap_or_default(),
                mark_serial: old.map_or(0, |a| a.mark_serial),
                text_style: old.and_then(|a| a.text_style.clone()),
            };
            atom.explicit_h = u32::from(if circular.contains(&id) {
                at(&state.graph.atoms, i)?.explicit_hydrogens
            } else {
                a.explicit_hydrogens
            });
            atom.stereo = match metadata.chiral_tag {
                0 => None,
                tag => Some(AtomStereo {
                    winding: if tag == 1 { "cw" } else { "ccw" }.into(),
                    neighbors: at(adjacent, i)?.clone(),
                }),
            };
            atom.label_h = u32::from(a.explicit_hydrogens)
                .checked_add(at(&work.state.valences, i)?.implicit_hydrogens)
                .ok_or_else(|| invalid("Drawing hydrogen count overflow"))?;
            atom.cip_label = None;
            Ok(atom)
        })
        .collect::<Result<Vec<_>, Error>>()
}

/// Bond indices in the base drawing's order, or in graph order on import.
fn ordered_bonds<'b>(
    base: Option<&'b Document>,
    bonds_by_pair: &HashMap<(u64, u64), usize>,
    e: usize,
) -> Result<Vec<(usize, Option<&'b Bond>)>, Error> {
    let ordered = if let Some(base) = base {
        base.bonds
            .iter()
            .map(|old| {
                let index = *bonds_by_pair
                    .get(&pair(old.a, old.b))
                    .ok_or_else(|| invalid("Drawing bond endpoints changed"))?;
                Ok((index, Some(old)))
            })
            .collect::<Result<Vec<_>, Error>>()?
    } else {
        (0..e).map(|i| (i, None)).collect()
    };
    Ok(ordered)
}

/// Rebuild each drawing bond in `ordered`, recording its graph index.
fn reconstruct_bonds(
    work: &Molecule,
    molecule: &Molecule,
    ordered: Vec<(usize, Option<&Bond>)>,
    e: usize,
) -> Result<(Vec<Bond>, Vec<usize>), Error> {
    let state = &molecule.state;
    let mut bond_indices = Vec::with_capacity(e);
    let bonds = ordered
        .into_iter()
        .map(|(i, old)| {
            bond_indices.push(i);
            let b = at(&work.state.graph.bonds, i)?;
            let meta = at(&work.state.metadata.bonds, i)?;
            let mut bond = Bond {
                distance_derived: false,
                highlight: None,
                ring_arc: false,
                projection: false,
                stereo_authoritative: false,
                a: *at(&work.ids, b.a)?,
                b: *at(&work.ids, b.b)?,
                order: b.order,
                display: if meta.stereo == 1 {
                    "wavy"
                } else {
                    match at(&work.state.directions, i)? {
                        crate::chemistry::kekulize::Direction::Wedge => "wedge",
                        crate::chemistry::kekulize::Direction::Hash => "hash",
                        crate::chemistry::kekulize::Direction::Unknown => "wavy",
                        _ => "plain",
                    }
                }
                .into(),
                stereo: None,
                stereo_atoms: Vec::new(),
                cip_label: None,
                double_position: Default::default(),
                secondary_display: None,
                color: Default::default(),
                indicator: Default::default(),
                z_order: 0,
            };
            if let Some(old) = old {
                bond = old.clone();
            }
            // A normalized dative bond has chemical direction. Restoring an
            // old metal-first single bond must not reverse donor -> acceptor.
            if b.order == 5 {
                bond.a = *at(&work.ids, b.a)?;
                bond.b = *at(&work.ids, b.b)?;
            }
            bond.order = if let Some(old) = old.filter(|old| matches!(old.order, 1 | 2 | 4))
                && at(&state.graph.bonds, i)?.aromatic
            {
                old.order
            } else {
                b.order
            };
            bond.stereo = match meta.stereo {
                2 => Some("z"),
                3 => Some("e"),
                4 => Some("cis"),
                5 => Some("trans"),
                _ => None,
            }
            .map(String::from);
            bond.stereo_atoms = meta
                .stereo_atoms
                .iter()
                .map(|&i| at(&work.ids, i).copied())
                .collect::<Result<Vec<_>, _>>()?;
            if bond.a != *at(&work.ids, b.a)? {
                bond.stereo_atoms.reverse();
            }
            bond.cip_label = None;
            Ok(bond)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok((bonds, bond_indices))
}

/// Mark an unspecified alkene, choosing an unambiguous substituent when the
/// document prefers a wavy single bond. Returns whether that depiction was used.
/// `can_change` protects fixed substituents during partial cleanup.
pub fn depict_unknown_alkene(
    molecule: &Molecule,
    index: usize,
    bonds: &mut [Bond],
    wavy: bool,
    can_change: impl Fn(&Bond) -> bool,
) -> Result<bool, Error> {
    let state = &molecule.state;
    let double = at(&state.graph.bonds, index)?;
    let double_pair = pair(*at(&molecule.ids, double.a)?, *at(&molecule.ids, double.b)?);
    let target = bonds
        .iter()
        .position(|bond| pair(bond.a, bond.b) == double_pair)
        .ok_or_else(|| invalid("Missing unknown double bond"))?;
    let mut candidates = Vec::new();
    if wavy
        || bonds
            .iter()
            .any(|bond| bond.order == 1 && bond.display == "wavy")
    {
        // Drawing preparation does not flag unassigned stereocentres.
        let possible = crate::chemistry::stereo::perception::perceive(
            state,
            crate::chemistry::stereo::perception::Options {
                clean: false,
                force: true,
                flag_possible: true,
            },
        )
        .map_err(Error::WedgeAssignment)?
        .properties
        .atoms;
        for end in [double.a, double.b] {
            for bond in &state.graph.bonds {
                let other = if bond.a == end {
                    bond.b
                } else if bond.b == end {
                    bond.a
                } else {
                    continue;
                };
                if bond.order != 1
                    || state.rings.atoms.iter().any(|ring| ring.contains(&other))
                    || at(&state.metadata.atoms, other)?.chiral_tag != 0
                    || at(&possible, other)?.possible == Some(true)
                    || state
                        .graph
                        .bonds
                        .iter()
                        .any(|bond| bond.order == 2 && (bond.a == other || bond.b == other))
                {
                    continue;
                }
                let endpoints = pair(*at(&molecule.ids, end)?, *at(&molecule.ids, other)?);
                if let Some(position) = bonds.iter().position(|bond| {
                    pair(bond.a, bond.b) == endpoints
                        && matches!(bond.display.as_str(), "plain" | "wavy")
                        && !bond.projection
                        && (bond.display == "wavy" || can_change(bond))
                }) {
                    candidates.push((
                        position,
                        *at(&molecule.ids, end)?,
                        *at(&molecule.ids, other)?,
                    ));
                }
            }
        }
    }
    // Reuse an existing marker, including a fixed one, before changing a plain
    // substituent. Remove obsolete markers only where cleanup permits edits.
    let substituent = if wavy {
        candidates
            .iter()
            .find(|&&(position, _, _)| {
                bonds
                    .get(position)
                    .is_some_and(|bond| bond.display == "wavy")
            })
            .or_else(|| candidates.first())
            .copied()
    } else {
        None
    };
    for &(position, _, _) in &candidates {
        let bond = bonds
            .get_mut(position)
            .ok_or_else(|| invalid("Missing alkene substituent"))?;
        if bond.display == "wavy"
            && substituent.is_none_or(|(chosen, _, _)| chosen != position)
            && can_change(bond)
        {
            bond.display = "plain".into();
            bond.stereo = None;
            bond.stereo_atoms.clear();
        }
    }
    let bond = bonds
        .get_mut(target)
        .ok_or_else(|| invalid("Missing unknown double bond"))?;
    bond.display = if substituent.is_some() {
        "plain"
    } else {
        "wavy"
    }
    .into();
    bond.stereo = None;
    bond.stereo_atoms.clear();
    if let Some((position, a, b)) = substituent {
        let bond = bonds
            .get_mut(position)
            .ok_or_else(|| invalid("Missing alkene substituent"))?;
        if can_change(bond) {
            bond.a = a;
            bond.b = b;
            bond.display = "wavy".into();
            bond.stereo = None;
            bond.stereo_atoms.clear();
        }
    }
    Ok(substituent.is_some())
}
