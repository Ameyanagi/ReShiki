//! The Orbitals tab's widgets: level diagram, list, surface controls and
//! Place on canvas.
// Column and level indices come from the choice made for these columns.
#![allow(clippy::indexing_slicing)]
use super::levels::Diagram;
use super::{Action, ISO_RANGE, LookChoice, PRESETS, send};
use crate::app::workspace::{accessible_checkbox, accessible_command, control, muted_text};
use crate::app::xtb::palette::Shown;
use crate::app::xtb::tone::{Tone, line};
use crate::app::{App, Message};
use iced::widget::{Column, Space, canvas, column, container, row, slider, text};
use iced::{Alignment, Color, Element, Length, Theme};
use reshiki::qcdata::Settings;

/// The height of the diagram and level list in the bottom dock.
const LEVELS_HEIGHT: f32 = 142.;

impl App {
    pub(in crate::app::xtb) fn xtb_orbitals_tab(&self, inspector: bool) -> Element<'_, Message> {
        let Some(target) = self.xtb_orbital_target() else {
            return line("This result has no orbital levels.", Tone::Muted);
        };
        let choice = self.xtb_orbital_choice(&target);
        let live = target.problem.is_none();
        let diagram = canvas(Diagram {
            columns: target.columns.clone(),
            selected: Some((choice.column, choice.index)),
            live,
        })
        .width(Diagram::width(target.columns.len()))
        .height(if inspector { 220. } else { LEVELS_HEIGHT });

