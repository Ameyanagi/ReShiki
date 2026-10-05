use super::*;
use reshiki::document::{Arrow, Point};
use reshiki::engine::{ChemistryEngine, LocalEngine};

#[test]
fn role_edits_are_undoable_and_arrow_reversal_swaps_roles() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let c = app.tab.doc.add_atom("C", Point::new(300., 0.));
    app.tab.doc.arrows.push(Arrow::new(
        4,
        Point::new(100., 0.),
        Point::new(240., 0.),
        Default::default(),
        Default::default(),
    ));
    app.tab.saved = app.tab.doc.clone();
    let original = app.tab.doc.clone();
    app.tab.selected = vec![a];
    let _ = app.update(Message::Reaction(Action::Assign(Role::Reactant)));
    assert_eq!(app.tab.doc.reactions[0].reactants[0].atoms, [a, b]);
    assert!(app.dirty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    app.tab.selected = vec![c];
    let _ = app.update(Message::Reaction(Action::Assign(Role::Product)));
    let ready = app.tab.doc.clone();
    app.tab.selected = vec![4];
    app.sync_arrows();
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    let _ = app.update(Message::ArrowAction(super::super::arrows::Action::Reverse));
    assert_eq!(
        app.tab.doc.reactions[0].reactants,
        ready.reactions[0].products
    );
    assert_eq!(
        app.tab.doc.reactions[0].products,
        ready.reactions[0].reactants
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, ready);
    // An incompatible chemistry edit is rejected as a whole.
    app.tab.doc.add_bond(a, c, 1, "plain");
    app.changed(ready.clone());
    assert!(app.error);
    assert_eq!(app.tab.doc, ready);
    let _ = app.update(Message::Reaction(Action::SelectAll));
    assert_eq!(app.tab.selected.len(), 4);
    let _ = app.update(Message::Reaction(Action::Unlink));
    assert!(app.tab.doc.reactions.is_empty());
    assert_eq!(app.tab.doc.atoms, ready.atoms);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, ready);
}

#[tokio::test]
#[ignore = "Manual GPU snapshots without opening desktop windows"]
async fn reaction_headless_snapshot() {
    use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
    let (mut app, _) = App::new();
    app.tab.doc = LocalEngine::default()
        .execute(Request::import(
            "rsmi",
            "CC(=O)O.CCO>OS(=O)(=O)O>CCOC(C)=O.O",
        ))
        .await
        .unwrap()
        .document
        .unwrap();
    app.tab.busy = false;
    app.status = "Reaction roles ready · Editable molecules and reaction data".into();
    let _ = app.update(Message::Reaction(Action::Open));
    let directory = std::path::Path::new("artifacts/reaction-qa");
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(
        directory.join("esterification.rsk"),
        serde_json::to_vec_pretty(&app.tab.doc).unwrap(),
    )
    .unwrap();
    for (name, width, height) in [("desktop", 1280, 820), ("compact", 1040, 680)] {
        app.viewport = iced::Size::new(width as f32 - 410., height as f32 - 200.);
        app.fit();
        let mut renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        let size = iced::Size::new(width as f32, height as f32);
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
            iced::Size::new(width, height),
            1.,
            iced::Color::WHITE,
        );
        image::save_buffer(
            directory.join(format!("{name}.png")),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
        for event in [
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            view.as_widget_mut().update(
                &mut tree,
                &event,
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Available(iced::Point::new(
                    width as f32 - 240.,
                    height as f32 - 108.,
                )),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
        }
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Reaction(Action::Export("rxn")))),
            "RXN export stays clickable at {width}×{height}"
        );
    }
}
