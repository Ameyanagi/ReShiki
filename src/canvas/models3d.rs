//! 3D models on the paper (#386): turning a selected model, the 2D → 3D morph
//! and the floating Show as bar above a selected molecule.
//! [`tiles`] draws the models themselves as resvg image tiles.
use super::hit::hit_selection;
use super::pick::{DRAG_SLOP_PX, PICK_RADIUS_PX};
use super::{Edit, MoleculeCanvas, State, World, command_held, layered, rgb};
use iced::widget::canvas::{self, Action, Path, Stroke};
use iced::{Color, Point, Rectangle, Size, Vector};
use reshiki::models3d::{self, Display};
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

mod tiles;
pub(super) use tiles::{draw, is_model};

/// Screen pixels inside a frame's edge that still move the model.
const EDGE_PX: f32 = 8.;
/// The prototype's morph length.
pub(crate) const MORPH: Duration = Duration::from_millis(750);
const DOUBLE_CLICK_MS: u128 = 450;

/// A drag turning a selected model. Each step turns about the screen axes as
/// they are then, as the prototype does, so the preview and the commit share
/// [`Turn::rotation_at`].
#[derive(Debug)]
pub(super) struct Turn {
    pub atoms: Vec<u64>,
    pub clicked: Vec<u64>,
    pub start: World,
    press: Point,
    last: Point,
    rotation: [f32; 4],
    moved: bool,
}

impl Turn {
    /// The view with the drag carried on to `point`; nothing turns within
    /// the drag slop, so a click never turns the model.
    pub fn rotation_at(&self, point: Point, roll: bool) -> [f32; 4] {
        if !self.moved && point.distance(self.press) < DRAG_SLOP_PX {
            return self.rotation;
        }
        models3d::turn(
            self.rotation,
            point.x - self.last.x,
            point.y - self.last.y,
            roll,
        )
    }

    pub fn track(&mut self, point: Point, roll: bool) {
        self.rotation = self.rotation_at(point, roll);
        self.moved |= point.distance(self.press) >= DRAG_SLOP_PX;
        if self.moved {
            self.last = point;
        }
    }
}

