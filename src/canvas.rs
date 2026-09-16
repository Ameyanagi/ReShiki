use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, Vector, mouse};
use moruno::{
    document::{Document, Point as World},
    scene::{Primitive, primitives},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool {
    Select,
    Bond(u8),
    Wedge,
    Hash,
    Wavy,
    Atom,
    Ring,
    Arrow,
    Text,
    Erase,
}
impl Tool {
    pub fn hint(self) -> &'static str {
        match self {
            Self::Select => "Drag to move or select · Double-click an atom to select its molecule",
            Self::Bond(_) => {
                "Click an endpoint to grow · Drag to draw · Click a bond to cycle single → double → triple"
            }
            Self::Wedge | Self::Hash | Self::Wavy => {
                "Click an endpoint to grow a chain · Drag to choose direction · Click a bond to change it"
            }
            Self::Atom => "Click to add an atom or replace an existing element",
            Self::Ring => {
                "Click to place a ring · Click a bond to fuse · Click an atom to share a vertex"
            }
            Self::Arrow => "Drag to draw a reaction arrow",
            Self::Text => "Enter a label above the canvas, then click to place it",
            Self::Erase => "Click an atom, bond, label, or arrow to erase",
        }
    }
    fn bond_order(self) -> Option<u8> {
        match self {
            Self::Bond(order) => Some(order),
            Self::Wedge | Self::Hash | Self::Wavy => Some(1),
            _ => None,
        }
    }
}
#[derive(Debug, Clone)]
pub enum Edit {
    Select(Vec<u64>),
    Move(Vec<u64>, f32, f32),
    Bond(World, World, Option<u64>, Option<u64>),
    Click(World),
    Pan(f32, f32),
    Zoom(f32, World),
}
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub center: World,
    pub zoom: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: World::default(),
            zoom: 1.7,
        }
    }
}
impl Camera {
    fn screen(self, p: World, bounds: Rectangle) -> Point {
        Point::new(
            (p.x - self.center.x) * self.zoom + bounds.width / 2.0,
            (p.y - self.center.y) * self.zoom + bounds.height / 2.0,
        )
    }
    fn world(self, p: Point, bounds: Rectangle) -> World {
        World::new(
            (p.x - bounds.width / 2.0) / self.zoom + self.center.x,
            (p.y - bounds.height / 2.0) / self.zoom + self.center.y,
        )
    }
}
#[derive(Default)]
pub struct State {
    gesture: Option<Gesture>,
    cursor: Option<Point>,
    last_click: Option<(std::time::Instant, u64)>,
}
#[derive(Debug)]
enum Gesture {
    Draw { start: World, id: Option<u64> },
    Move { start: World, ids: Vec<u64> },
    Select { start: World },
    Pan { last: Point },
}
pub struct MoleculeCanvas<'a> {
    pub doc: &'a Document,
    pub selected: &'a [u64],
    pub tool: Tool,
    pub camera: Camera,
    pub grid: bool,
}
fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

