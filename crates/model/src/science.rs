//! Scientific frames and calculation results kept with molecules.
//!
//! The drawing never changes for a calculation: frames (exact `f64`
//! geometries) and results live in [`Science`] beside it, immutable and shared
//! by `Arc` between the live document, undo history and clipboard copies of
//! the document. An [`Attachment`] links one frame's atoms to drawing atoms
//! and records the molecule's chemistry when the frame was made, so any
//! chemical edit makes its results stale and Undo makes them current again.
//! The design is `docs/plans/xtb-data.md`, section Documents.
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use reshiki_qcdata::{CalcResult, Frame, FrameHash, MAX_IMPORTED_HEAVY_BYTES, Settings};
use serde::{Deserialize, Serialize};

use crate::document::Document;

/// Heap bytes of frames and results the live document may hold.
pub const MAX_SCIENCE_BYTES: usize = 256 << 20;
/// Saved results above this size make Save offer "Make lighter".
pub const LIGHTER_BYTES: usize = 5_000_000;

/// Frames, results and their links to the drawing. Empty in drawings
/// without calculations, and then absent from the file.
#[derive(Debug, Clone, Default)]
pub struct Science {
    /// Every stored geometry, at most once per hash.
    pub frames: Vec<Arc<Frame>>,
    /// Results in the order they arrived.
    pub results: Vec<Arc<CalcResult>>,
    /// Links from frames to drawing atoms.
    pub attachments: Vec<Attachment>,
}

/// One frame linked to one molecule of the drawing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Unique among the document's attachments.
    pub id: u64,
    /// The linked frame.
    pub frame: FrameHash,
    /// The drawing atom of each frame atom; `None` for atoms the drawing
    /// does not show (implicit hydrogens) and for deleted atoms.
    pub atoms: Vec<Option<u64>>,
    /// Chemistry fingerprint of the linked atoms when the frame was made.
    pub baseline: String,
}

/// Why a calculation cannot start or its result cannot be stored or applied.
#[derive(Debug, thiserror::Error)]
pub enum ScienceError {
    #[error("The molecule has no 3D model")]
    NoModel,
    #[error("The 3D model is out of date; update it first")]
    StaleModel,
    #[error("Calculations on crystal structures are not supported")]
    Periodic,
    #[error("The molecule was deleted")]
    Gone,
    #[error("The molecule changed after this calculation")]
    Stale,
    #[error("The 3D model no longer matches this calculation")]
    ModelMismatch,
    #[error("Calculation results would exceed {} MiB in this document", MAX_SCIENCE_BYTES >> 20)]
    TooLarge,
    #[error("Unknown calculation data")]
    Unknown,
    #[error(transparent)]
    Invalid(#[from] reshiki_qcdata::Error),
}

/// A calculation's starting point, taken from the document when it starts.
/// It never changes the document; [`attach`] stores it with the result.
#[derive(Debug, Clone)]
pub struct Launch {
    /// The exact input geometry.
    pub frame: Arc<Frame>,
    /// Drawing atom of each frame atom.
    pub atoms: Vec<Option<u64>>,
    /// The molecule's chemistry fingerprint at launch: the revision a late
    /// result is checked against.
    pub baseline: String,
}

/// Whether a stored result describes the molecule as it is now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Current,
    /// The molecule changed after launch; the result is kept, greyed.
    Stale,
}

/// What [`attach`] stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attached {
    pub freshness: Freshness,
    /// The attachment of the result's own frame (an optimization's final
    /// geometry), for [`apply_geometry`].
    pub attachment: u64,
}

/// A result as the results palette shows it.
#[derive(Debug, Clone)]
pub struct ResultView<'a> {
    pub result: &'a Arc<CalcResult>,
    pub frame: &'a Arc<Frame>,
    pub attachment: &'a Attachment,
    pub freshness: Freshness,
}

/// A stereo configuration a frame does not share with the drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StereoChange {
    /// A drawn stereocentre is inverted, or flat in the frame.
    Centre { atom: u64, inverted: bool },
    /// A drawn E/Z double bond has the other configuration in the frame.
    DoubleBond { atoms: [u64; 2] },
}

