//! The card's Conformers fields, the Conformers tab and its footer.
use super::{Action, Diversity, Format, MAX_REPRESENTATIVES, MAX_STARTS, Minimizer, Row};
use crate::app::workspace::{accessible_command, control, muted_text};
use crate::app::xtb::palette::{Action as PaletteAction, Shown};
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{Action as XtbAction, view::segment};
use crate::app::{App, Message};
use iced::widget::{Column, Space, column, container, row, text};
use iced::{Alignment, Element, Length};

fn send(action: Action) -> Message {
    Message::Xtb(XtbAction::Conformers(action))
}

fn palette(action: PaletteAction) -> Message {
    Message::Xtb(XtbAction::Palette(action))
}

fn label<'a>(content: &'a str) -> Element<'a, Message> {
    text(content).size(12).into()
}

/// A number field of the card.
fn number<'a>(
    id: &'static str,
    name: &'static str,
    value: &str,
    on: Option<fn(String) -> Action>,
) -> Element<'a, Message> {
    reshiki::accessibility::text_input(id, name, "", value)
        .on_input_maybe(on.map(|on| move |value| send(on(value))))
        .on_submit_maybe(on.map(|_| Message::Xtb(XtbAction::Run)))
        .size(12)
        .padding(6)
        .into()
}

impl App {
    /// The Conformers task's own fields, below Method: starts and their
    /// minimizer, diversity, energy window and the xTB re-ranking.
    pub(in crate::app::xtb) fn xtb_conformer_fields(
        &self,
        idle: bool,
        numbers: Option<&[u8]>,
    ) -> Element<'_, Message> {
        let settings = &self.xtb.conformers;
        let method = self.xtb.settings.method;
        let set = |action: Action| idle.then(|| send(action));
        let on = |f: fn(String) -> Action| idle.then_some(f);

