//! Scientific frames and calculation results kept with molecules.
//!
//! The drawing never changes for a calculation: frames (exact `f64`
//! geometries) and results live in [`Science`] beside it, immutable and shared
//! by `Arc` between the live document and undo history. An [`Attachment`]
//! links one frame's atoms to drawing atoms and records the molecule's
//! chemistry when the frame was made; every result belongs to one attachment.
//! Any chemical edit makes the attachment's results stale and Undo makes them
//! current again. The design is `docs/plans/xtb-data.md`, section Documents.
use std::{
    borrow::Cow,
    cell::Cell,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use reshiki_qcdata::{
    BasisSet, Beta, CalcResult, Frame, FrameHash, MAX_IMPORTED_HEAVY_BYTES, Settings, Spin,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::document::Document;

/// Heap bytes of frames and results one document may hold, each stored
/// result counted, as a file stores them.
pub const MAX_SCIENCE_BYTES: usize = 256 << 20;
/// Frames, attachments and results one document may hold.
pub const MAX_SCIENCE_RECORDS: usize = 10_000;
/// Saved results above this size make Save offer "Make lighter".
pub const LIGHTER_BYTES: usize = 5_000_000;

/// Frames, results and their links to the drawing. Empty in drawings
/// without calculations, and then absent from the file.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Science {
    /// Every stored geometry, at most once per hash.
    pub frames: Vec<Arc<Frame>>,
    /// Links from frames to drawing atoms.
    pub attachments: Vec<Attachment>,
    /// Results in the order they arrived.
    pub results: Vec<Stored>,
}

/// One frame linked to one molecule of the drawing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    /// Unique among the document's attachments.
    pub id: u64,
    /// The linked frame.
    pub frame: FrameHash,
    /// The drawing atom of each frame atom; `None` for atoms the drawing
    /// does not show (implicit hydrogens) and for deleted atoms.
    pub atoms: Vec<Option<u64>>,
    /// Chemistry of the linked atoms when the frame was made.
    pub baseline: String,
}

/// A result and the attachment it was calculated for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "StoredWire")]
pub struct Stored {
    pub attachment: u64,
    /// "Keep full results in this file". Kept here, not in the shared
    /// result, so toggling it never copies the arrays.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_full: bool,
    /// Always has `keep_full` false; the flag above decides.
    pub result: Arc<CalcResult>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredWire {
    attachment: u64,
    #[serde(default)]
    keep_full: bool,
    result: CalcResult,
}

impl From<StoredWire> for Stored {
    fn from(wire: StoredWire) -> Self {
        let mut result = wire.result;
        let keep_full = wire.keep_full || result.keep_full;
        result.keep_full = false;
        Self {
            attachment: wire.attachment,
            keep_full,
            result: Arc::new(result),
        }
    }
}

impl PartialEq for Stored {
    fn eq(&self, other: &Self) -> bool {
        self.attachment == other.attachment
            && self.keep_full == other.keep_full
            && (Arc::ptr_eq(&self.result, &other.result) || self.result == other.result)
    }
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
    #[error("The 3D model holds more than this molecule")]
    Fragments,
    #[error("The molecule was deleted")]
    Gone,
    #[error("The molecule changed after this calculation")]
    Stale,
    #[error("The 3D model no longer matches this calculation")]
    ModelMismatch,
    #[error("Stereo check: {0}")]
    Chemistry(String),
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
    /// The molecule's chemistry at launch: the revision a late result is
    /// checked against.
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
    /// The attachment the result belongs to.
    pub attachment: u64,
}

/// A result as the results palette shows it.
#[derive(Debug, Clone)]
pub struct ResultView<'a> {
    /// Index into [`Science::results`].
    pub index: usize,
    pub stored: &'a Stored,
    /// The geometry the result's values describe.
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

/// Tetrahedral stereochemistry the drawing leaves unspecified.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TetrahedralStereo {
    /// Potential centres in drawing atom order, including explicit unknowns.
    pub undefined: Vec<UndefinedCentre>,
    /// At least one potential centre has a defined drawing configuration.
    pub has_defined: bool,
}

/// One potential centre with no defined drawing configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndefinedCentre {
    pub atom: u64,
    /// Full CIP descriptor inferred from the supplied frame, when resolved.
    pub cip: Option<String>,
    /// The frame's tetrahedral winding (1 or 2), relative to the prepared
    /// drawing's bond order. Comparable across frames of the same drawing,
    /// even when CIP cannot resolve a descriptor. Absent without a frame or
    /// when the geometry does not define a tetrahedron.
    pub winding: Option<u8>,
}

impl PartialEq for Science {
    /// Snapshots share their payloads, so pointer equality decides almost
    /// every comparison without reading the arrays.
    fn eq(&self, other: &Self) -> bool {
        self.frames.len() == other.frames.len()
            && self
                .frames
                .iter()
                .zip(&other.frames)
                .all(|(a, b)| Arc::ptr_eq(a, b) || a == b)
            && self.attachments == other.attachments
            && self.results == other.results
    }
}

