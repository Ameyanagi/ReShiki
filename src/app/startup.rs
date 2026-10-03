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
mod tests {
    use super::*;

    #[test]
    fn repeated_open_keeps_path_order_spaces_and_unicode() {
        let args = parse(
            [
                "--open",
                "first drawing.rsk",
                "--open",
                "資料/構造 β.rsk",
                "--open",
            ]
            .map(OsString::from),
        );
        assert_eq!(
            args.paths,
            [PathBuf::from("first drawing.rsk"), "資料/構造 β.rsk".into()]
        );
        assert!(!args.shortcut_examples);
    }

    #[test]
    fn old_single_path_and_shortcut_examples_flags_are_readable() {
        let args = parse(["--unused", "--open", "old drawing.rsk"].map(OsString::from));
        assert_eq!(args.paths, [PathBuf::from("old drawing.rsk")]);
        let args = parse(["--shortcut-examples"].map(OsString::from));
        assert!(args.paths.is_empty());
        assert!(args.shortcut_examples);
        assert!(parse([]).paths.is_empty());
    }

    #[test]
    fn microsoft_365_edit_flag_is_separate_from_legacy_office_and_libreoffice() {
        for (flag, host) in [
            ("--office-addin-edit", "Microsoft 365"),
            ("--office-edit", "Office"),
            ("--libreoffice-edit", "LibreOffice"),
        ] {
            let args = parse([flag, "--open", "drawing.rsk"].map(OsString::from));
            assert_eq!(args.office_host, Some(host));
            assert_eq!(args.paths, [PathBuf::from("drawing.rsk")]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn paths_do_not_require_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(b"drawing-\xff.rsk".to_vec());
        let args = parse([OsString::from("--open"), path.clone()]);
        assert_eq!(args.paths, [PathBuf::from(path)]);
    }

    #[test]
    fn launch_flag_opens_the_same_unbound_examples_tab() {
        let (mut app, _) = App::new();
        let _ = app.open_startup(parse(["--shortcut-examples"].map(OsString::from)));
        assert_eq!(app.document_name(), "Shortcut examples");
        assert!(app.tab.path.is_none());
        assert!(!app.dirty());
    }
}
