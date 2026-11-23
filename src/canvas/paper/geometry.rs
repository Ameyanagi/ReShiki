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
    // Raster sampling is frame-local; retain the vector layers around each run.
    layers: Vec<Layer>,
    raster: Vec<models3d::PreparedRun>,
}

#[cfg(test)]
mod tests;

enum Layer {
    Retained(Vec<layered::RetainedGeometry>),
    Raster {
        range: std::ops::Range<usize>,
        run: usize,
    },
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
            let wide = Rectangle {
                width: bounds.width + 2. * margin.x,
                height: bounds.height + 2. * margin.y,
                ..bounds
            };
            let retain = |layer: layered::Frame<'_>| {
                Layer::Retained(
                    layer
                        .finish()
                        .into_iter()
                        .map(|geometry| geometry.cache(Group::unique(), None))
                        .collect(),
                )
            };
            let mut layers = Vec::new();
            // View/selection changes need new vector geometry, but an unchanged
            // immutable scene keeps each raster run's bounds and content hashes.
            let mut raster = self
                .0
                .as_mut()
                .filter(|entry| Rc::ptr_eq(&entry.scene, &scene))
                .map(|entry| std::mem::take(&mut entry.raster))
                .unwrap_or_default();
            let mut raster_index = 0;
            let mut vector = frame.layer(margin);
            if !reviewing_pair {
                markers.draw_with_nmr(&mut vector, camera, wide, true, nmr_ids);
            }
            let mut start = 0;
            for run in scene.chunk_by(|a, b| models3d::is_model(a) == models3d::is_model(b)) {
                let end = start + run.len();
                if run.first().is_some_and(models3d::is_model) {
                    layers.push(retain(vector));
                    if raster_index == raster.len() {
                        raster.push(models3d::PreparedRun::new(run));
                    }
                    layers.push(Layer::Raster {
                        range: start..end,
                        run: raster_index,
                    });
                    raster_index += 1;
                    vector = frame.layer(margin);
                } else {
                    draw_primitives(&mut vector, run, camera, wide, 0.);
                }
                start = end;
            }
            layers.push(retain(vector));
            self.0 = Some(Entry {
                scene: scene.clone(),
                markers: markers.clone(),
                view,
                nmr_ids: nmr_ids.to_vec(),
                center: camera.center,
                margin,
                layers,
                raster,
            });
        }
        let entry = self.0.as_mut().expect("The scene was retained above");
        let shift = Vector::new(
            (entry.center.x - camera.center.x) * camera.zoom,
            (entry.center.y - camera.center.y) * camera.zoom,
        );
        for layer in &entry.layers {
            match layer {
                Layer::Retained(geometry) => frame.retained(geometry, shift),
                Layer::Raster { range, run } => {
                    entry.raster[*run].draw(frame, &entry.scene[range.clone()], camera, bounds, 1.);
                }
            }
        }
    }
}