impl PartialEq for Science {
    /// Snapshots share their payloads, so pointer equality decides almost
    /// every comparison without reading the arrays.
    fn eq(&self, other: &Self) -> bool {
        fn same<T: PartialEq>(a: &[Arc<T>], b: &[Arc<T>]) -> bool {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| Arc::ptr_eq(a, b) || a == b)
        }
        same(&self.frames, &other.frames)
            && same(&self.results, &other.results)
            && self.attachments == other.attachments
    }
}

impl Science {
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty() && self.results.is_empty() && self.attachments.is_empty()
    }

    pub fn frame(&self, hash: &FrameHash) -> Option<&Arc<Frame>> {
        self.frames.iter().find(|frame| frame.hash() == *hash)
    }

    pub fn attachment(&self, id: u64) -> Option<&Attachment> {
        self.attachments.iter().find(|a| a.id == id)
    }

    /// Each distinct payload once, as (address, heap bytes), for history
    /// accounting. An address stays unique while its `Arc` is held.
    pub(crate) fn payloads(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let frames = self
            .frames
            .iter()
            .map(|f| (Arc::as_ptr(f) as *const () as usize, f.heap_bytes()));
        let results = self
            .results
            .iter()
            .map(|r| (Arc::as_ptr(r) as *const () as usize, r.heap_bytes()));
        frames.chain(results)
    }

    /// Bytes a snapshot owns itself: the `Arc` lists and the attachments.
    pub(crate) fn container_bytes(&self) -> usize {
        use std::mem::{size_of, size_of_val};
        size_of_val(self.frames.as_slice())
            + size_of_val(self.results.as_slice())
            + size_of_val(self.attachments.as_slice())
            + self
                .attachments
                .iter()
                .map(|a| size_of_val(a.atoms.as_slice()) + a.baseline.len())
                .sum::<usize>()
            + size_of::<Self>()
    }

    /// Heap bytes of the frames and results, each counted once.
    pub fn heap_bytes(&self) -> usize {
        let mut seen = HashSet::new();
        self.payloads()
            .filter(|(address, _)| seen.insert(*address))
            .map(|(_, bytes)| bytes)
            .sum()
    }

    /// The form each result is saved in. Imported results that cannot be
    /// recomputed keep their heavy arrays while their molecule's imported
    /// data stays within [`MAX_IMPORTED_HEAVY_BYTES`], in arrival order.
    fn saved_forms(&self) -> Vec<Cow<'_, CalcResult>> {
        let mut budgets = HashMap::<Vec<u64>, usize>::new();
        self.results
            .iter()
            .map(|result| {
                let budget = budgets
                    .entry(self.molecule_key(result))
                    .or_insert(MAX_IMPORTED_HEAVY_BYTES);
                let saved = result.for_file(*budget, &[]);
                if matches!(result.settings, Settings::Imported { .. })
                    && matches!(saved, Cow::Borrowed(_))
                {
                    *budget = budget.saturating_sub(result.heavy_bytes());
                }
                saved
            })
            .collect()
    }

    /// The sorted drawing atoms of the molecule a result belongs to.
    fn molecule_key(&self, result: &CalcResult) -> Vec<u64> {
        let mut key: Vec<u64> = self
            .attachments
            .iter()
            .find(|a| belongs(result, a))
            .map(|a| a.atoms.iter().flatten().copied().collect())
            .unwrap_or_default();
        key.sort_unstable();
        key
    }

    /// Pretty-printed JSON bytes the file spends on this result ("Saved in
    /// file: 84 KB"). Results compare by pointer, as the palette holds them.
    pub fn saved_bytes(&self, result: &Arc<CalcResult>) -> Option<usize> {
        let index = self.results.iter().position(|r| Arc::ptr_eq(r, result))?;
        let forms = self.saved_forms();
        Some(
            serde_json::to_vec_pretty(&forms[index])
                .map(|json| json.len())
                .unwrap_or(0),
        )
    }

    /// Pretty-printed JSON bytes of everything this block saves; Save offers
    /// [`make_lighter`] above [`LIGHTER_BYTES`].
    pub fn file_bytes(&self) -> usize {
        serde_json::to_vec_pretty(self)
            .map(|json| json.len())
            .unwrap_or(0)
    }

    /// Check what a file, the clipboard or a draft supplied: every frame and
    /// result valid, every reference resolved, the heap budget kept.
    fn validate(&self) -> Result<(), String> {
        let mut hashes = HashSet::new();
        for frame in &self.frames {
            frame.validate().map_err(|e| e.to_string())?;
            if !hashes.insert(frame.hash()) {
                return Err("Calculation data repeats a frame".into());
            }
        }
        let missing = || "Calculation data refers to a missing frame".to_string();
        for result in &self.results {
            let frame = self.frame(&result.frame).ok_or_else(missing)?;
            result.validate(frame).map_err(|e| e.to_string())?;
            if referenced(result).any(|hash| !hashes.contains(&hash)) {
                return Err(missing());
            }
        }
        let mut ids = HashSet::new();
        for attachment in &self.attachments {
            let frame = self.frame(&attachment.frame).ok_or_else(missing)?;
            let mut atoms = HashSet::new();
            if !ids.insert(attachment.id)
                || attachment.atoms.len() != frame.numbers.len()
                || !attachment.atoms.iter().flatten().all(|id| atoms.insert(*id))
                || attachment.baseline.is_empty()
            {
                return Err("Invalid calculation attachment".into());
            }
        }
        if self.heap_bytes() > MAX_SCIENCE_BYTES {
            return Err(ScienceError::TooLarge.to_string());
        }
        Ok(())
    }

    /// Drop attachments without drawing atoms, then results no attachment
    /// shows and frames nothing refers to.
    fn collect_garbage(&mut self) {
        self.attachments
            .retain(|a| a.atoms.iter().any(Option::is_some));
        let attachments = &self.attachments;
        self.results
            .retain(|r| attachments.iter().any(|a| belongs(r, a)));
        let kept: HashSet<FrameHash> = self
            .attachments
            .iter()
            .map(|a| a.frame)
            .chain(self.results.iter().flat_map(|r| referenced(r)))
            .collect();
        self.frames.retain(|f| kept.contains(&f.hash()));
    }

    /// Take in a pasted or inserted drawing's data, its atom IDs already
    /// remapped; its attachments get new IDs.
    pub(crate) fn extend(&mut self, other: Self) {
        for frame in other.frames {
            self.insert_frame(frame);
        }
        self.results.extend(other.results);
        for attachment in other.attachments {
            self.link(attachment.frame, &attachment.atoms, &attachment.baseline);
        }
    }

    fn insert_frame(&mut self, frame: Arc<Frame>) -> FrameHash {
        let hash = frame.hash();
        if self.frame(&hash).is_none() {
            self.frames.push(frame);
        }
        hash
    }

    /// The attachment linking `frame` to these atoms with this baseline,
    /// added when there is none.
    fn link(&mut self, frame: FrameHash, atoms: &[Option<u64>], baseline: &str) -> u64 {
        if let Some(found) = self
            .attachments
            .iter()
            .find(|a| a.frame == frame && a.atoms == atoms && a.baseline == baseline)
        {
            return found.id;
        }
        let id = self
            .attachments
            .iter()
            .map(|a| a.id)
            .max()
            .map_or(1, |id| id.saturating_add(1));
        self.attachments.push(Attachment {
            id,
            frame,
            atoms: atoms.to_vec(),
            baseline: baseline.into(),
        });
        id
    }
}

