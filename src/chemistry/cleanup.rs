//! Selection-preserving cleanup, detached from the two-dimensional solver.
//! The caller supplies native-order conformers, then analyzes the verified
//! original-precision molecule. No partial drawing is ever published on error.
mod numeric;
mod warnings;
use super::{
    cdxml::native_hypot,
    depict::geometry::{Coordinates, Point as NativePoint},
    document::{self as molecular, Molecule},
    kekulize::Direction,
    smiles::write,
    stereo::{Point3, wedging},
};
use crate::{
    cleanup::{Options, Scope},
    document::Document,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Cleanup selection contains unavailable atoms")]
    Selection,
    #[error("Select atoms to clean up")]
    Select,
    #[error("Draw or select a molecule first")]
    Empty,
    #[error("Invalid abbreviation in cleanup drawing")]
    Abbreviation,
    #[error("Cleanup could not preserve the fixed atoms; choose Selected molecules")]
    Fixed,
    #[error("Cleanup could not preserve stereochemistry; try including the whole molecule")]
    ChemistryChanged,
    #[error("Cleanup could not preserve visible stereochemistry; include the whole molecule")]
    VisibleChanged,
    #[error("Cleanup layout dimensions, identities, or coordinates changed")]
    Layout,
    #[error("Cleanup exceeds its atom, bond, selection, or work limit")]
    Limit,
    #[error("Invalid cleanup drawing: {0}")]
    Document(String),
    #[error(transparent)]
    Chemistry(#[from] molecular::Error),
    #[error(transparent)]
    Smiles(#[from] write::Error),
    #[error("Cleanup rewedge failed: {0}")]
    Wedge(String),
}
impl Error {
    /// Original public chemical diagnostics, retaining this error's typed cause.
    /// Native implementation assertions use truthful Rust messages instead.
    pub(crate) fn diagnostic(&self) -> String {
        if let Self::Chemistry(error) = self
            && let Some(message) = warnings::native_message(error)
        {
            return message;
        }
        self.to_string()
    }
}
type Result<T> = std::result::Result<T, Error>;

struct Part {
    base: Document,
    molecule: Molecule,
    identity: String,
    moving: HashSet<u64>,
    fixed: Coordinates,
}

/// One Compute2DCoords call, before cleanup's rigid orientation. The solver
/// must preserve atom order and use the fixed map unchanged. Coordination
/// templates use the requested bond length, independently of previous calls.
#[derive(serde::Serialize)]
pub struct LayoutRequest<'a> {
    pub molecule: &'a Molecule,
    pub fixed: &'a Coordinates,
    pub bond_length: f64,
    pub canonical_orientation: bool,
    pub use_ring_templates: bool,
    pub force_rdkit: bool,
}

pub struct Prepared {
    source: Document,
    options: Options,
    parts: Vec<Part>,
    deferred_error: Option<Error>,
    bond_length: f64,
    preserved_attachments: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisPolicy {
    Required,
    /// Only original chemistry failures may become warnings. Helper discovery,
    /// transport, cancellation, resource failures and invariant errors propagate.
    SelectedChemistry,
    /// Semantic attachment components retain their original geometry and graph.
    /// Ordinary molecular identifiers cannot represent their target lists.
    RetainedAttachments,
}

/// A complete atomic edit. `molecule` retains f64 positions from the original
/// internal drawing, before the final response narrows its document to f32.
#[derive(Debug)]
pub struct Cleaned {
    pub document: Document,
    pub molecule: Option<Molecule>,
    pub warnings: Vec<String>,
    pub analysis_policy: AnalysisPolicy,
    /// The original typed chemistry failure behind an optional analysis warning.
    pub analysis_failure: Option<molecular::Error>,
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}
fn identity(molecule: &Molecule) -> Result<String> {
    Ok(write::write(&molecule.state, Default::default())?.text)
}
fn positions(document: &Document) -> Vec<Point3> {
    document
        .atoms
        .iter()
        .map(|a| Point3 {
            x: f64::from(a.position.x) / 28.,
            y: -f64::from(a.position.y) / 28.,
            z: 0.,
        })
        .collect()
}

/// Validate the app drawing and selection, then prepare only affected chemical
/// components. An invalid unselected molecule must not prevent selected cleanup.
pub fn prepare(source: &Document, options: Options, selection: &[u64]) -> Result<Prepared> {
    if source.atoms.len() > 100_000 || source.bonds.len() > 300_000 || selection.len() > 1_000_000 {
        return Err(Error::Limit);
    }
    source.validate().map_err(Error::Document)?;
    let indices: HashMap<_, _> = source
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let mut selected: HashSet<_> = selection.iter().copied().collect();
    if selected.iter().any(|id| !indices.contains_key(id)) {
        return Err(Error::Selection);
    }
    if options.scope != Scope::Drawing && selected.is_empty() {
        return Err(Error::Select);
    }
    for group in &source.abbreviations {
        if group.members.iter().any(|id| selected.contains(id)) {
            selected.extend(group.members.iter().copied());
        }
    }
    let mut adjacent = vec![Vec::new(); source.atoms.len()];
    // Haptic groups connect through target membership, not covalent bonds.
    // Keep that component intact without inventing metal–carbon bonds.
    for (count, (a, b)) in crate::attachments::edges(source).enumerate() {
        if count >= 600_000 {
            return Err(Error::Limit);
        }
        let a = *indices.get(&a).ok_or(Error::Layout)?;
        let b = *indices.get(&b).ok_or(Error::Layout)?;
        adjacent.get_mut(a).ok_or(Error::Layout)?.push(b);
        adjacent.get_mut(b).ok_or(Error::Layout)?.push(a);
    }
    let mut owners = vec![None; source.atoms.len()];
    let mut bases = Vec::<Document>::new();
    for start in 0..source.atoms.len() {
        if owners.get(start).ok_or(Error::Layout)?.is_some() {
            continue;
        }
        let owner = bases.len();
        bases.push(Document {
            version: source.version,
            atom_labels: source.atom_labels.clone(),
            ..Default::default()
        });
        let mut pending = vec![start];
        *owners.get_mut(start).ok_or(Error::Layout)? = Some(owner);
        while let Some(index) = pending.pop() {
            for &other in adjacent.get(index).ok_or(Error::Layout)? {
                let slot = owners.get_mut(other).ok_or(Error::Layout)?;
                if slot.is_none() {
                    *slot = Some(owner);
                    pending.push(other);
                }
            }
        }
    }
    let owner = |id: u64| -> Result<usize> {
        let index = *indices.get(&id).ok_or(Error::Layout)?;
        owners.get(index).copied().flatten().ok_or(Error::Layout)
    };
    for atom in &source.atoms {
        bases
            .get_mut(owner(atom.id)?)
            .ok_or(Error::Layout)?
            .atoms
            .push(atom.clone());
    }
    for bond in &source.bonds {
        bases
            .get_mut(owner(bond.a)?)
            .ok_or(Error::Layout)?
            .bonds
            .push(bond.clone());
    }
    for group in &source.abbreviations {
        let component = owner(group.anchor)?;
        if group
            .members
            .iter()
            .any(|id| owner(*id).ok() != Some(component))
        {
            return Err(Error::Abbreviation);
        }
        bases
            .get_mut(component)
            .ok_or(Error::Layout)?
            .abbreviations
            .push(group.clone());
    }
    let mut parts = Vec::new();
    let mut deferred_error = None;
    let mut preserved_attachments = 0;
    for base in bases {
        if options.scope != Scope::Drawing && !base.atoms.iter().any(|a| selected.contains(&a.id)) {
            continue;
        }
        // The ordinary solver and its identity check cannot represent ALL/ANY
        // attachment semantics. Preserve the entire connected structure while
        // allowing independent ordinary molecules to be cleaned normally.
        if crate::attachments::present(&base) {
            preserved_attachments += 1;
            continue;
        }
        let moving: HashSet<_> = base
            .atoms
            .iter()
            .filter(|a| options.scope != Scope::SelectedAtoms || selected.contains(&a.id))
            .map(|a| a.id)
            .collect();
        let prepared = molecular::prepare(&base)
            .map_err(Error::from)
            .and_then(|molecule| {
                let identity = identity(&molecule)?;
                Ok((molecule, identity))
            });
        let (molecule, identity) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                // Earlier components still undergo layout and stereo checks
                // before the original loop reaches this chemistry failure.
                deferred_error = Some(error);
                break;
            }
        };
        let fixed = molecule
            .ids
            .iter()
            .zip(&molecule.positions)
            .enumerate()
            .filter(|(_, (id, _))| !moving.contains(id))
            .map(|(i, (_, p))| (i, NativePoint { x: p.x, y: p.y }))
            .collect();
        parts.push(Part {
            base,
            molecule,
            identity,
            moving,
            fixed,
        });
    }
    if parts.is_empty() && deferred_error.is_none() && preserved_attachments == 0 {
        return Err(Error::Empty);
    }
    Ok(Prepared {
        source: source.clone(),
        options,
        parts,
        deferred_error,
        bond_length: f64::from(source.drawing_style.bond_length_world) / 28.,
        preserved_attachments,
    })
}

