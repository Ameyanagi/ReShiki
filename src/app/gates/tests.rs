//! Pins how the modal gates and draft predicates treat the cleanup, view-aid and graphic-input messages.
use super::preview_passthrough;
use crate::app::{
    App, CleanupPreview, Message, atom_text, cleanup, color_popover, inline_text, joining, tabs,
    updates, view_settings,
};
use crate::canvas::{Tool, guides::Unit};
use reshiki::cleanup::Scope;
use reshiki::graphics::{BracketSides, GraphicChange, GraphicKind, LinePattern};
use reshiki::scientific::{Phase, SymbolKind};

/// Columns: (message, passes the cleanup-preview gate, preview_passthrough, commits_draft).
fn cases() -> Vec<(Message, bool, bool, bool)> {
    vec![
        (Message::Cleanup(cleanup::Action::Begin), false, false, true),
        (Message::Cleanup(cleanup::Action::Apply), true, false, false),
        (
            Message::Cleanup(cleanup::Action::Cancel),
            true,
            false,
            false,
        ),
        (
            Message::Cleanup(cleanup::Action::Original(true)),
            true,
            false,
            false,
        ),
        (
            Message::Cleanup(cleanup::Action::Scope(Scope::SelectedMolecules)),
            true,
            false,
            false,
        ),
        (
            Message::Cleanup(cleanup::Action::Orientation(true)),
            true,
            false,
            false,
        ),
        (
            Message::View(view_settings::Action::Grid),
            true,
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::SmartGuides(false)),
            true,
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::Rulers(true)),
            true,
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::Crosshair(true)),
            true,
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::RulerUnit(Unit::Inches)),
            true,
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::Toggle),
            true,
            true,
            false,
        ),
        (
            Message::GraphicStyle(GraphicChange::Pattern(LinePattern::Dashed)),
            false,
            false,
            true,
        ),
        (Message::GraphicWidth("2".into()), false, false, false),
        (Message::ApplyGraphicWidth, false, false, false),
        (
            Message::GraphicStroke("#117E6C".into()),
            false,
            false,
            false,
        ),
        (Message::ApplyGraphicStroke, false, false, false),
        (Message::GraphicFill("#DCEFE9".into()), false, false, false),
        (Message::ApplyGraphicFill, false, false, false),
        (
            Message::GraphicSides(BracketSides::Left),
            false,
            false,
            false,
        ),
        (
            Message::ScientificKind(GraphicKind::Symbol(SymbolKind::CircleMinus)),
            false,
            false,
            false,
        ),
        (Message::OrbitalPhase(Phase::Shaded), false, false, false),
        (Message::FlipPhase(true), false, false, false),
        (Message::AttachSymbols(false), false, false, false),
    ]
}

fn previewing() -> App {
    let (mut app, _) = App::new();
    app.tab.cleanup = Some(CleanupPreview {
        job: cleanup::CleanupJob {
            options: Default::default(),
            selection: vec![],
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        },
        warnings: vec![],
        document: app.tab.doc.clone(),
        analysis: None,
        revision: app.tab.revision,
        epoch: app.tab.file_epoch,
        original: false,
    });
    app
}

#[test]
fn clustered_messages_keep_their_draft_and_background_predicates() {
    let (mut app, _) = App::new();
    for (message, _, passthrough, commits) in cases() {
        let context = format!("{message:?}");
        assert_eq!(preview_passthrough(&message), passthrough, "{context}");
        assert_eq!(inline_text::commits_draft(&message), commits, "{context}");
        assert_eq!(joining::cancels_draft(&message), commits, "{context}");
        assert!(!color_popover::keeps_open(&message), "{context}");
        assert!(!atom_text::background(&message), "{context}");
        assert!(!updates::background(&message), "{context}");
        assert!(!tabs::document_result(&message), "{context}");
        assert!(!app.reveals_inspector(&message), "{context}");
        app.tabs.menu = true;
        app.imports.menu = true;
        app.imports.examples_menu = true;
        app.close_transient_menus(&message);
        assert!(!app.tabs.menu, "{context}");
        assert!(!app.imports.menu, "{context}");
        assert!(!app.imports.examples_menu, "{context}");
    }
}

#[test]
fn cleanup_preview_admits_only_its_controls_and_view_aids() {
    for (message, passes, _, _) in cases() {
        let context = format!("{message:?}");
        let mut app = previewing();
        let status = app.status.clone();
        assert_eq!(
            app.cleanup_preview_gate(message).is_continue(),
            passes,
            "{context}"
        );
        if passes {
            assert_eq!(app.status, status, "{context}");
        } else {
            assert_eq!(
                app.status, "Apply or cancel the cleanup preview to continue editing",
                "{context}"
            );
        }
        assert!(app.tab.cleanup.is_some(), "{context}");
    }
    let mut app = previewing();
    assert!(
        app.cleanup_preview_gate(Message::Tool(Tool::Select))
            .is_break()
    );
    assert!(app.tab.cleanup.is_none());
    assert_eq!(app.status, "Cleanup cancelled · Drawing unchanged");
}