thread_local! {
    /// Bytes and records still admitted while one `Science` is decoded.
    static ROOM: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

/// Heap bytes a decoded record takes, for the admission budget.
trait Weight {
    fn weight(&self) -> usize;
}
impl Weight for Arc<Frame> {
    fn weight(&self) -> usize {
        size_of::<Frame>() + self.heap_bytes()
    }
}
impl Weight for Attachment {
    fn weight(&self) -> usize {
        size_of::<Self>() + size_of_val(self.atoms.as_slice()) + self.baseline.len()
    }
}
impl Weight for Stored {
    fn weight(&self) -> usize {
        size_of::<CalcResult>() + self.result.heap_bytes()
    }
}

/// A list decoded one record at a time, refused as soon as the document's
/// records together pass [`MAX_SCIENCE_BYTES`] or [`MAX_SCIENCE_RECORDS`].
fn admitted<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Weight,
{
    struct Records<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de> + Weight> serde::de::Visitor<'de> for Records<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a list of calculation records")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut records = Vec::new();
            while let Some(record) = seq.next_element::<T>()? {
                let (bytes, count) = ROOM.get();
                let weight = record.weight();
                if weight > bytes || count == 0 {
                    return Err(serde::de::Error::custom(ScienceError::TooLarge));
                }
                ROOM.set((bytes - weight, count - 1));
                records.push(record);
            }
            Ok(records)
        }
    }
    deserializer.deserialize_seq(Records(std::marker::PhantomData))
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
        let results = self.results.iter().map(|s| {
            (
                Arc::as_ptr(&s.result) as *const () as usize,
                s.result.heap_bytes(),
            )
        });
        let mut seen = HashSet::new();
        frames
            .chain(results)
            .filter(move |(address, _)| seen.insert(*address))
    }

    /// Bytes a snapshot owns itself: the lists and the attachments.
    pub(crate) fn container_bytes(&self) -> usize {
        size_of_val(self.frames.as_slice())
            + size_of_val(self.results.as_slice())
            + self.attachments.iter().map(Weight::weight).sum::<usize>()
            + size_of::<Self>()
    }

    /// Heap bytes as a file stores them: every stored result counted, even
    /// when two share one payload in memory.
    pub fn heap_bytes(&self) -> usize {
        self.frames.iter().map(Weight::weight).sum::<usize>()
            + self.attachments.iter().map(Weight::weight).sum::<usize>()
            + self.results.iter().map(Weight::weight).sum::<usize>()
    }

    /// Refuse a document whose calculation data a file could not hold.
    pub(crate) fn check_budget(&self) -> Result<(), String> {
        let records = self.frames.len() + self.attachments.len() + self.results.len();
        if self.heap_bytes() > MAX_SCIENCE_BYTES || records > MAX_SCIENCE_RECORDS {
            return Err(ScienceError::TooLarge.to_string());
        }
        Ok(())
    }

    /// What a file stores: each result in its saved form. Imported results
    /// that cannot be recomputed keep their heavy arrays while their
    /// molecule's imported data stays within [`MAX_IMPORTED_HEAVY_BYTES`],
    /// in arrival order.
    pub fn saved(&self) -> Self {
        self.saved_with_pins(&HashMap::new())
    }

    /// The saved form, retaining the orbital rows needed by placed figures.
    /// Pins are grouped by [`result_key`].
    pub fn saved_with_pins(&self, pins: &HashMap<String, Vec<(Spin, u32)>>) -> Self {
        let mut budgets = HashMap::<Vec<u64>, usize>::new();
        let results = self
            .results
            .iter()
            .map(|stored| {
                let budget = budgets
                    .entry(self.molecule_key(stored.attachment))
                    .or_insert(MAX_IMPORTED_HEAVY_BYTES);
                if stored.keep_full {
                    if matches!(stored.result.settings, Settings::Imported { .. }) {
                        *budget = budget.saturating_sub(stored.result.heavy_bytes());
                    }
                    return stored.clone();
                }
                let pinned = pins
                    .get(&result_key(&stored.result))
                    .map_or(&[][..], Vec::as_slice);
                let result = match stored.result.for_file(*budget, pinned) {
                    Cow::Borrowed(_) => {
                        if matches!(stored.result.settings, Settings::Imported { .. }) {
                            *budget = budget.saturating_sub(stored.result.heavy_bytes());
                        }
                        stored.result.clone()
                    }
                    Cow::Owned(light) => Arc::new(light),
                };
                Stored {
                    result,
                    ..stored.clone()
                }
            })
            .collect();
        Self {
            frames: self.frames.clone(),
            attachments: self.attachments.clone(),
            results,
        }
    }

    /// The sorted drawing atoms of an attachment's molecule.
    fn molecule_key(&self, attachment: u64) -> Vec<u64> {
        let mut key: Vec<u64> = self
            .attachment(attachment)
            .map(|a| a.atoms.iter().flatten().copied().collect())
            .unwrap_or_default();
        key.sort_unstable();
        key
    }

    /// Pretty-printed JSON bytes the file spends on each result, in
    /// [`Science::results`] order ("Saved in file: 84 KB"). One pass for the
    /// whole palette; slow for large imported results, so callers cache it
    /// per document revision off the UI thread.
    pub fn saved_sizes(&self) -> Vec<usize> {
        self.saved_sizes_with_pins(&HashMap::new())
    }

    /// Saved result sizes including the orbital rows needed by placed figures.
    pub fn saved_sizes_with_pins(&self, pins: &HashMap<String, Vec<(Spin, u32)>>) -> Vec<usize> {
        self.saved_with_pins(pins)
            .results
            .iter()
            .map(|stored| {
                serde_json::to_vec_pretty(stored)
                    .map(|json| json.len())
                    .unwrap_or(0)
            })
            .collect()
    }

    /// Pretty-printed JSON bytes the file spends on calculation data; Save
    /// offers [`make_lighter`] above [`LIGHTER_BYTES`].
    pub fn file_bytes(&self) -> usize {
        self.file_bytes_with_pins(&HashMap::new())
    }

    /// Saved calculation bytes including orbital rows needed by placed figures.
    pub fn file_bytes_with_pins(&self, pins: &HashMap<String, Vec<(Spin, u32)>>) -> usize {
        if self.is_empty() {
            return 0;
        }
        serde_json::to_vec_pretty(&self.saved_with_pins(pins))
            .map(|json| json.len())
            .unwrap_or(0)
    }

    /// Check what a file, a draft or an inserted drawing supplied: every
    /// frame and result valid, every reference resolved.
    fn validate(&self) -> Result<(), String> {
        let mut frames = HashMap::new();
        for frame in &self.frames {
            frame.validate().map_err(|e| e.to_string())?;
            if frames.insert(frame.hash(), frame).is_some() {
                return Err("Calculation data repeats a frame".into());
            }
        }
        let missing = || "Calculation data refers to a missing frame".to_string();
        let mut attachments = HashMap::new();
        for attachment in &self.attachments {
            let frame = frames.get(&attachment.frame).ok_or_else(missing)?;
            let mut atoms = HashSet::new();
            if attachments.insert(attachment.id, attachment).is_some()
                || attachment.atoms.len() != frame.numbers.len()
                || !attachment
                    .atoms
                    .iter()
                    .flatten()
                    .all(|id| atoms.insert(*id))
                || attachment.baseline.is_empty()
                || attachment.baseline.len() > MAX_BASELINE_PER_ATOM * (attachment.atoms.len() + 1)
            {
                return Err("Invalid calculation attachment".into());
            }
        }
        for stored in &self.results {
            let result = &stored.result;
            let attachment = attachments
                .get(&stored.attachment)
                .ok_or("A calculation result has no matching attachment")?;
            if referenced(result).any(|hash| !frames.contains_key(&hash)) {
                return Err(missing());
            }
            let (Some(input), Some(frame)) =
                (frames.get(&attachment.frame), frames.get(&result.frame))
            else {
                return Err(missing());
            };
            check_result(attachment, input, result, frame).map_err(|e| e.to_string())?;
        }
        self.check_budget()
    }

    /// Drop attachments without drawing atoms, then their results and the
    /// frames nothing refers to.
    fn collect_garbage(&mut self) {
        self.attachments
            .retain(|a| a.atoms.iter().any(Option::is_some));
        let ids: HashSet<u64> = self.attachments.iter().map(|a| a.id).collect();
        self.results.retain(|s| ids.contains(&s.attachment));
        let kept: HashSet<FrameHash> = self
            .attachments
            .iter()
            .map(|a| a.frame)
            .chain(self.results.iter().flat_map(|s| referenced(&s.result)))
            .collect();
        self.frames.retain(|f| kept.contains(&f.hash()));
    }

    fn insert_frame(&mut self, frame: Arc<Frame>) -> FrameHash {
        let hash = frame.hash();
        if self.frame(&hash).is_none() {
            self.frames.push(frame);
        }
        hash
    }

    /// The attachment linking `frame` to these atoms with this baseline,
    /// added under the smallest free ID when there is none.
    fn link(&mut self, frame: FrameHash, atoms: &[Option<u64>], baseline: &str) -> u64 {
        if let Some(found) = self
            .attachments
            .iter()
            .find(|a| a.frame == frame && a.atoms == atoms && a.baseline == baseline)
        {
            return found.id;
        }
        let used: HashSet<u64> = self.attachments.iter().map(|a| a.id).collect();
        let id = (1..).find(|id| !used.contains(id)).unwrap_or_default();
        self.attachments.push(Attachment {
            id,
            frame,
            atoms: atoms.to_vec(),
            baseline: baseline.into(),
        });
        id
    }

    /// Take in an inserted drawing's data, its atom IDs already remapped;
    /// its attachments get new IDs.
    pub(crate) fn extend(&mut self, other: Self) {
        let mut hashes: HashSet<FrameHash> = self.frames.iter().map(|f| f.hash()).collect();
        for frame in other.frames {
            if hashes.insert(frame.hash()) {
                self.frames.push(frame);
            }
        }
        let mut ids = HashMap::new();
        for attachment in &other.attachments {
            let id = self.link(attachment.frame, &attachment.atoms, &attachment.baseline);
            ids.insert(attachment.id, id);
        }
        for stored in other.results {
            if let Some(&attachment) = ids.get(&stored.attachment) {
                self.results.push(Stored {
                    attachment,
                    ..stored
                });
            }
        }
    }
}

