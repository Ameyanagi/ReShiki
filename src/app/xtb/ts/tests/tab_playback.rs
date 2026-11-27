use super::*;
use crate::app::tabs::Action as Tabs;

#[test]
fn irc_playback_stops_on_tab_switch_and_opening_another_file_without_resuming() {
    for open_file in [false, true] {
        let (mut app, _) = irc_profile::finished();
        let source = app.tab.id;
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        let other = if open_file {
            None
        } else {
            app.add_tab();
            let other = app.tab.id;
            let _ = app.update(Message::Tabs(Tabs::Select(source)));
            Some(other)
        };
        send(&mut app, XtbAction::Ts(Action::PlayIrc));
        send(&mut app, XtbAction::Ts(Action::IrcTick));
        assert!(app.xtb_irc_preview().is_some());
        let _ = app.update(Message::Tabs(Tabs::Select(source)));
        assert!(app.xtb_irc_preview().is_some(), "The tab did not change");

        if let Some(other) = other {
            let _ = app.update(Message::Tabs(Tabs::Select(other)));
        } else {
            let mut doc = Document::default();
            doc.add_atom("S", Default::default());
            let _ = app.update(Message::FilePrepared(Some((
                "irc-second-file.rsk".into(),
                Ok(crate::app::files::Prepared::Native(Box::new(doc), None)),
            ))));
        }
        assert_ne!(app.tab.id, source);
        assert_eq!(app.strip().count(), 2);
        let background = app.strip().find(|tab| tab.id == source).unwrap();
        assert!(background.xtb.palette.irc_playback.is_none());

        let _ = app.update(Message::Tabs(Tabs::Select(source)));
        assert!(app.xtb_irc_preview().is_none());
        send(&mut app, XtbAction::Ts(Action::IrcTick));
        assert!(app.xtb_irc_preview().is_none());
        assert_eq!(model(app.display_document()), model(&before));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
    }
}

#[test]
fn closing_the_playing_irc_tab_discards_playback_and_ignores_a_queued_tick() {
    let (mut app, _) = irc_profile::finished();
    let source = app.tab.id;
    app.add_tab();
    let other = app.tab.id;
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tabs(Tabs::Select(source)));
    app.tab.mark_saved(app.tab.doc.clone());
    send(&mut app, XtbAction::Ts(Action::PlayIrc));
    send(&mut app, XtbAction::Ts(Action::IrcTick));
    assert!(app.xtb_irc_preview().is_some());

    let _ = app.update(Message::Tabs(Tabs::Close(None)));
    assert_eq!(app.tab.id, other);
    assert!(app.tab_index(source).is_none());
    assert!(app.xtb_irc_preview().is_none());
    send(&mut app, XtbAction::Ts(Action::IrcTick));
    assert!(app.xtb_irc_preview().is_none());
    assert_eq!(app.tab.doc, before);
}