        // The list: HOMO and LUMO first, the spin switch for open shells,
        // then the levels around the gap, highest first.
        let spectrum = &target.columns[choice.column];
        let mut list = Column::new().spacing(2);
        let quick = |id: &'static str, label: &'static str, action, on: bool| {
            reshiki::accessibility::button(id, format!("Show the {label}"), text(label).size(12))
                .padding([4, 10])
                .checked(on)
                .style(control(on))
                .on_press_maybe(live.then(|| send(action)))
        };
        let mut head = row![
            quick(
                "xtb-orbital-homo",
                "HOMO",
                Action::Homo,
                spectrum.homo() == Some(choice.index)
            ),
            quick(
                "xtb-orbital-lumo",
                "LUMO",
                Action::Lumo,
                spectrum.lumo() == Some(choice.index)
            ),
        ]
        .spacing(4);
        if target.columns.len() > 1 {
            head = head.push(Space::new().width(8));
            for (index, column) in target.columns.iter().enumerate() {
                let on = index == choice.column;
                head = head.push(
                    reshiki::accessibility::button(
                        format!("xtb-orbital-spin-{index}"),
                        format!("{} spin orbitals", column.label),
                        text(column.label).size(12),
                    )
                    .padding([4, 10])
                    .checked(on)
                    .style(control(on))
                    .on_press(send(Action::Column(index))),
                );
            }
        }
        list = list.push(head);
        for index in spectrum.window(Some(choice.index)).rev() {
            let on = index == choice.index;
            let group = spectrum.group(index);
            let fold = group.end() - group.start() + 1;
            let mut name = spectrum.name(index);
            if fold > 1 {
                name.push_str(&format!(" · {fold}-fold"));
            }
            let energy = format!(
                "{} eV",
                super::super::state::signed(spectrum.energies_ev[index], 2)
            );
            list = list.push(
                reshiki::accessibility::button(
                    format!("xtb-orbital-{}-{index}", choice.column),
                    format!("{name}, {energy}"),
                    row![
                        text(name).size(12).width(Length::Fill),
                        text(energy).size(12),
                    ]
                    .spacing(8),
                )
                .padding([3, 8])
                .width(Length::Fill)
                .checked(on)
                .style(control(on))
                .on_press_maybe(live.then(|| send(Action::Choose(choice.column, index)))),
            );
        }
        list = list.push(line(
            "Click a level, then ↑/↓ move through the levels.",
            Tone::Muted,
        ));
        let list: Element<'_, Message> = if inspector {
            list.into()
        } else {
            iced::widget::scrollable(container(list).padding(iced::Padding::ZERO.right(10)))
                .height(LEVELS_HEIGHT)
                .width(230)
                .into()
        };

        let controls = self.xtb_orbital_controls(&target, &choice, live);
        if inspector {
            return column![diagram, list, controls].spacing(10).into();
        }
        row![diagram, list, container(controls).width(Length::Fill)]
            .spacing(18)
            .into()
    }

    fn xtb_orbital_controls<'a>(
        &'a self,
        target: &super::Target,
        choice: &super::Choice,
        live: bool,
    ) -> Element<'a, Message> {
        let (iso, look) = self.xtb_orbital_settings(target);
        let field = |label: &'static str| text(label).size(12).width(70);
        let presets = row(PRESETS.into_iter().map(|preset| {
            let on = (iso - preset).abs() < 1e-4;
            reshiki::accessibility::button(
                format!("xtb-orbital-iso-{preset}"),
                format!("Isovalue {preset:.2}"),
                text(format!("{preset:.2}")).size(11),
            )
            .padding([2, 6])
            .checked(on)
            .style(control(on))
            .on_press_maybe(live.then(|| send(Action::Preset(preset))))
            .into()
        }))
        .spacing(2);
        let isovalue = column![
            row![
                field("Isovalue"),
                slider(ISO_RANGE, iso, move |iso| {
                    send(if live {
                        Action::Iso(iso)
                    } else {
                        Action::Settle
                    })
                })
                .step(0.005_f32)
                .on_release(send(Action::Settle))
                .width(Length::Fill),
                text(format!("{iso:.3}")).size(12).width(42),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            row![
                Space::new().width(76),
                presets,
                text("a.u.").size(11).style(muted_text)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(4);
        let chosen = LookChoice::of(look);
        let looks = row(LookChoice::ALL.into_iter().map(|choice| {
            let on = choice == chosen;
            reshiki::accessibility::button(
                format!("xtb-orbital-look-{}", choice.name().to_lowercase()),
                format!("{} look", choice.name()),
                text(choice.name()).size(12),
            )
            .padding([4, 8])
            .checked(on)
            .style(control(on))
            .on_press_maybe(live.then(|| send(Action::Look(choice))))
            .into()
        }))
        .spacing(2);
        let opacity = row![
            field("Opacity"),
            slider(20_f32..=100., f32::from(look.opacity), move |v| {
                send(if live {
                    Action::Opacity(v.round() as u8)
                } else {
                    Action::Settle
                })
            })
            .step(1_f32)
            .on_release(send(Action::Settle))
            .width(Length::Fill),
            text(format!("{}%", look.opacity)).size(12).width(42),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let mesh_lines = accessible_checkbox(
            "xtb-orbital-mesh-lines",
            "Mesh lines",
            look.mesh,
            (live && chosen != LookChoice::Mesh).then(|| send(Action::MeshLines(!look.mesh))),
        );
        let dark = self.tab.doc.canvas_theme.is_dark();
        let swatch = |rgb: [u8; 3]| {
            container(Space::new().width(14).height(14)).style(move |_: &Theme| container::Style {
                background: Some(Color::from_rgb8(rgb[0], rgb[1], rgb[2]).into()),
                border: iced::Border {
                    color: Color::from_rgba8(0, 0, 0, 0.25),
                    width: 1.,
                    radius: 3.0.into(),
                },
                ..Default::default()
            })
        };
        let [positive, negative] = reshiki::models3d::surface::draw::figure_colors(dark);
        let colours = row![
            field("Phases"),
            swatch(positive),
            text("+").size(12),
            swatch(negative),
            text("−").size(12),
            text("from the canvas theme").size(11).style(muted_text),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let action: Element<'_, Message> = if target.placed.is_some() {
            row![
                line("Placed on the canvas", Tone::Good),
                accessible_command(
                    "xtb-orbital-remove",
                    "Remove the orbital from the canvas",
                    "Remove from canvas"
                )
                .on_press(send(Action::Remove)),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        } else {
            let ready = live && self.xtb_orbital_ready(target, choice);
            reshiki::accessibility::button(
                "xtb-orbital-place",
                "Place the orbital on the canvas",
                text("Place on canvas").size(12),
            )
            .padding([6, 14])
            .style(crate::appearance::primary)
            .on_press_maybe(ready.then(|| send(Action::Place)))
            .into()
        };
        let mut controls = column![
            isovalue,
            row![field("Look"), looks]
                .spacing(6)
                .align_y(Alignment::Center),
            mesh_lines,
            opacity,
            colours,
            action,
        ]
        .spacing(8)
        .max_width(380);
        if let Some(problem) = &target.problem {
            controls = controls.push(line(problem.clone(), Tone::Warning));
        } else if let Some(busy) = self.xtb_orbital_busy(target) {
            controls = controls.push(line(busy, Tone::Muted));
        }
        controls.into()
    }

    /// The footer line under the Orbitals tab.
    pub(in crate::app::xtb) fn xtb_orbitals_note(
        &self,
        shown: &Shown<'_>,
        wrapping: text::Wrapping,
    ) -> Element<'_, Message> {
        let note = match &shown.stored.result.settings {
            Settings::Xtb(_) => {
                "Orbitals are recomputed when shown (≈0.1 s); a placed orbital is saved with the file."
            }
            Settings::Imported { .. } => {
                "Orbitals from the opened file; a placed orbital is saved with the file."
            }
        };
        text(note)
            .size(11)
            .style(muted_text)
            .wrapping(wrapping)
            .into()
    }
}
