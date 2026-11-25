//! The orbital levels of a result: one column per spin, names around the
//! gap, degenerate groups, and the level diagram drawn from them.
// Level indices come from windows and groups bounded by the spectrum.
#![allow(clippy::indexing_slicing)]
use super::{Action, send};
use crate::app::Message;
use iced::widget::canvas;
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};
use reshiki::qcdata::{Beta, Orbitals, Spin};

const EV_PER_HARTREE: f64 = super::super::results::EV_PER_HARTREE;
/// Levels closer than this (eV) are one degenerate group.
const DEGENERATE_EV: f64 = 0.005;
/// Levels shown on each side of the frontier.
const AROUND: usize = 4;

/// One spin's spectrum as the tab shows it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Column {
    /// The spin whose coefficient rows hold these orbitals: alpha unless
    /// the result has its own beta orbitals.
    pub spin: Spin,
    /// "" for a closed shell, else "α" or "β".
    pub label: &'static str,
    pub energies_ev: Vec<f64>,
    /// Electrons in each level (0 to 2 for a closed shell, else 0 to 1);
    /// empty when the source gave none.
    pub occupations: Vec<f64>,
    /// Occupied levels; the HOMO is the one below.
    pub occupied: Option<usize>,
}

impl Column {
    /// "HOMO", "HOMO−1", "LUMO+2", or "MO 7" without occupations.
    pub(crate) fn name(&self, index: usize) -> String {
        let name = match self.occupied {
            None => format!("MO {}", index + 1),
            Some(n) => match index.checked_sub(n) {
                None if index + 1 == n => "HOMO".into(),
                None => format!("HOMO−{}", n - 1 - index),
                Some(0) => "LUMO".into(),
                Some(above) => format!("LUMO+{above}"),
            },
        };
        if self.label.is_empty() {
            name
        } else {
            format!("{} {name}", self.label)
        }
    }

    pub(crate) fn homo(&self) -> Option<usize> {
        self.occupied?.checked_sub(1)
    }

    pub(crate) fn lumo(&self) -> Option<usize> {
        self.occupied.filter(|&n| n < self.energies_ev.len())
    }

    /// The levels the list and diagram show: HOMO−4 … LUMO+4, widened to
    /// hold `selected`.
    pub(crate) fn window(&self, selected: Option<usize>) -> std::ops::RangeInclusive<usize> {
        let last = self.energies_ev.len().saturating_sub(1);
        let frontier = self.occupied.unwrap_or(0);
        let mut lo = frontier.saturating_sub(AROUND + 1);
        let mut hi = (frontier + AROUND).min(last);
        if let Some(index) = selected {
            lo = lo.min(index);
            hi = hi.max(index.min(last));
        }
        lo..=hi
    }

    /// The degenerate group of `index`, `index` included.
    pub(crate) fn group(&self, index: usize) -> std::ops::RangeInclusive<usize> {
        let e = &self.energies_ev;
        let mut lo = index;
        while lo > 0 && (e[lo] - e[lo - 1]).abs() < DEGENERATE_EV {
            lo -= 1;
        }
        let mut hi = index;
        while hi + 1 < e.len() && (e[hi + 1] - e[hi]).abs() < DEGENERATE_EV {
            hi += 1;
        }
        lo..=hi
    }
}

/// The spin columns of `orbitals`: one for a closed shell, α and β for an
/// open shell (sharing alpha's orbitals unless beta has its own).
pub(crate) fn columns(orbitals: &Orbitals) -> Vec<Column> {
    let alpha = &orbitals.alpha;
    let ev = |levels: &[f64]| {
        levels
            .iter()
            .map(|e| e * EV_PER_HARTREE)
            .collect::<Vec<_>>()
    };
    // Occupied levels from the electron count, as the Summary counts them:
    // thermal smearing does not move the frontier.
    let count = |occupations: &[f64], levels: usize, closed: bool| {
        (occupations.len() == levels).then(|| {
            let electrons = occupations.iter().sum::<f64>().round() as usize;
            if closed {
                electrons.div_ceil(2)
            } else {
                electrons
            }
        })
    };
    let n = alpha.energies_hartree.len();
    let column = |spin, label, energies: &[f64], occupations: &[f64], closed| Column {
        spin,
        label,
        energies_ev: ev(energies),
        occupations: occupations.to_vec(),
        occupied: count(occupations, energies.len(), closed),
    };
    match &orbitals.beta {
        Beta::Restricted => vec![column(
            Spin::Alpha,
            "",
            &alpha.energies_hartree,
            &alpha.occupations,
            true,
        )],
        Beta::SameOrbitals { occupations } => vec![
            column(
                Spin::Alpha,
                "α",
                &alpha.energies_hartree,
                &alpha.occupations,
                false,
            ),
            Column {
                occupied: count(occupations, n, false),
                ..column(
                    Spin::Alpha,
                    "β",
                    &alpha.energies_hartree,
                    occupations,
                    false,
                )
            },
        ],
        Beta::Unrestricted(beta) => vec![
            column(
                Spin::Alpha,
                "α",
                &alpha.energies_hartree,
                &alpha.occupations,
                false,
            ),
            column(
                Spin::Beta,
                "β",
                &beta.energies_hartree,
                &beta.occupations,
                false,
            ),
        ],
    }
}

