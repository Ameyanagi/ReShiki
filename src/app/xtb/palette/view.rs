//! The results palette's widgets: header, method lines, the stale banner,
//! the result tabs, and the footer with file sizes.
use super::super::results::{self, View};
use super::super::tone::{Tone, line};
use super::super::{Action as XtbAction, Failed, state};
use super::{Action, Shown};
use crate::app::workspace::{accessible_command, control, muted_text, panel};
use crate::app::{App, Message};
use iced::widget::{
    Column, Space, button, canvas, checkbox, column, container, row, scrollable, text,
};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};

/// The bottom dock's body height; the inspector shows the body in full.
const BODY_HEIGHT: f32 = 150.;
const LABEL_WIDTH: f32 = 130.;

fn send(action: Action) -> Message {
    Message::Xtb(XtbAction::Palette(action))
}

/// A checkbox whose label can change, such as "Keep full results (+84 KB)".
fn toggle<'a>(
    id: &'static str,
    label: String,
    checked: bool,
    action: Action,
) -> Element<'a, Message> {
    let message = send(action);
    reshiki::accessibility::button(
        id,
        label.clone(),
        checkbox(checked)
            .label(label)
            .on_toggle({
                let message = message.clone();
                move |_| message.clone()
            })
            .size(14)
            .text_size(12),
    )
    .checked(checked)
    .on_press(message)
    .padding(0)
    .style(|theme: &Theme, _| button::Style {
        text_color: theme.palette().text,
        ..Default::default()
    })
    .into()
}

/// Text in the palette: greyed while the result is stale.
fn greyed(stale: bool) -> impl Fn(&Theme) -> text::Style {
    move |theme| text::Style {
        color: Some(if stale {
            Color {
                a: 0.55,
                ..crate::appearance::muted(theme)
            }
        } else {
            theme.palette().text
        }),
    }
}

