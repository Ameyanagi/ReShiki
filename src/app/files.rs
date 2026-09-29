use super::Message;
use iced::Task;
use std::path::PathBuf;

const SAVE: &str = "Save";
const DONT_SAVE: &str = "Don’t Save";

/// Asks with the system dialog whether to save `name` before continuing a
/// pending action; the answer arrives as Save, Discard or Cancel.
pub(super) fn ask_to_save(name: String) -> Task<Message> {
    iced::window::latest().then(move |window| {
        let question = format!("Do you want to save the changes you made to “{name}”?");
        let consequence = "Your changes will be lost if you don’t save them.";
        // macOS alerts show the title in bold above the description; other
        // platforms use the title as the window caption.
        let (title, description) = if cfg!(target_os = "macos") {
            (question, consequence.to_owned())
        } else {
            (
                "Save changes?".to_owned(),
                format!("{question} {consequence}"),
            )
        };
        let dialog = rfd::AsyncMessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(title)
            .set_description(description)
            .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                SAVE.into(),
                DONT_SAVE.into(),
                "Cancel".into(),
            ));
        match window {
            Some(id) => iced::window::run(id, move |window| dialog.set_parent(&window).show())
                .then(|answer| Task::perform(answer, save_answer)),
            None => Task::perform(dialog.show(), save_answer),
        }
    })
}

/// Platforms without custom buttons answer Yes/No/Cancel in the same order.
fn save_answer(result: rfd::MessageDialogResult) -> Message {
    use rfd::MessageDialogResult as Answer;
    match result {
        Answer::Yes => Message::Save,
        Answer::No => Message::Discard,
        Answer::Custom(label) if label == SAVE => Message::Save,
        Answer::Custom(label) if label == DONT_SAVE => Message::Discard,
        _ => Message::Cancel,
    }
}

/// Keep native file types and filename completion consistent across save dialogs.
pub(super) async fn save_path(title: &str, name: &str, extension: &str) -> Option<PathBuf> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(name);
    let dialog = if extension == reshiki::compatibility::NATIVE_EXTENSION {
        dialog.add_filter("ReShiki drawing", reshiki::compatibility::NATIVE_EXTENSIONS)
    } else {
        dialog.add_filter(extension, &[extension])
    };
    let file = dialog.save_file().await?;
    let path = reshiki::storage::with_default_extension(file.path(), extension);
    // Some platforms do not append the filter suffix. If completion changes the
    // destination, the native dialog has not confirmed overwriting that file.
    if path != file.path() && path.exists() {
        let replace = rfd::AsyncMessageDialog::new()
            .set_title("Replace existing file?")
            .set_description(format!("{} already exists. Replace it?", path.display()))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            .await;
        if replace != rfd::MessageDialogResult::Yes {
            return None;
        }
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rfd::MessageDialogResult as Answer;

    #[test]
    fn save_dialog_answers_map_to_save_discard_and_cancel() {
        for answer in [Answer::Custom(SAVE.into()), Answer::Yes] {
            assert!(matches!(save_answer(answer), Message::Save));
        }
        for answer in [Answer::Custom(DONT_SAVE.into()), Answer::No] {
            assert!(matches!(save_answer(answer), Message::Discard));
        }
        for answer in [Answer::Custom("Cancel".into()), Answer::Cancel, Answer::Ok] {
            assert!(matches!(save_answer(answer), Message::Cancel));
        }
    }
}
