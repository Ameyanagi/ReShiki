//! NMR palette and docked report builders, and accessible descriptions.
use super::palette::{MAX_SIZE, MIN_SIZE, PANEL_PADDING, RESIZE_STEP};
use super::panel::site_id_prefix;
use super::presentation::{self, Presentation};
use super::style::{outlined, quiet, result_row, rule, segment, surface, table_surface};
use super::{Action, State, palette, panel, plot};
use crate::app::{
    App, Message,
    workspace::{control, muted_text},
};
use crate::canvas::nmr::LabelMode;
use iced::widget::{
    Column, Row, Space, checkbox, column, container, row, scrollable, sensor, stack, text,
};
use iced::{Alignment, Element, Length, Size};
use reshiki::document::Document;
use reshiki::nmr::{self as engine, Nucleus, Report};
use std::borrow::Cow;

const FLOATING_TITLE: &str = "NMR prediction";
/// The section heading inside the Predict tab, which already names the task.
const DOCKED_TITLE: &str = "NMR";
/// The panel's line while nothing is predicted or calculating.
const EMPTY_STATE: &str = "Select a molecule, or draw one, to predict its NMR shifts.";

/// Where the panel is shown: the movable palette over the drawing, or docked
/// in the inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Placement {
    Floating,
    Docked,
}

impl App {
    /// The inspector tab's content, inside the inspector's standard padded
    /// scrollable like every other tab.
    pub(in crate::app) fn nmr_inspector(&self) -> Element<'_, Message> {
        if self.tab.nmr.open && self.tab.nmr.dock.docked {
            return self.nmr_panel(Placement::Docked);
        }
        // Inspector selection is global; prediction mode/results belong to a
        // document. Never instantiate a second panel for a floating document.
        column![
            text(DOCKED_TITLE).size(14),
            text(if self.tab.nmr.open {
                "Results are in the floating palette."
            } else {
                EMPTY_STATE
            })
            .size(12)
            .style(muted_text),
            named_command(
                "nmr.inspector.dock",
                "Dock NMR prediction in this inspector",
                "Dock here",
                Action::Dock,
                true,
            )
            .style(outlined),
        ]
        .spacing(10)
        .into()
    }

    pub(in crate::app) fn with_nmr<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let state = &self.tab.nmr;
        // Floating content stays above the unchanged workspace. Docking is an
        // explicit choice to use the existing inspector, never a camera fit.
        let mut layers = stack![base];
        if state.open && !state.dock.docked {
            layers = layers.push(palette::floating(
                self.nmr_panel(Placement::Floating),
                state.palette.position,
                state.palette.width,
                !state.collapsed,
            ));
        }
        layers.into()
    }

    /// The floating palette card, or the docked report as plain inspector
    /// content: no card, shadow or inner scrollable, and always the full
    /// report, since docked mode has no collapse control. The floating
    /// collapse state is kept for Undock.
    pub(super) fn nmr_panel(&self, placement: Placement) -> Element<'_, Message> {
        let state = &self.tab.nmr;
        let title = title_bar(state, placement);
        if placement == Placement::Floating && state.collapsed {
            return container(title)
                .padding(PANEL_PADDING)
                .style(surface)
                .width(state.palette.width)
                .into();
        }
        let header = column![title, nucleus_switch(state), label_controls(state)].spacing(8);
        let body = self.nmr_body(placement);
        let footer = footer(state, placement);
        if placement == Placement::Docked {
            return palette::guard(column![header, body, footer].spacing(12));
        }
        let content = panel::view(
            header,
            scrollable(
                container(body).padding(
                    iced::Padding::ZERO
                        .bottom(panel::CONTENT_END_INSET)
                        .right(panel::SCROLLBAR_GUTTER),
                ),
            )
            .id("nmr-results")
            .height(Length::Shrink),
            footer,
            state
                .palette
                .resized
                .then_some((state.palette.height - 2. * PANEL_PADDING).max(0.)),
        );
        let panel = container(content)
            .padding(PANEL_PADDING)
            .style(surface)
            .width(state.palette.width);
        sensor(panel)
            .on_show(|size| Message::Nmr(Action::RenderedSize(size)))
            .on_resize(|size| Message::Nmr(Action::RenderedSize(size)))
            .into()
    }

    fn nmr_body(&self, placement: Placement) -> Column<'_, Message> {
        let state = &self.tab.nmr;
        let mut body = column![].spacing(8);
        if state.job.pending.is_some() {
            body = body.push(text("Calculating from the molecular graph…").size(12));
        }
        if let Some(notice) = &state.job.notice {
            body = body.push(text(notice).size(12));
        }
        if let Some(report) = &state.job.result {
            let presentation = state
                .job
                .presentation
                .as_ref()
                .filter(|presentation| presentation.matches(report));
            body = plot_section(body, state, report, presentation, &self.tab.selected);
            body = body.push(results_table(
                state,
                report,
                presentation,
                &self.tab.doc,
                &self.tab.selected,
            ));
        } else if state.job.pending.is_none() && state.job.notice.is_none() {
            body = body.push(text(EMPTY_STATE).size(12).style(muted_text));
        }
        if reported(state) {
            body = details_section(body, state, placement);
        }
        body
    }
}

