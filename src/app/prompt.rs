//! Questions the app asks in its own dialog: whether to save a drawing before
//! closing it, and notices to acknowledge. The desktop asks with the system's
//! dialogs; the browser's confirm() has only OK and Cancel, so the browser
//! build asks here instead.
use super::{
    App, Message,
    help::{Placement, modal},
    workspace::{control, muted_text},
};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{Space, column, row, stack, text};
use iced::{Alignment, Element, Length};

#[derive(Debug, Clone)]
#[cfg_attr(
    not(any(target_arch = "wasm32", test)),
    expect(
        dead_code,
        reason = "In-app prompts are constructed by the browser and desktop tests"
    )
)]
pub(super) enum Prompt {
    /// Save, Don’t Save or Cancel for the drawing with this name, before the
    /// pending action goes on.
    Save(String),
    Download {
        name: String,
        action: Download,
    },
    /// A notice with one OK button, which sends `then`.
    Notice {
        title: String,
        text: String,
        then: Box<Message>,
    },
}

#[derive(Debug, Clone)]
pub(super) enum Download {
    #[cfg(target_arch = "wasm32")]
    Drawing,
    Export {
        tab: super::document_tab::TabId,
        export: super::figure_export::Prepared,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Save, or OK for a notice; Enter.
    Save,
    DontSave,
    /// Cancel; Escape or a click outside the dialog. A notice takes it as OK.
    Cancel,
}

impl Prompt {
    /// The message that answers the question, as the system dialog's would.
    pub(super) fn answer(self, answer: Answer) -> Message {
        match self {
            Self::Save(_) => match answer {
                Answer::Save => Message::Save,
                Answer::DontSave => Message::Discard,
                Answer::Cancel => Message::Cancel,
            },
            Self::Notice { then, .. } => *then,
            Self::Download { name, action } => {
                let save = answer == Answer::Save && !name.trim().is_empty();
                match action {
                    #[cfg(target_arch = "wasm32")]
                    Download::Drawing => {
                        if save {
                            Message::SaveDownload(name)
                        } else {
                            Message::Cancel
                        }
                    }
                    Download::Export { tab, export } => Message::Tab(
                        tab,
                        Box::new(if save {
                            Message::SaveExport(name, export)
                        } else {
                            export.completed(Ok(None))
                        }),
                    ),
                }
            }
        }
    }
}

/// Enter answers with the default button and Escape cancels; other keys stay
/// in the dialog.
pub(super) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    if !modifiers.is_empty() {
        return None;
    }
    match key {
        Key::Named(Named::Enter) => Some(Message::Prompt(Answer::Save)),
        Key::Named(Named::Escape) => Some(Message::Prompt(Answer::Cancel)),
        _ => None,
    }
}

impl App {
    pub(super) fn with_prompt<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(prompt) = &self.prompt else {
            // Keep the workspace at the same child position, as Help does.
            return stack![base].into();
        };
        let button = |id, label: &'static str, answer, primary| {
            reshiki::accessibility::button(id, label, text(label).size(13))
                .padding([7, 14])
                .on_press(Message::Prompt(answer))
                .style(control(primary))
        };
        let (title, description, buttons) = match prompt {
            Prompt::Download { action, .. } => (
                match action {
                    #[cfg(target_arch = "wasm32")]
                    Download::Drawing => "Download drawing",
                    Download::Export { .. } => "Download export",
                }
                .into(),
                "Choose a file name. Downloading keeps any edits marked as unsaved.".into(),
                row![
                    Space::new().width(Length::Fill),
                    button("prompt-cancel", "Cancel", Answer::Cancel, false),
                    button("prompt-download", "Download", Answer::Save, true),
                ],
            ),
            Prompt::Save(name) => (
                format!("Do you want to save the changes you made to “{name}”?"),
                "Your changes will be lost if you don’t save them.".to_owned(),
                row![
                    button("prompt-dont-save", "Don’t Save", Answer::DontSave, false),
                    Space::new().width(Length::Fill),
                    button("prompt-cancel", "Cancel", Answer::Cancel, false),
                    button("prompt-save", "Save", Answer::Save, true),
                ],
            ),
            Prompt::Notice { title, text, .. } => (
                title.clone(),
                text.clone(),
                row![
                    Space::new().width(Length::Fill),
                    button("prompt-ok", "OK", Answer::Save, true),
                ],
            ),
        };
        let content = column![
            text(title).size(15),
            text(description).size(13).style(muted_text),
        ]
        .spacing(10);
        let content = if let Prompt::Download { name, .. } = prompt {
            content.push(
                iced::widget::text_input("File name", name)
                    .id("download-name")
                    .on_input(Message::DownloadName)
                    .on_submit(Message::Prompt(Answer::Save)),
            )
        } else {
            content
        };
        let content = content
            .push(Space::new().height(Length::Fill))
            .push(buttons.spacing(8).align_y(Alignment::Center));
        let height = 180.;
        let height = if matches!(prompt, Prompt::Download { .. }) {
            225.
        } else {
            height
        };
        modal(
            base,
            content.into(),
            440.,
            height,
            Placement::Center,
            Message::Prompt(Answer::Cancel),
        )
    }
}

#[cfg(test)]
mod tests;
