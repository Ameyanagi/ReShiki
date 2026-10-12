//! Editing aids drawn over the document: handles, notices and badges, gesture outlines and hover, the erase cursor, the selection box and tilt bounds.

use crate::canvas::hit::{bond_target_with, hit_selection};
use crate::canvas::overlay::{ACCENT, ACCENT_TINT, ARROW_SOURCE};
use crate::canvas::{
    Gesture, Handle, MoleculeCanvas, SelectionBox, State, Tool, World, layered, markers, pick, rgb,
};
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Vector, keyboard, mouse};
use reshiki::arrows::ArrowHandle;
use reshiki::document::Document;
use reshiki::graphics::{Graphic, GraphicKind, PathCommand, PathHandle};
use std::ops::ControlFlow;

enum HandleShape {
    Circle(f32),
    Square,
    Diamond,
}

fn draw_handle(frame: &mut layered::Frame<'_>, point: Point, shape: HandleShape, chosen: bool) {
    let path = match shape {
        HandleShape::Circle(radius) => Path::circle(point, radius),
        HandleShape::Square => {
            Path::rectangle(point - Vector::new(4., 4.), iced::Size::new(8., 8.))
        }
        HandleShape::Diamond => Path::new(|builder| {
            builder.move_to(point - Vector::new(0., 5.));
            builder.line_to(point + Vector::new(5., 0.));
            builder.line_to(point + Vector::new(0., 5.));
            builder.line_to(point - Vector::new(5., 0.));
            builder.close();
        }),
    };
    frame.fill(
        &path,
        if chosen {
            ACCENT
        } else if matches!(shape, HandleShape::Square) {
            ACCENT_TINT
        } else {
            Color::WHITE
        },
    );
    frame.stroke(
        &path,
        Stroke::default()
            .with_width(if chosen { 2. } else { 1.5 })
            .with_color(ACCENT),
    );
}

fn draw_direction_line(frame: &mut layered::Frame<'_>, from: Point, to: Point) {
    frame.stroke(
        &Path::line(from, to),
        Stroke::default().with_width(1.).with_color(ACCENT),
    );
}

