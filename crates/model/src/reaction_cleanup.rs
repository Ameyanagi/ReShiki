//! Clean up reaction: lays each row of a scheme out along its straight arrows.
//!
//! A row is one straight arrow, or several pointing the same way on one line.
//! Its molecules and `+` signs are centred on the arrow axis and spaced
//! evenly, every arrow takes the same length, and text or reagents above or
//! below an arrow move with it, centred on it.
use crate::document::{Document, Point};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

/// Space between neighbouring items on an axis, in bond lengths.
const GAP: f32 = 0.5;
/// Moves shorter than this are rounding, so a tidy scheme stays unchanged.
const EPSILON: f32 = 1e-3;
/// The widest gap along a row between a reaction's items, in bond lengths.
const REACH: f32 = 4.0;
/// How far a molecule or sign may sit off the axis and still be in the row,
/// in bond lengths.
const BAND: f32 = 1.0;

/// Where a unit goes in a row.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    /// On the axis, spaced with the arrows.
    Axis,
    /// Above or below this arrow, centred on it.
    On(u64),
    /// A caption just above or below this axis unit, moving with it.
    With(usize),
}

/// A unit measured in a row's frame: `along` the arrows and `across` them.
#[derive(Clone, Copy)]
struct Extent {
    along: (f32, f32),
    across: (f32, f32),
}
impl Extent {
    fn mid(range: (f32, f32)) -> f32 {
        (range.0 + range.1) / 2.
    }
}

struct Row {
    origin: Point,
    dir: Point,
    arrows: Vec<u64>,
}
impl Row {
    fn normal(&self) -> Point {
        Point::new(-self.dir.y, self.dir.x)
    }
    fn along(&self, p: Point) -> f32 {
        (p.x - self.origin.x) * self.dir.x + (p.y - self.origin.y) * self.dir.y
    }
    fn across(&self, p: Point) -> f32 {
        let n = self.normal();
        (p.x - self.origin.x) * n.x + (p.y - self.origin.y) * n.y
    }
    fn at(&self, along: f32, across: f32) -> Point {
        let n = self.normal();
        self.origin.offset(
            self.dir.x * along + n.x * across,
            self.dir.y * along + n.y * across,
        )
    }
    fn delta(&self, along: f32, across: f32) -> (f32, f32) {
        let n = self.normal();
        (
            self.dir.x * along + n.x * across,
            self.dir.y * along + n.y * across,
        )
    }
}

