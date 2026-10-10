//! New mechanism-arrow targets use the same opacity/crossing/rail clipping as
//! the bond scene. Persistent links deliberately do not use this hit-test gate.
use super::*;
use std::cell::OnceCell;

pub(crate) struct AttachmentInk<'a> {
    doc: &'a Document,
    active: bool,
    opacity: OnceCell<crate::rear_opacity::Paint>,
    bonds: OnceCell<Vec<Vec<Primitive>>>,
    incidents: OnceCell<std::collections::HashMap<u64, Vec<(usize, f32)>>>,
}
impl<'a> AttachmentInk<'a> {
    pub(crate) fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            active: crate::rear_opacity::present(doc),
            opacity: OnceCell::new(),
            bonds: OnceCell::new(),
            incidents: OnceCell::new(),
        }
    }
    fn opacity(&self) -> Option<&crate::rear_opacity::Paint> {
        self.active.then(|| {
            self.opacity
                .get_or_init(|| crate::rear_opacity::Paint::new(self.doc))
        })
    }
    pub(crate) fn atom(&self, id: u64) -> bool {
        self.doc.atom(id).is_some_and(|atom| self.vertex(atom))
    }
    fn vertex(&self, atom: &Atom) -> bool {
        if !self.doc.atom_visible(atom.id) {
            return false;
        }
        if self.opacity().is_none_or(|paint| paint.atom(atom.id) > 0.) {
            return true;
        }
        if atom.element != "C" || self.doc.abbreviation(atom.id).is_some() {
            return false;
        }
        // Implicit carbon vertices have no label glyph. Surviving incident
        // stroke ink, including projected rails/rim caps, still supplies a
        // real vertex target. An entirely hidden vertex supplies no new hit.
        let incidents = self.incidents.get_or_init(|| {
            let mut map: std::collections::HashMap<u64, Vec<(usize, f32)>> = Default::default();
            for (index, bond) in self.doc.bonds.iter().enumerate() {
                map.entry(bond.a).or_default().push((index, 0.));
                map.entry(bond.b).or_default().push((index, 1.));
            }
            map
        });
        let bonds = incidents
            .get(&atom.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if crate::atom_labels::visible_with_degree(atom, self.doc, || bonds.len()) {
            return false;
        }
        bonds.iter().any(|&(index, t)| self.bond(index, t))
    }
    pub(crate) fn mark(&self, id: u64) -> bool {
        self.doc.atom_visible(id) && self.opacity().is_none_or(|paint| paint.atom(id) > 0.)
    }
    pub(crate) fn segment(&self, bond: &Bond) -> Option<(Point, Point)> {
        bond_segment(self.doc, bond, self.opacity())
    }
    pub(crate) fn label_hit(&self, p: Point, radius: f32) -> Option<u64> {
        self.doc
            .atoms
            .iter()
            .rev()
            .filter(|a| self.doc.atom_visible(a.id))
            .find_map(|a| {
                (atom_label_ink_boxes(a, self.doc)
                    .into_iter()
                    .any(|(lo, hi)| {
                        p.x >= lo.x - radius
                            && p.x <= hi.x + radius
                            && p.y >= lo.y - radius
                            && p.y <= hi.y + radius
                    })
                    && self.mark(a.id))
                .then_some(a.id)
            })
    }
    pub(crate) fn nearest_atom(&self, p: Point, radius: f32) -> Option<u64> {
        self.doc
            .atoms
            .iter()
            .filter(|a| a.position.distance(p) < radius && self.vertex(a))
            .min_by(|a, b| a.position.distance(p).total_cmp(&b.position.distance(p)))
            .map(|a| a.id)
            .or_else(|| {
                crate::abbreviations::label_hit(self.doc, p, radius).filter(|id| self.atom(*id))
            })
    }
    pub(crate) fn bond(&self, index: usize, fraction: f32) -> bool {
        let Some(bond) = self.doc.bonds.get(index) else {
            return false;
        };
        if !self.doc.bond_visible(bond.a, bond.b)
            || !fraction.is_finite()
            || !(0.0..=1.).contains(&fraction)
        {
            return false;
        }
        // Keep the original fully opaque picker behavior. When opacity is
        // active, require surviving bond ink at this longitudinal fraction,
        // rather than classifying only the backbone's Z or midpoint alpha.
        let Some(opacity) = self.opacity() else {
            return true;
        };
        if opacity.is_empty() {
            return true;
        }
        let Some((a, b)) = self.doc.atom(bond.a).zip(self.doc.atom(bond.b)) else {
            return false;
        };
        let axis = Point::new(b.position.x - a.position.x, b.position.y - a.position.y);
        let length = axis.x.hypot(axis.y);
        if length < 0.00001 {
            return false;
        }
        let unit = Point::new(axis.x / length, axis.y / length);
        let at = length * fraction;
        self.bonds
            .get_or_init(|| rendered_bond_parts(self.doc, opacity))
            .get(index)
            .is_some_and(|parts| {
                parts
                    .iter()
                    .any(|part| intersects_fraction(part, a.position, unit, at))
            })
    }
}

