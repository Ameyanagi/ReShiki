//! Atom-linked offline NMR results; detached work and chemical identity gates.
use super::{App, InspectorTab, Message, files, workspace::muted_text};
use crate::canvas::nmr::LabelMode;
use iced::widget::{Space, checkbox, column, container, row, scrollable, sensor, stack, text};
use iced::{Alignment, Element, Length, Point, Size, Task};
use reshiki::document::Document;
use reshiki::nmr::{self as engine, Nucleus, Report};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobKey {
    serial: u64,
    epoch: u64,
    ids: Vec<u64>,
    fingerprint: String,
}
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Close,
    Dock,
    Undock,
    Predict(Nucleus),
    ToggleCollapsed,
    ToggleDetails,
    TogglePlot,
    ToggleLabels,
    LabelMode(LabelMode),
    LabelMenu(bool),
    Move(Point),
    Resize(Size),
    ResetPosition,
    RenderedSize(Size),
    SelectRow(usize),
    SelectMarker(Vec<usize>),
    Copy,
    Export,
    Finished(JobKey, Result<Arc<Report>, String>),
    Exported(Result<Option<std::path::PathBuf>, String>),
}

pub(super) use palette::OVERLAY_INSETS;
pub(super) use state::State;

const PLOT_DESCRIPTION: &str = concat!(
    "The plot marks chemical shifts only. ",
    "Stick heights do not represent intensity or integration. ",
    "No J couplings, multiplets or second-order simulation are predicted. ",
    "Nearby labels cycle their distinct atom-linked sites.",
);

const STALE_NOTICE: &str = "Molecular chemistry changed; run prediction again";

fn identity_current(
    document: &Document,
    epoch_matches: bool,
    ids: &[u64],
    fingerprint: &str,
) -> bool {
    epoch_matches && engine::fingerprint(document, ids).ok().as_deref() == Some(fingerprint)
}

mod palette;
mod panel;
mod plot;
mod state;
mod style;
mod worker;

use palette::{MAX_SIZE, MIN_SIZE, PANEL_PADDING, RESIZE_STEP};
use style::{outlined, quiet, result_row, rule, segment, surface, table_surface};

