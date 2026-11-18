//! Lasting rows, columns and grids. A layout belongs to a group: its units
//! (molecules, groups and whole reactions inside it) keep shared gutters when
//! one of them changes size. Units only move, so bond scale, labels and
//! reaction roles stay as drawn.
use crate::document::{Document, Point};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    Row,
    Column,
    /// Rows of this many units, in reading order.
    Grid(u16),
}
impl Flow {
    /// Units per row for `count` units.
    pub fn columns(self, count: usize) -> usize {
        match self {
            Self::Row => count.max(1),
            Self::Column => 1,
            Self::Grid(columns) => usize::from(columns.max(1)),
        }
    }
}
impl std::fmt::Display for Flow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Row => f.write_str("Row"),
            Self::Column => f.write_str("Column"),
            Self::Grid(columns) => write!(f, "Grid of {columns} columns"),
        }
    }
}

/// Where a unit sits in its cell when the row or column is larger than it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Start,
    #[default]
    Center,
    End,
}
impl Align {
    pub const ALL: [Self; 3] = [Self::Start, Self::Center, Self::End];
    fn offset(self, room: f32) -> f32 {
        match self {
            Self::Start => 0.,
            Self::Center => room * 0.5,
            Self::End => room,
        }
    }
}
impl std::fmt::Display for Align {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Start => "Top left",
            Self::Center => "Center",
            Self::End => "Bottom right",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub flow: Flow,
    /// Space between neighbouring units, in points.
    pub gutter_pt: f32,
    #[serde(default)]
    pub align: Align,
}
impl Layout {
    pub fn validate(&self) -> Result<(), String> {
        if !self.gutter_pt.is_finite() || !(0. ..=288.).contains(&self.gutter_pt) {
            return Err("Layout gutters must be 0–288 pt".into());
        }
        if matches!(self.flow, Flow::Grid(c) if !(1..=64).contains(&c)) {
            return Err("A layout grid has 1–64 columns".into());
        }
        Ok(())
    }
}

/// The units a layout group arranges, each moved as one: connected molecules
/// with their captions, nested groups, whole reactions and schemes drawn
/// along an arrow. When that leaves a single unit, as for one scheme laid
/// out in a row, its molecules and arrows are the units instead.
pub fn units(doc: &Document, group: u64) -> Vec<Vec<u64>> {
    let Some(members) = doc
        .groups
        .iter()
        .find(|g| g.id == group)
        .map(|g| &g.members)
    else {
        return vec![];
    };
    let mut inside: HashSet<_> = members.iter().copied().collect();
    // Captions linked to molecules inside move with them, even when they were
    // added after the layout.
    let links: Vec<(&[u64], u64)> = doc
        .molecule_names
        .iter()
        .map(|name| (name.atoms.as_slice(), name.annotation))
        .chain(
            doc.property_captions
                .iter()
                .map(|link| (link.atoms.as_slice(), link.annotation)),
        )
        .collect();
    for (atoms, annotation) in &links {
        if !atoms.is_empty() && atoms.iter().all(|id| inside.contains(id)) {
            inside.insert(*annotation);
        }
    }
    // Only groups nested inside this one split it into units; the group itself
    // and any group around it would make it a single unit.
    let mut view = doc.clone();
    view.groups.retain(|g| {
        g.id != group
            && !members.iter().all(|id| g.members.contains(id))
            && g.members.iter().all(|id| inside.contains(id))
    });
    let members: Vec<u64> = doc
        .all_ids()
        .into_iter()
        .filter(|id| inside.contains(id))
        .collect();
    let units = crate::editing::groups(&view, &members);
    let bounds = crate::scene::selections_bounds(doc, &units);
    let center = |(lo, hi): (Point, Point)| Point::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
    // Each unit's IDs and center.
    type Unit = (Vec<u64>, Option<Point>);
    let mut units: Vec<Unit> = units
        .into_iter()
        .zip(bounds.into_iter().map(|b| b.map(center)))
        .collect();
    // Merge every unit that `joins` picks into one, centered at `at`.
    let merge = |units: Vec<Unit>, joins: &dyn Fn(&Unit) -> bool, at: Option<Point>| {
        let (joined, mut rest): (Vec<_>, Vec<_>) = units.into_iter().partition(|u| joins(u));
        if !joined.is_empty() {
            let at = at.or_else(|| joined.iter().find_map(|u| u.1));
            rest.push((joined.into_iter().flat_map(|u| u.0).collect(), at));
        }
        rest
    };
    for (atoms, annotation) in links {
        if inside.contains(&annotation) {
            units = merge(
                units,
                &|u| u.0.contains(&annotation) || u.0.iter().any(|id| atoms.contains(id)),
                None,
            );
        }
    }
    let mut schemes = units.clone();
    for ids in doc.reactions.iter().map(|r| r.ids()) {
        if ids.iter().all(|id| inside.contains(id)) {
            schemes = merge(schemes, &|u| u.0.iter().any(|id| ids.contains(id)), None);
        }
    }
    // An arrow joins what lies on its line near it: the molecules of its step.
    let reach = crate::style::DEFAULT.bond_length_world;
    for arrow in doc.arrows.iter().filter(|a| inside.contains(&a.id)) {
        let (s, e) = (arrow.start, arrow.end);
        let length = s.distance(e);
        if length == 0. {
            continue;
        }
        let (dx, dy) = ((e.x - s.x) / length, (e.y - s.y) / length);
        let on_step = |c: Point| {
            let along = (c.x - s.x) * dx + (c.y - s.y) * dy;
            let across = ((c.y - s.y) * dx - (c.x - s.x) * dy).abs();
            across < reach && along > -4. * reach && along < length + 4. * reach
        };
        let middle = Point::new((s.x + e.x) * 0.5, (s.y + e.y) * 0.5);
        schemes = merge(
            schemes,
            &|u| u.0.contains(&arrow.id) || u.1.is_some_and(on_step),
            Some(middle),
        );
    }
    if schemes.len() >= 2 {
        units = schemes;
    }
    let mut units: Vec<Vec<u64>> = units.into_iter().map(|u| u.0).collect();
    // Keep drawing order stable for equal positions.
    units.sort_by_key(|unit| unit.iter().min().copied());
    units
}

