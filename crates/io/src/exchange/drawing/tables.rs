//! Native cell pages keep chemistry editable and let ChemDraw clip/reflow it.
use super::*;
use crate::graphics::{
    Graphic, LinePattern,
    table::{CellRef, Membership, Side},
};
use std::collections::{HashMap, HashSet};

/// All cell edges use the same parent-frame cut and native coordinate tick.
/// Reconstructing a cell's high corner from its rounded origin and extent can
/// introduce a one-tick gap. ChemDraw then inserts a column and reparents ink.
struct Grid {
    x: HashMap<u32, f64>,
    y: HashMap<u32, f64>,
}

impl Grid {
    fn key(value: f32) -> u32 {
        if value == 0. { 0 } else { value.to_bits() }
    }

    fn new(graphic: &Graphic, scale: f64, shift: P) -> Result<Self> {
        // Native table pages are axis aligned. Keep the existing allowance for
        // numerical off-axis noise, rather than exporting an affine bounding box.
        if graphic.axis_x.x <= 0.
            || graphic.axis_y.y <= 0.
            || graphic.axis_x.y.abs() > 0.00001
            || graphic.axis_y.x.abs() > 0.00001
        {
            return Err(invalid(
                "CDXML/CDX cannot preserve a turned, reflected or sheared table. Save as ReShiki (.rsk), or use SVG, PDF or PNG for the figure.",
            ));
        }
        let table = graphic
            .table
            .as_ref()
            .ok_or_else(|| invalid("Missing native table"))?;
        let cuts = |origin: f32, axis: f32, shift: f64, values: Vec<f32>| {
            let mut values = values;
            values.extend([0., 1.]);
            values.sort_by(f32::total_cmp);
            values.dedup_by(|a, b| *a == *b);
            let mut previous = None;
            let mut cuts = HashMap::new();
            for value in values {
                let paper = (real(origin) + real(axis) * real(value)) * scale + shift;
                let paper = (paper * 65536.).round_ties_even() / 65536.;
                if !paper.is_finite() || previous.is_some_and(|old| paper <= old) {
                    return Err(invalid(
                        "Table grid spacing is smaller than CDXML/CDX coordinate precision. Save as ReShiki (.rsk), or use SVG, PDF or PNG for the figure.",
                    ));
                }
                previous = Some(paper);
                cuts.insert(Self::key(value), paper);
            }
            Ok(cuts)
        };
        Ok(Self {
            x: cuts(
                graphic.origin.x,
                graphic.axis_x.x,
                shift.x,
                table.cells.iter().flat_map(|c| [c.lo.x, c.hi.x]).collect(),
            )?,
            y: cuts(
                graphic.origin.y,
                graphic.axis_y.y,
                shift.y,
                table.cells.iter().flat_map(|c| [c.lo.y, c.hi.y]).collect(),
            )?,
        })
    }

    fn point(&self, point: Point) -> Result<P> {
        Ok(P {
            x: *self
                .x
                .get(&Self::key(point.x))
                .ok_or_else(|| invalid("Missing table grid cut"))?,
            y: *self
                .y
                .get(&Self::key(point.y))
                .ok_or_else(|| invalid("Missing table grid cut"))?,
        })
    }

    fn bounds(&self, lo: Point, hi: Point) -> Result<String> {
        let (lo, hi) = (self.point(lo)?, self.point(hi)?);
        // Every 16.16 tick is exactly representable as f64. Its round-trip
        // decimal spelling also prevents the SDK's XML truncation adding a gap.
        Ok(format!(
            "{} {} {} {}",
            number(lo.x),
            number(lo.y),
            number(hi.x),
            number(hi.y)
        ))
    }
}