/// The level diagram: an energy axis, one column per spin, occupied levels
/// with electron arrows, HOMO and LUMO named, the gap marked. Clicking a
/// level chooses it.
pub(crate) struct Diagram {
    pub columns: Vec<Column>,
    /// Chosen column and level.
    pub selected: Option<(usize, usize)>,
    pub gap_ev: Option<f64>,
    pub live: bool,
}

/// One drawn level: column, index, and its line from `x0` to `x1` at `y`.
struct Placed {
    column: usize,
    index: usize,
    x0: f32,
    x1: f32,
    y: f32,
}

impl Diagram {
    const AXIS: f32 = 30.;
    const TOP: f32 = 18.;
    const BOTTOM: f32 = 8.;
    const LINE: f32 = 40.;
    const COLUMN: f32 = 54.;

    /// The diagram's width for `columns` spin columns.
    pub(crate) fn width(columns: usize) -> f32 {
        Self::AXIS + 12. + columns as f32 * Self::COLUMN + 76.
    }

    /// The energy range shown: the windows of every column.
    fn range(&self) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for (c, column) in self.columns.iter().enumerate() {
            let selected = self.selected.filter(|s| s.0 == c).map(|s| s.1);
            for index in column.window(selected) {
                if let Some(&e) = column.energies_ev.get(index) {
                    lo = lo.min(e);
                    hi = hi.max(e);
                }
            }
        }
        if lo > hi {
            return (-1., 1.);
        }
        let pad = ((hi - lo) * 0.04).max(0.2);
        (lo - pad, hi + pad)
    }

    fn y(&self, bounds: Rectangle) -> impl Fn(f64) -> f32 + use<> {
        let (lo, hi) = self.range();
        let height = (bounds.height - Self::TOP - Self::BOTTOM).max(1.);
        move |e: f64| Self::TOP + ((hi - e) / (hi - lo).max(1e-9)) as f32 * height
    }

    fn place(&self, bounds: Rectangle) -> Vec<Placed> {
        let y = self.y(bounds);
        let mut placed = Vec::new();
        for (c, column) in self.columns.iter().enumerate() {
            let left = Self::AXIS + 12. + c as f32 * Self::COLUMN;
            let selected = self.selected.filter(|s| s.0 == c).map(|s| s.1);
            let window = column.window(selected);
            let mut index = *window.start();
            while index <= *window.end() && index < column.energies_ev.len() {
                let group = column.group(index);
                let members: Vec<usize> =
                    (index.max(*group.start())..=(*group.end()).min(*window.end())).collect();
                // Degenerate levels side by side, sharing the column's width.
                let width = (Self::LINE - 4. * (members.len() - 1) as f32) / members.len() as f32;
                for (k, &member) in members.iter().enumerate() {
                    let x0 = left + k as f32 * (width + 4.);
                    placed.push(Placed {
                        column: c,
                        index: member,
                        x0,
                        x1: x0 + width,
                        y: y(column.energies_ev[member]),
                    });
                }
                index = members.last().map_or(index + 1, |last| last + 1);
            }
        }
        placed
    }

    /// The level under `point`, within a few pixels.
    fn hit(&self, bounds: Rectangle, point: Point) -> Option<(usize, usize)> {
        self.place(bounds)
            .iter()
            .filter(|p| point.x >= p.x0 - 3. && point.x <= p.x1 + 3.)
            .map(|p| ((p.y - point.y).abs(), (p.column, p.index)))
            .filter(|(distance, _)| *distance <= 6.)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, hit)| hit)
    }
}

