//! Review both reaction sides while keeping uncommon calculation settings folded.
use super::{Action, Mapping, Mode, Tone, line, send, species};
use crate::app::help::{Placement, modal};
use crate::app::workspace::accessible_command;
use crate::app::{App, Message};
use iced::widget::{Space, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

impl App {
    pub(in crate::app) fn with_ts_review<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(dialog) = &self.tab.xtb.ts_review else {
            return base;
        };
        if !dialog.visible() {
            return base;
        }
        let current = dialog.at == self.review_stamp();
        let mut content = column![
            text(dialog.heading()).size(21),
            line(dialog.subtitle(), Tone::Muted),
        ]
        .spacing(12);
        let changes = dialog.changes.as_ref().ok();
        if let Some(pictures) = &dialog.pictures {
            let mut sides = row![].spacing(8);
            for picture in pictures {
                if let Some(thumbnail) = picture.preview.marked_thumbnail(&picture.marks, 160) {
                    sides = sides.push(
                        column![text(picture.label).size(13), thumbnail]
                            .spacing(4)
                            .width(Length::FillPortion(1)),
                    );
                }
            }
            let label = match dialog.mapping {
                Mapping::Proposed(_) => "Proposed changes",
                Mapping::Finding => "Matching atoms…",
                _ => "Drawn atom map",
            };
            let mut picture = column![sides, text(label).size(13)].spacing(6);
            if let Some(changes) = changes
                && !changes.bonds.is_empty()
            {
                picture = picture.push(line(&changes.bonds, Tone::Muted));
            }
            content = content.push(container(picture).padding(10).width(Length::Fill).style(
                |theme: &iced::Theme| container::Style {
                    border: iced::Border {
                        color: theme.palette().primary,
                        width: 2.,
                        radius: 6.into(),
                    },
                    ..Default::default()
                },
            ));
        }
        if let Mapping::Proposed(proposal) = &dialog.mapping {
            if !proposal.complete {
                content = content.push(line(
                    "The search stopped before all atom maps were checked.",
                    Tone::Muted,
                ));
            }
            if proposal.unmatched_reactants != 0 || proposal.unmatched_products != 0 {
                content = content.push(line(
                    format!(
                        "{} reactant / {} product atoms still unmatched",
                        proposal.unmatched_reactants, proposal.unmatched_products
                    ),
                    Tone::Muted,
                ));
            }
        }
        if let Some(changes) = changes
            && !changes.hydrogens.is_empty()
        {
            content = content.push(line("Hydrogens that move", Tone::Muted));
            for entry in &changes.hydrogens {
                content = content.push(line(entry, Tone::Muted));
            }
        }
        if dialog.inferred {
            content = content.push(line(
                "Reactants and products detected from the drawing",
                Tone::Muted,
            ));
        }
        if let Some(ignored) = &dialog.ignored {
            content = content.push(line(ignored, Tone::Muted));
        }
        if let Some(note) = dialog.target_stereo_note {
            content = content.push(line(note, Tone::Muted));
        }
        if dialog.unmapped_isotope_hydrogen {
            content = content.push(line(
                "Isotope-labelled hydrogens need an explicit atom map.",
                Tone::Muted,
            ));
        }
        content = content.push(
            row![
                container(line(dialog.state_summary(), Tone::Muted)).width(Length::Fill),
                text("·").size(11),
                accessible_command(
                    "ts.review.state",
                    "Edit charges and unpaired electrons",
                    if dialog.edit_species { "Done" } else { "Edit" },
                )
                .checked(dialog.edit_species)
                .on_press(send(Action::EditSpecies)),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
        if dialog.edit_species {
            for (index, input) in dialog.inputs.iter().enumerate() {
                let field =
                    |label: &'static str, value: &'a str, action: fn(usize, String) -> Action| {
                        column![
                            text(label).size(11),
                            reshiki::accessibility::text_input(
                                format!("ts.review.{index}.{label}"),
                                format!("{} · {label}", input.label),
                                "",
                                value,
                            )
                            .style(crate::appearance::input_style)
                            .size(13)
                            .padding(5)
                            .width(94)
                            .on_input(move |value| send(action(index, value)))
                        ]
                        .spacing(3)
                    };
                content = content.push(
                    column![
                        text(&input.label).size(13),
                        row![
                            field("Charge", &input.charge, Action::Charge),
                            field("Unpaired electrons", &input.unpaired, Action::Unpaired),
                        ]
                        .spacing(10),
                    ]
                    .spacing(5),
                );
            }
        }
        if dialog.mode == Mode::TransitionState && !dialog.rerun {
            content = content.push(
                accessible_command(
                    "ts.review.advanced",
                    "Advanced (path points)",
                    if dialog.advanced {
                        "▾ Advanced (path points)"
                    } else {
                        "▸ Advanced (path points)"
                    },
                )
                .checked(dialog.advanced)
                .on_press(send(Action::Advanced)),
            );
            if dialog.advanced {
                content = content.push(
                    row![
                        text("Path points (10–20)").size(12),
                        reshiki::accessibility::text_input(
                            "ts.review.path-points",
                            "Path points",
                            "15",
                            &dialog.path_points,
                        )
                        .style(crate::appearance::input_style)
                        .size(13)
                        .padding(5)
                        .width(94)
                        .on_input(|value| send(Action::PathPoints(value))),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                );
            }
        }
        if dialog.free_energy {
            content = content.push(line(
                if dialog.mode == Mode::TransitionState {
                    "Report ΔG and ΔG‡ at 298.15 K"
                } else {
                    "Report ΔG at 298.15 K"
                },
                Tone::Muted,
            ));
        }
        if dialog.mode == Mode::Energy {
            if let Mapping::Failed(reason) = &dialog.mapping {
                content = content.push(line(reason, Tone::Muted));
            } else if let Err(reason) = &dialog.changes
                && !matches!(dialog.mapping, Mapping::Finding)
            {
                content = content.push(line(reason.to_string(), Tone::Muted));
            }
        }
        let valid = if current {
            let checked = species::validate(&dialog.inputs);
            if checked.is_ok() {
                content = content.push(line("Atoms and charge balance", Tone::Good));
            }
            checked.and_then(|species| dialog.can_run(&species))
        } else {
            Err("The reaction drawing changed. Close this review and try again.".into())
        };
        if let Err(error) = &valid {
            content = content.push(line(
                error,
                if matches!(dialog.mapping, Mapping::Finding) {
                    Tone::Muted
                } else {
                    Tone::Error
                },
            ));
        }
        let run = dialog.run_label();
        content = content
            .push(
                row![
                    accessible_command("ts.review.edit", "Edit map", "Edit map").on_press_maybe(
                        (current && !matches!(dialog.mapping, Mapping::Finding))
                            .then(|| send(Action::EditMap))
                    ),
                    Space::new().width(Length::Fill),
                    accessible_command("ts.review.cancel", "Cancel", "Cancel")
                        .on_press(send(Action::Cancel)),
                ]
                .spacing(8),
            )
            .push(
                accessible_command("ts.review.accept", run.clone(), run)
                    .width(Length::Fill)
                    .style(crate::appearance::primary)
                    .on_press_maybe(valid.is_ok().then(|| send(Action::Accept))),
            );
        modal(
            base,
            scrollable(content).into(),
            640.,
            700.,
            Placement::Fit,
            send(Action::Cancel),
        )
    }
}