/// Results are shown through the attachment of their own or their input frame.
fn belongs(result: &CalcResult, attachment: &Attachment) -> bool {
    result.frame == attachment.frame || result.input_frame == Some(attachment.frame)
}

/// Every frame a result refers to, which must stay stored while it exists.
fn referenced(result: &CalcResult) -> impl Iterator<Item = FrameHash> + '_ {
    let gfnff = match &result.settings {
        Settings::Xtb(settings) => settings.gfnff.as_ref().map(|g| g.reference),
        Settings::Imported { .. } => None,
    };
    [Some(result.frame), result.input_frame, gfnff]
        .into_iter()
        .flatten()
        .chain(
            result
                .endpoints
                .iter()
                .flat_map(|e| [e.reactant, e.product]),
        )
}

/// Files, drafts and engine round trips save each result in its saved form.
impl Serialize for Science {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            frames: &'a [Arc<Frame>],
            results: Vec<Cow<'a, CalcResult>>,
            attachments: &'a [Attachment],
        }
        Wire {
            frames: &self.frames,
            results: self.saved_forms(),
            attachments: &self.attachments,
        }
        .serialize(serializer)
    }
}

/// Every route that reads a document (files, recovery, the clipboard)
/// validates its calculation data here, once.
impl<'de> Deserialize<'de> for Science {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(default)]
            frames: Vec<Arc<Frame>>,
            #[serde(default)]
            results: Vec<Arc<CalcResult>>,
            #[serde(default)]
            attachments: Vec<Attachment>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let science = Self {
            frames: wire.frames,
            results: wire.results,
            attachments: wire.attachments,
        };
        science.validate().map_err(serde::de::Error::custom)?;
        Ok(science)
    }
}

