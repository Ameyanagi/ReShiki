//! The card's Conformers fields, the Conformers tab and its footer.
use super::{Action, Diversity, Format, MAX_REPRESENTATIVES, MAX_STARTS, Minimizer, Row};
use crate::app::workspace::{accessible_command, control, hover_hint, muted_text};
use crate::app::xtb::palette::{Action as PaletteAction, Shown};
use crate::app::xtb::tone::{Tone, line, toned_text};
use crate::app::xtb::{Action as XtbAction, view::segment};
use crate::app::{App, Message};
use iced::widget::{Column, Space, column, container, row, text, tooltip};
use iced::{Alignment, Element, Length};
use reshiki::qcdata::{CalcResult, Settings};
use reshiki::science::{self, TetrahedralStereo};

const UNASSIGNED_CONFIGURATION: &str = "configuration could not be assigned";
const FOOTNOTE: &str = "Pop. = Boltzmann population of the kept conformers at 298 K · RMSD: heavy atoms after superposition";

pub(super) fn headers(result: &CalcResult, inspector: bool) -> Vec<(String, Length)> {
    let (delta, pop) = if inspector {
        (("ΔE (kcal/mol)", 90.), ("Pop.", 60.))
    } else {
        let label = match result.settings {
            Settings::Xtb(_) => "ΔE xTB (kcal/mol)",
            Settings::Imported { .. } => "ΔE (kcal/mol)",
        };
        ((label, 114.), ("Pop. 298 K", 68.))
    };
    let mut columns = vec![
        ("#".into(), Length::Fixed(24.)),
        (delta.0.into(), Length::Fixed(delta.1)),
    ];
    if !inspector && let Some(field) = super::start_field(result) {
        columns.push((format!("ΔE {field} (kcal/mol)"), Length::Fixed(144.)));
    }
    columns.push((pop.0.into(), Length::Fixed(pop.1)));
    if !inspector {
        columns.extend([
            ("RMSD to #1 (Å)".into(), Length::Fixed(100.)),
            ("Starts".into(), Length::Fixed(42.)),
        ]);
    }
    columns.push(("Stereo".into(), Length::Fill));
    columns
}

