//! Native cell pages keep chemistry editable and let ChemDraw clip/reflow it.
use super::*;
use crate::graphics::{
    Graphic, LinePattern,
    table::{CellRef, Membership, Side},
};
use std::collections::{HashMap, HashSet};

impl Writer<'_> {
    pub(super) fn native_table(&mut self, g: &Graphic, z: usize) -> Result<Key> {
        // CDX's cell BoundsInParent is an axis-aligned rectangle. Writing its
        // bounding box for a turned/sheared frame would change both ownership
        // geometry and clipping, so leave that drawing to native ReShiki.
        if g.axis_x.x <= 0.
            || g.axis_y.y <= 0.
            || g.axis_x.y.abs() > 0.00001
            || g.axis_y.x.abs() > 0.00001
        {
            return Err(invalid(
                "CDXML/CDX cannot preserve a turned, reflected or sheared table. Save as ReShiki (.rsk), or use SVG, PDF or PNG for the figure.",
            ));
        }
        let id = self.id()?;
        self.tree.add(
            Some(self.page),
            "table",
            [
                ("id", id),
                ("Z", z.to_string()),
                (
                    "BoundingBox",
                    format!(
                        "{} {}",
                        self.position(g.origin),
                        self.position(g.origin.offset(g.axis_x.x, g.axis_y.y))
                    ),
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
            let parent = self
                .objects
                .iter()
                .find(|(id, _)| *id == graphic.id)
                .map(|(_, key)| *key)
                .ok_or_else(|| invalid("Missing native table"))?;
            for cell in &table.cells {
                let frame = cell
                    .frame(graphic)
                    .ok_or_else(|| invalid("Invalid table cell frame"))?;
                let bounds = format!(
                    "{} {}",
                    self.position(frame.origin),
                    self.position(frame.point(Point::new(1., 1.)))
                );
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
                let origin = P::from(frame.origin);
                pages.insert(
                    CellRef {
                        table: graphic.id,
                        cell: cell.id,
                    },
                    (
                        page,
                        origin.x * self.scale + self.shift.x,
                        origin.y * self.scale + self.shift.y,
                    ),
                );
            }
        }

        // Summarize actual serialized subtrees. A fragment, an abbreviation,
        // marks, and an ordinary cell-local group move together. Parent groups
        // that also contain the table or outside objects remain outside it.
        let mut owners: HashMap<Key, Option<CellRef>> = HashMap::new();
        let mut mixed = HashSet::new();
        for (id, key) in self.objects.clone() {
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
        for (id, key) in self.objects.clone() {
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
