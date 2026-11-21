//! Editable table cells own ordinary drawing objects by ID. Positions stay in
//! world coordinates; the cell frame controls clipping, never chemical data.
use super::{Graphic, GraphicKind, GraphicStyle, LinePattern, PathCommand};
use crate::{
    document::{Document, Point},
    palette::Color,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const MAX_CELLS: usize = 4096;
pub const MAX_MEMBERS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Top,
    Bottom,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Border {
    pub side: Side,
    /// Zero is an explicitly invisible edge, not the default line width.
    pub width_pt: f32,
    pub color: Color,
    #[serde(default)]
    pub pattern: LinePattern,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    /// Stable within this table; reordering cells does not change their IDs.
    pub id: u32,
    /// Rectangle in the table's unit frame. Children retain world positions.
    pub lo: Point,
    pub hi: Point,
    #[serde(default)]
    pub members: Vec<u64>,
    /// Source paint order. Adjacent cells keep both overlapping edges.
    #[serde(default)]
    pub borders: Vec<Border>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    /// Cell order is also paint order, including their border records.
    pub cells: Vec<Cell>,
}

/// A checked affine unit frame, shared by clipping and native interchange.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub origin: Point,
    pub axis_x: Point,
    pub axis_y: Point,
}

impl Frame {
    pub fn of(graphic: &Graphic) -> Option<Self> {
        let frame = Self {
            origin: graphic.origin,
            axis_x: graphic.axis_x,
            axis_y: graphic.axis_y,
        };
        frame.valid().then_some(frame)
    }

    fn valid(self) -> bool {
        let x = self.axis_x.distance(Point::default());
        let y = self.axis_y.distance(Point::default());
        self.origin.is_finite()
            && self.axis_x.is_finite()
            && self.axis_y.is_finite()
            && (0.001..=1_000_000.).contains(&x)
            && (0.001..=1_000_000.).contains(&y)
            && self.determinant().abs() > x * y * 1e-6
    }

    fn determinant(self) -> f32 {
        self.axis_x.x * self.axis_y.y - self.axis_x.y * self.axis_y.x
    }

    /// A normalized local point mapped to world coordinates.
    pub fn point(self, p: Point) -> Point {
        self.origin.offset(
            self.axis_x.x * p.x + self.axis_y.x * p.y,
            self.axis_x.y * p.x + self.axis_y.y * p.y,
        )
    }

    /// A world point in the normalized local frame. No clamping or snapping.
    pub fn local(self, p: Point) -> Option<Point> {
        if !self.valid() || !p.is_finite() {
            return None;
        }
        let (x, y) = (p.x - self.origin.x, p.y - self.origin.y);
        let d = self.determinant();
        let q = Point::new(
            (x * self.axis_y.y - y * self.axis_y.x) / d,
            (y * self.axis_x.x - x * self.axis_x.y) / d,
        );
        q.is_finite().then_some(q)
    }

    pub fn corners(self) -> [Point; 4] {
        [
            Point::new(0., 0.),
            Point::new(1., 0.),
            Point::new(1., 1.),
            Point::new(0., 1.),
        ]
        .map(|p| self.point(p))
    }

    pub fn contains(self, point: Point) -> bool {
        self.local(point)
            .is_some_and(|p| (0. ..=1.).contains(&p.x) && (0. ..=1.).contains(&p.y))
    }
}

impl Cell {
    pub fn frame(&self, graphic: &Graphic) -> Option<Frame> {
        let parent = Frame::of(graphic)?;
        let frame = Frame {
            origin: parent.point(self.lo),
            axis_x: Point::new(
                parent.axis_x.x * (self.hi.x - self.lo.x),
                parent.axis_x.y * (self.hi.x - self.lo.x),
            ),
            axis_y: Point::new(
                parent.axis_y.x * (self.hi.y - self.lo.y),
                parent.axis_y.y * (self.hi.y - self.lo.y),
            ),
        };
        frame.valid().then_some(frame)
    }