/// Longest baseline per linked atom: the digest, a singlet flag and an
/// enhanced-stereo group for each atom.
const MAX_BASELINE_PER_ATOM: usize = 64;

/// A result belongs to the attachment of its input frame, or of its own
/// frame when it has none, and describes the same atoms in the same order
/// (a transition state's frame is its own reaction's).
fn check_result(
    attachment: &Attachment,
    input: &Frame,
    result: &CalcResult,
    frame: &Frame,
) -> Result<(), ScienceError> {
    let isotopes = |f: &Frame| -> Vec<u16> {
        if f.isotopes.is_empty() {
            vec![0; f.numbers.len()]
        } else {
            f.isotopes.clone()
        }
    };
    let same_atoms = result.task == reshiki_qcdata::Task::TransitionState
        || input.numbers == frame.numbers
            && isotopes(input) == isotopes(frame)
            && input.masses_amu == frame.masses_amu
            && input.charge == frame.charge
            && input.multiplicity == frame.multiplicity;
    if result.input_frame.unwrap_or(result.frame) != attachment.frame
        || attachment.atoms.len() != input.numbers.len()
        || !same_atoms
    {
        return Err(ScienceError::Unknown);
    }
    result.validate(frame)?;
    Ok(())
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

/// Every route that reads a document (files, recovery, inserted drawings)
/// validates its calculation data here, once, and never decodes more than
/// the document budget.
impl<'de> Deserialize<'de> for Science {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(default, deserialize_with = "admitted")]
            frames: Vec<Arc<Frame>>,
            #[serde(default, deserialize_with = "admitted")]
            attachments: Vec<Attachment>,
            #[serde(default, deserialize_with = "admitted")]
            results: Vec<Stored>,
        }
        ROOM.set((MAX_SCIENCE_BYTES, MAX_SCIENCE_RECORDS));
        let wire = Wire::deserialize(deserializer)?;
        let science = Self {
            frames: wire.frames,
            attachments: wire.attachments,
            results: wire.results,
        };
        science.validate().map_err(serde::de::Error::custom)?;
        Ok(science)
    }
}

/// After every edit and after reading: deleted drawing atoms leave their
/// attachments and deleted TS arrows lose their links in the same edit, so
/// reused IDs never link again; data no attachment needs goes with them.
pub fn prune(doc: &mut Document) {
    if doc.science.is_empty() {
        return;
    }
    let present: HashSet<u64> = doc.atoms.iter().map(|a| a.id).collect();
    let arrows: HashSet<u64> = doc.arrows.iter().map(|arrow| arrow.id).collect();
    let mut science = std::mem::take(&mut doc.science);
    for attachment in &mut science.attachments {
        for atom in &mut attachment.atoms {
            if atom.is_some_and(|id| !present.contains(&id)) {
                *atom = None;
            }
        }
    }
    for stored in &mut science.results {
        let missing = stored
            .result
            .run
            .as_ref()
            .and_then(|run| run.transition_state.as_ref())
            .and_then(|search| search.arrow)
            .is_some_and(|arrow| !arrows.contains(&arrow));
        if missing
            && let Some(search) = Arc::make_mut(&mut stored.result)
                .run
                .as_mut()
                .and_then(|run| run.transition_state.as_mut())
        {
            search.arrow = None;
        }
    }
    science.collect_garbage();
    doc.science = science;
}

/// The chemistry digest 3D models use, plus each linked atom's singlet
/// choice, which sets the spin a calculation describes, and its enhanced
/// stereo group, which the digest leaves out.
pub fn baseline(doc: &Document, ids: &[u64]) -> String {
    let mut text = crate::models3d::fingerprint(doc, ids);
    text.push('/');
    let mut groups = String::new();
    for atom in ids.iter().filter_map(|id| doc.atom(*id)) {
        text.push(if atom.singlet { '1' } else { '0' });
        if let Some(group) = &atom.stereo_group {
            groups.push_str(&serde_json::to_string(group).unwrap_or_default());
        }
        groups.push(',');
    }
    if groups.chars().any(|c| c != ',') {
        text.push('/');
        text.push_str(&groups);
    }
    text
}

