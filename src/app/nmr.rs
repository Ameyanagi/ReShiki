//! Atom-linked offline NMR results; detached work and chemical identity gates.
use super::{
    App, Message, files,
    workspace::{command, control, muted_text},
};
use iced::widget::{Space, column, container, row, scrollable, sensor, slider, text};
use iced::{Alignment, Element, Length, Task};
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
    Predict(Nucleus),
    Collapse,
    Details,
    Height(f32),
    Layout(iced::Size),
    Select(usize),
    Copy,
    Export,
    Finished(Key, Result<Arc<Report>, String>),
    Exported(Result<Option<std::path::PathBuf>, String>),
}

pub(super) struct State {
    pub(super) open: bool,
    collapsed: bool,
    details: bool,
    height: f32,
    available_height: f32,
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
            height: 250.,
            available_height: 600.,
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
            self.serial = self.serial.wrapping_add(1);
            self.notice = Some("Molecular chemistry changed; run prediction again".into());
        }
    }
}

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
                return self.nmr_action(Action::Predict(self.tab.nmr.nucleus));
            }
            Action::Close => self.tab.nmr.open = false,
            Action::Collapse => self.tab.nmr.collapsed = !self.tab.nmr.collapsed,
            Action::Details => self.tab.nmr.details = !self.tab.nmr.details,
            Action::Height(value) => self.tab.nmr.height = value.clamp(140., 360.),
            Action::Layout(size) => self.tab.nmr.available_height = size.height,
            Action::Predict(nucleus) => {
                if self.tab.nmr.pending.is_some() {
                    return Task::none();
                }
                self.tab.nmr.open = true;
                self.tab.nmr.collapsed = false;
                self.tab.nmr.nucleus = nucleus;
                self.tab.nmr.result = None;
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

    pub(super) fn with_nmr<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.tab.nmr.open {
            return sensor(column![container(base).height(Length::Fill)])
                .on_show(|size| Message::Nmr(Action::Layout(size)))
                .on_resize(|size| Message::Nmr(Action::Layout(size)))
                .into();
        }
        let state = &self.tab.nmr;
        let header = row![
            text("NMR prediction").size(15),
            command("¹H", Message::Nmr(Action::Predict(Nucleus::H1)))
                .on_press_maybe(
                    state
                        .pending
                        .is_none()
                        .then_some(Message::Nmr(Action::Predict(Nucleus::H1)))
                )
                .style(control(state.nucleus == Nucleus::H1)),
            command("¹³C", Message::Nmr(Action::Predict(Nucleus::C13)))
                .on_press_maybe(
                    state
                        .pending
                        .is_none()
                        .then_some(Message::Nmr(Action::Predict(Nucleus::C13)))
                )
                .style(control(state.nucleus == Nucleus::C13)),
            Space::new().width(Length::Fill),
            command(
                if state.collapsed {
                    "Show table"
                } else {
                    "Collapse"
                },
                Message::Nmr(Action::Collapse)
            ),
            command("Close", Message::Nmr(Action::Close)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let mut panel = column![header].spacing(8);
        if !state.collapsed {
            let mut content = column![
                text("HOSE median v1 · nmrshiftdb2 2026-03-15 · measured CDCl3 data")
                    .size(11)
                    .style(muted_text)
            ]
            .spacing(7)
            .push(text("Connectivity-only: E/Z and relative stereochemistry are unmodeled; attached H groups are unresolved. Observed SD is reference dispersion.").size(10).style(muted_text));
            if state.pending.is_some() {
                content =
                    content.push(text("Calculating from the complete molecular graph…").size(13));
            }
            if let Some(notice) = &state.notice {
                content = content.push(text(notice).size(13));
            }
            if let Some(report) = &state.result {
                content = content.push(
                    row![
                        text("Atom / H site").width(Length::Fill),
                        text("δ / ppm").width(72),
                        text("Sphere").width(52),
                        text("Refs").width(52),
                        text("Obs. SD").width(72)
                    ]
                    .spacing(8),
                );
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
                    let mut line = column![
                        row![
                            reshiki::accessibility::button(
                                format!("nmr-site-{index}"),
                                format!("Select NMR site {label}"),
                                text(label).size(12)
                            )
                            .padding([5, 9])
                            .on_press(Message::Nmr(Action::Select(index)))
                            .style(control(false))
                            .width(Length::Fill),
                            text(
                                item.statistics
                                    .as_ref()
                                    .map(|s| format!("{:.3}", s.median))
                                    .unwrap_or_else(|| "—".into())
                            )
                            .width(72),
                            text(
                                item.radius
                                    .map(|r| r.to_string())
                                    .unwrap_or_else(|| "—".into())
                            )
                            .width(52),
                            text(
                                item.statistics
                                    .as_ref()
                                    .map(|s| s.support.to_string())
                                    .unwrap_or_else(|| "—".into())
                            )
                            .width(52),
                            text(
                                item.statistics
                                    .as_ref()
                                    .map(|s| format!("{:.3}", s.standard_deviation))
                                    .unwrap_or_else(|| "—".into())
                            )
                            .width(72)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    ]
                    .spacing(3);
                    if let Some(limitation) = &item.limitation {
                        line = line.push(text(limitation).size(10).style(muted_text));
                    }
                    content = content.push(line);
                }
            }
            if state.details {
                content = content
                    .push(text(engine::DATA_VERSION).size(11))
                    .push(text(engine::CONDITIONS).size(11))
                    .push(text(engine::LIMITATIONS).size(11))
                    .push(text(engine::ATTRIBUTION).size(10));
            }
            let toolbar = row![
                command(
                    if state.details {
                        "Hide method / limits"
                    } else {
                        "Method / conditions / limits"
                    },
                    Message::Nmr(Action::Details)
                ),
                command("Copy table", Message::Nmr(Action::Copy))
                    .on_press_maybe(state.result.is_some().then_some(Message::Nmr(Action::Copy))),
                command("Export TSV…", Message::Nmr(Action::Export)).on_press_maybe(
                    state
                        .result
                        .is_some()
                        .then_some(Message::Nmr(Action::Export))
                ),
                Space::new().width(Length::Fill),
                text("Height").size(10),
                slider(140.0..=360.0, state.height, |h| Message::Nmr(
                    Action::Height(h)
                ))
                .width(80)
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            panel = panel
                .push(scrollable(content).height(Length::Fill))
                .push(toolbar);
        }
        let height = if state.collapsed {
            48.
        } else {
            state.height.min(state.available_height * 0.4).max(140.)
        };
        sensor(column![
            container(base).height(Length::Fill),
            container(panel)
                .padding([10, 14])
                .height(height)
                .width(Length::Fill)
                .style(container::bordered_box)
        ])
        .on_show(|size| Message::Nmr(Action::Layout(size)))
        .on_resize(|size| Message::Nmr(Action::Layout(size)))
        .into()
    }
}

#[cfg(test)]
mod tests;