impl canvas::Program<Message> for Diagram {
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
        let point = cursor.position_in(bounds).filter(|_| self.live)?;
        let (column, index) = self.hit(bounds, point)?;
        Some(canvas::Action::publish(send(Action::Choose(column, index))).and_capture())
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let over = cursor
            .position_in(bounds)
            .filter(|_| self.live)
            .and_then(|point| self.hit(bounds, point));
        if over.is_some() {
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
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let muted = crate::appearance::muted(theme);
        let ink = if self.live {
            theme.palette().text
        } else {
            muted
        };
        let accent = theme.palette().primary;
        let (lo, hi) = self.range();
        let y = self.y(bounds);
        let label = |frame: &mut canvas::Frame, content: String, at: Point, color, right: bool| {
            frame.fill_text(canvas::Text {
                content,
                position: at,
                color,
                size: 10.into(),
                align_x: if right {
                    iced::alignment::Horizontal::Right.into()
                } else {
                    iced::alignment::Horizontal::Left.into()
                },
                align_y: iced::alignment::Vertical::Center,
                ..Default::default()
            });
        };
        // The axis with round ticks, at most about six.
        let axis = canvas::Stroke::default()
            .with_color(Color { a: 0.5, ..muted })
            .with_width(1.);
        frame.stroke(
            &canvas::Path::line(Point::new(Self::AXIS, y(hi)), Point::new(Self::AXIS, y(lo))),
            axis,
        );
        let step = [0.5, 1., 2., 5., 10., 20., 50.]
            .into_iter()
            .find(|step| (hi - lo) / step <= 6.)
            .unwrap_or(100.);
        let mut tick = (lo / step).ceil() * step;
        while tick <= hi {
            let at = y(tick);
            frame.stroke(
                &canvas::Path::line(Point::new(Self::AXIS - 4., at), Point::new(Self::AXIS, at)),
                axis,
            );
            let places = usize::from(step < 1.);
            label(
                &mut frame,
                super::super::state::signed(tick, places),
                Point::new(Self::AXIS - 6., at),
                muted,
                true,
            );
            tick += step;
        }
        label(
            &mut frame,
            "eV".into(),
            Point::new(Self::AXIS - 6., 6.),
            muted,
            true,
        );
        for (c, column) in self.columns.iter().enumerate() {
            if !column.label.is_empty() {
                label(
                    &mut frame,
                    column.label.into(),
                    Point::new(
                        Self::AXIS + 12. + c as f32 * Self::COLUMN + Self::LINE / 2. - 3.,
                        6.,
                    ),
                    muted,
                    false,
                );
            }
        }
        for level in &self.place(bounds) {
            let column = &self.columns[level.column];
            let chosen = self.selected == Some((level.column, level.index));
            let occupation = column.occupations.get(level.index).copied().unwrap_or(0.);
            let color = if chosen && self.live {
                accent
            } else if occupation > 0.05 {
                ink
            } else {
                muted
            };
            frame.stroke(
                &canvas::Path::line(Point::new(level.x0, level.y), Point::new(level.x1, level.y)),
                canvas::Stroke::default()
                    .with_color(color)
                    .with_width(if chosen { 3. } else { 1.8 }),
            );
            // Electron arrows: up for alpha, down for beta; a closed shell's
            // doubly occupied level gets both.
            let arrows: &[bool] = if column.label.is_empty() && occupation > 1.5 {
                &[true, false]
            } else if occupation > 0.5 {
                if column.label == "β" {
                    &[false]
                } else {
                    &[true]
                }
            } else {
                &[]
            };
            let middle = (level.x0 + level.x1) / 2.;
            for (k, &up) in arrows.iter().enumerate() {
                let x = middle + (k as f32 - (arrows.len() - 1) as f32 / 2.) * 7.;
                let (tip, tail, head) = if up {
                    (level.y - 5., level.y + 4., 2.5)
                } else {
                    (level.y + 5., level.y - 4., -2.5)
                };
                frame.stroke(
                    &canvas::Path::new(|path| {
                        path.move_to(Point::new(x, tail));
                        path.line_to(Point::new(x, tip));
                        path.move_to(Point::new(x - 2., tip + head));
                        path.line_to(Point::new(x, tip));
                        path.line_to(Point::new(x + 2., tip + head));
                    }),
                    canvas::Stroke::default().with_color(color).with_width(1.),
                );
            }
        }
        // HOMO and LUMO named right of the last column; the gap beside them.
        let right = Self::AXIS + 12. + self.columns.len() as f32 * Self::COLUMN;
        if let Some(column) = self.columns.first() {
            let frontier = [(column.homo(), "HOMO"), (column.lumo(), "LUMO")];
            for (index, name) in frontier {
                if let Some(index) = index {
                    label(
                        &mut frame,
                        name.into(),
                        Point::new(right - 8., y(column.energies_ev[index])),
                        ink,
                        false,
                    );
                }
            }
            if let (Some(homo), Some(lumo), Some(gap)) = (column.homo(), column.lumo(), self.gap_ev)
            {
                let (top, bottom) = (y(column.energies_ev[lumo]), y(column.energies_ev[homo]));
                let x = right + 30.;
                frame.stroke(
                    &canvas::Path::line(Point::new(x, top + 2.), Point::new(x, bottom - 2.)),
                    canvas::Stroke::default().with_color(accent).with_width(1.2),
                );
                label(
                    &mut frame,
                    "gap".into(),
                    Point::new(x + 4., (top + bottom) / 2. - 6.),
                    accent,
                    false,
                );
                label(
                    &mut frame,
                    format!("{gap:.2}"),
                    Point::new(x + 4., (top + bottom) / 2. + 6.),
                    accent,
                    false,
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::qcdata::OrbitalSet;

    fn set(energies: &[f64], occupations: &[f64]) -> OrbitalSet {
        OrbitalSet {
            energies_hartree: energies.to_vec(),
            occupations: occupations.to_vec(),
            symmetries: vec![],
            rows: vec![],
            coefficients: vec![],
        }
    }

    fn closed(energies: &[f64], occupations: &[f64]) -> Orbitals {
        Orbitals {
            basis: None,
            core_electrons: vec![],
            alpha: set(energies, occupations),
            beta: Beta::Restricted,
        }
    }

    #[test]
    fn a_closed_shell_names_levels_around_the_gap_and_groups_degenerate_ones() {
        let columns = columns(&closed(
            &[-0.6, -0.4, -0.4, 0.1, 0.1, 0.3],
            &[2., 2., 2., 0., 0., 0.],
        ));
        assert_eq!(columns.len(), 1);
        let column = &columns[0];
        assert_eq!((column.homo(), column.lumo()), (Some(2), Some(3)));
        assert_eq!(column.name(2), "HOMO");
        assert_eq!(column.name(1), "HOMO−1");
        assert_eq!(column.name(4), "LUMO+1");
        assert_eq!(column.group(2), 1..=2);
        assert_eq!(column.group(3), 3..=4);
        assert_eq!(column.group(0), 0..=0);
    }

    #[test]
    fn an_open_shell_has_alpha_and_beta_columns() {
        // A doublet sharing orbitals: alpha fills one more level.
        let orbitals = Orbitals {
            beta: Beta::SameOrbitals {
                occupations: vec![1., 0., 0.],
            },
            ..closed(&[-0.5, -0.3, 0.2], &[1., 1., 0.])
        };
        let split = columns(&orbitals);
        assert_eq!(split.len(), 2);
        assert_eq!(split[0].name(1), "α HOMO");
        assert_eq!(split[1].name(1), "β LUMO");
        assert_eq!(split[1].spin, Spin::Alpha, "beta shares alpha's rows");
        let unrestricted = Orbitals {
            beta: Beta::Unrestricted(set(&[-0.45, 0.1, 0.25], &[1., 0., 0.])),
            ..orbitals
        };
        let split = columns(&unrestricted);
        assert_eq!(split[1].spin, Spin::Beta);
        assert_eq!(split[1].homo(), Some(0));
        assert_eq!(split[1].energies_ev[0], -0.45 * EV_PER_HARTREE);
    }

    #[test]
    fn levels_without_occupations_are_numbered() {
        let column = &columns(&closed(&[-0.5, 0.2], &[]))[0];
        assert_eq!(column.name(0), "MO 1");
        assert_eq!(column.homo(), None);
    }

    #[test]
    fn the_window_widens_to_the_chosen_level() {
        let energies: Vec<f64> = (0..30).map(|i| -1. + f64::from(i) * 0.05).collect();
        let mut occupations = vec![2.; 15];
        occupations.resize(30, 0.);
        let column = &columns(&closed(&energies, &occupations))[0];
        assert_eq!(column.window(None), 10..=19);
        assert_eq!(column.window(Some(2)), 2..=19);
        assert_eq!(column.window(Some(29)), 10..=29);
    }
}
