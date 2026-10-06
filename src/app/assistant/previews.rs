//! Draft preview canvases and the panel-target move and arrow-length controls.

use super::{Action, action};
use crate::app::{App, Message};
use crate::canvas::layered::canvas;
use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length};
use reshiki::{assistant, document::Document};

impl App {
    pub(super) fn assistant_preview_controls<'a>(
        &'a self,
        doc: &'a Document,
    ) -> Element<'a, Message> {
        let targets = assistant::review::targets(doc);
        let labels: Vec<_> = std::iter::once("Overview".to_string())
            .chain(targets.iter().map(|t| t.label()))
            .collect();
        let selected = self
            .assistant
            .preview_target
            .as_ref()
            .filter(|s| labels.contains(s));
        let mut controls = column![
            crate::appearance::pick_list(
                labels.clone(),
                Some(selected.cloned().unwrap_or_else(|| "Overview".into())),
                |s| Message::Assistant(Action::PreviewTarget(s))
            )
            .placeholder("Inspect or edit a panel…")
            .text_size(11)
            .width(Length::Fill)
        ]
        .spacing(6);
        if let Some(target) =
            selected.and_then(|label| targets.iter().find(|t| t.label() == *label))
        {
            let moving = |label, dx_pt, dy_pt| {
                action(
                    label,
                    Action::PreviewEdit(assistant::review::Edit::Move {
                        target: target.name.clone(),
                        dx_pt,
                        dy_pt,
                    }),
                )
                .padding([4, 8])
            };
            let mut buttons = row![
                text("Move").size(11),
                moving("←", -6., 0.),
                moving("→", 6., 0.),
                moving("↑", 0., -6.),
                moving("↓", 0., 6.)
            ]
            .spacing(3)
            .align_y(Alignment::Center);
            if target.kind == "arrow" {
                let length = doc
                    .arrows
                    .iter()
                    .find(|a| target.ids.contains(&a.id))
                    .map(|a| a.start.distance(a.end) * reshiki::style::DEFAULT.points_per_world())
                    .unwrap_or(40.);
                buttons = buttons.push(
                    action(
                        "Shorten",
                        Action::PreviewEdit(assistant::review::Edit::ArrowLength {
                            target: target.name.clone(),
                            length_pt: (length - 6.).max(12.),
                        }),
                    )
                    .padding([4, 6]),
                );
            }
            controls = controls.push(buttons);
        }
        controls.into()
    }
    pub(super) fn assistant_preview_canvas<'a>(
        &'a self,
        doc: &'a Document,
    ) -> Element<'a, Message> {
        if let Some(target) = self.assistant.preview_target.as_ref().and_then(|label| {
            assistant::review::targets(doc)
                .into_iter()
                .find(|t| t.label() == *label)
        }) {
            let ids = if target.kind == "panel" {
                target
                    .name
                    .strip_prefix("reaction:")
                    .and_then(|s| s.parse::<usize>().ok())
                    .and_then(|i| doc.reactions.get(i))
                    .map(|r| r.ids())
                    .unwrap_or(target.ids)
            } else {
                target.ids
            };
            return canvas(crate::canvas::OwnedDrawingPreview(
                assistant::review::fragment(doc, &ids),
            ))
            .width(Length::Fill)
            .height(200)
            .into();
        }
        canvas(crate::canvas::DrawingPreview(doc))
            .width(Length::Fill)
            .height(200)
            .into()
    }
}