        let minimizer: Element<'_, Message> = if idle {
            crate::appearance::pick_list(
                Minimizer::ALL.to_vec(),
                Some(settings.minimizer),
                |m: Minimizer| send(Action::Minimizer(m)),
            )
            .text_size(12)
            .width(Length::Fill)
            .into()
        } else {
            container(text(settings.minimizer.label()).size(12))
                .padding(6)
                .into()
        };
        let starts = row![
            column![
                label("Starting structures"),
                number(
                    "xtb-conformer-starts",
                    "Starting structures",
                    &settings.starts,
                    on(Action::Starts)
                ),
                line(format!("ETKDG, max {MAX_STARTS}"), Tone::Muted),
            ]
            .spacing(3)
            .width(Length::Fill),
            column![
                label("Minimize starts with"),
                minimizer,
                line("ETKDG makes the starts", Tone::Muted),
            ]
            .spacing(3)
            .width(Length::Fill),
        ]
        .spacing(8);

        let mut body = column![starts].spacing(6);
        let xtb_minimizes = settings.minimizer.xtb().is_some();
        match settings.minimizer {
            Minimizer::Gfn2 => {
                let starts = settings.starts.trim().parse::<u32>().unwrap_or(0);
                let estimate = numbers.map(|numbers| {
                    super::gfn2_estimate(numbers.len(), starts, self.xtb.settings.threads())
                });
                body = body.push(line(
                    match estimate {
                        Some(seconds) => format!(
                            "About {} for {starts} starts (estimate). Re-ranking is skipped: xTB minimizes every start.",
                            duration(seconds)
                        ),
                        None => "Re-ranking is skipped: xTB minimizes every start.".into(),
                    },
                    Tone::Warning,
                ));
            }
            Minimizer::GfnFf => {
                body = body.push(line(
                    "GFN-FF minimizes every start; re-ranking is skipped.",
                    Tone::Muted,
                ))
            }
            _ => {}
        }

        let diversity = row(Diversity::ALL.map(|d| {
            segment(
                format!("xtb-conformer-diversity-{}", d.label()),
                d.label(),
                settings.diversity == d,
                set(Action::Diversity(d)),
                None,
            )
        }))
        .spacing(4);
        body = body.push(
            column![
                label("Diversity"),
                diversity,
                line(
                    format!(
                        "Clusters structures within {} Å heavy-atom RMSD",
                        settings.diversity.angstrom()
                    ),
                    Tone::Muted
                ),
            ]
            .spacing(3),
        );
        body = body.push(
            column![
                label("Energy window (starts)"),
                row![
                    container(number(
                        "xtb-conformer-window",
                        "Energy window",
                        &settings.window,
                        on(Action::Window)
                    ))
                    .width(70),
                    line("kcal/mol above the lowest start", Tone::Muted),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ]
            .spacing(3),
        );

        // Re-ranking: skipped when xTB already minimized every start.
        let rerank = idle && !xtb_minimizes;
        let per_cluster = row(
            [(false, "lowest 1"), (true, "lowest 3")].map(|(three, name)| {
                segment(
                    format!("xtb-conformer-per-cluster-{}", if three { 3 } else { 1 }),
                    name,
                    settings.three == three,
                    rerank.then(|| send(Action::Three(three))),
                    None,
                )
            }),
        )
        .spacing(4);
        body = body.push(
            column![
                row![
                    text("Re-rank up to").size(12),
                    container(number(
                        "xtb-conformer-representatives",
                        "Representatives to re-rank",
                        &settings.representatives,
                        rerank.then_some(Action::Representatives as fn(String) -> Action)
                    ))
                    .width(52),
                    text(format!("cluster representatives with {}", method.label())).size(12),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .wrap(),
                row![text("Per cluster").size(12), per_cluster]
                    .spacing(8)
                    .align_y(Alignment::Center),
                line(
                    if xtb_minimizes {
                        format!(
                            "Skipped: {} minimizes every start.",
                            settings.minimizer.label()
                        )
                    } else {
                        format!("Optimized with xTB in total; raise up to {MAX_REPRESENTATIVES}")
                    },
                    Tone::Muted
                ),
            ]
            .spacing(4),
        );
        if let Err(reason) = settings.config(numbers.map(<[u8]>::len)) {
            body = body.push(line(reason, Tone::Error));
        }
        body.into()
    }

    /// The Conformers tab: the ranked table, then the shown conformer's
    /// actions and the overlay choice.
    pub(in crate::app::xtb) fn xtb_conformers_tab<'a>(
        &'a self,
        shown: &Shown<'a>,
        inspector: bool,
    ) -> Element<'a, Message> {
        let rows = self.xtb_conformer_rows(shown);
        let selected = self.xtb_step();
        // Rows preview only between runs: a run owns the model preview.
        let live = !shown.stale && self.tab.xtb.run.is_none();
        let shows = (live && selected.is_none())
            .then(|| self.xtb_model_step(shown))
            .flatten();
        let current = selected.or(shows);
        // The inspector is narrow: there the table keeps rank, ΔE, share and
        // the stereo check; the selected row's details follow below it.
        let field = super::start_field(&shown.stored.result).filter(|_| !inspector);
        let cell = |content: String, width: f32| text(content).size(12).width(width);
        let mut header = row![cell("#".into(), 24.), cell("ΔE xTB".into(), 64.)].spacing(8);
        if let Some(field) = field {
            header = header.push(cell(format!("ΔE {field}"), 84.));
        }
        header = header.push(cell("Boltzmann".into(), 72.));
        if !inspector {
            header = header
                .push(cell("RMSD".into(), 60.))
                .push(cell("Cluster".into(), 50.));
        }
        header = header.push(cell("Stereo".into(), 60.));
        let header = container(header).padding([2, 8]);
        let list = Column::with_children(rows.iter().enumerate().map(|(index, r)| {
            let active = current == Some(index);
            let mut cells = row![
                cell((index + 1).to_string(), 24.),
                cell(format!("{:.2}", r.delta_kcal), 64.),
            ]
            .spacing(8);
            if field.is_some() {
                cells = cells.push(cell(
                    r.start_kcal
                        .map_or_else(|| "—".into(), |e| format!("{e:.2}")),
                    84.,
                ));
            }
            cells = cells.push(cell(format!("{:.1} %", r.percent), 72.));
            if !inspector {
                cells = cells
                    .push(cell(
                        r.rmsd.map_or_else(|| "—".into(), |x| format!("{x:.2} Å")),
                        60.,
                    ))
                    .push(cell(
                        r.cluster.map_or_else(|| "—".into(), |c| c.to_string()),
                        50.,
                    ));
            }
            cells = cells.push(stereo(r));
            reshiki::accessibility::button(
                format!("xtb-conformer-{index}"),
                format!("Conformer {}, {:.2} kcal/mol", index + 1, r.delta_kcal),
                cells,
            )
            .padding([3, 8])
            .width(Length::Fill)
            .checked(active)
            .style(control(active))
            .on_press_maybe(live.then(|| palette(PaletteAction::Step(index))))
            .into()
        }))
        .spacing(1);
        let note = line(
            format!(
                "{} kept · ΔE in kcal/mol · Boltzmann at 298 K · heavy-atom RMSD to #1 after superposition",
                rows.len()
            ),
            Tone::Muted,
        );
        let table = column![header, list, note].spacing(4);
        let side = self.xtb_conformer_side(shown, &rows, current, live);
        if inspector {
            return column![table, side].spacing(10).into();
        }
        row![
            container(table).width(Length::FillPortion(3)),
            container(side).width(Length::FillPortion(2)),
        ]
        .spacing(20)
        .into()
    }

    fn xtb_conformer_side<'a>(
        &'a self,
        shown: &Shown<'a>,
        rows: &[Row],
        current: Option<usize>,
        live: bool,
    ) -> Element<'a, Message> {
        let palette_state = &self.tab.xtb.palette;
        let overlay = palette_state.ensemble.overlay;
        let choice = row(
            [(false, "Off"), (true, "All conformers")].map(|(on, name)| {
                segment(
                    format!("xtb-conformer-overlay-{}", if on { "all" } else { "off" }),
                    name,
                    overlay == on,
                    (live && rows.len() > 1).then(|| send(Action::Overlay(on))),
                    None,
                )
            }),
        )
        .spacing(4);
        let mut side = column![
            row![text("Overlay").size(12), container(choice).width(220)]
                .spacing(8)
                .align_y(Alignment::Center)
        ]
        .spacing(8);
        let selected = self.xtb_step();
        if palette_state.confirm.is_empty() || selected.is_none() {
            let usable = live
                && self.xtb_step_preview().is_some()
                && selected
                    .and_then(|index| shown.step_positions(index))
                    .is_some_and(|positions| {
                        !crate::app::xtb::model_shows(&self.tab.doc, shown.atoms, &positions)
                    });
            let mut actions = row![
                reshiki::accessibility::button(
                    "xtb-use-conformer",
                    "Use this conformer",
                    text("Use this conformer").size(12),
                )
                .padding([6, 12])
                .style(crate::appearance::primary)
                .on_press_maybe(usable.then(|| palette(PaletteAction::UseStep))),
            ]
            .spacing(8);
            if selected.is_some() {
                actions = actions.push(
                    accessible_command(
                        "xtb-conformer-back",
                        "Show the 3D model's geometry again",
                        "Back to model",
                    )
                    .on_press(palette(PaletteAction::StepBack)),
                );
            }
            side = side.push(actions);
            let about = match current.and_then(|i| Some((i, rows.get(i)?))) {
                Some((index, r)) => {
                    let mut text = format!("#{}", index + 1);
                    if let Some(cluster) = r.cluster {
                        text.push_str(&format!(
                            ": {cluster} start{} in this cluster",
                            if cluster == 1 { "" } else { "s" }
                        ));
                    }
                    if let Some(rmsd) = r.rmsd.filter(|_| index > 0) {
                        text.push_str(&format!(" · RMSD to #1 {rmsd:.2} Å"));
                    }
                    if r.stereo == Some(false) {
                        text.push_str(" · the stereo check finds a change");
                    }
                    text
                }
                None => String::new(),
            };
            side = side.push(line(about, Tone::Muted));
            side = side.push(line(
                if live {
                    "Click a row or use the list to preview a conformer on the 3D model. Esc returns."
                } else {
                    "Recalculate to preview conformers."
                },
                Tone::Muted,
            ));
        } else {
            for reason in &palette_state.confirm {
                side = side.push(text(reason.clone()).size(12));
            }
            side = side.push(
                row![
                    reshiki::accessibility::button(
                        "xtb-use-conformer-anyway",
                        "Use this conformer anyway",
                        text("Use anyway").size(12),
                    )
                    .padding([6, 12])
                    .style(crate::appearance::primary)
                    .on_press(palette(PaletteAction::UseStepAnyway)),
                    accessible_command("xtb-conformer-keep", "Keep the 3D model", "Keep model")
                        .on_press(palette(PaletteAction::KeepModel)),
                ]
                .spacing(8),
            );
        }
        side.into()
    }
}