/// The floating Show as bar: its panel and each button.
pub(super) struct Bar {
    panel: Rectangle,
    buttons: [(Display, &'static str, Rectangle); 3],
}

const BAR_PADDING: f32 = 4.;
const BUTTON_HEIGHT: f32 = 26.;
const ICON: Size = Size::new(16., 14.);

impl Bar {
    /// Centred above the selection's box and its rotation handle, or below it
    /// when there is no room; when neither fits it stays inside the paper,
    /// over the selection.
    fn new(selection: Rectangle, paper: Size) -> Self {
        let font = |label: &str, size: f32| crate::app::text_width(label, size);
        let label = font("Show as", 11.5) + 14.;
        let widths = ["2D", "3D", "Both"].map(|l| 10. + ICON.width + 6. + font(l, 12.5) + 10.);
        let width = 2. * BAR_PADDING + label + widths.iter().map(|w| w + 4.).sum::<f32>();
        let height = 2. * BAR_PADDING + BUTTON_HEIGHT;
        // The box's grips sit 12 px out and its rotation handle 34 px above them.
        let above = selection.y - 12. - 34. - 10. - height;
        let below = selection.y + selection.height + 12. + 8.;
        let lowest = (paper.height - height - 4.).max(4.);
        // Whichever side it takes, it stays inside the canvas, even when the
        // selection lies above or below it.
        let y = if above >= 4. { above } else { below }.clamp(4., lowest);
        let x = (selection.center_x() - width / 2.).clamp(4., (paper.width - width - 4.).max(4.));
        let panel = Rectangle::new(Point::new(x, y), Size::new(width, height));
        let mut left = x + BAR_PADDING + label + 4.;
        let [a, b, c] = widths;
        let buttons = [
            (Display::TwoD, "2D", a),
            (Display::ThreeD, "3D", b),
            (Display::Both, "Both", c),
        ]
        .map(|(display, text, width)| {
            let rect = Rectangle::new(
                Point::new(left, y + BAR_PADDING),
                Size::new(width, BUTTON_HEIGHT),
            );
            left += width + 4.;
            (display, text, rect)
        });
        Self { panel, buttons }
    }

    pub fn contains(&self, p: Point) -> bool {
        self.panel.contains(p)
    }

    pub fn button(&self, p: Point) -> Option<Display> {
        self.buttons
            .iter()
            .find(|(_, _, rect)| rect.contains(p))
            .map(|(display, ..)| *display)
    }

    fn draw(&self, frame: &mut layered::Frame<'_>, active: Display, hover: Option<Point>) {
        let rounded = |r: Rectangle, radius: f32| {
            Path::rounded_rectangle(r.position(), r.size(), radius.into())
        };
        // The prototype's soft shadow: 0 4px 14px rgba(20, 30, 40, .1).
        for spread in [10., 6., 3.] {
            let shadow = Rectangle {
                x: self.panel.x - spread / 2.,
                y: self.panel.y + 4. - spread / 2.,
                width: self.panel.width + spread,
                height: self.panel.height + spread,
            };
            frame.fill(
                &rounded(shadow, 10. + spread / 2.),
                Color::from_rgba8(20, 30, 40, 0.035),
            );
        }
        let panel = rounded(self.panel, 10.);
        frame.fill(&panel, Color::WHITE);
        frame.stroke(
            &panel,
            Stroke::default()
                .with_width(1.)
                .with_color(rgb([0xd5, 0xdb, 0xe0])),
        );
        let font = iced::Font::with_name(reshiki::style::ui_font_family());
        let middle = self.panel.center_y();
        frame.fill_text(canvas::Text {
            content: "Show as".into(),
            position: Point::new(self.panel.x + BAR_PADDING + 8., middle),
            color: rgb([0x66, 0x71, 0x7a]),
            size: 11.5.into(),
            font,
            align_y: iced::alignment::Vertical::Center,
            ..Default::default()
        });
        for (display, label, rect) in &self.buttons {
            let pressed = *display == active;
            let ink = if pressed {
                rgb([0x2f, 0x7a, 0x66])
            } else {
                rgb([0x26, 0x30, 0x2c])
            };
            if pressed || hover.is_some_and(|p| rect.contains(p)) {
                frame.fill(
                    &rounded(*rect, 7.),
                    rgb(if pressed {
                        [0xe3, 0xf0, 0xeb]
                    } else {
                        [0xf1, 0xf4, 0xf3]
                    }),
                );
            }
            let icon = Point::new(rect.x + 10., middle - ICON.height / 2.);
            draw_icon(frame, *display, icon, ink);
            frame.fill_text(canvas::Text {
                content: (*label).into(),
                position: Point::new(icon.x + ICON.width + 6., middle),
                color: ink,
                size: 12.5.into(),
                font,
                align_y: iced::alignment::Vertical::Center,
                ..Default::default()
            });
        }
    }
}

/// The prototype's 16 × 14 pictograms: a hexagon, a ball & stick pair, both.
fn draw_icon(frame: &mut layered::Frame<'_>, display: Display, at: Point, ink: Color) {
    let p = |x: f32, y: f32| at + Vector::new(x, y);
    let polygon = |points: &[(f32, f32)]| {
        Path::new(|b| {
            for (i, (x, y)) in points.iter().enumerate() {
                if i == 0 {
                    b.move_to(p(*x, *y));
                } else {
                    b.line_to(p(*x, *y));
                }
            }
            b.close();
        })
    };
    let stroke = |width: f32| Stroke::default().with_width(width).with_color(ink);
    match display {
        Display::TwoD => frame.stroke(
            &polygon(&[
                (8., 1.5),
                (13.6, 4.75),
                (13.6, 9.25),
                (8., 12.5),
                (2.4, 9.25),
                (2.4, 4.75),
            ]),
            stroke(1.4),
        ),
        Display::ThreeD => {
            frame.stroke(&Path::line(p(4., 10.), p(12., 4.)), stroke(1.6));
            frame.fill(&Path::circle(p(4., 10.), 3.), ink);
            frame.fill(&Path::circle(p(12., 4.), 2.6), ink);
        }
        Display::Both => {
            frame.stroke(
                &polygon(&[
                    (5., 2.),
                    (8.5, 4.),
                    (8.5, 8.),
                    (5., 10.),
                    (1.5, 8.),
                    (1.5, 4.),
                ]),
                stroke(1.3),
            );
            frame.fill(&Path::circle(p(12.5, 9.5), 2.6), ink);
        }
    }
}

impl MoleculeCanvas<'_> {
    /// The bar for the selected molecule, while no gesture is under way.
    pub(super) fn model_bar(&self, state: &State, bounds: Rectangle) -> Option<Bar> {
        self.model_bar?;
        if state.gesture.is_some() || !self.tool.selects() {
            return None;
        }
        let (lo, hi) = reshiki::scene::selection_bounds(self.doc, self.selected)?;
        let (lo, hi) = (
            self.camera.screen(lo, bounds),
            self.camera.screen(hi, bounds),
        );
        Some(Bar::new(
            Rectangle::new(lo, Size::new(hi.x - lo.x, hi.y - lo.y)),
            bounds.size(),
        ))
    }

    pub(super) fn draw_model_bar(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        if let (Some(active), Some(bar)) = (self.model_bar, self.model_bar(state, bounds)) {
            let hover = state
                .cursor
                .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
            bar.draw(frame, active, hover);
        }
    }

    /// A press on the bar picks a display; elsewhere on it, it does nothing.
    pub(super) fn press_model_bar(
        &self,
        state: &State,
        point: Point,
        bounds: Rectangle,
    ) -> Option<Action<Edit>> {
        let bar = self.model_bar(state, bounds)?;
        if !bar.contains(point) {
            return None;
        }
        // The active button does nothing, like the section's.
        Some(
            match bar.button(point).filter(|d| Some(*d) != self.model_bar) {
                Some(display) => Action::publish(Edit::ShowAs(display)).and_capture(),
                None => Action::capture(),
            },
        )
    }

    /// The selected model that a drag at `p` turns: inside its frame, not at
    /// its edge, with its molecule the selection, and no Alt or copy key.
    /// Also returns what a click there selects.
    pub(super) fn turn_target(&self, state: &State, p: World) -> Option<(Vec<u64>, Vec<u64>)> {
        if !self.tool.selects() || state.modifiers.alt() || command_held(state.modifiers) {
            return None;
        }
        let model = models3d::model_at(self.doc, p, self.camera.world_len(EDGE_PX))?;
        let selected = models3d::whole_molecule(self.doc, self.selected)?;
        let atoms: Vec<_> = model.atoms.iter().map(|a| a.id).collect();
        let hit = hit_selection(self.doc, p, self.camera.world_len(PICK_RADIUS_PX));
        (selected.len() == atoms.len()
            && atoms.iter().all(|id| selected.contains(id))
            && hit.first().is_some_and(|id| model.contains(*id)))
        .then(|| (atoms, self.doc.expand_groups(&hit)))
    }

    pub(super) fn press_turn(
        &self,
        state: &mut State,
        p: World,
        point: Point,
    ) -> Option<Action<Edit>> {
        let (atoms, clicked) = self.turn_target(state, p)?;
        let rotation = models3d::model_of(self.doc, *atoms.first()?)?.rotation;
        state.gesture = Some(super::Gesture::Turn(Turn {
            atoms,
            clicked,
            start: p,
            press: point,
            last: point,
            rotation,
            moved: false,
        }));
        Some(Action::publish(Edit::Hover(None)).and_capture())
    }

    /// A drag commits the turn; a click selects as usual, and a double-click
    /// restores the opening view.
    pub(super) fn release_turn(
        &self,
        state: &mut State,
        turn: Turn,
        position: Point,
        p: World,
        bounds: Rectangle,
    ) -> ControlFlow<Action<Edit>, Edit> {
        let roll = state.modifiers.shift();
        if turn.moved || position.distance(turn.press) >= DRAG_SLOP_PX {
            state.last_click = None;
            return ControlFlow::Continue(Edit::ModelView {
                rotation: turn.rotation_at(position, roll),
                atoms: turn.atoms,
            });
        }
        let now = Instant::now();
        if state.last_click.is_some_and(|(time, id)| {
            turn.atoms.contains(&id) && now.duration_since(time).as_millis() < DOUBLE_CLICK_MS
        }) && let Some(model) = turn
            .atoms
            .first()
            .and_then(|id| models3d::model_of(self.doc, *id))
        {
            state.last_click = None;
            return ControlFlow::Continue(Edit::ModelView {
                rotation: model.initial_view(self.doc),
                atoms: turn.atoms,
            });
        }
        self.release_move(state, turn.start, turn.atoms, turn.clicked, p, bounds)
    }

    /// The model morphing from its drawing into 3D, and how far along it is.
    pub(super) fn morph(&self, now: Instant) -> Option<(u64, f32)> {
        let (id, start) = self.morph?;
        let t = now.checked_duration_since(start)?.as_secs_f32() / MORPH.as_secs_f32();
        (t < 1.).then_some((id, t))
    }
}

/// Show `turn`'s view on the preview while it is dragged.
pub(super) fn preview_turn(
    doc: &mut reshiki::document::Document,
    turn: &Turn,
    point: Point,
    roll: bool,
) {
    if let Some(id) = turn.atoms.first() {
        models3d::set_rotation(doc, *id, turn.rotation_at(point, roll));
    }
}

#[cfg(test)]
mod tests;
