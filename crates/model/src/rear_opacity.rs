//! Transparency for view-occluded cage/atom/bond ink, independent of RGB.
//! Positive Z is toward the viewer. Exposed ink, including the complete cage
//! rim, remains opaque regardless of its distance from the depth midplane.
use crate::{
    document::{Bond, Document, Point},
    graphics::PathCommand,
    scene::{
        Primitive,
        depth_field::{DepthField, bond_depth_field, lerp},
    },
};
use std::collections::BTreeMap;
mod clip;
mod ink;
mod visibility;
pub(crate) use clip::capsule;
use clip::{flatten, mask_clip, split_curve, split_fill, split_intervals};

pub const EXPORT_NOTICE: &str = "This chemical format does not retain rear opacity. Chemical structure and XYZ are unchanged; use native ReShiki for editable appearance or SVG/PDF/PNG for the figure.";

pub fn present(doc: &Document) -> bool {
    doc.depth_appearance
        .iter()
        .any(crate::depth_appearance::Scope::has_rear_opacity)
}
#[derive(Clone, Copy)]
struct Node {
    position: Point,
    depth: f32,
    opacity: f32,
}
/// Built from original current XYZ, before detached RGB/style normalization.
pub struct Paint {
    nodes: BTreeMap<u64, Node>,
    atoms: BTreeMap<u64, f32>,
    visibility: Option<visibility::Visibility>,
    corners: BTreeMap<(u64, u64, u64), Vec<Point>>,
}
impl Paint {
    pub fn new(doc: &Document) -> Self {
        if !present(doc) {
            return Self {
                nodes: BTreeMap::new(),
                atoms: BTreeMap::new(),
                visibility: None,
                corners: BTreeMap::new(),
            };
        }
        let mut nodes: BTreeMap<_, _> = doc
            .atoms
            .iter()
            .map(|a| {
                (
                    a.id,
                    Node {
                        position: a.position,
                        depth: a.depth,
                        opacity: 1.,
                    },
                )
            })
            .collect();
        for scope in doc.depth_appearance.iter().filter(|s| s.has_rear_opacity()) {
            for id in &scope.atoms {
                if let Some(node) = nodes.get_mut(id) {
                    node.opacity = scope.rear_opacity_for(*id).clamp(0., 1.);
                }
            }
        }
        for atom in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
            let opacity = atom
                .centroid
                .iter()
                .filter_map(|id| nodes.get(id).map(|n| n.opacity))
                .fold(1., f32::min);
            if let Some(node) = nodes.get_mut(&atom.id) {
                node.opacity = opacity;
            }
        }
        // Preserve the exact legacy 100% path, including draw order and curves.
        let visibility = nodes
            .values()
            .any(|n| n.opacity < 1.)
            .then(|| visibility::Visibility::new(doc))
            .filter(|v| !v.is_empty());
        let atoms: BTreeMap<_, _> = nodes
            .iter()
            .map(|(id, node)| {
                (
                    *id,
                    if node.opacity < 1.
                        && visibility
                            .as_ref()
                            .is_some_and(|v| v.occluded(node.position, node.depth, &[*id], 0))
                    {
                        node.opacity
                    } else {
                        1.
                    },
                )
            })
            .collect();
        let mut corners = BTreeMap::new();
        if let Some(v) = &visibility {
            let joins = crate::bond_joins::Joins::new(doc);
            for bond in &doc.bonds {
                if !joins.needed(bond) {
                    continue;
                }
                for id in [bond.a, bond.b] {
                    if atoms.get(&id) == Some(&1.)
                        && v.rim_anchor(id).is_some()
                        && let Some(corner) = joins.corner(bond, id)
                    {
                        corners.insert((bond.a.min(bond.b), bond.a.max(bond.b), id), corner);
                    }
                }
            }
        }
        Self {
            nodes,
            atoms,
            visibility,
            corners,
        }
    }
    fn node(&self, id: u64) -> Node {
        self.nodes.get(&id).copied().unwrap_or(Node {
            position: Point::default(),
            depth: 0.,
            opacity: 1.,
        })
    }
    /// Whether view visibility is available; false retains the legacy opaque path.
    pub fn has_visibility(&self) -> bool {
        self.visibility.is_some()
    }
    pub fn atom(&self, id: u64) -> f32 {
        self.atoms.get(&id).copied().unwrap_or(1.)
    }
    fn bond_alpha(&self, bond: &Bond) -> f32 {
        self.node(bond.a).opacity.min(self.node(bond.b).opacity)
    }
    fn min_alpha(&self, ids: &[u64]) -> f32 {
        ids.iter()
            .map(|id| self.node(*id).opacity)
            .fold(1., f32::min)
    }
    pub(crate) fn manages_bond(&self, bond: &Bond) -> bool {
        self.visibility.is_some() && self.bond_alpha(bond) < 1.
    }
    pub fn bond(&self, bond: &Bond, t: f32) -> f32 {
        let (a, b) = (self.node(bond.a), self.node(bond.b));
        let alpha = a.opacity.min(b.opacity);
        if alpha >= 1. {
            return 1.;
        }
        let t = t.clamp(0., 1.);
        if self.visibility.as_ref().is_some_and(|v| {
            v.occluded(
                a.position.lerp(b.position, t),
                lerp(a.depth, b.depth, t),
                &[bond.a, bond.b],
                bond.z_order,
            )
        }) {
            alpha
        } else {
            1.
        }
    }
    pub(crate) fn bond_parts(
        &self,
        doc: &Document,
        bond: &Bond,
        parts: Vec<Primitive>,
    ) -> Vec<Primitive> {
        let alpha = self.bond_alpha(bond);
        let (Some(v), Some(a), Some(b)) = (&self.visibility, doc.atom(bond.a), doc.atom(bond.b))
        else {
            return parts;
        };
        if alpha >= 1. {
            return parts;
        }
        let field = bond_depth_field(doc, bond);
        let mut out = vec![];
        for part in parts {
            // Every projected secondary rail uses its own actual centerline.
            // Wedges/junctions inherit the backbone intervals across full width.
            let (first, last) = match &part {
                Primitive::Line(first, last, _)
                    if parallel(*first, *last, a.position, b.position) =>
                {
                    (*first, *last)
                }
                _ => (a.position, b.position),
            };
            let hidden = v.hidden(first, last, field, &[bond.a, bond.b], bond.z_order);
            if !hidden.is_empty() {
                out.extend(self.exposed_caps(doc, bond, &part));
            }
            out.extend(split_intervals(part, first, last, &hidden, alpha));
        }
        out
    }
    pub(crate) fn atom_parts(&self, id: u64, parts: Vec<Primitive>) -> Vec<Primitive> {
        with_opacity(parts, self.atom(id))
    }
    pub(crate) fn rail_parts(
        &self,
        doc: &Document,
        bond: &Bond,
        rail: &crate::scene::projected_bonds::RailDepth,
        parts: Vec<Primitive>,
    ) -> Vec<Primitive> {
        let alpha = self.bond_alpha(bond);
        let Some(v) = &self.visibility else {
            return parts;
        };
        if alpha >= 1. {
            return parts;
        }
        let (a, b) = rail.points;
        let support = (!rail.support.is_empty()).then_some(rail.support.as_slice());
        let hidden =
            v.hidden_with_support(a, b, rail.field, &[bond.a, bond.b], bond.z_order, support);
        let mut out = vec![];
        for part in parts {
            if support.is_none() && !hidden.is_empty() {
                out.extend(self.exposed_caps(doc, bond, &part));
            }
            out.extend(split_intervals(part, a, b, &hidden, alpha));
        }
        out
    }
    fn exposed_caps(&self, doc: &Document, bond: &Bond, part: &Primitive) -> Vec<Primitive> {
        let Some(v) = &self.visibility else {
            return vec![];
        };
        let original = match part {
            Primitive::Polygon(p) => p.clone(),
            Primitive::Line(a, b, w) => capsule(*a, *b, *w),
            _ => return vec![],
        };
        let width = doc.drawing_style.world(
            if matches!(bond.display.as_str(), "bold" | "wedge" | "hollow_wedge") {
                doc.drawing_style.bold_width_pt
            } else {
                doc.drawing_style.line_width_pt
            },
        );
        [bond.a, bond.b]
            .into_iter()
            .filter(|id| self.atom(*id) == 1.)
            .filter_map(|id| v.rim_anchor(id).map(|p| (id, p)))
            .flat_map(|(id, anchor)| {
                // The shared corner belongs to the exposed atom anchor.
                // Restore only its original ink, never an added disc/halo.
                let round = capsule(anchor, anchor, width);
                let corner = self
                    .corners
                    .get(&(bond.a.min(bond.b), bond.a.max(bond.b), id));
                let mask = if matches!(part, Primitive::Polygon(_)) {
                    corner.unwrap_or(&round)
                } else {
                    &round
                };
                let pieces = mask_clip(&original, mask, false);
                let other = self
                    .node(if id == bond.a { bond.b } else { bond.a })
                    .position;
                let length = anchor.distance(other);
                if length < 1e-6 {
                    return vec![];
                }
                let axis = Point::new((other.x - anchor.x) / length, (other.y - anchor.y) / length);
                // Include the complete original cap extent, including any
                // shared miter projecting outside the backbone endpoints.
                let (low, high) = pieces
                    .iter()
                    .flatten()
                    .map(|p| (p.x - anchor.x) * axis.x + (p.y - anchor.y) * axis.y)
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), t| {
                        (lo.min(t), hi.max(t))
                    });
                if high - low < 1e-6 {
                    return vec![];
                }
                let a = anchor.offset(axis.x * low, axis.y * low);
                let b = anchor.offset(axis.x * high, axis.y * high);
                let hidden = v.external_cap_hidden(
                    a,
                    b,
                    DepthField::Constant(self.node(id).depth),
                    bond,
                    id,
                );
                pieces
                    .into_iter()
                    .flat_map(|p| split_intervals(Primitive::Polygon(p), a, b, &hidden, 0.))
                    .collect()
            })
            .collect()
    }
    fn owned_field(doc: &Document, ids: &[u64]) -> DepthField {
        let vertices: Vec<_> = ids
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| (a.position, a.depth)))
            .collect();
        DepthField::plane(&vertices).unwrap_or_else(|| {
            DepthField::Constant(
                vertices.iter().map(|(_, z)| z).sum::<f32>() / vertices.len().max(1) as f32,
            )
        })
    }
    pub(crate) fn owned_at(&self, doc: &Document, ids: &[u64], point: Point) -> f32 {
        let alpha = self.min_alpha(ids);
        if !self.has_visibility() || ids.is_empty() || alpha >= 1. {
            return 1.;
        }
        let field = Self::owned_field(doc, ids);
        if self
            .visibility
            .as_ref()
            .is_some_and(|v| v.occluded(point, field.at(point), ids, 0))
        {
            alpha
        } else {
            1.
        }
    }
    pub(crate) fn owned_parts(
        &self,
        doc: &Document,
        ids: &[u64],
        parts: Vec<Primitive>,
    ) -> Vec<Primitive> {
        if let [id] = ids {
            return self.atom_parts(*id, parts);
        }
        let Some(v) = &self.visibility else {
            return parts;
        };
        let alpha = self.min_alpha(ids);
        if alpha >= 1. || ids.is_empty() {
            return parts;
        }
        let field = Self::owned_field(doc, ids);
        let mut out = vec![];
        for part in parts {
            match &part {
                Primitive::Line(a, b, _) => out.extend(split_intervals(
                    part.clone(),
                    *a,
                    *b,
                    &v.hidden(*a, *b, field, ids, 0),
                    alpha,
                )),
                Primitive::Path {
                    commands,
                    filled: false,
                    ..
                } => {
                    let paths = flatten(commands);
                    let samples: Vec<_> = paths.iter().flat_map(|p| p.windows(2)).collect();
                    if samples
                        .iter()
                        .all(|p| matches!(p,[a,b] if v.hidden(*a,*b,field,ids,0).is_empty()))
                    {
                        out.push(part);
                    } else {
                        out.extend(split_curve(part, paths, field, v, ids, alpha));
                    }
                }
                _ => out.extend(split_fill(part, field, v, ids, alpha)),
            }
        }
        out
    }
}
const RAIL_PARALLEL_COS: f32 = 0.7;

fn parallel(a: Point, b: Point, c: Point, d: Point) -> bool {
    let x = b.x - a.x;
    let y = b.y - a.y;
    let u = d.x - c.x;
    let v = d.y - c.y;
    (x * u + y * v).abs() >= RAIL_PARALLEL_COS * (x * x + y * y).sqrt() * (u * u + v * v).sqrt()
}
pub(crate) fn with_opacity(parts: Vec<Primitive>, alpha: f32) -> Vec<Primitive> {
    if alpha <= 0. {
        vec![]
    } else if alpha >= 1. {
        parts
    } else {
        parts
            .into_iter()
            .map(|part| Primitive::Opacity {
                alpha,
                primitive: Box::new(ink::single_paint(part)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