/// Whether the drawing atoms of `atoms` are still exactly one whole molecule
/// with the chemistry of `expected`.
fn freshness(doc: &Document, atoms: &[Option<u64>], expected: &str) -> Freshness {
    let ids: Vec<u64> = atoms.iter().flatten().copied().collect();
    if !ids.is_empty() && closed(doc, &ids) && baseline(doc, &ids) == expected {
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

/// Whether `ids` are whole molecules: every atom bonded to one of them is
/// one of them. A calculation is for one molecule, a file's result can be
/// for a complex of several.
fn closed(doc: &Document, ids: &[u64]) -> bool {
    let set: HashSet<u64> = ids.iter().copied().collect();
    ids.iter().all(|&id| {
        doc.atom(id).is_some()
            && crate::molecule_names::component(doc, id)
                .into_iter()
                .all(|atom| set.contains(&atom))
    })
}

fn model_baseline(
    doc: &Document,
    atom: u64,
    atoms: &[Option<u64>],
) -> Result<String, ScienceError> {
    let ids: Vec<u64> = atoms.iter().flatten().copied().collect();
    let molecule: HashSet<u64> = crate::molecule_names::component(doc, atom)
        .into_iter()
        .collect();
    if ids.iter().copied().collect::<HashSet<_>>() != molecule {
        return Err(if ids.iter().all(|id| doc.atom(*id).is_some()) {
            ScienceError::Fragments
        } else {
            ScienceError::StaleModel
        });
    }
    Ok(baseline(doc, &ids))
}

/// The total charge and multiplicity the drawing gives the molecule of
/// `atom`: its formal charges, and one unpaired electron per radical
/// electron except on singlet atoms.
pub fn drawn_state(doc: &Document, atom: u64) -> (i32, u32) {
    let ids: HashSet<u64> = crate::molecule_names::component(doc, atom)
        .into_iter()
        .collect();
    let atoms = doc.atoms.iter().filter(|a| ids.contains(&a.id));
    atoms.fold((0i32, 1u32), |(charge, multiplicity), a| {
        let unpaired = if a.singlet {
            0
        } else {
            u32::from(a.radical_electrons)
        };
        (
            charge.saturating_add(a.charge),
            multiplicity.saturating_add(unpaired),
        )
    })
}

/// Masses (u) for a frame with isotope labels: labelled atoms take their
/// isotope's mass, the others the standard atomic weight; `None` without labels.
pub fn isotope_masses(frame: &Frame) -> Option<Vec<f64>> {
    use crate::chemistry::{ELEMENTS, ISOTOPES};
    if frame.isotopes.iter().all(|&isotope| isotope == 0) {
        return None;
    }
    Some(
        frame
            .numbers
            .iter()
            .enumerate()
            .map(
                |(index, &z)| match frame.isotopes.get(index).copied().unwrap_or(0) {
                    0 => ELEMENTS
                        .get(usize::from(z))
                        .map_or(0., |element| element.average_mass_amu()),
                    isotope => ISOTOPES
                        .binary_search_by_key(&(z, isotope), |&(n, i, _)| (n, i))
                        .ok()
                        .and_then(|found| ISOTOPES.get(found))
                        .map_or(f64::from(isotope), |&(_, _, mass)| mass),
                },
            )
            .collect(),
    )
}

/// For each frame atom the 3D model added (a hydrogen the drawing does not
/// show), the drawing atom it is bonded to.
pub fn hydrogen_parents(doc: &Document, launch: &Launch) -> Option<Vec<Option<u64>>> {
    let first = launch.atoms.iter().flatten().next()?;
    let model = doc.models.iter().find(|m| m.linked_atoms.contains(first))?;
    let sources = model.geometry_sources();
    let heavy = model.atoms.len();
    let mut parents = vec![None; heavy];
    for hydrogen in &model.hydrogens {
        // Generated models index their parents; imported ones use drawing IDs.
        let parent = if model.generated.is_some() {
            *sources.get(usize::try_from(hydrogen.parent).ok()?)?
        } else {
            Some(hydrogen.parent)
        };
        parents.push(Some(parent?));
    }
    (parents.len() == launch.atoms.len()).then_some(parents)
}

/// Drawing bonds in frame order, with each model hydrogen joined to its nearest linked atom.
pub fn frame_bonds(
    doc: &Document,
    atoms: &[Option<u64>],
    positions: &[[f64; 3]],
) -> Vec<(usize, usize, u8)> {
    let linked: HashMap<_, _> = atoms
        .iter()
        .enumerate()
        .filter_map(|(index, id)| id.map(|id| (id, index)))
        .collect();
    let mut bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|bond| {
            let (&a, &b) = (linked.get(&bond.a)?, linked.get(&bond.b)?);
            (a != b).then_some((a.min(b), a.max(b), bond.order))
        })
        .collect();

    let located_atoms = atoms.iter().zip(positions).enumerate();
    let linked_positions: Vec<_> = located_atoms
        .clone()
        .filter_map(|(index, (id, position))| id.is_some().then_some((index, position)))
        .collect();
    for (hydrogen, (_, position)) in located_atoms.filter(|(_, (id, _))| id.is_none()) {
        let nearest = linked_positions
            .iter()
            .map(|&(index, candidate)| {
                let squared_distance = position
                    .iter()
                    .zip(candidate)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f64>();
                (index, squared_distance)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((neighbor, _)) = nearest {
            bonds.push((hydrogen.min(neighbor), hydrogen.max(neighbor), 1));
        }
    }

    bonds.sort_by_key(|&(a, b, _)| (a, b));
    bonds.dedup_by_key(|bond| (bond.0, bond.1));
    bonds
}

/// The calculation input for the molecule of `atom`: its 3D model's
/// geometry in model order, then the model's added hydrogens. A model that
/// still shows a frame stored for this molecule exactly (an applied
/// optimization) starts from that frame, so chained calculations keep full
/// precision.
pub fn launch(
    doc: &Document,
    atom: u64,
    charge: i32,
    multiplicity: u32,
) -> Result<Launch, ScienceError> {
    let model = model_of(doc, atom)
        .and_then(|index| doc.models.get(index))
        .ok_or(ScienceError::NoModel)?;
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
    let baseline = model_baseline(doc, atom, &atoms)?;
    let shown = |stored: &Frame| {
        stored.numbers == frame.numbers
            && stored.isotopes == frame.isotopes
            && stored.masses_amu.is_empty()
            && stored.charge == frame.charge
            && stored.multiplicity == frame.multiplicity
            && stored
                .positions_angstrom
                .iter()
                .zip(&frame.positions_angstrom)
                .all(|(a, b)| a.map(|x| x as f32) == b.map(|x| x as f32))
    };
    let science = &doc.science;
    let mine: HashSet<u64> = science
        .attachments
        .iter()
        .filter(|a| a.atoms == atoms && a.baseline == baseline)
        .map(|a| a.id)
        .collect();
    // The newest result's geometry first: an applied optimization.
    let frames: HashMap<FrameHash, &Arc<Frame>> =
        science.frames.iter().map(|f| (f.hash(), f)).collect();
    let frame = science
        .results
        .iter()
        .rev()
        .filter(|s| mine.contains(&s.attachment))
        .map(|s| s.result.frame)
        .chain(
            science
                .attachments
                .iter()
                .filter(|a| mine.contains(&a.id))
                .map(|a| a.frame),
        )
        .filter_map(|hash| frames.get(&hash))
        .find(|stored| shown(stored))
        .map(|stored| Arc::clone(stored))
        .unwrap_or_else(|| Arc::new(frame));
    Ok(Launch {
        frame,
        atoms,
        baseline,
    })
}

/// Store a finished calculation with the molecule it was launched for, as
/// one edit. `frames` are the new geometries the result refers to (an
/// optimization's final frame); the result must start from the launch
/// frame. A molecule edited since launch gets the result as
/// [`Freshness::Stale`]; Undo back to the launch state makes it current.
/// Nothing changes on error.
pub fn attach(
    doc: &mut Document,
    launch: &Launch,
    frames: Vec<Frame>,
    mut result: CalcResult,
) -> Result<Attached, ScienceError> {
    if !launch
        .atoms
        .iter()
        .flatten()
        .any(|id| doc.atom(*id).is_some())
    {
        return Err(ScienceError::Gone);
    }
    let mut science = doc.science.clone();
    let input = science.insert_frame(launch.frame.clone());
    for frame in frames {
        frame.validate()?;
        science.insert_frame(Arc::new(frame));
    }
    if referenced(&result).any(|hash| science.frame(&hash).is_none()) {
        return Err(ScienceError::Unknown);
    }
    let frame = science
        .frame(&result.frame)
        .ok_or(ScienceError::Unknown)?
        .clone();
    let attachment = science.link(input, &launch.atoms, &launch.baseline);
    let link = science
        .attachment(attachment)
        .ok_or(ScienceError::Unknown)?;
    check_result(link, &launch.frame, &result, &frame)?;
    let keep_full = std::mem::take(&mut result.keep_full);
    science.results.push(Stored {
        attachment,
        keep_full,
        result: Arc::new(result),
    });
    // Atoms deleted since launch leave the link, as on any edit.
    let present: HashSet<u64> = doc.atoms.iter().map(|a| a.id).collect();
    for link in &mut science.attachments {
        for atom in &mut link.atoms {
            if atom.is_some_and(|id| !present.contains(&id)) {
                *atom = None;
            }
        }
    }
    science.check_budget().map_err(|_| ScienceError::TooLarge)?;
    doc.science = science;
    Ok(Attached {
        freshness: freshness(doc, &launch.atoms, &launch.baseline),
        attachment,
    })
}

/// Attach an external result whose exact `frame` matches `model` in atom
/// order and within 1e-3 Å. Pass any other referenced geometries in `frames`.
pub fn import(
    doc: &mut Document,
    model: usize,
    frame: Frame,
    mut frames: Vec<Frame>,
    result: CalcResult,
) -> Result<Attached, ScienceError> {
    let model = doc.models.get(model).ok_or(ScienceError::NoModel)?;
    if model.cell.is_some() {
        return Err(ScienceError::Periodic);
    }
    if frame.hash() != result.frame {
        return Err(ScienceError::Unknown);
    }
    let mut atoms = model.geometry_sources();
    atoms.resize(model.atoms.len() + model.hydrogens.len(), None);
    let matches = frame.numbers.len() == atoms.len()
        && frame.numbers.iter().enumerate().all(|(i, &number)| {
            let element = if i < model.atoms.len() {
                model_element(doc, model, i)
            } else {
                Some("H")
            };
            element.and_then(atomic_number) == Some(number)
        })
        && model
            .atoms
            .iter()
            .map(|a| a.xyz)
            .chain(model.hydrogens.iter().map(|h| h.xyz))
            .zip(&frame.positions_angstrom)
            .all(|(shown, exact)| {
                shown
                    .into_iter()
                    .zip(exact)
                    .map(|(a, b)| (f64::from(a) - b).powi(2))
                    .sum::<f64>()
                    <= 1e-6
            });
    if !matches {
        return Err(ScienceError::ModelMismatch);
    }
    let ids: Vec<u64> = atoms.iter().flatten().copied().collect();
    if ids.is_empty() {
        return Err(ScienceError::Gone);
    }
    // The model may hold a complex of several molecules (a dimer, an ion
    // pair): its result belongs to all of them.
    if !closed(doc, &ids) {
        return Err(ScienceError::Fragments);
    }
    let baseline = baseline(doc, &ids);
    let frame = if let Some(input) = result.input_frame.filter(|hash| *hash != result.frame) {
        let input_frame = if let Some(index) = frames.iter().position(|frame| frame.hash() == input)
        {
            let input = frames.swap_remove(index);
            input.validate()?;
            Arc::new(input)
        } else {
            doc.science
                .frame(&input)
                .cloned()
                .ok_or(ScienceError::Unknown)?
        };
        frames.push(frame);
        input_frame
    } else {
        frame.validate()?;
        Arc::new(frame)
    };
    let launch = Launch {
        frame,
        atoms,
        baseline,
    };
    attach(doc, &launch, frames, result)
}

/// Every result stored for the molecule of `atom` (any of its current atoms
/// still linked), oldest first.
pub fn results_for(doc: &Document, atom: u64) -> Vec<ResultView<'_>> {
    let science = &doc.science;
    let molecule: HashSet<u64> = crate::molecule_names::component(doc, atom)
        .into_iter()
        .collect();
    let attachments: HashMap<u64, (&Attachment, Freshness)> = science
        .attachments
        .iter()
        .filter(|a| a.atoms.iter().flatten().any(|id| molecule.contains(id)))
        .map(|a| (a.id, (a, a.freshness(doc))))
        .collect();
    if attachments.is_empty() {
        return vec![];
    }
    let frames: HashMap<FrameHash, &Arc<Frame>> =
        science.frames.iter().map(|f| (f.hash(), f)).collect();
    science
        .results
        .iter()
        .enumerate()
        .filter_map(|(index, stored)| {
            let (attachment, freshness) = *attachments.get(&stored.attachment)?;
            Some(ResultView {
                index,
                stored,
                frame: frames.get(&stored.result.frame)?,
                attachment,
                freshness,
            })
        })
        .collect()
}

/// Stable figure reference: the first 16 lowercase SHA-256 hex digits of
/// the result's frame, input frame, task, settings and provenance JSON tuple.
/// Changing calculated values or pinning orbital rows leaves the key intact.
pub fn result_key(result: &CalcResult) -> String {
    let identity = (
        result.frame,
        result.input_frame,
        result.task,
        &result.settings,
        &result.provenance,
    );
    let bytes = serde_json::to_vec(&identity).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    format!(
        "{:016x}",
        u64::from_be_bytes(digest.first_chunk::<8>().copied().unwrap_or_default())
    )
}

/// The newest result matching the figure on this molecule, and its storage index.
pub fn figure_result<'a>(
    doc: &'a Document,
    model: &crate::models3d::Model3d,
) -> Option<(usize, &'a Stored)> {
    let figure = model.surface.as_ref()?;
    let atom = model.linked_atoms.first()?;
    let mut matches = doc
        .science
        .results
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, stored)| {
            doc.science
                .attachment(stored.attachment)
                .is_some_and(|attachment| {
                    attachment.atoms.contains(&Some(*atom))
                        && attachment
                            .atoms
                            .iter()
                            .flatten()
                            .all(|id| model.contains(*id))
                        && result_key(&stored.result) == figure.orbital.result
                })
        });
    // A repeated calculation shares the key: the figure stays with the
    // result that keeps its row, so its signs never change.
    let newest = matches.next()?;
    let keeps = |stored: &Stored| {
        stored.result.orbitals.as_ref().is_some_and(|orbitals| {
            let set = match (&orbitals.beta, figure.orbital.spin) {
                (Beta::Unrestricted(beta), Spin::Beta) => beta,
                _ => &orbitals.alpha,
            };
            set.rows.binary_search(&figure.orbital.index).is_ok()
        })
    };
    if keeps(newest.1) {
        return Some(newest);
    }
    Some(matches.find(|(_, stored)| keeps(stored)).unwrap_or(newest))
}

