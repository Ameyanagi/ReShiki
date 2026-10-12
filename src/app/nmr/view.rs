//! NMR panel builders, placement wrappers and accessible descriptions.
use super::palette::{MAX_SIZE, MIN_SIZE, PANEL_PADDING, RESIZE_STEP};
use super::panel::site_id_prefix;
use super::presentation::{self, Presentation};
use super::style::{outlined, quiet, result_row, rule, segment, surface, table_surface};
use super::{Action, State, palette, panel, plot};
use crate::app::{App, Message, workspace::muted_text};
use crate::canvas::nmr::LabelMode;
use iced::widget::{
    Column, Container, Row, Space, checkbox, column, container, row, scrollable, sensor, stack,
    text,
};
use iced::{Alignment, Element, Length, Size};
use reshiki::document::Document;
use reshiki::nmr::{self as engine, Nucleus, Report};
use std::borrow::Cow;

const PLOT_DESCRIPTION: &str = concat!(
    "The plot marks chemical shifts only. ",
    "Stick heights do not represent intensity or integration. ",
    "No J couplings, multiplets or second-order simulation are predicted. ",
    "Nearby labels cycle their distinct atom-linked sites.",
);

/// Where the panel is shown: the movable palette over the drawing, or docked
/// in the inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Placement {
    Floating,
    Docked,
}

impl Placement {
    fn title_glyph(self) -> &'static str {
        match self {
            Self::Floating => "⠿",
            Self::Docked => "",
        }
    }

    fn mode_toggle<'a>(self) -> reshiki::accessibility::Button<'a, Message> {
        let (name, label, action) = match self {
            Self::Floating => ("Dock NMR in the inspector", "Dock", Action::Dock),
            Self::Docked => (
                "Undock NMR into a movable palette",
                "Undock",
                Action::Undock,
            ),
        };
        named_command("nmr.mode", name, label, action, true).style(segment(true))
    }

    fn wrap<'a>(
        self,
        surface: Container<'a, Message>,
        width: f32,
        collapsed: bool,
    ) -> Element<'a, Message> {
        match (self, collapsed) {
            (Self::Floating, false) => sensor(surface.width(width))
                .on_show(|size| Message::Nmr(Action::RenderedSize(size)))
                .on_resize(|size| Message::Nmr(Action::RenderedSize(size)))
                .into(),
            (Self::Docked, false) => {
                palette::guard(surface.width(Length::Fill).height(Length::Fill))
            }
            (Self::Floating, true) => surface.width(width).into(),
            (Self::Docked, true) => palette::guard(surface.width(Length::Fill)),
        }
    }
}

impl App {
    pub(in crate::app) fn nmr_inspector(&self) -> Element<'_, Message> {
        if self.tab.nmr.open && self.tab.nmr.dock.docked {
            return self.nmr_panel(Placement::Docked);
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

    pub(super) fn nmr_panel(&self, placement: Placement) -> Element<'_, Message> {
        let state = &self.tab.nmr;
        let title = title_bar(state, placement);
        if state.collapsed {
            let title = container(title).padding(PANEL_PADDING).style(surface);
            return placement.wrap(title, state.palette.width, true);
        }
        let header = column![title, nucleus_switch(state), label_controls(state)].spacing(8);
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
        }
        body = details_section(body, state, placement);
        let content = panel::view(
            header,
            scrollable(
                container(body).padding(iced::Padding::ZERO.bottom(panel::CONTENT_END_INSET)),
            )
            .id("nmr-results")
            .height(Length::Shrink),
            footer(state, placement),
            (placement == Placement::Floating && state.palette.resized)
                .then_some((state.palette.height - 2. * PANEL_PADDING).max(0.)),
            placement,
        );
        let panel = container(content).padding(PANEL_PADDING).style(surface);
        placement.wrap(panel, state.palette.width, false)
    }
}

fn title_bar(state: &State, placement: Placement) -> Row<'_, Message> {
    row![
        text(placement.title_glyph()).size(15).style(muted_text),
        text("NMR prediction").size(14),
        Space::new().width(Length::Fill),
        placement.mode_toggle(),
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
    nuclei
}

fn label_controls(state: &State) -> Element<'_, Message> {
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
        table = table.push(container(Space::new().height(1).width(Length::Fill)).style(rule));
        table = table.push(panel::table_row(
            index,
            site_row(state, report, item, index, presentation, document, selected),
        ));
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
        || Cow::Owned(presentation::row_label(report.nucleus, item)),
        |site| Cow::Borrowed(site.label.as_str()),
    );
    let shift = site.map_or_else(
        || Cow::Owned(presentation::row_shift(item)),
        |site| Cow::Borrowed(site.shift.as_str()),
    );
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
        {
            let mut description = site.map_or_else(
                || site_description(report.nucleus, item),
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
            description
        },
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
            .push(text(PLOT_DESCRIPTION).size(11))
            .push(text(source.method).size(11))
            .push(text(source.data_version).size(11))
            .push(text(source.conditions).size(11))
            .push(text(source.limitations).size(11))
            .push(text(source.attribution).size(10));
    }
    body
}

fn footer(state: &State, placement: Placement) -> Column<'_, Message> {
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
    if placement == Placement::Floating {
        commands = commands.push(text("◢").size(12).style(muted_text));
    }
    column![
        text(if state.job.nucleus == Nucleus::H1 {
            "Predicted H groups · unresolved · CDCl₃"
        } else {
            "Predicted shifts · CDCl₃"
        })
        .size(10)
        .style(muted_text),
        commands,
    ]
    .spacing(5)
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

pub(super) fn site_description(nucleus: Nucleus, row: &engine::Row) -> String {
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

pub(super) fn panel_description(state: &State) -> String {
    let status = if state.job.pending.is_some() {
        format!("Calculating {} shifts", state.job.nucleus.label())
    } else if let Some(notice) = &state.job.notice {
        notice.clone()
    } else if let Some(report) = &state.job.result {
        format!(
            "{} prediction: {} of {} atom-linked sites supported",
            report.nucleus.label(),
            state
                .job
                .presentation
                .as_ref()
                .filter(|presentation| presentation.matches(report))
                .map_or_else(
                    || report.supported_count(),
                    |presentation| presentation.supported_count
                ),
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
