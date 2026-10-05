use super::*;
use iced::keyboard::{Key, Modifiers, key::Named};

#[test]
fn recent_context_matches_history_after_the_101st_edit_and_a_new_branch() {
    let mut doc = Document::default();
    let first = doc.add_atom("C", Point::default());
    let second = doc.add_atom("O", Point::new(100., 0.));
    let mut history = reshiki::document::History::default();
    let mut recent = Recent::default();
    let mut states = vec![(doc.clone(), recent.atoms.clone())];
    for index in 1..=101 {
        let before = doc.clone();
        let id = if index % 2 == 0 { first } else { second };
        doc.atom_mut(id).unwrap().position.y += 1.;
        history.commit(before.clone(), &doc);
        recent.record(&before, &doc, 0, false);
        states.push((doc.clone(), recent.atoms.clone()));
    }
    for index in (1..101).rev() {
        assert!(history.undo(&mut doc));
        recent.restore(false, 0);
        assert_eq!(doc, states[index].0);
        assert_eq!(recent.atoms, states[index].1);
    }
    assert!(!history.undo(&mut doc));
    recent.record(&doc, &doc, 0, false);
    assert!(!history.commit(doc.clone(), &doc));
    assert_eq!(recent.redo.len(), 100);
    for state in &states[2..] {
        assert!(history.redo(&mut doc));
        recent.restore(true, 0);
        assert_eq!(doc, state.0);
        assert_eq!(recent.atoms, state.1);
    }
    assert!(history.undo(&mut doc));
    recent.restore(false, 0);
    let before = doc.clone();
    doc.atom_mut(second).unwrap().position.y += 10.;
    history.commit(before.clone(), &doc);
    recent.record(&before, &doc, 0, false);
    assert!(!history.can_redo());
    assert!(recent.redo.is_empty());
    assert_eq!(recent.atoms, [second]);
    recent.check_epoch(1);
    assert!(recent.undo.is_empty() && recent.redo.is_empty() && recent.atoms.is_empty());
}

fn chain(doc: &mut Document, x: f32, y: f32) -> Vec<u64> {
    let a = doc.add_atom("C", Point::new(x, y));
    let b = doc.add_atom("C", Point::new(x + 36., y - 21.));
    let c = doc.add_atom("O", Point::new(x + 72., y));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    vec![a, b, c]
}

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app
}

fn space(app: &mut App) {
    let _ = app.update(Message::Shortcut(Action::SelectRecent));
}

#[test]
fn modifier_routing_keeps_other_arrow_shortcuts() {
    let key = Key::Named(Named::ArrowRight);
    let command = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    assert!(matches!(
        super::super::shortcuts::key_message(&key, &key, command | Modifiers::SHIFT),
        Some(Message::Shortcut(Action::ReactionCopy))
    ));
    assert!(super::super::shortcuts::key_message(&key, &key, command).is_none());
    assert!(
        super::super::shortcuts::key_message(
            &key,
            &key,
            command | Modifiers::SHIFT | Modifiers::ALT
        )
        .is_none()
    );
    assert!(matches!(
        super::super::shortcuts::key_message(&key, &key, Modifiers::ALT | Modifiers::SHIFT),
        Some(Message::Transform(_))
    ));
    let space = Key::Named(Named::Space);
    assert!(matches!(
        super::super::shortcuts::key_message(&space, &space, Modifiers::empty()),
        Some(Message::Shortcut(Action::SelectRecent))
    ));
    assert!(matches!(
        super::super::shortcuts::key_message(&space, &space, Modifiers::SHIFT),
        Some(Message::Tool(Tool::Select))
    ));
}

