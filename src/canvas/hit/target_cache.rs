//! One bond drag's target and source plane, invalidated with the scene's document.
use super::{BondDrawing, Document, Tool, World};
use reshiki::projection::growth::{Endpoint, Plane};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::canvas) struct BondTarget {
    pub end: World,
    pub atom: Option<u64>,
    pub text: Option<u64>,
    pub plane: Option<Endpoint>,
}

#[derive(PartialEq)]
struct Key {
    start: World,
    end: World,
    source: (Option<u64>, Option<u64>),
    radius: f32,
    drawing: (bool, bool, u32),
    tool: Tool,
}

#[derive(Default)]
pub(in crate::canvas) struct BondTargetCache {
    source: Option<u64>,
    // Retain negative results too: ordinary bonds have no aromatic plane.
    plane: Option<Plane>,
    target: Option<(Key, Result<BondTarget, String>)>,
    #[cfg(test)]
    pub plane_builds: usize,
    #[cfg(test)]
    pub target_builds: usize,
}

impl BondTargetCache {
    pub fn resolve(
        &mut self,
        doc: &Document,
        points: (World, World),
        source: (Option<u64>, Option<u64>),
        radius: f32,
        drawing: BondDrawing,
        tool: Tool,
    ) -> Result<BondTarget, String> {
        let (start, end) = points;
        let key = Key {
            start,
            end,
            source,
            radius,
            drawing: (
                drawing.fixed_length,
                drawing.fixed_angles,
                drawing.length.to_bits(),
            ),
            tool,
        };
        if let Some((cached, target)) = &self.target
            && *cached == key
        {
            return target.clone();
        }
        if self.source != source.0 {
            self.source = source.0;
            self.plane = source.0.and_then(|id| {
                #[cfg(test)]
                {
                    self.plane_builds += 1;
                }
                Plane::at(doc, id)
            });
        }
        let plane = self
            .plane
            .as_ref()
            .and_then(|plane| plane.endpoint(end, drawing));
        let target = super::bond_target_with_plane(
            doc, points, source, radius, drawing, tool, plane,
        )
        .map(|(end, atom, text)| BondTarget {
            end,
            atom,
            text,
            plane,
        });
        #[cfg(test)]
        {
            self.target_builds += 1;
        }
        self.target = Some((key, target.clone()));
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bond_target_reuses_negative_planes_and_identical_queries() {
        let mut doc = Document::default();
        let start = doc.add_atom("C", World::default());
        let target = doc.add_atom("C", World::new(84., 0.));
        doc.add_bond(start, target, 1, "plain");
        let mut cache = BondTargetCache::default();
        for end in [
            World::new(80., 3.),
            World::new(80., 3.),
            World::new(100., 20.),
        ] {
            let resolved = cache
                .resolve(
                    &doc,
                    (World::default(), end),
                    (Some(start), None),
                    12.,
                    BondDrawing::default(),
                    Tool::Bond(1),
                )
                .unwrap();
            assert_eq!(
                (resolved.end, resolved.atom, resolved.text),
                super::super::bond_target_with_text(
                    &doc,
                    World::default(),
                    end,
                    (Some(start), None),
                    12.,
                    BondDrawing::default(),
                    Tool::Bond(1),
                )
                .unwrap(),
            );
            assert_eq!(resolved.plane, None);
        }
        assert_eq!(cache.plane_builds, 1);
        assert_eq!(cache.target_builds, 2);
    }

    #[test]
    fn bond_target_reuses_the_plane_when_constraints_or_targets_change() {
        let mut doc = reshiki::rings::Preset::Benzene.document(42., false);
        let ids = doc.all_ids();
        reshiki::projection::tilt(&mut doc, &ids, 55., false);
        let source = doc.atoms[1].id;
        let origin = doc.atoms[1].position;
        let plane = Plane::at(&doc, source).unwrap();
        let cursor = plane.outward(84.).unwrap().position;
        let target = doc.atoms[0].position;
        let mut cache = BondTargetCache::default();
        for drawing in [
            BondDrawing::default(),
            BondDrawing::default().unconstrained(true),
        ] {
            for end in [cursor, target] {
                let resolved = cache
                    .resolve(
                        &doc,
                        (origin, end),
                        (Some(source), None),
                        12.,
                        drawing,
                        Tool::Atom,
                    )
                    .unwrap();
                assert_eq!(resolved.plane, plane.endpoint(end, drawing));
                assert_eq!(
                    (resolved.end, resolved.atom),
                    super::super::bond_target_with(&doc, origin, end, Some(source), 12., drawing),
                );
                if end == target {
                    assert_eq!(resolved.atom, Some(doc.atoms[0].id));
                    assert_eq!(resolved.end, target);
                }
            }
        }
        assert_eq!(cache.plane_builds, 1);
        assert_eq!(cache.target_builds, 4);
    }

    #[test]
    fn bond_target_preserves_an_ambiguous_source_plane() {
        let mut doc = Document::default();
        let source = doc.add_atom("C", World::default());
        doc.atom_mut(source).unwrap().aromatic = true;
        for vertical in [false, true] {
            let mut ring = vec![source];
            for i in 1..6 {
                let angle = i as f32 * std::f32::consts::TAU / 6.;
                let x = 42. * (angle.cos() - 1.);
                let y = 42. * angle.sin();
                let id = doc.add_atom("C", World::new(x, if vertical { 0. } else { y }));
                let atom = doc.atom_mut(id).unwrap();
                atom.aromatic = true;
                atom.depth = if vertical { y } else { 0. };
                ring.push(id);
            }
            for i in 0..6 {
                doc.add_bond(ring[i], ring[(i + 1) % 6], 4, "plain");
            }
            if !vertical {
                assert!(Plane::at(&doc, source).is_some());
            }
        }
        assert!(Plane::at(&doc, source).is_none());
        let mut cache = BondTargetCache::default();
        for end in [World::new(80., 20.), World::new(90., 30.)] {
            let resolved = cache
                .resolve(
                    &doc,
                    (World::default(), end),
                    (Some(source), None),
                    12.,
                    BondDrawing::default(),
                    Tool::Bond(1),
                )
                .unwrap();
            assert_eq!(resolved.plane, None);
            assert_eq!(
                resolved.end,
                BondDrawing::default().endpoint(World::default(), end)
            );
        }
        assert_eq!(cache.plane_builds, 1);
    }
}
