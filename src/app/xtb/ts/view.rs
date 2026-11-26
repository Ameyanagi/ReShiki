//! Transition-state path profile and the saved saddle's details.
use super::Action;
use crate::app::workspace::{accessible_command, control, muted_text, panel};
use crate::app::xtb::palette::{Action as PaletteAction, Shown};
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{Action as XtbAction, state};
use crate::app::{App, Message};
use iced::widget::{canvas, column, container, row, text};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use reshiki::qcdata::{CalcResult, Frame, Status};

const KCAL_PER_HARTREE: f64 = 627.509_474_063;
const IMAGINARY_CM1: f64 = 50.;
const RELOAD_NOTE: &str = "Path images are recomputed when the search runs again";

fn send(action: Action) -> Message {
    Message::Xtb(XtbAction::Ts(action))
}

/// Keep a previous selection only while it indexes this profile. The climbing
/// image is the default, including after a different result replaces the path.
fn selection(count: usize, selected: Option<usize>, climbing: usize) -> Option<usize> {
    selected
        .filter(|&index| index < count)
        .or_else(|| (climbing < count).then_some(climbing))
        .or_else(|| (count > 0).then_some(0))
}

fn climbing_image(energies: &[f64], remembered: Option<usize>) -> usize {
    remembered
        .filter(|&index| index > 0 && index < energies.len().saturating_sub(1))
        .or_else(|| {
            energies
                .iter()
                .enumerate()
                .skip(1)
                .take(energies.len().saturating_sub(2))
                .reduce(|best, next| if next.1 > best.1 { next } else { best })
                .map(|(index, _)| index)
        })
        .unwrap_or(0)
}

fn profile_note(positions: bool, stale: bool) -> Option<&'static str> {
    if !positions {
        Some(RELOAD_NOTE)
    } else if stale {
        Some("The drawing changed; select an image to inspect its saved energy.")
    } else {
        None
    }
}

fn selection_label(index: usize, count: usize, climbing: usize, delta: f64) -> String {
    let role = if index == climbing {
        "climbing image"
    } else if index == 0 {
        "reactant"
    } else if index + 1 == count {
        "product"
    } else {
        "path image"
    };
    format!(
        "Image {} of {count} · {role} · ΔE {} kcal/mol",
        index + 1,
        state::signed(delta, 1)
    )
}

fn relative_energies(energies: &[f64], saddle: Option<f64>) -> (Vec<f64>, Option<f64>) {
    let Some(&reactant) = energies.first() else {
        return (vec![], None);
    };
    let delta = |energy| (energy - reactant) * KCAL_PER_HARTREE;
    (
        energies.iter().copied().map(delta).collect(),
        saddle.map(delta),
    )
}

