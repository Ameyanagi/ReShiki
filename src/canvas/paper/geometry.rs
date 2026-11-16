//! Retain the paper's static selection markers and tessellated scene together.
//!
//! Panning reuses the tessellation, shifted: the first pan at a zoom level
//! tessellates half a view beyond each edge, so later pans within that margin
//! draw nothing new.
use super::super::{Camera, World, layered, markers::Markers, models3d, render::draw_primitives};
use iced::{Rectangle, Vector};
use reshiki::scene::Primitive;
use std::rc::Rc;

#[derive(Default)]
pub(in crate::canvas) struct SceneGeometryCache(Option<Entry>);

struct Entry {
    scene: Rc<[Primitive]>,
    markers: Rc<Markers>,
    view: View,
    nmr_ids: Vec<u64>,
    center: World,
    /// How far the view can pan, in pixels, before the edges need tessellating.
    margin: Vector,
    // Model drawing reads frame-local tile sampling state; leave it live.
    geometry: Option<Vec<layered::RetainedGeometry>>,
}

#[derive(PartialEq, Eq)]
struct View {
    zoom: u32,
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
            zoom: camera.zoom.to_bits(),
            bounds: [
                bounds.x.to_bits(),
                bounds.y.to_bits(),
                bounds.width.to_bits(),
                bounds.height.to_bits(),
            ],
            frame: frame.key(),
            reviewing_pair,
        };
        let draw = |frame: &mut layered::Frame<'_>, camera: Camera, bounds: Rectangle| {
            if !reviewing_pair {
                markers.draw_with_nmr(frame, camera, bounds, true, nmr_ids);
            }
            draw_primitives(frame, &scene, camera, bounds, 0.);
        };
        let same = self.0.as_ref().filter(|entry| {
            Rc::ptr_eq(&entry.scene, &scene)
                && Rc::ptr_eq(&entry.markers, &markers)
                && entry.view == view
                && entry.nmr_ids == nmr_ids
        });
        let shift = same.map(|entry| {
            Vector::new(
                (entry.center.x - camera.center.x) * camera.zoom,
                (entry.center.y - camera.center.y) * camera.zoom,
            )
        });
        let reuse = same.zip(shift).is_some_and(|(entry, shift)| {
            shift.x.abs() <= entry.margin.x && shift.y.abs() <= entry.margin.y
        });
        if !reuse {
            // Only a pan pays for the margin; edits and zooms tessellate the view.
            let margin = if shift.is_some() {
                Vector::new(bounds.width / 2., bounds.height / 2.)
            } else {
                Vector::ZERO
            };
            let geometry = (!scene.iter().any(models3d::is_model)).then(|| {
                let mut layer = frame.layer(margin);
                let wide = Rectangle {
                    width: bounds.width + 2. * margin.x,
                    height: bounds.height + 2. * margin.y,
                    ..bounds
                };
                draw(&mut layer, camera, wide);
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
                center: camera.center,
                margin,
                geometry,
            });
        }
        match self.0.as_ref() {
            Some(Entry {
                geometry: Some(geometry),
                center,
                ..
            }) => frame.retained(
                geometry,
                Vector::new(
                    (center.x - camera.center.x) * camera.zoom,
                    (center.y - camera.center.y) * camera.zoom,
                ),
            ),
            _ => draw(frame, camera, bounds),
        }
    }
}