impl App {
    /// The palette under the status bar, when it is docked there.
    pub(in crate::app) fn xtb_bottom_palette(&self) -> Option<Element<'_, Message>> {
        if self.xtb.dock.inspector {
            return None;
        }
        let content = self.xtb_palette_content(false)?;
        Some(
            container(content)
                .padding([10, 16])
                .width(Length::Fill)
                .style(panel)
                .into(),
        )
    }

    /// The palette docked in the inspector, below the Calculate card.
    pub(in crate::app) fn xtb_inspector_palette(&self) -> Option<Element<'_, Message>> {
        if !self.xtb.dock.inspector {
            return None;
        }
        self.xtb_palette_content(true)
    }

    fn xtb_palette_content(&self, inspector: bool) -> Option<Element<'_, Message>> {
        if !self.tab.xtb.palette.open {
            return None;
        }
        let collapsed = self.xtb.dock.collapsed && !inspector;
        let shown = self.xtb_shown();
        // The reaction on display, else the selected one.
        let arrow = shown
            .as_ref()
            .and_then(|shown| shown.arrow)
            .or_else(|| self.xtb_selected_reaction());
        let failed = self.xtb_palette_failure(arrow);
        let reaction = failed
            .and_then(|failed| failed.ts())
            .map(|request| request.context.title.clone())
            .or_else(|| {
                let shown = shown.as_ref()?;
                super::ts::is_ts(&shown.stored.result).then(|| {
                    if !shown.stale
                        && let Some(arrow) = shown.arrow
                    {
                        self.xtb_reaction_label(arrow).title()
                    } else {
                        self.xtb_saved_reaction_title(&shown.stored.result, shown.arrow)
                    }
                })
            });
        // Results opened from other programs are not xTB's.
        let title = match shown.as_ref().map(|s| &s.stored.result.settings) {
            Some(reshiki::qcdata::Settings::Imported { .. }) => "Calculation results".into(),
            _ => reaction.map_or_else(
                || "xTB results".into(),
                |title| format!("xTB results · {title}"),
            ),
        };
        if let Some(failed) = failed.filter(|failed| failed.ts().is_none() || shown.is_none()) {
            let header = self.xtb_palette_header(title, vec![], View::Summary, inspector);
            return Some(if collapsed {
                header
            } else {
                column![header, self.xtb_failure_card(failed)]
                    .extend(self.xtb_previous_result(failed, inspector))
                    .spacing(8)
                    .into()
            });
        }
        let Some(shown) = shown else {
            if arrow.is_some() {
                return None;
            }
            let header = self.xtb_palette_header(title, vec![], View::Summary, inspector);
            return Some(if collapsed {
                header
            } else {
                column![
                    header,
                    line("No xTB result for the selected molecule yet.", Tone::Muted),
                ]
                .spacing(8)
                .into()
            });
        };
        let palette = &self.tab.xtb.palette;
        let views = View::available(&shown.stored.result);
        let view = if views.contains(&palette.view) {
            palette.view
        } else {
            View::Summary
        };
        let header = self.xtb_palette_header(title, views, view, inspector);
        if collapsed {
            return Some(header);
        }

        let method = shown.method();
        let mut content = column![header].spacing(4);
        if let Some(failed) = failed.filter(|failed| failed.ts().is_some()) {
            content = content.push(self.xtb_failure_card(failed));
            content = content.push(line(
                results::previous_result(&shown.stored.result),
                Tone::Muted,
            ));
        }
        content = content.push(text(method.clone()).size(12));
        // The semi-empirical caveat is about xTB, not imported results.
        if matches!(
            shown.stored.result.settings,
            reshiki::qcdata::Settings::Xtb(_)
        ) {
            content = content.push(line(results::LIMITATIONS, Tone::Muted));
        }
        if shown.stale {
            content = content.push(self.xtb_stale_banner(&shown));
        }
        if let Some(warning) = results::warning(&shown.stored.result, shown.info) {
            content = content.push(alert(true, warning.into(), String::new(), vec![]));
        }
        let body = match view {
            View::Summary => summary(&shown, inspector, false),
            View::Vibrations => self.xtb_vibrations(&shown, inspector),
            View::Steps => self.xtb_steps(&shown, inspector),
            View::Orbitals => self.xtb_orbitals_tab(inspector),
            View::Conformers => self.xtb_conformers_tab(&shown, inspector),
            View::Path => self.xtb_ts_path(&shown, inspector),
            View::Log => log(&shown, &method),
        };
        let body: Element<'_, Message> = if inspector {
            body
        } else {
            scrollable(container(body).padding(iced::Padding::ZERO.right(12)))
                .id("xtb-results-body")
                .height(if view == View::Orbitals {
                    super::super::orbitals::HEIGHT
                } else {
                    BODY_HEIGHT
                })
                .into()
        };
        content = content.push(container(body).padding([6, 0]));
        content = content.push(self.xtb_footer(&shown, view, inspector));
        Some(content.into())
    }

    /// "xTB results", the tabs `views` with `view` chosen, and the dock commands.
    fn xtb_palette_header(
        &self,
        title: String,
        views: Vec<View>,
        view: View,
        inspector: bool,
    ) -> Element<'_, Message> {
        let tabs = row(views.into_iter().map(|choice| {
            let active = choice == view;
            reshiki::accessibility::button(
                format!("xtb-results-{}", choice.name().to_lowercase()),
                format!("{} tab", choice.name()),
                text(choice.name()).size(12),
            )
            .padding([5, 10])
            .checked(active)
            .style(control(active))
            .on_press(send(Action::View(choice)))
            .into()
        }))
        .spacing(4);
        let collapsed = self.xtb.dock.collapsed && !inspector;
        let mut header = row![text(title).size(14), tabs]
            .spacing(14)
            .align_y(Alignment::Center);
        if !inspector {
            header = header.push(Space::new().width(Length::Fill));
        }
        header = header.push(
            accessible_command(
                "xtb-results-dock",
                if inspector {
                    "Dock the xTB results at the bottom"
                } else {
                    "Dock the xTB results in the inspector"
                },
                if inspector {
                    "Dock bottom"
                } else {
                    "Dock right"
                },
            )
            .on_press(send(Action::Dock)),
        );
        if !inspector {
            header = header.push(
                accessible_command(
                    "xtb-results-collapse",
                    "Show or hide the xTB results",
                    if collapsed { "Expand" } else { "Collapse" },
                )
                .expanded(!collapsed)
                .on_press(send(Action::Collapse)),
            );
        }
        header = header.push(
            accessible_command("xtb-results-close", "Close the xTB results", "Close")
                .on_press(send(Action::Close)),
        );
        // The inspector is narrow: its commands wrap below the tabs.
        if inspector {
            header.wrap().vertical_spacing(6).into()
        } else {
            header.into()
        }
    }

    /// The last run's failure and the actions available for it.
    fn xtb_failure_card(&self, failed: &Failed) -> Element<'_, Message> {
        let retry = self.xtb_can_retry(failed);
        let (title, detail) = failed.explain(retry);
        let button = |id: &'static str, name: &'static str, label: &'static str, action| {
            reshiki::accessibility::button(id, name, text(label).size(12))
                .padding([6, 12])
                .style(crate::appearance::secondary)
                .on_press(Message::Xtb(action))
                .into()
        };
        let actions = match failed.failure.reason() {
            state::Failure::Scf(_) if retry => {
                let mut actions = vec![];
                if failed.scf_iteration_limit().is_some() {
                    actions.push(button(
                        "xtb-reset-retry",
                        "Reset Max SCF iterations to 250 and retry",
                        "Reset to 250 and retry",
                        XtbAction::ResetRetry,
                    ));
                }
                if failed
                    .request
                    .as_ref()
                    .is_some_and(|r| r.ts.is_none() && r.config.hotter().is_some())
                {
                    actions.push(button(
                        "xtb-retry-hot",
                        "Retry at electronic temperature 1000 K",
                        "Retry at 1000 K",
                        XtbAction::Retry,
                    ));
                }
                actions
            }
            state::Failure::Crashed { .. } => {
                let mut actions = vec![];
                if retry {
                    actions.push(button(
                        "xtb-run-again",
                        "Run the calculation again",
                        "Run again",
                        XtbAction::RunAgain,
                    ));
                }
                actions.push(button(
                    "xtb-copy-details",
                    "Copy the calculation details",
                    "Copy details",
                    XtbAction::CopyDetails,
                ));
                actions
            }
            _ => vec![],
        };
        alert(false, title, detail, actions)
    }

    /// A failed run leaves the previous result available as a read-only summary.
    fn xtb_previous_result(
        &self,
        failed: &Failed,
        inspector: bool,
    ) -> Option<Element<'_, Message>> {
        let shown = self.xtb_last_successful(failed)?;
        let expanded = self.tab.xtb.palette.previous;
        let mut label = "Last successful run".to_owned();
        if let Some(finished) = results::finished_local(&shown.stored.result) {
            label.push_str(&format!(" · {}", finished.format("%-d %b %Y %H:%M")));
        }
        let arrow = if expanded { '▾' } else { '▸' };
        let mut previous = column![
            accessible_command(
                "xtb-results-previous",
                label.clone(),
                format!("{label} {arrow}")
            )
            .expanded(expanded)
            .on_press(send(Action::Previous))
        ]
        .spacing(6);
        if expanded {
            let body = column![
                text(shown.method()).size(12).style(greyed(true)),
                summary(&shown, inspector, true)
            ]
            .spacing(6);
            previous = if inspector {
                previous.push(body)
            } else {
                previous.push(container(scrollable(body)).max_height(BODY_HEIGHT))
            };
        }
        Some(previous.into())
    }

    /// The newest converged result for the molecule whose run failed.
    fn xtb_last_successful(&self, failed: &Failed) -> Option<Shown<'_>> {
        if failed.ts().is_some() {
            return None;
        }
        let target = failed.atoms.iter().copied().collect();
        let entry = self
            .tab
            .doc
            .science
            .results
            .iter()
            .enumerate()
            .rev()
            .find(|(_, stored)| {
                matches!(stored.result.status, reshiki::qcdata::Status::Converged)
                    && self.xtb_result_belongs(stored, &target)
            })?;
        self.xtb_result(entry)
    }

    fn xtb_stale_banner(&self, shown: &Shown<'_>) -> Element<'_, Message> {
        let (message, action): (String, Element<'_, Message>) = match &self.tab.xtb.run {
            _ if super::ts::not_verified(&shown.stored.result) => (
                "Reaction freshness not verified. Results are kept, greyed.".into(),
                Space::new().into(),
            ),
            _ if super::ts::is_ts(&shown.stored.result) => (
                "The reaction changed after this search. Results are kept, greyed.".into(),
                Space::new().into(),
            ),
            Some(run) => (
                format!(
                    "Recalculating with the same settings · {}",
                    run.launch.as_ref().map_or_else(
                        || "making the 3D model first (MMFF94s)".into(),
                        |_| run.progress.line(run.config.task, run.seconds())
                    )
                ),
                accessible_command("xtb-results-cancel", "Cancel the calculation", "Cancel")
                    .on_press(Message::Xtb(XtbAction::Cancel))
                    .into(),
            ),
            // Builds without the engine show stored results only, and the
            // card cannot repeat every stored calculation.
            None if !super::super::available() || !shown.repeatable() => (
                "The molecule changed after this calculation. Results are kept, greyed.".into(),
                Space::new().into(),
            ),
            None => (
                "The molecule changed after this calculation. Results are kept, greyed.".into(),
                reshiki::accessibility::button(
                    "xtb-results-recalculate",
                    "Recalculate xTB, rebuilding the 3D model first",
                    text("Recalculate (rebuilds the 3D model)").size(12),
                )
                .padding([6, 12])
                .style(crate::appearance::secondary)
                .on_press(send(Action::Recalculate))
                .into(),
            ),
        };
        container(
            row![text(message).size(12).width(Length::Fill), action,]
                .spacing(12)
                .align_y(Alignment::Center),
        )
        .padding([6, 10])
        .width(Length::Fill)
        .style(|theme: &Theme| {
            let warning = theme.palette().warning;
            container::Style {
                background: Some(Color { a: 0.12, ..warning }.into()),
                border: iced::Border {
                    color: warning,
                    width: 1.,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
    }

    fn xtb_steps<'a>(&'a self, shown: &Shown<'a>, inspector: bool) -> Element<'a, Message> {
        let energies = results::energies(&shown.stored.result).to_vec();
        let last = energies.len().saturating_sub(1);
        let selected = self.xtb_step();
        // Steps preview only between runs: a run owns the model preview.
        let live = !shown.stale && self.tab.xtb.run.is_none();
        // Without a preview, the list marks the step the model shows (the
        // last one unless another step was used).
        let shows = (live && selected.is_none())
            .then(|| self.xtb_model_step(shown))
            .flatten();
        let chart = canvas(StepChart {
            energies: energies.clone(),
            selected: selected.or(shows).unwrap_or(last),
            live,
        })
        .width(if inspector {
            Length::Fill
        } else {
            Length::Fixed(420.)
        })
        .height(140);
        let list = Column::with_children(energies.iter().enumerate().map(|(index, energy)| {
            let delta = index
                .checked_sub(1)
                .and_then(|before| energies.get(before))
                .map_or_else(
                    || "start".into(),
                    |before| state::scientific(energy - before),
                );
            let active = selected.or(shows).unwrap_or(last) == index;
            reshiki::accessibility::button(
                format!("xtb-step-{index}"),
                format!("Step {index}"),
                row![
                    text(format!("Step {index}")).size(12).width(70),
                    text(format!("{} Eh", state::signed(*energy, 6)))
                        .size(12)
                        .width(Length::Fill),
                    text(delta).size(11).style(muted_text),
                ]
                .spacing(8),
            )
            .padding([3, 8])
            .width(Length::Fill)
            .checked(active)
            .style(control(active))
            .on_press_maybe(live.then(|| send(Action::Step(index))))
            .into()
        }))
        .spacing(1);
        let palette = &self.tab.xtb.palette;
        let index = selected.or(shows).unwrap_or(last);
        let mut side = column![text(format!("Step {index} of {last}")).size(12)].spacing(6);
        // The stereo question belongs to the current preview.
        if palette.confirm.is_empty() || selected.is_none() {
            // Any step the model does not show already, the last one too
            // after another step was used.
            let usable = live
                && self.xtb_step_preview().is_some()
                && shown.step_positions(index).is_some_and(|positions| {
                    !super::super::model_shows(&self.tab.doc, shown.atoms, &positions)
                });
            side = side.push(
                reshiki::accessibility::button(
                    "xtb-use-step",
                    "Use this geometry",
                    text("Use this geometry").size(12),
                )
                .padding([6, 12])
                .style(crate::appearance::primary)
                .on_press_maybe(usable.then(|| send(Action::UseStep))),
            );
            if selected.is_some() {
                side = side.push(
                    accessible_command(
                        "xtb-step-back",
                        "Show the result's geometry again",
                        "Back to result",
                    )
                    .on_press(send(Action::StepBack)),
                );
            }
            side = side.push(line(
                if live {
                    "Click a step to preview it on the 3D model. Esc returns to the result."
                } else {
                    "Recalculate to preview steps."
                },
                Tone::Muted,
            ));
        } else {
            for reason in &palette.confirm {
                side = side.push(text(reason.clone()).size(12));
            }
            side = side.push(
                row![
                    reshiki::accessibility::button(
                        "xtb-use-step-anyway",
                        "Use this geometry anyway",
                        text("Use anyway").size(12),
                    )
                    .padding([6, 12])
                    .style(crate::appearance::primary)
                    .on_press(send(Action::UseStepAnyway)),
                    accessible_command("xtb-keep-model", "Keep the 3D model", "Keep model")
                        .on_press(send(Action::KeepModel)),
                ]
                .spacing(8),
            );
        }
        if inspector {
            return column![chart, list, side].spacing(10).into();
        }
        row![
            chart,
            container(list).width(Length::FillPortion(3)),
            container(side).width(Length::FillPortion(2)),
        ]
        .spacing(16)
        .into()
    }

    fn xtb_footer<'a>(
        &'a self,
        shown: &Shown<'a>,
        view: View,
        inspector: bool,
    ) -> Element<'a, Message> {
        let mut footer = row![].spacing(10).align_y(Alignment::Center);
        match view {
            View::Summary => {
                footer = footer
                    .push(
                        accessible_command(
                            "xtb-copy-table",
                            "Copy the results table",
                            "Copy table",
                        )
                        .on_press(send(Action::CopyTable)),
                    )
                    .push(
                        accessible_command(
                            "xtb-export-table",
                            "Export the results table as TSV",
                            "Export…",
                        )
                        .on_press(send(Action::Export)),
                    )
                    .push(toggle(
                        "xtb-labels",
                        "Show as labels".into(),
                        self.tab.xtb.palette.labels,
                        Action::Labels(!self.tab.xtb.palette.labels),
                    ));
            }
            View::Vibrations => {
                footer = footer.push(line(
                    "Normal modes are recomputed when shown; the file keeps frequencies, intensities and thermochemistry.",
                    Tone::Muted,
                ));
            }
            View::Steps => {
                footer = footer.push(line(
                    "The file keeps the first and last steps; keep full results to save every step.",
                    Tone::Muted,
                ));
            }
            View::Orbitals => footer = footer.push(self.xtb_orbitals_note(shown)),
            View::Conformers => {
                footer = footer.push(super::super::conformers::footer());
            }
            View::Path => {}
            View::Log => {
                footer = footer.push(
                    accessible_command("xtb-copy-log", "Copy the log", "Copy")
                        .on_press(send(Action::CopyLog)),
                );
            }
        }
        let (saved, full) = self.xtb_sizes(shown);
        let keep = shown.stored.keep_full;
        let sizes = row![
            toggle(
                "xtb-keep-full",
                format!("Keep full results in this file (+{})", results::size(full)),
                keep,
                Action::KeepFull(!keep),
            ),
            text(format!("Saved in file: {}", results::size(saved))).size(12),
        ]
        .spacing(14)
        .align_y(Alignment::Center);
        let note = shown.stored.result.omitted_bytes.map(|bytes| {
            // Imported results whose heavy data did not fit; the import lane
            // adds the source file and the reopen action here.
            line(
                format!("Not saved in the file ({}).", results::size(bytes)),
                Tone::Muted,
            )
        });
        let mut footer: Column<'_, Message> = if inspector {
            column![footer.wrap(), sizes.wrap()].spacing(6)
        } else {
            column![footer.push(Space::new().width(Length::Fill)).push(sizes)]
        };
        if let Some(note) = note {
            footer = footer.push(note);
        }
        footer.into()
    }
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod failure_tests;

/// An inline card with a round "!" icon: red for an error, amber for a
/// warning. The status bar repeats an error's sentence.
pub(in crate::app::xtb) fn alert<'a>(
    warning: bool,
    title: String,
    detail: String,
    actions: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let icon = container(text("!").size(12).center())
        .width(20)
        .height(20)
        .center(20)
        .style(move |theme: &Theme| {
            let tone = if warning {
                theme.palette().warning
            } else {
                theme.palette().danger
            };
            container::Style {
                background: Some(tone.into()),
                text_color: Some(Color::WHITE),
                border: iced::Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        });
    let mut words = column![text(title).size(12)].spacing(2);
    if !detail.is_empty() {
        words = words.push(line(detail, Tone::Muted));
    }
    let heading = row![icon, words.width(Length::Fill)]
        .spacing(10)
        .align_y(Alignment::Center);
    let mut content = column![heading].spacing(6);
    if !actions.is_empty() {
        content = content.push(row(actions).spacing(8).wrap().vertical_spacing(4));
    }
    let card = container(content)
        .padding([8, 10])
        .width(Length::Fill)
        .style(move |theme: &Theme| {
            let tone = if warning {
                theme.palette().warning
            } else {
                theme.palette().danger
            };
            container::Style {
                background: Some(Color { a: 0.08, ..tone }.into()),
                border: iced::Border {
                    color: Color { a: 0.6, ..tone },
                    width: 1.,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        });
    card.into()
}

/// Label and value rows: two columns at the bottom, one in the inspector.
fn summary<'a>(shown: &Shown<'a>, inspector: bool, previous: bool) -> Element<'a, Message> {
    let rows = shown.rows();
    let label_width = if inspector { 104. } else { LABEL_WIDTH };
    let column_of = |rows: &[(String, String)]| {
        Column::with_children(rows.iter().map(|(label, content)| {
            row![
                text(label.clone())
                    .size(12)
                    .style(move |theme| if previous {
                        greyed(true)(theme)
                    } else {
                        muted_text(theme)
                    })
                    .width(label_width),
                text(content.clone())
                    .size(12)
                    .width(Length::Fill)
                    .style(greyed(shown.stale || previous)),
            ]
            .spacing(8)
            .into()
        }))
        .spacing(6)
        .width(Length::Fill)
    };
    let tables = (!previous).then(|| tables(shown)).flatten();
    if inspector {
        let mut body = column![column_of(&rows)].spacing(12);
        if let Some(tables) = tables {
            body = body.push(tables);
        }
        return body.into();
    }
    let (left, right) = rows.split_at(rows.len().div_ceil(2));
    let mut body = row![column_of(left), column_of(right)].spacing(24);
    if let Some(tables) = tables {
        body = body.push(tables);
    }
    body.into()
}

/// Bond orders and orbital levels as small two-column tables.
fn tables<'a>(shown: &Shown<'a>) -> Option<Element<'a, Message>> {
    let result = &shown.stored.result;
    let table = |title: &'static str, rows: Vec<(String, String)>| -> Element<'a, Message> {
        let mut table = column![text(title).size(12).style(muted_text)].spacing(4);
        for (name, value) in rows {
            table = table.push(
                row![
                    text(name).size(12).width(72).style(greyed(shown.stale)),
                    text(value).size(12).style(greyed(shown.stale)),
                ]
                .spacing(8),
            );
        }
        table.width(Length::Shrink).into()
    };
    let bonds = results::bond_orders(result, shown.frame);
    let levels: Vec<_> = results::levels(result)
        .into_iter()
        .map(|(name, ev)| (name, format!("{} eV", state::signed(ev, 2))))
        .collect();
    if bonds.is_empty() && levels.is_empty() {
        return None;
    }
    let mut both = row![].spacing(24);
    if !bonds.is_empty() {
        both = both.push(table("Bond orders (≥ 0.1)", bonds));
    }
    if !levels.is_empty() {
        both = both.push(table("Orbital levels", levels));
    }
    Some(both.into())
}

