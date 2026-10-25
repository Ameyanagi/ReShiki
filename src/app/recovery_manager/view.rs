use super::*;
use crate::app::{
    help::modal,
    workspace::{control, hover_hint, muted_text},
};
use iced::widget::{Space, column, container, row, scrollable, text, tooltip};
use iced::{Alignment, Border, Element, Length};

fn action(
    label: &'static str,
    id: &'static str,
    action: Action,
    primary: bool,
    enabled: bool,
) -> Element<'static, Message> {
    reshiki::accessibility::button(id, label, text(label).size(12))
        .on_press_maybe(enabled.then_some(Message::Recovery(action)))
        .padding([6, 10])
        .style(control(primary))
        .into()
}

impl App {
    pub(in crate::app) fn recovery_banner(&self) -> Element<'_, Message> {
        let count = self.recovered.len();
        let newest = self.recovered.first();
        let name = newest.map(title).unwrap_or_else(|| "Untitled".into());
        let more = if count > 1 {
            format!(" +{}", count - 1)
        } else {
            String::new()
        };
        let summary = format!(
            "{count} {} · {}",
            plural(count, "draft", "drafts"),
            newest
                .map(|candidate| age(candidate.snapshot.saved_at, now()))
                .unwrap_or_default()
        );
        let mut short: String = name.chars().take(32).collect();
        if short != name || crate::app::workspace::text_width(&format!("{short}{more}"), 10.) > 112.
        {
            while !short.is_empty()
                && crate::app::workspace::text_width(&format!("{short}…{more}"), 10.) > 112.
            {
                short.pop();
            }
            short.push('…');
        }
        let details = self
            .recovered
            .iter()
            .map(|candidate| {
                format!(
                    "{} · {} · {} atoms",
                    title(candidate),
                    date(candidate.snapshot.saved_at),
                    candidate.snapshot.document.atoms.len()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        row![
            hover_hint(
                container(
                    column![
                        text(summary)
                            .size(10)
                            .wrapping(iced::widget::text::Wrapping::None),
                        text(format!("{short}{more}"))
                            .size(10)
                            .wrapping(iced::widget::text::Wrapping::None),
                    ]
                    .spacing(1)
                )
                .width(112)
                .clip(true),
                details,
                tooltip::Position::Top
            ),
            reshiki::accessibility::button(
                "recovery-review",
                "Review recovery drafts",
                text("Restore…").size(11)
            )
            .on_press_maybe(
                self.recovery_manager
                    .pending
                    .is_none()
                    .then_some(Message::Recovery(Action::Show))
            )
            .padding([3, 5])
            .style(control(true)),
            reshiki::accessibility::button(
                "recovery-discard",
                "Discard offered recovery drafts",
                text("Discard…").size(11)
            )
            .on_press_maybe(
                self.recovery_manager
                    .pending
                    .is_none()
                    .then_some(Message::Recovery(Action::Discard))
            )
            .padding([3, 5])
            .style(control(false)),
            hover_hint(
                reshiki::accessibility::button(
                    "recovery-dismiss",
                    "Dismiss recovery drafts until the next launch",
                    text("Dismiss").size(11)
                )
                .on_press(Message::DismissRecovery)
                .padding([3, 5])
                .style(control(false)),
                "Hide until the next launch; keep all draft files",
                tooltip::Position::Top
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    }

    pub(in crate::app) fn with_recovery_manager<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(dialog) = self.recovery_manager.dialog else {
            return iced::widget::stack![base].into();
        };
        let busy = self.recovery_manager.pending.is_some();
        let count = self.recovered.len();
        let content: Element<'a, Message> = match dialog {
            Dialog::Picker => {
                let mut drafts = column![].spacing(6);
                for (index, candidate) in self.recovered.iter().enumerate() {
                    let checked = self.recovery_manager.selected.contains(&candidate.path);
                    let check = container(text(if checked { "✓" } else { "" }).size(13))
                        .center_x(18)
                        .center_y(18)
                        .style(move |theme| container::Style {
                            border: Border {
                                color: crate::appearance::muted(theme),
                                width: 1.,
                                radius: 3.into(),
                            },
                            ..container::Style::default()
                        });
                    let detail = format!(
                        "{} · {} atoms · {}",
                        date(candidate.snapshot.saved_at),
                        candidate.snapshot.document.atoms.len(),
                        age(candidate.snapshot.saved_at, now())
                    );
                    let name = title(candidate);
                    let label = format!("{name} · {detail}");
                    let content = row![
                        check,
                        column![text(name).size(13), text(detail).size(11).style(muted_text)]
                            .spacing(3)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center);
                    drafts =
                        drafts.push(
                            reshiki::accessibility::button(
                                format!("recovery-draft-{index}"),
                                label,
                                content,
                            )
                            .checked(checked)
                            .on_press_maybe((!busy).then_some(Message::Recovery(Action::Toggle(
                                candidate.path.clone(),
                            ))))
                            .padding(10)
                            .width(Length::Fill)
                            .style(control(checked)),
                        );
                }
                let mut buttons = row![
                    action("Cancel", "recovery-cancel", Action::Close, false, !busy),
                    action(
                        "Discard all…",
                        "recovery-discard-all",
                        Action::Discard,
                        false,
                        count > 0 && !busy
                    ),
                    Space::new().width(Length::Fill),
                    action(
                        "Restore all",
                        "recovery-restore-all",
                        Action::RestoreAll,
                        false,
                        count > 0 && !busy
                    )
                ]
                .spacing(8);
                buttons = buttons.push(action(
                    "Restore selected",
                    "recovery-restore-selected",
                    Action::RestoreSelected,
                    true,
                    !self.recovery_manager.selected.is_empty() && !busy,
                ));
                column![
                    text(format!(
                        "Review {count} recovery {}",
                        plural(count, "draft", "drafts")
                    ))
                    .size(18),
                    text("Choose drawings to open as tabs. Save them to keep new copies.")
                        .size(12)
                        .style(muted_text),
                    self.recovery_manager
                        .failure
                        .as_ref()
                        .map(|failure| Element::from(text(failure).size(12)))
                        .unwrap_or_else(|| Space::new().into()),
                    if busy {
                        Element::from(text("Checking open ReShiki pages…").size(12))
                    } else {
                        Space::new().into()
                    },
                    scrollable(drafts).height(Length::Fill),
                    buttons,
                ]
                .spacing(12)
                .into()
            }
            Dialog::Discard => {
                let (question, consequence, label) = if cfg!(target_arch = "wasm32") {
                    (
                        format!(
                            "Permanently delete {count} recovery {}?",
                            plural(count, "draft", "drafts")
                        ),
                        "The browser has no Trash. These recovery copies cannot be restored after deletion. Saved drawing files are not changed.",
                        "Delete permanently",
                    )
                } else {
                    (
                        format!(
                            "Move {count} recovery {} to the Trash?",
                            plural(count, "draft", "drafts")
                        ),
                        "These are the recovery copies offered at startup. Saved drawing files and drafts owned by running instances are not changed.",
                        "Move to Trash",
                    )
                };
                column![
                    text(question).size(18),
                    text(consequence).size(13).style(muted_text),
                    if busy {
                        text("Discarding recovery drafts…").size(13).into()
                    } else {
                        Element::from(Space::new())
                    },
                    Space::new().height(Length::Fill),
                    row![
                        action(
                            "Cancel",
                            "recovery-discard-cancel",
                            Action::Close,
                            false,
                            !busy
                        ),
                        Space::new().width(Length::Fill),
                        action(
                            label,
                            "recovery-discard-confirm",
                            Action::ConfirmDiscard,
                            true,
                            !busy
                        )
                    ]
                    .spacing(8),
                ]
                .spacing(14)
                .into()
            }
        };
        modal(
            base,
            content,
            640.,
            if dialog == Dialog::Picker { 490. } else { 250. },
            Message::Recovery(Action::Close),
        )
    }
}