/// A report exists or is being calculated, so provenance and Details apply.
fn reported(state: &State) -> bool {
    state.job.result.is_some() || state.job.pending.is_some()
}

fn title_bar(state: &State, placement: Placement) -> Row<'_, Message> {
    let close = named_command(
        "nmr.close",
        "Close NMR prediction",
        "×",
        Action::Close,
        true,
    )
    .padding([4, 5]);
    match placement {
        Placement::Floating => row![
            text("⠿").size(15).style(muted_text),
            text(FLOATING_TITLE).size(14),
            Space::new().width(Length::Fill),
            named_command(
                "nmr.mode",
                "Dock NMR in the inspector",
                "Dock",
                Action::Dock,
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
            close,
        ],
        Placement::Docked => row![
            text(DOCKED_TITLE).size(14),
            Space::new().width(Length::Fill),
            named_command(
                "nmr.mode",
                "Undock NMR into a movable palette",
                "Undock",
                Action::Undock,
                true,
            )
            .style(outlined),
            close,
        ],
    }
    .spacing(4)
    .align_y(Alignment::Center)
}

fn nucleus_switch(state: &State) -> Row<'_, Message> {
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
        let label = text(label)
            .size(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        nuclei = nuclei.push(
            reshiki::accessibility::button(id, name, label)
                .padding([5, 7])
                .on_press_maybe(
                    state
                        .job
                        .pending
                        .is_none()
                        .then_some(Message::Nmr(Action::Predict(nucleus))),
                )
                .checked(state.job.nucleus == nucleus)
                .width(Length::Fill)
                .height(30)
                .style(segment(state.job.nucleus == nucleus)),
        );
    }
    nuclei
}

fn label_controls(state: &State) -> Element<'_, Message> {
    // Iced draws the real checkbox; the enclosing named control supplies
    // the application's keyboard/native activation and checked semantics.
    let labels = reshiki::accessibility::button(
        "nmr.labels",
        "Show NMR assignment labels on the drawing",
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
    let toggle_menu = Message::Nmr(Action::LabelMenu(!state.labels.menu));
    let anchor = crate::app::popover::choice_anchor(
        "nmr.labels.mode",
        "NMR assignment label format".into(),
        label_mode_name(state.labels.mode).into(),
        12.,
        [5, 7],
        state.labels.menu,
        toggle_menu.clone(),
    )
    .value(label_mode_name(state.labels.mode))
    .width(132)
    .on_press_maybe(state.labels.visible.then_some(toggle_menu));
    let popup = state.labels.menu.then(|| {
        let choices = [
            ("nmr.labels.atom", "Atom numbers", LabelMode::Atom),
            ("nmr.labels.ppm", "ppm", LabelMode::Shift),
            ("nmr.labels.both", "Both", LabelMode::Both),
        ]
        .map(|(id, label, mode)| {
            reshiki::accessibility::button(
                id,
                "Choose NMR assignment label format",
                text(label).size(12),
            )
            .checked(state.labels.mode == mode)
            .padding([6, 10])
            .width(Length::Fill)
            .style(control(state.labels.mode == mode))
            .on_press(Message::Nmr(Action::LabelMode(mode)))
            .into()
        });
        palette::menu(
            container(column(choices))
                .width(Length::Fill)
                .padding(5)
                .style(surface),
        )
    });
    let dropdown = Element::new(
        crate::app::popover::popover(anchor, popup, Message::Nmr(Action::LabelMenu(false)))
            .fit_anchor(),
    );
    row![labels, dropdown]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
}

