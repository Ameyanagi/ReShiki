//! The paper's dot grid and its retained geometry.

use super::{Camera, World, layered};
use iced::widget::canvas::{self, Path};
use iced::{Color, Point, Rectangle, Vector};

/// Grid dots on bond-length multiples from the world origin, with fainter half
/// steps once they are 12 px apart; none when the step is under 10 px. Each
/// dot is its screen position and whether it is on a whole step. Large views
/// thin the grid by powers of two to bound the work while zooming or panning.
pub(super) fn grid_dots(camera: Camera, bounds: Rectangle, step: f32) -> Vec<(Point, bool)> {
    let step = step * camera.zoom;
    let origin = camera.screen(World::default(), bounds);
    if !step.is_finite()
        || step < 10.
        || !origin.x.is_finite()
        || !origin.y.is_finite()
        || !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width <= 0.
        || bounds.height <= 0.
    {
        return vec![];
    }
    let mut halves = step / 2. >= 12.;
    let mut pitch = if halves { step / 2. } else { step };
    while (bounds.width / pitch + 1.) * (bounds.height / pitch + 1.) > 8192. {
        pitch *= 2.;
        halves = false;
    }
    let indices = |origin: f32, length: f32| {
        (-origin / pitch).ceil() as i64..=((length - origin) / pitch).floor() as i64
    };
    let mut dots = Vec::new();
    for i in indices(origin.x, bounds.width) {
        for j in indices(origin.y, bounds.height) {
            dots.push((
                Point::new(origin.x + i as f32 * pitch, origin.y + j as f32 * pitch),
                !halves || (i.rem_euclid(2) == 0 && j.rem_euclid(2) == 0),
            ));
        }
    }
    dots
}

#[derive(Default)]
pub(super) struct GridCache {
    key: Option<([f32; 4], bool)>,
    pub(super) geometry: canvas::Cache,
}

impl GridCache {
    /// Reuse tessellated geometry throughout object drags and pointer movement.
    /// The canvas bounds are also part of the geometry cache's own key.
    pub(super) fn draw(
        &mut self,
        frame: &mut layered::Frame<'_>,
        camera: Camera,
        bounds: Rectangle,
        step: f32,
        dark: bool,
    ) {
        let key = ([camera.center.x, camera.center.y, camera.zoom, step], dark);
        if self.key != Some(key) {
            self.geometry.clear();
            self.key = Some(key);
        }
        frame.cached(&self.geometry, |frame| {
            let dots = grid_dots(camera, bounds, step);
            for (major, radius, alpha) in [(false, 0.9, 0.3), (true, 1.3, 0.6)] {
                let dot = Path::circle(Point::ORIGIN, radius);
                let color = crate::appearance::color(dark, Color::from_rgba8(90, 104, 99, alpha));
                // Separate circles avoid the tessellator's cross-contour work
                // on a single path containing thousands of disconnected dots.
                for (point, _) in dots.iter().filter(|(_, m)| *m == major) {
                    frame.with_save(|frame| {
                        frame.translate(Vector::new(point.x, point.y));
                        frame.fill(&dot, color);
                    });
                }
            }
        });
    }
}
