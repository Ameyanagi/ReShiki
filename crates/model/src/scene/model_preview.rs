//! Keep the drawing's expensive bond and label scene while a molecular model moves.
use super::{Document, Primitive, Resolve, SceneGraphics, primitives_with_tables};
use std::{ops::Range, rc::Rc};

/// The exact paint bands before and after the 3D models. Coordinate-only
/// updates replace the model band without laying out the unchanged drawing.
/// Tables, crystals and molecular diagrams keep the complete scene path: their
/// clipped scopes, 2D cell ink and orbital pictures also depend on models.
pub struct ModelScene {
    scene: Rc<[Primitive]>,
    models: Range<usize>,
}

impl ModelScene {
    /// Prepare a complete ordinary scene, sharing its storage with the retained
    /// paint bands rather than making another copy of the static primitives.
    /// `flat` and `opacity` have the same contract as [`super::primitives_with_opacity`].
    pub fn new(
        raw: &Document,
        flat: &Document,
        opacity: &crate::rear_opacity::Paint,
    ) -> Option<(Self, Rc<[Primitive]>)> {
        if !raw
            .models
            .iter()
            .any(|model| model.display != crate::models3d::Display::TwoD)
            || raw.models.iter().any(|model| model.cell.is_some())
            || raw.graphics.iter().any(|graphic| {
                graphic.table.is_some() || graphic.mo.is_some() || graphic.energy_profile.is_some()
            })
        {
            return None;
        }
        let range = std::cell::Cell::new((0, 0));
        let scene = primitives_with_tables(
            raw,
            raw,
            flat,
            opacity,
            SceneGraphics {
                model_range: Some(&range),
                ..Default::default()
            },
            Resolve::Scene,
            false,
        );
        let (start, end) = range.get();
        let scene: Rc<[_]> = scene.into();
        Some((
            Self {
                scene: scene.clone(),
                models: start..end,
            },
            scene,
        ))
    }

