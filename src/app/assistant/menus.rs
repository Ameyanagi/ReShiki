//! The composer's popup menus: attachments, edit mode, models and reasoning/service tier.

use super::{Action, Menu, action};
use crate::app::{App, Message};
use iced::widget::{Space, button, checkbox, column, rich_text, row, scrollable, text, tooltip};
use iced::{Element, Length};

impl App {
    pub(super) fn assistant_menu(&self, menu: Menu) -> Element<'_, Message> {
        let state = &self.assistant;
        let mut options = column![].spacing(4);
        match menu {
            Menu::Attachments => {
                options = options.push(text("Reference image").size(12)).push(
                    action("Choose image…", Action::OpenImage)
                        .width(Length::Fill)
                        .on_press_maybe(
                            (!state.busy && !state.reading_image)
                                .then_some(Message::Assistant(Action::OpenImage)),
                        ),
                );
                if reshiki::clipboard::available() {
                    options =
                        options.push(
                            action("Paste image", Action::Paste { image_only: true })
                                .width(Length::Fill)
                                .on_press_maybe((!state.busy && !state.reading_image).then_some(
                                    Message::Assistant(Action::Paste { image_only: true }),
                                )),
                        );
                }
                if state.source_image.is_some() {
                    options = options.push(
                        action("Remove attached image", Action::ClearImage)
                            .width(Length::Fill)
                            .on_press_maybe(
                                (!state.busy).then_some(Message::Assistant(Action::ClearImage)),
                            ),
                    );
                }
                options = options.push(
                    rich_text(super::super::shortcuts::spans(&format!(
                        "Paste with {} · Send with {}",
                        super::super::shortcuts::label(&Message::Paste).unwrap_or_default(),
                        super::super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, "Enter")
                    )))
                    .size(11),
                );
            }
            Menu::Edits => {
                options = options
                    .push(
                        checkbox(state.replace)
                            .label("Replace selection")
                            .text_size(12)
                            .on_toggle(|v| Message::Assistant(Action::Replace(v))),
                    )
                    .push(iced::widget::rule::horizontal(1));
                options = options.push(
                    text("Assistant edits")
                        .size(12)
                        .style(super::super::workspace::muted_text),
                );
                for (label, description, auto) in [
                    (
                        "Review edits",
                        "Preview each proposal, then apply or discard.",
                        false,
                    ),
                    (
                        "Accept all edits",
                        "Apply valid changes to the canvas automatically. Every change can be undone.",
                        true,
                    ),
                ] {
                    options = options.push(
                        button(
                            column![
                                text(label).size(13),
                                text(description)
                                    .size(11)
                                    .style(super::super::workspace::muted_text)
                            ]
                            .spacing(4),
                        )
                        .width(Length::Fill)
                        .padding([9, 10])
                        .style(super::super::workspace::control(
                            state.preferences.auto_apply == auto,
                        ))
                        .on_press(Message::Assistant(Action::AutoApply(auto))),
                    );
                }
            }
            Menu::Models => {
                options = options.push(
                    crate::appearance::text_input("Search models…", &state.search)
                        .on_input(|s| Message::Assistant(Action::Search(s)))
                        .size(13)
                        .padding(9),
                );
                options = options.push(
                    action("Default · GPT-6 Astra", Action::Model(None))
                        .width(Length::Fill)
                        .style(super::super::workspace::control(
                            state.preferences.model.is_none(),
                        )),
                );
                options = options.push(
                    text("Uses GPT-6 Astra when available; otherwise your account default")
                        .size(10)
                        .style(super::super::workspace::muted_text),
                );
                if let Some(account) = &state.account {
                    let search = state.search.to_lowercase();
                    let mut models: Vec<_> = account
                        .models
                        .iter()
                        .filter(|m| {
                            m.label.to_lowercase().contains(&search)
                                || m.id.to_lowercase().contains(&search)
                        })
                        .collect();
                    // Keep the resolved model visible first; retain catalog ordering otherwise.
                    models
                        .sort_by_key(|m| state.model().is_none_or(|selected| selected.id != m.id));
                    for model in models {
                        let selected = state.preferences.model.as_deref() == Some(&model.id);
                        options = options.push(
                            button(
                                column![
                                    row![
                                        text(&model.label).size(13),
                                        Space::new().width(Length::Fill),
                                        text(if selected { "✓" } else { "" }).size(13)
                                    ],
                                    text(&model.description)
                                        .size(10)
                                        .style(super::super::workspace::muted_text)
                                ]
                                .spacing(4),
                            )
                            .width(Length::Fill)
                            .padding([9, 10])
                            .style(super::super::workspace::control(selected))
                            .on_press(Message::Assistant(Action::Model(Some(model.id.clone())))),
                        );
                    }
                } else {
                    options = options.push(text("Connect to load your available models.").size(12));
                }
            }
            Menu::Effort => {
                options = options.push(
                    text("Reasoning")
                        .size(12)
                        .style(super::super::workspace::muted_text),
                );
                if let Some(model) = state.model() {
                    for effort in &model.efforts {
                        let label = if effort.id == model.initial_effort() {
                            format!("{}  ·  Default", effort.label())
                        } else {
                            effort.label().into()
                        };
                        options = options.push(super::super::workspace::hover_hint(
                            action(label, Action::Effort(effort.id.clone()))
                                .width(Length::Fill)
                                .style(super::super::workspace::control(
                                    state.preferences.effort(model) == effort.id,
                                )),
                            effort.description.clone(),
                            tooltip::Position::Left,
                        ));
                    }
                    options = options.push(iced::widget::rule::horizontal(1)).push(
                        text("Service tier")
                            .size(12)
                            .style(super::super::workspace::muted_text),
                    );
                    options = options.push(
                        action("Standard · Default", Action::Tier("default".into()))
                            .width(Length::Fill)
                            .style(super::super::workspace::control(
                                state.preferences.tier(model).is_none_or(|t| t == "default"),
                            )),
                    );
                    for tier in &model.tiers {
                        options = options.push(super::super::workspace::hover_hint(
                            action(tier.name.clone(), Action::Tier(tier.id.clone()))
                                .width(Length::Fill)
                                .style(super::super::workspace::control(
                                    state.preferences.tier(model) == Some(tier.id.as_str()),
                                )),
                            tier.description.clone(),
                            tooltip::Position::Left,
                        ));
                    }
                } else {
                    options = options.push(
                        text("Choose an available model to see its reasoning levels.").size(12),
                    );
                }
            }
        }
        scrollable(options)
            .height(if menu == Menu::Edits { 170 } else { 310 })
            .into()
    }
}
