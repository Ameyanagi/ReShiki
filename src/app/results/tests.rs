use super::{Modal, Owner, admitted, async_result};
use crate::app::{Message, accessibility, atom_text, gates, office, tabs, updates};

#[test]
fn receipts_outside_the_matrix_preserve_predicates_and_wrapper_routing() {
    let binding = office::Binding::standalone("drawing.rsk".into(), office::Host::Office);
    // Historical raw predicate answers, including receipts handled before any modal gate.
    // Columns: atom text, preview, cleanup, optimization, updates background.
    let mut cases = vec![
        (Message::Opened(None), [true; 5]),
        (Message::OfficePrepared(binding.clone(), None), [false; 5]),
        (
            Message::Updates(updates::Action::Checked(0, Err("failure".into()))),
            [true, false, false, true, false],
        ),
        (
            Message::Updates(updates::Action::Saved(Err("failure".into()))),
            [true, false, false, true, false],
        ),
        (
            Message::Updates(updates::Action::Prepared(Err("failure".into()))),
            [true, false, false, true, false],
        ),
        (
            Message::Updates(updates::Action::RecoveryCleared),
            [true, false, false, true, false],
        ),
        (
            Message::Updates(updates::Action::Restarted(Err("failure".into()))),
            [true, false, false, true, false],
        ),
        (
            Message::Updates(updates::Action::Opened(Err("failure".into()))),
            [true, false, false, true, false],
        ),
        (
            Message::Accessibility(accessibility::Action::Geometry(
                iced::window::Id::unique(),
                iced::Size::new(800., 600.),
                1.,
            )),
            [false; 5],
        ),
        (
            Message::Accessibility(accessibility::Action::Snapshot(0, 0, Default::default())),
            [false; 5],
        ),
        (
            Message::Accessibility(accessibility::Action::Installed(0, Err("failure".into()))),
            [false; 5],
        ),
        (
            Message::Accessibility(accessibility::Action::Published(0, Err("failure".into()))),
            [false; 5],
        ),
    ];
    #[cfg(any(windows, target_os = "linux"))]
    cases.extend([
        (
            Message::Desktop(crate::app::desktop::Action::Ready),
            [false; 5],
        ),
        (
            Message::Desktop(crate::app::desktop::Action::Finished),
            [false; 5],
        ),
        (
            Message::Desktop(crate::app::desktop::Action::Office(binding, None)),
            [false; 5],
        ),
    ]);
    #[cfg(target_os = "macos")]
    cases.push((
        Message::MacFiles(crate::app::macos_files::Action::Open(Err("failure".into()))),
        [false; 5],
    ));
    for (message, expected) in cases {
        assert_eq!(async_result(&message), Some(Owner::App), "{message:?}");
        assert!(!tabs::document_result(&message), "{message:?}");
        assert_eq!(atom_text::background(&message), expected[0], "{message:?}");
        assert_eq!(
            gates::preview_passthrough(&message),
            expected[1],
            "{message:?}"
        );
        assert_eq!(updates::background(&message), expected[4], "{message:?}");
        for (modal, expected) in [
            Modal::AtomText,
            Modal::Preview,
            Modal::Cleanup,
            Modal::Optimization,
            Modal::UpdatesDialog,
        ]
        .into_iter()
        .zip(expected)
        {
            assert_eq!(
                admitted(modal, &message),
                expected,
                "{modal:?}: {message:?}"
            );
        }
    }

    for message in [
        Message::Tab(
            crate::app::document_tab::TabId(0),
            Box::new(Message::Pasted(None)),
        ),
        Message::Accessibility(accessibility::Action::Dispatch(
            0,
            Box::new(Message::Pasted(None)),
        )),
    ] {
        assert_eq!(async_result(&message), None, "{message:?}");
        assert!(!tabs::document_result(&message), "{message:?}");
    }
}