    /// Recompose only when every change is XYZ of an already shown molecular
    /// model. On success, update the caller's document snapshot too; on refusal,
    /// leave it untouched. Comparing complete documents/model settings keeps new
    /// style, topology, links, view, science and layout fields in the cache key.
    pub fn update(&self, previous: &mut Document, current: &Document) -> Option<Vec<Primitive>> {
        let before_ink = self.scene.get(..self.models.start)?;
        let after_ink = self.scene.get(self.models.end..)?;
        if previous.models == current.models || previous.models.len() != current.models.len() {
            return None;
        }
        for (before, after) in previous.models.iter().zip(&current.models) {
            if before == after {
                continue;
            }
            if before.display == crate::models3d::Display::TwoD {
                return None;
            }
            let mut normalized = after.clone();
            for (atom, original) in normalized.atoms.iter_mut().zip(&before.atoms) {
                atom.xyz = original.xyz;
            }
            for (hydrogen, original) in normalized.hydrogens.iter_mut().zip(&before.hydrogens) {
                hydrogen.xyz = original.xyz;
            }
            if &normalized != before {
                return None;
            }
        }
        let original = std::mem::replace(&mut previous.models, current.models.clone());
        if &*previous != current {
            previous.models = original;
            return None;
        }
        let models = crate::models3d::primitives(current);
        let mut scene = Vec::with_capacity(before_ink.len() + models.len() + after_ink.len());
        scene.extend_from_slice(before_ink);
        scene.extend(models);
        scene.extend_from_slice(after_ink);
        Some(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas_theme::CanvasTheme,
        document::{Annotation, Arrow},
        graphics::{Graphic, GraphicKind},
        models3d::{self, Display, Style},
    };

    fn drawing() -> Document {
        let mut doc = Document::from_json(include_bytes!(
            "../../../../tests/fixtures/models3d/controls/caffeine.rsk"
        ))
        .unwrap();
        let center = models3d::frame_origin(&doc, &doc.models[0]).unwrap();
        let a = doc.add_atom("N", center.offset(-15., -10.));
        let b = doc.add_atom("O", center.offset(25., 10.));
        doc.add_bond(a, b, 2, "plain");
        doc.annotations.push(Annotation {
            id: doc.next_id(),
            position: center,
            text: "Over the model".into(),
            format: Default::default(),
            angle: 0.,
        });
        doc.arrows.push(Arrow::new(
            doc.next_id(),
            center.offset(-70., 20.),
            center.offset(70., -20.),
            crate::arrows::Preset::Forward,
            Default::default(),
        ));
        for layer in [-1, 1] {
            let mut graphic = Graphic::dragged(
                doc.next_id(),
                GraphicKind::Rectangle,
                center.offset(-40., -40.),
                center.offset(40., 40.),
                Default::default(),
                Default::default(),
                false,
            );
            graphic.layer = layer;
            doc.graphics.push(graphic);
        }
        doc
    }

    fn prepare(doc: &Document) -> Option<(ModelScene, Rc<[Primitive]>)> {
        let flat = models3d::drawn(doc);
        let paint = crate::rear_opacity::Paint::new(&flat);
        ModelScene::new(doc, &flat, &paint)
    }

    #[test]
    fn moving_model_scene_matches_complete_scene_with_overlapping_ink() {
        for display in [Display::ThreeD, Display::Both] {
            for theme in CanvasTheme::ALL {
                for style in [Style::BallStick, Style::Stick, Style::SpaceFilling] {
                    let mut previous = drawing();
                    previous.models[0].display = display;
                    previous.models[0].style = style;
                    previous.canvas_theme = theme;
                    let (cache, initial) = prepare(&previous).unwrap();
                    assert!(Rc::ptr_eq(&cache.scene, &initial));
                    assert_eq!(
                        format!("{initial:?}"),
                        format!("{:?}", super::super::primitives(&previous))
                    );
                    assert!(cache.models.start > 0 && cache.models.end < cache.scene.len());
                    let mut current = previous.clone();
                    for frame in 1..=3 {
                        current.models[0].atoms[0].xyz[2] += frame as f32 * 0.03;
                        current.models[0].hydrogens[0].xyz[0] += frame as f32 * 0.02;
                        let updated = cache.update(&mut previous, &current).unwrap();
                        assert_eq!(previous, current);
                        assert_eq!(
                            format!("{updated:?}"),
                            format!("{:?}", super::super::primitives(&current))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn model_scene_rejects_other_document_and_model_changes_without_updating_snapshot() {
        let original = drawing();
        let (cache, _) = prepare(&original).unwrap();
        let changed: [fn(&mut Document); 8] = [
            |doc| doc.models[0].display = Display::Both,
            |doc| doc.models[0].rotation = models3d::turn(doc.models[0].rotation, 5., 3., false),
            |doc| doc.models[0].size += 5.,
            |doc| doc.models[0].atoms[0].id += 1,
            |doc| doc.atoms[0].position.x += 3.,
            |doc| doc.annotations[0].text.push('!'),
            |doc| doc.canvas_theme = doc.canvas_theme.toggled(),
            |doc| doc.bonds[0].order = if doc.bonds[0].order == 1 { 2 } else { 1 },
        ];
        for change in changed {
            let mut previous = original.clone();
            let mut current = original.clone();
            current.models[0].atoms[0].xyz[2] += 0.1;
            change(&mut current);
            assert!(cache.update(&mut previous, &current).is_none());
            assert_eq!(previous, original);
        }
    }

    #[test]
    fn model_scene_keeps_tables_crystals_and_model_linked_diagrams_on_full_path() {
        let mut doc = drawing();
        doc.models[0].display = Display::TwoD;
        assert!(prepare(&doc).is_none());
        doc.models[0].display = Display::ThreeD;
        doc.models[0].cell = Some(models3d::Cell {
            vectors: [[8., 0., 0.], [0., 7., 0.], [0., 0., 6.]],
            crossing: vec![],
        });
        assert!(prepare(&doc).is_none());
        doc.models[0].cell = None;
        doc.graphics[0].table = Some(Box::new(crate::graphics::table::Table { cells: vec![] }));
        assert!(prepare(&doc).is_none());
        doc.graphics[0].table = None;
        doc.graphics[0].mo = Some(Box::new(crate::graphics::mo_diagram::MoDiagram::new(
            doc.atoms[0].id,
            String::new(),
            vec![],
        )));
        assert!(prepare(&doc).is_none());
    }
}