/// Clipboard calculation data needed by the document's placed orbital figures.
/// Only their linked molecules' referenced results survive, in light form with
/// the figures' coefficient rows and basis. Other calculation data stays behind.
pub fn for_figures(doc: &Document) -> Science {
    for_figures_from(doc, &doc.science, &[])
}

pub(crate) fn for_figures_from(
    doc: &Document,
    source: &Science,
    stationary: &[&crate::graphics::MoDiagram],
) -> Science {
    let mut molecules = HashMap::<&str, Vec<HashSet<u64>>>::new();
    let mut pins = HashMap::<&str, Vec<_>>::new();
    for model in &doc.models {
        for figure in model.figures() {
            let orbital = &figure.orbital;
            molecules
                .entry(&orbital.result)
                .or_default()
                .push(model.linked_atoms.iter().copied().collect());
            pins.entry(&orbital.result)
                .or_default()
                .push((orbital.spin, orbital.index));
        }
    }
    for mo in doc
        .graphics
        .iter()
        .filter_map(|g| g.mo.as_deref())
        .chain(stationary.iter().copied())
    {
        if let Some(model) = mo.model(doc) {
            molecules
                .entry(&mo.result)
                .or_default()
                .push(model.linked_atoms.iter().copied().collect());
            for orbital in mo.pictures.iter().map(|p| &p.figure.orbital) {
                if orbital.result != mo.result {
                    molecules
                        .entry(&orbital.result)
                        .or_default()
                        .push(model.linked_atoms.iter().copied().collect());
                }
                pins.entry(&orbital.result)
                    .or_default()
                    .push((orbital.spin, orbital.index));
            }
        }
    }
    if molecules.is_empty() {
        return Science::default();
    }
    let results: Vec<_> = source
        .results
        .iter()
        .filter_map(|stored| {
            let attachment = source.attachment(stored.attachment)?;
            let key = result_key(&stored.result);
            let linked = molecules.get(key.as_str())?;
            let belongs = linked.iter().any(|atoms| {
                attachment.atoms.iter().any(Option::is_some)
                    && attachment
                        .atoms
                        .iter()
                        .flatten()
                        .all(|id| atoms.contains(id))
            });
            belongs.then(|| Stored {
                attachment: stored.attachment,
                keep_full: false,
                result: Arc::new(
                    stored
                        .result
                        .light(pins.get(key.as_str()).map_or(&[][..], Vec::as_slice)),
                ),
            })
        })
        .collect();
    let attachments: HashSet<u64> = results.iter().map(|stored| stored.attachment).collect();
    let mut science = Science {
        frames: source.frames.clone(),
        attachments: source
            .attachments
            .iter()
            .filter(|attachment| attachments.contains(&attachment.id))
            .cloned()
            .collect(),
        results,
    };
    science.collect_garbage();
    science
}

