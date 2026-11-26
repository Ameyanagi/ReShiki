//! Properties for a placed IR figure, with explicit numeric drafts.
use super::*;
use crate::app::workspace::{
    accessible_checkbox, accessible_command, section, segment, segment_track,
};
use iced::widget::{column, container, row, text, toggler};
use iced::{Alignment, Element, Length};

impl App {
    pub(in crate::app) fn ir_figure_card(&self) -> Option<Element<'_, Message>> {
        let target = self.tab.ir_figure.target?;
        let (figure, _) = self.current_ir_figure(target)?;
        let mut body = column![
            section("IR spectrum"),
            choices(
                target,
                "Y axis",
                figure.y,
                [
                    (YAxis::Transmittance, "transmittance", "Transmittance"),
                    (YAxis::Absorbance, "absorbance", "Absorbance"),
                ],
                Edit::Y,
            ),
            column![
                text("Range (cm⁻¹)").size(11),
                row![
                    self.ir_figure_input(target, Field::From, "ir-figure-from", "From"),
                    self.ir_figure_input(target, Field::To, "ir-figure-to", "To"),
                ]
                .spacing(8),
            ]
            .spacing(3),
            accessible_checkbox(
                "ir-figure-minor-ticks",
                "Minor ticks",
                figure.minor_ticks,
                Some(Message::IrFigure(Action::Edit(
                    target,
                    Edit::MinorTicks(!figure.minor_ticks),
                ))),
            ),
            choices(
                target,
                "Frame",
                figure.frame,
                [
                    (Frame::Box, "box", "Box"),
                    (Frame::Axes, "axes", "Axes"),
                    (Frame::Baseline, "baseline", "Baseline"),
                ],
                Edit::Frame,
            ),
            choices(
                target,
                "Bands",
                figure.shape,
                [
                    (BandShape::Lorentzian, "lorentzian", "Lorentzian"),
                    (BandShape::Gaussian, "gaussian", "Gaussian"),
                ],
                Edit::Shape,
            ),
            row![
                self.ir_figure_input(target, Field::Fwhm, "ir-figure-fwhm", "FWHM (cm⁻¹)"),
                self.ir_figure_input(target, Field::Scale, "ir-figure-scale", "Scale"),
            ]
            .spacing(8),
            accessible_command("ir-figure-apply", "Apply IR numeric settings", "Apply")
                .padding([5, 8])
                .on_press(Message::IrFigure(Action::Apply(target))),
            row![
                text("Peak labels").size(11).width(Length::Fill),
                accessible_command("ir-figure-labels-less", "Fewer peak labels", "−")
                    .padding([3, 8])
                    .on_press_maybe((figure.labels > 0).then(|| {
                        Message::IrFigure(Action::Edit(target, Edit::Labels(figure.labels - 1)))
                    })),
                text(figure.labels.to_string()).size(12).width(20).center(),
                accessible_command("ir-figure-labels-more", "More peak labels", "+")
                    .padding([3, 8])
                    .on_press_maybe((figure.labels < IrFigure::MAX_LABELS).then(|| {
                        Message::IrFigure(Action::Edit(target, Edit::Labels(figure.labels + 1)))
                    })),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
            choices(
                target,
                "Label direction",
                figure.vertical_labels,
                [
                    (false, "horizontal", "Horizontal"),
                    (true, "vertical", "Vertical"),
                ],
                Edit::VerticalLabels,
            ),
        ]
        .spacing(9);
        for &peak in &self.tab.ir_figure.peaks {
            let visible = !figure.hidden_labels.contains(&peak);
            let label = format!("{peak} cm⁻¹");
            body = body.push(
                reshiki::accessibility::button(
                    format!("ir-figure-peak-{peak}"),
                    label.clone(),
                    toggler(visible)
                        .label(label)
                        .size(14)
                        .text_size(12)
                        .spacing(8)
                        .on_toggle(move |visible| {
                            Message::IrFigure(Action::Edit(target, Edit::Peak(peak, visible)))
                        }),
                )
                .checked(visible)
                .on_press(Message::IrFigure(Action::Edit(
                    target,
                    Edit::Peak(peak, !visible),
                )))
                .padding(0)
                .style(|theme, _| iced::widget::button::Style {
                    text_color: theme.palette().text,
                    ..Default::default()
                }),
            );
        }
        Some(
            body.push(accessible_checkbox(
                "ir-figure-method-line",
                "Method line",
                figure.method_line,
                Some(Message::IrFigure(Action::Edit(
                    target,
                    Edit::MethodLine(!figure.method_line),
                ))),
            ))
            .push(choices(
                target,
                "Style",
                figure.journal,
                [
                    (Journal::Acs, "acs", "ACS"),
                    (Journal::Rsc, "rsc", "RSC"),
                    (Journal::Wiley, "wiley", "Wiley"),
                ],
                Edit::Journal,
            ))
            .push(choices(
                target,
                "Size",
                figure.columns,
                [
                    (Columns::One, "one-column", "1 column"),
                    (Columns::Two, "two-columns", "2 columns"),
                ],
                Edit::Columns,
            ))
            .push(
                accessible_command(
                    "ir-figure-copy-caption",
                    "Copy the IR caption",
                    "Copy caption",
                )
                .padding([5, 8])
                .on_press(Message::IrFigure(Action::CopyCaption(target))),
            )
            .into(),
        )
    }

    fn ir_figure_input(
        &self,
        target: Target,
        field: Field,
        id: &'static str,
        label: &'static str,
    ) -> Element<'_, Message> {
        let mut body = column![
            text(label).size(11),
            reshiki::accessibility::text_input(
                id,
                match field {
                    Field::From => "From wavenumber (cm⁻¹)",
                    Field::To => "To wavenumber (cm⁻¹)",
                    Field::Fwhm => "FWHM (cm⁻¹)",
                    Field::Scale => "Frequency scale factor",
                },
                "",
                self.tab
                    .ir_figure
                    .fields
                    .get(field.index())
                    .map_or("", String::as_str),
            )
            .style(crate::appearance::input_style)
            .on_input(move |value| Message::IrFigure(Action::Draft(target, field, value)))
            .on_submit(Message::IrFigure(Action::Apply(target)))
            .size(12)
            .padding(5)
            .width(Length::Fill),
        ]
        .spacing(3)
        .width(Length::Fill);
        if let Some(Some(error)) = self.tab.ir_figure.errors.get(field.index()) {
            body = body.push(text(*error).size(11).style(text::danger));
        }
        body.into()
    }
}

fn choices<T: Copy + PartialEq + 'static, const N: usize>(
    target: Target,
    label: &'static str,
    selected: T,
    options: [(T, &'static str, &'static str); N],
    edit: fn(T) -> Edit,
) -> Element<'static, Message> {
    column![
        text(label).size(11),
        container(
            row(
                options.map(|(value, id, name)| -> Element<'static, Message> {
                    let active = selected == value;
                    reshiki::accessibility::button(
                        format!("ir-figure-{id}"),
                        name,
                        text(name).size(12).center().width(Length::Fill),
                    )
                    .padding([5, 6])
                    .checked(active)
                    .style(segment(active))
                    .width(Length::Fill)
                    .on_press(Message::IrFigure(Action::Edit(target, edit(value))))
                    .into()
                })
            )
            .spacing(2)
        )
        .padding(2)
        .width(Length::Fill)
        .style(segment_track),
    ]
    .spacing(3)
    .into()
}
