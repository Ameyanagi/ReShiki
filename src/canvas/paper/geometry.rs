//! Retain the paper's static selection markers and tessellated scene together.
use super::super::{Camera, layered, markers::Markers, models3d, render::draw_primitives};
use iced::Rectangle;
use reshiki::scene::Primitive;
use std::rc::Rc;

#[derive(Default)]
pub(in crate::canvas) struct SceneGeometryCache(Option<Entry>);

struct Entry {
    scene: Rc<[Primitive]>,
    markers: Rc<Markers>,
    view: View,
    nmr_ids: Vec<u64>,
    // Model drawing reads frame-local tile sampling state; leave it live.
    geometry: Option<Vec<layered::RetainedGeometry>>,
}

#[derive(PartialEq, Eq)]
struct View {
    camera: [u32; 3],
    bounds: [u32; 4],
    frame: layered::FrameKey,
    reviewing_pair: bool,
}

impl SceneGeometryCache {
    pub(in crate::canvas) fn draw(
        &mut self,
        frame: &mut layered::Frame<'_>,
        rendered: (Rc<Markers>, Rc<[Primitive]>),
        camera: Camera,
        bounds: Rectangle,
        nmr_ids: &[u64],
        reviewing_pair: bool,
    ) {
        use iced::advanced::graphics::cache::{Cached, Group};
        let (markers, scene) = rendered;
        let view = View {
            camera: [
                camera.center.x.to_bits(),
                camera.center.y.to_bits(),
                camera.zoom.to_bits(),
            ],
            bounds: [
                bounds.x.to_bits(),
                bounds.y.to_bits(),
                bounds.width.to_bits(),
                bounds.height.to_bits(),
            ],
            frame: frame.key(),
            reviewing_pair,
        };
        let draw = |frame: &mut layered::Frame<'_>| {
            if !reviewing_pair {
                markers.draw_with_nmr(frame, camera, bounds, true, nmr_ids);
            }
            draw_primitives(frame, &scene, camera, bounds, 0.);
        };
        if !self.0.as_ref().is_some_and(|entry| {
            Rc::ptr_eq(&entry.scene, &scene)
                && Rc::ptr_eq(&entry.markers, &markers)
                && entry.view == view
                && entry.nmr_ids == nmr_ids
        }) {
            let geometry = (!scene.iter().any(models3d::is_model)).then(|| {
                let mut layer = frame.layer();
                draw(&mut layer);
                layer
                    .finish()
                    .into_iter()
                    .map(|geometry| geometry.cache(Group::unique(), None))
                    .collect()
            });
            self.0 = Some(Entry {
                scene: scene.clone(),
                markers: markers.clone(),
                view,
                nmr_ids: nmr_ids.to_vec(),
                geometry,
            });
        }
        if let Some(geometry) = self.0.as_ref().and_then(|entry| entry.geometry.as_ref()) {
            frame.retained(geometry);
        } else {
            draw(frame);
        }
    }
}