impl App {
    pub(in crate::app::xtb) fn xtb_ts_path<'a>(
        &'a self,
        shown: &Shown<'a>,
        inspector: bool,
    ) -> Element<'a, Message> {
        let result = &shown.stored.result;
        let energies = super::path_energies(result);
        let climbing = climbing_image(energies, shown.ts.map(|s| s.extras.climbing));
        let selected = self.xtb_step().filter(|&index| index < energies.len());
        let current = selection(energies.len(), selected, climbing);
        let (deltas, refined_delta) = relative_energies(energies, result.energy_hartree);
        let positions = shown.ts.is_some_and(|session| {
            session.extras.image_positions.len() == energies.len()
                && session
                    .extras
                    .image_positions
                    .iter()
                    .all(|image| image.len() == shown.frame.numbers.len())
        });
        let interactive = !self.xtb_running();
        let chart = canvas(PathChart {
            deltas: deltas.clone(),
            refined_delta,
            selected: current,
            climbing,
            interactive,
            stale: shown.stale,
        })
        .width(Length::Fill)
        .height(190);
        let mut profile = column![chart].spacing(6);
        if let Some((index, delta)) =
            current.and_then(|index| deltas.get(index).map(|&delta| (index, delta)))
        {
            profile = profile
                .push(text(selection_label(index, energies.len(), climbing, delta)).size(12));
            // The buttons also make every canvas point keyboard accessible.
            let images = row(deltas.iter().enumerate().map(|(index, &delta)| {
                reshiki::accessibility::button(
                    format!("xtb-ts-image-{index}"),
                    selection_label(index, energies.len(), climbing, delta),
                    text((index + 1).to_string()).size(11),
                )
                .padding([3, 6])
                .checked(index == current.unwrap_or(climbing))
                .style(control(index == current.unwrap_or(climbing)))
                .on_press_maybe(interactive.then(|| send(Action::Image(index))))
                .into()
            }))
            .spacing(2)
            .wrap();
            profile = profile.push(images);
        } else {
            profile = profile.push(line("No path energy profile was saved.", Tone::Muted));
        }
        if selected.is_some() {
            profile = profile.push(
                accessible_command(
                    "xtb-ts-back",
                    "Return to the transition state result",
                    "Back to result",
                )
                .on_press(Message::Xtb(XtbAction::Palette(PaletteAction::StepBack))),
            );
        }
        if !energies.is_empty()
            && let Some(note) = profile_note(positions, shown.stale)
        {
            profile = profile.push(line(note, Tone::Muted));
        }

        let title = if energies.is_empty() {
            "Transition state".into()
        } else {
            format!(
                "Transition state · image {} of {}",
                climbing + 1,
                energies.len()
            )
        };
        let mut details = column![text(title).size(14)].spacing(7);
        if let (Some(forward), Some(ts), Some(&product)) =
            (refined_delta, result.energy_hartree, energies.last())
        {
            details = details.push(
                row![
                    barrier("ΔE‡ forward", forward),
                    barrier("ΔE‡ back", (ts - product) * KCAL_PER_HARTREE),
                ]
                .spacing(22)
                .wrap(),
            );
        }
        let imaginary = shown.ts.map_or_else(
            || imaginary_frequency(result),
            |session| session.extras.engine_imaginary,
        );
        details = details.push(match imaginary {
            Some(frequency) => row![
                line(
                    format!(
                        "Imaginary frequency {}i cm⁻¹",
                        state::signed(-frequency.abs(), 0)
                    ),
                    Tone::Good,
                ),
                accessible_command(
                    "xtb-ts-animate",
                    "Animate the transition state's imaginary mode",
                    "Animate",
                )
                .on_press_maybe(
                    (interactive && super::card::imaginary_mode(result).is_some()).then(|| {
                        Message::Xtb(XtbAction::Vibrations(
                            crate::app::xtb::vibrations::Action::AnimateImaginary,
                        ))
                    }),
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into(),
            None if shown.ts.is_some() => {
                line("No significant engine imaginary frequency.", Tone::Warning)
            }
            None => line("No significant imaginary frequency.", Tone::Warning),
        });
        if let Some(session) = shown.ts {
            let frequencies = &session.extras.worker_frequencies;
            if !frequencies.is_empty() {
                let imaginary: Vec<_> = frequencies
                    .iter()
                    .copied()
                    .filter(|&frequency| frequency < -IMAGINARY_CM1)
                    .collect();
                let values = imaginary
                    .iter()
                    .map(|&frequency| format!("{}i", state::signed(frequency, 0)))
                    .collect::<Vec<_>>()
                    .join(", ");
                details = details.push(line(
                    if values.is_empty() {
                        "Worker check: no significant imaginary frequency".into()
                    } else {
                        format!("Worker check: {values} cm⁻¹")
                    },
                    Tone::Muted,
                ));
            }
        }
        let maps: Vec<_> = shown
            .atoms
            .iter()
            .map(|id| {
                (!shown.stale)
                    .then(|| id.and_then(|id| self.tab.doc.atom(id)))
                    .flatten()
                    .map(|atom| atom.map_num)
                    .filter(|&number| number > 0)
            })
            .collect();
        let changes = self.xtb_ts_bond_changes(shown);
        for (kind, pair) in changes {
            if let Some(value) = bond_length(shown.frame, pair) {
                details = details.push(
                    row![
                        text(format!("{kind} {}", bond_label(shown.frame, pair, &maps)))
                            .size(12)
                            .width(Length::Fill),
                        text(format!("{value:.2} Å")).size(12),
                    ]
                    .spacing(10)
                    .wrap(),
                );
            }
        }
        let connects = shown
            .ts
            .map_or(matches!(result.status, Status::Converged), |s| {
                s.extras.connects
            });
        if connects {
            details = details.push(line("✓ IRC connects reactant and product", Tone::Good));
            if let Status::NotConverged { reason } = &result.status {
                details = details.push(line(reason.clone(), Tone::Warning));
            }
        } else {
            let reason = match &result.status {
                Status::NotConverged { reason } | Status::Failed { reason }
                    if reason.ends_with("on both sides") =>
                {
                    format!(
                        "{reason}. This saddle belongs to another reaction. Check the map or search with more images."
                    )
                }
                Status::NotConverged { reason } | Status::Failed { reason } => reason.clone(),
                Status::Converged => {
                    "IRC did not connect the intended reactant and product.".into()
                }
            };
            details = details.push(
                container(
                    column![
                        line(reason, Tone::Warning),
                        reshiki::accessibility::button(
                            "xtb-ts-rerun-20",
                            "Search with 20 images",
                            text("Search with 20 images").size(12),
                        )
                        .padding([5, 9])
                        .style(control(false))
                        .on_press_maybe(self.xtb_ts_can_rerun(shown).then(|| send(Action::Rerun))),
                    ]
                    .spacing(7),
                )
                .padding(10)
                .width(Length::Fill)
                .style(|theme: &Theme| container::Style {
                    background: Some(
                        Color {
                            a: 0.1,
                            ..theme.palette().warning
                        }
                        .into(),
                    ),
                    border: iced::Border {
                        color: Color {
                            a: 0.5,
                            ..theme.palette().warning
                        },
                        width: 1.,
                        radius: 6.into(),
                    },
                    ..Default::default()
                }),
            );
        }
        details = details.push(line(
            "Semi-empirical barriers are rough, often several kcal/mol off. Refine with DFT before quoting them.",
            Tone::Muted,
        ));
        details = details.push(
            reshiki::accessibility::button(
                "xtb-ts-export-input",
                "Export TS input",
                text("Export TS input…").size(12),
            )
            .padding([6, 10])
            .style(control(false))
            .on_press(send(Action::Export)),
        );
        let card = container(details)
            .padding(12)
            .width(Length::Fill)
            .style(panel);
        if inspector {
            column![profile, card].spacing(12).into()
        } else {
            row![
                container(profile).width(Length::FillPortion(3)),
                container(card).width(Length::FillPortion(2)),
            ]
            .spacing(18)
            .into()
        }
    }

    /// Compare the saved optimized endpoints, so reopened results use the same
    /// covalent-radius criterion as the search instead of the current drawing.
    fn xtb_ts_bond_changes(&self, shown: &Shown<'_>) -> Vec<(&'static str, [usize; 2])> {
        let Some(endpoints) = shown.stored.result.endpoints else {
            return vec![];
        };
        let science = &self.tab.doc.science;
        let (Some(reactant), Some(product)) = (
            science.frame(&endpoints.reactant),
            science.frame(&endpoints.product),
        ) else {
            return vec![];
        };
        changed_bonds(reactant, product)
    }
}

