use super::*;
use iced::widget::column;
use reshiki::arrows::{Head, HeadShape};
use reshiki::graphics::PathHeads;

#[derive(Debug, Clone, Copy)]
pub enum Field {
    Length,
    Width,
    Notch,
}
impl Field {
    pub(in crate::app) const ALL: [Self; 3] = [Self::Length, Self::Width, Self::Notch];

    pub(in crate::app) fn get(self, heads: &PathHeads) -> f32 {
        match self {
            Self::Length => heads.head_length_pt,
            Self::Width => heads.head_width_pt,
            Self::Notch => heads.head_notch,
        }
    }
    fn set(self, heads: &mut PathHeads, value: f32) {
        match self {
            Self::Length => heads.head_length_pt = value,
            Self::Width => heads.head_width_pt = value,
            Self::Notch => heads.head_notch = value,
        }
    }
}

impl App {
    pub(super) fn path_head_controls(&self, heads: PathHeads) -> Element<'_, Message> {
        let number = |label: &'static str, field: Field| {
            let value = self
                .tab
                .path_head_inputs
                .get(field as usize)
                .map(String::as_str)
                .unwrap_or("");
            row![
                text(label).size(11).width(Length::Fill),
                hover_hint(
                    crate::appearance::text_input("", value)
                        .size(12)
                        .padding(5)
                        .width(70)
                        .on_input(move |value| Message::Graphics(Action::HeadNumber(field, value)))
                        .on_submit(Message::Graphics(Action::ApplyHeadNumber(field))),
                    "Enter applies",
                    tooltip::Position::Top,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        };
        column![
            section("ARROWHEADS"),
            row![
                text("End").size(11).width(44),
                crate::appearance::pick_list(Head::ALL, Some(heads.head), |value| {
                    Message::Graphics(Action::Head(value))
                })
                .text_size(12)
                .padding(5)
                .width(Length::Fill),
            ]
            .align_y(Alignment::Center),
            row![
                text("Start").size(11).width(44),
                crate::appearance::pick_list(Head::ALL, Some(heads.tail), |value| {
                    Message::Graphics(Action::Tail(value))
                })
                .text_size(12)
                .padding(5)
                .width(Length::Fill),
            ]
            .align_y(Alignment::Center),
            text("Arrowhead shape").size(11).style(muted_text),
            crate::appearance::pick_list(HeadShape::ALL, Some(heads.shape), |value| {
                Message::Graphics(Action::HeadShape(value))
            })
            .text_size(12)
            .padding(6)
            .width(Length::Fill),
            number("Head length (pt)", Field::Length),
            number("Head half-width (pt)", Field::Width),
            number("Head notch (0–0.9)", Field::Notch),
        ]
        .spacing(8)
        .into()
    }

    pub(super) fn path_head_action(&mut self, action: Action) {
        if let Action::HeadNumber(field, value) = action {
            if let Some(input) = self.tab.path_head_inputs.get_mut(field as usize) {
                *input = value;
            }
            return;
        }
        let change = |heads: &mut PathHeads| -> Result<(), String> {
            match action {
                Action::Head(value) => heads.head = value,
                Action::Tail(value) => heads.tail = value,
                Action::HeadShape(value) => heads.shape = value,
                Action::ApplyHeadNumber(field) => field.set(
                    heads,
                    self.tab
                        .path_head_inputs
                        .get(field as usize)
                        .ok_or("Unknown path arrowhead property")?
                        .parse::<f32>()
                        .map_err(|_| "Type a number, then press Enter")?,
                ),
                _ => {}
            }
            heads.validate()
        };
        let mut next = self.tab.doc.clone();
        for graphic in next
            .graphics
            .iter_mut()
            .filter(|g| self.tab.selected.contains(&g.id) && is_open_path(g))
        {
            let mut heads = graphic.heads.unwrap_or_default();
            if let Err(error) = change(&mut heads) {
                self.status = error;
                self.error = true;
                return;
            }
            // Shape and sizes stay set while both ends are off.
            graphic.heads = Some(heads);
        }
        let before = std::mem::replace(&mut self.tab.doc, next);
        self.changed(before);
        self.sync_graphics();
        self.error = false;
        self.status = "Path arrowheads updated".into();
    }
}
