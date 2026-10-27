//! Prepaid worker input and sealed, measured expansion transport.
use crate::document::{Document, Point};
use std::sync::{Arc, Mutex};

#[cfg(test)]
use super::expansion::expand_drawing_inner;
use super::{
    Limits,
    budget::{self, Work, drawing_size},
    certify,
    errors::retained,
    expansion::{DrawingBatch, expand_drawing},
    ink,
    probe::{self, Stage},
};

const TILE_GAP: f32 = 84.;
const MAX_GRID_COLUMNS: usize = 5;
const GRID_GUARD: f32 = 1.;
const MAX_PLACEMENT_ATTEMPTS: usize = 3;

/// Cheaply cloned, single-use transport for an already bounded, measured native
/// insertion. No caller can mutate the sealed drawing or construct a proof.
#[derive(Clone)]
pub struct PreparedExpansion {
    pub(super) data: Arc<PreparedData>,
}
pub(super) struct PreparedData {
    pub(super) document: Mutex<Option<Document>>,
    count: usize,
    bounds: (Point, Point),
}
impl std::fmt::Debug for PreparedExpansion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedExpansion")
            .field("count", &self.data.count)
            .field("bounds", &self.data.bounds)
            .finish_non_exhaustive()
    }
}
impl PreparedExpansion {
    pub fn count(&self) -> usize {
        self.data.count
    }
    pub fn bounds(&self) -> (Point, Point) {
        self.data.bounds
    }
    /// Move the sealed document out once; count and bounds stay readable.
    pub(crate) fn take(&self) -> Result<Document, String> {
        self.data
            .document
            .lock()
            .map_err(|_| "Prepared expansion transport is unavailable".to_owned())?
            .take()
            .ok_or_else(|| "Prepared expansion was already consumed".into())
    }
}