fn plot_section<'a>(
    mut body: Column<'a, Message>,
    state: &State,
    report: &'a Report,
    presentation: Option<&'a Presentation>,
    selected: &'a [u64],
) -> Column<'a, Message> {
    if state.plot {
        body = body.push(text("Predicted shifts").size(12).style(muted_text));
        body = if presentation.map_or_else(
            || report.rows.iter().any(engine::Row::has_statistics),
            |presentation| presentation.supported_count > 0,
        ) {
            body.push(plot::view_with_presentation(report, selected, presentation))
        } else {
            body.push(
                text("No supported shifts to plot")
                    .size(11)
                    .style(muted_text),
            )
        };
    }
    body
}

fn results_table<'a>(
    state: &'a State,
    report: &'a Report,
    presentation: Option<&'a Presentation>,
    document: &'a Document,
    selected: &'a [u64],
) -> Element<'a, Message> {
    let mut table = column![
        container(
            row![
                text("Atom").size(12).width(Length::Fill),
                text("δ / ppm").size(12).width(72).align_x(Alignment::End),
            ]
            .spacing(8)
        )
        .padding([7, 9])
    ];
    for (index, item) in report.rows.iter().enumerate() {
        table = table.push(container(Space::new().height(1).width(Length::Fill)).style(rule));
        table = table.push(panel::table_row(site_row(
            state,
            report,
            item,
            index,
            presentation,
            document,
            selected,
        )));
    }
    container(table).style(table_surface).into()
}

fn site_row<'a>(
    state: &'a State,
    report: &'a Report,
    item: &'a engine::Row,
    index: usize,
    presentation: Option<&'a Presentation>,
    document: &'a Document,
    selected: &'a [u64],
) -> reshiki::accessibility::Button<'a, Message> {
    let site = presentation.and_then(|presentation| presentation.rows.get(index));
    let label = site.map_or_else(
        || {
            Cow::Owned(presentation::row_label(
                report.nucleus,
                item,
                presentation::element(document, item),
            ))
        },
        |site| Cow::Borrowed(site.label.as_str()),
    );
    let shift = site.map_or_else(
        || Cow::Owned(presentation::row_shift(item)),
        |site| Cow::Borrowed(site.shift.as_str()),
    );
    let mut description = site.map_or_else(
        || site_description(report.nucleus, item, &label),
        |site| site.description.clone(),
    );
    if !document.atom_visible(item.atom_id)
        && let Some(group) = document.abbreviation_containing(item.atom_id)
    {
        description.push_str(&format!(
            ". Shown at abbreviation {} anchored on atom #{}",
            group.label, group.anchor
        ));
    }
    let mut values = column![
        row![
            text(label).size(12).width(Length::Fill),
            text(shift).size(12).width(72).align_x(Alignment::End),
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
            text(site.map_or_else(
                || Cow::Owned(presentation::row_details(item)),
                |site| Cow::Borrowed(site.details.as_str()),
            ))
            .size(10)
            .style(muted_text),
        );
    }
    reshiki::accessibility::button(
        format!(
            concat!(site_id_prefix!(), "{}.{}"),
            report.nucleus.code(),
            item.atom_id
        ),
        description,
        values,
    )
    .padding([7, 9])
    .width(Length::Fill)
    .on_press(Message::Nmr(Action::SelectRow(index)))
    .checked(selected.contains(&item.atom_id))
    .value(presentation.map_or_else(
        || format!("{}. {}", report.data_version, report.conditions),
        |presentation| presentation.value.clone(),
    ))
    .style(result_row(selected.contains(&item.atom_id)))
}

