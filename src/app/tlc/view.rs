//! TLC lanes and spots, with plate settings one click away.
use super::*;
use crate::app::workspace::{control, muted_text, section};
use iced::widget::{checkbox, column, row, text};
use iced::{Alignment, Element, Length};
#[derive(Debug, Clone, PartialEq, Eq)]
struct Choice {
    index: usize,
    label: String,
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
impl App {
    pub(in crate::app) fn tlc_panel(&self) -> Element<'_, Message> {
        let Some(g) = self.selected_tlc() else {
            return column![section("TLC plate"),text(if self.tool==crate::canvas::Tool::Graphic(reshiki::graphics::GraphicKind::TlcPlate){"Drag a frame to place a plate with three lanes. Then edit its lanes and spots here."}else{"Select one TLC plate to edit its lanes and spots."}).size(12).style(muted_text)].spacing(8).into();
        };
        let (Some(t), Some(k)) = (g.tlc.as_deref(), self.tab.tlc.target) else {
            return text("Select the TLC plate to show its controls")
                .size(12)
                .into();
        };
        let action = |id: &'static str, label: &'static str, a: Action| {
            reshiki::accessibility::button(id, label, text(label).size(12))
                .padding([5, 8])
                .style(control(false))
                .on_press(Message::Tlc(a))
        };
        let lanes: Vec<_> = t
            .lanes
            .iter()
            .enumerate()
            .map(|(i, l)| Choice {
                index: i,
                label: format!(
                    "Lane {} · {} {}",
                    i + 1,
                    l.spots.len(),
                    if l.spots.len() == 1 { "spot" } else { "spots" }
                ),
            })
            .collect();
        let selected = lanes.get(k.lane).cloned();
        let mut body = column![
            section("TLC plate"),
            text("Lane").size(11),
            crate::appearance::pick_list(lanes, selected, move |c| Message::Tlc(Action::Lane(
                k, c.index
            )))
            .text_size(12)
            .padding(5)
            .width(Length::Fill),
            row![
                action("tlc-add-lane", "Add lane", Action::AddLane(k)),
                reshiki::accessibility::button(
                    "tlc-delete-lane",
                    "Delete lane",
                    text("Delete lane").size(12)
                )
                .padding([5, 8])
                .style(control(false))
                .on_press_maybe(
                    t.lanes
                        .get(k.lane)
                        .map(|_| Message::Tlc(Action::DeleteLane(k)))
                )
            ]
            .spacing(8)
        ]
        .spacing(9);
        if let Some(lane) = t.lanes.get(k.lane) {
            let spots: Vec<_> = lane
                .spots
                .iter()
                .enumerate()
                .map(|(i, s)| Choice {
                    index: i,
                    label: format!("Spot {} · Rf {:.2}", i + 1, s.rf),
                })
                .collect();
            let selected = k.spot.and_then(|i| spots.get(i)).cloned();
            body = body
                .push(text("Spot").size(11))
                .push(
                    crate::appearance::pick_list(spots, selected, move |c| {
                        Message::Tlc(Action::Spot(k, c.index))
                    })
                    .text_size(12)
                    .padding(5)
                    .width(Length::Fill),
                )
                .push(
                    row![
                        action("tlc-add-spot", "Add spot", Action::AddSpot(k)),
                        reshiki::accessibility::button(
                            "tlc-delete-spot",
                            "Delete spot",
                            text("Delete spot").size(12)
                        )
                        .padding([5, 8])
                        .style(control(false))
                        .on_press_maybe(k.spot.map(|_| Message::Tlc(Action::DeleteSpot(k))))
                    ]
                    .spacing(8),
                );
            if let Some(s) = k.spot.and_then(|i| lane.spots.get(i)) {
                body = body
                    .push(self.tlc_input(k, "Rf", Field::Rf))
                    .push(
                        row![
                            self.tlc_input(k, "Width (pt)", Field::Width),
                            self.tlc_input(k, "Height (pt)", Field::Height)
                        ]
                        .spacing(8),
                    )
                    .push(
                        row![
                            checkbox(s.style.fill.is_some())
                                .label("Filled")
                                .on_toggle(move |v| Message::Tlc(Action::Toggle(
                                    k,
                                    Switch::Filled,
                                    v
                                )))
                                .text_size(12),
                            checkbox(s.show_rf)
                                .label("Show Rf")
                                .on_toggle(move |v| Message::Tlc(Action::Toggle(k, Switch::Rf, v)))
                                .text_size(12)
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .push(text("Spot color").size(11))
                    .push(self.tlc_swatches(k, PaintScope::Spot, s.style.stroke))
                    .push(self.tlc_input(k, "Spot hex color", Field::SpotColor));
                if let Some(label) = &s.label {
                    body = body.push(text(label.text.as_str()).size(11).style(muted_text));
                }
            } else {
                body = body.push(text("Add a spot to this lane.").size(11).style(muted_text));
            }
        }
        body.push(self.tlc_plate_settings(g, k))
            .push(
                text("Enter applies a number or hex color. Each change has one Undo.")
                    .size(11)
                    .style(muted_text),
            )
            .into()
    }
    fn tlc_input(&self, k: Target, label: &'static str, field: Field) -> Element<'_, Message> {
        column![
            text(label).size(11).width(Length::Fill),
            reshiki::accessibility::text_input(field.id(), label, "", self.tab.tlc.input(field))
                .style(crate::appearance::input_style)
                .on_input(move |v| Message::Tlc(Action::Draft(k, field, v)))
                .on_submit(Message::Tlc(Action::Apply(k, field)))
                .size(12)
                .padding(5)
                .width(Length::Fill)
        ]
        .spacing(3)
        .width(Length::Fill)
        .into()
    }
    fn tlc_plate_settings<'a>(&'a self, g: &'a Graphic, k: Target) -> Element<'a, Message> {
        let open = self.tab.tlc.plate_settings;
        let mut body = column![
            reshiki::accessibility::button(
                "tlc-plate-settings",
                "Show or hide plate settings",
                row![
                    text(if open { "▾" } else { "▸" }).size(12),
                    text("Plate settings").size(12)
                ]
                .spacing(6)
            )
            .padding([5, 0])
            .style(control(false))
            .on_press(Message::Tlc(Action::Details(k, !open)))
        ]
        .spacing(9);
        if open && let Some(t) = g.tlc.as_deref() {
            body = body
                .push(
                    row![
                        checkbox(t.show_borders)
                            .label("Border")
                            .on_toggle(move |v| Message::Tlc(Action::Toggle(k, Switch::Borders, v)))
                            .text_size(12),
                        checkbox(t.show_origin)
                            .label("Origin")
                            .on_toggle(move |v| Message::Tlc(Action::Toggle(k, Switch::Origin, v)))
                            .text_size(12)
                    ]
                    .spacing(10),
                )
                .push(
                    checkbox(t.show_solvent_front)
                        .label("Solvent front")
                        .on_toggle(move |v| Message::Tlc(Action::Toggle(k, Switch::Front, v)))
                        .text_size(12),
                )
                .push(
                    row![
                        self.tlc_input(k, "Origin from bottom (%)", Field::Origin),
                        self.tlc_input(k, "Front from top (%)", Field::Front)
                    ]
                    .spacing(8),
                )
                .push(self.tlc_input(k, "Line (pt)", Field::Line))
                .push(text("Plate color").size(11))
                .push(self.tlc_swatches(k, PaintScope::Plate, g.style.stroke))
                .push(self.tlc_input(k, "Plate hex color", Field::PlateColor));
        }
        body.into()
    }
    fn tlc_swatches(&self, k: Target, scope: PaintScope, current: Color) -> Element<'_, Message> {
        let p = reshiki::palette::Palette::of(&self.tab.doc);
        let hues = reshiki::palette::Hues::of(&self.tab.doc);
        let row = reshiki::palette::Row::Strong;
        crate::app::color_popover::paper(
            crate::app::color_popover::palette_row(
                &p,
                row,
                hues,
                20.,
                (
                    current == Color::Ink,
                    Some(Message::Tlc(Action::Paint(k, scope, Color::Ink))),
                ),
                |h| {
                    let c = Color::Palette(h, row);
                    (current == c, Some(Message::Tlc(Action::Paint(k, scope, c))))
                },
            ),
            self.tab.doc.canvas_theme,
        )
        .padding([6, 6])
        .into()
    }
}