/// Place a layout group's units in its flow, keeping the top-left corner of
/// their extent. Units are read in the order `previous` lays them out (rows
/// top to bottom, each left to right), so changing the flow keeps the order.
/// Reading uses the units' places in `reference` when given, so a unit that
/// grew past its neighbors keeps its turn.
pub fn reflow(
    doc: &mut Document,
    group: u64,
    previous: Option<Flow>,
    reference: Option<&Document>,
) {
    let Some(layout) = doc
        .groups
        .iter()
        .find(|g| g.id == group)
        .and_then(|g| g.layout)
    else {
        return;
    };
    let units = units(doc, group);
    let bounds = crate::scene::selections_bounds(doc, &units);
    let cells: Vec<(Vec<u64>, (Point, Point))> = units
        .into_iter()
        .zip(bounds)
        .filter_map(|(unit, bounds)| bounds.map(|b| (unit, b)))
        .collect();
    if cells.is_empty() {
        return;
    }
    let center = |(lo, hi): (Point, Point)| Point::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
    let earlier: Vec<Option<(Point, Point)>> = match reference {
        Some(reference) => {
            let known: HashSet<u64> = reference.object_ids().collect();
            let kept: Vec<Vec<u64>> = cells
                .iter()
                .map(|(unit, _)| {
                    unit.iter()
                        .copied()
                        .filter(|id| known.contains(id))
                        .collect()
                })
                .collect();
            crate::scene::selections_bounds(reference, &kept)
        }
        None => vec![None; cells.len()],
    };
    // A unit, its bounds now and where it was before the edit.
    type Cell = (Vec<u64>, (Point, Point), (Point, Point));
    let mut cells: Vec<Cell> = cells
        .into_iter()
        .zip(earlier)
        .map(|((unit, b), earlier)| (unit, b, earlier.unwrap_or(b)))
        .collect();
    // Read in the order the units were laid out: a row left to right, a
    // column top to bottom, a grid by rows (a row ends where a unit's middle
    // falls below the first one's extent).
    match previous.unwrap_or(layout.flow) {
        Flow::Row => cells.sort_by(|a, b| center(a.2).x.total_cmp(&center(b.2).x)),
        Flow::Column => cells.sort_by(|a, b| center(a.2).y.total_cmp(&center(b.2).y)),
        Flow::Grid(_) => {
            cells.sort_by(|a, b| center(a.2).y.total_cmp(&center(b.2).y));
            let mut rows: Vec<Vec<Cell>> = vec![];
            for cell in cells {
                let half = |c: &Cell| (c.2.1.y - c.2.0.y) * 0.5;
                // Items on one line (flat ones too) share a row.
                let below =
                    |first: &Cell| center(cell.2).y - center(first.2).y > half(first).max(1.);
                match rows.last_mut() {
                    Some(row) if row.first().is_some_and(|first| !below(first)) => row.push(cell),
                    _ => rows.push(vec![cell]),
                }
            }
            for row in &mut rows {
                row.sort_by(|a, b| center(a.2).x.total_cmp(&center(b.2).x));
            }
            cells = rows.into_iter().flatten().collect();
        }
    }
    let columns = layout.flow.columns(cells.len());
    let rows = cells.len().div_ceil(columns);
    let size = |(lo, hi): (Point, Point)| (hi.x - lo.x, hi.y - lo.y);
    let mut widths = vec![0f32; columns];
    let mut heights = vec![0f32; rows];
    for (i, (_, b, _)) in cells.iter().enumerate() {
        let (w, h) = size(*b);
        if let Some(width) = widths.get_mut(i % columns) {
            *width = width.max(w);
        }
        if let Some(height) = heights.get_mut(i / columns) {
            *height = height.max(h);
        }
    }
    let gutter = crate::style::DEFAULT.world(layout.gutter_pt);
    let anchor = cells.iter().fold(
        Point::new(f32::INFINITY, f32::INFINITY),
        |p, (_, (lo, _), _)| Point::new(p.x.min(lo.x), p.y.min(lo.y)),
    );
    // Where each column and row starts, after the ones before it and a gutter each.
    let starts = |sizes: &[f32]| {
        sizes
            .iter()
            .scan(0., |at, size| {
                let start = *at;
                *at += size + gutter;
                Some(start)
            })
            .collect::<Vec<f32>>()
    };
    let (lefts, tops) = (starts(&widths), starts(&heights));
    let moves: Vec<_> = cells
        .into_iter()
        .enumerate()
        .map(|(i, (unit, b, _))| {
            let (column, row) = (i % columns, i / columns);
            let (w, h) = size(b);
            let slot = |starts: &[f32], sizes: &[f32], index: usize, size: f32| {
                let start = starts.get(index).copied().unwrap_or_default();
                let room = sizes.get(index).copied().unwrap_or(size) - size;
                start + layout.align.offset(room)
            };
            let x = anchor.x + slot(&lefts, &widths, column, w);
            let y = anchor.y + slot(&tops, &heights, row, h);
            (unit, x - b.0.x, y - b.0.y)
        })
        .collect();
    doc.translate_units(&moves);
    // A panel wrapped around the layout follows its new extent.
    let members = doc
        .groups
        .iter()
        .find(|g| g.id == group)
        .map(|g| g.members.clone())
        .unwrap_or_default();
    // Inner panels first, so an outer one fits around their new frames.
    let mut panels = crate::graphics::panel::around(doc, &members);
    let depth = |doc: &Document, id: u64| {
        doc.groups
            .iter()
            .filter(|g| g.members.contains(&id))
            .count()
    };
    panels.sort_by_key(|id| std::cmp::Reverse(depth(doc, *id)));
    for panel in panels {
        crate::graphics::panel::fit(doc, panel);
    }
}