fn rendered_bond_parts(
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
) -> Vec<Vec<Primitive>> {
    // Exactly the scene's detached color/style resolution and molecular bond
    // pass. Ring strokes provide crossing gaps; their own ink is not attributed
    // to an arbitrary bond. Unrelated labels, pictures and arrows cannot make a
    // hidden chemical bond eligible. The one-time result is shared by all
    // candidate bonds of this pointer/commit query.
    let paint = crate::depth_appearance::Paint::new(doc);
    let painted = paint.materialize(doc);
    let resolved = crate::canvas_theme::canonical_document(&painted);
    let doc = resolved.as_ref();
    let RingStrokes {
        arcs,
        circles,
        crossing_gaps,
    } = push_ring_strokes(&mut Vec::new(), doc, opacity);
    let labels = collect_atom_labels(doc, opacity);
    let joins = crate::bond_joins::Joins::new(doc);
    let scene = BondScene {
        doc,
        style: &doc.drawing_style,
        arcs: &arcs,
        circles: &circles,
        label_bounds: &labels.bounds,
        joins: &joins,
        crossing_gaps: &crossing_gaps,
        opacity,
    };
    doc.bonds
        .iter()
        .enumerate()
        .map(|(index, bond)| {
            let mut parts = Vec::new();
            let mut joined = Default::default();
            if doc.bond_visible(bond.a, bond.b) {
                push_bond(scene, &mut parts, &mut joined, index, bond, None);
                push_joined_outlines(&mut parts, joined);
            }
            parts
        })
        .collect()
}

