//! A small energy diagram shared by the import sheet and the Conformers
//! tab: levels of an ensemble sorted by energy, or energy against frame for
//! an ordered trajectory. The selected item is highlighted; a click selects
//! the nearest one.
use super::Message;
use iced::widget::canvas::{self, Frame, Path, Stroke, Text};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};

/// How the items are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    /// An ensemble: one short level per item, sorted by energy.
    Levels,
    /// A trajectory: energy against item, in order, joined by a line.
    Line,
}

pub(crate) struct EnergyPlot {
    /// Energy above the lowest in kcal/mol, one per item in item order.
    pub delta_kcal: Vec<f64>,
    pub layout: Layout,
    pub selected: Option<usize>,
    /// A short note under each level (its Boltzmann share), in item order;
    /// empty for none.
    pub notes: Vec<String>,
    /// The message that selects an item; `None` while selecting is off.
    pub select: Option<fn(usize) -> Message>,
    /// Source trajectory steps, when the plot omits frames.
    pub steps: Vec<usize>,
}

const LEFT: f32 = 44.;
const RIGHT: f32 = 8.;
const TOP: f32 = 16.;
const BOTTOM: f32 = 16.;

impl EnergyPlot {
    /// Items in drawing order, left to right.
    fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.delta_kcal.len()).collect();
        if self.layout == Layout::Levels {
            order.sort_by(|&a, &b| {
                let energy = |i: usize| self.delta_kcal.get(i).copied().unwrap_or(0.);
                energy(a).total_cmp(&energy(b))
            });
        }
        order
    }

    /// The x of the `slot`-th item drawn, and the width of one level.
    fn x(&self, slot: usize, bounds: Rectangle) -> (f32, f32) {
        let width = (bounds.width - LEFT - RIGHT).max(1.);
        let count = self.delta_kcal.len().max(1);
        match self.layout {
            Layout::Levels => {
                let step = width / count as f32;
                (LEFT + step * (slot as f32 + 0.5), (step * 0.7).min(28.))
            }
            Layout::Line => {
                let x = LEFT + width * slot as f32 / count.saturating_sub(1).max(1) as f32;
                (x, 0.)
            }
        }
    }

    /// The highest level drawn, from the finite ones.
    fn top(&self) -> f64 {
        self.delta_kcal
            .iter()
            .copied()
            .filter(|d| d.is_finite())
            .fold(0.1, f64::max)
    }

    /// Where item `item`, drawn in `slot`, sits.
    fn point(&self, slot: usize, item: usize, bounds: Rectangle) -> Point {
        let delta = self.delta_kcal.get(item).copied().unwrap_or(0.);
        Point::new(self.x(slot, bounds).0, self.y(delta, bounds))
    }

    fn y(&self, delta: f64, bounds: Rectangle) -> f32 {
        let bottom = bounds.height - BOTTOM;
        let ratio = delta / self.top();
        let ratio = if ratio.is_finite() {
            ratio.clamp(0., 1.)
        } else {
            1.
        };
        bottom - (bottom - TOP).max(1.) * ratio as f32
    }
}

impl canvas::Program<Message> for EnergyPlot {
    type State = ();

