use super::{App, Message, files};
use iced::Task;
use std::{ffi::OsString, path::PathBuf};

#[derive(Default)]
pub(crate) struct Arguments {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) shortcut_examples: bool,
    pub(crate) office_host: Option<super::office::Host>,
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Arguments {
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
            parsed.office_host = Some(super::office::Host::Office);
        } else if arg == "--libreoffice-edit" {
            parsed.office_host = Some(super::office::Host::LibreOffice);
        } else if arg == "--office-addin-edit" {
            parsed.office_host = Some(super::office::Host::Microsoft365);
        }
    }
    parsed
}

impl App {
    pub(super) fn open_startup(&mut self, mut args: Arguments) -> Task<Message> {
        if let Some(host) = args.office_host
            && !args.paths.is_empty()
        {
            let path = args.paths.remove(0);
            let binding = super::office::Binding::standalone(path.clone(), host);
            return Task::perform(files::read(path), move |opened| {
                Message::OfficePrepared(binding.clone(), opened)
            })
            .chain(files::open_paths(args.paths));
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