/// After every edit and after reading: deleted drawing atoms leave their
/// attachments in the same edit, so a reused atom ID never links again;
/// data no attachment needs any more goes with them.
pub fn prune(doc: &mut Document) {
    if doc.science.is_empty() {
        return;
    }
    let present: HashSet<u64> = doc.atoms.iter().map(|a| a.id).collect();
    let mut science = std::mem::take(&mut doc.science);
    for attachment in &mut science.attachments {
        for atom in &mut attachment.atoms {
            if atom.is_some_and(|id| !present.contains(&id)) {
                *atom = None;
            }
        }
    }
    science.collect_garbage();
    doc.science = science;
}

/// The drawing atoms in frame order, and whether they are still exactly
/// one whole molecule with the chemistry of `baseline`.
fn freshness(doc: &Document, atoms: &[Option<u64>], baseline: &str) -> Freshness {
    let ids: Vec<u64> = atoms.iter().flatten().copied().collect();
    let whole = !ids.is_empty() && ids.iter().all(|id| doc.atom(*id).is_some()) && {
        let set: HashSet<u64> = ids.iter().copied().collect();
        let mut component = HashSet::new();
        for id in &ids {
            if !component.contains(id) {
                component.extend(crate::molecule_names::component(doc, *id));
            }
        }
        component == set
    };
    if whole && crate::models3d::fingerprint(doc, &ids) == baseline {
        Freshness::Current
    } else {
        Freshness::Stale
    }
}

impl Attachment {
    /// Current while the molecule has exactly these atoms and the chemistry
    /// it had when the frame was made.
    pub fn freshness(&self, doc: &Document) -> Freshness {
        freshness(doc, &self.atoms, &self.baseline)
    }
}

/// The 3D model showing the molecule of `atom`.
fn model_of(doc: &Document, atom: u64) -> Option<usize> {
    doc.models
        .iter()
        .position(|m| m.linked_atoms.contains(&atom))
}

/// Element symbol of a model's geometry atom.
fn model_element<'a>(
    doc: &'a Document,
    model: &'a crate::models3d::Model3d,
    index: usize,
) -> Option<&'a str> {
    match &model.generated {
        Some(generated) => generated.elements.get(index).map(String::as_str),
        None => doc
            .atom(model.atoms.get(index)?.id)
            .map(|a| a.element.as_str()),
    }
}

fn atomic_number(symbol: &str) -> Option<u8> {
    crate::editing::ELEMENTS
        .iter()
        .position(|e| *e == symbol)
        .and_then(|i| u8::try_from(i + 1).ok())
}

/// The total charge and multiplicity the drawing gives the molecule of
/// `atom`: its formal charges, and one unpaired electron per radical
/// electron except on singlet atoms.
pub fn drawn_state(doc: &Document, atom: u64) -> (i32, u32) {
    let ids: HashSet<u64> = crate::molecule_names::component(doc, atom)
        .into_iter()
        .collect();
    let atoms = doc.atoms.iter().filter(|a| ids.contains(&a.id));
    atoms.fold((0, 1), |(charge, multiplicity), a| {
        let unpaired = if a.singlet {
            0
        } else {
            u32::from(a.radical_electrons)
        };
        (charge + a.charge, multiplicity + unpaired)
    })
}