/// The drawing with each reaction row in `selection` cleaned up, or the whole
/// drawing's when nothing is selected. With `style_bond_length`, molecules
/// are first scaled to the drawing style's bond length.
pub fn clean(
    doc: &Document,
    selection: &[u64],
    style_bond_length: bool,
) -> Result<Document, String> {
    let bond = doc.drawing_style.bond_length_world;
    let scope = scope(doc, selection);
    let units = crate::editing::groups(doc, &scope);
    let atoms: HashSet<u64> = doc.atoms.iter().map(|a| a.id).collect();
    let straight: HashMap<u64, usize> = units
        .iter()
        .enumerate()
        .filter(|(_, unit)| !unit.iter().any(|id| atoms.contains(id)))
        .filter_map(|(i, unit)| {
            let mut arrows = doc.arrows.iter().filter(|a| unit.contains(&a.id));
            let arrow = arrows.next()?;
            (arrows.next().is_none()
                && crate::arrow_labels::supports(arrow)
                && arrow.start.distance(arrow.end) > 1e-3)
                .then_some((arrow.id, i))
        })
        .collect();
    if straight.is_empty() {
        return Err("Select a reaction with a straight arrow to clean up".into());
    }
    let molecule = |unit: &[u64]| unit.iter().any(|id| atoms.contains(id));
    // Molecules and `+` signs can sit on an axis; other text only beside an arrow.
    let sign = |unit: &[u64]| {
        let [id] = unit else { return false };
        doc.annotations
            .iter()
            .any(|a| a.id == *id && a.text.trim() == "+")
            || doc.graphics.iter().any(|g| {
                g.id == *id
                    && g.kind
                        == crate::graphics::GraphicKind::Symbol(crate::scientific::SymbolKind::Plus)
            })
    };
    let text = |unit: &[u64]| {
        unit.iter()
            .all(|id| doc.annotations.iter().any(|a| a.id == *id))
    };
    // A molecule is measured by its own ink, so a name under it does not
    // pull it off the axis; the name moves with it.
    let cores: Vec<Vec<u64>> = units
        .iter()
        .map(|unit| {
            let core: Vec<u64> = unit
                .iter()
                .copied()
                .filter(|id| atoms.contains(id))
                .collect();
            if core.is_empty() { unit.clone() } else { core }
        })
        .collect();
    let owner: HashMap<u64, usize> = units
        .iter()
        .enumerate()
        .flat_map(|(i, unit)| unit.iter().map(move |id| (*id, i)))
        .collect();
    let shortest = bond * 3.2;
    // Where each unit goes, per row, from the drawing as it was.
    let planned: Vec<(Row, HashMap<usize, Place>)> = rows(doc, &straight)
        .into_iter()
        .map(|row| {
            let measured = measure(doc, &row, &units, &cores);
            // An arrow's reach once it has at least the standard length.
            let span = |arrow: u64| {
                let a = doc.arrows.iter().find(|a| a.id == arrow)?;
                let mid = row.along(a.start.lerp(a.end, 0.5));
                let half = a.start.distance(a.end).max(shortest) / 2.;
                Some((mid - half, mid + half))
            };
            // Recorded roles first, then position.
            let mut places: HashMap<usize, Place> = HashMap::new();
            let mut recorded = HashSet::new();
            for reaction in doc
                .reactions
                .iter()
                .filter(|r| row.arrows.contains(&r.arrow))
            {
                for (role, place) in [
                    (crate::reactions::Role::Reactant, Place::Axis),
                    (crate::reactions::Role::Product, Place::Axis),
                    (crate::reactions::Role::Agent, Place::On(reaction.arrow)),
                ] {
                    for participant in reaction.participants(role) {
                        for i in participant.atoms.iter().filter_map(|id| owner.get(id)) {
                            places.entry(*i).or_insert(place);
                            recorded.insert(*i);
                        }
                    }
                }
            }
            for (i, unit) in units.iter().enumerate() {
                if places.contains_key(&i) || straight.values().any(|a| *a == i) {
                    continue;
                }
                let Some(extent) = measured.get(&i).copied() else {
                    continue;
                };
                let centre = Extent::mid(extent.along);
                // Beside an arrow means clear of its line, near its middle.
                let clear = extent.across.0 > 0. || extent.across.1 < 0.;
                let beside = row.arrows.iter().copied().find(|arrow| {
                    clear
                        && !sign(unit)
                        && span(*arrow).is_some_and(|(s, e)| (s..=e).contains(&centre))
                        && Extent::mid(extent.across).abs() <= bond * 4.
                });
                if let Some(arrow) = beside.filter(|_| molecule(unit) || text(unit)) {
                    places.insert(i, Place::On(arrow));
                } else if (molecule(unit) || sign(unit))
                    && extent.across.0 <= bond * BAND
                    && extent.across.1 >= -bond * BAND
                {
                    places.insert(i, Place::Axis);
                }
            }
            // Molecules found by position belong to the row only within reach
            // of its arrows: a gap wider than REACH ends the scheme.
            let mut line: Vec<((f32, f32), Option<usize>)> = row
                .arrows
                .iter()
                .filter_map(|arrow| Some((span(*arrow)?, None)))
                .chain(
                    places
                        .iter()
                        .filter(|(i, place)| **place == Place::Axis && !recorded.contains(*i))
                        .filter_map(|(i, _)| Some((measured.get(i)?.along, Some(*i)))),
                )
                .collect();
            line.sort_by(|a, b| a.0.0.total_cmp(&b.0.0));
            let mut cluster: Vec<usize> = vec![];
            let (mut end, mut arrow) = (f32::NEG_INFINITY, false);
            for (range, item) in line
                .into_iter()
                .chain([((f32::INFINITY, f32::INFINITY), None)])
            {
                if range.0 - end > bond * REACH {
                    if !arrow {
                        for i in cluster.drain(..) {
                            places.remove(&i);
                        }
                    }
                    (cluster, arrow) = (vec![], false);
                }
                end = end.max(range.1);
                match item {
                    Some(i) => cluster.push(i),
                    None => arrow = true,
                }
            }
            // Captions such as compound numbers follow the molecule they label.
            for (i, unit) in units.iter().enumerate() {
                let Some(extent) = measured.get(&i) else {
                    continue;
                };
                if places.contains_key(&i) || !text(unit) || sign(unit) {
                    continue;
                }
                let centre = Extent::mid(extent.along);
                let owner = places.iter().find_map(|(j, place)| {
                    let molecule = measured.get(j)?;
                    let clear = (extent.across.0 - molecule.across.1)
                        .max(molecule.across.0 - extent.across.1);
                    (*place == Place::Axis
                        && (molecule.along.0..=molecule.along.1).contains(&centre)
                        && clear <= bond * 1.5)
                        .then_some(*j)
                });
                if let Some(j) = owner {
                    places.insert(i, Place::With(j));
                }
            }
            (row, places)
        })
        .collect();
    // An arrow alone, with nothing on its line, is left as it is.
    let planned: Vec<_> = planned
        .into_iter()
        .filter(|(_, places)| places.values().any(|place| *place == Place::Axis))
        .collect();
    if planned.is_empty() {
        return Err("Select a reaction's molecules with its arrow to clean it up".into());
    }
    let mut out = doc.clone();
    if style_bond_length {
        let members: HashSet<usize> = planned
            .iter()
            .flat_map(|(_, p)| p.keys().copied())
            .collect();
        for unit in units
            .iter()
            .enumerate()
            .filter(|(i, unit)| members.contains(i) && molecule(unit))
            .map(|(_, unit)| unit)
        {
            scale_to(&mut out, unit, bond);
        }
    }
    // One length for every arrow: the longest any needs for the text or
    // reagents on one side of it, with the journal spacing labelled arrows follow.
    let widest = planned
        .iter()
        .flat_map(|(row, places)| {
            let measured = measure(&out, row, &units, &cores);
            row.arrows
                .iter()
                .flat_map(|arrow| {
                    let unit = straight.get(arrow).and_then(|i| units.get(*i));
                    let labels: Vec<u64> = unit
                        .into_iter()
                        .flatten()
                        .copied()
                        .filter(|id| id != arrow)
                        .collect();
                    let labels =
                        measure(&out, row, std::slice::from_ref(&labels), &[labels]).remove(&0);
                    sides(&measured, places, *arrow)
                        .into_iter()
                        .flatten()
                        .chain(labels)
                        .map(|e| e.along.1 - e.along.0)
                })
                .collect::<Vec<_>>()
        })
        .fold(0., f32::max);
    let length = widest.max(bond * 2.8) + bond * 0.4;
    let mut placed = HashSet::new();
    for (row, places) in &planned {
        let measured = measure(&out, row, &units, &cores);
        lay_out(
            &mut out,
            row,
            &units,
            &straight,
            &measured,
            places,
            &mut placed,
            length,
            bond,
        );
    }
    crate::arrow_labels::reconcile(&mut out, doc);
    Ok(out)
}