    /// Borders remain outside the content clip. Native tables paint vertical
    /// bands before horizontal bands, including their shared corner coverage.
    pub fn border_parts(&self, graphic: &Graphic) -> Vec<crate::scientific::Part> {
        let Some(parent) = Frame::of(graphic).filter(|_| self.frame(graphic).is_some()) else {
            return vec![];
        };
        [Side::Left, Side::Right, Side::Top, Side::Bottom]
            .into_iter()
            .filter_map(|side| self.borders.iter().find(|border| border.side == side))
            .filter(|b| b.width_pt > 0.)
            .filter_map(|border| {
                let (a, b) = match border.side {
                    Side::Left => (self.lo, Point::new(self.lo.x, self.hi.y)),
                    Side::Top => (self.lo, Point::new(self.hi.x, self.lo.y)),
                    Side::Bottom => (Point::new(self.lo.x, self.hi.y), self.hi),
                    Side::Right => (Point::new(self.hi.x, self.lo.y), self.hi),
                };
                let (a, b) = (parent.point(a), parent.point(b));
                // Native solid grid edges end at the intersection, without a
                // round cap. A filled band uses the existing shared vector
                // path everywhere, including SVG and Windows print fallback.
                if border.pattern == LinePattern::Solid {
                    let (dx, dy) = (b.x - a.x, b.y - a.y);
                    let length = dx.hypot(dy);
                    if !length.is_finite() || length <= 0. {
                        return None;
                    }
                    let half = crate::style::DEFAULT.world(border.width_pt) * 0.5;
                    let (ux, uy) = (dx / length, dy / length);
                    let (nx, ny) = (-uy, ux);
                    let (start, end, adjacent) = match border.side {
                        Side::Left | Side::Right => (Side::Top, Side::Bottom, parent.axis_x),
                        Side::Top | Side::Bottom => (Side::Left, Side::Right, parent.axis_y),
                    };
                    let join = |at: Point, side, sign: f32, offset: f32| {
                        let width = self
                            .borders
                            .iter()
                            .find(|b| b.side == side)
                            .map_or(0., |b| crate::style::DEFAULT.world(b.width_pt) * 0.5);
                        if width == 0. {
                            return at.offset(nx * offset, ny * offset);
                        }
                        // Intersect the band's two supporting lines with the
                        // adjacent border's outside edge. This is also a closed
                        // miter on rotated, reflected and sheared RSK frames.
                        let adjacent_length = adjacent.x.hypot(adjacent.y);
                        let (ax, ay) =
                            (-adjacent.y / adjacent_length, adjacent.x / adjacent_length);
                        let across = ax * ux + ay * uy;
                        if !across.is_finite() || across.abs() < 1e-7 {
                            return at.offset(nx * offset, ny * offset);
                        }
                        let along = (sign * width * across.signum() - (ax * nx + ay * ny) * offset)
                            / across;
                        at.offset(nx * offset + ux * along, ny * offset + uy * along)
                    };
                    // An orthogonal band's exact rectangle is also a flat-ended
                    // stroke. Rasterizers retain coverage for very thin strokes
                    // even when both polygon edges quantize to the same scanline.
                    // Keep the polygon for affine miters with slanted end faces.
                    let adjacent_length = adjacent.x.hypot(adjacent.y);
                    let perpendicular =
                        (adjacent.x * ux + adjacent.y * uy).abs() <= adjacent_length * 1e-6;
                    if perpendicular {
                        return Some(crate::scientific::Part {
                            commands: vec![
                                PathCommand::Move(join(a, start, -1., 0.)),
                                PathCommand::Line(join(b, end, 1., 0.)),
                            ],
                            style: GraphicStyle {
                                stroke: border.color,
                                width_pt: border.width_pt,
                                flat_caps: true,
                                ..Default::default()
                            },
                            filled: false,
                        });
                    }
                    return Some(crate::scientific::Part {
                        commands: vec![
                            PathCommand::Move(join(a, start, -1., half)),
                            PathCommand::Line(join(b, end, 1., half)),
                            PathCommand::Line(join(b, end, 1., -half)),
                            PathCommand::Line(join(a, start, -1., -half)),
                            PathCommand::Close,
                        ],
                        style: GraphicStyle {
                            stroke: border.color,
                            fill: Some(border.color),
                            width_pt: 0.,
                            ..Default::default()
                        },
                        filled: true,
                    });
                }
                Some(crate::scientific::Part {
                    commands: vec![PathCommand::Move(a), PathCommand::Line(b)],
                    style: GraphicStyle {
                        stroke: border.color,
                        width_pt: border.width_pt,
                        pattern: border.pattern,
                        ..Default::default()
                    },
                    filled: false,
                })
            })
            .collect()
    }
}