impl Prepared {
    /// Requests before the first component preparation failure. Always call
    /// `finish`, including for an empty iterator: a later error is retained so
    /// earlier components undergo their native-order geometry/stereo checks.
    pub fn requests(&self) -> impl ExactSizeIterator<Item = LayoutRequest<'_>> {
        self.parts.iter().map(|part| LayoutRequest {
            molecule: &part.molecule,
            fixed: &part.fixed,
            bond_length: self.bond_length,
            canonical_orientation: false,
            use_ring_templates: true,
            force_rdkit: true,
        })
    }

    pub fn finish(self, layouts: Vec<Vec<Point3>>) -> Result<Cleaned> {
        if layouts.len() > self.parts.len() {
            return Err(Error::Layout);
        }
        let mut layouts = layouts.into_iter();
        self.finish_with(|_| layouts.next().ok_or(Error::Layout))
    }

    /// Lay out and verify each component before starting the next one. This
    /// preserves the original first failure when a later solver call could fail.
    /// Callback errors remain typed and no intermediate drawing is published.
    pub fn finish_with<E>(
        self,
        mut compute: impl FnMut(LayoutRequest<'_>) -> std::result::Result<Vec<Point3>, E>,
    ) -> std::result::Result<Cleaned, E>
    where
        E: From<Error>,
    {
        let mut result = self.source;
        let mut result_positions = positions(&result);
        let atom_indices: HashMap<_, _> = result
            .atoms
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id, i))
            .collect();
        let bond_indices: HashMap<_, _> = result
            .bonds
            .iter()
            .enumerate()
            .map(|(i, b)| (pair(b.a, b.b), i))
            .collect();
        let mut crossed = 0usize;
        let mut work = 50_000_000usize;
        for part in self.parts {
            let mut layout = compute(LayoutRequest {
                molecule: &part.molecule,
                fixed: &part.fixed,
                bond_length: self.bond_length,
                canonical_orientation: false,
                use_ring_templates: true,
                force_rdkit: true,
            })?;
            if layout.len() != part.molecule.ids.len()
                || layout
                    .iter()
                    .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.z != 0.)
            {
                return Err(Error::Layout.into());
            }
            numeric::orient(
                &part.molecule.positions,
                &mut layout,
                &part.fixed,
                self.options.keep_orientation,
            )?;
            for (&index, fixed) in &part.fixed {
                let point = layout.get_mut(index).ok_or(Error::Layout)?;
                if native_hypot(point.x - fixed.x, point.y - fixed.y) > 1e-6 {
                    return Err(Error::Fixed.into());
                }
                *point = Point3 {
                    x: fixed.x,
                    y: fixed.y,
                    z: 0.,
                };
            }
            let mut molecule = part.molecule;
            molecule.positions = layout;
            let drawing = molecular::for_drawing(&molecule, &part.base).map_err(Error::from)?;
            let displays = rewedge(drawing.molecule(), &part.base, &mut work)?;
            let labels = drawing.labels().map_err(Error::from)?;
            let mut generated = drawing.finish(labels).map_err(Error::from)?;
            for (bond, display) in generated.bonds.iter_mut().zip(displays) {
                bond.display = display;
            }
            let mut merged = part.base.clone();
            let mut raw = positions(&part.base);
            for (((target, generated), p), raw) in merged
                .atoms
                .iter_mut()
                .zip(generated.atoms)
                .zip(&molecule.positions)
                .zip(&mut raw)
            {
                if part.moving.contains(&target.id) {
                    *target = generated;
                    // The original writer multiplies by SCALE; read() then
                    // divides those f64 values before either stereo check.
                    *raw = Point3 {
                        x: (p.x * 28.) / 28.,
                        y: -(-p.y * 28.) / 28.,
                        z: 0.,
                    };
                }
            }
            for (target, generated) in merged.bonds.iter_mut().zip(generated.bonds) {
                if part.moving.contains(&target.a) || part.moving.contains(&target.b) {
                    *target = generated;
                }
            }
            let checked = molecular::prepare_at(&merged, &raw).map_err(Error::from)?;
            for ((old, new), bond) in molecule
                .state
                .metadata
                .bonds
                .iter()
                .zip(&checked.state.metadata.bonds)
                .zip(&mut merged.bonds)
            {
                if old.stereo <= 1
                    && new.stereo > 1
                    && (part.moving.contains(&bond.a) || part.moving.contains(&bond.b))
                {
                    bond.display = "wavy".into();
                    bond.stereo = None;
                    bond.stereo_atoms.clear();
                    crossed += 1;
                }
            }
            if identity(&molecular::prepare_at(&merged, &raw).map_err(Error::from)?)?
                != part.identity
            {
                return Err(Error::ChemistryChanged.into());
            }
            let mut before = part.base;
            let mut after = merged.clone();
            for drawing in [&mut before, &mut after] {
                for atom in &mut drawing.atoms {
                    atom.stereo = None;
                }
                for bond in &mut drawing.bonds {
                    bond.stereo = None;
                    bond.stereo_atoms.clear();
                }
            }
            if identity(&molecular::prepare(&before).map_err(Error::from)?)?
                != identity(&molecular::prepare_at(&after, &raw).map_err(Error::from)?)?
            {
                return Err(Error::VisibleChanged.into());
            }
            for (atom, point) in merged.atoms.into_iter().zip(raw) {
                if part.moving.contains(&atom.id) {
                    let index = *atom_indices.get(&atom.id).ok_or(Error::Layout)?;
                    *result.atoms.get_mut(index).ok_or(Error::Layout)? = atom;
                    *result_positions.get_mut(index).ok_or(Error::Layout)? = point;
                }
            }
            for bond in merged.bonds {
                if part.moving.contains(&bond.a) || part.moving.contains(&bond.b) {
                    let index = *bond_indices
                        .get(&pair(bond.a, bond.b))
                        .ok_or(Error::Layout)?;
                    *result.bonds.get_mut(index).ok_or(Error::Layout)? = bond;
                }
            }
        }
        if let Some(error) = self.deferred_error {
            return Err(error.into());
        }
        result.version = 15;
        result.validate().map_err(Error::Document)?;
        let mut warnings = if crossed != 0 {
            vec![format!(
                "{crossed} double bond(s) use a crossed depiction to keep unspecified stereochemistry."
            )]
        } else {
            vec![]
        };
        if self.preserved_attachments != 0 {
            warnings.push(format!(
                "{} attachment-containing structure(s) kept unchanged. Automatic cleanup of multi-center and variable attachments is not yet supported.",
                self.preserved_attachments
            ));
        }
        if crate::attachments::present(&result) {
            warnings.push("Attachment targets and charges retained; molecular identifiers and coordination-valence analysis are unavailable for this drawing.".into());
            return Ok(Cleaned {
                document: result,
                molecule: None,
                warnings,
                analysis_policy: AnalysisPolicy::RetainedAttachments,
                analysis_failure: None,
            });
        }
        let analysis_policy = if self.options.scope == Scope::Drawing {
            AnalysisPolicy::Required
        } else {
            AnalysisPolicy::SelectedChemistry
        };
        let (molecule, analysis_failure) = match molecular::prepare_at(&result, &result_positions) {
            Ok(molecule) => (Some(molecule), None),
            Err(error) => {
                if analysis_policy == AnalysisPolicy::Required {
                    return Err(Error::from(error).into());
                }
                let Some(message) = warnings::native_message(&error) else {
                    return Err(Error::from(error).into());
                };
                warnings.push(format!("Selected geometry cleaned. Another part of the drawing needs checking: {message}"));
                (None, Some(error))
            }
        };
        Ok(Cleaned {
            document: result,
            molecule,
            warnings,
            analysis_policy,
            analysis_failure,
        })
    }
}