/// The calculation input for the molecule of `atom`: its 3D model's
/// geometry in model order, then the model's added hydrogens. A model that
/// still shows an attached frame exactly (an applied optimization) starts
/// from that frame, so chained calculations keep full precision.
pub fn launch(
    doc: &Document,
    atom: u64,
    charge: i32,
    multiplicity: u32,
) -> Result<Launch, ScienceError> {
    let model = &doc.models[model_of(doc, atom).ok_or(ScienceError::NoModel)?];
    if model.cell.is_some() {
        return Err(ScienceError::Periodic);
    }
    if model.is_stale(doc) {
        return Err(ScienceError::StaleModel);
    }
    let mut atoms = model.geometry_sources();
    atoms.resize(model.atoms.len() + model.hydrogens.len(), None);
    let mut numbers = Vec::with_capacity(atoms.len());
    let mut isotopes = Vec::with_capacity(atoms.len());
    for (index, source) in atoms.iter().enumerate() {
        let element = if index < model.atoms.len() {
            model_element(doc, model, index).ok_or(ScienceError::StaleModel)?
        } else {
            "H"
        };
        numbers.push(atomic_number(element).ok_or(ScienceError::StaleModel)?);
        let isotope = source
            .and_then(|id| doc.atom(id))
            .map_or(0, |a| u16::try_from(a.isotope).unwrap_or(0));
        isotopes.push(isotope);
    }
    if isotopes.iter().all(|i| *i == 0) {
        isotopes.clear();
    }
    let positions = model
        .atoms
        .iter()
        .map(|a| a.xyz)
        .chain(model.hydrogens.iter().map(|h| h.xyz))
        .map(|xyz| xyz.map(f64::from))
        .collect();
    let frame = Frame {
        numbers,
        positions_angstrom: positions,
        isotopes,
        masses_amu: vec![],
        charge,
        multiplicity,
        drawing_atoms: vec![],
    };
    frame.validate()?;
    let ids: Vec<u64> = atoms.iter().flatten().copied().collect();
    let baseline = crate::models3d::fingerprint(doc, &ids);
    if freshness(doc, &atoms, &baseline) == Freshness::Stale {
        return Err(ScienceError::StaleModel);
    }
    let shown = |stored: &Frame| {
        stored.numbers == frame.numbers
            && stored.isotopes == frame.isotopes
            && stored.charge == frame.charge
            && stored.multiplicity == frame.multiplicity
            && stored
                .positions_angstrom
                .iter()
                .zip(&frame.positions_angstrom)
                .all(|(a, b)| a.map(|x| x as f32) == b.map(|x| x as f32))
    };
    let science = &doc.science;
    let frame = science
        .attachments
        .iter()
        .filter(|a| a.atoms == atoms && a.baseline == baseline)
        .filter_map(|a| science.frame(&a.frame))
        .find(|stored| stored.masses_amu.is_empty() && shown(stored))
        .cloned()
        .unwrap_or_else(|| Arc::new(frame));
    Ok(Launch {
        frame,
        atoms,
        baseline,
    })
}

/// Store a finished calculation with the molecule it was launched for, as
/// one edit. `frames` are the new geometries the result refers to (an
/// optimization's final frame). A molecule edited since launch gets the
/// result as [`Freshness::Stale`]; Undo back to the launch state makes it
/// current. Nothing changes on error.
pub fn attach(
    doc: &mut Document,
    launch: &Launch,
    frames: Vec<Frame>,
    result: CalcResult,
) -> Result<Attached, ScienceError> {
    if !launch.atoms.iter().flatten().any(|id| doc.atom(*id).is_some()) {
        return Err(ScienceError::Gone);
    }
    let mut science = doc.science.clone();
    let input = science.insert_frame(launch.frame.clone());
    for frame in frames {
        frame.validate()?;
        science.insert_frame(Arc::new(frame));
    }
    let frame = science
        .frame(&result.frame)
        .ok_or(ScienceError::Unknown)?
        .clone();
    result.validate(&frame)?;
    if referenced(&result).any(|hash| science.frame(&hash).is_none()) {
        return Err(ScienceError::Unknown);
    }
    let input_attachment = science.link(input, &launch.atoms, &launch.baseline);
    let attachment = if result.frame == input {
        input_attachment
    } else if frame.numbers == launch.frame.numbers {
        science.link(result.frame, &launch.atoms, &launch.baseline)
    } else {
        input_attachment
    };
    science.results.push(Arc::new(result));
    // Deleted atoms leave the new links as they would on any edit.
    let present: HashSet<u64> = doc.atoms.iter().map(|a| a.id).collect();
    for link in &mut science.attachments {
        for atom in &mut link.atoms {
            if atom.is_some_and(|id| !present.contains(&id)) {
                *atom = None;
            }
        }
    }
    if science.heap_bytes() > MAX_SCIENCE_BYTES {
        return Err(ScienceError::TooLarge);
    }
    doc.science = science;
    Ok(Attached {
        freshness: freshness(doc, &launch.atoms, &launch.baseline),
        attachment,
    })
}