impl App {
    pub(super) fn nmr_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Open => return self.nmr_open(),
            Action::Close => return self.nmr_close(),
            Action::Dock => return self.nmr_dock(),
            Action::Undock => return self.nmr_undock(),
            Action::Predict(nucleus) => return self.nmr_predict(nucleus),
            Action::Finished(key, result) => return self.nmr_finished(key, result),
            Action::SelectRow(index) => return self.nmr_select_row(index),
            Action::SelectMarker(indices) => return self.nmr_select_marker(indices),
            Action::Copy => return self.nmr_copy(),
            Action::Export => return self.nmr_export(),
            Action::Exported(result) => return self.nmr_exported(result),
            Action::ToggleCollapsed => self.tab.nmr.toggle_collapsed(),
            Action::ToggleDetails => self.tab.nmr.toggle_details(),
            Action::TogglePlot => self.tab.nmr.toggle_plot(),
            Action::ToggleLabels => self.tab.nmr.toggle_labels(),
            Action::LabelMode(mode) => self.tab.nmr.set_label_mode(mode),
            Action::LabelMenu(open) => self.tab.nmr.set_label_menu(open),
            Action::Move(position) => self.tab.nmr.move_to(position),
            Action::Resize(size) => self.tab.nmr.resize(size),
            Action::ResetPosition => self.tab.nmr.reset_position(),
            Action::RenderedSize(size) => self.tab.nmr.set_rendered_size(size),
        }
        Task::none()
    }

    fn nmr_open(&mut self) -> Task<Message> {
        self.tab.nmr.open = true;
        self.tab.nmr.collapsed = false;
        self.tab.nmr.details = false;
        if self.tab.nmr.dock.docked {
            self.place_nmr_inspector(true);
        }
        // The Properties entry predicts the currently selected
        // molecule, including when another valid report was retained.
        self.nmr_predict(self.tab.nmr.job.nucleus)
    }

    fn nmr_close(&mut self) -> Task<Message> {
        self.tab.nmr.open = false;
        self.tab.nmr.dock.restore_dock = false;
        self.tab.nmr.labels.visible = false;
        self.tab.nmr.labels.menu = false;
        if self.tab.nmr.dock.docked {
            self.place_nmr_inspector(false);
        }
        Task::none()
    }

    fn nmr_dock(&mut self) -> Task<Message> {
        self.tab.nmr.open = true;
        self.tab.nmr.dock.docked = true;
        self.place_nmr_inspector(true);
        if self.tab.nmr.job.result.is_none() && self.tab.nmr.job.pending.is_none() {
            return self.nmr_predict(self.tab.nmr.job.nucleus);
        }
        Task::none()
    }

    fn nmr_undock(&mut self) -> Task<Message> {
        self.tab.nmr.dock.docked = false;
        self.tab.nmr.dock.restore_dock = false;
        self.place_nmr_inspector(false);
        Task::none()
    }

    fn nmr_select_marker(&mut self, indices: Vec<usize>) -> Task<Message> {
        let Some(report) = &self.tab.nmr.job.result else {
            return Task::none();
        };
        let indices: Vec<_> = indices
            .into_iter()
            .filter(|&index| {
                report
                    .rows
                    .get(index)
                    .is_some_and(engine::Row::has_statistics)
            })
            .collect();
        // Nearby sticks share a label, not a prediction or intensity.
        // Repeated activation cycles their distinct original atom IDs.
        let current = indices.iter().position(|&index| {
            report
                .rows
                .get(index)
                .is_some_and(|row| self.tab.selected.contains(&row.atom_id))
        });
        if let Some(&index) = indices.get(current.map_or(0, |i| (i + 1) % indices.len())) {
            return self.nmr_select_row(index);
        }
        Task::none()
    }

    fn nmr_predict(&mut self, nucleus: Nucleus) -> Task<Message> {
        if self.tab.nmr.job.pending.is_some() {
            return Task::none();
        }
        self.tab.nmr.open = true;
        self.tab.nmr.collapsed = false;
        self.tab.nmr.job.nucleus = nucleus;
        self.tab.nmr.job.result = None;
        self.tab.nmr.labels.visible = false;
        self.tab.nmr.labels.menu = false;
        let request = match engine::prepare(&self.tab.doc, &self.tab.selected) {
            Ok(r) => r,
            Err(error) => {
                self.tab.nmr.job.notice = Some(error.clone());
                self.status = error;
                self.error = true;
                return Task::none();
            }
        };
        self.tab.nmr.job.serial = self.tab.nmr.job.serial.wrapping_add(1);
        let key = JobKey {
            serial: self.tab.nmr.job.serial,
            epoch: self.tab.file_epoch,
            ids: request.atom_ids.clone(),
            fingerprint: request.fingerprint.clone(),
        };
        self.tab.nmr.job.pending = Some(key.clone());
        self.tab.nmr.job.notice = None;
        self.status = format!(
            "Predicting {} chemical shifts from local observed data…",
            nucleus.label()
        );
        self.error = false;
        Task::perform(worker::calculate(request, nucleus), move |result| {
            Message::Nmr(Action::Finished(key.clone(), result))
        })
    }

    fn nmr_finished(&mut self, key: JobKey, result: Result<Arc<Report>, String>) -> Task<Message> {
        if self.tab.nmr.job.pending.as_ref() != Some(&key) {
            return Task::none();
        }
        self.tab.nmr.job.pending = None;
        if !identity_current(
            &self.tab.doc,
            key.epoch == self.tab.file_epoch,
            &key.ids,
            &key.fingerprint,
        ) {
            self.tab.nmr.job.notice = Some(STALE_NOTICE.into());
            return Task::none();
        }
        match result {
            Ok(report) => {
                let supported = report.supported_count();
                self.status = format!(
                    "{} NMR prediction · {supported}/{} supported atom-linked sites",
                    report.nucleus.label(),
                    report.rows.len()
                );
                self.error = false;
                self.tab.nmr.job.result_epoch = Some(key.epoch);
                self.tab.nmr.job.result = Some(report);
            }
            Err(error) => {
                self.tab.nmr.job.notice = Some(error.clone());
                self.status = error;
                self.error = true;
            }
        }
        Task::none()
    }

    fn nmr_select_row(&mut self, index: usize) -> Task<Message> {
        if let Some(row) = self
            .tab
            .nmr
            .job
            .result
            .as_ref()
            .and_then(|r| r.rows.get(index))
        {
            let mut selected: Vec<_> = std::iter::once(row.atom_id)
                .chain(row.hydrogen_ids.iter().copied())
                .collect();
            let collapsed = self.tab.doc.abbreviation_containing(row.atom_id);
            if let Some(group) = collapsed
                && !selected.contains(&group.anchor)
            {
                selected.push(group.anchor);
            }
            let atom_id = row.atom_id;
            let hydrogen_count = row.hydrogen_count;
            self.set_selection(selected);
            self.status = format!(
                "NMR prediction linked to atom #{}{}",
                atom_id,
                if hydrogen_count > 1 && self.tab.nmr.job.nucleus == Nucleus::H1 {
                    " · unresolved attached-H group"
                } else {
                    ""
                }
            );
        }
        Task::none()
    }

    fn nmr_copy(&mut self) -> Task<Message> {
        if let Some(report) = &self.tab.nmr.job.result {
            self.status =
                "Copied predicted NMR shifts with method, conditions and data attribution".into();
            return iced::clipboard::write(engine::to_tsv(report));
        }
        Task::none()
    }

    fn nmr_export(&mut self) -> Task<Message> {
        if let Some(report) = &self.tab.nmr.job.result {
            let contents = engine::to_tsv(report);
            return Task::perform(
                async move {
                    files::save_bytes(
                        "Export predicted NMR shifts",
                        "Predicted NMR shifts.tsv",
                        "tsv",
                        contents.as_bytes(),
                    )
                    .await
                },
                |result| Message::Nmr(Action::Exported(result)),
            );
        }
        Task::none()
    }

    fn nmr_exported(
        &mut self,
        result: Result<Option<std::path::PathBuf>, String>,
    ) -> Task<Message> {
        match result {
            Ok(Some(path)) => {
                self.status = format!("Exported predicted NMR shifts to {}", path.display());
                self.error = false;
            }
            Ok(None) => {}
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
        Task::none()
    }

    pub(super) fn nmr_canvas(&self) -> Option<crate::canvas::nmr::Context<'_>> {
        let state = &self.tab.nmr;
        (state.open && state.labels.visible).then_some(())?;
        Some(crate::canvas::nmr::Context {
            report: state.job.result.as_deref()?,
            mode: state.labels.mode,
        })
    }

    pub(super) fn nmr_leave_tab(&mut self) {
        self.tab.nmr.dock.restore_dock = self.tab.nmr.open
            && self.tab.nmr.dock.docked
            && self.inspector_open
            && self.inspector_tab == InspectorTab::Nmr;
        if self.tab.nmr.dock.restore_dock {
            self.place_nmr_inspector(false);
        }
    }

    pub(super) fn nmr_enter_tab(&mut self) -> bool {
        if !self.tab.nmr.dock.restore_dock {
            return false;
        }
        self.place_nmr_inspector(true);
        // Restoring a report must preserve its saved camera even when the
        // other document already had an equally wide inspector open.
        true
    }

    pub(super) fn nmr_inspector(&self) -> Element<'_, Message> {
        if self.tab.nmr.open && self.tab.nmr.dock.docked {
            return self.nmr_panel(false);
        }
        // Inspector selection is global; prediction mode/results belong to a
        // document. Never instantiate a second panel for a floating document.
        container(
            column![
                text("NMR prediction").size(14),
                text(if self.tab.nmr.open {
                    "Results are in the floating palette."
                } else {
                    "Select a molecule to predict its chemical shifts."
                })
                .size(12),
                named_command(
                    "nmr.inspector.dock",
                    "Dock NMR prediction in this inspector",
                    "Dock here",
                    Action::Dock,
                    true,
                ),
            ]
            .spacing(10),
        )
        .padding(8)
        .height(Length::Fill)
        .into()
    }

    fn place_nmr_inspector(&mut self, dock: bool) {
        let old_width = self.inspector_width();
        if dock {
            if self.inspector_tab != InspectorTab::Nmr {
                self.tab.nmr.dock.dock_return = Some((self.inspector_open, self.inspector_tab));
            }
            self.inspector_open = true;
            self.inspector_tab = InspectorTab::Nmr;
        } else if self.inspector_tab == InspectorTab::Nmr {
            let (open, tab) = self
                .tab
                .nmr
                .dock
                .dock_return
                .take()
                .unwrap_or((true, InspectorTab::Properties));
            self.inspector_open = open;
            self.inspector_tab = tab;
        }
        self.expect_inspector_width_change(old_width);
    }

    pub(super) fn with_nmr<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let state = &self.tab.nmr;
        // Floating content stays above the unchanged workspace. Docking is an
        // explicit choice to use the existing inspector, never a camera fit.
        let mut layers = stack![base];
        if state.open && !state.dock.docked {
            layers = layers.push(palette::floating(
                self.nmr_panel(true),
                state.palette.position,
                state.palette.width,
                !state.collapsed,
            ));
        }
        layers.into()
    }

    pub(super) fn nmr_panel(&self, floating: bool) -> Element<'_, Message> {
        let state = &self.tab.nmr;
        let title = row![
            text(if floating { "⠿" } else { "" })
                .size(15)
                .style(muted_text),
            text("NMR prediction").size(14),
            Space::new().width(Length::Fill),
            named_command(
                "nmr.mode",
                if floating {
                    "Dock NMR in the inspector"
                } else {
                    "Undock NMR into a movable palette"
                },
                if floating { "Dock" } else { "Undock" },
                if floating {
                    Action::Dock
                } else {
                    Action::Undock
                },
                true,
            )
            .style(segment(true)),
            named_command(
                "nmr.collapse",
                "Show or hide the NMR prediction results",
                if state.collapsed { "+" } else { "−" },
                Action::ToggleCollapsed,
                true,
            )
            .expanded(!state.collapsed)
            .padding([4, 5]),
            named_command(
                "nmr.close",
                "Close NMR prediction",
                "×",
                Action::Close,
                true,
            )
            .padding([4, 5]),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if state.collapsed {
            let title = container(title).padding(PANEL_PADDING).style(surface);
            return if floating {
                title.width(state.palette.width).into()
            } else {
                palette::guard(title.width(Length::Fill))
            };
        }
        let mut nuclei = row![].spacing(0);
        for (id, name, label, nucleus) in [
            (
                "nmr.predict.1H",
                "Predict proton (¹H) NMR shifts",
                "¹H",
                Nucleus::H1,
            ),
            (
                "nmr.predict.13C",
                "Predict carbon-13 (¹³C) NMR shifts",
                "¹³C",
                Nucleus::C13,
            ),
        ] {
            nuclei = nuclei.push(
                named_command(
                    id,
                    name,
                    label,
                    Action::Predict(nucleus),
                    state.job.pending.is_none(),
                )
                .checked(state.job.nucleus == nucleus)
                .width(Length::Fill)
                .height(30)
                .style(segment(state.job.nucleus == nucleus)),
            );
        }
        // Iced draws the real checkbox; the enclosing named control supplies
        // the application's keyboard/native activation and checked semantics.
        let labels = reshiki::accessibility::button(
            "nmr.labels",
            "Show atom-linked NMR assignment labels on the drawing",
            checkbox(state.labels.visible)
                .label("Show labels")
                .size(15)
                .text_size(12)
                .spacing(7)
                .style(move |theme, _| {
                    checkbox::primary(
                        theme,
                        if state.job.result.is_some() {
                            checkbox::Status::Active {
                                is_checked: state.labels.visible,
                            }
                        } else {
                            checkbox::Status::Disabled {
                                is_checked: state.labels.visible,
                            }
                        },
                    )
                }),
        )
        .checked(state.labels.visible)
        .padding([3, 0])
        .style(quiet)
        .on_press_maybe(
            state
                .job
                .result
                .is_some()
                .then_some(Message::Nmr(Action::ToggleLabels)),
        );
        let mode = row![
            text(label_mode_name(state.labels.mode)).size(12),
            Space::new().width(Length::Fill),
            text("▾").size(10)
        ];
        let anchor =
            reshiki::accessibility::button("nmr.labels.mode", "NMR assignment label format", mode)
                .expanded(state.labels.menu)
                .value(label_mode_name(state.labels.mode))
                .padding([5, 7])
                .width(132)
                .style(outlined)
                .on_press_maybe(
                    state
                        .labels
                        .visible
                        .then_some(Message::Nmr(Action::LabelMenu(!state.labels.menu))),
                );
        let popup = state.labels.menu.then(|| {
            let mut choices = column![].spacing(2);
            for (id, label, mode) in [
                ("nmr.labels.atom", "Atom numbers", LabelMode::Atom),
                ("nmr.labels.ppm", "ppm", LabelMode::Shift),
                ("nmr.labels.both", "Both", LabelMode::Both),
            ] {
                choices = choices.push(
                    named_command(
                        id,
                        "Choose NMR assignment label format",
                        label,
                        Action::LabelMode(mode),
                        true,
                    )
                    .checked(state.labels.mode == mode)
                    .width(Length::Fill)
                    .style(segment(state.labels.mode == mode)),
                );
            }
            palette::menu(container(choices).padding(4).style(surface))
        });
        let dropdown = Element::new(
            super::popover::popover(anchor, popup, Message::Nmr(Action::LabelMenu(false)))
                .fit_anchor(),
        );
        let header = column![
            title,
            nuclei,
            row![labels, dropdown]
                .spacing(12)
                .align_y(Alignment::Center)
        ]
        .spacing(8);

        let mut body = column![].spacing(8);
        if state.job.pending.is_some() {
            body = body.push(text("Calculating from the molecular graph…").size(12));
        }
        if let Some(notice) = &state.job.notice {
            body = body.push(text(notice).size(12));
        }
        if let Some(report) = &state.job.result {
            if state.plot {
                body = body.push(text("Predicted shifts").size(12).style(muted_text));
                body = if report.rows.iter().any(engine::Row::has_statistics) {
                    body.push(plot::view(report, &self.tab.selected))
                } else {
                    body.push(
                        text("No supported shifts to plot")
                            .size(11)
                            .style(muted_text),
                    )
                };
            }
            let mut table = column![
                container(
                    row![
                        text(if report.nucleus == Nucleus::H1 {
                            "H site"
                        } else {
                            "Atom"
                        })
                        .size(12)
                        .width(Length::Fill),
                        text("δ / ppm").size(12).width(72).align_x(Alignment::End),
                    ]
                    .spacing(8)
                )
                .padding([7, 9])
            ];
            for (index, item) in report.rows.iter().enumerate() {
                let label = if report.nucleus == Nucleus::H1 {
                    format!(
                        "#{} · {} H{}",
                        item.atom_id,
                        item.hydrogen_count,
                        if item.hydrogen_count > 1 {
                            " group"
                        } else {
                            ""
                        }
                    )
                } else {
                    format!("C #{}", item.atom_id)
                };
                let mut values = column![
                    row![
                        text(label).size(12).width(Length::Fill),
                        text(
                            item.statistics
                                .as_ref()
                                .map(|s| format!("{:.3}", s.median))
                                .unwrap_or_else(|| "—".into())
                        )
                        .size(12)
                        .width(72)
                        .align_x(Alignment::End),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                ]
                .spacing(3);
                if let Some(limitation) = &item.limitation {
                    values = values.push(text(limitation).size(10).style(muted_text));
                }
                if state.details {
                    values = values.push(
                        text(format!(
                            "Sphere {} · Refs {} · Obs. SD {} ppm",
                            item.radius
                                .map(|r| r.to_string())
                                .unwrap_or_else(|| "—".into()),
                            item.statistics
                                .as_ref()
                                .map(|s| s.support.to_string())
                                .unwrap_or_else(|| "—".into()),
                            item.statistics
                                .as_ref()
                                .map(|s| format!("{:.3}", s.standard_deviation))
                                .unwrap_or_else(|| "—".into()),
                        ))
                        .size(10)
                        .style(muted_text),
                    );
                }
                table =
                    table.push(container(Space::new().height(1).width(Length::Fill)).style(rule));
                table = table.push(
                    reshiki::accessibility::button(
                        format!("nmr.site.{}.{}", report.nucleus.code(), item.atom_id),
                        {
                            let mut description = site_description(report.nucleus, item);
                            if !self.tab.doc.atom_visible(item.atom_id)
                                && let Some(group) =
                                    self.tab.doc.abbreviation_containing(item.atom_id)
                            {
                                description.push_str(&format!(
                                    ". Shown at abbreviation {} anchored on atom #{}",
                                    group.label, group.anchor
                                ));
                            }
                            description
                        },
                        values,
                    )
                    .padding([7, 9])
                    .width(Length::Fill)
                    .on_press(Message::Nmr(Action::SelectRow(index)))
                    .checked(self.tab.selected.contains(&item.atom_id))
                    .value(format!("{}. {}", report.data_version, report.conditions))
                    .style(result_row(self.tab.selected.contains(&item.atom_id))),
                );
            }
            body = body.push(container(table).style(table_surface));
        }
        if state.details {
            let mut options = row![
                named_command(
                    "nmr.plot",
                    "Show or hide the predicted chemical-shift plot",
                    if state.plot { "Hide plot" } else { "Show plot" },
                    Action::TogglePlot,
                    true,
                )
                .expanded(state.plot)
            ]
            .spacing(4);
            if floating {
                options = options.push(named_command(
                    "nmr.position.reset",
                    "Restore NMR palette position",
                    "Reset position",
                    Action::ResetPosition,
                    true,
                ));
                // Deliberately enable from the requested size while resizing
                // from the rendered height; changing this would change behavior.
                let height = state.palette.rendered_size.height;
                options = options
                    .push(named_command(
                        "nmr.size.less",
                        "Reduce NMR palette size",
                        "−",
                        Action::Resize(Size::new(
                            state.palette.width - RESIZE_STEP,
                            height - RESIZE_STEP,
                        )),
                        state.palette.width > MIN_SIZE.width
                            || state.palette.height > MIN_SIZE.height,
                    ))
                    .push(named_command(
                        "nmr.size.more",
                        "Increase NMR palette size",
                        "+",
                        Action::Resize(Size::new(
                            state.palette.width + RESIZE_STEP,
                            height + RESIZE_STEP,
                        )),
                        state.palette.width < MAX_SIZE.width
                            || state.palette.height < MAX_SIZE.height,
                    ));
            }
            let source = provenance(state.job.result.as_deref());
            body = body
                .push(options)
                .push(text(PLOT_DESCRIPTION).size(11))
                .push(text(source.method).size(11))
                .push(text(source.data_version).size(11))
                .push(text(source.conditions).size(11))
                .push(text(source.limitations).size(11))
                .push(text(source.attribution).size(10));
        }
        let mut commands = row![
            named_command(
                "nmr.method",
                "NMR prediction method, source, conditions and limitations",
                if state.details {
                    "▾ Details…"
                } else {
                    "▸ Details…"
                },
                Action::ToggleDetails,
                true,
            )
            .expanded(state.details)
            .value(panel_description(state))
            .padding([5, 0]),
            Space::new().width(Length::Fill),
            named_command(
                "nmr.copy",
                "Copy predicted NMR shifts with attribution",
                "Copy",
                Action::Copy,
                state.job.result.is_some(),
            ),
            named_command(
                "nmr.export",
                "Export predicted NMR shifts with attribution as TSV",
                "Export…",
                Action::Export,
                state.job.result.is_some(),
            ),
        ]
        .spacing(3)
        .align_y(Alignment::Center);
        if floating {
            commands = commands.push(text("◢").size(12).style(muted_text));
        }
        let footer = column![
            text(if state.job.nucleus == Nucleus::H1 {
                "Predicted H groups · unresolved · CDCl₃"
            } else {
                "Predicted shifts · CDCl₃"
            })
            .size(10)
            .style(muted_text),
            commands,
        ]
        .spacing(5);
        let content = panel::view(
            header,
            scrollable(
                container(body).padding(iced::Padding::ZERO.bottom(panel::CONTENT_END_INSET)),
            )
            .id("nmr-results")
            .height(Length::Shrink),
            footer,
            (floating && state.palette.resized)
                .then_some((state.palette.height - 2. * PANEL_PADDING).max(0.)),
            floating,
        );
        let panel = container(content).padding(PANEL_PADDING).style(surface);
        if floating {
            sensor(panel.width(state.palette.width))
                .on_show(|size| Message::Nmr(Action::RenderedSize(size)))
                .on_resize(|size| Message::Nmr(Action::RenderedSize(size)))
                .into()
        } else {
            palette::guard(panel.width(Length::Fill).height(Length::Fill))
        }
    }
}

