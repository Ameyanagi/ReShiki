use super::{App, Message, files};
use crate::session::{Arguments, Binding};
use iced::Task;

impl App {
    pub(super) fn open_startup(&mut self, mut args: Arguments) -> Task<Message> {
        if let Some(host) = args.office_host
            && !args.paths.is_empty()
        {
            let path = args.paths.remove(0);
            let binding = Binding::standalone(path.clone(), host);
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