impl Writer<'_> {
    pub(super) fn native_table(&mut self, g: &Graphic, z: usize) -> Result<Key> {
        // CDX's cell BoundsInParent is an axis-aligned rectangle. Writing its
        // bounding box for a turned/sheared frame would change both ownership
        // geometry and clipping, so leave that drawing to native ReShiki.
        let grid = Grid::new(g, self.scale, self.shift)?;
        let id = self.id()?;
        self.tree.add(
            Some(self.page),
            "table",
            [
                ("id", id),
                ("Z", z.to_string()),
                (
                    "BoundingBox",
                    grid.bounds(Point::default(), Point::new(1., 1.))?,
                ),
            ],
        )
    }

    pub(super) fn tables(&mut self) -> Result<()> {
        let doc = self.doc;
        if !doc.graphics.iter().any(|g| g.table.is_some()) {
            return Ok(());
        }
        let ownership = Membership::new(doc).map_err(invalid)?;
        // A child group must fit in one cell. An outer group may include a
        // whole table and outside objects, represented by that table object.
        for group in &doc.groups {
            let owners: HashSet<_> = group
                .members
                .iter()
                .filter_map(|&id| ownership.cell_of(id))
                .collect();
            if owners
                .iter()
                .any(|owner| !group.members.contains(&owner.table))
                && (owners.len() > 1
                    || group
                        .members
                        .iter()
                        .any(|&id| ownership.cell_of(id).is_none()))
            {
                return Err(invalid(
                    "A drawing group crosses table cells. Ungroup it before editable CDXML/CDX export, or save as ReShiki (.rsk).",
                ));
            }
        }
        let mut pages: HashMap<CellRef, (Key, f64, f64)> = HashMap::new();
        for graphic in &doc.graphics {
            let Some(table) = &graphic.table else {
                continue;
            };
            let grid = Grid::new(graphic, self.scale, self.shift)?;
            let parent = self
                .objects
                .iter()
                .find(|(id, _)| *id == graphic.id)
                .map(|(_, key)| *key)
                .ok_or_else(|| invalid("Missing native table"))?;
            for cell in &table.cells {
                let bounds = grid.bounds(cell.lo, cell.hi)?;
                let id = self.id()?;
                let page = self.tree.add(
                    Some(parent),
                    "page",
                    [
                        ("id", id),
                        ("BoundingBox", bounds.clone()),
                        ("BoundsInParent", bounds),
                        ("WidthPages", "1".into()),
                        ("HeightPages", "1".into()),
                    ],
                )?;
                let origin = grid.point(cell.lo)?;
                pages.insert(
                    CellRef {
                        table: graphic.id,
                        cell: cell.id,
                    },
                    (page, origin.x, origin.y),
                );
            }
        }

        // Summarize actual serialized subtrees. A fragment, an abbreviation,
        // marks, and an ordinary cell-local group move together. Parent groups
        // that also contain the table or outside objects remain outside it.
        let mut owners: HashMap<Key, Option<CellRef>> = HashMap::new();
        let mut mixed = HashSet::new();
        let mut objects = self.objects.clone();
        objects.extend(self.atom_artwork.iter().map(|(&key, &id)| (id, key)));
        for &(id, key) in &objects {
            let owner = ownership.cell_of(id);
            let mut next = Some(key);
            while let Some(node) = next {
                self.spend(1)?;
                if let Some(old) = owners.insert(node, owner)
                    && old != owner
                {
                    mixed.insert(node);
                }
                next = self.tree.node(node)?.parent;
            }
        }
        let mut roots = HashMap::new();
        for (id, key) in objects {
            let Some(owner) = ownership.cell_of(id) else {
                continue;
            };
            let mut root = key;
            while let Some(parent) = self.tree.node(root)?.parent {
                if parent == self.page
                    || mixed.contains(&parent)
                    || owners.get(&parent) != Some(&Some(owner))
                {
                    break;
                }
                root = parent;
            }
            if mixed.contains(&root) {
                return Err(invalid("A serialized object crosses table cell ownership"));
            }
            if roots.insert(root, owner).is_some_and(|old| old != owner) {
                return Err(invalid("A serialized object crosses table cells"));
            }
        }
        // Stable source/XML order, independent of hash iteration order.
        let ordered = self.tree.descendants(self.page)?;
        for root in ordered {
            let Some(owner) = roots.get(&root) else {
                continue;
            };
            let &(page, x, y) = pages
                .get(owner)
                .ok_or_else(|| invalid("Missing native table cell"))?;
            self.tree.attach(page, root)?;
            for node in self.tree.descendants(root)? {
                let fields = crate::chemistry::cdxml::tables::GEOMETRY
                    .iter()
                    .filter_map(|&(key, dimensions)| {
                        self.tree
                            .get(node, key)
                            .transpose()
                            .map(|v| v.map(|v| (key, dimensions, v.to_owned())))
                    })
                    .collect::<Result<Vec<_>>>()?;
                for (key, dimensions, text) in fields {
                    let text =
                        crate::chemistry::cdxml::tables::translate(&text, key, dimensions, -x, -y)
                            .map_err(|e| invalid(e.to_string()))?;
                    self.tree.set(node, key, text)?;
                }
                if let Some(record) = self.tree.get(node, crate::exchange::PANEL)? {
                    let record = crate::chemistry::cdxml::tables::translate_panel(record, -x, -y)
                        .map_err(|e| invalid(e.to_string()))?;
                    self.tree.set(node, crate::exchange::PANEL, record)?;
                }
            }
        }
        // Coincident edges remain distinct records, painted in original cell
        // order and original record order, after each cell's clipped contents.
        for graphic in &doc.graphics {
            let Some(table) = &graphic.table else {
                continue;
            };
            for cell in &table.cells {
                let &(page, _, _) = pages
                    .get(&CellRef {
                        table: graphic.id,
                        cell: cell.id,
                    })
                    .ok_or_else(|| invalid("Missing native table cell"))?;
                for border in &cell.borders {
                    let pattern = match border.pattern {
                        LinePattern::Solid => "Solid",
                        LinePattern::Dashed => "Dashed",
                        _ => {
                            return Err(invalid(
                                "CDXML/CDX cannot preserve dotted or wavy table borders. Use solid/dashed borders, or save as ReShiki (.rsk).",
                            ));
                        }
                    };
                    let id = self.id()?;
                    let color = self.color(border.color)?;
                    self.tree.add(
                        Some(page),
                        "border",
                        [
                            ("id", id),
                            ("LineWidth", number(real(border.width_pt))),
                            ("color", color),
                            (
                                "Side",
                                match border.side {
                                    Side::Left => "left",
                                    Side::Top => "top",
                                    Side::Bottom => "bottom",
                                    Side::Right => "right",
                                }
                                .into(),
                            ),
                            ("LineType", pattern.into()),
                        ],
                    )?;
                }
            }
        }
        Ok(())
    }
}