    fn update(
        &self,
        _: &mut (),
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event else {
            return None;
        };
        let select = self.select?;
        let point = cursor.position_in(bounds)?;
        // The nearest level by x; the nearest point of a line in the plane.
        let order = self.order();
        let distance = |slot: usize, item: usize| {
            let at = self.point(slot, item, bounds);
            match self.layout {
                Layout::Levels => (at.x - point.x).abs(),
                Layout::Line => at.distance(point),
            }
        };
        let (_, &item) = order
            .iter()
            .enumerate()
            .min_by(|(a, i), (b, j)| distance(*a, **i).total_cmp(&distance(*b, **j)))?;
        let step = if self.steps.is_empty() {
            item
        } else {
            *self.steps.get(item)?
        };
        Some(canvas::Action::publish(select(step)).and_capture())
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.select.is_some() && cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let muted = crate::appearance::muted(theme);
        let ink = theme.palette().text;
        let accent = theme.palette().primary;
        let bottom = bounds.height - BOTTOM;
        let top = self.top();
        let y = |delta: f64| self.y(delta, bounds);
        let label = |frame: &mut Frame, text: String, at: Point, color: Color| {
            frame.fill_text(Text {
                content: text,
                position: at,
                color,
                size: 10.0.into(),
                ..Text::default()
            });
        };
        // The axis: kcal/mol from zero up to the highest item.
        frame.stroke(
            &Path::line(Point::new(LEFT - 4., TOP), Point::new(LEFT - 4., bottom)),
            Stroke::default()
                .with_color(Color { a: 0.5, ..muted })
                .with_width(1.),
        );
        label(
            &mut frame,
            if top < 1e4 {
                format!("{top:.1}")
            } else {
                format!("{top:.0e}")
            },
            Point::new(4., TOP - 6.),
            muted,
        );
        label(&mut frame, "0".into(), Point::new(4., bottom - 6.), muted);
        label(
            &mut frame,
            "ΔE, kcal/mol".into(),
            Point::new(LEFT, 0.),
            muted,
        );
        let order = self.order();
        match self.layout {
            Layout::Levels => {
                for (slot, &item) in order.iter().enumerate() {
                    let Some(&delta) = self.delta_kcal.get(item) else {
                        continue;
                    };
                    let (x, width) = self.x(slot, bounds);
                    let chosen = self.selected == Some(item);
                    let level = y(delta);
                    frame.stroke(
                        &Path::line(
                            Point::new(x - width / 2., level),
                            Point::new(x + width / 2., level),
                        ),
                        Stroke::default()
                            .with_color(if chosen { accent } else { ink })
                            .with_width(if chosen { 4. } else { 2. }),
                    );
                    if let Some(note) = self.notes.get(item).filter(|_| width >= 20.) {
                        label(
                            &mut frame,
                            note.clone(),
                            Point::new(x - width / 2., bottom + 3.),
                            if chosen { accent } else { muted },
                        );
                    }
                }
            }
            Layout::Line => {
                let points: Vec<Point> = order
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, &item)| {
                        let delta = *self.delta_kcal.get(item)?;
                        Some(Point::new(self.x(slot, bounds).0, y(delta)))
                    })
                    .collect();
                let path = Path::new(|builder| {
                    for (i, point) in points.iter().enumerate() {
                        if i == 0 {
                            builder.move_to(*point);
                        } else {
                            builder.line_to(*point);
                        }
                    }
                });
                frame.stroke(&path, Stroke::default().with_color(ink).with_width(1.5));
                for (item, point) in points.iter().enumerate() {
                    let chosen = self.selected == Some(item);
                    frame.fill(
                        &Path::circle(*point, if chosen { 4.5 } else { 2. }),
                        if chosen { accent } else { ink },
                    );
                }
                label(
                    &mut frame,
                    format!("frame 1 … {}", points.len()),
                    Point::new(LEFT, bottom + 3.),
                    muted,
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot(delta_kcal: Vec<f64>, layout: Layout) -> EnergyPlot {
        EnergyPlot {
            delta_kcal,
            layout,
            selected: None,
            notes: vec![],
            select: None,
            steps: vec![],
        }
    }

    #[test]
    fn levels_sort_by_energy_and_huge_values_stay_on_the_chart() {
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(300., 120.));
        let levels = plot(vec![2., 0., f64::INFINITY], Layout::Levels);
        assert_eq!(levels.order(), [1, 0, 2]);
        for (slot, &item) in levels.order().iter().enumerate() {
            let at = levels.point(slot, item, bounds);
            assert!(at.x.is_finite() && at.y.is_finite(), "{at:?}");
            assert!((TOP..=bounds.height - BOTTOM).contains(&at.y));
        }
        // A trajectory keeps file order; its lowest sits on the axis.
        let line = plot(vec![3., 0., 1.], Layout::Line);
        assert_eq!(line.order(), [0, 1, 2]);
        assert_eq!(line.point(1, 1, bounds).y, bounds.height - BOTTOM);
        assert_eq!(line.point(0, 0, bounds).y, TOP);
    }
}
