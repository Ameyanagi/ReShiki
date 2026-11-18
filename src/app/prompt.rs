//! Questions the app asks in its own dialog: whether to save a drawing before
//! closing it, which format to save in, and notices to acknowledge. The
//! desktop asks about unsaved changes with the system's dialogs; the browser's
//! confirm() has only OK and Cancel, so the browser build asks here instead.
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
    /// The first Save of a ChemDraw or MOL file: keep this format, or save
    /// as a ReShiki drawing.
    KeepFormat(&'static str),
    /// The formats Save as offers.
    SaveAs,
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
    /// A row of the Save as list.
    Format(&'static str),
}

impl Prompt {
    /// The message that answers the question, as the system dialog's would.
    pub(super) fn answer(self, answer: Answer) -> Message {
        match self {
            Self::Save(_) => match answer {
                Answer::Save => Message::Save,
                Answer::DontSave => Message::Discard,
                Answer::Cancel | Answer::Format(_) => Message::Cancel,
            },
            Self::KeepFormat(_) => match answer {
                Answer::Save => Message::SaveFormat(reshiki::compatibility::NATIVE_EXTENSION),
                Answer::DontSave => Message::KeepFormat,
                _ => Message::Cancel,
            },
            Self::SaveAs => match answer {
                Answer::Save => Message::SaveFormat(reshiki::compatibility::NATIVE_EXTENSION),
                Answer::Format(format) => Message::SaveFormat(format),
                _ => Message::Cancel,
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
        let button = |id: &str, label: &str, answer, primary| {
            reshiki::accessibility::button(id.to_owned(), label, text(label.to_owned()).size(13))
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
            Prompt::KeepFormat(format) => (
                format!(
                    "Save {}",
                    std::path::Path::new(&self.document_name())
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                ),
                super::files::keep_format_text(format, self.tab.chemdraw_loss, &self.tab.doc),
                row![
                    Space::new().width(Length::Fill),
                    button(
                        "prompt-keep-format",
                        &format!("Keep as {}", format.to_uppercase()),
                        Answer::DontSave,
                        false
                    ),
                    button("prompt-save", "Save as ReShiki", Answer::Save, true),
                ],
            ),
            Prompt::SaveAs => (
                "Save as".to_owned(),
                "ChemDraw keeps most of the drawing. MOL keeps the structure only.".to_owned(),
                row![
                    Space::new().width(Length::Fill),
                    button("prompt-cancel", "Cancel", Answer::Cancel, false),
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
        let height = match prompt {
            Prompt::Download { .. } => 225.,
            // The list of what a ChemDraw save loses runs to three lines.
            Prompt::KeepFormat(format) if description != super::files::format_keeps(format) => 190.,
            Prompt::KeepFormat(_) => 150.,
            Prompt::SaveAs => 330.,
            _ => 180.,
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
        } else if matches!(prompt, Prompt::SaveAs) {
            super::files::SAVE_FORMATS.iter().enumerate().fold(
                content,
                |content, (index, (format, name))| {
                    content.push(
                        button(
                            &format!("prompt-format-{format}"),
                            name,
                            Answer::Format(format),
                            index == 0,
                        )
                        .width(Length::Fill),
                    )
                },
            )
        } else {
            content
        };
        let content = content
            .push(Space::new().height(Length::Fill))
            .push(buttons.spacing(8).align_y(Alignment::Center));
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