#[test]
fn reaction_expands_partial_molecule_remaps_ids_and_undoes_once() -> Result<(), String> {
    let mut app = app();
    let original_ids = chain(&mut app.tab.doc, 0., 0.);
    app.tab.selected = vec![original_ids[1]];
    let original = app.tab.doc.clone();
    let _ = app.update(Message::Shortcut(Action::ReactionCopy));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.atoms.len(), 6);
    assert_eq!(app.tab.doc.bonds.len(), 4);
    assert_eq!(app.tab.doc.arrows.len(), 1);
    assert_eq!(app.tab.doc.reactions[0].reactants[0].atoms, original_ids);
    let copied = app.tab.selected.clone();
    assert_eq!(app.tab.doc.reactions[0].products[0].atoms, copied);
    assert!(copied.iter().all(|id| !original_ids.contains(id)));
    assert_eq!(&app.tab.doc.atoms[..3], original.atoms);
    let arrow = &app.tab.doc.arrows[0];
    let (_, source_hi) = reshiki::scene::selection_bounds(&original, &original_ids).unwrap();
    let (product_lo, _) = reshiki::scene::selection_bounds(&app.tab.doc, &copied).unwrap();
    assert!(arrow.start.x > source_hi.x && arrow.end.x < product_lo.x);
    let result = app.tab.doc.clone();
    let json = serde_json::to_string(&result).unwrap();
    assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), result);
    app.tab.doc.validate()?;
    app.tab.selected.clear();
    space(&mut app);
    assert_eq!(app.tab.selected, copied);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, result);
    app.tab.selected.clear();
    space(&mut app);
    assert_eq!(app.tab.selected, copied);
    // The copied product is the source for a second arrow in a scheme.
    let _ = app.update(Message::Shortcut(Action::ReactionCopy));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.reactions.len(), 2);
    assert_eq!(app.tab.doc.reactions[1].reactants[0].atoms, copied);
    Ok(())
}

#[test]
fn reaction_preserves_groups_abbreviations_and_multiple_participants() -> Result<(), String> {
    let mut doc = Document::default();
    let first = chain(&mut doc, 0., 0.);
    let second = chain(&mut doc, 0., 100.);
    doc.contract(&first, "EtOH", "")?;
    let label = doc.next_id();
    doc.annotations.push(reshiki::document::Annotation {
        id: label,
        position: Point::new(0., 150.),
        text: "Starting materials".into(),
        format: Default::default(),
    });
    let mut ids = [first, second].concat();
    ids.push(label);
    doc.group_selection(&ids)?;
    let (result, copied) = reaction_copy(&doc, &[ids[0]], 42.)?;
    assert_eq!(result.groups.len(), 2);
    assert_eq!(result.abbreviations.len(), 2);
    assert_eq!(result.annotations.len(), 2);
    assert_eq!(result.reactions[0].reactants.len(), 2);
    assert_eq!(result.reactions[0].products.len(), 2);
    assert_eq!(copied.len(), ids.len());
    assert!(
        result.groups[1]
            .members
            .iter()
            .all(|id| copied.contains(id))
    );
    assert!(
        result.abbreviations[1]
            .members
            .iter()
            .all(|id| copied.contains(id))
    );
    result.validate()
}

#[test]
fn ambiguous_or_obstructed_reaction_does_not_change_history() {
    let mut app = app();
    let ids = chain(&mut app.tab.doc, 0., 0.);
    let obstacle = chain(&mut app.tab.doc, 255., 0.);
    for selected in [vec![], ids.clone()] {
        app.tab.selected = selected;
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Shortcut(Action::ReactionCopy));
        assert!(app.error);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
    app.tab.doc.delete(&obstacle);
    app.tab.selected = ids;
    let _ = app.update(Message::Shortcut(Action::ReactionCopy));
    let before = app.tab.doc.clone();
    app.tab.selected.push(app.tab.doc.arrows[0].id);
    let _ = app.update(Message::Shortcut(Action::ReactionCopy));
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn background_graphics_allow_reaction_copy_and_remain_unchanged() -> Result<(), String> {
    use reshiki::graphics::{Graphic, GraphicKind};
    let mut doc = Document::default();
    let ids = chain(&mut doc, 0., 0.);
    let background = Graphic::dragged(
        doc.next_id(),
        GraphicKind::Rectangle,
        Point::new(-100., -100.),
        Point::new(1000., 100.),
        Default::default(),
        Default::default(),
        false,
    );
    assert!(background.layer < 0);
    doc.graphics.push(background.clone());
    let (copy, _) = reaction_copy(&doc, &ids, 42.)?;
    assert_eq!(copy.graphics, vec![background]);
    assert_eq!(copy.arrows.len(), 1);
    doc.graphics[0].layer = 1;
    assert!(
        reaction_copy(&doc, &ids, 42.)
            .unwrap_err()
            .contains("Make room")
    );
    Ok(())
}

#[test]
fn space_tracks_edits_instead_of_selection_and_follows_history() {
    let mut app = app();
    let before = app.tab.doc.clone();
    let first = chain(&mut app.tab.doc, 0., 0.);
    app.changed(before);
    let before = app.tab.doc.clone();
    let second = chain(&mut app.tab.doc, 0., 100.);
    app.changed(before);
    app.tab.selected = first.clone();
    space(&mut app);
    assert_eq!(app.tab.selected, second);
    let _ = app.update(Message::Undo);
    app.tab.selected.clear();
    space(&mut app);
    assert_eq!(app.tab.selected, first);
    let _ = app.update(Message::Redo);
    app.tab.selected.clear();
    space(&mut app);
    assert_eq!(app.tab.selected, second);
    let before = app.tab.doc.clone();
    app.tab.doc.bonds[0].order = 2;
    app.changed(before);
    space(&mut app);
    assert_eq!(app.tab.selected, first);
    let before = app.tab.doc.clone();
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id: app.tab.doc.next_id(),
        position: Point::new(0., 200.),
        text: "Caption".into(),
        format: Default::default(),
    });
    app.changed(before);
    app.tab.selected.clear();
    space(&mut app);
    assert_eq!(app.tab.selected, first);
    let _ = app.update(Message::Delete);
    app.tab.selected.clear();
    space(&mut app);
    assert!(app.tab.selected.is_empty());
    let _ = app.update(Message::Undo);
    space(&mut app);
    assert_eq!(app.tab.selected, first);
}