impl canvas::Program<Edit> for MoleculeCanvas<'_> {
    type State = State;
    fn update(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Edit>> {
        // Iced may dispatch a batch with the final cursor position. Preserve the
        // position carried by each motion event so fast drags retain their origin.
        if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
            state.cursor = Some(*position);
        }
        let point = state
            .cursor
            .or(cursor.position())
            .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
        let inside = point.is_some_and(|p| Rectangle::with_size(bounds.size()).contains(p));
        match event {
            Event::Window(iced::window::Event::Unfocused) => {
                state.gesture = None;
                state.last_click = None;
                Some(Action::request_redraw())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if inside => {
                let amount = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y * 0.12,
                    mouse::ScrollDelta::Pixels { y, .. } => *y * 0.003,
                };
                Some(
                    Action::publish(Edit::Zoom(amount.exp(), self.camera.world(point?, bounds)))
                        .and_capture(),
                )
            }
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right | mouse::Button::Middle,
            )) if inside => {
                state.gesture = Some(Gesture::Pan { last: point? });
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside => {
                let p = self.camera.world(point?, bounds);
                let hit = hit_object(self.doc, p, 10.0 / self.camera.zoom);
                state.gesture = match self.tool {
                    Tool::Select => Some(if let Some(id) = hit {
                        Gesture::Move {
                            start: p,
                            ids: if self.selected.contains(&id) {
                                self.selected.to_vec()
                            } else {
                                vec![id]
                            },
                        }
                    } else {
                        Gesture::Select { start: p }
                    }),
                    Tool::Bond(_) | Tool::Wedge | Tool::Hash | Tool::Wavy | Tool::Arrow => {
                        Some(Gesture::Draw {
                            start: p,
                            id: self.doc.nearest(p, 10.0 / self.camera.zoom),
                        })
                    }
                    _ => return Some(Action::publish(Edit::Click(p)).and_capture()),
                };
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(Gesture::Pan { last }) = &mut state.gesture {
                    let p = point?;
                    let delta = p - *last;
                    *last = p;
                    return Some(
                        Action::publish(Edit::Pan(
                            delta.x / self.camera.zoom,
                            delta.y / self.camera.zoom,
                        ))
                        .and_capture(),
                    );
                }
                if state.gesture.is_some() || inside {
                    Some(Action::request_redraw())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                let gesture = state.gesture.take()?;
                let p = self.camera.world(point?, bounds);
                let edit = match gesture {
                    Gesture::Draw { start, id } => {
                        if start.distance(p) < 3.0 / self.camera.zoom {
                            Edit::Click(p)
                        } else {
                            let origin = id
                                .and_then(|id| self.doc.atom(id).map(|a| a.position))
                                .unwrap_or(start);
                            let (end, target) = if self.tool == Tool::Arrow {
                                (p, None)
                            } else {
                                bond_target(self.doc, origin, p, id, 12.0 / self.camera.zoom)
                            };
                            Edit::Bond(origin, end, id, target)
                        }
                    }
                    Gesture::Move { start, ids } => {
                        if start.distance(p) < 1.0 / self.camera.zoom {
                            if let Some(atom) = self.doc.nearest(start, 10.0 / self.camera.zoom) {
                                let now = std::time::Instant::now();
                                if state.last_click.is_some_and(|(time, id)| {
                                    id == atom && now.duration_since(time).as_millis() < 450
                                }) {
                                    state.last_click = None;
                                    let connected =
                                        moruno::editing::groups(self.doc, &self.doc.all_ids())
                                            .into_iter()
                                            .find(|g| g.contains(&atom))
                                            .unwrap_or(ids);
                                    return Some(
                                        Action::publish(Edit::Select(connected)).and_capture(),
                                    );
                                }
                                state.last_click = Some((now, atom));
                            }
                            Edit::Select(ids)
                        } else {
                            state.last_click = None;
                            Edit::Move(ids, p.x - start.x, p.y - start.y)
                        }
                    }
                    Gesture::Select { start } => {
                        let min = World::new(start.x.min(p.x), start.y.min(p.y));
                        let max = World::new(start.x.max(p.x), start.y.max(p.y));
                        Edit::Select(
                            self.doc
                                .atoms
                                .iter()
                                .filter(|a| {
                                    a.position.x >= min.x
                                        && a.position.x <= max.x
                                        && a.position.y >= min.y
                                        && a.position.y <= max.y
                                })
                                .map(|a| a.id)
                                .chain(
                                    self.doc
                                        .annotations
                                        .iter()
                                        .filter(|a| {
                                            a.position.x >= min.x
                                                && a.position.x <= max.x
                                                && a.position.y >= min.y
                                                && a.position.y <= max.y
                                        })
                                        .map(|a| a.id),
                                )
                                .chain(
                                    self.doc
                                        .arrows
                                        .iter()
                                        .filter(|a| {
                                            a.start.x >= min.x
                                                && a.start.x <= max.x
                                                && a.start.y >= min.y
                                                && a.start.y <= max.y
                                        })
                                        .map(|a| a.id),
                                )
                                .collect(),
                        )
                    }
                    Gesture::Pan { .. } => return Some(Action::request_redraw()),
                };
                Some(Action::publish(edit).and_capture())
            }
            _ => None,
        }
    }
    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color::from_rgb8(253, 253, 250),
        );
        if self.grid {
            let spacing = 28.0 * self.camera.zoom;
            if spacing > 9.0 {
                let origin = self.camera.screen(World::default(), bounds);
                let mut x = origin.x.rem_euclid(spacing);
                while x < bounds.width {
                    let mut y = origin.y.rem_euclid(spacing);
                    while y < bounds.height {
                        frame.fill(
                            &Path::circle(Point::new(x, y), 0.7),
                            Color::from_rgb8(218, 226, 224),
                        );
                        y += spacing;
                    }
                    x += spacing;
                }
            }
        }
        let mut preview = self.doc.clone();
        if let (Some(Gesture::Move { start, ids }), Some(p)) = (&state.gesture, state.cursor) {
            let p = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            preview.translate(ids, p.x - start.x, p.y - start.y);
        }
        for id in self.selected {
            if let Some(atom) = preview.atom(*id) {
                frame.fill(
                    &Path::circle(self.camera.screen(atom.position, bounds), 10.0),
                    Color::from_rgba8(31, 145, 130, 0.20),
                );
            }
            for a in preview.annotations.iter().filter(|a| a.id == *id) {
                frame.stroke(
                    &Path::rectangle(
                        self.camera.screen(a.position, bounds),
                        iced::Size::new(
                            a.size().0 * self.camera.zoom,
                            a.size().1 * self.camera.zoom,
                        ),
                    ),
                    Stroke::default().with_color(Color::from_rgb8(20, 130, 112)),
                );
            }
            for a in preview.arrows.iter().filter(|a| a.id == *id) {
                for p in [a.start, a.end] {
                    frame.stroke(
                        &Path::circle(self.camera.screen(p, bounds), 5.0),
                        Stroke::default().with_color(rgb([19, 135, 116])),
                    );
                }
            }
        }
        for primitive in primitives(&preview) {
            match primitive {
                Primitive::Line(a, b, width) => frame.stroke(
                    &Path::line(self.camera.screen(a, bounds), self.camera.screen(b, bounds)),
                    Stroke::default()
                        .with_width(width * self.camera.zoom)
                        .with_color(Color::BLACK),
                ),
                Primitive::Polygon(points) => {
                    let path = Path::new(|builder| {
                        if let Some(p) = points.first() {
                            builder.move_to(self.camera.screen(*p, bounds));
                            for p in &points[1..] {
                                builder.line_to(self.camera.screen(*p, bounds));
                            }
                            builder.close();
                        }
                    });
                    frame.fill(&path, Color::BLACK);
                }
                Primitive::Text {
                    position,
                    text,
                    size,
                    color,
                } => {
                    let t = canvas::Text {
                        content: text,
                        position: self.camera.screen(position, bounds),
                        size: (size * self.camera.zoom).into(),
                        font: iced::Font::with_name(&moruno::style::DEFAULT.font_family),
                        line_height: iced::widget::text::LineHeight::Relative(1.0),
                        color: rgb(color),
                        shaping: iced::widget::text::Shaping::Advanced,
                        ..Default::default()
                    };
                    t.draw_with(|path, color| frame.fill(&path, color));
                }
            }
        }
        if let Some(p) = state.cursor {
            let p = Point::new(p.x - bounds.x, p.y - bounds.y);
            match &state.gesture {
                Some(Gesture::Draw { start, id }) => {
                    let origin = id
                        .and_then(|id| self.doc.atom(id).map(|a| a.position))
                        .unwrap_or(*start);
                    let end = self.camera.world(p, bounds);
                    let end = if let Some(order) = self.tool.bond_order() {
                        if start.distance(end) < 3.0 / self.camera.zoom {
                            moruno::editing::bond_extension(self.doc, origin, *id, order)
                        } else {
                            bond_target(self.doc, origin, end, *id, 12.0 / self.camera.zoom).0
                        }
                    } else {
                        end
                    };
                    frame.stroke(
                        &Path::line(
                            self.camera.screen(origin, bounds),
                            self.camera.screen(end, bounds),
                        ),
                        Stroke::default()
                            .with_width(2.0)
                            .with_color(rgb([19, 135, 116])),
                    );
                }
                Some(Gesture::Select { start }) => {
                    let a = self.camera.screen(*start, bounds);
                    let lo = Point::new(a.x.min(p.x), a.y.min(p.y));
                    let size = iced::Size::new((a.x - p.x).abs(), (a.y - p.y).abs());
                    frame.fill_rectangle(lo, size, Color::from_rgba8(19, 135, 116, 0.08));
                    frame.stroke(
                        &Path::rectangle(lo, size),
                        Stroke::default().with_color(rgb([19, 135, 116])),
                    );
                }
                _ => {
                    if cursor.is_over(bounds) {
                        let point = self.camera.world(p, bounds);
                        let atom = self.doc.nearest(point, 10.0 / self.camera.zoom);
                        let origin = atom
                            .and_then(|id| self.doc.atom(id))
                            .map(|a| a.position)
                            .unwrap_or(point);
                        if let Some(order) = self.tool.bond_order()
                            && (atom.is_some()
                                || moruno::editing::nearest_bond(
                                    self.doc,
                                    point,
                                    7.0 / self.camera.zoom,
                                )
                                .is_none())
                        {
                            let end =
                                moruno::editing::bond_extension(self.doc, origin, atom, order);
                            frame.stroke(
                                &Path::line(
                                    self.camera.screen(origin, bounds),
                                    self.camera.screen(end, bounds),
                                ),
                                Stroke::default()
                                    .with_width(1.5)
                                    .with_color(Color::from_rgba8(19, 135, 116, 0.5)),
                            );
                            frame.fill(
                                &Path::circle(self.camera.screen(end, bounds), 3.0),
                                Color::from_rgba8(19, 135, 116, 0.5),
                            );
                        }
                        if atom.is_some() {
                            frame.stroke(
                                &Path::circle(self.camera.screen(origin, bounds), 9.0),
                                Stroke::default().with_color(rgb([19, 135, 116])),
                            );
                        }
                    }
                }
            }
        }
        vec![frame.into_geometry()]
    }
    fn mouse_interaction(
        &self,
        _state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            if self.tool == Tool::Select {
                mouse::Interaction::default()
            } else {
                mouse::Interaction::Crosshair
            }
        } else {
            mouse::Interaction::default()
        }
    }
}
pub fn snap(start: World, end: World) -> World {
    let angle = ((end.y - start.y).atan2(end.x - start.x) / (std::f32::consts::PI / 6.0)).round()
        * (std::f32::consts::PI / 6.0);
    let length = moruno::style::DEFAULT.bond_length_world;
    start.offset(angle.cos() * length, angle.sin() * length)
}
/// Use the same attachment resolution for the drag preview and committed bond.
/// Excluding the source lets short drags grow a bond rather than snap to themselves.
fn bond_target(
    doc: &Document,
    start: World,
    end: World,
    source: Option<u64>,
    radius: f32,
) -> (World, Option<u64>) {
    let nearest = |p: World| {
        doc.atoms
            .iter()
            .filter(|a| Some(a.id) != source && a.position.distance(p) < radius)
            .min_by(|a, b| a.position.distance(p).total_cmp(&b.position.distance(p)))
    };
    let snapped = snap(start, end);
    if let Some(atom) = nearest(end).or_else(|| nearest(snapped)) {
        (atom.position, Some(atom.id))
    } else {
        (snapped, None)
    }
}
pub fn hit_object(doc: &Document, p: World, r: f32) -> Option<u64> {
    doc.nearest(p, r)
        .or_else(|| {
            doc.annotations
                .iter()
                .rev()
                .find(|a| {
                    p.x >= a.position.x
                        && p.x < a.position.x + a.size().0
                        && p.y >= a.position.y
                        && p.y < a.position.y + a.size().1
                })
                .map(|a| a.id)
        })
        .or_else(|| {
            doc.arrows
                .iter()
                .rev()
                .find(|a| {
                    if a.kind != "curved" {
                        return distance_to_segment(p, a.start, a.end) < r;
                    }
                    let control = World::new(
                        (a.start.x + a.end.x) / 2.0 - (a.end.y - a.start.y) * 0.5,
                        (a.start.y + a.end.y) / 2.0 + (a.end.x - a.start.x) * 0.5,
                    );
                    let mut prev = a.start;
                    for i in 1..=32 {
                        let t = i as f32 / 32.0;
                        let u = 1.0 - t;
                        let end = World::new(
                            u * u * a.start.x + 2.0 * u * t * control.x + t * t * a.end.x,
                            u * u * a.start.y + 2.0 * u * t * control.y + t * t * a.end.y,
                        );
                        if distance_to_segment(p, prev, end) < r {
                            return true;
                        }
                        prev = end;
                    }
                    false
                })
                .map(|a| a.id)
        })
}
pub fn distance_to_segment(p: World, a: World, b: World) -> f32 {
    let v = Vector::new(b.x - a.x, b.y - a.y);
    let len = v.x * v.x + v.y * v.y;
    if len < 0.001 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * v.x + (p.y - a.y) * v.y) / len).clamp(0.0, 1.0);
    p.distance(a.offset(v.x * t, v.y * t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;

    #[test]
    fn short_endpoint_drag_grows_instead_of_snapping_to_its_source() {
        let mut doc = Document::default();
        let source = doc.add_atom("C", World::default());
        let canvas = MoleculeCanvas {
            doc: &doc,
            selected: &[],
            tool: Tool::Bond(1),
            camera: Camera {
                center: World::default(),
                zoom: 1.0,
            },
            grid: false,
        };
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
        let mut state = State::default();
        let end = Point::new(206.0, 150.0);
        let cursor = mouse::Cursor::Available(end);
        for event in [
            mouse::Event::CursorMoved {
                position: Point::new(200.0, 150.0),
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved { position: end },
        ] {
            canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
        }
        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap();
        let Some(Edit::Bond(start, end, Some(id), None)) = action.into_inner().0 else {
            panic!("short drag must extend, not connect the source to itself");
        };
        assert_eq!(id, source);
        assert_eq!(start, World::default());
        assert_eq!(end, World::new(42.0, 0.0));
    }

    #[test]
    fn drag_reuses_atoms_at_the_cursor_or_the_snapped_endpoint() {
        let mut doc = Document::default();
        let source = doc.add_atom("C", World::default());
        let target = doc.add_atom("C", World::new(42.0, 0.0));
        for cursor in [World::new(42.0, 3.0), World::new(80.0, 0.0)] {
            let (end, id) = bond_target(&doc, World::default(), cursor, Some(source), 12.0);
            assert_eq!(id, Some(target));
            assert_eq!(end, World::new(42.0, 0.0));
        }
    }

    #[test]
    fn fast_drag_uses_each_motion_event_instead_of_final_cursor_snapshot() {
        let doc = Document::default();
        let canvas = MoleculeCanvas {
            doc: &doc,
            selected: &[],
            tool: Tool::Bond(1),
            camera: Camera {
                center: World::default(),
                zoom: 1.0,
            },
            grid: false,
        };
        let mut state = State::default();
        let bounds = Rectangle {
            x: 200.0,
            y: 100.0,
            width: 400.0,
            height: 300.0,
        };
        let origin = Point::new(350.0, 250.0);
        let end = Point::new(410.0, 250.0);
        let snapshot = mouse::Cursor::Available(end);
        for event in [
            mouse::Event::CursorMoved { position: origin },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved { position: end },
        ] {
            canvas.update(&mut state, &Event::Mouse(event), bounds, snapshot);
        }
        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                snapshot,
            )
            .unwrap();
        let Some(Edit::Bond(start, end, None, None)) = action.into_inner().0 else {
            panic!("drag must produce a bond, not a click");
        };
        assert_eq!(start, World::new(-50.0, 0.0));
        assert_eq!(end, World::new(-8.0, 0.0));
    }
}