/// Make the selection a lasting layout: an existing group when the selection
/// is exactly one, otherwise a new group. Returns the group's ID.
pub fn arrange(doc: &mut Document, ids: &[u64], flow: Flow, gutter_pt: f32) -> Result<u64, String> {
    // Whole molecules, but not the groups around the selection: an inner
    // group picked with Option/Alt-click is laid out by itself.
    let mut members = doc.expand_abbreviation_selection(ids);
    loop {
        let count = members.len();
        for b in &doc.bonds {
            if members.contains(&b.a) != members.contains(&b.b) {
                members.push(if members.contains(&b.a) { b.b } else { b.a });
            }
        }
        if count == members.len() {
            break;
        }
    }
    // Linked captions follow their molecules without being members, so a
    // reselected layout still matches its group.
    let linked: HashSet<u64> = doc
        .molecule_names
        .iter()
        .map(|name| (name.atoms.as_slice(), name.annotation))
        .chain(
            doc.property_captions
                .iter()
                .map(|link| (link.atoms.as_slice(), link.annotation)),
        )
        .filter(|(atoms, _)| !atoms.is_empty() && atoms.iter().all(|id| members.contains(id)))
        .map(|(_, annotation)| annotation)
        .collect();
    if doc.groups.iter().any(|g| {
        !g.members.iter().any(|id| linked.contains(id)) && {
            let rest: Vec<u64> = members
                .iter()
                .copied()
                .filter(|id| !linked.contains(id))
                .collect();
            g.members.len() == rest.len() && g.members.iter().all(|id| rest.contains(id))
        }
    }) {
        members.retain(|id| !linked.contains(id));
    }
    let same = |g: &crate::grouping::Group, members: &[u64]| {
        g.members.len() == members.len() && g.members.iter().all(|id| members.contains(id))
    };
    // A panel's whole group lays out what the panel holds, inside it.
    let panels: Vec<u64> = doc
        .graphics
        .iter()
        .filter(|g| g.panel.is_some() && members.contains(&g.id))
        .map(|g| g.id)
        .collect();
    let own_group = |panel: u64| {
        doc.groups
            .iter()
            .filter(|g| g.members.contains(&panel))
            .min_by_key(|g| g.members.len())
    };
    if let [panel] = panels[..]
        && members.len() > 2
        && own_group(panel).is_some_and(|g| same(g, &members))
    {
        members.retain(|id| *id != panel);
    }
    let existing = doc
        .groups
        .iter()
        .find(|g| same(g, &members))
        .map(|g| (g.id, g.layout.map(|l| l.flow)));
    let (group, previous) = match existing {
        Some(found) => found,
        None => {
            let id = doc.next_id();
            doc.groups.push(crate::grouping::Group {
                id,
                members,
                integral: false,
                layout: None,
            });
            (id, None)
        }
    };
    if units(doc, group).len() < 2 {
        if existing.is_none() {
            doc.groups.retain(|g| g.id != group);
        }
        return Err("Select at least two molecules, groups or reactions to arrange".into());
    }
    let Some(entry) = doc.groups.iter_mut().find(|g| g.id == group) else {
        return Err("The layout group was not found".into());
    };
    let align = entry.layout.map(|l| l.align).unwrap_or_default();
    entry.layout = Some(Layout {
        flow,
        gutter_pt,
        align,
    });
    reflow(doc, group, previous, None);
    Ok(group)
}