fn named_command<'a>(
    id: &'static str,
    name: &'static str,
    label: &'a str,
    action: Action,
    enabled: bool,
) -> reshiki::accessibility::Button<'a, Message> {
    let message = Message::Nmr(action);
    reshiki::accessibility::button(id, name, text(label).size(12))
        .padding([5, 7])
        .on_press_maybe(enabled.then_some(message))
        .style(quiet)
}

fn site_description(nucleus: Nucleus, row: &engine::Row) -> String {
    let site = if nucleus == Nucleus::H1 {
        format!(
            "{} attached H{}",
            row.hydrogen_count,
            if row.hydrogen_count > 1 {
                " in an unresolved group"
            } else {
                ""
            }
        )
    } else {
        "carbon atom".into()
    };
    let prediction = row
        .statistics
        .as_ref()
        .map(|s| {
            format!(
                "Predicted {:.3} ppm; sphere {}; {} independent references; observed SD {:.3} ppm",
                s.median,
                row.radius.map(|r| r.to_string()).unwrap_or_default(),
                s.support,
                s.standard_deviation
            )
        })
        .unwrap_or_else(|| {
            format!(
                "Unsupported: {}",
                row.limitation
                    .as_deref()
                    .unwrap_or("No qualified reference data")
            )
        });
    format!(
        "Select {} NMR site atom #{}: {site}. {prediction}",
        nucleus.label(),
        row.atom_id
    )
}

