use super::super::state::signed;
use super::view::{send, wavenumber};
use super::{Action, State};
use crate::app::Message;
use crate::app::workspace::{control, muted_text};
use iced::widget::{column, row, scrollable, text};
use iced::{Alignment, Color, Element, Length, Task, Theme};
use reshiki::qcdata::Vibrations;

const LIST_ID: &str = "xtb-vibration-modes";
const ROW_HEIGHT: f32 = 21.;
const FREQUENCY_WIDTH: f32 = 43.;
const INTENSITY_WIDTH: f32 = 61.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Sort {
    #[default]
    Intensity,
    Wavenumber,
}

fn intensity(vibrations: &Vibrations, index: usize) -> Option<f64> {
    vibrations.ir_intensities_km_mol.get(index).copied()
}

/// Imaginary frequencies keep their original order. Real bands are ranked by
/// intensity or frequency, with the higher frequency first when bands tie.
pub(super) fn order(vibrations: &Vibrations, sort: Sort) -> Vec<usize> {
    let mut bands: Vec<_> = vibrations
        .frequencies_cm1
        .iter()
        .copied()
        .enumerate()
        .collect();
    bands.sort_by(|&(a, fa), &(b, fb)| match (fa < 0., fb < 0.) {
        (true, true) => a.cmp(&b),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => {
            if sort == Sort::Intensity {
                intensity(vibrations, b)
                    .unwrap_or_default()
                    .partial_cmp(&intensity(vibrations, a).unwrap_or_default())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| fb.total_cmp(&fa))
            } else {
                fb.total_cmp(&fa)
            }
        }
    });
    bands.into_iter().map(|(index, _)| index).collect()
}

pub(super) fn mode_list<'a>(
    vibrations: &Vibrations,
    settings: &State,
    names: &[String],
    recomputing: bool,
    selected: usize,
    stale: bool,
    inspector: bool,
) -> Element<'a, Message> {
    let mut list = column![].spacing(3);
    if !vibrations.ir_intensities_km_mol.is_empty() {
        list = list.push(
            row([
                (Sort::Intensity, "Strongest first", "intensity"),
                (Sort::Wavenumber, "By wavenumber", "wavenumber"),
            ]
            .map(|(sort, label, id)| {
                let active = settings.sort == sort;
                reshiki::accessibility::button(
                    format!("xtb-vibration-sort-{id}"),
                    label,
                    text(label).size(11).center().width(Length::Fill),
                )
                .width(Length::Fill)
                .padding([3, 5])
                .checked(active)
                .style(control(active))
                .on_press(send(Action::Sort(sort)))
                .into()
            }))
            .spacing(4),
        );
    }
    let header = |label| text(label).size(11).style(muted_text);
    list = list.push(
        row![
            header("cm⁻¹").width(FREQUENCY_WIDTH),
            header("IR (km/mol)").width(INTENSITY_WIDTH),
            header("Motion"),
        ]
        .spacing(6)
        .padding([0, 8]),
    );
    let rows = column(
        order(vibrations, settings.sort)
            .into_iter()
            .filter_map(|index| {
                let frequency = *vibrations.frequencies_cm1.get(index)?;
                let imaginary = frequency < 0.;
                let band = if imaginary {
                    format!("{}i", signed(frequency, 0))
                } else {
                    format!("{frequency:.0}")
                };
                let ir = intensity(vibrations, index);
                let motion = names.get(index).map_or("", String::as_str);
                let mut label = format!("Mode {}, {}", index + 1, wavenumber(frequency));
                if let Some(ir) = ir {
                    label.push_str(&format!(", {ir:.0} km/mol"));
                }
                if !motion.is_empty() {
                    label.push_str(&format!(", {motion}"));
                }
                if imaginary {
                    label.push_str(", imaginary");
                }
                let cell = |value: String| {
                    text(value).size(12).wrapping(text::Wrapping::None).style(
                        move |theme: &Theme| text::Style {
                            color: Some(if imaginary {
                                theme.palette().danger
                            } else if stale {
                                Color {
                                    a: 0.55,
                                    ..crate::appearance::muted(theme)
                                }
                            } else {
                                theme.palette().text
                            }),
                        },
                    )
                };
                let active = selected == index;
                Some(
                    reshiki::accessibility::button(
                        mode_id(index),
                        label,
                        row![
                            cell(band).width(FREQUENCY_WIDTH),
                            cell(ir.map_or_else(|| "—".into(), |ir| format!("{ir:.0}")))
                                .width(INTENSITY_WIDTH),
                            cell(if recomputing { "…" } else { motion }.into()),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([0, 8])
                    .width(Length::Fill)
                    .height(ROW_HEIGHT)
                    .checked(active)
                    .style(control(active))
                    .on_press(send(Action::Mode(index)))
                    .into(),
                )
            }),
    )
    .padding(iced::Padding::ZERO.right(10));
    list.push(
        scrollable(rows)
            .id(LIST_ID)
            .width(Length::Fill)
            .height(if inspector { 147. } else { 105. }),
    )
    .into()
}

fn mode_id(index: usize) -> String {
    format!("xtb-mode-{}", index + 1)
}

/// Scroll the sorted row and its enclosing dock or inspector into view.
pub(super) fn reveal(index: usize) -> Task<Message> {
    iced::advanced::widget::operate(reshiki::accessibility::FocusControl::reveal(mode_id(index)))
        .discard()
}
