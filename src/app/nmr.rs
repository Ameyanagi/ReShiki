//! Atom-linked offline NMR results; detached work and chemical identity gates.
use super::{
    App, Message, files,
    workspace::{control, muted_text},
};
use iced::widget::{Space, column, container, row, scrollable, sensor, slider, stack, text};
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

mod palette;

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
        let state = &self.tab.nmr;
        // Keep the workspace at the same child position and size as the palette
        // opens or expands. The drawing sensor must not see a smaller viewport.
        let mut layers = stack![base];
        if state.open {
            let header = row![
                text("NMR prediction").size(14),
                named_command(
                    "nmr.predict.1H",
                    "Predict proton (¹H) NMR shifts",
                    "¹H",
                    Action::Predict(Nucleus::H1)
                )
                .checked(state.nucleus == Nucleus::H1)
                .on_press_maybe(
                    state
                        .pending
                        .is_none()
                        .then_some(Message::Nmr(Action::Predict(Nucleus::H1)))
                )
                .style(control(state.nucleus == Nucleus::H1)),
                named_command(
                    "nmr.predict.13C",
                    "Predict carbon-13 (¹³C) NMR shifts",
                    "¹³C",
                    Action::Predict(Nucleus::C13)
                )
                .checked(state.nucleus == Nucleus::C13)
                .on_press_maybe(
                    state
                        .pending
                        .is_none()
                        .then_some(Message::Nmr(Action::Predict(Nucleus::C13)))
                )
                .style(control(state.nucleus == Nucleus::C13)),
                Space::new().width(Length::Fill),
                named_command(
                    "nmr.collapse",
                    "Show or hide the NMR prediction results",
                    if state.collapsed { "Show" } else { "−" },
                    Action::Collapse
                )
                .expanded(!state.collapsed),
                named_command(
                    "nmr.close",
                    "Close the NMR prediction palette",
                    "×",
                    Action::Close
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center);
            let mut panel = column![header].spacing(6);
            if !state.collapsed {
                let mut content = column![].spacing(5);
                if state.pending.is_some() {
                    content = content.push(text("Calculating from the molecular graph…").size(12));
                }
                if let Some(notice) = &state.notice {
                    content = content.push(text(notice).size(12));
                }
                if let Some(report) = &state.result {
                    let mut headings = row![
                        text("Atom / H site").size(11).width(Length::Fill),
                        text("δ / ppm").size(11).width(64),
                    ]
                    .spacing(8);
                    if state.details {
                        headings = headings
                            .push(text("Sphere").size(11).width(44))
                            .push(text("Refs").size(11).width(44))
                            .push(text("Obs. SD").size(11).width(64));
                    }
                    content = content.push(container(headings).padding([0, 7]));
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
                        let mut values = row![
                            text(label).size(12).width(Length::Fill),
                            text(
                                item.statistics
                                    .as_ref()
                                    .map(|s| format!("{:.3}", s.median))
                                    .unwrap_or_else(|| "—".into())
                            )
                            .size(12)
                            .width(64),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center);
                        if state.details {
                            values = values
                                .push(
                                    text(
                                        item.radius
                                            .map(|r| r.to_string())
                                            .unwrap_or_else(|| "—".into()),
                                    )
                                    .size(12)
                                    .width(44),
                                )
                                .push(
                                    text(
                                        item.statistics
                                            .as_ref()
                                            .map(|s| s.support.to_string())
                                            .unwrap_or_else(|| "—".into()),
                                    )
                                    .size(12)
                                    .width(44),
                                )
                                .push(
                                    text(
                                        item.statistics
                                            .as_ref()
                                            .map(|s| format!("{:.3}", s.standard_deviation))
                                            .unwrap_or_else(|| "—".into()),
                                    )
                                    .size(12)
                                    .width(64),
                                );
                        }
                        let selected = self.tab.selected.contains(&item.atom_id);
                        let mut site = column![
                            reshiki::accessibility::button(
                                format!("nmr.site.{}.{}", report.nucleus.code(), item.atom_id),
                                site_description(report.nucleus, item),
                                values,
                            )
                            .padding([5, 7])
                            .on_press(Message::Nmr(Action::Select(index)))
                            .checked(selected)
                            .value(format!("{}. {}", report.data_version, report.conditions))
                            .style(control(selected))
                            .width(Length::Fill)
                        ]
                        .spacing(2);
                        if let Some(limitation) = &item.limitation {
                            site = site.push(text(limitation).size(10).style(muted_text));
                        }
                        content = content.push(site);
                    }
                }
                if state.details {
                    content = content
                        .push(text(reshiki::chemistry::nmr::METHOD).size(11))
                        .push(text(engine::DATA_VERSION).size(11))
                        .push(text(engine::CONDITIONS).size(11))
                        .push(text(engine::LIMITATIONS).size(11))
                        .push(text(engine::ATTRIBUTION).size(10));
                }
                let mut toolbar = row![
                    named_command(
                        "nmr.method",
                        "NMR prediction method, source, conditions and limitations",
                        if state.details {
                            "Compact"
                        } else {
                            "Details…"
                        },
                        Action::Details
                    )
                    .expanded(state.details)
                    .value(panel_description(state)),
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
                .spacing(5)
                .align_y(Alignment::Center);
                if state.details {
                    toolbar = toolbar
                        .push(named_command(
                            "nmr.height.less",
                            "Reduce NMR palette height",
                            "−",
                            Action::Height(state.height - 20.)
                        )
                        .on_press_maybe((state.height > 140.).then_some(Message::Nmr(Action::Height(state.height - 20.))))
                        .value(format!("Palette height {:.0} logical pixels; requested {:.0}; range 140–360", panel_height(state), state.height)))
                        .push(text(format!("{:.0} px", panel_height(state))).size(10))
                        .push(named_command(
                            "nmr.height.more",
                            "Increase NMR palette height",
                            "+",
                            Action::Height(state.height + 20.)
                        )
                        .on_press_maybe((state.height < 360.).then_some(Message::Nmr(Action::Height(state.height + 20.))))
                        .value(format!("Palette height {:.0} logical pixels; requested {:.0}; range 140–360", panel_height(state), state.height)))
                        .push(slider(140.0..=360.0, state.height, |h| Message::Nmr(Action::Height(h))).width(64));
                }
                panel = panel
                    .push(scrollable(content).height(Length::Fill))
                    .push(
                        text(if state.nucleus == Nucleus::H1 {
                            "Predicted H groups · unresolved · CDCl3"
                        } else {
                            "Predicted carbon shifts · CDCl3 reference"
                        })
                        .size(10)
                        .style(muted_text),
                    )
                    .push(toolbar);
            }
            let width = if state.details && !state.collapsed {
                520.
            } else {
                320.
            };
            let palette = container(panel)
                .padding(8)
                .height(panel_height(state))
                .width(width)
                .style(container::bordered_box);
            layers = layers.push(
                container(palette::guard(palette))
                    .padding(iced::Padding {
                        top: 150.,
                        right: 12.,
                        bottom: 38.,
                        left: 112.,
                    })
                    .align_right(Length::Fill)
                    .align_bottom(Length::Fill),
            );
        }
        sensor(layers)
            .on_show(|size| Message::Nmr(Action::Layout(size)))
            .on_resize(|size| Message::Nmr(Action::Layout(size)))
            .into()
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
        .style(control(false))
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

fn panel_height(state: &State) -> f32 {
    if state.collapsed {
        48.
    } else {
        let requested = if state.details {
            state.height
        } else {
            124. + 30. * state.result.as_ref().map_or(0, |r| r.rows.len().min(4)) as f32
        };
        requested
            .clamp(if state.details { 140. } else { 150. }, 360.)
            .min((state.available_height - 188.).max(150.))
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
