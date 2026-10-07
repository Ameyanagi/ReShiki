use super::{App, Message, files};
use iced::Task;
use std::{ffi::OsString, path::PathBuf};

#[derive(Default)]
pub(super) struct Arguments {
    pub(super) paths: Vec<PathBuf>,
    shortcut_examples: bool,
    office_host: Option<&'static str>,
}

pub(super) fn parse(args: impl IntoIterator<Item = OsString>) -> Arguments {
    let mut args = args.into_iter();
    let mut parsed = Arguments::default();
    while let Some(arg) = args.next() {
        if arg == "--open" {
            if let Some(path) = args.next() {
                parsed.paths.push(path.into());
            }
        } else if arg == "--shortcut-examples" {
            parsed.shortcut_examples = true;
        }
        if arg == "--office-edit" {
            parsed.office_host = Some("Office");
        } else if arg == "--libreoffice-edit" {
            parsed.office_host = Some("LibreOffice");
        } else if arg == "--office-addin-edit" {
            parsed.office_host = Some("Microsoft 365");
        }
    }
    parsed
}

impl App {
    pub(super) fn open_startup(&mut self, args: Arguments) -> Task<Message> {
        if let Some(host) = args.office_host {
            self.office_path = args.paths.first().cloned();
            self.office_host = host;
        }
        if args.paths.is_empty() && args.shortcut_examples {
            return self.open_shortcut_examples();
        }
        let open = files::open_paths(args.paths);
        if args.shortcut_examples {
            open.chain(Task::done(Message::OpenShortcutExamples))
        } else {
            open
        }
    }
}

#[cfg(test)]
mod tests;