/// Owned, prepaid worker input. It is moved into the task and cannot be cloned
/// or constructed by hosts; its original shared work allowance never resets.
pub struct ExpansionInput {
    pub(super) source: Document,
    limits: Limits,
    work: Work,
    presentation: budget::Presentation,
}
/// Borrow the live drawing and selection, reject costs before snapshot/selection
/// copies or scheduling. Selection retains complete dependencies without repair.
pub fn prepare_expansion_input(
    source: &Document,
    selected: &[u64],
    limits: Limits,
) -> Result<ExpansionInput, String> {
    prepare_input(source, selected, limits, false)
}
/// Expand the complete molecular component and native dependencies under a click.
pub fn prepare_component_expansion_input(
    source: &Document,
    hit: &[u64],
    limits: Limits,
) -> Result<ExpansionInput, String> {
    prepare_input(source, hit, limits, true)
}
fn prepare_input(
    source: &Document,
    selected: &[u64],
    limits: Limits,
    connected: bool,
) -> Result<ExpansionInput, String> {
    limits.check(source, Some(selected))?;
    let mut work = Work::new(limits.max_work);
    let scope;
    let selected = if connected {
        scope = super::selection::component(source, selected, &mut work)?;
        &scope
    } else {
        if !selected.is_empty() {
            super::selection::check_molecules(source, selected, &mut work)?;
        }
        selected
    };
    let chosen = if selected.is_empty() || connected {
        selected.to_vec()
    } else {
        // Refuse the repeat and drawing-size metadata passes from O(1) lengths.
        work.charge(
            source
                .generic
                .repeats
                .len()
                .saturating_add(source.groups.len())
                .saturating_add(source.abbreviations.len()),
        )?;
        probe::enter(Stage::RepeatCount);
        let refs = source
            .generic
            .repeats
            .iter()
            .map(|r| {
                r.members
                    .len()
                    .saturating_add(r.crossings.len())
                    .saturating_add(r.bracket_objects.len())
                    .saturating_add(1)
            })
            .fold(0usize, usize::saturating_add);
        work.charge(
            drawing_size(source)
                .saturating_add(refs)
                .saturating_add(selected.len())
                .saturating_mul(source.generic.repeats.len().saturating_add(1))
                .saturating_mul(32),
        )?;
        crate::generic::selection(source, selected)
    };
    let ids: std::collections::HashSet<_> = chosen.iter().copied().collect();
    let whole_drawing = selected.is_empty() || source.object_ids().all(|id| ids.contains(&id));
    let mut presentation = budget::Presentation::inspect(source, &mut work)?;
    presentation.charge_input(
        source,
        if whole_drawing { &[] } else { selected },
        &mut work,
    )?;
    let owned = if whole_drawing {
        probe::enter(Stage::Snapshot);
        source.clone()
    } else {
        // Native validation uses the same remaining allowance before extraction.
        budget::validate(source, &mut work)?;
        probe::enter(Stage::Snapshot);
        let owned = crate::editing::preserved_expansion_selection(source, selected)?;
        presentation = budget::Presentation::inspect(&owned, &mut work)?;
        owned
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
    let batch = expand_drawing(&source, limits, presentation, work)?;
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
    assemble_expansion(source, expand_drawing_inner(source, limits)?)
}
fn assemble_expansion(source: &Document, batch: DrawingBatch) -> Result<PreparedExpansion, String> {
    let DrawingBatch {
        tiles,
        mut work,
        presentation,
    } = batch;
    let count = tiles.len();
    work.charge(drawing_size(source).saturating_mul(32))?;
    let mut result = source.clone();
    result.clear_content();
    result.version = result.file_version();
    let columns = (count as f32).sqrt().ceil().min(MAX_GRID_COLUMNS as f32) as usize;
    let mut x = 0_f32;
    let mut y = 0_f32;
    let mut row_height = 0_f32;
    let mut grid: Option<(Point, Point)> = None;
    for (i, tile) in tiles.into_iter().enumerate() {
        if i > 0 && i % columns == 0 {
            x = 0.;
            y += row_height + TILE_GAP;
            row_height = 0.;
        }
        let (lo, _hi) = tile.bounds;
        // Start with the same one-world-unit guard used by retries below.
        // Exact-boundary translation can round the actual f32 ink just outside
        // the grid and waste a fully prepaid attempt. Measurement still checks
        // the translated tile; this inset does not replace its certificate.
        let mut shift = Point::new(x - lo.x + GRID_GUARD, y - lo.y + GRID_GUARD);
        let mut accepted = None;
        // Supplementary cap: all attempts still debit the SAME Work before
        // baseline deepclone and scene/normalization/glyph calculation.
        for attempt in 0..MAX_PLACEMENT_ATTEMPTS {
            if !shift.is_finite() {
                return Err("Prepared expansion grid is nonfinite".into());
            }
            let total_atoms = result.atoms.len().saturating_add(tile.document.atoms.len());
            let total_size = drawing_size(&result).saturating_add(drawing_size(&tile.document));
            presentation.charge_assembly(
                source,
                budget::AssemblyCounts {
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
            certify::certify_translation(&tile.document, &part, &mapping, shift)?;
            crate::molecule_names::certify_inherited(&tile.document, &part, &mapping, shift)?;
            let bounds = ink::measure_translated(
                &mut part,
                &tile.hydrogens,
                source,
                &presentation,
                &mut work,
            )?;
            budget::validate(&part, &mut work)?;
            if bounds.0.x >= x && bounds.0.y >= y {
                accepted = Some((part, bounds));
                break;
            }
            if attempt == MAX_PLACEMENT_ATTEMPTS - 1 {
                return Err(retained(
                    "Prepared tile cannot reach a stable measured grid position",
                ));
            }
            // Rebuild the original tile next time. A one-world-unit guard
            // prevents a subpixel correction loop; no chemistry/stereo changes.
            shift.x += (x - bounds.0.x).max(0.) + GRID_GUARD;
            shift.y += (y - bounds.0.y).max(0.) + GRID_GUARD;
        }
        let (part, (a, b)) = accepted.ok_or("Missing stable measured expansion tile")?;
        crate::editing::extend_append_part(&mut result, part);
        budget::validate(&result, &mut work)?;
        grid = Some(grid.map_or((a, b), |(lo, hi)| {
            (
                Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                Point::new(hi.x.max(b.x), hi.y.max(b.y)),
            )
        }));
        x = b.x.max(x + TILE_GAP) + TILE_GAP;
        row_height = row_height.max((b.y - y).max(TILE_GAP));
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
        data: Arc::new(PreparedData {
            document: Mutex::new(Some(result)),
            count,
            bounds,
        }),
    })
}