#[test]
fn continuous_and_multi_molecule_edits_have_matching_selection_history() {
    let mut app = app();
    let before = app.tab.doc.clone();
    let first = chain(&mut app.tab.doc, 0., 0.);
    let second = chain(&mut app.tab.doc, 0., 100.);
    app.changed(before);
    app.tab.selected.clear();
    space(&mut app);
    assert_eq!(app.tab.selected, [first.clone(), second.clone()].concat());
    for continuing in [false, true, true] {
        let before = app.tab.doc.clone();
        app.tab.doc.translate(&first, 1., 0.);
        app.changed_continuing(before, continuing);
    }
    space(&mut app);
    assert_eq!(app.tab.selected, first);
    let _ = app.update(Message::Undo);
    space(&mut app);
    assert_eq!(app.tab.selected, [first.clone(), second.clone()].concat());
    let _ = app.update(Message::Redo);
    space(&mut app);
    assert_eq!(app.tab.selected, first);
    // A rejected edit and a selection-only action cannot steal context.
    let before = app.tab.doc.clone();
    app.tab.doc.atoms.last_mut().unwrap().position.x = f32::NAN;
    app.changed(before);
    let _ = app.update(Message::Canvas(Edit::Select(second)));
    space(&mut app);
    assert_eq!(app.tab.selected, first);
}

#[test]
fn erase_stroke_recalls_every_affected_molecule_without_previous_edits() {
    for caption_first in [false, true] {
        let mut app = app();
        app.tab.busy = false;
        app.tab.camera.zoom = 1.;
        let first = chain(&mut app.tab.doc, 0., 0.);
        let second = chain(&mut app.tab.doc, 0., 100.);
        let unrelated = chain(&mut app.tab.doc, 300., 0.);
        let before = app.tab.doc.clone();
        app.tab.doc.atom_mut(unrelated[2]).unwrap().element = "N".into();
        app.changed(before);
        if caption_first {
            app.tab.doc.annotations.push(reshiki::document::Annotation {
                id: app.tab.doc.next_id(),
                position: Point::new(0., -100.),
                text: "Caption".into(),
                format: Default::default(),
            });
        }
        let original = app.tab.doc.clone();
        let a = app.tab.doc.atom(first[0]).unwrap().position;
        let b = app.tab.doc.atom(second[0]).unwrap().position;
        let _ = app.update(Message::Tool(Tool::Erase));
        if caption_first {
            let p = Point::new(0., -100.);
            let _ = app.update(Message::Canvas(Edit::EraseStart(p)));
            assert!(app.tab.doc.annotations.is_empty());
            let _ = app.update(Message::Canvas(Edit::EraseTo(p, a)));
        } else {
            let _ = app.update(Message::Canvas(Edit::EraseStart(a)));
        }
        let _ = app.update(Message::Canvas(Edit::EraseTo(a, b)));
        let _ = app.update(Message::Canvas(Edit::EraseEnd));
        assert!(app.tab.doc.atom(first[0]).is_none() && app.tab.doc.atom(second[0]).is_none());
        let expected = [first[1..].to_vec(), second[1..].to_vec()].concat();
        let erased = app.tab.doc.clone();
        let _ = app.update(Message::Tool(Tool::Select));
        space(&mut app);
        assert_eq!(app.tab.selected, expected);
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.tab.doc, original,
            "One Undo restores the complete erase stroke"
        );
        space(&mut app);
        assert_eq!(app.tab.selected, unrelated);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, erased);
        space(&mut app);
        assert_eq!(app.tab.selected, expected);
    }
}

