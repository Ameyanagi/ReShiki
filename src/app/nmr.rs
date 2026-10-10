//! Atom-linked offline NMR results; detached work and chemical identity gates.
use super::{App, InspectorTab, Message, files, workspace::muted_text};
use crate::canvas::nmr::LabelMode;
use iced::widget::{
    Space, button, checkbox, column, container, row, scrollable, sensor, stack, text,
};
use iced::{Alignment, Border, Color, Element, Length, Point, Shadow, Size, Task, Theme, Vector};
use reshiki::chemistry::nmr::Nucleus;
use reshiki::document::Document;
use reshiki_io::nmr::{self as engine, Report};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
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
    Collapse,
    Details,
    Plot,
    Labels,
    LabelMode(LabelMode),
    LabelMenu(bool),
    Move(Point),
    Resize(Size),
    ResetPosition,
    Layout(iced::Size),
    PanelSize(Size),
    Select(usize),
    SelectMarker(Vec<usize>),
    Copy,
    Export,
    Finished(Key, Result<Arc<Report>, String>),
    Exported(Result<Option<std::path::PathBuf>, String>),
}

pub(super) struct State {
    pub(super) open: bool,
    collapsed: bool,
    details: bool,
    plot: bool,
    labels: bool,
    label_mode: LabelMode,
    label_menu: bool,
    docked: bool,
    restore_dock: bool,
    position: Option<Point>,
    width: f32,
    height: f32,
    resized: bool,
    available: Size,
    rendered_size: Size,
    dock_return: Option<(bool, InspectorTab)>,
    layout_transition: Option<Size>,
    pub(super) nucleus: Nucleus,
    serial: u64,
    pending: Option<Key>,
    result_epoch: Option<u64>,
    pub(super) result: Option<Arc<Report>>,
    pub(super) notice: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            collapsed: false,
            details: false,
            plot: true,
            labels: false,
            label_mode: LabelMode::default(),
            label_menu: false,
            docked: false,
            restore_dock: false,
            position: None,
            width: 360.,
            height: 440.,
            resized: false,
            available: Size::new(1280., 820.),
            rendered_size: Size::new(360., 440.),
            dock_return: None,
            layout_transition: None,
            nucleus: Nucleus::H1,
            serial: 0,
            pending: None,
            result_epoch: None,
            result: None,
            notice: None,
        }
    }
}
impl State {
    pub(super) fn take_layout_transition(&mut self, size: Size) -> bool {
        self.layout_transition.take().is_some_and(|expected| {
            (size.width - expected.width).abs() < 1. && (size.height - expected.height).abs() < 1.
        })
    }

    pub(super) fn invalidate_if_changed(&mut self, document: &Document, epoch: u64) {
        let stale_pending = self.pending.as_ref().is_some_and(|key| {
            key.epoch != epoch
                || engine::fingerprint(document, &key.ids).ok().as_deref()
                    != Some(key.fingerprint.as_str())
        });
        let stale_result = self.result.as_ref().is_some_and(|report| {
            self.result_epoch != Some(epoch)
                || engine::fingerprint(document, &report.atom_ids)
                    .ok()
                    .as_deref()
                    != Some(report.fingerprint.as_str())
        });
        if stale_pending || stale_result {
            self.pending = None;
            self.result = None;
            self.labels = false;
            self.label_menu = false;
            self.serial = self.serial.wrapping_add(1);
            self.notice = Some("Molecular chemistry changed; run prediction again".into());
        }
    }
}

mod palette;
mod panel;
mod plot;

static WORK: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
async fn calculate(request: engine::Request, nucleus: Nucleus) -> Result<Arc<Report>, String> {
    let permit = WORK
        .try_acquire()
        .map_err(|_| "An NMR calculation is already running; try again shortly")?;
    let job = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        engine::predict(&request, nucleus).map(Arc::new)
    });
    tokio::time::timeout(std::time::Duration::from_secs(6), job)
        .await
        .map_err(|_| "NMR prediction exceeded its time limit")?
        .map_err(|e| format!("NMR calculation failed: {e}"))?
}

