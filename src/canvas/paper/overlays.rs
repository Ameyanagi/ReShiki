//! Editing aids drawn over the document: handles, notices and badges, gesture outlines and hover, the erase cursor, the selection box and tilt bounds.

use crate::canvas::hit::{bond_target_with, hit_selection};
use crate::canvas::{
    Gesture, Handle, MoleculeCanvas, SelectionBox, State, Tool, World, layered, markers, rgb,
};
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Vector, mouse};
use reshiki::document::Document;
use reshiki::graphics::PathCommand;
use std::ops::ControlFlow;

impl MoleculeCanvas<'_> {
    pub(super) fn draw_editor_markers(
        &self,
        frame: &mut layered::Frame<'_>,
        preview: &Document,
        bounds: Rectangle,
    ) {
        // Editing aids stay out of the shared scene used by figure/Office export.
        for atom in reshiki::attachments::editor_markers(preview) {
            let center = self.camera.screen(atom.position, bounds);
            let stroke = Stroke::default()
                .with_width(1.2)
                .with_color(rgb([19, 135, 116]));
            frame.stroke(&Path::circle(center, 4.), stroke);
            for delta in [Vector::new(6., 0.), Vector::new(0., 6.)] {
                frame.stroke(&Path::line(center - delta, center + delta), stroke);
            }
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
                for (i, p) in a.handles().into_iter().enumerate() {
                    let p = self.camera.screen(p, bounds);
                    let path = if i == 2 {
                        Path::rectangle(p - Vector::new(4., 4.), iced::Size::new(8., 8.))
                    } else {
                        Path::circle(p, 4.)
                    };
                    frame.fill(&path, Color::WHITE);
                    frame.stroke(
                        &path,
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(rgb([19, 135, 116])),
                    );
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
                rgb(if valid {
                    [225, 242, 237]
                } else {
                    [253, 235, 233]
                }),
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
        if self.tool == Tool::EditPoints {
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
                    let path = Path::circle(center, 4.);
                    frame.fill(&path, Color::WHITE);
                    frame.stroke(
                        &path,
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(rgb([19, 135, 116])),
                    );
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
                    let path = Path::circle(center, 4.);
                    frame.fill(&path, Color::WHITE);
                    frame.stroke(
                        &path,
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(rgb([19, 135, 116])),
                    );
                }
            }
            for graphic in preview.graphics.iter().filter(|g| selected.contains(&g.id)) {
                if graphic.kind == reshiki::graphics::GraphicKind::Arc {
                    for p in graphic.edit_points() {
                        let path = Path::circle(self.camera.screen(p, bounds), 5.0);
                        frame.fill(&path, Color::WHITE);
                        frame.stroke(
                            &path,
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(rgb([19, 135, 116])),
                        );
                    }
                    continue;
                }
                let mut anchor = World::default();
                for c in graphic.commands() {
                    let points = c.points();
                    if let PathCommand::Cubic(a, b, end) = c {
                        for (from, to) in [(anchor, a), (b, end)] {
                            frame.stroke(
                                &Path::line(
                                    self.camera.screen(from, bounds),
                                    self.camera.screen(to, bounds),
                                ),
                                Stroke::default().with_color(rgb([19, 135, 116])),
                            );
                        }
                        anchor = end;
                    } else if let Some(p) = points.last() {
                        anchor = *p;
                    }
                    for p in points {
                        let path = Path::circle(self.camera.screen(p, bounds), 4.0);
                        frame.fill(&path, Color::WHITE);
                        frame.stroke(
                            &path,
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(rgb([19, 135, 116])),
                        );
                    }
                }
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
                    if start.distance(end) < 3.0 / self.camera.zoom {
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
                        Stroke::default()
                            .with_width(2.0)
                            .with_color(rgb([19, 135, 116])),
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
                        frame.fill(&path, Color::from_rgba8(19, 135, 116, 0.08));
                        frame.stroke(&path, Stroke::default().with_color(rgb([19, 135, 116])));
                    }
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
                None if cursor.is_over(bounds) && self.tool != Tool::Erase => {
                    let point = self.camera.world(p, bounds);
                    let mut hit = hit_selection(self.doc, point, 10.0 / self.camera.zoom);
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
        rgb(if valid {
            [225, 242, 237]
        } else {
            [253, 235, 233]
        }),
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