impl Table {
    pub fn cell(&self, id: u32) -> Option<&Cell> {
        self.cells.iter().find(|cell| cell.id == id)
    }
    pub fn cell_mut(&mut self, id: u32) -> Option<&mut Cell> {
        self.cells.iter_mut().find(|cell| cell.id == id)
    }
    pub fn next_cell_id(&self) -> Option<u32> {
        self.cells
            .iter()
            .map(|cell| cell.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.cells.is_empty() || self.cells.len() > MAX_CELLS {
            return Err("A table needs 1–4096 cells".into());
        }
        let mut ids = HashSet::new();
        let mut members = 0usize;
        let mut events = Vec::with_capacity(self.cells.len() * 2);
        let key = |v: f32| if v == 0. { 0 } else { v.to_bits() };
        for (i, cell) in self.cells.iter().enumerate() {
            if cell.id == 0 || !ids.insert(cell.id) {
                return Err("Duplicate or zero table cell ID".into());
            }
            if !cell.lo.is_finite()
                || !cell.hi.is_finite()
                || !(0. ..=1.).contains(&cell.lo.x)
                || !(0. ..=1.).contains(&cell.lo.y)
                || !(0. ..=1.).contains(&cell.hi.x)
                || !(0. ..=1.).contains(&cell.hi.y)
                || cell.lo.x >= cell.hi.x
                || cell.lo.y >= cell.hi.y
            {
                return Err("Table cells need positive rectangles inside the table frame".into());
            }
            members = members.saturating_add(cell.members.len());
            if members > MAX_MEMBERS {
                return Err("Too many table members".into());
            }
            let mut sides = HashSet::new();
            for b in &cell.borders {
                if !sides.insert(b.side)
                    || !b.width_pt.is_finite()
                    || !(0. ..=12.).contains(&b.width_pt)
                {
                    return Err(
                        "Table sides need unique borders with widths from 0 to 12 pt".into(),
                    );
                }
                if b.pattern == LinePattern::Wavy {
                    return Err("Table borders support solid, dashed or dotted strokes".into());
                }
            }
            events.push((key(cell.lo.x), true, i));
            events.push((key(cell.hi.x), false, i));
        }
        // Ends precede starts at the same x. The active y intervals must be
        // disjoint, so each insertion needs only its two neighbors.
        events.sort_unstable();
        let mut active: BTreeMap<u32, f32> = BTreeMap::new();
        for (_, start, i) in events {
            let Some(cell) = self.cells.get(i) else {
                return Err("Invalid table cell".into());
            };
            let y = key(cell.lo.y);
            if !start {
                active.remove(&y);
                continue;
            }
            if active
                .range(..=y)
                .next_back()
                .is_some_and(|(_, &hi)| hi > cell.lo.y)
                || active
                    .range(y..)
                    .next()
                    .is_some_and(|(&lo, _)| f32::from_bits(lo) < cell.hi.y)
            {
                return Err("Table cell interiors cannot overlap".into());
            }
            active.insert(y, cell.hi.y);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellRef {
    pub table: u64,
    pub cell: u32,
}

/// One checked ownership index per render/edit operation, never position-based.
#[derive(Debug, Default)]
pub struct Membership {
    owners: HashMap<u64, CellRef>,
}

impl Membership {
    pub fn new(doc: &Document) -> Result<Self, String> {
        let objects: HashSet<_> = doc.object_ids().collect();
        let tables: HashSet<_> = doc
            .graphics
            .iter()
            .filter(|g| g.table.is_some())
            .map(|g| g.id)
            .collect();
        let mut owners = HashMap::new();
        let mut cells = 0usize;
        for g in &doc.graphics {
            let Some(table) = &g.table else {
                continue;
            };
            cells = cells.saturating_add(table.cells.len());
            if cells > MAX_CELLS {
                return Err("A drawing can hold at most 4096 table cells".into());
            }
            for cell in &table.cells {
                for &id in &cell.members {
                    if !objects.contains(&id) || tables.contains(&id) {
                        return Err(
                            "A table member must be an existing ordinary drawing object".into()
                        );
                    }
                    if owners
                        .insert(
                            id,
                            CellRef {
                                table: g.id,
                                cell: cell.id,
                            },
                        )
                        .is_some()
                    {
                        return Err("A drawing object can belong to only one table cell".into());
                    }
                    if owners.len() > MAX_MEMBERS {
                        return Err("Too many table members".into());
                    }
                }
            }
        }
        Ok(Self { owners })
    }
    pub fn cell_of(&self, id: u64) -> Option<CellRef> {
        self.owners.get(&id).copied()
    }
    pub fn owned_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.owners.keys().copied()
    }

    fn together(&self, ids: impl IntoIterator<Item = u64>) -> bool {
        let mut ids = ids.into_iter();
        let Some(first) = ids.next() else {
            return true;
        };
        let owner = self.cell_of(first);
        ids.all(|id| self.cell_of(id) == owner)
    }
}

pub fn validate_graphic(g: &Graphic) -> Result<(), String> {
    if (g.kind == GraphicKind::Table) != g.table.is_some() {
        return Err("Table data does not match the graphic kind".into());
    }
    let Some(table) = &g.table else {
        return Ok(());
    };
    table.validate()?;
    if Frame::of(g).is_none()
        || table.cells.iter().any(|cell| cell.frame(g).is_none())
        || g.depth != [0.; 3]
    {
        return Err("Table cells need finite, nonsingular planar frames".into());
    }
    if g.style.fill.is_some() || !g.style.effects.is_plain() {
        return Err("Table appearance is defined by its cell borders".into());
    }
    Ok(())
}

pub fn validate(doc: &Document) -> Result<(), String> {
    if !doc.graphics.iter().any(|g| g.table.is_some()) {
        return Ok(());
    }
    let ownership = Membership::new(doc)?;
    for bond in &doc.bonds {
        require_same(&ownership, [bond.a, bond.b])?;
    }
    for atom in &doc.atoms {
        require_same(
            &ownership,
            std::iter::once(atom.id).chain(atom.centroid.iter().copied()),
        )?;
    }
    for name in &doc.molecule_names {
        require_same(
            &ownership,
            std::iter::once(name.annotation).chain(name.atoms.iter().copied()),
        )?;
    }
    for caption in &doc.property_captions {
        require_same(
            &ownership,
            std::iter::once(caption.annotation).chain(caption.atoms.iter().copied()),
        )?;
    }
    for label in &doc.arrow_labels {
        require_same(&ownership, [label.arrow, label.annotation])?;
    }
    for reaction in &doc.reactions {
        require_same(&ownership, reaction.ids())?;
    }
    for group in &doc.abbreviations {
        require_same(&ownership, group.members.iter().copied())?;
    }
    for fill in &doc.ring_fills {
        require_same(&ownership, fill.atoms.iter().copied())?;
    }
    for repeat in &doc.generic.repeats {
        require_same(
            &ownership,
            repeat
                .members
                .iter()
                .copied()
                .chain(repeat.bracket)
                .chain(repeat.bracket_objects.iter().copied())
                .chain(repeat.crossing_brackets.iter().copied())
                .chain(
                    repeat
                        .crossings
                        .iter()
                        .flat_map(|crossing| [crossing.inside, crossing.outside]),
                ),
        )?;
    }
    for model in &doc.models {
        require_same(&ownership, model.linked_ids(doc))?;
    }
    for scope in &doc.depth_appearance {
        require_same(&ownership, scope.atoms.iter().copied())?;
    }
    for arrow in &doc.arrows {
        for anchor in arrow.start_anchor.iter().chain(&arrow.end_anchor) {
            use crate::arrow_anchors::Target;
            match anchor.target {
                Target::Atom { atom } | Target::LonePair { atom, .. } => {
                    require_same(&ownership, [arrow.id, atom])?
                }
                Target::Bond { a, b, .. } => require_same(&ownership, [arrow.id, a, b])?,
            }
        }
    }
    for g in &doc.graphics {
        if let Some(text) = g.text {
            require_same(&ownership, [g.id, text])?;
        }
        if let Some(cycle) = &g.cycle {
            require_same(
                &ownership,
                std::iter::once(g.id).chain(cycle.parts().flat_map(|(_, ids)| ids.iter().copied())),
            )?;
        }
        if let Some(leader) = &g.leader {
            for target in leader.start.iter().chain(&leader.end) {
                use crate::leaders::Target;
                match *target {
                    Target::Atom { atom } | Target::Molecule { atom } => {
                        require_same(&ownership, [g.id, atom])?
                    }
                    Target::Object { id } => require_same(&ownership, [g.id, id])?,
                    Target::Bond { a, b, .. } => require_same(&ownership, [g.id, a, b])?,
                }
            }
        }
    }
    for seat in &doc.seats {
        require_same(
            &ownership,
            [Some(seat.a.piece), Some(seat.b.piece), seat.clip]
                .into_iter()
                .flatten(),
        )?;
    }
    Ok(())
}

fn require_same(ownership: &Membership, ids: impl IntoIterator<Item = u64>) -> Result<(), String> {
    if ownership.together(ids) {
        Ok(())
    } else {
        Err("Linked chemistry and labels must belong to the same table cell".into())
    }
}

/// Joining cannot silently relink atoms between cells or onto the canvas.
pub(crate) fn require_same_cell(doc: &Document, ids: &[u64]) -> Result<(), String> {
    if !doc.graphics.iter().any(|g| g.table.is_some()) {
        return Ok(());
    }
    require_same(&Membership::new(doc)?, ids.iter().copied())
}

pub(crate) fn remap_join(doc: &mut Document, mapping: &HashMap<u64, u64>) {
    for table in doc.graphics.iter_mut().filter_map(|g| g.table.as_mut()) {
        for cell in &mut table.cells {
            for id in &mut cell.members {
                if let Some(&mapped) = mapping.get(id) {
                    *id = mapped;
                }
            }
            let mut seen = HashSet::new();
            cell.members.retain(|id| seen.insert(*id));
        }
    }
}

/// A newly inserted fragment inherits its explicit chemical attachment's cell.
/// Existing objects never change owners here; moving existing fragments checks
/// their original ownership before using the template attachment pipeline.
pub(crate) fn inherit_attachment(
    before: &Document,
    after: &mut Document,
    inserted: &[u64],
    targets: &[u64],
) -> Result<(), String> {
    if !before.graphics.iter().any(|g| g.table.is_some()) {
        return Ok(());
    }
    let ownership = Membership::new(before)?;
    require_same(&ownership, targets.iter().copied())?;
    let Some(target) = targets.first().and_then(|id| ownership.cell_of(*id)) else {
        return Ok(());
    };
    let existing: HashSet<_> = before.object_ids().collect();
    let retained: HashSet<_> = after.object_ids().collect();
    let added: Vec<_> = inserted
        .iter()
        .copied()
        .filter(|id| !existing.contains(id) && retained.contains(id))
        .collect();
    let cell = after
        .graphics
        .iter_mut()
        .find(|g| g.id == target.table)
        .and_then(|g| g.table.as_mut())
        .and_then(|table| table.cell_mut(target.cell))
        .ok_or("The attachment's table cell is unavailable")?;
    cell.members.extend(added);
    let mut seen = HashSet::new();
    cell.members.retain(|id| seen.insert(*id));
    Ok(())
}

/// Selecting a table includes its contents once; selecting a child alone does
/// not implicitly select the table or its other cells.
pub fn with_contents(doc: &Document, ids: &[u64]) -> Vec<u64> {
    if !doc.graphics.iter().any(|g| g.table.is_some()) {
        return ids.to_vec();
    }
    let mut selected: HashSet<_> = ids.iter().copied().collect();
    for g in &doc.graphics {
        if selected.contains(&g.id)
            && let Some(table) = &g.table
        {
            selected.extend(
                table
                    .cells
                    .iter()
                    .flat_map(|cell| cell.members.iter().copied()),
            );
        }
    }
    doc.object_ids()
        .filter(|id| selected.contains(id))
        .collect()
}

/// The cell grid, including border ink, remains selectable even when empty.
pub fn bounds(graphic: &Graphic) -> Option<(Point, Point)> {
    let frame = Frame::of(graphic)?;
    let table = graphic.table.as_ref()?;
    let corners = frame.corners();
    let mut lo = frame.origin;
    let mut hi = lo;
    for point in corners {
        lo = Point::new(lo.x.min(point.x), lo.y.min(point.y));
        hi = Point::new(hi.x.max(point.x), hi.y.max(point.y));
    }
    for cell in &table.cells {
        for part in cell.border_parts(graphic) {
            let pad = part.style.width() * 0.5;
            for point in part.commands.iter().flat_map(PathCommand::iter_points) {
                lo.x = lo.x.min(point.x - pad);
                lo.y = lo.y.min(point.y - pad);
                hi.x = hi.x.max(point.x + pad);
                hi.y = hi.y.max(point.y + pad);
            }
        }
    }
    Some((lo, hi))
}

pub(crate) fn prune(doc: &mut Document) {
    let existing: HashSet<_> = doc.object_ids().collect();
    for table in doc.graphics.iter_mut().filter_map(|g| g.table.as_mut()) {
        for cell in &mut table.cells {
            cell.members.retain(|id| existing.contains(id));
        }
    }
}

pub(crate) fn remap(g: &mut Graphic, ids: &HashMap<u64, u64>) -> Option<()> {
    if let Some(table) = &mut g.table {
        for id in table.cells.iter_mut().flat_map(|cell| &mut cell.members) {
            *id = *ids.get(id)?;
        }
    }
    Some(())
}

/// Replace a materialized apparatus/cycle/arrow by its ordinary output objects
/// without dropping cell ownership. The materializer validates its final copy.
pub fn replace_member(doc: &mut Document, source: u64, replacements: &[u64]) {
    for table in doc.graphics.iter_mut().filter_map(|g| g.table.as_mut()) {
        for cell in &mut table.cells {
            if let Some(at) = cell.members.iter().position(|id| *id == source) {
                cell.members.splice(at..=at, replacements.iter().copied());
                // Several pieces of one assembly can lower to the same curve
                // set; membership names each resulting object only once.
                let mut seen = HashSet::new();
                cell.members.retain(|id| seen.insert(*id));
            }
        }
    }
}

/// Relink an explicitly selected set without moving or merging any object.
/// Partial molecules/linked captions are refused atomically. None releases the
/// selection onto the ordinary canvas while retaining its world positions.
pub fn relink(doc: &mut Document, destination: Option<CellRef>, ids: &[u64]) -> Result<(), String> {
    doc.validate()?;
    let existing: HashSet<_> = doc.object_ids().collect();
    let selected: HashSet<_> = ids.iter().copied().collect();
    if selected.len() != ids.len() || ids.iter().any(|id| !existing.contains(id)) {
        return Err("Relink existing objects once each".into());
    }
    let mut candidate = doc.clone();
    for table in candidate
        .graphics
        .iter_mut()
        .filter_map(|g| g.table.as_mut())
    {
        for cell in &mut table.cells {
            cell.members.retain(|id| !selected.contains(id));
        }
    }
    if let Some(target) = destination {
        let cell = candidate
            .graphics
            .iter_mut()
            .find(|g| g.id == target.table)
            .and_then(|g| g.table.as_mut())
            .and_then(|table| table.cell_mut(target.cell))
            .ok_or("Choose an existing table cell")?;
        cell.members.extend_from_slice(ids);
    }
    candidate.validate()?;
    *doc = candidate;
    Ok(())
}

#[cfg(test)]
mod paint_tests;
#[cfg(test)]
mod tests;