impl App {
    pub(super) fn nmr_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Open => {
                self.tab.nmr.open = true;
                self.tab.nmr.collapsed = false;
                self.tab.nmr.details = false;
                if self.tab.nmr.docked {
                    self.place_nmr_inspector(true);
                }
                // The Properties entry predicts the currently selected
                // molecule, including when another valid report was retained.
                return self.nmr_action(Action::Predict(self.tab.nmr.nucleus));
            }
            Action::Close => {
                self.tab.nmr.open = false;
                self.tab.nmr.restore_dock = false;
                self.tab.nmr.labels = false;
                self.tab.nmr.label_menu = false;
                if self.tab.nmr.docked {
                    self.place_nmr_inspector(false);
                }
            }
            Action::Dock => {
                self.tab.nmr.open = true;
                self.tab.nmr.docked = true;
                self.place_nmr_inspector(true);
                if self.tab.nmr.result.is_none() && self.tab.nmr.pending.is_none() {
                    return self.nmr_action(Action::Predict(self.tab.nmr.nucleus));
                }
            }
            Action::Undock => {
                self.tab.nmr.docked = false;
                self.tab.nmr.restore_dock = false;
                self.place_nmr_inspector(false);
            }
            Action::Collapse => {
                self.tab.nmr.collapsed = !self.tab.nmr.collapsed;
                self.tab.nmr.label_menu = false;
            }
            Action::Details => self.tab.nmr.details = !self.tab.nmr.details,
            Action::Plot => self.tab.nmr.plot = !self.tab.nmr.plot,
            Action::Labels if self.tab.nmr.result.is_some() => {
                self.tab.nmr.labels = !self.tab.nmr.labels;
                self.tab.nmr.label_menu = false;
            }
            Action::Labels => {}
            Action::LabelMode(mode) => {
                self.tab.nmr.label_mode = mode;
                self.tab.nmr.label_menu = false;
            }
            Action::LabelMenu(open) => self.tab.nmr.label_menu = open && self.tab.nmr.labels,
            Action::Move(position) if position.x.is_finite() && position.y.is_finite() => {
                self.tab.nmr.position = Some(position);
            }
            Action::Resize(size) if size.width.is_finite() && size.height.is_finite() => {
                self.tab.nmr.width = size.width.clamp(320., 600.);
                self.tab.nmr.height = size.height.clamp(180., 640.);
                self.tab.nmr.resized = true;
            }
            Action::ResetPosition => self.tab.nmr.position = None,
            Action::Move(_) | Action::Resize(_) => {}
            Action::Layout(size) => self.tab.nmr.available = size,
            Action::PanelSize(size) => self.tab.nmr.rendered_size = size,
            Action::SelectMarker(indices) => {
                let Some(report) = &self.tab.nmr.result else {
                    return Task::none();
                };
                let indices: Vec<_> = indices
                    .into_iter()
                    .filter(|&index| {
                        report
                            .rows
                            .get(index)
                            .is_some_and(|r| r.statistics.is_some())
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
                    return self.nmr_action(Action::Select(index));
                }
            }
            Action::Predict(nucleus) => {
                if self.tab.nmr.pending.is_some() {
                    return Task::none();
                }
                self.tab.nmr.open = true;
                self.tab.nmr.collapsed = false;
                self.tab.nmr.nucleus = nucleus;
                self.tab.nmr.result = None;
                self.tab.nmr.labels = false;
                self.tab.nmr.label_menu = false;
                let request = match engine::prepare(&self.tab.doc, &self.tab.selected) {
                    Ok(r) => r,
                    Err(error) => {
                        self.tab.nmr.notice = Some(error.clone());
                        self.status = error;
                        self.error = true;
                        return Task::none();
                    }
                };
                self.tab.nmr.serial = self.tab.nmr.serial.wrapping_add(1);
                let key = Key {
                    serial: self.tab.nmr.serial,
                    epoch: self.tab.file_epoch,
                    ids: request.atom_ids.clone(),
                    fingerprint: request.fingerprint.clone(),
                };
                self.tab.nmr.pending = Some(key.clone());
                self.tab.nmr.notice = None;
                self.status = format!(
                    "Predicting {} chemical shifts from local observed data…",
                    nucleus.label()
                );
                self.error = false;
                return Task::perform(calculate(request, nucleus), move |result| {
                    Message::Nmr(Action::Finished(key.clone(), result))
                });
            }
            Action::Finished(key, result) => {
                if self.tab.nmr.pending.as_ref() != Some(&key) {
                    return Task::none();
                }
                self.tab.nmr.pending = None;
                if key.epoch != self.tab.file_epoch
                    || engine::fingerprint(&self.tab.doc, &key.ids).ok().as_deref()
                        != Some(key.fingerprint.as_str())
                {
                    self.tab.nmr.notice =
                        Some("Molecular chemistry changed; run prediction again".into());
                    return Task::none();
                }
                match result {
                    Ok(report) => {
                        let supported = report
                            .rows
                            .iter()
                            .filter(|r| r.statistics.is_some())
                            .count();
                        self.status = format!(
                            "{} NMR prediction · {supported}/{} supported atom-linked sites",
                            report.nucleus.label(),
                            report.rows.len()
                        );
                        self.error = false;
                        self.tab.nmr.result_epoch = Some(key.epoch);
                        self.tab.nmr.result = Some(report);
                    }
                    Err(error) => {
                        self.tab.nmr.notice = Some(error.clone());
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::Select(index) => {
                if let Some(row) = self.tab.nmr.result.as_ref().and_then(|r| r.rows.get(index)) {
                    self.tab.selected = std::iter::once(row.atom_id)
                        .chain(row.hydrogen_ids.iter().copied())
                        .collect();
                    let collapsed = self
                        .tab
                        .doc
                        .abbreviations
                        .iter()
                        .find(|a| a.members.contains(&row.atom_id));
                    if let Some(group) = collapsed
                        && !self.tab.selected.contains(&group.anchor)
                    {
                        self.tab.selected.push(group.anchor);
                    }
                    self.status = format!(
                        "NMR prediction linked to atom #{}{}",
                        row.atom_id,
                        if row.hydrogen_count > 1 && self.tab.nmr.nucleus == Nucleus::H1 {
                            " · unresolved attached-H group"
                        } else {
                            ""
                        }
                    );
                }
            }
            Action::Copy => {
                if let Some(report) = &self.tab.nmr.result {
                    self.status =
                        "Copied predicted NMR shifts with method, conditions and data attribution"
                            .into();
                    return iced::clipboard::write(engine::to_tsv(report));
                }
            }
            Action::Export => {
                if let Some(report) = &self.tab.nmr.result {
                    let contents = engine::to_tsv(report);
                    return Task::perform(
                        async move {
                            let Some(path) = files::save_path(
                                "Export predicted NMR shifts",
                                "Predicted NMR shifts.tsv",
                                "tsv",
                            )
                            .await
                            else {
                                return Ok(None);
                            };
                            reshiki::storage::write_atomic(&path, contents.as_bytes())?;
                            Ok(Some(path))
                        },
                        |result| Message::Nmr(Action::Exported(result)),
                    );
                }
            }
            Action::Exported(result) => match result {
                Ok(Some(path)) => {
                    self.status = format!("Exported predicted NMR shifts to {}", path.display());
                    self.error = false;
                }
                Ok(None) => {}
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            },
        }
        Task::none()
    }

    pub(super) fn nmr_canvas(&self) -> Option<crate::canvas::nmr::Context<'_>> {
        let state = &self.tab.nmr;
        (state.open && state.labels).then_some(())?;
        Some(crate::canvas::nmr::Context {
            report: state.result.as_deref()?,
            mode: state.label_mode,
        })
    }

    pub(super) fn nmr_leave_tab(&mut self) {
        self.tab.nmr.restore_dock = self.tab.nmr.open
            && self.tab.nmr.docked
            && self.inspector_open
            && self.inspector_tab == InspectorTab::Nmr;
        if self.tab.nmr.restore_dock {
            self.place_nmr_inspector(false);
        }
    }

    pub(super) fn nmr_enter_tab(&mut self) -> bool {
        if !self.tab.nmr.restore_dock {
            return false;
        }
        self.place_nmr_inspector(true);
        // Restoring a report must preserve its saved camera even when the
        // other document already had an equally wide inspector open.
        true
    }

    pub(super) fn nmr_inspector(&self) -> Element<'_, Message> {
        if self.tab.nmr.open && self.tab.nmr.docked {
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
                    Action::Dock
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
                self.tab.nmr.dock_return = Some((self.inspector_open, self.inspector_tab));
            }
            self.inspector_open = true;
            self.inspector_tab = InspectorTab::Nmr;
        } else if self.inspector_tab == InspectorTab::Nmr {
            let (open, tab) = self
                .tab
                .nmr
                .dock_return
                .take()
                .unwrap_or((true, InspectorTab::Properties));
            self.inspector_open = open;
            self.inspector_tab = tab;
        }
        let new_width = self.inspector_width();
        if old_width != new_width {
            self.tab.nmr.layout_transition = Some(Size::new(
                self.viewport.width + old_width - new_width,
                self.viewport.height,
            ));
        }
    }

    pub(super) fn with_nmr<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let state = &self.tab.nmr;
        // Floating content stays above the unchanged workspace. Docking is an
        // explicit choice to use the existing inspector, never a camera fit.
        let mut layers = stack![base];
        if state.open && !state.docked {
            layers = layers.push(palette::floating(
                self.nmr_panel(true),
                state.position,
                Size::new(state.width, state.available.height),
                !state.collapsed,
            ));
        }
        sensor(layers)
            .on_show(|size| Message::Nmr(Action::Layout(size)))
            .on_resize(|size| Message::Nmr(Action::Layout(size)))
            .into()
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
                }
            )
            .style(segment(true)),
            named_command(
                "nmr.collapse",
                "Show or hide the NMR prediction results",
                if state.collapsed { "+" } else { "−" },
                Action::Collapse
            )
            .expanded(!state.collapsed)
            .padding([4, 5]),
            named_command("nmr.close", "Close NMR prediction", "×", Action::Close).padding([4, 5]),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if state.collapsed {
            let title = container(title).padding(10).style(surface);
            return if floating {
                title.width(state.width).into()
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
                named_command(id, name, label, Action::Predict(nucleus))
                    .checked(state.nucleus == nucleus)
                    .on_press_maybe(
                        state
                            .pending
                            .is_none()
                            .then_some(Message::Nmr(Action::Predict(nucleus))),
                    )
                    .width(Length::Fill)
                    .height(30)
                    .style(segment(state.nucleus == nucleus)),
            );
        }
        // Iced draws the real checkbox; the enclosing named control supplies
        // the application's keyboard/native activation and checked semantics.
        let labels = reshiki::accessibility::button(
            "nmr.labels",
            "Show atom-linked NMR assignment labels on the drawing",
            checkbox(state.labels)
                .label("Show labels")
                .size(15)
                .text_size(12)
                .spacing(7)
                .style(move |theme, _| {
                    checkbox::primary(
                        theme,
                        if state.result.is_some() {
                            checkbox::Status::Active {
                                is_checked: state.labels,
                            }
                        } else {
                            checkbox::Status::Disabled {
                                is_checked: state.labels,
                            }
                        },
                    )
                }),
        )
        .checked(state.labels)
        .padding([3, 0])
        .style(quiet)
        .on_press_maybe(
            state
                .result
                .is_some()
                .then_some(Message::Nmr(Action::Labels)),
        );
        let mode = row![
            text(label_mode_name(state.label_mode)).size(12),
            Space::new().width(Length::Fill),
            text("▾").size(10)
        ];
        let anchor =
            reshiki::accessibility::button("nmr.labels.mode", "NMR assignment label format", mode)
                .expanded(state.label_menu)
                .value(label_mode_name(state.label_mode))
                .padding([5, 7])
                .width(132)
                .style(outlined)
                .on_press_maybe(
                    state
                        .labels
                        .then_some(Message::Nmr(Action::LabelMenu(!state.label_menu))),
                );
        let popup = state.label_menu.then(|| {
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
                    )
                    .checked(state.label_mode == mode)
                    .width(Length::Fill)
                    .style(segment(state.label_mode == mode)),
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
        if state.pending.is_some() {
            body = body.push(text("Calculating from the molecular graph…").size(12));
        }
        if let Some(notice) = &state.notice {
            body = body.push(text(notice).size(12));
        }
        if let Some(report) = &state.result {
            if state.plot {
                body = body.push(text("Predicted shifts").size(12).style(muted_text));
                body = if report.rows.iter().any(|row| row.statistics.is_some()) {
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
                                && let Some(group) = self
                                    .tab
                                    .doc
                                    .abbreviations
                                    .iter()
                                    .find(|group| group.members.contains(&item.atom_id))
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
                    .on_press(Message::Nmr(Action::Select(index)))
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
                    Action::Plot
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
                ));
                let height = state.rendered_size.height;
                options = options
                    .push(
                        named_command(
                            "nmr.size.less",
                            "Reduce NMR palette size",
                            "−",
                            Action::Resize(Size::new(state.width - 40., height - 40.)),
                        )
                        .on_press_maybe(
                            (state.width > 320. || state.height > 180.).then_some(Message::Nmr(
                                Action::Resize(Size::new(state.width - 40., height - 40.)),
                            )),
                        ),
                    )
                    .push(
                        named_command(
                            "nmr.size.more",
                            "Increase NMR palette size",
                            "+",
                            Action::Resize(Size::new(state.width + 40., height + 40.)),
                        )
                        .on_press_maybe(
                            (state.width < 600. || state.height < 640.).then_some(Message::Nmr(
                                Action::Resize(Size::new(state.width + 40., height + 40.)),
                            )),
                        ),
                    );
            }
            body = body.push(options)
                .push(text("The plot marks chemical shifts only. Stick heights do not represent intensity or integration. No J couplings, multiplets or second-order simulation are predicted. Nearby labels cycle their distinct atom-linked sites.").size(11))
                .push(text(reshiki::chemistry::nmr::METHOD).size(11))
                .push(text(engine::DATA_VERSION).size(11))
                .push(text(engine::CONDITIONS).size(11))
                .push(text(engine::LIMITATIONS).size(11))
                .push(text(engine::ATTRIBUTION).size(10));
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
                Action::Details
            )
            .expanded(state.details)
            .value(panel_description(state))
            .padding([5, 0]),
            Space::new().width(Length::Fill),
            named_command(
                "nmr.copy",
                "Copy predicted NMR shifts with attribution",
                "Copy",
                Action::Copy
            )
            .on_press_maybe(state.result.is_some().then_some(Message::Nmr(Action::Copy))),
            named_command(
                "nmr.export",
                "Export predicted NMR shifts with attribution as TSV",
                "Export…",
                Action::Export
            )
            .on_press_maybe(
                state
                    .result
                    .is_some()
                    .then_some(Message::Nmr(Action::Export))
            ),
        ]
        .spacing(3)
        .align_y(Alignment::Center);
        if floating {
            commands = commands.push(text("◢").size(12).style(muted_text));
        }
        let footer = column![
            text(if state.nucleus == Nucleus::H1 {
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
            (floating && state.resized).then_some((state.height - 20.).max(0.)),
            floating,
        );
        let panel = container(content).padding(10).style(surface);
        if floating {
            sensor(panel.width(state.width))
                .on_show(|size| Message::Nmr(Action::PanelSize(size)))
                .on_resize(|size| Message::Nmr(Action::PanelSize(size)))
                .into()
        } else {
            palette::guard(panel.width(Length::Fill).height(Length::Fill))
        }
    }
}