/// "✓ OK", "changed" or "—" when the check cannot run.
fn stereo<'a>(row: &Row) -> Element<'a, Message> {
    let (content, tone) = match row.stereo {
        Some(true) => ("✓ OK", Tone::Good),
        Some(false) => ("changed", Tone::Warning),
        None => ("—", Tone::Muted),
    };
    container(line(content, tone)).width(60).into()
}

/// "40 s", "3 min".
fn duration(seconds: f64) -> String {
    if seconds < 90. {
        format!("{} s", seconds.max(1.).round())
    } else {
        format!("{} min", (seconds / 60.).round())
    }
}

/// The Conformers tab's footer: export every kept conformer.
pub(crate) fn footer<'a>() -> Element<'a, Message> {
    row![
        accessible_command(
            "xtb-export-conformers-xyz",
            "Export the conformers as multi-frame XYZ",
            "Export XYZ…"
        )
        .on_press(send(Action::Export(Format::Xyz))),
        accessible_command(
            "xtb-export-conformers-sdf",
            "Export the conformers as SDF with energies",
            "Export SDF…"
        )
        .on_press(send(Action::Export(Format::Sdf))),
        Space::new().width(8),
        text("Kept conformers are saved as compact frames (at most 32).")
            .size(11)
            .style(muted_text),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}
