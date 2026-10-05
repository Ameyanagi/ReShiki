use super::*;
use reshiki::document::{Document, Point};
use reshiki::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle};

fn fixture() -> App {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    for (x, y, element) in [(40., 40., "O"), (190., 100., "N"), (360., 65., "Cl")] {
        let a = app.tab.doc.add_atom("C", Point::new(x, y));
        let b = app.tab.doc.add_atom(element, Point::new(x + 40., y));
        app.tab.doc.add_bond(a, b, 1, "plain");
    }
    app.tab.selected = app.tab.doc.all_ids();
    app
}

#[test]
fn controls_count_molecules_and_groups_and_require_supported_layers() {
    let mut app = fixture();
    assert_eq!(app.alignment_count(), 3);
    assert!(Command::Align(Arrange::DistributeHorizontal).enabled(&app, app.alignment_count()));
    app.tab.selected.truncate(4);
    assert_eq!(app.alignment_count(), 2);
    assert!(Command::Align(Arrange::AlignLeft).enabled(&app, app.alignment_count()));
    assert!(!Command::Align(Arrange::DistributeHorizontal).enabled(&app, app.alignment_count()));
    app.tab.selected.truncate(2);
    assert_eq!(app.alignment_count(), 1);
    assert!(!Command::Align(Arrange::AlignLeft).enabled(&app, app.alignment_count()));
    assert!(Command::Layer(true).enabled(&app, 1));
    app.tab.selected.truncate(1);
    assert!(!Command::Layer(true).enabled(&app, 1));
    assert!(Command::Rotate.enabled(&app, 1));
    app.tab.selected.clear();
    assert!(!Command::Reflect(true).enabled(&app, 0));
    assert!(!Command::Rotate.enabled(&app, 0));

    let mut grouped = fixture();
    grouped
        .tab
        .doc
        .group_selection(&grouped.tab.selected[..4])
        .unwrap();
    assert_eq!(grouped.alignment_count(), 2);
    assert!(Command::Align(Arrange::AlignLeft).enabled(&grouped, grouped.alignment_count()));
    assert!(
        !Command::Align(Arrange::DistributeHorizontal).enabled(&grouped, grouped.alignment_count())
    );
    grouped
        .tab
        .doc
        .group_selection(&grouped.tab.selected)
        .unwrap();
    assert_eq!(grouped.alignment_count(), 1);
    assert!(!Command::Align(Arrange::AlignLeft).enabled(&grouped, grouped.alignment_count()));
}

#[test]
fn arrange_setting_roundtrips_and_ignores_the_retired_toolbar_key() {
    let mut app = fixture();
    let before = app.tab.doc.clone();
    for visible in [false, true] {
        let _ = app.update(Message::ObjectToolbar(Action::Visible(visible)));
        assert_eq!(app.appearance.arrange_controls, visible);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let bytes = serde_json::to_vec(&app.appearance).unwrap();
        let reloaded: crate::appearance::Settings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reloaded.arrange_controls, visible);
    }
    for legacy in [
        r#"{"mode":"dark"}"#,
        r#"{"mode":"dark","object_toolbar":false}"#,
    ] {
        let legacy: crate::appearance::Settings = serde_json::from_str(legacy).unwrap();
        assert!(legacy.arrange_controls);
        assert_eq!(legacy.mode, crate::appearance::Mode::Dark);
    }
}

#[test]
fn mixed_layers_are_one_undo_step_and_survive_save_reopen() {
    let mut app = fixture();
    let id = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        Point::new(10., 10.),
        Point::new(100., 100.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    app.tab.selected = vec![app.tab.doc.atoms[0].id, app.tab.doc.atoms[1].id, id];
    let before = app.tab.doc.clone();
    let _ = app.update(Command::Layer(true).message());
    assert_eq!(app.tab.doc.graphics[0].layer, 1);
    assert_eq!(app.tab.doc.bonds[0].z_order, 1);
    assert_eq!(app.tab.doc.bonds[1].z_order, 0);
    let after = app.tab.doc.clone();
    let reopened: Document = serde_json::from_slice(&serde_json::to_vec(&after).unwrap()).unwrap();
    assert_eq!(reopened, after);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
    let _ = app.update(Command::Layer(false).message());
    assert_eq!(app.tab.doc.graphics[0].layer, -1);
    assert_eq!(app.tab.doc.bonds[0].z_order, -2);
}

#[test]
fn bond_depth_changes_only_bonds() {
    let mut app = fixture();
    let id = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        Point::new(10., 10.),
        Point::new(100., 100.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    app.tab.selected.push(id);
    let graphics = app.tab.doc.graphics.clone();
    let _ = app.update(Message::BondDepth(true));
    assert!(app.tab.doc.bonds.iter().all(|b| b.z_order == 1));
    assert_eq!(app.tab.doc.graphics, graphics);
}

#[test]
fn toolbar_arrangement_and_transforms_use_standard_undo() {
    for command in [
        Command::Align(Arrange::AlignTop),
        Command::Align(Arrange::DistributeHorizontal),
        Command::Reflect(true),
        Command::Reflect(false),
        Command::Rotate,
    ] {
        let mut app = fixture();
        let before = app.tab.doc.clone();
        let _ = app.update(command.message());
        assert_ne!(app.tab.doc, before);
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer layout and pointer checks"]
async fn arrange_buttons_match_their_menu_anchors_in_a_fixed_height_row() {
    use iced::advanced::{Layout, Shell, clipboard, layout, mouse, renderer::Headless};
    use iced::{Event, Rectangle, Size};
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    // Canvas widths at 1040 and 1280 with the default inspector.
    for (width, selected) in [(636., true), (636., false), (876., true)] {
        let mut app = fixture();
        if !selected {
            app.tab.selected.clear();
        }
        let size = Size::new(width, 80.);
        let left = 14. + (width - 28.) - GROUP_WIDTH;
        let click = |x: f32| {
            let mut bar = app.context_bar();
            let mut tree = iced::advanced::widget::Tree::new(bar.as_widget());
            let node = bar.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            assert_eq!(node.size().height, 46., "Row height is fixed");
            let mut messages = Vec::new();
            for event in [mouse::Event::ButtonPressed, mouse::Event::ButtonReleased] {
                bar.as_widget_mut().update(
                    &mut tree,
                    &Event::Mouse(event(mouse::Button::Left)),
                    Layout::new(&node),
                    mouse::Cursor::Available(iced::Point::new(x, 23.)),
                    &renderer,
                    &mut clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &Rectangle::with_size(size),
                );
            }
            messages
        };
        let align = click(left + MENU_BUTTON / 2.);
        let flip = click(left + 3. * (MENU_BUTTON + GAP) + 11. + GAP + ICON_BUTTON / 2.);
        if !selected {
            assert!(
                align.is_empty() && flip.is_empty(),
                "Disabled without a selection"
            );
            continue;
        }
        assert!(matches!(
            align.as_slice(),
            [Message::ContextMenu(context_menu::Action::Open(Page::AlignObjects, x))]
                if (x - left).abs() < 0.5
        ));
        assert!(matches!(
            flip.as_slice(),
            [Message::Transform(Transform::FlipHorizontal)]
        ));
    }
}