fn intersects_fraction(part: &Primitive, origin: Point, axis: Point, at: f32) -> bool {
    let project = |p: Point| (p.x - origin.x) * axis.x + (p.y - origin.y) * axis.y;
    let range = |points: &[Point], half: f32| {
        let lo = points
            .iter()
            .copied()
            .map(project)
            .fold(f32::INFINITY, f32::min);
        let hi = points
            .iter()
            .copied()
            .map(project)
            .fold(f32::NEG_INFINITY, f32::max);
        lo - half <= at && at <= hi + half
    };
    match part {
        Primitive::Opacity { alpha, primitive } => {
            *alpha > 0. && intersects_fraction(primitive, origin, axis, at)
        }
        Primitive::Line(a, b, width) => *width > 0. && range(&[*a, *b], *width * 0.5),
        Primitive::Polygon(points) => area(points).abs() > 1e-10 && range(points, 0.),
        Primitive::Path {
            commands,
            style,
            filled,
        } => {
            // Existing scene geometry uses the same bounded 32-sample cubic
            // flattening for pointer tests. This is an ink eligibility test,
            // not a new per-pixel visibility certificate.
            crate::graphics::flattened(commands).iter().any(|path| {
                (*filled && style.fill.is_some() && area(path).abs() > 1e-10 && range(path, 0.))
                    || (style.width() > 0.
                        && path.windows(2).any(|line| range(line, style.width() * 0.5)))
            })
        }
        Primitive::Picture(_) | Primitive::Text { .. } => false,
    }
}
fn area(points: &[Point]) -> f64 {
    let Some(origin) = points.first() else {
        return 0.;
    };
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| {
            (f64::from(a.x) - f64::from(origin.x)) * (f64::from(b.y) - f64::from(origin.y))
                - (f64::from(a.y) - f64::from(origin.y)) * (f64::from(b.x) - f64::from(origin.x))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fully_opaque_and_remote_pointer_queries_leave_visibility_caches_cold() {
        let mut doc = Document::from_native_file(include_bytes!(
            "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
        ))
        .unwrap();
        let ids = doc.all_ids();
        crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
        let ink = AttachmentInk::new(&doc);
        assert!(ink.atom(52));
        assert!(ink.bond(0, 0.5));
        assert!(ink.opacity.get().is_none());
        assert!(ink.bonds.get().is_none());
        assert!(ink.incidents.get().is_none());
        crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
        let ink = AttachmentInk::new(&doc);
        let remote = Point::new(50_000., 50_000.);
        assert_eq!(ink.label_hit(remote, 2.), None);
        assert_eq!(ink.nearest_atom(remote, 2.), None);
        assert!(!ink.bond(0, -1.));
        assert!(ink.opacity.get().is_none());
        assert!(ink.bonds.get().is_none());
        assert!(
            !ink.bond(
                doc.bonds
                    .iter()
                    .position(|b| b.a == 52 && b.b == 57)
                    .unwrap(),
                0.5
            )
        );
        let cache = ink.bonds.get().unwrap().as_ptr();
        assert!(
            ink.bond(
                doc.bonds
                    .iter()
                    .position(|b| b.a == 38 && b.b == 48)
                    .unwrap(),
                0.5
            )
        );
        assert_eq!(
            ink.bonds.get().unwrap().as_ptr(),
            cache,
            "All nearby candidates share one actual bond-scene construction"
        );
    }
    #[test]
    fn clipped_ink_fraction_does_not_bridge_hidden_middle_or_accept_zero_alpha() {
        let a = Point::new(-80., 0.);
        let axis = Point::new(1., 0.);
        let pieces = [
            Primitive::Line(a, Point::new(-5., 0.), 2.),
            Primitive::Line(Point::new(5., 0.), Point::new(80., 0.), 2.),
        ];
        assert!(!pieces.iter().any(|p| intersects_fraction(p, a, axis, 80.)));
        assert!(pieces.iter().any(|p| intersects_fraction(p, a, axis, 40.)));
        for alpha in [0., 0.25] {
            let part = Primitive::Opacity {
                alpha,
                primitive: Box::new(Primitive::Line(Point::new(-5., 0.), Point::new(5., 0.), 2.)),
            };
            assert_eq!(intersects_fraction(&part, a, axis, 80.), alpha > 0.);
        }
        // A genuine surviving secondary rail is eligible even away from the
        // chemical backbone; absent neighboring bonds/arrows do not count.
        assert!(intersects_fraction(
            &Primitive::Line(Point::new(-5., 6.), Point::new(5., 6.), 1.),
            a,
            axis,
            80.
        ));
    }
    #[test]
    fn actual_crossing_clip_hides_middle_and_keeps_exposed_bond_parts() {
        let mut doc = Document::default();
        let ids: Vec<_> = [
            (-80., 0., -20.),
            (80., 0., -20.),
            (0., -50., 20.),
            (0., 50., 20.),
        ]
        .into_iter()
        .map(|(x, y, z)| {
            let id = doc.add_atom("C", Point::new(x, y));
            doc.atom_mut(id).unwrap().depth = z;
            id
        })
        .collect();
        doc.add_bond(ids[0], ids[1], 1, "plain");
        doc.add_bond(ids[2], ids[3], 1, "plain");
        doc.add_bond(ids[0], ids[2], 1, "plain");
        crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
        let ink = AttachmentInk::new(&doc);
        assert!(!ink.bond(0, 0.5));
        assert!(ink.bond(0, 0.25));
        assert!(ink.bond(0, 0.75));
        assert!(ink.bond(1, 0.5));
        crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
        assert!(AttachmentInk::new(&doc).bond(0, 0.5));
    }
}