/// After an edit, reflow each layout whose units changed size or number, so
/// the gutters last. Moving a unit alone leaves the layout as placed.
pub fn reconcile(doc: &mut Document, before: &Document) {
    // Inner layouts first, so an outer one sees their new extent.
    let mut groups: Vec<(usize, u64)> = doc
        .groups
        .iter()
        .filter(|g| g.layout.is_some())
        .map(|g| (g.members.len(), g.id))
        .collect();
    groups.sort_unstable();
    for (_, group) in groups {
        let sizes = |doc: &Document| -> HashMap<Option<u64>, (i32, i32)> {
            let units = units(doc, group);
            let bounds = crate::scene::selections_bounds(doc, &units);
            units
                .iter()
                .zip(bounds)
                .map(|(unit, b)| {
                    let (w, h) = b.map_or((0., 0.), |(lo, hi)| (hi.x - lo.x, hi.y - lo.y));
                    // Hundredths of a world unit ignore rounding noise.
                    (
                        unit.iter().min().copied(),
                        ((w * 100.).round() as i32, (h * 100.).round() as i32),
                    )
                })
                .collect()
        };
        if before.groups.iter().any(|g| g.id == group) && sizes(doc) != sizes(before) {
            reflow(doc, group, None, Some(before));
        }
    }
}

#[cfg(test)]
mod tests;