impl MoleculeCanvas<'_> {
    pub(super) fn scientific_drawing(
        &self,
        kind: GraphicKind,
        modifiers: keyboard::Modifiers,
    ) -> reshiki::scientific::Drawing {
        reshiki::scientific::Drawing {
            kind,
            style: self.graphics.style.clone(),
            phase: self.graphics.phase,
            flipped: self.graphics.flipped,
            attach: self.graphics.attach,
            snap_orbitals: self.graphics.snap && !modifiers.alt(),
        }
    }

    pub(super) fn draw_arrow_targets(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) {
        if !self.attaching_arrows(state.modifiers) {
            return;
        }
        let pointer = state
            .cursor
            .or(cursor.position())
            .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
        let hovered = pointer.and_then(|p| {
            reshiki::arrow_anchors::pick(
                self.doc,
                self.camera.world(p, bounds),
                self.camera.world_len(pick::ARROW_TARGET_PICK_PX),
            )
        });
        for (target, color, radius) in [
            (self.arrows.source, ARROW_SOURCE, 10.),
            (hovered.as_ref(), ACCENT, 8.),
        ] {
            if let Some((target, center)) =
                target.and_then(|t| t.center(self.doc).map(|center| (t, center)))
            {
                // Surround the actual label/dot group instead of painting a
                // small ring through the lone-pair dots at high zoom.
                let extent = match target {
                    reshiki::arrow_anchors::Pick::LonePair { atom, mark } => {
                        self.doc.atom(*atom).map(|a| {
                            let size = mark.size_pt.unwrap_or_else(|| {
                                a.text_style
                                    .as_ref()
                                    .map_or(self.doc.drawing_style.font_size_pt, |s| s.size_pt)
                                    * 0.75
                            });
                            reshiki::style::DEFAULT.world(size) * 0.35
                                + self.doc.drawing_style.line_width() * 0.5
                        })
                    }
                    reshiki::arrow_anchors::Pick::Atom(id) => self
                        .doc
                        .atom(*id)
                        .and_then(|a| reshiki::scene::atom_label_bounds(a, self.doc))
                        .map(|(lo, hi)| {
                            [lo, hi, World::new(lo.x, hi.y), World::new(hi.x, lo.y)]
                                .into_iter()
                                .map(|p| p.distance(center))
                                .fold(0., f32::max)
                        }),
                    _ => None,
                };
                let radius = extent.map_or(radius, |extent| {
                    (extent * self.camera.zoom + if radius > 8. { 6. } else { 3. }).max(radius)
                });
                frame.stroke(
                    &Path::circle(self.camera.screen(center, bounds), radius),
                    Stroke::default().with_width(1.5).with_color(color),
                );
            }
        }
    }

    pub(super) fn draw_orbital_target(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Tool::Graphic(kind @ reshiki::graphics::GraphicKind::Orbital(_)) = self.tool else {
            return;
        };
        let start = match state.gesture {
            Some(Gesture::Graphic { start }) => start,
            None => {
                let Some(cursor) = state.cursor.filter(|cursor| bounds.contains(*cursor)) else {
                    return;
                };
                self.camera
                    .world(Point::new(cursor.x - bounds.x, cursor.y - bounds.y), bounds)
            }
            _ => return,
        };
        let drawing = self.scientific_drawing(kind, state.modifiers);
        if let Some(id) =
            drawing.orbital_target(self.doc, start, self.camera.world_len(pick::PICK_RADIUS_PX))
            && let Some(atom) = self.doc.atom(id)
        {
            let center = self.camera.screen(atom.position, bounds);
            frame.stroke(
                &Path::circle(center, 8.),
                Stroke::default().with_width(1.5).with_color(ACCENT),
            );
        }
    }

    pub(super) fn draw_editor_markers(
        &self,
        frame: &mut layered::Frame<'_>,
        preview: &Document,
        bounds: Rectangle,
    ) {
        // Editing aids stay out of the shared scene used by figure/Office export.
        for atom in reshiki::attachments::editor_markers(preview) {
            let center = self.camera.screen(atom.position, bounds);
            let stroke = Stroke::default().with_width(1.2).with_color(ACCENT);
            frame.stroke(&Path::circle(center, 4.), stroke);
            for delta in [Vector::new(6., 0.), Vector::new(0., 6.)] {
                frame.stroke(&Path::line(center - delta, center + delta), stroke);
            }
        }
        if let Tool::StretchBond { fixed, moving } = self.tool
            && let Some((a, b)) = preview.atom(fixed).zip(preview.atom(moving))
        {
            let a = self.camera.screen(a.position, bounds);
            let b = self.camera.screen(b.position, bounds);
            let stroke = Stroke::default().with_width(1.5).with_color(ACCENT);
            frame.stroke(&Path::line(a, b), stroke);
            frame.stroke(
                &Path::rectangle(a - Vector::new(4., 4.), iced::Size::new(8., 8.)),
                stroke,
            );
            frame.stroke(&Path::circle(b, 5.), stroke);
        }
    }

    pub(super) fn draw_arrow_handles(
        &self,
        frame: &mut layered::Frame<'_>,
        preview: &Document,
        selected: &[u64],
        bounds: Rectangle,
    ) {
        if selected.len() == 1
            && (self.tool.selects() || matches!(self.tool, Tool::Arrow | Tool::EditPoints))
        {
            for a in preview.arrows.iter().filter(|a| selected.contains(&a.id)) {
                if let Some([departure, arrival]) = a.bezier_controls() {
                    for (endpoint, control) in [(a.start, departure), (a.end, arrival)] {
                        draw_direction_line(
                            frame,
                            self.camera.screen(endpoint, bounds),
                            self.camera.screen(control, bounds),
                        );
                    }
                }
                for (handle, p) in a.handles().typed() {
                    let p = self.camera.screen(p, bounds);
                    let shape = match handle {
                        ArrowHandle::Departure | ArrowHandle::Arrival => HandleShape::Square,
                        ArrowHandle::Bend => HandleShape::Diamond,
                        ArrowHandle::Start | ArrowHandle::End => HandleShape::Circle(4.),
                    };
                    draw_handle(frame, p, shape, false);
                }
            }
        }
    }

    pub(super) fn draw_notices(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
        template_notice: Option<(String, bool)>,
        chain_badge: Option<(World, String, bool)>,
        rejection: Option<reshiki::editing::RingRejection>,
    ) {
        if let Some((content, valid)) = template_notice {
            let position = Point::new(14., (bounds.height - 30.).max(4.));
            frame.fill_rectangle(
                position - Vector::new(5., 4.),
                iced::Size::new((bounds.width - 18.).max(1.), 25.),
                if valid {
                    ACCENT_TINT
                } else {
                    rgb([253, 235, 233])
                },
            );
            frame.fill_text(canvas::Text {
                content,
                position,
                color: rgb(if valid { [30, 100, 85] } else { [160, 50, 45] }),
                size: 12.into(),
                ..Default::default()
            });
        }
        if let Some((point, content, valid)) = chain_badge {
            badge(
                frame,
                bounds,
                self.camera.screen(point, bounds),
                content,
                valid,
            );
        }
        // A rejected ring shows where it would go, and why, at the pointer;
        // the status bar gets the full message if the user clicks anyway.
        if let (Some(rejection), Some(p)) = (rejection, state.cursor) {
            let error = rgb([182, 66, 61]);
            let pointer = Point::new(p.x - bounds.x, p.y - bounds.y);
            if let [first, rest @ ..] = rejection.outline.as_slice() {
                let path = Path::new(|b| {
                    b.move_to(self.camera.screen(*first, bounds));
                    for point in rest {
                        b.line_to(self.camera.screen(*point, bounds));
                    }
                    b.close();
                });
                frame.fill(&path, Color::from_rgba8(182, 66, 61, 0.08));
                frame.stroke(&path, Stroke::default().with_width(2.).with_color(error));
            }
            let marker = rejection
                .atom
                .and_then(|id| self.doc.atom(id))
                .map(|a| self.camera.screen(a.position, bounds))
                .or(rejection.outline.is_empty().then_some(pointer));
            if let Some(center) = marker {
                frame.stroke(
                    &Path::circle(center, 10.),
                    Stroke::default().with_width(2.).with_color(error),
                );
            }
            badge(frame, bounds, pointer, rejection.label, false);
        }
    }

    pub(super) fn draw_edit_points(
        &self,
        frame: &mut layered::Frame<'_>,
        preview: &Document,
        selected: &[u64],
        bounds: Rectangle,
    ) {
        let edit_points = self.tool == Tool::EditPoints;
        if !edit_points && self.tool != Tool::Graphic(GraphicKind::Path) {
            return;
        }
        if edit_points {
            for indicator in reshiki::atom_labels::indicators(preview)
                .into_iter()
                .filter(|i| i.owner.selected(selected))
            {
                if let Some(anchor) = indicator.owner.anchor(preview) {
                    let center = self.camera.screen(indicator.center, bounds);
                    frame.stroke(
                        &Path::line(self.camera.screen(anchor, bounds), center),
                        Stroke::default().with_color(rgb([120, 170, 155])),
                    );
                    draw_handle(frame, center, HandleShape::Circle(4.), false);
                }
            }
            for a in preview
                .atoms
                .iter()
                .filter(|a| selected.contains(&a.id) && preview.atom_visible(a.id))
            {
                for m in &a.marks {
                    let center = self
                        .camera
                        .screen(a.position.offset(m.offset.x, m.offset.y), bounds);
                    frame.stroke(
                        &Path::line(self.camera.screen(a.position, bounds), center),
                        Stroke::default().with_color(rgb([120, 170, 155])),
                    );
                    draw_handle(frame, center, HandleShape::Circle(4.), false);
                }
            }
        }
        for graphic in preview.graphics.iter().filter(|g| selected.contains(&g.id)) {
            if let Some(handles) = graphic.path_handles() {
                self.draw_path_handles(frame, graphic, handles, bounds, edit_points);
            } else if edit_points {
                self.draw_graphic_points(frame, graphic, bounds);
            }
        }
    }

    fn draw_path_handles(
        &self,
        frame: &mut layered::Frame<'_>,
        graphic: &Graphic,
        handles: Vec<PathHandle>,
        bounds: Rectangle,
        controls: bool,
    ) {
        if controls {
            let mut anchor = World::default();
            for command in graphic.commands() {
                if let PathCommand::Cubic(a, b, end) = command {
                    for (from, to) in [(anchor, a), (end, b)] {
                        draw_direction_line(
                            frame,
                            self.camera.screen(from, bounds),
                            self.camera.screen(to, bounds),
                        );
                    }
                    anchor = end;
                } else if let Some(point) = command.points().last() {
                    anchor = *point;
                }
            }
        }
        for handle in handles.into_iter().filter(|h| controls || h.node) {
            let point = self.camera.screen(handle.point, bounds);
            let shape = if handle.node {
                HandleShape::Circle(4.5)
            } else {
                HandleShape::Square
            };
            let chosen = self.graphics.point == Some((graphic.id, handle.index));
            draw_handle(frame, point, shape, chosen);
        }
    }

    fn draw_graphic_points(
        &self,
        frame: &mut layered::Frame<'_>,
        graphic: &Graphic,
        bounds: Rectangle,
    ) {
        if graphic.kind == GraphicKind::Arc {
            for p in graphic.edit_points() {
                draw_handle(
                    frame,
                    self.camera.screen(p, bounds),
                    HandleShape::Circle(5.0),
                    false,
                );
            }
            return;
        }
        let mut anchor = World::default();
        for c in graphic.commands() {
            let points = c.points();
            if let PathCommand::Cubic(a, b, end) = c {
                for (from, to) in [(anchor, a), (b, end)] {
                    draw_direction_line(
                        frame,
                        self.camera.screen(from, bounds),
                        self.camera.screen(to, bounds),
                    );
                }
                anchor = end;
            } else if let Some(p) = points.last() {
                anchor = *p;
            }
            for p in points {
                draw_handle(
                    frame,
                    self.camera.screen(p, bounds),
                    HandleShape::Circle(4.0),
                    false,
                );
            }
        }
    }

    pub(super) fn draw_gesture_overlay(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> ControlFlow<()> {
        if let Some(p) = state.cursor {
            let p = Point::new(p.x - bounds.x, p.y - bounds.y);
            match &state.gesture {
                Some(Gesture::Draw { start, id })
                    if self.tool.bond_preset().is_none()
                        && self.tool != Tool::Arrow
                        && self.tool != Tool::Atom =>
                {
                    let origin = id
                        .and_then(|id| self.doc.atom(id).map(|a| a.position))
                        .unwrap_or(*start);
                    let end = self.camera.world(p, bounds);
                    // A preview exists only during an actual drag, never while idle.
                    if start.distance(end) < self.camera.world_len(pick::DRAG_SLOP_PX) {
                        return ControlFlow::Break(());
                    }
                    let end = if self.tool == Tool::Arrow {
                        end
                    } else {
                        bond_target_with(
                            self.doc,
                            origin,
                            end,
                            *id,
                            12.0 / self.camera.zoom,
                            self.bond_drawing.unconstrained(state.modifiers.alt()),
                        )
                        .0
                    };
                    frame.stroke(
                        &Path::line(
                            self.camera.screen(origin, bounds),
                            self.camera.screen(end, bounds),
                        ),
                        Stroke::default().with_width(2.0).with_color(ACCENT),
                    );
                }
                Some(Gesture::Lasso { points }) => {
                    if let Some(first) = points.first() {
                        let path = Path::new(|b| {
                            b.move_to(self.camera.screen(*first, bounds));
                            for point in points.iter().skip(1) {
                                b.line_to(self.camera.screen(*point, bounds));
                            }
                            b.line_to(p);
                            b.close();
                        });
                        frame.fill(&path, Color { a: 0.08, ..ACCENT });
                        frame.stroke(&path, Stroke::default().with_color(ACCENT));
                    }
                }
                Some(Gesture::Select { start }) => {
                    let a = self.camera.screen(*start, bounds);
                    let lo = Point::new(a.x.min(p.x), a.y.min(p.y));
                    let size = iced::Size::new((a.x - p.x).abs(), (a.y - p.y).abs());
                    frame.fill_rectangle(lo, size, Color { a: 0.08, ..ACCENT });
                    frame.stroke(
                        &Path::rectangle(lo, size),
                        Stroke::default().with_color(ACCENT),
                    );
                }
                None if cursor.is_over(bounds) && self.tool != Tool::Erase => {
                    let point = self.camera.world(p, bounds);
                    let mut hit =
                        hit_selection(self.doc, point, self.camera.world_len(pick::PICK_RADIUS_PX));
                    if self.tool.selects() && hit.is_empty() {
                        hit = reshiki::editing::ring_at(self.doc, point).unwrap_or_default();
                    }
                    if self.tool.selects() && !state.modifiers.alt() {
                        hit = self.doc.expand_groups(&hit);
                    }
                    markers::Markers::hover(self.doc, &hit).draw(frame, self.camera, bounds, false);
                }
                _ => {}
            }
        }
        ControlFlow::Continue(())
    }

    pub(super) fn draw_erase_cursor(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) {
        if self.tool == Tool::Erase
            && cursor.is_over(bounds)
            && let Some(p) = state.cursor
        {
            frame.stroke(
                &Path::circle(Point::new(p.x - bounds.x, p.y - bounds.y), 7.),
                Stroke::default()
                    .with_width(1.)
                    .with_color(rgb([180, 66, 66])),
            );
        }
    }

    pub(super) fn draw_selection(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
        preview: &Document,
        selected: &[u64],
        cached: Option<World>,
    ) {
        if self.tool.selects() && self.optimizer.is_none() {
            if let (Some(Gesture::Transform(drag)), Some(p)) = (&state.gesture, state.cursor)
                && matches!(drag.handle, Handle::Rotate)
            {
                let end = self
                    .camera
                    .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
                drag.selection
                    .draw(frame, drag.values(end, state.modifiers.shift()).1);
            } else {
                let selection = if let Some(translation) = cached {
                    state
                        .scene
                        .borrow_mut()
                        .selection(self.doc, selected, self.camera, bounds)
                        .map(|selection| selection.translated(translation))
                } else {
                    SelectionBox::new(preview, selected, self.camera, bounds)
                };
                if let Some(selection) = selection {
                    selection.draw(frame, 0.0);
                }
            }
            if let (Some(Gesture::Transform(drag)), Some(p)) = (&state.gesture, state.cursor)
                && let Handle::Edge(i) = drag.handle
            {
                let end = self
                    .camera
                    .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
                let (scale, _) = drag.values(end, false);
                let position = Point::new(
                    (p.x - bounds.x + 16.).clamp(4., (bounds.width - 120.).max(4.)),
                    (p.y - bounds.y + 18.).clamp(4., (bounds.height - 28.).max(4.)),
                );
                frame.fill_rectangle(
                    position - Vector::new(4., 3.),
                    iced::Size::new(116., 23.),
                    Color::WHITE,
                );
                frame.fill_text(canvas::Text {
                    content: format!(
                        "{} {:.0}%",
                        if i.is_multiple_of(2) {
                            "Height"
                        } else {
                            "Width"
                        },
                        scale * 100.
                    ),
                    position,
                    color: rgb([30, 100, 85]),
                    size: 12.into(),
                    ..Default::default()
                });
            }
        }
    }

    pub(super) fn draw_tilt_bounds(
        &self,
        frame: &mut layered::Frame<'_>,
        preview: &Document,
        selected: &[u64],
        bounds: Rectangle,
    ) {
        if self.tool == Tool::Tilt
            && let Some((lo, hi)) = reshiki::scene::selection_bounds(preview, selected)
        {
            let lo = self.camera.screen(lo, bounds);
            let hi = self.camera.screen(hi, bounds);
            frame.stroke(
                &Path::rectangle(
                    Point::new(lo.x - 7., lo.y - 7.),
                    iced::Size::new(hi.x - lo.x + 14., hi.y - lo.y + 14.),
                ),
                Stroke::default()
                    .with_width(1.)
                    .with_color(rgb([65, 136, 119])),
            );
        }
    }
}

/// A one-line label beside the pointer at `p`, kept inside the canvas.
fn badge(
    frame: &mut layered::Frame<'_>,
    bounds: Rectangle,
    p: Point,
    content: String,
    valid: bool,
) {
    let width = crate::app::text_width(&content, 12.) + 10.;
    let position = Point::new(
        (p.x + 14.).clamp(4., (bounds.width - width - 4.).max(4.)),
        (p.y + 18.).clamp(20., (bounds.height - 30.).max(20.)),
    );
    frame.fill_rectangle(
        position - Vector::new(5., 4.),
        iced::Size::new(width, 24.),
        if valid {
            ACCENT_TINT
        } else {
            rgb([253, 235, 233])
        },
    );
    frame.fill_text(canvas::Text {
        content,
        position,
        color: rgb(if valid { [30, 100, 85] } else { [160, 50, 45] }),
        size: 12.into(),
        font: iced::Font::with_name(reshiki::style::ui_font_family()),
        ..Default::default()
    });
}
