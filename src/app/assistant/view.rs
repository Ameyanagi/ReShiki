//! The assistant panel: conversation, activity, draft and result cards, composer and footer.
use super::{Action, ChatMessage, Draft, Menu, action, card, surface};
use crate::app::{App, InspectorTab, Message, document_tab::TabId};
use crate::canvas::layered::canvas;
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, scrollable, stack, text,
    text_editor, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length};
use reshiki::assistant::{self, settings::effort_label};
use reshiki::document::Document;

impl App {
    pub(in crate::app) fn assistant_panel(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        if let Some(id) = state.tab.filter(|id| *id != self.tab.id) {
            return self.assistant_elsewhere(id);
        }
        let chat = self.assistant_chat();
        let header = panel_header();
        let footer = self.assistant_footer();
        let result_navigation = self.assistant_result_navigation();
        let base: Element<'_, Message> = container(
            column![
                header,
                scrollable(chat)
                    .id("assistant-chat")
                    .height(Length::Fill)
                    .on_scroll(|v| Message::Assistant(Action::ChatScrolled {
                        follow: v.content_bounds().height <= v.bounds().height
                            || v.relative_offset().y >= 0.97,
                        offset: v.absolute_offset().y,
                    })),
                result_navigation,
                footer
            ]
            .spacing(9),
        )
        .padding(10)
        .height(Length::Fill)
        .into();
        // Always keep the chat at the same location in the widget tree.
        let layers = stack![base];
        let Some(menu) = state.menu else {
            return layers.into();
        };
        let popup = container(self.assistant_menu(menu))
            .padding(10)
            .width(340)
            .style(|theme| {
                let mut style = card();
                style.shadow = crate::appearance::surface_shadow(iced::Shadow {
                    color: Color::from_rgba8(25, 40, 36, 0.16),
                    offset: iced::Vector::new(0., 4.),
                    blur_radius: 18.,
                });
                crate::appearance::container(theme, style)
            });
        layers
            .push(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .on_press(Message::Assistant(Action::Menu(None))),
            )
            .push(
                container(opaque(popup))
                    .height(Length::Fill)
                    .align_y(Alignment::End)
                    .padding(iced::Padding {
                        top: 8.,
                        right: 14.,
                        bottom: 110.,
                        left: 14.,
                    }),
            )
            .into()
    }
    fn assistant_elsewhere(&self, id: TabId) -> Element<'_, Message> {
        let name = self.strip().find(|tab| tab.id == id).map(|tab| tab.name());
        column![
            text(format!(
                "This conversation belongs to {}.",
                name.unwrap_or_else(|| "a closed drawing".into())
            ))
            .size(13),
            button(text("Go to drawing").size(12))
                .on_press(Message::Tabs(super::super::tabs::Action::Select(id))),
            action("New conversation here", Action::Reset),
        ]
        .spacing(12)
        .padding(12)
        .into()
    }
    fn assistant_chat(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        let mut chat = column![].spacing(12).padding([2, 2]).width(Length::Fill);
        if state.messages.is_empty() || state.connection != super::setup::Connection::Ready {
            chat = chat.push(self.assistant_setup_card());
        }
        if state.guided_example {
            chat = chat.push(self.assistant_example_card());
        }
        if state.messages.is_empty()
            && state.connection == super::setup::Connection::Ready
            && !state.guided_example
        {
            chat = chat.push(Space::new().height(20))
                .push(text("What would you like to draw?").size(19))
                .push(text("Molecules, reactions, or a starting point for your next scheme.").size(13).style(super::super::workspace::muted_text))
                .push(action("Draw a molecule", Action::Example("Draw caffeine.")))
                .push(action("Build a reaction", Action::Example("Draw the esterification of acetic acid with ethanol to ethyl acetate. Put H₂SO₄ and heat above the arrow.")))
                .push(text("Review each proposal before applying. Changes stay editable and can be undone.").size(11).style(super::super::workspace::muted_text));
        }
        for message in &state.messages {
            chat = chat.push(chat_message(message));
        }
        if !state.reply.is_empty() && state.busy {
            chat = chat.push(codex_reply(&state.reply));
        }
        if state.busy {
            chat = chat.push(self.assistant_activity_card());
        } else if state.draft.is_none()
            && let Some(doc) = &state.preview
        {
            chat = chat.push(self.assistant_retained_preview(doc));
        }
        if let Some(draft) = &state.draft {
            chat = chat.push(self.assistant_draft_card(draft));
        }
        if let Some((doc, report)) = &state.completed {
            chat = chat.push(self.assistant_completed_card(doc, report));
        }
        if !state.busy && state.draft.is_none() && !self.tab.doc.all_ids().is_empty() {
            chat = chat.push(action(
                if self.tab.selected.is_empty() {
                    "Improve drawing layout"
                } else {
                    "Improve selected layout"
                },
                Action::Improve,
            ));
        }
        chat.into()
    }
    fn assistant_activity_card(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        let seconds = state.started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let mut activity = column![
            row![
                text(&state.status).size(13).width(Length::Fill),
                action("Stop", Action::Stop)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text(format!(
                "Elapsed {seconds}s · You can keep drawing or close this panel"
            ))
            .size(11)
            .style(super::super::workspace::muted_text),
        ]
        .spacing(10);
        if !state.running_model.is_empty() {
            activity = activity.push(
                text(&state.running_model)
                    .size(10)
                    .style(super::super::workspace::muted_text),
            );
        }
        if !state.plan.is_empty() {
            activity = activity.push(text(&state.plan).size(12));
        }
        if let Some((completed, total)) = state.structures {
            activity =
                activity.push(text(format!("{completed} of {total} structures prepared")).size(11));
            if total > 0 {
                activity = activity.push(
                    iced::widget::progress_bar(0. ..=total as f32, completed as f32).girth(3),
                );
            }
        }
        if state
            .last_activity
            .is_some_and(|t| t.elapsed().as_secs() >= 8)
        {
            activity = activity.push(
                text("Generation is still running. Waiting for the next completed step…")
                    .size(11)
                    .style(super::super::workspace::muted_text),
            );
        }
        if let Some(doc) = &state.preview {
            activity = activity.push(self.assistant_preview_canvas(doc));
            activity = activity.push(
                text("Editable draft · Changes here stop generation and retain this preview.")
                    .size(11),
            );
            activity = activity.push(self.assistant_preview_controls(doc));
        }
        container(activity)
            .padding(12)
            .width(Length::Fill)
            .style(|theme| crate::appearance::container(theme, card()))
            .into()
    }
    fn assistant_retained_preview<'a>(&'a self, doc: &'a Document) -> Element<'a, Message> {
        container(column![
            text("Retained preview").size(12),
            self.assistant_preview_canvas(doc),
            text("Review did not finish. The completed preview is retained.").size(11)
        ])
        .padding(12)
        .style(|theme| crate::appearance::container(theme, card()))
        .into()
    }
    fn assistant_draft_card<'a>(&'a self, draft: &'a Draft) -> Element<'a, Message> {
        let state = &self.assistant;
        let current = draft.epoch == self.tab.file_epoch
            && (draft.replace.is_empty() || draft.revision == self.tab.revision);
        let mut proposal = column![
            text(if draft.review.can_auto_apply() {
                "Quality checked"
            } else {
                "Draft · Review needed"
            })
            .size(13),
            self.assistant_preview_canvas(&draft.fragment),
            text(if draft.replace.is_empty() {
                "Adds editable objects to your drawing"
            } else {
                "Replaces the targeted objects; keeps the rest of your drawing"
            })
            .size(11)
            .style(super::super::workspace::muted_text)
        ]
        .spacing(10);
        for change in &draft.review.changes {
            proposal = proposal.push(text(change).size(12));
        }
        proposal = proposal.push(text(&draft.review.summary).size(12));
        for issue in &draft.review.issues {
            proposal = proposal.push(
                text(format!("• {issue}"))
                    .size(11)
                    .style(crate::appearance::text_color(Color::from_rgb8(151, 86, 24))),
            );
        }
        proposal = proposal.push(self.assistant_preview_controls(&draft.fragment));
        if !current {
            proposal = proposal.push(
                text("Your drawing changed. Send a follow-up to refresh this proposal.").size(11),
            );
        }
        proposal = proposal.push(
            row![
                action("Apply", Action::Apply)
                    .on_press_maybe(
                        (current && !state.busy && self.tab.cleanup.is_none())
                            .then_some(Message::Assistant(Action::Apply))
                    )
                    .style(crate::appearance::primary),
                action("Improve layout", Action::Improve).on_press_maybe(
                    (current && !state.busy).then_some(Message::Assistant(Action::Improve))
                ),
                action("Discard", Action::Reject)
                    .on_press_maybe((!state.busy).then_some(Message::Assistant(Action::Reject)))
            ]
            .spacing(6),
        );
        container(proposal)
            .padding(12)
            .style(|theme| crate::appearance::container(theme, card()))
            .into()
    }
    fn assistant_completed_card<'a>(
        &'a self,
        doc: &'a Document,
        report: &'a assistant::review::Report,
    ) -> Element<'a, Message> {
        let state = &self.assistant;
        let mut result = column![
            text(format!("Applied · Elapsed {}s", state.elapsed)).size(12),
            canvas(crate::canvas::DrawingPreview(doc))
                .width(Length::Fill)
                .height(180)
        ]
        .spacing(8);
        for change in &report.changes {
            result = result.push(text(change).size(12).width(Length::Fill));
        }
        result = result.push(text(&report.summary).size(12).width(Length::Fill));
        for issue in &report.issues {
            result = result.push(text(format!("• {issue}")).size(11).width(Length::Fill));
        }
        container(result)
            .padding(12)
            .width(Length::Fill)
            .style(|theme| crate::appearance::container(theme, card()))
            .into()
    }
    fn assistant_footer(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        let connected = state.account.as_ref().is_some_and(|a| a.connected);
        let activity = self.assistant_status_line();
        let effort = state
            .model()
            .map(|m| effort_label(state.preferences.effort(m)))
            .unwrap_or("Reasoning");
        let composer = self.assistant_composer();
        let connection = super::super::workspace::hover_hint(
            action(
                if connected {
                    "● Codex"
                } else {
                    "Connect Codex"
                },
                Action::Connect,
            )
            .on_press_maybe((!state.busy).then_some(Message::Assistant(Action::Connect))),
            if connected {
                "Connected · Click to refresh models"
            } else {
                "Connect using your Codex sign-in"
            },
            tooltip::Position::Top,
        );
        column![
            activity,
            composer,
            self.assistant_transfer_note(),
            row![
                connection,
                Space::new().width(Length::Fill),
                action(format!("{effort} ⌄"), Action::Menu(Some(Menu::Effort))).padding([3, 4]),
                action(
                    if state.guided_example || state.requires_apply {
                        "Review example ⌄"
                    } else if state.preferences.auto_apply {
                        "Auto apply ⌄"
                    } else {
                        "Review ⌄"
                    },
                    Action::Menu(Some(Menu::Edits))
                )
                .padding([3, 4])
            ]
            .spacing(4)
            .align_y(Alignment::Center)
        ]
        .spacing(5)
        .into()
    }
    fn assistant_status_line(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        let mut activity = column![].spacing(6);
        if state.busy {
            let seconds = state.started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
            activity = activity.push(
                text(format!(
                    "{} · {seconds}s",
                    if state.connection == super::setup::Connection::Checking {
                        "Checking connection"
                    } else if state.checking > 0 {
                        "Checking draft"
                    } else if state.structures.is_some() {
                        "Preparing structures"
                    } else {
                        "Preparing scheme"
                    }
                ))
                .size(11)
                .style(super::super::workspace::muted_text),
            );
        }
        if state.error {
            activity = activity.push(
                text(&state.status)
                    .size(11)
                    .style(crate::appearance::text_color(Color::from_rgb8(175, 54, 54))),
            );
        }
        activity.into()
    }
    fn assistant_composer(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        let model_label = match state.model() {
            Some(model) => model.label.clone(),
            None => state
                .preferences
                .model
                .clone()
                .unwrap_or_else(|| "GPT-6 Astra".into()),
        };
        let editor = text_editor(&state.input)
            .id("assistant-input")
            .placeholder(if state.messages.is_empty() {
                "Describe a molecule, or paste an image…"
            } else {
                "Ask for changes or another drawing…"
            })
            .on_action(|a| Message::Assistant(Action::Input(a)))
            .key_binding(|key| {
                if !matches!(key.status, text_editor::Status::Focused { .. }) {
                    return text_editor::Binding::from_key_press(key);
                }
                if key.modifiers.command()
                    && key.key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter)
                {
                    Some(text_editor::Binding::Custom(Message::Assistant(
                        Action::Send,
                    )))
                } else if key.modifiers.command() && matches!(&key.key, iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                    Some(text_editor::Binding::Custom(Message::Assistant(Action::Paste { image_only:false })))
                } else {
                    text_editor::Binding::from_key_press(key)
                }
            })
            .size(13)
            .height(64)
            .padding(4)
            .style(|theme, _| text_editor::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                placeholder: crate::appearance::muted(theme),
                value: crate::appearance::themed(theme, Color::from_rgb8(37, 46, 48)),
                selection: crate::appearance::themed(theme, Color::from_rgb8(198, 223, 215)),
            });
        let mut input_row = row![editor].spacing(8).align_y(Alignment::Center);
        if let Some(source) = &state.source_image
            && let Some(handle) = source.handle(false)
        {
            input_row = input_row.push(super::super::workspace::hover_hint(
                button(
                    iced::widget::image(handle)
                        .width(46)
                        .height(46)
                        .content_fit(iced::ContentFit::Contain),
                )
                .padding(3)
                .style(super::super::workspace::control(false))
                .on_press(Message::Assistant(Action::Menu(Some(Menu::Attachments)))),
                format!(
                    "Attached image · {} × {} · Click for options",
                    source.width(),
                    source.height()
                ),
                tooltip::Position::Top,
            ));
        }
        let submit = if state.busy {
            action("■ Stop", Action::Stop)
        } else {
            action("↑ Send", Action::Send)
                .style(crate::appearance::primary)
                .on_press_maybe(
                    ((state.account.as_ref().is_some_and(|a| a.connected)
                        && (!state.input.text().trim().is_empty()
                            || state.source_image.is_some()))
                        && !state.reading_image
                        && self.tab.cleanup.is_none())
                    .then_some(Message::Assistant(Action::Send)),
                )
        };
        let attach = super::super::workspace::hover_hint(
            action("＋", Action::Menu(Some(Menu::Attachments))).padding([5, 8]),
            "Attach or paste an image",
            tooltip::Position::Top,
        );
        container(
            column![
                input_row,
                row![
                    attach,
                    action(format!("{model_label} ⌄"), Action::Menu(Some(Menu::Models)))
                        .padding([5, 3]),
                    Space::new().width(Length::Fill),
                    submit.padding([6, 10])
                ]
                .spacing(4)
                .align_y(Alignment::Center)
            ]
            .spacing(4),
        )
        .padding(8)
        .style(|theme| crate::appearance::container(theme, card()))
        .into()
    }
    fn assistant_result_navigation(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        if !state.follow_chat && (state.draft.is_some() || state.completed.is_some() || state.busy)
        {
            action(
                if state.busy {
                    "↓ Jump to activity"
                } else {
                    "↓ Jump to result"
                },
                Action::JumpToResult,
            )
            .width(Length::Fill)
            .into()
        } else {
            Space::new().height(0).into()
        }
    }
}
fn chat_message(message: &ChatMessage) -> Element<'_, Message> {
    let role = &message.role;
    if role == "You" {
        let mut content = column![text(&message.text).size(13).width(Length::Fill)].spacing(8);
        if let Some(source) = &message.image
            && let Some(handle) = source.handle(false)
        {
            content = content
                .push(
                    button(
                        iced::widget::image(handle)
                            .width(Length::Fill)
                            .height(140)
                            .content_fit(iced::ContentFit::Contain),
                    )
                    .padding(4)
                    .width(Length::Fill)
                    .style(super::super::workspace::control(false))
                    .on_press(Message::Assistant(Action::ViewImage(Some(source.clone())))),
                )
                .push(
                    text("Sent image · Click to enlarge")
                        .size(11)
                        .style(super::super::workspace::muted_text),
                );
        }
        container(content)
            .padding([10, 12])
            .width(Length::Fill)
            .style(|theme| {
                crate::appearance::container(theme, surface(Color::from_rgb8(234, 241, 239), 12.))
            })
            .into()
    } else if role == "ReShiki" {
        text(&message.text)
            .size(11)
            .style(super::super::workspace::muted_text)
            .into()
    } else {
        codex_reply(&message.text)
    }
}
// The "Codex" reply column shared by finished replies and the reply streaming in.
fn codex_reply(reply: &str) -> Element<'_, Message> {
    column![
        text("Codex")
            .size(11)
            .style(crate::appearance::text_color(Color::from_rgb8(
                17, 126, 108
            ))),
        text(reply).size(13).width(Length::Fill)
    ]
    .spacing(6)
    .into()
}
fn panel_header<'a>() -> Element<'a, Message> {
    row![
        super::super::workspace::hover_hint(
            button(text("‹").size(18))
                .padding([3, 8])
                .style(super::super::workspace::control(false))
                .on_press(Message::Inspector(InspectorTab::Properties)),
            "Back to inspector",
            tooltip::Position::Bottom
        ),
        text("Assistant").size(16),
        Space::new().width(Length::Fill),
        super::super::workspace::hover_hint(
            action("＋", Action::Reset),
            "New conversation",
            tooltip::Position::Bottom
        ),
        super::super::workspace::hover_hint(
            button(text("×").size(18))
                .padding([3, 8])
                .style(super::super::workspace::control(false))
                .on_press(Message::ToggleInspector),
            "Close assistant",
            tooltip::Position::Bottom
        )
    ]
    .align_y(Alignment::Center)
    .into()
}
