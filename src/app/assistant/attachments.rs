//! Image attachments: clipboard paste, image files and the clipboard-text fallback, guarded by image serial and file epoch.

use super::Action;
use crate::app::{App, Message};
use iced::Task;
use iced::widget::text_editor;

impl App {
    pub(super) fn assistant_paste(&mut self, image_only: bool) -> Task<Message> {
        self.assistant.menu = None;
        if self.assistant.reading_image || (image_only && self.assistant.busy) {
            return Task::none();
        }
        self.assistant.image_serial = self.assistant.image_serial.wrapping_add(1);
        let serial = self.assistant.image_serial;
        let epoch = self.tab.file_epoch;
        self.assistant.reading_image = true;
        if !reshiki::clipboard::available() {
            return iced::clipboard::read().map(move |text| {
                Message::Assistant(Action::TextPasted {
                    serial,
                    epoch,
                    text,
                })
            });
        }
        Task::perform(reshiki::clipboard::picture(), move |result| {
            Message::Assistant(Action::ImageRead {
                serial,
                epoch,
                image_only,
                result,
            })
        })
    }
    pub(super) fn assistant_open_image(&mut self) -> Task<Message> {
        self.assistant.menu = None;
        if self.assistant.busy || self.assistant.reading_image {
            return Task::none();
        }
        self.assistant.image_serial = self.assistant.image_serial.wrapping_add(1);
        let serial = self.assistant.image_serial;
        let epoch = self.tab.file_epoch;
        self.assistant.reading_image = true;
        Task::perform(
            async {
                let Some(file) = rfd::AsyncFileDialog::new()
                    .set_title("Attach a chemical drawing")
                    .add_filter("Images", &["png", "jpg", "jpeg", "tif", "tiff", "webp"])
                    .pick_file()
                    .await
                else {
                    return Ok(None);
                };
                let path = file.path().to_owned();
                tokio::task::spawn_blocking(move || {
                    reshiki::pictures::Picture::open(&path).map(Some)
                })
                .await
                .map_err(|e| e.to_string())?
            },
            move |result| {
                Message::Assistant(Action::ImageRead {
                    serial,
                    epoch,
                    image_only: true,
                    result,
                })
            },
        )
    }
    pub(super) fn assistant_image_read(
        &mut self,
        serial: u64,
        epoch: u64,
        image_only: bool,
        result: Result<Option<reshiki::pictures::Picture>, String>,
    ) -> Task<Message> {
        if serial != self.assistant.image_serial {
            return Task::none();
        }
        self.assistant.reading_image = false;
        if epoch != self.tab.file_epoch {
            return Task::none();
        }
        match result {
            Ok(Some(_)) if self.assistant.busy => {
                self.assistant.status =
                    "Finish or stop the current request, then paste the image again.".into();
            }
            Ok(Some(image)) => {
                // Keep the exact source for follow-up requests and show it in the composer.
                self.assistant.source_image = Some(image);
                self.assistant.guided_example = false;
                if self.assistant.draft.is_none() {
                    self.assistant.requires_apply = false;
                }
                self.assistant.example_reference = None;
                self.assistant.error = false;
                self.assistant.status =
                    "Image attached · Add instructions or Send to draw its structure.".into();
            }
            Ok(None) if !image_only => {
                self.assistant.reading_image = true;
                return iced::clipboard::read().map(move |text| {
                    Message::Assistant(Action::TextPasted {
                        serial,
                        epoch,
                        text,
                    })
                });
            }
            Ok(None) => {
                self.assistant.status =
                    "Copy an image, then choose Paste image, or open an image file.".into();
            }
            Err(error) => {
                self.assistant.error = true;
                self.assistant.status = format!("Could not read image: {error}");
            }
        }
        Task::none()
    }
    pub(super) fn assistant_text_pasted(
        &mut self,
        serial: u64,
        epoch: u64,
        text: Option<String>,
    ) -> Task<Message> {
        if serial != self.assistant.image_serial {
            return Task::none();
        }
        self.assistant.reading_image = false;
        if epoch == self.tab.file_epoch
            && let Some(text) = text
        {
            self.assistant
                .input
                .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                    std::sync::Arc::new(text),
                )));
        }
        Task::none()
    }
    pub(super) fn assistant_clear_image(&mut self) {
        self.assistant.menu = None;
        self.assistant.source_image = None;
        if !self.assistant.busy && self.assistant.draft.is_none() {
            self.assistant.guided_example = false;
            self.assistant.requires_apply = false;
            self.assistant.example_reference = None;
        }
        self.assistant.image_serial = self.assistant.image_serial.wrapping_add(1);
        self.assistant.reading_image = false;
    }
}
