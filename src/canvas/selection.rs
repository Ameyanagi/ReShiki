use super::{Camera, World, rgb};
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::{Color, Point, Rectangle, Size, mouse};
use reshiki::{document::Document, editing, scene};

#[derive(Clone, Copy, Debug)]
pub(super) enum Handle {
    Resize(usize),
    Edge(usize),
    Rotate,
}

impl Handle {
    pub fn cursor(self) -> mouse::Interaction {
        match self {
            Self::Rotate => mouse::Interaction::Grab,
            Self::Resize(0 | 2) => mouse::Interaction::ResizingDiagonallyDown,
            Self::Resize(_) => mouse::Interaction::ResizingDiagonallyUp,
            Self::Edge(0 | 2) => mouse::Interaction::ResizingVertically,
            Self::Edge(_) => mouse::Interaction::ResizingHorizontally,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SelectionBox {
    corners: [World; 4],
    pub pivot: World,
    camera: Camera,
    bounds: Rectangle,
}

impl SelectionBox {
    pub fn new(doc: &Document, ids: &[u64], camera: Camera, bounds: Rectangle) -> Option<Self> {
        if matches!(ids, [id] if doc.atom(*id).is_some()) {
            return None;
        }
        let (lo, hi) = scene::selection_bounds(doc, ids)?;
        Some(Self {
            corners: [lo, World::new(hi.x, lo.y), hi, World::new(lo.x, hi.y)],
            pivot: editing::center(doc, ids),
            camera,
            bounds,
        })
    }

    fn grips(self) -> [Point; 4] {
        let offsets = [(-12.0, -12.0), (12.0, -12.0), (12.0, 12.0), (-12.0, 12.0)];
        let mut pairs = self.corners.into_iter().zip(offsets);
        std::array::from_fn(|_| {
            let (corner, (x, y)) = pairs.next().unwrap_or((self.pivot, (0., 0.)));
            let p = self.camera.screen(corner, self.bounds);
            Point::new(p.x + x, p.y + y)
        })
    }

    fn rotation_grip(self) -> Point {
        let p = self.grips();
        Point::new((p[0].x + p[1].x) / 2.0, p[0].y - 28.0)
    }

    fn edge_points(self) -> [World; 4] {
        let [a, b, c, d] = self.corners;
        [
            World::new((a.x + b.x) / 2., a.y),
            World::new(b.x, (b.y + c.y) / 2.),
            World::new((c.x + d.x) / 2., c.y),
            World::new(a.x, (a.y + d.y) / 2.),
        ]
    }

    fn edge_grips(self) -> [Point; 4] {
        let [a, b, c, d] = self.grips();
        [
            Point::new((a.x + b.x) / 2., a.y),
            Point::new(b.x, (b.y + c.y) / 2.),
            Point::new((c.x + d.x) / 2., c.y),
            Point::new(a.x, (a.y + d.y) / 2.),
        ]
    }

    fn edge_enabled(self, edge: usize) -> bool {
        let [a, _, c, _] = self.corners;
        let width = (c.x - a.x).abs();
        let height = (c.y - a.y).abs();
        if edge.is_multiple_of(2) {
            height > 0.001
        } else {
            width > 0.001
        }
    }

    pub fn hit(self, p: Point) -> Option<Handle> {
        if p.distance(self.rotation_grip()) < 10.0 {
            return Some(Handle::Rotate);
        }
        self.grips()
            .iter()
            .position(|q| (p.x - q.x).abs() < 8.0 && (p.y - q.y).abs() < 8.0)
            .map(Handle::Resize)
            .or_else(|| {
                self.edge_grips()
                    .iter()
                    .enumerate()
                    .find(|(i, q)| {
                        self.edge_enabled(*i) && (p.x - q.x).abs() < 8. && (p.y - q.y).abs() < 8.
                    })
                    .map(|(i, _)| Handle::Edge(i))
            })
    }

    /// During rotation the box and its handle rotate with the preview.
    pub fn draw(self, frame: &mut Frame, rotation: f32) {
        let pivot = self.camera.screen(self.pivot, self.bounds);
        let (s, c) = rotation.to_radians().sin_cos();
        let rotate = |p: Point| {
            Point::new(
                pivot.x + (p.x - pivot.x) * c - (p.y - pivot.y) * s,
                pivot.y + (p.x - pivot.x) * s + (p.y - pivot.y) * c,
            )
        };
        let corners = self.grips().map(rotate);
        let outline = Path::new(|p| {
            p.move_to(corners[0]);
            for corner in &corners[1..] {
                p.line_to(*corner);
            }
            p.close();
        });
        let stroke = Stroke::default()
            .with_width(1.2)
            .with_color(rgb([19, 135, 116]));
        frame.stroke(&outline, stroke);
        for p in corners {
            let handle = Path::rectangle(Point::new(p.x - 4.0, p.y - 4.0), Size::new(8.0, 8.0));
            frame.fill(&handle, Color::WHITE);
            frame.stroke(&handle, stroke);
        }
        for (i, p) in self.edge_grips().into_iter().enumerate() {
            if self.edge_enabled(i) {
                let p = rotate(p);
                let handle = Path::rectangle(Point::new(p.x - 3., p.y - 3.), Size::new(6., 6.));
                frame.fill(&handle, Color::WHITE);
                frame.stroke(&handle, stroke);
            }
        }
        let top = Point::new(
            (corners[0].x + corners[1].x) / 2.0,
            (corners[0].y + corners[1].y) / 2.0,
        );
        let grip = rotate(self.rotation_grip());
        frame.stroke(&Path::line(top, grip), stroke);
        let handle = Path::circle(grip, 6.0);
        frame.fill(&handle, Color::WHITE);
        frame.stroke(&handle, stroke.with_width(1.8));
        frame.fill(&Path::circle(grip, 2.0), rgb([19, 135, 116]));
    }
}

#[derive(Debug)]
pub(super) struct TransformDrag {
    pub ids: Vec<u64>,
    pub pivot: World,
    pub handle: Handle,
    pub selection: SelectionBox,
    start: World,
    corner: World,
    offset: World,
}

impl TransformDrag {
    pub fn new(selection: SelectionBox, handle: Handle, start: World, ids: &[u64]) -> Self {
        let (pivot, corner) = match handle {
            Handle::Rotate => (selection.pivot, start),
            Handle::Resize(i) => (
                selection
                    .corners
                    .get((i % 4 + 2) % 4)
                    .copied()
                    .unwrap_or(selection.pivot),
                selection.corners.get(i).copied().unwrap_or(start),
            ),
            Handle::Edge(i) => (
                selection
                    .edge_points()
                    .get((i % 4 + 2) % 4)
                    .copied()
                    .unwrap_or(selection.pivot),
                selection.edge_points().get(i).copied().unwrap_or(start),
            ),
        };
        Self {
            ids: ids.to_vec(),
            pivot,
            handle,
            selection,
            start,
            corner,
            offset: World::new(start.x - corner.x, start.y - corner.y),
        }
    }

    pub fn values(&self, end: World, snap_angle: bool) -> (f32, f32) {
        match self.handle {
            Handle::Rotate => {
                if end.distance(self.pivot) < 0.001 {
                    return (1.0, 0.0);
                }
                let a = (self.start.y - self.pivot.y).atan2(self.start.x - self.pivot.x);
                let b = (end.y - self.pivot.y).atan2(end.x - self.pivot.x);
                let angle = ((b - a).to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
                (
                    1.0,
                    if snap_angle {
                        (angle / 15.0).round() * 15.0
                    } else {
                        angle
                    },
                )
            }
            Handle::Resize(_) => {
                let x = self.corner.x - self.pivot.x;
                let y = self.corner.y - self.pivot.y;
                let length = x * x + y * y;
                if length < 0.001 {
                    return (1.0, 0.0);
                }
                let factor = ((end.x - self.offset.x - self.pivot.x) * x
                    + (end.y - self.offset.y - self.pivot.y) * y)
                    / length;
                (factor.clamp(0.05, 20.0), 0.0)
            }
            Handle::Edge(i) => {
                let (span, moved) = if i.is_multiple_of(2) {
                    (
                        self.corner.y - self.pivot.y,
                        end.y - self.offset.y - self.pivot.y,
                    )
                } else {
                    (
                        self.corner.x - self.pivot.x,
                        end.x - self.offset.x - self.pivot.x,
                    )
                };
                let factor = if span.abs() < 0.001 { 1. } else { moved / span };
                (factor.clamp(0.05, 20.), 0.)
            }
        }
    }

    pub fn apply(&self, doc: &mut Document, end: World, snap_angle: bool) {
        let (scale, rotation) = self.values(end, snap_angle);
        if let Handle::Edge(i) = self.handle {
            let (x, y) = if i.is_multiple_of(2) {
                (1., scale)
            } else {
                (scale, 1.)
            };
            editing::scale_axes_about(doc, &self.ids, self.pivot, x, y);
        } else {
            editing::transform_about(doc, &self.ids, self.pivot, scale, rotation);
        }
    }

    pub fn into_edit(self, end: World, snap_angle: bool) -> super::Edit {
        let (scale, rotation) = self.values(end, snap_angle);
        if let Handle::Edge(i) = self.handle {
            let (x, y) = if i.is_multiple_of(2) {
                (1., scale)
            } else {
                (scale, 1.)
            };
            super::Edit::ScaleAxes {
                ids: self.ids,
                pivot: self.pivot,
                x,
                y,
            }
        } else {
            super::Edit::Transform {
                ids: self.ids,
                pivot: self.pivot,
                scale,
                rotation,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_handles_stretch_one_axis_and_match_committed_geometry() -> Result<(), String> {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::new(-20., -10.));
        let b = doc.add_atom("C", World::new(20., 10.));
        doc.add_bond(a, b, 1, "plain");
        for zoom in [0.5, 1., 3.] {
            let camera = Camera {
                center: World::new(30., -15.),
                zoom,
            };
            let bounds = Rectangle::new(Point::new(80., 100.), Size::new(400., 300.));
            let selection =
                SelectionBox::new(&doc, &[a, b], camera, bounds).ok_or("Selection box")?;
            for (i, grip) in selection.edge_grips().into_iter().enumerate() {
                assert!(matches!(selection.hit(grip), Some(Handle::Edge(edge)) if edge == i));
                let start = camera.world(Point::new(grip.x + 2., grip.y - 2.), bounds);
                let drag = TransformDrag::new(selection, Handle::Edge(i), start, &[a, b]);
                assert_eq!(drag.values(start, false), (1., 0.));
                let end = start.offset(drag.corner.x - drag.pivot.x, drag.corner.y - drag.pivot.y);
                assert!((drag.values(end, false).0 - 2.).abs() < 0.001);
                let mut preview = doc.clone();
                drag.apply(&mut preview, end, false);
                let edit = drag.into_edit(end, false);
                let super::super::Edit::ScaleAxes { ids, pivot, x, y } = edit else {
                    return Err("Side handle did not emit an axis resize".into());
                };
                let mut committed = doc.clone();
                editing::scale_axes_about(&mut committed, &ids, pivot, x, y);
                assert_eq!(preview, committed);
                for atom in &preview.atoms {
                    let old = doc.atom(atom.id).ok_or("Original atom")?;
                    if i.is_multiple_of(2) {
                        assert_eq!(atom.position.x, old.position.x);
                    } else {
                        assert_eq!(atom.position.y, old.position.y);
                    }
                }
                assert_eq!(preview.bonds, doc.bonds);
                assert_eq!(if i.is_multiple_of(2) { x } else { y }, 1.);
            }
        }
        Ok(())
    }

    #[test]
    fn handles_keep_grab_offsets_and_snap_rotation_without_reflecting() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::new(-20.0, -10.0));
        let b = doc.add_atom("C", World::new(20.0, 10.0));
        doc.add_bond(a, b, 1, "plain");
        for zoom in [0.5, 1.0, 3.0] {
            let camera = Camera {
                center: World::new(30.0, -15.0),
                zoom,
            };
            let bounds = Rectangle::new(Point::new(80.0, 100.0), Size::new(400.0, 300.0));
            let selection = SelectionBox::new(&doc, &[a, b], camera, bounds).unwrap();
            let grip = selection.grips()[2];
            let start = camera.world(Point::new(grip.x + 3.0, grip.y - 2.0), bounds);
            let drag = TransformDrag::new(selection, Handle::Resize(2), start, &[a, b]);
            assert_eq!(drag.values(start, false), (1.0, 0.0));
            assert!((drag.values(start.offset(40.0, 20.0), false).0 - 2.0).abs() < 0.001);
            assert!(drag.values(start.offset(-100.0, -100.0), false).0 > 0.0);
            let start = camera.world(selection.rotation_grip(), bounds);
            let drag = TransformDrag::new(selection, Handle::Rotate, start, &[a, b]);
            let angle = 22.0_f32.to_radians();
            let radius = start.distance(drag.pivot);
            let end = drag
                .pivot
                .offset(radius * angle.sin(), -radius * angle.cos());
            assert!((drag.values(end, false).1 - 22.0).abs() < 0.001);
            assert_eq!(drag.values(end, true), (1.0, 15.0));
        }
    }
}