#[cfg(test)]
mod tests;

fn named_command<'a>(
    id: &'static str,
    name: &'static str,
    label: &'a str,
    action: Action,
) -> reshiki::accessibility::Button<'a, Message> {
    reshiki::accessibility::button(id, name, text(label).size(12))
        .padding([5, 7])
        .on_press(Message::Nmr(action))
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
fn surface(theme: &Theme) -> container::Style {
    let dark = crate::appearance::is_dark(theme);
    container::Style {
        background: Some(
            if dark {
                theme.extended_palette().background.base.color
            } else {
                Color::WHITE
            }
            .into(),
        ),
        text_color: Some(theme.palette().text),
        border: Border {
            color: if dark {
                theme.extended_palette().background.strong.color
            } else {
                Color::from_rgb8(215, 223, 229)
            },
            width: 1.,
            radius: 6.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(if dark { 0.2 } else { 0.08 }),
            offset: Vector::new(0., 3.),
            blur_radius: 12.,
        },
        ..Default::default()
    }
}
fn table_surface(theme: &Theme) -> container::Style {
    container::Style {
        shadow: Shadow::default(),
        border: Border {
            radius: 4.into(),
            ..surface(theme).border
        },
        ..surface(theme)
    }
}
fn rule(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(surface(theme).border.color.into()),
        ..Default::default()
    }
}
fn quiet(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = super::workspace::control(false)(theme, status);
    style.border = Border::default();
    style
}
fn outlined(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = quiet(theme, status);
    style.border = Border {
        radius: 4.into(),
        ..surface(theme).border
    };
    style
}
fn segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = super::workspace::control(selected)(theme, status);
        style.border.radius = 4.into();
        if !selected {
            style.border = Border {
                radius: 4.into(),
                ..surface(theme).border
            };
        }
        style
    }
}
fn result_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = super::workspace::control(selected)(theme, status);
        style.border = Border::default();
        style
    }
}

fn panel_description(state: &State) -> String {
    let status = if state.pending.is_some() {
        format!("Calculating {} shifts", state.nucleus.label())
    } else if let Some(notice) = &state.notice {
        notice.clone()
    } else if let Some(report) = &state.result {
        format!(
            "{} prediction: {} of {} atom-linked sites supported",
            report.nucleus.label(),
            report
                .rows
                .iter()
                .filter(|r| r.statistics.is_some())
                .count(),
            report.rows.len()
        )
    } else {
        "No prediction result".into()
    };
    format!(
        "Status: {status}. {}. {}. {}. {}. {}",
        reshiki::chemistry::nmr::METHOD,
        engine::DATA_VERSION,
        engine::CONDITIONS,
        engine::LIMITATIONS,
        engine::ATTRIBUTION
    )
}