fn label_mode_name(mode: LabelMode) -> &'static str {
    match mode {
        LabelMode::Atom => "Atom numbers",
        LabelMode::Shift => "ppm",
        LabelMode::Both => "Both",
    }
}
struct Provenance<'a> {
    method: &'a str,
    data_version: &'a str,
    conditions: &'a str,
    limitations: &'a str,
    attribution: &'a str,
}

fn provenance(report: Option<&Report>) -> Provenance<'_> {
    match report {
        Some(report) => Provenance {
            method: &report.method,
            data_version: &report.data_version,
            conditions: &report.conditions,
            limitations: &report.limitations,
            attribution: &report.attribution,
        },
        None => Provenance {
            method: engine::METHOD,
            data_version: engine::DATA_VERSION,
            conditions: engine::CONDITIONS,
            limitations: engine::LIMITATIONS,
            attribution: engine::ATTRIBUTION,
        },
    }
}

fn panel_description(state: &State) -> String {
    let status = if state.job.pending.is_some() {
        format!("Calculating {} shifts", state.job.nucleus.label())
    } else if let Some(notice) = &state.job.notice {
        notice.clone()
    } else if let Some(report) = &state.job.result {
        format!(
            "{} prediction: {} of {} atom-linked sites supported",
            report.nucleus.label(),
            report.supported_count(),
            report.rows.len()
        )
    } else {
        "No prediction result".into()
    };
    let source = provenance(state.job.result.as_deref());
    format!(
        "Status: {status}. {}. {}. {}. {}. {}",
        source.method,
        source.data_version,
        source.conditions,
        source.limitations,
        source.attribution
    )
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