#[test]
fn fresh_file_and_non_select_tools_do_not_reuse_stale_context() {
    let mut app = app();
    let before = app.tab.doc.clone();
    let ids = chain(&mut app.tab.doc, 0., 0.);
    app.changed(before);
    app.tab.selected.clear();
    app.tool = Tool::Bond(1);
    space(&mut app);
    assert_eq!(app.tool, Tool::Select);
    assert!(app.tab.selected.is_empty());
    space(&mut app);
    assert_eq!(app.tab.selected, ids);
    app.tab.file_epoch += 1;
    app.tab.selected.clear();
    space(&mut app);
    assert!(app.tab.selected.is_empty());
    assert!(app.status.contains("No recently edited"));
}

#[test]
fn paste_is_recalled_but_open_and_new_clear_atom_id_context() {
    let mut app = app();
    let before = app.tab.doc.clone();
    let original = chain(&mut app.tab.doc, 0., 0.);
    app.changed(before);
    let contents = serde_json::to_string(&app.tab.doc).unwrap();
    let _ = app.update(Message::Pasted(Some(format!(
        "{}{contents}",
        editing::CLIPBOARD_PREFIX
    ))));
    let pasted = app.tab.selected.clone();
    assert!(pasted.iter().all(|id| !original.contains(id)));
    app.tab.selected.clear();
    space(&mut app);
    assert_eq!(app.tab.selected, pasted);
    let task = app.update(Message::Opened(Some((
        "different.rsk".into(),
        Ok(contents.clone().into_bytes()),
    ))));
    assert!(task.units() > 0);
    super::super::files::finish_dispatched_open(
        &mut app,
        "different.rsk".into(),
        Ok(contents.into_bytes()),
    );
    space(&mut app);
    assert!(app.tab.selected.is_empty());
    assert!(!app.tab.history.can_undo());
    let before = app.tab.doc.clone();
    app.tab.doc.atoms[0].charge = 1;
    app.changed(before);
    space(&mut app);
    assert_eq!(app.tab.selected, original);
    let _ = app.update(Message::New);
    space(&mut app);
    assert!(app.tab.selected.is_empty());
    assert!(app.tab.doc.atoms.is_empty());
}

#[test]
fn splitting_a_molecule_recalls_both_remaining_components() {
    let mut app = app();
    let before = app.tab.doc.clone();
    let ids = chain(&mut app.tab.doc, 0., 0.);
    app.changed(before);
    app.tab.selected = vec![ids[1]];
    let _ = app.update(Message::Delete);
    space(&mut app);
    assert_eq!(app.tab.selected, [ids[0], ids[2]]);
    let _ = app.update(Message::Undo);
    space(&mut app);
    assert_eq!(app.tab.selected, ids);
}

#[test]
fn copied_stereocenter_keeps_ordered_neighbors_in_its_own_molecule() -> Result<(), String> {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    let mut neighbors = Vec::new();
    for (element, x, y) in [
        ("F", -42., 0.),
        ("Cl", 0., -42.),
        ("Br", 42., 0.),
        ("I", 0., 42.),
    ] {
        let id = doc.add_atom(element, Point::new(x, y));
        doc.add_bond(center, id, 1, "plain");
        neighbors.push(id);
    }
    doc.atom_mut(center).unwrap().stereo = Some(reshiki::document::AtomStereo {
        winding: "cw".into(),
        neighbors: neighbors.clone(),
    });
    let (copied, ids) = reaction_copy(&doc, &[center], 42.)?;
    let copy = copied
        .atoms
        .iter()
        .find(|a| ids.contains(&a.id) && a.element == "C")
        .unwrap();
    let stereo = copy.stereo.as_ref().unwrap();
    assert_eq!(stereo.winding, "cw");
    let labels: Vec<_> = stereo
        .neighbors
        .iter()
        .map(|id| copied.atom(*id).unwrap().element.as_str())
        .collect();
    assert_eq!(labels, ["F", "Cl", "Br", "I"]);
    assert!(
        stereo
            .neighbors
            .iter()
            .all(|id| ids.contains(id) && !neighbors.contains(id))
    );
    copied.validate()
}