/// Keep a recomputed orbital row with its result so a placed figure reopens
/// with the same phase. Existing rows keep their original coefficients and
/// shared payload. Nothing changes on error.
pub fn pin_orbital(
    doc: &mut Document,
    stored_index: usize,
    spin: Spin,
    orbital_index: u32,
    basis: &BasisSet,
    row: &[f64],
) -> Result<(), ScienceError> {
    let stored = doc
        .science
        .results
        .get(stored_index)
        .ok_or(ScienceError::Unknown)?;
    let invalid =
        |message: &str| ScienceError::Invalid(reshiki_qcdata::Error::Invalid(message.into()));
    let orbitals = stored
        .result
        .orbitals
        .as_ref()
        .ok_or_else(|| invalid("Calculation has no orbital spectrum"))?;
    if orbitals
        .basis
        .as_ref()
        .is_some_and(|stored| stored != basis)
    {
        return Err(invalid(
            "Pinned orbital basis differs from the stored basis",
        ));
    }
    let set = match (spin, &orbitals.beta) {
        (Spin::Beta, Beta::Unrestricted(set)) => set,
        _ => &orbitals.alpha,
    };
    let position = match set.rows.binary_search(&orbital_index) {
        Ok(_) => return Ok(()),
        Err(position) => position,
    };
    let frame = doc
        .science
        .frame(&stored.result.frame)
        .ok_or(ScienceError::Unknown)?;
    let n_ao: usize = basis.shells.iter().map(|shell| shell.n_functions()).sum();
    if row.len() != n_ao {
        return Err(invalid("Pinned orbital row must match its basis"));
    }
    let mut result = stored.result.as_ref().clone();
    let orbitals = result
        .orbitals
        .as_mut()
        .ok_or_else(|| invalid("Calculation has no orbital spectrum"))?;
    orbitals.basis = Some(basis.clone());
    let set = match (spin, &mut orbitals.beta) {
        (Spin::Beta, Beta::Unrestricted(set)) => set,
        _ => &mut orbitals.alpha,
    };
    set.rows.insert(position, orbital_index);
    set.coefficients
        .splice(position * n_ao..position * n_ao, row.iter().copied());
    result.validate(frame)?;
    if doc
        .science
        .heap_bytes()
        .saturating_sub(stored.result.heap_bytes())
        .saturating_add(result.heap_bytes())
        > MAX_SCIENCE_BYTES
    {
        return Err(ScienceError::TooLarge);
    }
    doc.science
        .results
        .get_mut(stored_index)
        .ok_or(ScienceError::Unknown)?
        .result = Arc::new(result);
    Ok(())
}

/// Move the molecule's 3D model to `frame`, a geometry stored for
/// `attachment` (its own frame or a result's, such as an optimization's
/// final frame): "Apply xTB geometry", one undo step for the caller. The 2D
/// drawing does not move. Nothing changes on error.
pub fn apply_geometry(
    doc: &mut Document,
    attachment: u64,
    frame: &FrameHash,
) -> Result<(), ScienceError> {
    let science = &doc.science;
    let link = science
        .attachment(attachment)
        .ok_or(ScienceError::Unknown)?;
    let own = link.frame == *frame
        || science
            .results
            .iter()
            .any(|s| s.attachment == attachment && s.result.frame == *frame);
    let frame = science
        .frame(frame)
        .filter(|_| own)
        .ok_or(ScienceError::Unknown)?;
    if link.freshness(doc) == Freshness::Stale {
        return Err(ScienceError::Stale);
    }
    let atoms = link.atoms.clone();
    let frame = frame.clone();
    move_model(doc, &atoms, &frame)
}