pub(super) fn search_funnel(result: &CalcResult) -> Option<String> {
    let Settings::Xtb(settings) = &result.settings else {
        return None;
    };
    let counts = result.run.as_ref()?.search.as_ref()?;
    let method = settings.method.label();
    let minimizer = super::start_field(result).unwrap_or(method);
    let starts = if counts.made == counts.requested {
        format!("{} starts", counts.made)
    } else {
        format!("{} of {} starts made", counts.made, counts.requested)
    };
    let minimized = if counts.used_unconverged_starts {
        format!("{} unconverged with {minimizer}", counts.minimized)
    } else {
        format!("{} minimized with {minimizer}", counts.minimized)
    };
    let clusters = if counts.clusters == 1 {
        "cluster"
    } else {
        "clusters"
    };
    let mut summary = format!(
        "{starts} · {minimized} · {} distinct · {} {clusters}",
        counts.distinct, counts.clusters
    );
    if counts.rerank_planned > 0 {
        summary.push_str(&format!(" · {method} re-ranked {}", counts.reranked));
        if counts.reranked < counts.rerank_planned {
            summary.push_str(&format!(" of {}", counts.rerank_planned));
        }
    }
    let kept = result
        .trajectory
        .as_ref()
        .map_or(counts.kept, |path| path.steps());
    summary.push_str(&format!(" · {kept} kept"));
    Some(summary)
}

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
        target: Option<&[u64]>,
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

        let mut body = column![].spacing(6);
        if let Some((info, choices)) =
            target.and_then(|ids| self.xtb_conformer_stereo(ids, numbers.is_some()))
        {
            let choice = row(choices.into_iter().enumerate().map(|(index, name)| {
                let invert = index == 1;
                segment(
                    format!(
                        "xtb-conformer-stereo-{}",
                        if invert { "inverted" } else { "model" }
                    ),
                    name,
                    settings.invert_undefined_stereo == invert,
                    set(Action::InvertUndefinedStereo(invert)),
                    None,
                )
            }))
            .spacing(4);
            body = body.push(line(info, Tone::Muted)).push(choice);
        }
        body = body.push(starts);
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
        let table = self.xtb_conformer_table(shown);
        let rows = &table.rows;
        let selected = self.xtb_step();
        // Rows preview only between runs: a run owns the model preview.
        let live = !shown.stale && self.tab.xtb.run.is_none();
        let shows = (live && selected.is_none())
            .then(|| self.xtb_model_step(shown))
            .flatten();
        let current = selected.or(shows).and_then(|step| table.rank(step));
        // The inspector is narrow: there the table keeps rank, ΔE, share and
        // the stereo check; the selected row's details follow below it.
        let result = &shown.stored.result;
        let field = super::start_field(result).filter(|_| !inspector);
        let headers = headers(result, inspector);
        let cell = |content: String, width: Length| {
            text(content)
                .size(12)
                .width(width)
                .wrapping(text::Wrapping::None)
        };
        let header = row(headers
            .iter()
            .map(|(label, width)| cell(label.clone(), *width).into()))
        .spacing(8);
        let header = container(header).padding([2, 8]);
        let list = Column::with_children(rows.iter().enumerate().map(|(index, r)| {
            let active = current == Some(index);
            let (stereo, tone) = stereo_text(r);
            let values = [
                Some((index + 1).to_string()),
                Some(format!("{:.2}", r.delta_kcal)),
                field.map(|_| {
                    r.start_kcal
                        .map_or_else(|| "—".into(), |e| format!("{e:.2}"))
                }),
                Some(format!("{:.1} %", r.percent)),
                (!inspector).then(|| r.rmsd.map_or_else(|| "—".into(), |x| format!("{x:.2}"))),
                (!inspector).then(|| r.cluster.map_or_else(|| "—".into(), |c| c.to_string())),
            ];
            let mut name = format!("Conformer {}, {:.2} kcal/mol", index + 1, r.delta_kcal);
            if stereo != "—" {
                name.push_str(&format!(" · Stereo {stereo}"));
            }
            let unassigned = stereo.contains('?');
            let stereo_cell = toned_text(stereo, tone)
                .width(Length::Fill)
                .wrapping(text::Wrapping::WordOrGlyph);
            let stereo_cell: Element<'_, Message> = if unassigned {
                name.push_str(&format!(" · {UNASSIGNED_CONFIGURATION}"));
                hover_hint(
                    stereo_cell,
                    UNASSIGNED_CONFIGURATION,
                    tooltip::Position::Top,
                )
                .into()
            } else {
                stereo_cell.into()
            };
            let cells = row(values
                .into_iter()
                .flatten()
                .zip(&headers)
                .map(|(value, (_, width))| cell(value, *width).into()))
            .push(stereo_cell)
            .spacing(8);
            reshiki::accessibility::button(format!("xtb-conformer-{}", r.step), name, cells)
                .padding([3, 8])
                .width(Length::Fill)
                .checked(active)
                .style(control(active))
                .on_press_maybe(live.then(|| palette(PaletteAction::Step(r.step))))
                .into()
        }))
        .spacing(1);
        // The same energy diagram as the import sheet: levels with their
        // Boltzmann shares; a click shows that conformer.
        let plot = iced::widget::canvas(crate::app::energy_plot::EnergyPlot {
            delta_kcal: rows.iter().map(|r| r.delta_kcal).collect(),
            layout: crate::app::energy_plot::Layout::Levels,
            selected: current,
            notes: rows.iter().map(|r| format!("{:.1}%", r.percent)).collect(),
            select: live
                .then_some((|step| palette(PaletteAction::Step(step))) as fn(usize) -> Message),
            steps: rows.iter().map(|r| r.step).collect(),
        })
        .width(Length::Fill)
        .height(84);
        let mut body = column![header, list].spacing(4);
        if let Some(funnel) = search_funnel(result) {
            body = body.push(line(funnel, Tone::Muted));
        }
        body = body.push(line(FOOTNOTE, Tone::Muted));
        // Which stereoisomer the rows are leads the side column, in view
        // without scrolling the table.
        let mut notes = column![].spacing(4);
        if let Some(stereo_note) = &table.stereo_note {
            notes = notes.push(line(stereo_note.clone(), Tone::Warning));
        }
        let pairs = rows.iter().filter(|r| r.mirror_step.is_some()).count();
        if pairs > 0 {
            let noun = if pairs == 1 { "pair" } else { "pairs" };
            notes = notes.push(line(
                format!("{pairs} mirror {noun} merged · populations recomputed"),
                Tone::Muted,
            ));
        }
        let side = self.xtb_conformer_side(shown, rows, current, live);
        if inspector {
            return column![notes, body, plot, side].spacing(10).into();
        }
        row![
            container(body).width(720),
            container(column![notes, side].spacing(8)).width(320),
            container(plot).width(240),
        ]
        .spacing(20)
        .wrap()
        .into()
    }

    pub(super) fn xtb_conformer_stereo_note(
        &self,
        shown: &Shown<'_>,
        rows: &[Row],
    ) -> Option<String> {
        let names = |atom| self.xtb_atom_name(shown.atoms, atom).0;
        let input = if matches!(
            shown.stored.result.settings,
            reshiki::qcdata::Settings::Xtb(_)
        ) && shown.stored.result.input_frame.is_some()
            && rows.first().is_some_and(|r| !r.undefined().is_empty())
        {
            let ids: Vec<_> = shown.atoms.iter().flatten().copied().collect();
            science::tetrahedral_stereo(&self.tab.doc, &ids, Some((shown.atoms, shown.input))).ok()
        } else {
            None
        };
        stereo_note(rows, input.as_ref(), names)
    }

    fn xtb_conformer_side<'a>(
        &'a self,
        shown: &Shown<'a>,
        rows: &[Row],
        current: Option<usize>,
        live: bool,
    ) -> Element<'a, Message> {
        let palette_state = &self.tab.xtb.palette;
        // The overlay draws only on a live preview.
        let overlay = palette_state.ensemble.overlay && self.xtb_step_preview().is_some();
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
                    if let (Some(field), Some(start)) =
                        (super::start_field(&shown.stored.result), r.start_kcal)
                    {
                        text.push_str(&format!(" · ΔE {field} {start:.2} kcal/mol"));
                    }
                    if let Some(rmsd) = r.rmsd.filter(|_| index > 0) {
                        text.push_str(&format!(" · RMSD to #1 {rmsd:.2} Å"));
                    }
                    if !r.undefined().is_empty() {
                        text.push_str(&format!(" · Stereo {}", stereo_text(r).0));
                    } else if r.stereo == Some(false) {
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

/// The unspecified centres' configurations or the status of drawn stereo.
pub(super) fn stereo_text(row: &Row) -> (String, Tone) {
    let centres = row.undefined();
    if centres.is_empty() {
        return match row.stereo {
            Some(true) if row.tetrahedral.is_some() => ("as drawn".into(), Tone::Good),
            Some(false) => ("changed".into(), Tone::Warning),
            _ => ("—".into(), Tone::Muted),
        };
    }
    let configuration = |invert| {
        centres
            .iter()
            .map(|centre| {
                let cip = centre.cip.as_deref();
                if invert {
                    super::opposite_cip(cip)
                } else {
                    cip
                }
                .unwrap_or("?")
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut text = configuration(false);
    if row.mirror_step.is_some() {
        if matches!(text.as_str(), "S" | "R") {
            text = "S/R".into();
        } else {
            text.push('/');
            text.push_str(&configuration(true));
        }
    }
    if row.stereo == Some(false) {
        text.push_str(" · changed");
        (text, Tone::Warning)
    } else {
        (text, Tone::Muted)
    }
}

fn atom_list(names: &[String]) -> String {
    match names.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

fn wedge_text(names: &[String]) -> String {
    format!(
        "{} {} no wedge",
        atom_list(names),
        if names.len() == 1 { "has" } else { "have" }
    )
}

pub(super) fn unpinned_stereo_note(names: &[String]) -> String {
    let reason = if names.len() == 1 {
        "its 3D configuration could not be read. Both forms may appear."
    } else {
        "their 3D configurations could not be read. Both forms of each may appear."
    };
    format!("{} and {reason}", wedge_text(names))
}

pub(super) fn card_stereo(
    stereo: &TetrahedralStereo,
    has_model: bool,
    name: impl Fn(u64) -> String,
) -> Option<(String, [String; 2])> {
    if stereo.undefined.is_empty() {
        return None;
    }
    let names: Vec<_> = stereo.undefined.iter().map(|c| name(c.atom)).collect();
    let mut info = format!(
        "{}. The search keeps one configuration:",
        wedge_text(&names)
    );
    if !has_model {
        info = format!(
            "{}. The search keeps one configuration (the 3D model is made first):",
            wedge_text(&names)
        );
    } else {
        let unresolved: Vec<_> = stereo
            .undefined
            .iter()
            .filter(|centre| centre.winding.is_none())
            .map(|centre| name(centre.atom))
            .collect();
        if !unresolved.is_empty() {
            info = unpinned_stereo_note(&unresolved);
        }
    }
    let model = "As in the 3D model";
    if stereo.has_defined {
        return Some((
            info,
            [model.into(), format!("Inverted at {}", atom_list(&names))],
        ));
    }
    let cips = |invert| {
        stereo
            .undefined
            .iter()
            .map(|c| {
                let cip = c.cip.as_deref();
                if invert {
                    super::opposite_cip(cip)
                } else {
                    cip
                }
            })
            .collect::<Option<Vec<_>>>()
    };
    let choice = |label: &str, invert| {
        cips(invert).map_or_else(
            || label.into(),
            |cips| format!("{label} ({})", cips.join(",")),
        )
    };
    Some((info, [choice(model, false), choice("Mirror image", true)]))
}

pub(super) fn stereo_note(
    rows: &[Row],
    input: Option<&TetrahedralStereo>,
    name: impl Fn(u64) -> String,
) -> Option<String> {
    let first = rows.first()?.tetrahedral.as_ref()?;
    if first.undefined.is_empty() {
        return None;
    }
    let unresolved: Vec<_> = input
        .into_iter()
        .flat_map(|stereo| &stereo.undefined)
        .filter(|centre| centre.winding.is_none())
        .map(|centre| name(centre.atom))
        .collect();
    if !unresolved.is_empty() {
        return Some(unpinned_stereo_note(&unresolved));
    }
    let names: Vec<_> = first.undefined.iter().map(|c| name(c.atom)).collect();
    let wedge = wedge_text(&names);
    let handedness = |stereo: &TetrahedralStereo| {
        stereo
            .undefined
            .iter()
            .map(|c| Some((c.atom, c.winding?)))
            .collect::<Option<std::collections::BTreeMap<_, _>>>()
    };
    let first_hand = handedness(first);
    let single = first_hand.is_some()
        && rows.iter().all(|r| {
            r.mirror_step.is_none() && r.tetrahedral.as_ref().and_then(handedness) == first_hand
        });
    let Some(cips) = first
        .undefined
        .iter()
        .map(|c| c.cip.as_deref())
        .collect::<Option<Vec<_>>>()
        .filter(|_| single)
    else {
        return Some(format!(
            "{wedge}. The Stereo column lists the configurations."
        ));
    };
    if first.has_defined {
        let configurations: Vec<_> = names
            .iter()
            .zip(&cips)
            .map(|(name, cip)| format!("{name} ({cip})"))
            .collect();
        let choose = if names.len() == 1 {
            "draw a wedge to choose the other diastereomer."
        } else {
            "draw wedges to choose a different diastereomer."
        };
        return Some(format!(
            "{wedge}: searched {}; {choose}",
            atom_list(&configurations)
        ));
    }
    let model = match input.and_then(handedness) {
        Some(hand) if Some(&hand) == first_hand.as_ref() => ", as in the starting 3D model",
        Some(hand)
            if first_hand.as_ref().is_some_and(|first| {
                hand.len() == first.len()
                    && first
                        .iter()
                        .all(|(atom, tag)| hand.get(atom) == Some(&(3 - tag)))
            }) =>
        {
            ", the mirror image of the starting 3D model"
        }
        _ => "",
    };
    Some(format!(
        "{wedge}: searched the ({}) form{model}. Its mirror image has the same energies and populations.",
        cips.join(",")
    ))
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
pub(crate) fn footer<'a>(result: &CalcResult, wrapping: text::Wrapping) -> Element<'a, Message> {
    let count = result.trajectory.as_ref().map_or(0, |path| path.steps());
    let mut footer = row(
        [(Format::Sdf, "sdf", "SDF"), (Format::Xyz, "xyz", "XYZ")].map(|(format, id, name)| {
            let label = format!("Export all {count} as {name}…");
            accessible_command(format!("xtb-export-conformers-{id}"), label.clone(), label)
                .on_press_maybe((count > 0).then(|| send(Action::Export(format))))
                .into()
        }),
    )
    .spacing(10)
    .align_y(Alignment::Center);
    footer = footer.push(Space::new().width(8)).push(
        text("Kept conformers are saved as compact frames (at most 32).")
            .size(11)
            .style(muted_text)
            .wrapping(wrapping),
    );
    footer.into()
}