/// The combined extent of the units beside `arrow`, above it and below it.
fn sides(
    measured: &HashMap<usize, Extent>,
    places: &HashMap<usize, Place>,
    arrow: u64,
) -> [Option<Extent>; 2] {
    let mut sides = [None::<Extent>; 2];
    for (i, place) in places {
        let Some(extent) = measured.get(i).filter(|_| *place == Place::On(arrow)) else {
            continue;
        };
        let side = usize::from(Extent::mid(extent.across) > 0.);
        if let Some(slot) = sides.get_mut(side) {
            *slot = Some(match *slot {
                None => *extent,
                Some(e) => Extent {
                    along: (e.along.0.min(extent.along.0), e.along.1.max(extent.along.1)),
                    across: (
                        e.across.0.min(extent.across.0),
                        e.across.1.max(extent.across.1),
                    ),
                },
            });
        }
    }
    sides
}

/// The selection with whole molecules, their names and the labels of selected arrows.
fn scope(doc: &Document, selection: &[u64]) -> Vec<u64> {
    if selection.is_empty() {
        return doc.all_ids();
    }
    let mut ids: Vec<u64> = selection.to_vec();
    // A selected arrow brings its recorded reaction.
    for reaction in doc
        .reactions
        .iter()
        .filter(|r| selection.contains(&r.arrow))
    {
        ids.extend(reaction.ids());
    }
    ids.extend(
        crate::reactions::molecules(doc, &ids.clone())
            .into_iter()
            .flatten(),
    );
    let atoms: HashSet<u64> = ids.iter().copied().collect();
    ids.extend(
        doc.molecule_names
            .iter()
            .filter(|name| name.atoms.iter().any(|id| atoms.contains(id)))
            .map(|name| name.annotation),
    );
    let mut ids = crate::arrow_labels::with_labels(doc, &ids);
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Straight arrows grouped into rows: on one line, pointing either way.
fn rows(doc: &Document, straight: &HashMap<u64, usize>) -> Vec<Row> {
    let bond = doc.drawing_style.bond_length_world;
    let mut rows: Vec<Row> = vec![];
    for arrow in doc.arrows.iter().filter(|a| straight.contains_key(&a.id)) {
        let len = arrow.start.distance(arrow.end);
        let dir = Point::new(
            (arrow.end.x - arrow.start.x) / len,
            (arrow.end.y - arrow.start.y) / len,
        );
        let mid = arrow.start.lerp(arrow.end, 0.5);
        match rows.iter_mut().find(|row| {
            (row.dir.x * dir.x + row.dir.y * dir.y).abs() > 3f32.to_radians().cos()
                && row.across(mid).abs() < bond * BAND
        }) {
            Some(row) => row.arrows.push(arrow.id),
            None => rows.push(Row {
                origin: mid,
                dir,
                arrows: vec![arrow.id],
            }),
        }
    }
    rows
}

/// Each unit measured in the row's frame, by unit index: along the row with
/// everything that moves with it, across it by its core (a molecule's own ink).
fn measure(
    doc: &Document,
    row: &Row,
    units: &[Vec<u64>],
    cores: &[Vec<u64>],
) -> HashMap<usize, Extent> {
    let degrees = row.dir.y.atan2(row.dir.x).to_degrees();
    let frame = |selections| {
        crate::scene::selections_bounds_in_axes(doc, selections, Some((row.origin, degrees)))
    };
    frame(units)
        .into_iter()
        .zip(frame(cores))
        .enumerate()
        .filter_map(|(i, (whole, core))| {
            let ((lo, hi), (core_lo, core_hi)) = (whole?, core?);
            Some((
                i,
                Extent {
                    along: (lo.x - row.origin.x, hi.x - row.origin.x),
                    across: (core_lo.y - row.origin.y, core_hi.y - row.origin.y),
                },
            ))
        })
        .collect()
}

/// Scale a molecule about its centre so its median bond is `bond`.
fn scale_to(doc: &mut Document, unit: &[u64], bond: f32) {
    let position: HashMap<u64, Point> = doc
        .atoms
        .iter()
        .filter(|a| unit.contains(&a.id))
        .map(|a| (a.id, a.position))
        .collect();
    let mut lengths: Vec<f32> = doc
        .bonds
        .iter()
        .filter_map(|b| Some(position.get(&b.a)?.distance(*position.get(&b.b)?)))
        .filter(|l| *l > 1e-3)
        .collect();
    lengths.sort_by(f32::total_cmp);
    // The same median as structure Clean up: the mean of the middle pair.
    let middle = lengths.len() / 2;
    let median = match (
        lengths.len() % 2,
        lengths.get(middle.wrapping_sub(1)),
        lengths.get(middle),
    ) {
        (1, _, Some(m)) => *m,
        (_, Some(a), Some(b)) => (a + b) / 2.,
        _ => return,
    };
    let atoms: Vec<u64> = position.keys().copied().collect();
    let centre = crate::editing::center(doc, &atoms);
    // A molecule already at the length stays exactly as it is.
    if (bond / median - 1.).abs() > 1e-4 {
        crate::editing::transform_about(doc, &atoms, centre, bond / median, 0.);
    }
}

#[allow(clippy::too_many_arguments)]
fn lay_out(
    doc: &mut Document,
    row: &Row,
    units: &[Vec<u64>],
    straight: &HashMap<u64, usize>,
    measured: &HashMap<usize, Extent>,
    places: &HashMap<usize, Place>,
    placed: &mut HashSet<usize>,
    length: f32,
    bond: f32,
) {
    let gap = bond * GAP;
    // The axis items and arrows in reading order: unit, extent and arrow.
    let mut items: Vec<(usize, Extent, Option<u64>)> = places
        .iter()
        .filter(|(_, place)| **place == Place::Axis)
        .filter_map(|(i, _)| Some((*i, *measured.get(i)?, None)))
        .chain(row.arrows.iter().filter_map(|arrow| {
            let a = doc.arrows.iter().find(|a| a.id == *arrow)?;
            let (s, e) = (row.along(a.start), row.along(a.end));
            let across = row.across(a.start.lerp(a.end, 0.5));
            let extent = Extent {
                along: (s.min(e), s.max(e)),
                across: (across, across),
            };
            Some((*straight.get(arrow)?, extent, Some(*arrow)))
        }))
        .collect();
    items.sort_by(|a, b| Extent::mid(a.1.along).total_cmp(&Extent::mid(b.1.along)));
    // A unit an earlier row placed stays put; otherwise the first arrow does.
    let Some(anchor) = items
        .iter()
        .position(|(i, ..)| placed.contains(i))
        .or_else(|| items.iter().position(|item| item.2.is_some()))
    else {
        return;
    };
    let Some(&(_, anchor_extent, anchor_arrow)) = items.get(anchor) else {
        return;
    };
    let axis = Extent::mid(anchor_extent.across);
    let size = |(_, extent, arrow): &(usize, Extent, Option<u64>)| match arrow {
        Some(_) => length,
        None => extent.along.1 - extent.along.0,
    };
    // Each item's new start along the axis, walking out from the anchor.
    let first = match anchor_arrow {
        Some(_) => Extent::mid(anchor_extent.along) - length / 2.,
        None => anchor_extent.along.0,
    };
    let mut starts: HashMap<usize, f32> = HashMap::from([(anchor, first)]);
    let mut cursor = first + size(&(0, anchor_extent, anchor_arrow));
    for (k, item) in items.iter().enumerate().skip(anchor + 1) {
        starts.insert(k, cursor + gap);
        cursor += gap + size(item);
    }
    let mut cursor = first;
    for (k, item) in items.iter().enumerate().take(anchor).rev() {
        cursor -= gap + size(item);
        starts.insert(k, cursor);
    }
    let mut moves: Vec<(Vec<u64>, f32, f32)> = vec![];
    let mut arrows: Vec<(u64, f32)> = vec![];
    for (k, (i, extent, arrow)) in items.iter().enumerate() {
        let (Some(start), Some(unit)) = (starts.get(&k).copied(), units.get(*i)) else {
            continue;
        };
        if !placed.insert(*i) {
            continue;
        }
        let across = axis - Extent::mid(extent.across);
        let Some(arrow) = arrow else {
            let (dx, dy) = row.delta(start - extent.along.0, across);
            moves.push((unit.clone(), dx, dy));
            for (j, place) in places {
                if *place == Place::With(*i)
                    && let Some(caption) = units.get(*j)
                    && placed.insert(*j)
                {
                    moves.push((caption.clone(), dx, dy));
                }
            }
            continue;
        };
        // The arrow and its labels move with its middle. What stands on each
        // side moves as one block, centred on the new middle.
        let middle = start + length / 2.;
        let (dx, dy) = row.delta(middle - Extent::mid(extent.along), across);
        moves.push((unit.clone(), dx, dy));
        arrows.push((*arrow, start));
        let sides = sides(measured, places, *arrow);
        for (j, place) in places {
            if *place == Place::On(*arrow)
                && let (Some(beside), Some(unit)) = (measured.get(j), units.get(*j))
                && let Some(block) = sides
                    .get(usize::from(Extent::mid(beside.across) > 0.))
                    .copied()
                    .flatten()
                && placed.insert(*j)
            {
                let (dx, dy) = row.delta(middle - Extent::mid(block.along), across);
                moves.push((unit.clone(), dx, dy));
            }
        }
    }
    moves.retain(|(_, dx, dy)| dx.abs() > EPSILON || dy.abs() > EPSILON);
    doc.translate_units(&moves);
    // Then each arrow takes the common length about its new middle.
    for (arrow, start) in arrows {
        let Some(a) = doc.arrows.iter_mut().find(|a| a.id == arrow) else {
            continue;
        };
        // An arrow pointing back along the row keeps its direction.
        let (mut start, mut end) = (row.at(start, axis), row.at(start + length, axis));
        if row.along(a.end) < row.along(a.start) {
            std::mem::swap(&mut start, &mut end);
        }
        if a.start.distance(start) > EPSILON || a.end.distance(end) > EPSILON {
            (a.start, a.end) = (start, end);
            // A straightened arrow keeps its control on the shaft.
            if a.control.is_some() {
                a.control = Some(start.lerp(end, 0.5));
            }
        }
    }
}
