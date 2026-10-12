//! Finite generic definitions are drawing data until deliberately materialized.
//! Expansion never changes the source and never infers an unspecified polymer.
use crate::document::{Document, Point};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
mod authoring;
mod definitions;
mod guards;
mod layout;
mod probe;
mod symmetry;

pub use authoring::{alternative, apply_label, define_bracket, set_repeat_unit};
pub use definitions::{prune, selection, validate};
pub use guards::{atom_references, component_intersects, intersects, touches};

use definitions::{check_range, pair, printable};
use guards::reject_affected_stereo;
use probe::Stage;

/// A detached concrete drawing and its positioned label/object extents. These
/// bounds are transient layout output, never authoritative document metadata.
#[derive(Debug)]
pub struct ExpandedDrawing {
    pub document: Document,
    pub bounds: (Point, Point),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Generic {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<AlternativeSite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeats: Vec<RepeatUnit>,
}
impl Generic {
    pub fn is_empty(&self) -> bool {
        self.alternatives.is_empty() && self.links.is_empty() && self.repeats.is_empty()
    }
    /// Whether an alternative or link site owns this atom's label.
    pub fn owns_label(&self, atom: u64) -> bool {
        self.alternatives.iter().any(|site| site.atom == atom)
            || self.links.iter().any(|site| site.atom == atom)
    }
    pub fn extend(&mut self, other: Self) {
        self.alternatives.extend(other.alternatives);
        self.links.extend(other.links);
        self.repeats.extend(other.repeats);
    }
    pub fn remap(&mut self, ids: &HashMap<u64, u64>) -> Option<()> {
        for site in &mut self.alternatives {
            site.atom = *ids.get(&site.atom)?;
        }
        for link in &mut self.links {
            link.atom = *ids.get(&link.atom)?;
        }
        for unit in &mut self.repeats {
            if let Some(id) = &mut unit.bracket {
                *id = *ids.get(id)?;
            }
            for id in &mut unit.bracket_objects {
                *id = *ids.get(id)?;
            }
            for id in &mut unit.members {
                *id = *ids.get(id)?;
            }
            for c in &mut unit.crossings {
                c.inside = *ids.get(&c.inside)?;
                c.outside = *ids.get(&c.outside)?;
            }
            for id in &mut unit.crossing_brackets {
                *id = *ids.get(id)?;
            }
        }
        Some(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Alternative {
    Element {
        symbol: String,
        #[serde(default)]
        isotope: u32,
    },
    Group {
        label: String,
    },
    Unresolved {
        label: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlternativeSite {
    pub atom: u64,
    pub choices: Vec<Alternative>,
    #[serde(default)]
    pub excluded: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkNode {
    pub atom: u64,
    pub min: u32,
    pub max: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crossing {
    pub inside: u64,
    pub outside: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatPattern {
    HeadToTail,
    HeadToHead,
    EitherUnknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepeatCount {
    Finite { min: u32, max: u32 },
    Indefinite { label: String },
}
impl RepeatCount {
    pub fn parse(label: &str) -> Result<Self, String> {
        let label = label.trim();
        if label.is_empty() || label.len() > 64 || label.chars().any(char::is_control) {
            return Err("Enter a repeat count or label, such as 1-3 or n".into());
        }
        if let Ok(n) = label.parse::<u32>() {
            check_range(n, n)?;
            return Ok(Self::Finite { min: n, max: n });
        }
        if let Some((a, b)) = label.split_once('-')
            && a.trim().chars().all(|c| c.is_ascii_digit())
            && b.trim().chars().all(|c| c.is_ascii_digit())
        {
            let min = a.trim().parse().map_err(|_| "Invalid repeat count")?;
            let max = b.trim().parse().map_err(|_| "Invalid repeat count")?;
            check_range(min, max)?;
            return Ok(Self::Finite { min, max });
        }
        Ok(Self::Indefinite {
            label: label.into(),
        })
    }
    fn range(&self) -> Result<(u32, u32), String> {
        match self {
            Self::Finite { min, max } => Ok((*min, *max)),
            Self::Indefinite { .. } => {
                Err("An indefinite polymer needs a finite repeat count before expansion".into())
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatUnit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bracket: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bracket_objects: Vec<u64>,
    pub members: Vec<u64>,
    /// Ordered head and tail bonds for an ordinary linear unit. Larger sets
    /// retain ladder/copolymer input without inventing a connection order.
    pub crossings: Vec<Crossing>,
    /// Source bracketattachment graphic IDs, in crossing order. Empty uses
    /// the primary paired graphic for definitions created in ReShiki.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crossing_brackets: Vec<u64>,
    /// Number of leading crossing bonds explicitly assigned as heads. Absence
    /// preserves an input whose head/tail ports were never specified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_count: Option<usize>,
    pub usage: String,
    pub pattern: RepeatPattern,
    pub count: RepeatCount,
    pub label: String,
    #[serde(default)]
    pub flip: String,
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_outputs: usize,
    pub max_total_atoms: usize,
    pub max_work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_outputs: 500,
            max_total_atoms: 100_000,
            max_work: 5_000_000,
        }
    }
}
pub const NOTICE: &str = "Generic choices retained. Expand Generic Structures to choose concrete atoms, attachment positions or finite repeats before calculating identifiers or molecular properties.";
/// Joining referenced generic structures requires concrete chemical choices first.
pub const JOIN_NOTICE: &str =
    "Expand Generic Structures before joining referenced generic atoms or repeating units.";

pub fn present(doc: &Document) -> bool {
    !doc.generic.is_empty() || doc.atoms.iter().any(|a| a.is_variable_attachment())
}
fn bonds_at(doc: &Document, id: u64) -> Vec<&crate::document::Bond> {
    doc.bonds
        .iter()
        .filter(|b| b.a == id || b.b == id)
        .collect()
}

#[derive(Clone)]
enum Site {
    Alternative(AlternativeSite),
    Variable(u64, Vec<u64>),
    Link(LinkNode),
    Repeat(RepeatUnit),
}
impl Site {
    fn count(&self) -> Result<usize, String> {
        match self {
            Self::Alternative(s) => {
                if s.excluded {
                    return Err("An element NOT list has no finite enumeration".into());
                }
                if s.choices
                    .iter()
                    .any(|c| matches!(c, Alternative::Unresolved { .. }))
                {
                    return Err(
                        "A generic nickname needs a defined fragment before expansion".into(),
                    );
                }
                Ok(s.choices.len())
            }
            Self::Variable(_, v) => Ok(v.len()),
            Self::Link(s) => Ok((s.max - s.min + 1) as usize),
            Self::Repeat(s) => {
                let (min, max) = s.count.range()?;
                Ok((max - min + 1) as usize)
            }
        }
    }
}
struct Work {
    remaining: usize,
}
impl Work {
    fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }
    fn charge(&mut self, amount: usize) -> Result<(), String> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or("Generic expansion exceeds the work limit")?;
        Ok(())
    }
}
fn drawing_size(doc: &Document) -> usize {
    doc.atoms
        .len()
        .saturating_add(doc.bonds.len())
        .saturating_add(doc.annotations.len())
        .saturating_add(doc.arrows.len())
        .saturating_add(doc.graphics.len())
        .saturating_add(
            doc.groups
                .iter()
                .map(|g| g.members.len() + 1)
                .sum::<usize>(),
        )
        .saturating_add(
            doc.abbreviations
                .iter()
                .map(|g| g.members.len())
                .sum::<usize>(),
        )
}
/// Allocation is checked once before appending. No growing-document scans occur
/// inside a replication loop, and the reserved sentinel ID is never produced.
struct Ids {
    next: u64,
    remaining: usize,
}
impl Ids {
    fn new(doc: &Document, count: usize) -> Result<Self, String> {
        let maximum = doc
            .object_ids()
            .chain(doc.groups.iter().map(|g| g.id))
            .max()
            .unwrap_or(0);
        let next = maximum
            .checked_add(1)
            .ok_or("Generic object ID limit exceeded")?;
        let count64 = u64::try_from(count).map_err(|_| "Generic object ID limit exceeded")?;
        if next.checked_add(count64).is_none() {
            return Err("Generic object ID limit exceeded".into());
        }
        Ok(Self {
            next,
            remaining: count,
        })
    }
    fn take(&mut self) -> Result<u64, String> {
        if self.remaining == 0 || self.next == u64::MAX {
            return Err("Generic object ID limit exceeded".into());
        }
        let id = self.next;
        self.next += 1;
        self.remaining -= 1;
        Ok(id)
    }
}
fn ordinary_bond(a: u64, b: u64) -> crate::document::Bond {
    crate::document::Bond::plain(a, b, 1, "plain")
}
fn clear_after_expansion(doc: &mut Document) {
    // The explicit-stereo guard has already checked every affected center and
    // bond. Cached labels need clearing, but unrelated captured stereo remains.
    crate::atom_labels::clear_computed(doc);
    for a in &mut doc.atoms {
        a.label_h = 0;
    }
}
/// Enumerate chemistry without changing the materializers' coordinates. This
/// API retains large finite graph enumeration under its existing work bounds;
/// generated canvas layout/ink measurement has a separate, stricter cost.
/// Verification entry point used by io round-trip and app tests.
#[doc(hidden)]
pub fn expand(source: &Document, limits: Limits) -> Result<Vec<Document>, String> {
    Ok(expand_inner(source, limits, false)?
        .drawings
        .into_iter()
        .map(|p| p.document)
        .collect())
}

/// Materialize every finite choice atomically. Link/repeat components are
/// depicted; a detachable variable substituent receives only a rigid exterior
/// pose. All products share one work allowance.
/// Changed components with generated-name, attached-arrow or depth-scope
/// dependencies fail closed. Independent native associations are inherited
/// exactly; this API does not freshly verify arbitrary saved name claims.
/// Verification entry point used by io round-trip and app tests.
#[doc(hidden)]
pub fn expand_drawings(source: &Document, limits: Limits) -> Result<Vec<ExpandedDrawing>, String> {
    Ok(expand_inner(source, limits, true)?.drawings)
}
struct ExpandedBatch {
    drawings: Vec<ExpandedDrawing>,
    hydrogens: Vec<Vec<u32>>,
    work: Work,
    presentation: Option<layout::Presentation>,
}

/// Cheaply cloned, single-use transport for an already bounded, measured native
/// insertion. No caller can mutate the sealed drawing or construct a proof.
#[derive(Clone)]
pub struct PreparedExpansion {
    data: std::sync::Arc<std::sync::Mutex<Option<PreparedData>>>,
    count: usize,
    bounds: (Point, Point),
}
pub(crate) struct PreparedData {
    pub(crate) document: Document,
    pub(crate) count: usize,
    pub(crate) bounds: (Point, Point),
}
impl std::fmt::Debug for PreparedExpansion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedExpansion")
            .field("count", &self.count)
            .field("bounds", &self.bounds)
            .finish_non_exhaustive()
    }
}
impl PreparedExpansion {
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn bounds(&self) -> (Point, Point) {
        self.bounds
    }
    pub(crate) fn take(self) -> Result<PreparedData, String> {
        self.data
            .lock()
            .map_err(|_| "Prepared expansion transport is unavailable".to_owned())?
            .take()
            .ok_or_else(|| "Prepared expansion was already consumed".into())
    }
}

/// Owned, prepaid worker input. It is moved into the task and cannot be cloned
/// or constructed by hosts; its original shared work allowance never resets.
pub struct ExpansionInput {
    source: Document,
    limits: Limits,
    work: Work,
    presentation: layout::Presentation,
}
/// Borrow the live drawing and selection, reject costs before snapshot/selection
/// copies or scheduling. Selection retains complete dependencies without repair.
pub fn prepare_expansion_input(
    source: &Document,
    selected: &[u64],
    limits: Limits,
) -> Result<ExpansionInput, String> {
    if limits.max_outputs == 0 || limits.max_total_atoms == 0 || limits.max_work == 0 {
        return Err("Expansion limits must be positive".into());
    }
    if source.atoms.len() > 100_000
        || source.bonds.len() > 300_000
        || selected.is_empty() && source.atoms.len() > limits.max_total_atoms
    {
        return Err("Generic expansion exceeds the source atom or bond limit".into());
    }
    let mut work = Work::new(limits.max_work);
    let presentation = layout::Presentation::inspect(source, &mut work)?;
    presentation.charge_input(source, selected, &mut work)?;
    let owned = if selected.is_empty() {
        probe::enter(Stage::Snapshot);
        source.clone()
    } else {
        // Whole source validation is covered by the selected-input reservation.
        source.validate()?;
        probe::enter(Stage::Snapshot);
        crate::editing::preserved_expansion_selection(source, selected)?
    };
    Ok(ExpansionInput {
        source: owned,
        limits,
        work,
        presentation,
    })
}
/// Finish an owned input on a worker using its exact remaining allowance.
pub fn expand_prepared(input: ExpansionInput) -> Result<PreparedExpansion, String> {
    let ExpansionInput {
        source,
        limits,
        work,
        presentation,
    } = input;
    let batch = expand_with_work(&source, limits, true, work, Some(presentation))?;
    assemble_expansion(&source, batch)
}

/// Assemble under the same aggregate remainder and publish an opaque one-use
/// result. Preserved tiles use exact remap/common translation; ordinary append
/// and its whole-result arrow/name repair are deliberately not this path.
#[cfg(test)]
pub(crate) fn prepare_expansion(
    source: &Document,
    limits: Limits,
) -> Result<PreparedExpansion, String> {
    assemble_expansion(source, expand_inner(source, limits, true)?)
}
fn assemble_expansion(
    source: &Document,
    batch: ExpandedBatch,
) -> Result<PreparedExpansion, String> {
    let ExpandedBatch {
        drawings,
        hydrogens,
        mut work,
        presentation,
    } = batch;
    let presentation = presentation.ok_or("Missing drawing presentation allowance")?;
    let count = drawings.len();
    work.charge(drawing_size(source).saturating_mul(32))?;
    let mut result = source.clone();
    result.version = crate::document::VERSION;
    result.atoms.clear();
    result.bonds.clear();
    result.annotations.clear();
    result.arrows.clear();
    result.graphics.clear();
    result.groups.clear();
    result.abbreviations.clear();
    result.reactions.clear();
    result.ring_fills.clear();
    result.depth_appearance.clear();
    result.molecule_names.clear();
    result.generic = Generic::default();
    let columns = (count as f32).sqrt().ceil().min(5.) as usize;
    let mut x = 0_f32;
    let mut y = 0_f32;
    let mut row_height = 0_f32;
    let mut grid: Option<(Point, Point)> = None;
    if hydrogens.len() != count {
        return Err("Prepared expansion lost cached label states".into());
    }
    for (i, (tile, hydrogens)) in drawings.into_iter().zip(hydrogens).enumerate() {
        if i > 0 && i % columns == 0 {
            x = 0.;
            y += row_height + 84.;
            row_height = 0.;
        }
        let (lo, _hi) = tile.bounds;
        // Start with the same one-world-unit guard used by retries below.
        // Exact-boundary translation can round the actual f32 ink just outside
        // the grid and waste a fully prepaid attempt. Measurement still checks
        // the translated tile; this inset does not replace its certificate.
        let mut shift = Point::new(x - lo.x + 1., y - lo.y + 1.);
        let mut accepted = None;
        // Supplementary cap: all attempts still debit the SAME Work before
        // baseline deepclone and scene/normalization/glyph calculation.
        for attempt in 0..3 {
            if !shift.is_finite() {
                return Err("Prepared expansion grid is nonfinite".into());
            }
            let total_atoms = result.atoms.len().saturating_add(tile.document.atoms.len());
            let total_size = drawing_size(&result).saturating_add(drawing_size(&tile.document));
            presentation.charge_assembly(
                source,
                layout::AssemblyCounts {
                    tile_atoms: tile.document.atoms.len(),
                    tile_size: drawing_size(&tile.document),
                    tiles: i + 1,
                    total_atoms,
                    total_size,
                },
                &mut work,
            )?;
            let (mut part, mapping, ids) =
                crate::editing::preserved_append_part(&result, &tile.document, shift)?;
            if ids.is_empty() {
                return Err("Prepared expansion has an empty tile".into());
            }
            layout::certify_translation(&tile.document, &part, &mapping, shift)?;
            crate::molecule_names::certify_inherited(&tile.document, &part, &mapping, shift)?;
            let bounds = layout::measure_translated(
                &mut part,
                &hydrogens,
                source,
                &presentation,
                &mut work,
            )?;
            part.validate()?;
            if bounds.0.x >= x && bounds.0.y >= y {
                accepted = Some((part, bounds));
                break;
            }
            if attempt == 2 {
                return Err("Prepared tile cannot reach a stable measured grid position; the source drawing is retained".into());
            }
            // Rebuild the original tile next time. A one-world-unit guard
            // prevents a subpixel correction loop; no chemistry/stereo changes.
            shift.x += (x - bounds.0.x).max(0.) + 1.;
            shift.y += (y - bounds.0.y).max(0.) + 1.;
        }
        let (part, (a, b)) = accepted.ok_or("Missing stable measured expansion tile")?;
        crate::editing::extend_append_part(&mut result, part);
        result.validate()?;
        grid = Some(grid.map_or((a, b), |(lo, hi)| {
            (
                Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                Point::new(hi.x.max(b.x), hi.y.max(b.y)),
            )
        }));
        x = b.x.max(x + 84.) + 84.;
        row_height = row_height.max((b.y - y).max(84.));
    }
    let bounds = grid.ok_or("Prepared expansion has no measured ink")?;
    presentation.charge_publication(
        source,
        count,
        result.atoms.len(),
        drawing_size(&result),
        &mut work,
    )?;
    // Native validation and all geometry/association checks have completed. The
    // transport moves the only prepared document; Clone copies the Arc only.
    Ok(PreparedExpansion {
        data: std::sync::Arc::new(std::sync::Mutex::new(Some(PreparedData {
            document: result,
            count,
            bounds,
        }))),
        count,
        bounds,
    })
}
fn expand_inner(source: &Document, limits: Limits, drawing: bool) -> Result<ExpandedBatch, String> {
    if limits.max_outputs == 0 || limits.max_total_atoms == 0 || limits.max_work == 0 {
        return Err("Expansion limits must be positive".into());
    }
    expand_with_work(source, limits, drawing, Work::new(limits.max_work), None)
}
fn expand_with_work(
    source: &Document,
    limits: Limits,
    drawing: bool,
    mut work: Work,
    inspected: Option<layout::Presentation>,
) -> Result<ExpandedBatch, String> {
    if source.atoms.len() > limits.max_total_atoms
        || source.atoms.len() > 100_000
        || source.bonds.len() > 300_000
    {
        return Err("Generic expansion exceeds the total atom or bond limit".into());
    }
    let presentation = if drawing {
        match inspected {
            Some(profile) => Some(profile),
            None => Some(layout::Presentation::inspect(source, &mut work)?),
        }
    } else {
        None
    };
    let source_size = drawing_size(source);
    // Validation scans repeat boundaries and attachment target lists. Debit a
    // conservative bound before validation or catalog construction starts.
    let definitions = source.generic.alternatives.len()
        + source.generic.links.len()
        + source.generic.repeats.len()
        + source.atoms.iter().map(|a| a.centroid.len()).sum::<usize>();
    work.charge(source_size.saturating_mul(definitions.saturating_add(10)))?;
    source.validate()?;
    if drawing {
        layout::protect_references(source, &mut work)?;
    }
    if source
        .reactions
        .iter()
        .any(|r| intersects(source, &r.ids()))
    {
        return Err("Generic expansion involving explicit reaction membership is unsupported; expand a separate generic structure first".into());
    }
    let replicated: HashSet<_> = source
        .generic
        .links
        .iter()
        .map(|s| s.atom)
        .chain(
            source
                .generic
                .repeats
                .iter()
                .flat_map(|s| s.members.iter().copied()),
        )
        .collect();
    if source
        .atoms
        .iter()
        .any(|a| replicated.contains(&a.id) && a.map_num > 0)
    {
        return Err("Mapped link nodes or polymer members cannot be replicated without an explicit mapping policy".into());
    }
    let mut owners = HashMap::<u64, usize>::new();
    for (index, unit) in source.generic.repeats.iter().enumerate() {
        for &id in &unit.members {
            owners.insert(id, index);
        }
    }
    let links: HashSet<_> = source.generic.links.iter().map(|s| s.atom).collect();
    for unit in &source.generic.repeats {
        if unit.crossings.iter().any(|c| {
            links.contains(&c.outside)
                || owners
                    .get(&c.inside)
                    .zip(owners.get(&c.outside))
                    .is_some_and(|(a, b)| a != b)
        }) {
            return Err("Directly adjacent repeating units or link nodes are retained but cannot be expanded together".into());
        }
    }
    let mut repeat_members = HashSet::new();
    for unit in &source.generic.repeats {
        let members: HashSet<_> = unit.members.iter().copied().collect();
        if unit.members.iter().any(|id| !repeat_members.insert(*id))
            || source
                .generic
                .alternatives
                .iter()
                .any(|s| members.contains(&s.atom))
            || source
                .generic
                .links
                .iter()
                .any(|s| members.contains(&s.atom))
            || source.atoms.iter().any(|a| {
                a.attachment.is_some()
                    && (members.contains(&a.id) || a.centroid.iter().any(|id| members.contains(id)))
            })
        {
            return Err(
                "Nested or overlapping generic repeating units are retained but cannot be expanded"
                    .into(),
            );
        }
    }
    let mut sites = Vec::new();
    sites.extend(
        source
            .generic
            .alternatives
            .iter()
            .cloned()
            .map(Site::Alternative),
    );
    sites.extend(
        source
            .atoms
            .iter()
            .filter(|a| a.is_variable_attachment())
            .map(|a| Site::Variable(a.id, a.centroid.clone())),
    );
    sites.extend(source.generic.links.iter().cloned().map(Site::Link));
    sites.extend(source.generic.repeats.iter().cloned().map(Site::Repeat));
    if sites.is_empty() {
        return Err(
            "Select a structure containing alternatives, variable attachments or finite repeats"
                .into(),
        );
    }
    let mut outputs = 1usize;
    for site in &sites {
        outputs = outputs
            .checked_mul(site.count()?)
            .ok_or("Too many generic combinations")?;
        if outputs > limits.max_outputs.min(500) {
            return Err(
                "Generic expansion exceeds the output limit (maximum 500 structures)".into(),
            );
        }
    }
    let mut fragments = HashMap::<String, Document>::new();
    let mut upper_atoms = source.atoms.len();
    let mut upper_size = source_size;
    let mut total_atoms = (source.atoms.len() as i128) * (outputs as i128);
    let mut partial_outputs = 1usize;
    let mut construction_copies = 1usize;
    let mut planned_work = 0usize;
    for site in &sites {
        let count = site.count()?;
        let (extra, graph_extra, summed_atoms) = match site {
            Site::Alternative(s) => {
                let mut max_atoms = 0;
                let mut max_graph = 0;
                let mut sum_atoms = 0i128;
                for c in &s.choices {
                    match c {
                        Alternative::Element { .. } => {}
                        Alternative::Group { label } => {
                            if !fragments.contains_key(label) {
                                // Charge once, BEFORE catalog graph preparation.
                                work.charge(4096)?;
                                let mut fragment = Document::default();
                                let id = fragment.add_atom("*", Point::new(0., 0.));
                                fragments.insert(label.clone(), group(&fragment, id, label)?);
                            }
                            let f = fragments.get(label).ok_or("Missing fragment")?;
                            let atoms = f.atoms.len().saturating_sub(1);
                            max_atoms = max_atoms.max(atoms);
                            max_graph = max_graph.max(atoms + f.bonds.len());
                            sum_atoms += atoms as i128;
                        }
                        Alternative::Unresolved { .. } => {
                            return Err("Unresolved alternative".into());
                        }
                    }
                }
                (max_atoms, max_graph, sum_atoms)
            }
            Site::Variable(..) => (0, 0, -(count as i128)),
            Site::Link(s) => {
                let n = s.max.saturating_sub(1) as usize;
                let sum: i128 = (s.min..=s.max).map(|n| n.saturating_sub(1) as i128).sum();
                (n, n.saturating_mul(2), sum)
            }
            Site::Repeat(s) => {
                let (min, max) = s.count.range()?;
                let members: HashSet<_> = s.members.iter().copied().collect();
                let internal = source
                    .bonds
                    .iter()
                    .filter(|b| members.contains(&b.a) && members.contains(&b.b))
                    .count();
                let copies = max.saturating_sub(1) as usize;
                let sum_copies: i128 = (min..=max).map(|n| n.saturating_sub(1) as i128).sum();
                (
                    s.members.len().saturating_mul(copies),
                    (s.members.len() + internal + 1).saturating_mul(copies),
                    sum_copies * s.members.len() as i128,
                )
            }
        };
        upper_atoms = upper_atoms
            .checked_add(extra)
            .ok_or("Generic atom count overflow")?;
        upper_size = upper_size
            .checked_add(graph_extra)
            .ok_or("Generic work count overflow")?;
        total_atoms += summed_atoms * (outputs / count) as i128;
        partial_outputs = partial_outputs
            .checked_mul(count)
            .ok_or("Too many generic combinations")?;
        construction_copies = construction_copies.saturating_add(partial_outputs);
        planned_work = planned_work.saturating_add(
            upper_size
                .saturating_mul(partial_outputs)
                .saturating_mul(10),
        );
    }
    if upper_atoms > 100_000 || total_atoms < 0 || total_atoms > limits.max_total_atoms as i128 {
        return Err("Generic expansion exceeds the total atom limit".into());
    }
    // Reserve construction, validation and final graph preparation before
    // cloning even the first candidate. Every graph append below is linear.
    work.charge(
        planned_work.saturating_add(upper_size.saturating_mul(outputs).saturating_mul(10)),
    )?;
    if let Some(presentation) = &presentation {
        // Seed + every intermediate Cartesian materializer and its validation
        // (which may clone a theme reference), then bounded final snapshots,
        // validation-theme copies and app assembly for each output.
        // All owned text/style/path buffers are reserved before the seed copy.
        presentation.reserve(
            source,
            upper_atoms,
            upper_size,
            construction_copies
                .saturating_mul(2)
                .saturating_add(outputs.saturating_mul(8)),
            &mut work,
        )?;
    }
    probe::enter(Stage::Snapshot);
    let mut seed = source.clone();
    seed.generic = Generic::default();
    for link in &source.generic.links {
        if let Some(atom) = seed.atom_mut(link.atom) {
            atom.display.variable = None;
        }
    }
    let mut products = vec![seed];
    for site in sites {
        let count = site.count()?;
        let mut next = Vec::new();
        let mut atoms = 0usize;
        for product in products {
            for i in 0..count {
                probe::enter(Stage::Materialize);
                let mut candidate =
                    materialize(&product, &site, i, &fragments, drawing, &mut work)?;
                candidate.validate()?;
                atoms = atoms.saturating_add(candidate.atoms.len());
                if atoms > limits.max_total_atoms {
                    return Err("Generic expansion exceeds the total atom limit".into());
                }
                crate::atom_labels::clear_computed(&mut candidate);
                next.push(candidate);
            }
        }
        products = next;
    }
    let anchors: HashSet<_> = source
        .generic
        .links
        .iter()
        .map(|s| s.atom)
        .chain(
            source
                .generic
                .repeats
                .iter()
                .flat_map(|s| s.members.iter().copied()),
        )
        .collect();
    let mut drawings = Vec::with_capacity(products.len());
    let mut hydrogens = Vec::new();
    let mut unique = symmetry::Deduplicator::default();
    for mut product in products {
        if product
            .atoms
            .iter()
            .any(|a| a.attachment.is_some() || a.element == "*" || !a.centroid.is_empty())
        {
            return Err("Expansion still contains an unresolved attachment or atom".into());
        }
        // Index/copy/ink/clearance work is reserved before the existing final
        // preparation. Its Molecule is reused; no cleanup/identity/rewedge call.
        let plan = if drawing {
            Some(layout::Plan::new(&product, &anchors, &mut work)?)
        } else {
            None
        };
        probe::enter(Stage::Prepare);
        let molecule = crate::chemistry::document::prepare(&product)
            .map_err(|e| format!("Expanded structure has invalid chemistry: {e}"))?;
        if !unique.insert(&molecule, &mut work)? {
            continue;
        }
        let label_state = if drawing {
            work.charge(
                product
                    .atoms
                    .len()
                    .saturating_mul(std::mem::size_of::<u32>()),
            )?;
            let values: Result<Vec<_>, String> = molecule
                .state
                .graph
                .atoms
                .iter()
                .zip(&molecule.state.valences)
                .map(|(a, v)| {
                    u32::from(a.explicit_hydrogens)
                        .checked_add(v.implicit_hydrogens)
                        .ok_or_else(|| "Generated hydrogen label overflow".into())
                })
                .collect();
            let values = values?;
            layout::normalize_arrows(
                &mut product,
                &values,
                source,
                presentation.as_ref().ok_or("Missing drawing allowance")?,
                &mut work,
            )?;
            Some(values)
        } else {
            None
        };
        let bounds = if let Some(plan) = plan {
            plan.finish(&mut product, &molecule, &mut work)?
        } else {
            (Point::default(), Point::default())
        };
        drawings.push(ExpandedDrawing {
            document: product,
            bounds,
        });
        if let Some(values) = label_state {
            hydrogens.push(values);
        }
    }
    Ok(ExpandedBatch {
        drawings,
        hydrogens,
        work,
        presentation,
    })
}
fn materialize(
    doc: &Document,
    site: &Site,
    index: usize,
    fragments: &HashMap<String, Document>,
    drawing: bool,
    work: &mut Work,
) -> Result<Document, String> {
    match site {
        Site::Alternative(s) => {
            let choice = s.choices.get(index).ok_or("Missing alternative choice")?;
            match choice {
                Alternative::Element { symbol, isotope } => {
                    reject_affected_stereo(doc, &[s.atom])?;
                    let mut result = doc.clone();
                    let a = result.atom_mut(s.atom).ok_or("Missing alternative atom")?;
                    a.element = symbol.clone();
                    a.isotope = *isotope;
                    a.no_implicit = false;
                    a.display.variable = None;
                    result.invalidate_chemistry(&[s.atom]);
                    Ok(result)
                }
                Alternative::Group { label } => {
                    insert_fragment(doc, s.atom, fragments.get(label).ok_or("Missing fragment")?)
                }
                Alternative::Unresolved { .. } => Err("Unresolved generic alternative".into()),
            }
        }
        Site::Variable(id, targets) => {
            let target = *targets.get(index).ok_or("Missing attachment target")?;
            let contacts = bonds_at(doc, *id);
            if contacts.len() != 1 || contacts.iter().any(|b| b.order != 1) {
                return Err(
                    "Finite variable attachments require one ordinary single connecting bond"
                        .into(),
                );
            }
            let contact = contacts.first().ok_or("Missing variable attachment bond")?;
            reject_affected_stereo(doc, &[*id, target, contact.a, contact.b])?;
            let mut result = doc.clone();
            if drawing {
                let root = if contact.a == *id {
                    contact.b
                } else {
                    contact.a
                };
                layout::pose_variable(&mut result, *id, target, root, work)?;
            }
            result.atoms.retain(|a| a.id != *id);
            for b in &mut result.bonds {
                if b.a == *id {
                    b.a = target;
                }
                if b.b == *id {
                    b.b = target;
                }
            }
            result.groups.retain(|g| !g.members.contains(id));
            result.abbreviations.retain(|g| !g.members.contains(id));
            crate::projection::sync_centroids(&mut result);
            result.invalidate_chemistry(&[target]);
            Ok(result)
        }
        Site::Link(s) => link(doc, s, s.min + index as u32),
        Site::Repeat(s) => {
            let (min, _) = s.count.range()?;
            repeat(doc, s, min + index as u32)
        }
    }
}
fn group(doc: &Document, id: u64, label: &str) -> Result<Document, String> {
    let mut clean = doc.clone();
    clean
        .atom_mut(id)
        .ok_or("Missing alternative atom")?
        .display
        .variable = None;
    if label != "OPh" {
        let mut result = crate::chemistry::abbreviations::replace(&clean, &[id], label)
            .map_err(|e| e.to_string())?;
        result.expand_abbreviations(&[id]);
        return Ok(result);
    }
    let mut result = crate::chemistry::abbreviations::replace(&clean, &[id], "OMe")
        .map_err(|e| e.to_string())?;
    let members = result
        .abbreviation(id)
        .ok_or("Missing methoxy fragment")?
        .members
        .clone();
    let carbon = *members
        .iter()
        .find(|&&n| n != id)
        .ok_or("Missing methoxy carbon")?;
    result.expand_abbreviations(&[id]);
    result = crate::chemistry::abbreviations::replace(&result, &[carbon], "Ph")
        .map_err(|e| e.to_string())?;
    result.expand_abbreviations(&[carbon]);
    Ok(result)
}
/// A concrete catalog alternative, used by expansion and editable producers.
pub fn alternative_fragment(choice: &Alternative) -> Result<Document, String> {
    let mut doc = Document::default();
    let id = doc.add_atom("*", Point::default());
    match choice {
        Alternative::Group { label } => group(&doc, id, label),
        Alternative::Element { symbol, isotope } => {
            let a = doc.atom_mut(id).ok_or("Missing fragment root")?;
            a.element = symbol.clone();
            a.isotope = *isotope;
            a.no_implicit = false;
            Ok(doc)
        }
        Alternative::Unresolved { .. } => {
            Err("An unresolved nickname has no concrete alternative fragment".into())
        }
    }
}
fn insert_fragment(doc: &Document, id: u64, fragment: &Document) -> Result<Document, String> {
    reject_affected_stereo(doc, &[id])?;
    let anchor = doc.atom(id).ok_or("Missing alternative atom")?;
    let contacts = bonds_at(doc, id);
    if contacts.len() > 1 || contacts.iter().any(|b| b.order != 1) {
        return Err(
            "A group alternative requires one single attachment bond or an isolated atom".into(),
        );
    }
    let root = fragment.atoms.first().ok_or("Empty alternative fragment")?;
    // Cached fragments use the same existing template framework, pointing away
    // from an isolated site's default outside direction. Rotate that geometry
    // to the drawing's explicit attachment and scale by its bond length.
    let (angle, scale) = if let Some(b) = contacts.first() {
        let outside = doc
            .atom(if b.a == id { b.b } else { b.a })
            .ok_or("Missing group endpoint")?;
        let delta = Point::new(
            outside.position.x - anchor.position.x,
            outside.position.y - anchor.position.y,
        );
        let length = delta.distance(Point::default());
        if length < 0.1 {
            return Err("The group attachment bond has invalid geometry".into());
        }
        (
            (delta.y as f64).atan2(delta.x as f64) - std::f64::consts::PI,
            length as f64 / 42.,
        )
    } else {
        (0., 1.)
    };
    let (sin, cos) = angle.sin_cos();
    let mut allocator = Ids::new(doc, fragment.atoms.len().saturating_sub(1))?;
    let mut ids = HashMap::new();
    let mut atoms = Vec::with_capacity(fragment.atoms.len());
    for a in &fragment.atoms {
        let mut a = a.clone();
        let old = a.id;
        a.id = if old == root.id {
            id
        } else {
            allocator.take()?
        };
        let x = (a.position.x - root.position.x) as f64 * scale;
        let y = (a.position.y - root.position.y) as f64 * scale;
        a.position = anchor
            .position
            .offset((x * cos - y * sin) as f32, (x * sin + y * cos) as f32);
        if old == root.id {
            a.display = anchor.display.clone();
            a.display.variable = None;
            a.text_style = anchor.text_style.clone();
            a.map_num = anchor.map_num;
            a.depth = anchor.depth;
            a.marks = anchor.marks.clone();
        }
        ids.insert(old, a.id);
        atoms.push(a);
    }
    let members: Vec<_> = atoms.iter().map(|a| a.id).collect();
    let mut result = doc.clone();
    result.atoms.retain(|a| a.id != id);
    result.atoms.extend(atoms);
    for b in &fragment.bonds {
        let mut b = b.clone();
        b.a = *ids.get(&b.a).ok_or("Missing fragment bond")?;
        b.b = *ids.get(&b.b).ok_or("Missing fragment bond")?;
        result.bonds.push(b);
    }
    for g in &mut result.groups {
        if g.members.contains(&id) {
            g.members.retain(|n| *n != id);
            g.members.extend(&members);
        }
    }
    for reaction in &mut result.reactions {
        for role in [
            &mut reaction.reactants,
            &mut reaction.products,
            &mut reaction.agents,
        ] {
            for participant in role {
                if participant.atoms.contains(&id) {
                    participant.atoms.retain(|n| *n != id);
                    participant.atoms.extend(&members);
                }
            }
        }
    }
    result.abbreviations.retain(|g| !g.members.contains(&id));
    crate::ring_fills::prune(&mut result);
    clear_after_expansion(&mut result);
    Ok(result)
}
fn link(doc: &Document, node: &LinkNode, count: u32) -> Result<Document, String> {
    let atom = doc.atom(node.atom).ok_or("Missing link node")?;
    if atom.element == "*" {
        return Err("A generic link-node atom needs a concrete element before expansion".into());
    }
    let contacts = bonds_at(doc, node.atom);
    let [head, tail] = contacts.as_slice() else {
        return Err("A link node requires exactly two bonds".into());
    };
    if head.order != 1 || tail.order != 1 || atom.stereo.is_some() {
        return Err(
            "Link expansion supports ordinary single bonds without specified atom stereochemistry"
                .into(),
        );
    }
    let outside = if tail.a == node.atom { tail.b } else { tail.a };
    reject_affected_stereo(doc, &[node.atom, outside])?;
    let finish = doc.atom(outside).ok_or("Missing link endpoint")?.position;
    let mut result = doc.clone();
    result
        .atom_mut(node.atom)
        .ok_or("Missing link atom")?
        .display
        .variable = None;
    if count == 1 {
        return Ok(result);
    }
    result
        .bonds
        .retain(|b| pair(b.a, b.b) != pair(node.atom, outside));
    let mut previous = node.atom;
    let mut allocator = Ids::new(doc, count.saturating_sub(1) as usize)?;
    let mut atoms = Vec::with_capacity(count.saturating_sub(1) as usize);
    let mut bonds = Vec::with_capacity(count as usize);
    for i in 1..count {
        let mut copy = atom.clone();
        copy.id = allocator.take()?;
        copy.display.variable = None;
        copy.cip_label = None;
        copy.stereo = None;
        let ratio = i as f32 / count as f32;
        copy.position = Point::new(
            atom.position.x + (finish.x - atom.position.x) * ratio,
            atom.position.y + (finish.y - atom.position.y) * ratio,
        );
        let id = copy.id;
        atoms.push(copy);
        bonds.push(ordinary_bond(previous, id));
        previous = id;
    }
    let mut end = (*tail).clone();
    if end.a == node.atom {
        end.a = previous
    } else {
        end.b = previous
    }
    bonds.push(end);
    result.atoms.extend(atoms);
    result.bonds.extend(bonds);
    clear_after_expansion(&mut result);
    Ok(result)
}
fn repeat(doc: &Document, unit: &RepeatUnit, count: u32) -> Result<Document, String> {
    if !matches!(unit.usage.as_str(), "SRU" | "Generic")
        || unit.pattern != RepeatPattern::HeadToTail
        || !matches!(unit.flip.as_str(), "" | "NoFlip" | "Unspecified")
    {
        return Err("This polymer usage, repeat pattern or flip type is retained but cannot be finitely expanded".into());
    }
    let [head, tail] = unit.crossings.as_slice() else {
        return Err("Finite polymer expansion requires exactly two ordered crossing bonds; ladder polymers remain generic".into());
    };
    if unit.head_count != Some(1) {
        return Err(
            "Finite polymer expansion requires explicitly ordered head and tail ports".into(),
        );
    }
    let members: HashSet<_> = unit.members.iter().copied().collect();
    let atom_index: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a)).collect();
    if unit.members.iter().any(|id| {
        atom_index
            .get(id)
            .is_none_or(|a| a.attachment.is_some() || a.stereo.is_some())
    }) {
        return Err("Finite polymer expansion requires an ordinary repeat unit without specified atom stereochemistry".into());
    }
    let head_bond = doc
        .bonds
        .iter()
        .find(|b| pair(b.a, b.b) == pair(head.inside, head.outside))
        .ok_or("Missing head crossing bond")?;
    let tail_bond = doc
        .bonds
        .iter()
        .find(|b| pair(b.a, b.b) == pair(tail.inside, tail.outside))
        .ok_or("Missing tail crossing bond")?;
    if head_bond.order != 1 || tail_bond.order != 1 {
        return Err("Finite polymer expansion requires single crossing bonds".into());
    }
    let mut affected = unit.members.clone();
    affected.extend(unit.crossings.iter().map(|c| c.outside));
    reject_affected_stereo(doc, &affected)?;
    // Overlapping/nested definitions and disconnected unit graphs cannot infer
    // a head/tail chain from the bracket's appearance.
    let reached = component(doc, head.inside, &members);
    if reached.len() != members.len() {
        return Err("Repeating-unit atoms must form one connected fragment".into());
    }
    let mut result = doc.clone();
    if let Some(id) = unit.bracket {
        result.graphics.retain(|g| g.id != id);
        result.groups.retain(|g| !g.members.contains(&id));
    }
    result
        .graphics
        .retain(|g| !unit.bracket_objects.contains(&g.id));
    result
        .groups
        .retain(|g| !g.members.iter().any(|id| unit.bracket_objects.contains(id)));
    if count == 1 {
        return Ok(result);
    }
    let head_pos = doc.atom(head.inside).ok_or("Missing head atom")?.position;
    let end_pos = doc.atom(tail.outside).ok_or("Missing tail atom")?.position;
    let shift = Point::new(end_pos.x - head_pos.x, end_pos.y - head_pos.y);
    if shift.distance(Point::default()) < 0.01 {
        return Err("The repeating unit has no usable head-to-tail drawing direction".into());
    }
    result
        .bonds
        .retain(|b| pair(b.a, b.b) != pair(tail.inside, tail.outside));
    let mut previous = tail.inside;
    let extra_atoms = unit
        .members
        .len()
        .checked_mul(count.saturating_sub(1) as usize)
        .ok_or("Generic atom count overflow")?;
    let mut allocator = Ids::new(doc, extra_atoms)?;
    let mut atoms = Vec::with_capacity(extra_atoms);
    let internal: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| members.contains(&b.a) && members.contains(&b.b))
        .collect();
    let mut bonds = Vec::with_capacity((internal.len() + 1).saturating_mul(count as usize));
    for repeat_index in 1..count {
        let mut ids = HashMap::new();
        for &old in &unit.members {
            let mut a = (**atom_index.get(&old).ok_or("Missing repeat member")?).clone();
            a.id = allocator.take()?;
            a.position = a
                .position
                .offset(shift.x * repeat_index as f32, shift.y * repeat_index as f32);
            a.cip_label = None;
            a.stereo = None;
            ids.insert(old, a.id);
            atoms.push(a);
        }
        for b in &internal {
            let mut b = (**b).clone();
            b.a = *ids.get(&b.a).ok_or("Missing repeat bond")?;
            b.b = *ids.get(&b.b).ok_or("Missing repeat bond")?;
            b.stereo = None;
            b.stereo_atoms.clear();
            b.cip_label = None;
            bonds.push(b);
        }
        let first = *ids.get(&head.inside).ok_or("Missing repeat head")?;
        bonds.push(ordinary_bond(previous, first));
        previous = *ids.get(&tail.inside).ok_or("Missing repeat tail")?;
    }
    result.atoms.extend(atoms);
    result.bonds.extend(bonds);
    let outside: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| !members.contains(&a.id))
        .map(|a| a.id)
        .collect();
    let tail_component = component(doc, tail.outside, &outside);
    if !tail_component.contains(&head.outside) {
        for a in &mut result.atoms {
            if tail_component.contains(&a.id) {
                a.position = a
                    .position
                    .offset(shift.x * (count - 1) as f32, shift.y * (count - 1) as f32);
            }
        }
    }
    let mut end = tail_bond.clone();
    if end.a == tail.inside {
        end.a = previous
    } else {
        end.b = previous
    }
    result.bonds.push(end);
    clear_after_expansion(&mut result);
    Ok(result)
}
fn component(doc: &Document, start: u64, allowed: &HashSet<u64>) -> HashSet<u64> {
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for b in &doc.bonds {
        if allowed.contains(&b.a) && allowed.contains(&b.b) {
            adjacent.entry(b.a).or_default().push(b.b);
            adjacent.entry(b.b).or_default().push(b.a);
        }
    }
    let mut pending = vec![start];
    let mut found = HashSet::new();
    while let Some(id) = pending.pop() {
        if allowed.contains(&id) && found.insert(id) {
            pending.extend(adjacent.get(&id).into_iter().flatten().copied());
        }
    }
    found
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;