fn log<'a>(shown: &Shown<'a>, method: &str) -> Element<'a, Message> {
    let log = results::log_with_ts(
        &shown.stored.result,
        shown.frame,
        shown.info,
        method,
        shown.ts,
    );
    text(log)
        .size(11)
        .font(iced::Font::MONOSPACE)
        .width(Length::Fill)
        .style(greyed(shown.stale))
        .into()
}

/// Energy against step; a click previews the nearest step.
struct StepChart {
    energies: Vec<f64>,
    selected: usize,
    live: bool,
}

impl StepChart {
    const LEFT: f32 = 70.;
    const TOP: f32 = 8.;
    const BOTTOM: f32 = 22.;

    fn x(&self, index: usize, bounds: Rectangle) -> f32 {
        let span = (bounds.width - Self::LEFT - 10.).max(1.);
        Self::LEFT + span * index as f32 / self.energies.len().saturating_sub(1).max(1) as f32
    }
}

impl canvas::Program<Message> for StepChart {
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
        let nearest = (0..self.energies.len()).min_by(|&a, &b| {
            (self.x(a, bounds) - point.x)
                .abs()
                .total_cmp(&(self.x(b, bounds) - point.x).abs())
        })?;
        Some(canvas::Action::publish(send(Action::Step(nearest))).and_capture())
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.live && cursor.is_over(bounds) {
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
        let height = bounds.height - Self::TOP - Self::BOTTOM;
        let width = bounds.width - Self::LEFT - 10.;
        frame.stroke(
            &canvas::Path::rectangle(
                Point::new(Self::LEFT, Self::TOP),
                iced::Size::new(width, height),
            ),
            canvas::Stroke::default()
                .with_color(Color { a: 0.4, ..muted })
                .with_width(1.),
        );
        let (lo, hi) = self
            .energies
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), e| {
                (lo.min(*e), hi.max(*e))
            });
        let range = (hi - lo).max(1e-9);
        let y = |e: f64| Self::TOP + 4. + ((hi - e) / range) as f32 * (height - 8.);
        let points: Vec<Point> = self
            .energies
            .iter()
            .enumerate()
            .map(|(index, e)| Point::new(self.x(index, bounds), y(*e)))
            .collect();
        frame.stroke(
            &canvas::Path::new(|path| {
                for (index, point) in points.iter().enumerate() {
                    if index == 0 {
                        path.move_to(*point);
                    } else {
                        path.line_to(*point);
                    }
                }
            }),
            canvas::Stroke::default().with_color(ink).with_width(1.4),
        );
        for (index, point) in points.iter().enumerate() {
            let chosen = index == self.selected;
            let dot = canvas::Path::circle(*point, if chosen { 4.5 } else { 2.5 });
            frame.fill(&dot, if chosen && self.live { accent } else { ink });
        }
        for (e, label) in [(hi, hi), (lo, lo)] {
            frame.fill_text(canvas::Text {
                content: state::signed(label, 5),
                position: Point::new(Self::LEFT - 4., y(e) - 6.),
                color: muted,
                size: 10.into(),
                align_x: iced::alignment::Horizontal::Right.into(),
                ..Default::default()
            });
        }
        frame.fill_text(canvas::Text {
            content: "Step (energy in Eh)".into(),
            position: Point::new(Self::LEFT + width / 2., bounds.height - 16.),
            color: muted,
            size: 10.into(),
            align_x: iced::alignment::Horizontal::Center.into(),
            ..Default::default()
        });
        vec![frame.into_geometry()]
    }
}