#[test]
fn text_drafts_keep_both_shortcuts_out_of_the_drawing() {
    for atom_text in [false, true] {
        let mut app = app();
        let ids = chain(&mut app.tab.doc, 0., 0.);
        app.tab.selected = vec![ids[0]];
        let _ = if atom_text {
            app.update(Message::AtomText(super::super::atom_text::Action::Begin(
                Some(ids[0]),
            )))
        } else {
            app.update(Message::InlineText(
                super::super::inline_text::Action::Begin(None, Point::new(100., 100.)),
            ))
        };
        let before = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        space(&mut app);
        let _ = app.update(Message::Shortcut(Action::ReactionCopy));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.atom_text.is_some(), atom_text);
        assert_eq!(app.tab.inline_text.is_some(), !atom_text);
        assert!(!app.tab.history.can_undo());
    }
}

#[tokio::test]
#[ignore = "Manual application-renderer captures and focused-widget key verification"]
async fn molecule_shortcut_visual_evidence() {
    use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
    let mut app = app();
    app.inspector_open = false;
    app.grid = false;
    let ids = chain(&mut app.tab.doc, -170., 0.);
    app.tab.selected = ids;
    let directory = std::path::Path::new("docs/images/reaction-selection-shortcuts");
    std::fs::create_dir_all(directory).unwrap();
    let fixtures = std::path::Path::new("docs/changes/fixtures");
    std::fs::create_dir_all(fixtures).unwrap();
    std::fs::write(
        fixtures.join("reaction-shortcuts-input.rsk"),
        serde_json::to_vec_pretty(&app.tab.doc).unwrap(),
    )
    .unwrap();
    let _ = app.update(Message::Shortcut(Action::ReactionCopy));
    assert!(!app.error, "{}", app.status);
    std::fs::write(
        fixtures.join("reaction-shortcuts-output.rsk"),
        serde_json::to_vec_pretty(&app.tab.doc).unwrap(),
    )
    .unwrap();
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    for (name, title) in [
        (
            "reaction-copy",
            "Cmd/Ctrl + Shift + Right: arrow and selected product copy (one Undo)",
        ),
        (
            "space-selection",
            "Space with Select active: recalls the molecule whose terminal atom changed to N",
        ),
    ] {
        if name == "space-selection" {
            let before = app.tab.doc.clone();
            let atom = *app.tab.selected.last().unwrap();
            app.tab.doc.atom_mut(atom).unwrap().element = "N".into();
            app.changed(before);
            app.tab.selected.clear();
            space(&mut app);
        }
        app.status = title.into();
        app.viewport = iced::Size::new(1080., 560.);
        app.fit();
        let size = iced::Size::new(1200., 760.);
        let theme = app.theme();
        let mut view = app.view();
        let mut tree = Tree::new(view.as_widget());
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let mut messages = Vec::new();
        view.as_widget_mut().update(
            &mut tree,
            &iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &iced::Rectangle::with_size(size),
        );
        view.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &iced::advanced::renderer::Style::default(),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &iced::Rectangle::with_size(size),
        );
        let pixels = Headless::screenshot(
            &mut renderer,
            iced::Size::new(1200, 760),
            1.,
            iced::Color::WHITE,
        );
        image::save_buffer(
            directory.join(format!("{name}.png")),
            &pixels,
            1200,
            760,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }

    // Exercise the actual focused input widget. These events must be
    // captured before the app's global ignored-event shortcut subscription.
    let mut input: iced::Element<'_, String> = iced::widget::text_input("", "Sample text")
        .on_input(|value| value)
        .width(300)
        .into();
    let size = iced::Size::new(320., 50.);
    let mut tree = Tree::new(input.as_widget());
    let node = input
        .as_widget_mut()
        .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    let command = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    for event in [
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::Space),
            modified_key: Key::Named(Named::Space),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Space),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: Some(" ".into()),
            repeat: false,
        }),
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::ArrowRight),
            modified_key: Key::Named(Named::ArrowRight),
            physical_key: iced::keyboard::key::Physical::Code(
                iced::keyboard::key::Code::ArrowRight,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers: command | Modifiers::SHIFT,
            text: None,
            repeat: false,
        }),
    ] {
        let mut messages = Vec::new();
        let mut shell = iced::advanced::Shell::new(&mut messages);
        input.as_widget_mut().update(
            &mut tree,
            &event,
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Available(iced::Point::new(20., 15.)),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &iced::Rectangle::with_size(size),
        );
        if matches!(event, iced::Event::Keyboard(_)) {
            assert_eq!(shell.event_status(), iced::event::Status::Captured);
        }
    }
}