/// Every result shown for the molecule of `atom`, oldest first.
pub fn results_for(doc: &Document, atom: u64) -> Vec<ResultView<'_>> {
    let science = &doc.science;
    let mut views = Vec::new();
    for result in &science.results {
        let Some(attachment) = science
            .attachments
            .iter()
            .find(|a| a.atoms.contains(&Some(atom)) && belongs(result, a))
        else {
            continue;
        };
        let Some(frame) = science.frame(&result.frame) else {
            continue;
        };
        views.push(ResultView {
            result,
            frame,
            attachment,
            freshness: attachment.freshness(doc),
        });
    }
    views
}

/// Move the molecule's 3D model to the attached frame ("Apply xTB
/// geometry", one undo step for the caller). The 2D drawing does not move.
pub fn apply_geometry(doc: &mut Document, attachment: u64) -> Result<(), ScienceError> {
    let link = doc
        .science
        .attachment(attachment)
        .ok_or(ScienceError::Unknown)?;
    if link.freshness(doc) == Freshness::Stale {
        return Err(ScienceError::Stale);
    }
    let frame = doc
        .science
        .frame(&link.frame)
        .ok_or(ScienceError::Unknown)?
        .clone();
    let first = link.atoms.iter().flatten().next().copied();
    let index = first
        .and_then(|atom| model_of(doc, atom))
        .ok_or(ScienceError::NoModel)?;
    let model = &doc.models[index];
    let heavy = model.atoms.len();
    let matches = model.cell.is_none()
        && heavy + model.hydrogens.len() == frame.numbers.len()
        && model
            .geometry_sources()
            .iter()
            .chain(std::iter::repeat(&None))
            .zip(&link.atoms)
            .all(|(model, link)| model == link)
        && frame.numbers.iter().enumerate().all(|(i, z)| {
            let element = if i < heavy {
                model_element(doc, model, i)
            } else {
                Some("H")
            };
            element.and_then(atomic_number) == Some(*z)
        });
    if !matches {
        return Err(ScienceError::ModelMismatch);
    }
    let model = &mut doc.models[index];
    let mut positions = frame
        .positions_angstrom
        .iter()
        .map(|p| p.map(|x| x as f32));
    for (atom, xyz) in model.atoms.iter_mut().zip(positions.by_ref()) {
        atom.xyz = xyz;
    }
    for (hydrogen, xyz) in model.hydrogens.iter_mut().zip(positions) {
        hydrogen.xyz = xyz;
    }
    Ok(())
}

/// Set "Keep full results in this file" on one result.
pub fn set_keep_full(doc: &mut Document, result: &Arc<CalcResult>, keep: bool) -> bool {
    let Some(slot) = doc
        .science
        .results
        .iter_mut()
        .find(|r| Arc::ptr_eq(r, result))
    else {
        return false;
    };
    if slot.keep_full == keep {
        return false;
    }
    let mut changed = (**slot).clone();
    changed.keep_full = keep;
    *slot = Arc::new(changed);
    true
}

/// "Make lighter": results the engine can recompute stop keeping their full
/// data in the file and in this document. Imported results, which cannot be
/// recomputed, are left alone. Returns how many results changed.
pub fn make_lighter(doc: &mut Document) -> usize {
    let mut changed = 0;
    for slot in &mut doc.science.results {
        if slot.keep_full && matches!(slot.settings, Settings::Xtb(_)) {
            let mut light = slot.light(&[]);
            light.keep_full = false;
            *slot = Arc::new(light);
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests;