/// Move the 3D model linked to `atoms` (the drawing atom of each frame
/// atom, as an attachment records them) to `frame`'s geometry, when the
/// model still has those atoms in that order and each added hydrogen still
/// sits nearest its own parent; otherwise [`ScienceError::ModelMismatch`].
/// For geometries no result stores, such as an optimization step; the
/// caller checks freshness. Nothing changes on error.
pub fn move_model(
    doc: &mut Document,
    atoms: &[Option<u64>],
    frame: &Frame,
) -> Result<(), ScienceError> {
    if atoms.len() != frame.numbers.len() || frame.positions_angstrom.len() != frame.numbers.len() {
        return Err(ScienceError::ModelMismatch);
    }
    let index = atoms
        .iter()
        .flatten()
        .next()
        .and_then(|atom| model_of(doc, *atom))
        .ok_or(ScienceError::NoModel)?;
    let model = doc.models.get(index).ok_or(ScienceError::NoModel)?;
    let heavy = model.atoms.len();
    let positions: Vec<[f32; 3]> = frame
        .positions_angstrom
        .iter()
        .map(|p| p.map(|x| x as f32))
        .collect();
    let parent_of = |h: &crate::models3d::Hydrogen| -> Option<usize> {
        // Generated models index their parents; imported ones use drawing IDs.
        if model.generated.is_some() {
            usize::try_from(h.parent).ok()
        } else {
            model.atoms.iter().position(|a| a.id == h.parent)
        }
    };
    let matches = model.cell.is_none()
        && heavy + model.hydrogens.len() == frame.numbers.len()
        && model
            .geometry_sources()
            .iter()
            .chain(std::iter::repeat(&None))
            .zip(atoms)
            .all(|(model, link)| model == link)
        && frame.numbers.iter().enumerate().all(|(i, z)| {
            let element = if i < heavy {
                model_element(doc, model, i)
            } else {
                Some("H")
            };
            element.and_then(atomic_number) == Some(*z)
        })
        && positions.iter().flatten().all(|v| {
            v.is_finite() && v.abs() <= crate::models3d::MAX_COORDINATE
        })
        // Each added hydrogen sits nearest its own parent in the frame, so
        // the model's hydrogen order still matches the frame's.
        && model.hydrogens.iter().enumerate().all(|(i, h)| {
            let at = frame.positions_angstrom.get(heavy + i);
            let nearest = at.and_then(|at| {
                (0..heavy).min_by(|&a, &b| {
                    let d = |j: usize| {
                        frame.positions_angstrom.get(j).map_or(f64::INFINITY, |p| {
                            p.iter().zip(at).map(|(a, b)| (a - b).powi(2)).sum::<f64>()
                        })
                    };
                    d(a).total_cmp(&d(b))
                })
            });
            nearest.is_some() && nearest == parent_of(h)
        });
    if !matches {
        return Err(ScienceError::ModelMismatch);
    }
    let model = doc.models.get_mut(index).ok_or(ScienceError::NoModel)?;
    let mut positions = positions.into_iter();
    for (atom, xyz) in model.atoms.iter_mut().zip(positions.by_ref()) {
        atom.xyz = xyz;
    }
    for (hydrogen, xyz) in model.hydrogens.iter_mut().zip(positions) {
        hydrogen.xyz = xyz;
    }
    Ok(())
}