fn details_section<'a>(
    mut body: Column<'a, Message>,
    state: &'a State,
    placement: Placement,
) -> Column<'a, Message> {
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
        if placement == Placement::Floating {
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
                    state.palette.width > MIN_SIZE.width || state.palette.height > MIN_SIZE.height,
                ))
                .push(named_command(
                    "nmr.size.more",
                    "Increase NMR palette size",
                    "+",
                    Action::Resize(Size::new(
                        state.palette.width + RESIZE_STEP,
                        height + RESIZE_STEP,
                    )),
                    state.palette.width < MAX_SIZE.width || state.palette.height < MAX_SIZE.height,
                ));
        }
        let source = provenance(state.job.result.as_deref());
        body = body
            .push(options)
            .push(text(engine::PLOT_NOTES.join(" ")).size(11))
            .push(text(source.method).size(11))
            .push(text(source.data_version).size(11))
            .push(text(source.conditions).size(11))
            .push(text(source.limitations).size(11))
            .push(text(source.attribution).size(10));
    }
    body
}

fn footer(state: &State, placement: Placement) -> Column<'_, Message> {
    let reported = reported(state);
    let mut commands = row![].spacing(3).align_y(Alignment::Center);
    if reported {
        commands = commands.push(
            named_command(
                "nmr.method",
                "NMR prediction method, source, conditions and limitations",
                if state.details {
                    "▾ Details"
                } else {
                    "▸ Details"
                },
                Action::ToggleDetails,
                true,
            )
            .expanded(state.details)
            .value(panel_description(state))
            .padding([5, 0]),
        );
    }
    commands = commands
        .push(Space::new().width(Length::Fill))
        .push(named_command(
            "nmr.copy",
            "Copy predicted NMR shifts with attribution",
            "Copy",
            Action::Copy,
            state.job.result.is_some(),
        ))
        .push(named_command(
            "nmr.export",
            "Export predicted NMR shifts with attribution as TSV",
            "Export…",
            Action::Export,
            state.job.result.is_some(),
        ));
    if placement == Placement::Floating {
        commands = commands.push(text("◢").size(12).style(muted_text));
    }
    let mut footer = column![].spacing(5);
    if reported {
        // Proton shifts are medians per parent atom's H group, not averages.
        let grouping = if state.job.nucleus == Nucleus::H1 {
            " · grouped per atom"
        } else {
            ""
        };
        let conditions = format!(
            "{}{grouping} · {}",
            state.job.nucleus.label(),
            engine::SOLVENT_LABEL
        );
        footer = footer.push(text(conditions).size(10).style(muted_text));
    }
    footer.push(commands)
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

/// The accessible name of a result row or plot marker, built from the same
/// site label as the table (`C #1 · 3H`).
pub(super) fn site_description(nucleus: Nucleus, row: &engine::Row, label: &str) -> String {
    let prediction = row.statistics.as_ref().map_or_else(
        || {
            format!(
                "Not predicted: {}",
                row.limitation
                    .as_deref()
                    .unwrap_or(engine::NO_REFERENCE_DATA)
            )
        },
        |s| {
            let mut description = format!(
                "Predicted {:.3} ppm; shell {}; {} references; SD {:.3} ppm",
                s.median,
                row.radius
                    .map_or_else(|| "—".into(), |radius| radius.to_string()),
                s.support,
                s.standard_deviation
            );
            if let Some(note) = &row.limitation {
                description.push_str(". ");
                description.push_str(note);
            }
            description
        },
    );
    format!("Select {} NMR row {label}. {prediction}", nucleus.label())
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

pub(super) fn panel_description(state: &State) -> String {
    let status = if state.job.pending.is_some() {
        format!("Calculating {} shifts", state.job.nucleus.label())
    } else if let Some(notice) = &state.job.notice {
        notice.clone()
    } else if let Some(report) = &state.job.result {
        super::summary(
            report.nucleus,
            state
                .job
                .presentation
                .as_ref()
                .filter(|presentation| presentation.matches(report))
                .map_or_else(
                    || report.supported_count(),
                    |presentation| presentation.supported_count,
                ),
            report.rows.len(),
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