fn rewedge(molecule: &Molecule, base: &Document, work: &mut usize) -> Result<Vec<String>> {
    let state = &molecule.state;
    let mut wedge = wedging::WedgeState {
        graph: state.graph.clone(),
        metadata: state.metadata.clone(),
        directions: state.directions.clone(),
        rings: state.rings.clone(),
    };
    let properties = wedging::WedgeProperties {
        valences: state.valences.clone(),
        attachment_points: vec![false; molecule.ids.len()],
    };
    let conformer = wedging::Conformer {
        positions: molecule.positions.clone(),
        is_3d: false,
    };
    let indices: HashMap<_, _> = molecule
        .ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let mut result = Vec::new();
    for (i, bond) in base.bonds.iter().enumerate() {
        let mut display = bond.display.clone();
        let atom = *indices.get(&bond.a).ok_or(Error::Layout)?;
        if bond.order == 1
            && !bond.projection
            && matches!(
                display.as_str(),
                "wedge" | "hash" | "hollow_wedge" | "hashed" | "bold"
            )
            && state
                .metadata
                .atoms
                .get(atom)
                .ok_or(Error::Layout)?
                .chiral_tag
                != 0
        {
            let storage = state.rings.atoms.iter().try_fold(
                molecule.ids.len().saturating_add(base.bonds.len()),
                |size, ring| size.checked_add(ring.len()).ok_or(Error::Limit),
            )?;
            *work = work.checked_sub(storage).ok_or(Error::Limit)?;
            wedge = wedging::wedge_bond(&wedge, &properties, &conformer, i, atom)
                .map_err(Error::Wedge)?;
            let up = wedge.directions.get(i) == Some(&Direction::Wedge);
            display = match (display.as_str(), up) {
                ("bold", true) => "bold",
                ("bold", false) => "hashed",
                ("hollow_wedge" | "hashed", true) => "hollow_wedge",
                ("hollow_wedge" | "hashed", false) => "hashed",
                (_, true) => "wedge",
                (_, false) => "hash",
            }
            .into();
        }
        result.push(display);
    }
    Ok(result)
}