/// Stereocentres and E/Z double bonds the drawing defines that `frame` does
/// not reproduce. `atoms` maps frame atoms to drawing atoms (a Launch's or an
/// Attachment's map). Empty when they all agree.
pub fn stereo_changes(
    doc: &Document,
    atoms: &[Option<u64>],
    frame: &Frame,
) -> Result<Vec<StereoChange>, ScienceError> {
    use crate::chemistry::{
        kekulize::Direction,
        stereo::{self, Point3, SpatialAnnotations, SpatialOptions, wedging::Conformer},
    };

    if atoms.len() != frame.numbers.len() || atoms.len() != frame.positions_angstrom.len() {
        return Err(ScienceError::ModelMismatch);
    }
    let ids: Vec<_> = atoms.iter().flatten().copied().collect();
    let mut part = crate::editing::analysis_document(doc, &ids);
    part.models.clear();
    let molecule = crate::chemistry::document::prepare(&part)
        .map_err(|e| ScienceError::Chemistry(e.to_string()))?;
    let state = &molecule.state;
    let positions: HashMap<_, _> = atoms
        .iter()
        .zip(&frame.positions_angstrom)
        .filter_map(|(&id, &[x, y, z])| id.map(|id| (id, Point3 { x, y, z })))
        .collect();
    let positions = molecule
        .ids
        .iter()
        .map(|id| {
            positions
                .get(id)
                .copied()
                .ok_or(ScienceError::ModelMismatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let conformer = Conformer {
        positions,
        is_3d: true,
    };
    let directions: Vec<_> = state
        .directions
        .iter()
        .map(|&direction| match direction {
            Direction::Wedge | Direction::Hash => Direction::None,
            other => other,
        })
        .collect();
    let geometry = stereo::from_3d(
        &state.graph,
        &state.metadata,
        &directions,
        Some(&conformer),
        &SpatialAnnotations {
            non_explicit: vec![None; molecule.ids.len()],
            done: None,
        },
        SpatialOptions {
            replace_existing: true,
            allow_nontetrahedral: false,
        },
    )
    .map_err(|e| ScienceError::Chemistry(e.to_string()))?;

    // A wavy single bond leaves the centres it touches unspecified.
    let wiggly: HashSet<usize> = state
        .graph
        .bonds
        .iter()
        .zip(&directions)
        .zip(&state.metadata.bonds)
        .filter(|((bond, direction), meta)| {
            bond.order == 1 && (**direction == Direction::Unknown || meta.unknown_stereo)
        })
        .flat_map(|((bond, _), _)| [bond.a, bond.b])
        .collect();
    let id = |i: usize| {
        molecule
            .ids
            .get(i)
            .copied()
            .ok_or(ScienceError::ModelMismatch)
    };
    let position = |i: usize| {
        conformer
            .positions
            .get(i)
            .copied()
            .ok_or(ScienceError::ModelMismatch)
    };
    let mut changes = Vec::new();
    let centres = state.metadata.atoms.iter().zip(&geometry.metadata.atoms);
    for (i, (drawn, spatial)) in centres.enumerate() {
        // Non-tetrahedral tags are outside this comparison.
        if wiggly.contains(&i) || !matches!(drawn.chiral_tag, 1 | 2) {
            continue;
        }
        let spatial = spatial.chiral_tag;
        if matches!(spatial, 0..=2) && spatial != drawn.chiral_tag {
            let change = StereoChange::Centre {
                atom: id(i)?,
                inverted: spatial != 0,
            };
            changes.push((i, change));
        }
    }
    for (bond, drawn) in state.graph.bonds.iter().zip(&state.metadata.bonds) {
        let expected_cis = match drawn.stereo {
            2 | 4 => true,
            3 | 5 => false,
            _ => continue,
        };
        let &[s0, s1] = drawn.stereo_atoms.as_slice() else {
            continue;
        };
        let angle = dihedral(
            position(s0)?,
            position(bond.a)?,
            position(bond.b)?,
            position(s1)?,
        );
        if angle.is_some_and(|angle| (angle < std::f64::consts::FRAC_PI_2) != expected_cis) {
            let change = StereoChange::DoubleBond {
                atoms: [id(bond.a)?, id(bond.b)?],
            };
            changes.push((bond.a.min(bond.b), change));
        }
    }
    changes.sort_by_key(|(index, _)| *index);
    Ok(changes.into_iter().map(|(_, change)| change).collect())
}

/// Classify potential tetrahedral centres in `ids` without requiring a 3D
/// model. A zero drawing chiral tag or a wavy single bond touching either end
/// leaves a centre undefined. With a frame and its drawing-atom map, infer
/// each undefined centre's winding and full CIP descriptor from that geometry.
/// Flat or unresolved centres remain in the result with absent descriptors.
/// Neither the drawing nor its saved stereochemistry changes.
pub fn tetrahedral_stereo(
    doc: &Document,
    ids: &[u64],
    frame: Option<(&[Option<u64>], &Frame)>,
) -> Result<TetrahedralStereo, ScienceError> {
    use crate::chemistry::{
        document,
        kekulize::Direction,
        stereo::{
            self, Point3, SpatialAnnotations, SpatialOptions, perception, wedging::Conformer,
        },
    };

    if frame.is_some_and(|(atoms, frame)| {
        atoms.len() != frame.numbers.len() || atoms.len() != frame.positions_angstrom.len()
    }) {
        return Err(ScienceError::ModelMismatch);
    }
    let chemistry = |error: document::Error| ScienceError::Chemistry(error.to_string());
    let mut part = crate::editing::analysis_document(doc, ids);
    part.models.clear();
    let mut molecule = document::prepare(&part).map_err(chemistry)?;
    let mut input = molecule.state.clone();
    // As in the drawing stereo lint, maps and cached labels must not make
    // chemically equivalent ligands distinct during potential-centre detection.
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
    let possible = if part.bonds.iter().any(|bond| bond.stereo_authoritative) {
        let mask: Vec<_> = part
            .bonds
            .iter()
            .map(|bond| bond.stereo_authoritative)
            .collect();
        perception::perceive_preserving_bonds(&input, options, &mask)
    } else {
        perception::perceive(&input, options)
    }
    .map_err(ScienceError::Chemistry)?;
    let (centres, _) = perception::stereogenic_units(&possible).map_err(ScienceError::Chemistry)?;
    let centres: HashSet<_> = centres.into_iter().collect();
    let wiggly: HashSet<_> = molecule
        .state
        .graph
        .bonds
        .iter()
        .zip(&molecule.state.directions)
        .zip(&molecule.state.metadata.bonds)
        .filter(|((bond, direction), metadata)| {
            bond.order == 1 && (**direction == Direction::Unknown || metadata.unknown_stereo)
        })
        .flat_map(|((bond, _), _)| [bond.a, bond.b])
        .collect();
    let mut result = TetrahedralStereo::default();
    let mut undefined = Vec::new();
    for (index, (&atom, drawn)) in molecule
        .ids
        .iter()
        .zip(&molecule.state.metadata.atoms)
        .enumerate()
    {
        if !centres.contains(&index) {
            continue;
        }
        if drawn.chiral_tag == 0 || wiggly.contains(&index) {
            undefined.push(index);
            result.undefined.push(UndefinedCentre {
                atom,
                cip: None,
                winding: None,
            });
        } else if matches!(drawn.chiral_tag, 1 | 2) {
            result.has_defined = true;
        }
    }
    let Some((atoms, frame)) = frame.filter(|_| !undefined.is_empty()) else {
        return Ok(result);
    };
    let positions: HashMap<_, _> = atoms
        .iter()
        .zip(&frame.positions_angstrom)
        .filter_map(|(&id, &[x, y, z])| id.map(|id| (id, Point3 { x, y, z })))
        .collect();
    let conformer = Conformer {
        positions: molecule
            .ids
            .iter()
            .map(|id| {
                positions
                    .get(id)
                    .copied()
                    .ok_or(ScienceError::ModelMismatch)
            })
            .collect::<Result<Vec<_>, _>>()?,
        is_3d: true,
    };
    // Wavy marks say what the drawing leaves open. They must not suppress
    // inference of the particular geometry the calculation produced.
    for ((bond, direction), metadata) in molecule
        .state
        .graph
        .bonds
        .iter()
        .zip(&mut molecule.state.directions)
        .zip(&mut molecule.state.metadata.bonds)
    {
        if matches!(*direction, Direction::Wedge | Direction::Hash)
            || bond.order == 1 && *direction == Direction::Unknown
        {
            *direction = Direction::None;
        }
        if bond.order == 1 {
            metadata.unknown_stereo = false;
        }
    }
    molecule.state.metadata = stereo::from_3d(
        &molecule.state.graph,
        &molecule.state.metadata,
        &molecule.state.directions,
        Some(&conformer),
        &SpatialAnnotations {
            non_explicit: vec![None; molecule.ids.len()],
            done: None,
        },
        SpatialOptions {
            replace_existing: true,
            allow_nontetrahedral: false,
        },
    )
    .map_err(|error| ScienceError::Chemistry(error.to_string()))?
    .metadata;
    molecule.set_conformer(conformer);
    let labels = document::for_drawing(&molecule, &part)
        .map_err(chemistry)?
        .labels()
        .ok();
    for (centre, index) in result.undefined.iter_mut().zip(undefined) {
        let tag = molecule
            .state
            .metadata
            .atoms
            .get(index)
            .ok_or(ScienceError::ModelMismatch)?
            .chiral_tag;
        centre.winding = matches!(tag, 1 | 2).then_some(tag);
        centre.cip = labels
            .as_ref()
            .and_then(|labels| labels.atoms.get(index))
            .cloned()
            .flatten();
    }
    Ok(result)
}

/// Unsigned dihedral between the substituents around the bond axis.
fn dihedral(
    s0: crate::chemistry::stereo::Point3,
    a: crate::chemistry::stereo::Point3,
    b: crate::chemistry::stereo::Point3,
    s1: crate::chemistry::stereo::Point3,
) -> Option<f64> {
    let axis = [b.x - a.x, b.y - a.y, b.z - a.z];
    let normal = |[x, y, z]: [f64; 3]| {
        let [u, v, w] = axis;
        let cross = [v * z - w * y, w * x - u * z, u * y - v * x];
        let length = cross[0].hypot(cross[1]).hypot(cross[2]);
        (length != 0.0).then(|| cross.map(|component| component / length))
    };
    let left = normal([s0.x - a.x, s0.y - a.y, s0.z - a.z])?;
    let right = normal([s1.x - b.x, s1.y - b.y, s1.z - b.z])?;
    let cosine = left.into_iter().zip(right).map(|(l, r)| l * r).sum::<f64>();
    Some(cosine.clamp(-1.0, 1.0).acos())
}

/// Set "Keep full results in this file" on the result at `index`.
pub fn set_keep_full(doc: &mut Document, index: usize, keep: bool) -> bool {
    match doc.science.results.get_mut(index) {
        Some(stored) if stored.keep_full != keep => {
            stored.keep_full = keep;
            true
        }
        _ => false,
    }
}

/// "Make lighter": results the engine can recompute stop keeping their full
/// data, in the file and in this document. Imported results, which cannot
/// be recomputed, are left alone. Returns how many results changed.
pub fn make_lighter(doc: &mut Document) -> usize {
    let pins = doc.orbital_pins();
    let mut changed = 0;
    for stored in &mut doc.science.results {
        if stored.keep_full && matches!(stored.result.settings, Settings::Xtb(_)) {
            let pinned = pins
                .get(&result_key(&stored.result))
                .map_or(&[][..], Vec::as_slice);
            stored.result = Arc::new(stored.result.light(pinned));
            stored.keep_full = false;
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests;
