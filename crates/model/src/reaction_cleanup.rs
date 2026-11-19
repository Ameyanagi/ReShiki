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
    let mut out = doc.clone();
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
    if style_bond_length {
        for unit in units.iter().filter(|unit| molecule(unit)) {
            scale_to(&mut out, unit, bond);
        }
    }
    // Molecules and `+` signs can sit on an axis; other text only beside an arrow.
    let sign = |unit: &[u64]| {
        let [id] = unit else { return false };
        doc.annotations
            .iter()
            .any(|a| a.id == *id && a.text.trim() == "+")
    };
    let text = |unit: &[u64]| {
        unit.iter()
            .all(|id| doc.annotations.iter().any(|a| a.id == *id))
    };
    let owner: HashMap<u64, usize> = units
        .iter()
        .enumerate()
        .flat_map(|(i, unit)| unit.iter().map(move |id| (*id, i)))
        .collect();
    // Where each unit goes, per row, from the drawing as it was.
    let planned: Vec<(Row, HashMap<usize, Place>)> = rows(doc, &straight)
        .into_iter()
        .map(|row| {
            let measured = measure(&out, &row, &units);
            let span = |arrow: u64| {
                let a = out.arrows.iter().find(|a| a.id == arrow)?;
                Some((row.along(a.start), row.along(a.end)))
            };
            // Recorded roles first, then position.
            let mut places: HashMap<usize, Place> = HashMap::new();
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
                        if let Some(i) = participant.atoms.first().and_then(|id| owner.get(id)) {
                            places.entry(*i).or_insert(place);
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
                let beside = row.arrows.iter().copied().find(|arrow| {
                    span(*arrow).is_some_and(|(s, e)| (s..=e).contains(&centre))
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
            (row, places)
        })
        .collect();
    // One length for every arrow: the longest any needs for the text or
    // reagents beside it, with the journal spacing labelled arrows follow.
    let widest = planned
        .iter()
        .flat_map(|(row, places)| {
            let measured = measure(&out, row, &units);
            let labels = row.arrows.iter().filter_map(|arrow| {
                let unit = units.get(*straight.get(arrow)?)?;
                let texts: Vec<u64> = unit.iter().copied().filter(|id| *id != *arrow).collect();
                let extent = measure(&out, row, &[texts]).remove(&0)?;
                Some(extent.along.1 - extent.along.0)
            });
            places
                .iter()
                .filter(|(_, place)| matches!(place, Place::On(_)))
                .filter_map(move |(i, _)| measured.get(i).map(|e| e.along.1 - e.along.0))
                .chain(labels)
                .collect::<Vec<_>>()
        })
        .fold(0., f32::max);
    let length = widest.max(bond * 2.8) + bond * 0.4;
    let mut placed = HashSet::new();
    for (row, places) in &planned {
        let measured = measure(&out, row, &units);
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

/// The selection with whole molecules, their names and the labels of selected arrows.
fn scope(doc: &Document, selection: &[u64]) -> Vec<u64> {
    if selection.is_empty() {
        return doc.all_ids();
    }
    let mut ids: Vec<u64> = selection.to_vec();
    ids.extend(
        crate::reactions::molecules(doc, selection)
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

/// Straight arrows grouped into rows: same direction, on one line.
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
            row.dir.x * dir.x + row.dir.y * dir.y > 3f32.to_radians().cos()
                && row.across(mid).abs() < bond * 0.5
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

/// Each unit's ink measured in the row's frame, by unit index.
fn measure(doc: &Document, row: &Row, units: &[Vec<u64>]) -> HashMap<usize, Extent> {
    let degrees = row.dir.y.atan2(row.dir.x).to_degrees();
    crate::scene::selections_bounds_in_axes(doc, units, Some((row.origin, degrees)))
        .into_iter()
        .enumerate()
        .filter_map(|(i, bounds)| {
            let (lo, hi) = bounds?;
            Some((
                i,
                Extent {
                    along: (lo.x - row.origin.x, hi.x - row.origin.x),
                    across: (lo.y - row.origin.y, hi.y - row.origin.y),
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
    let Some(median) = lengths.get(lengths.len() / 2).copied() else {
        return;
    };
    let atoms: Vec<u64> = position.keys().copied().collect();
    let centre = crate::editing::center(doc, &atoms);
    crate::editing::transform_about(doc, &atoms, centre, bond / median, 0.);
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
            continue;
        };
        // The arrow, its labels and the units beside it move with its middle.
        let middle = start + length / 2.;
        let (dx, dy) = row.delta(middle - Extent::mid(extent.along), across);
        moves.push((unit.clone(), dx, dy));
        arrows.push((*arrow, start));
        for (j, place) in places {
            if *place == Place::On(*arrow)
                && let (Some(beside), Some(unit)) = (measured.get(j), units.get(*j))
                && placed.insert(*j)
            {
                let (dx, dy) = row.delta(middle - Extent::mid(beside.along), across);
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
        // Every arrow in a row points along it.
        let (start, end) = (row.at(start, axis), row.at(start + length, axis));
        if a.start.distance(start) > EPSILON || a.end.distance(end) > EPSILON {
            (a.start, a.end) = (start, end);
            // A straightened arrow keeps its control on the shaft.
            if a.control.is_some() {
                a.control = Some(start.lerp(end, 0.5));
            }
        }
    }
}
