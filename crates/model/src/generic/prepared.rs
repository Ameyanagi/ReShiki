//! Prepaid worker input and sealed, measured expansion transport.
use crate::document::{Document, Point};
use std::sync::{Arc, Mutex};

#[cfg(test)]
use super::expansion::expand_drawing_inner;
use super::{
    Limits,
    budget::{Work, drawing_size},
    expansion::{DrawingBatch, expand_drawing},
    layout,
    probe::{self, Stage},
};

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
    result.version = crate::document::VERSION;
    result.clear_content();
    let columns = (count as f32).sqrt().ceil().min(5.) as usize;
    let mut x = 0_f32;
    let mut y = 0_f32;
    let mut row_height = 0_f32;
    let mut grid: Option<(Point, Point)> = None;
    for (i, tile) in tiles.into_iter().enumerate() {
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
                &tile.hydrogens,
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
        data: Arc::new(PreparedData {
            document: Mutex::new(Some(result)),
            count,
            bounds,
        }),
    })
}