fn barrier<'a>(label: &'static str, value: f64) -> Element<'a, Message> {
    column![
        text(label).size(11).style(muted_text),
        text(format!("{} kcal/mol", state::signed(value, 1))).size(15),
    ]
    .spacing(2)
    .into()
}

fn imaginary_frequency(result: &CalcResult) -> Option<f64> {
    super::card::imaginary_mode(result).map(|(_, frequency)| frequency)
}

fn bond_label(frame: &Frame, [a, b]: [usize; 2], maps: &[Option<u32>]) -> String {
    let atom = |index: usize| {
        format!(
            "{}{}",
            state::symbol(frame.numbers.get(index).copied().unwrap_or(0)),
            maps.get(index)
                .copied()
                .flatten()
                .map_or(index + 1, |number| number as usize)
        )
    };
    format!("{}···{}", atom(a), atom(b))
}

fn bond_length(frame: &Frame, [a, b]: [usize; 2]) -> Option<f64> {
    let a = frame.positions_angstrom.get(a)?;
    let b = frame.positions_angstrom.get(b)?;
    Some(
        a.iter()
            .zip(b)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt(),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn changed_bonds(reactant: &Frame, product: &Frame) -> Vec<(&'static str, [usize; 2])> {
    use std::collections::BTreeSet;
    if reactant.numbers != product.numbers {
        return vec![];
    }
    let graph = |frame: &Frame| {
        let positions: Vec<_> = frame
            .positions_angstrom
            .iter()
            .flatten()
            .map(|value| value / reshiki_optimize::ANGSTROM_PER_BOHR)
            .collect();
        reshiki_optimize::ts::bonds(&frame.numbers, &positions)
            .ok()
            .map(|bonds| bonds.into_iter().collect::<BTreeSet<_>>())
    };
    let (Some(reactant), Some(product)) = (graph(reactant), graph(product)) else {
        return vec![];
    };
    product
        .difference(&reactant)
        .map(|&(a, b)| ("Forming", [a, b]))
        .chain(
            reactant
                .difference(&product)
                .map(|&(a, b)| ("Breaking", [a, b])),
        )
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn changed_bonds(_: &Frame, _: &Frame) -> Vec<(&'static str, [usize; 2])> {
    vec![]
}

/// Reactant-relative energy against the NEB image index.
struct PathChart {
    deltas: Vec<f64>,
    /// The optimized saddle can differ from its climbing NEB image.
    refined_delta: Option<f64>,
    selected: Option<usize>,
    climbing: usize,
    interactive: bool,
    stale: bool,
}

impl PathChart {
    const LEFT: f32 = 48.;
    const RIGHT: f32 = 24.;
    const TOP: f32 = 34.;
    const BOTTOM: f32 = 34.;

    fn x(&self, index: usize, bounds: Rectangle) -> f32 {
        let width = (bounds.width - Self::LEFT - Self::RIGHT).max(1.);
        Self::LEFT + width * index as f32 / self.deltas.len().saturating_sub(1).max(1) as f32
    }

    fn nearest(&self, x: f32, bounds: Rectangle) -> Option<usize> {
        (0..self.deltas.len()).min_by(|&a, &b| {
            (self.x(a, bounds) - x)
                .abs()
                .total_cmp(&(self.x(b, bounds) - x).abs())
        })
    }

    fn energy_bounds(&self) -> (f64, f64) {
        self.deltas
            .iter()
            .chain(self.refined_delta.iter())
            .fold((0_f64, 0_f64), |(lo, hi), &energy| {
                (lo.min(energy), hi.max(energy))
            })
    }

    fn refined_ts_marker(&self) -> Option<(f64, String)> {
        self.refined_delta
            .map(|delta| (delta, format!("TS‡ {}", state::signed(delta, 1))))
    }
}

impl canvas::Program<Message> for PathChart {
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
        let point = cursor.position_in(bounds).filter(|_| self.interactive)?;
        let image = self.nearest(point.x, bounds)?;
        Some(canvas::Action::publish(send(Action::Image(image))).and_capture())
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.interactive && cursor.is_over(bounds) && !self.deltas.is_empty() {
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
        let ink = if self.stale {
            muted
        } else {
            theme.palette().text
        };
        let amber = crate::appearance::readable(
            theme.palette().warning,
            &[theme.palette().background],
            reshiki::color_contrast::TEXT_TARGET,
        );
        frame.fill_text(canvas::Text {
            content: "ΔE (kcal/mol, reactant = 0)".into(),
            position: Point::new(Self::LEFT, 3.),
            color: muted,
            size: 10.into(),
            ..Default::default()
        });
        if self.deltas.is_empty() {
            return vec![frame.into_geometry()];
        }
        let (lo, hi) = self.energy_bounds();
        let range = (hi - lo).max(1.);
        let plot_height = (bounds.height - Self::TOP - Self::BOTTOM).max(1.);
        let y = |energy: f64| Self::TOP + ((hi - energy) / range) as f32 * plot_height;
        let grid = Color { a: 0.25, ..muted };
        for energy in [hi, 0., lo] {
            let start = Point::new(Self::LEFT, y(energy));
            frame.stroke(
                &canvas::Path::line(start, Point::new(bounds.width - Self::RIGHT, start.y)),
                canvas::Stroke::default().with_color(grid).with_width(1.),
            );
        }
        for energy in [hi, lo] {
            frame.fill_text(canvas::Text {
                content: state::signed(energy, 1),
                position: Point::new(Self::LEFT - 7., y(energy) - 6.),
                color: muted,
                size: 10.into(),
                align_x: iced::alignment::Horizontal::Right.into(),
                ..Default::default()
            });
        }
        let points: Vec<_> = self
            .deltas
            .iter()
            .enumerate()
            .map(|(index, &energy)| Point::new(self.x(index, bounds), y(energy)))
            .collect();
        if let Some(point) = self.selected.and_then(|index| points.get(index)) {
            frame.stroke(
                &canvas::Path::line(
                    Point::new(point.x, Self::TOP),
                    Point::new(point.x, bounds.height - Self::BOTTOM),
                ),
                canvas::Stroke::default()
                    .with_color(Color {
                        a: 0.25,
                        ..theme.palette().primary
                    })
                    .with_width(1.),
            );
        }
        frame.stroke(
            &canvas::Path::new(|path| {
                for (index, &point) in points.iter().enumerate() {
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
            }),
            canvas::Stroke::default().with_color(ink).with_width(1.5),
        );
        for (index, (&point, &delta)) in points.iter().zip(&self.deltas).enumerate() {
            let selected = self.selected == Some(index);
            if selected {
                frame.stroke(
                    &canvas::Path::circle(point, 6.),
                    canvas::Stroke::default().with_color(ink).with_width(1.),
                );
            }
            frame.fill(
                &canvas::Path::circle(point, if selected { 3.8 } else { 2.6 }),
                ink,
            );
            let label = if index == 0 {
                Some("R 0.0".into())
            } else if index + 1 == self.deltas.len() {
                Some(format!("P {}", state::signed(delta, 1)))
            } else {
                None
            };
            if let Some(content) = label {
                frame.fill_text(canvas::Text {
                    content,
                    position: Point::new(point.x, point.y + 7.),
                    color: ink,
                    size: 11.into(),
                    align_x: iced::alignment::Horizontal::Center.into(),
                    ..Default::default()
                });
            }
        }
        if let Some((delta, content)) = self.refined_ts_marker()
            && let Some(climbing) = points.get(self.climbing)
        {
            let point = Point::new(climbing.x, y(delta));
            frame.stroke(
                &canvas::Path::new(|path| {
                    path.move_to(Point::new(point.x, point.y - 6.5));
                    path.line_to(Point::new(point.x + 6.5, point.y));
                    path.line_to(Point::new(point.x, point.y + 6.5));
                    path.line_to(Point::new(point.x - 6.5, point.y));
                    path.close();
                }),
                canvas::Stroke::default().with_color(amber).with_width(1.4),
            );
            let label_y = point.y + if point.y >= climbing.y { 8. } else { -19. };
            frame.fill_text(canvas::Text {
                content,
                position: Point::new(point.x, label_y),
                color: amber,
                size: 11.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                ..Default::default()
            });
        }
        frame.fill_text(canvas::Text {
            content: "NEB image index".into(),
            position: Point::new(
                (Self::LEFT + bounds.width - Self::RIGHT) / 2.,
                bounds.height - 13.,
            ),
            color: muted,
            size: 10.into(),
            align_x: iced::alignment::Horizontal::Center.into(),
            ..Default::default()
        });
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
