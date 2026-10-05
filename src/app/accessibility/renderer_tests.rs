use super::*;
use iced::advanced::{renderer::Headless, widget::operation};
use iced_runtime::{UserInterface, user_interface::Cache};

fn snapshot(app: &App, renderer: &mut iced::Renderer, size: iced::Size) -> Snapshot {
    let mut ui = UserInterface::build(app.view(), size, Cache::new(), renderer);
    let mut collect = Collect::new(iced::Rectangle::with_size(size));
    ui.operate(renderer, &mut operation::black_box(&mut collect));
    let snapshot = collect.snapshot().clone();
    assert!(
        snapshot.duplicate_ids.is_empty(),
        "{:?}",
        snapshot.duplicate_ids
    );
    NativeTree::default()
        .update(&snapshot, "ReShiki", iced::Rectangle::with_size(size), 2.)
        .expect("the native consumer receives a valid live tree");
    snapshot
}

#[tokio::test]
#[ignore = "Opt-in real App widget tree and overlay check"]
async fn changed_surfaces_expose_live_controls_and_mask_foreground_backgrounds() {
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    for size in [iced::Size::new(1280., 820.), iced::Size::new(1040., 680.)] {
        let (mut app, _) = App::new();
        app.viewport = size;
        let a = app
            .tab
            .doc
            .add_atom("C", reshiki::document::Point::default());
        let b = app
            .tab
            .doc
            .add_atom("O", reshiki::document::Point { x: 42., y: 42. });
        app.tab.selected = vec![a, b];
        app.sync_numeric_transforms();
        app.tab
            .inspector_ui
            .update(super::super::inspector::Action::Section(
                super::super::inspector::Section::Transform,
                true,
            ));
        let tree = snapshot(&app, &mut renderer, size);
        for id in [
            "header-new",
            "header-export",
            "help-open",
            "transform-rotation",
            "transform-width",
            "transform-more",
            "transform-proportions",
            "transform-apply",
        ] {
            assert!(tree.nodes.iter().any(|node| node.id == id), "missing {id}");
        }
        assert!(
            tree.nodes
                .iter()
                .find(|node| node.id == "transform-width")
                .unwrap()
                .name
                .contains("pt")
        );
        assert_eq!(
            tree.nodes
                .iter()
                .find(|node| node.id == "transform-more")
                .unwrap()
                .expanded,
            Some(false)
        );
        assert!(!tree.nodes.iter().any(|node| node.id == "transform-tilt-x"));
        app.help_open = true;
        let help = snapshot(&app, &mut renderer, size);
        assert_eq!(help.nodes.len(), 3);
        assert!(help.nodes.iter().all(|node| node.id.starts_with("help-")));
        app.help_open = false;
        let _ = app.context_action(super::super::context_menu::Action::Open(
            super::super::context_menu::Page::Arrange,
            200.,
        ));
        let menu = snapshot(&app, &mut renderer, size);
        assert!(!menu.nodes.is_empty());
        assert!(menu.nodes.iter().all(|node| node.id.starts_with("menu-")));
        app.context_menu = None;
        app.inspector_tab = super::super::InspectorTab::Export;
        app.tab
            .inspector_ui
            .update(super::super::inspector::Action::FigureMenu(true));
        let formats = snapshot(&app, &mut renderer, size);
        assert!(!formats.nodes.is_empty());
        assert!(
            formats
                .nodes
                .iter()
                .all(|node| node.id.starts_with("figure-format-"))
        );
        app.tab.inspector_ui.close_menu();
        app.tab
            .inspector_ui
            .update(super::super::inspector::Action::Section(
                super::super::inspector::Section::ExportChemical,
                true,
            ));
        app.tab
            .inspector_ui
            .update(super::super::inspector::Action::ChemicalMenu(true));
        let chemicals = snapshot(&app, &mut renderer, size);
        assert_eq!(chemicals.nodes.len(), 4);
        assert!(
            chemicals
                .nodes
                .iter()
                .all(|node| node.id.starts_with("chemical-format-"))
        );
        app.tab.inspector_ui.close_menu();
        app.inspector_tab = super::super::InspectorTab::Import;
        app.imports.set_text("CCO");
        app.imports.examples_menu = true;
        let examples = snapshot(&app, &mut renderer, size);
        assert_eq!(examples.nodes.len(), 4);
        assert!(
            examples
                .nodes
                .iter()
                .all(|node| node.id.starts_with("import-example-"))
        );
        app.imports.examples_menu = false;
        app.imports.menu = true;
        let insert = snapshot(&app, &mut renderer, size);
        assert_eq!(insert.nodes.len(), 1);
        assert_eq!(insert.nodes[0].id, "import-replace");
        app.imports.menu = false;
        let import = snapshot(&app, &mut renderer, size);
        assert!(
            import.nodes.iter().any(|node| node.id == "import-input"
                && node.role == reshiki::accessibility::Role::TextArea)
        );
        let _ = app.style_menu_action(super::super::color_popover::Action::Color);
        let color = snapshot(&app, &mut renderer, size);
        assert!(color.nodes.iter().any(|node| node.id == "color-input"));
        assert!(
            color
                .nodes
                .iter()
                .all(|node| node.id == "color-input" || node.id == "style-color-apply")
        );
        app.style_menu = None;
        let _ = app.inline_action(super::super::inline_text::Action::Begin(
            None,
            reshiki::document::Point::default(),
        ));
        let caption = snapshot(&app, &mut renderer, size);
        assert!(caption.nodes.iter().any(|node| node.id == "inline-caption"
            && node.role == reshiki::accessibility::Role::TextArea));
        app.finish_inline(false);
        app.inspector_tab = super::super::InspectorTab::Properties;
        app.tool = crate::canvas::Tool::Graphic(reshiki::graphics::GraphicKind::Arc);
        let arcs = snapshot(&app, &mut renderer, size);
        assert!(arcs.nodes.iter().any(|node| node.id == "arc-start"));
        assert!(arcs.nodes.iter().any(|node| node.id == "arc-sweep"));
        assert!(
            arcs.nodes
                .iter()
                .any(|node| node.id.starts_with("arc-preset-context-"))
        );
        assert!(
            arcs.nodes
                .iter()
                .any(|node| node.id.starts_with("arc-preset-inspector-"))
        );
    }
}
