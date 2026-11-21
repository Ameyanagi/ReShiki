use super::*;
use reshiki::document::{Arrow, Point};
use reshiki::graphics::{Graphic, GraphicKind};

fn fixture() -> (App, [u64; 3], u64, u64) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let atoms = [
        app.tab.doc.add_atom("C", Point::default()),
        app.tab.doc.add_atom("C", Point::new(42., 0.)),
        app.tab.doc.add_atom("O", Point::new(84., 0.)),
    ];
    app.tab.doc.add_bond(atoms[0], atoms[1], 1, "plain");
    app.tab.doc.add_bond(atoms[1], atoms[2], 1, "plain");
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(150., 0.),
        Point::new(250., 0.),
        Default::default(),
        Default::default(),
    ));
    let graphic = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        graphic,
        GraphicKind::Rectangle,
        Point::new(0., 80.),
        Point::new(100., 130.),
        Default::default(),
        Default::default(),
        false,
    ));
    (app, atoms, arrow, graphic)
}

#[test]
fn explicit_fragment_properties_return_to_context_and_keep_identity_section_choice() {
    let (mut app, atoms, _, graphic) = fixture();
    app.tab.inspector_ui.seed_molecular(Some(false));
    app.set_selection(atoms[..2].to_vec());
    let drawing = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let _ = app.show_molecular_properties();
    assert!(app.requested_molecular_properties());
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.selected, selected);
    let _ = app.inspector_action(Action::Section(Section::RequestedMolecule, false));
    assert!(
        app.requested_molecular_properties(),
        "The requested card can stay collapsed"
    );
    app.set_selection(vec![graphic]);
    assert!(!app.requested_molecular_properties());
    assert_eq!(app.tab.inspector_ui.expanded(Section::Molecule), Some(true));
    app.set_selection(atoms[..2].to_vec());
    let _ = app.context_action(crate::app::context_menu::Action::Properties(true));
    assert!(app.requested_molecular_properties());
    app.select_tool(Tool::Bond(1));
    assert!(!app.requested_molecular_properties());
    assert!(!app.tab.history.can_undo());
}

#[test]
fn requested_analysis_survives_caption_pins_and_their_undo_redo() {
    use reshiki::linked_properties::{self, Property};
    for fragment in [false, true] {
        let (mut app, atoms, _, _) = fixture();
        if fragment {
            app.set_selection(atoms[..2].to_vec());
        }
        let selected = app.tab.selected.clone();
        let _ = app.show_molecular_properties();
        for property in [Property::Formula, Property::Weight] {
            let _ = app.update(Message::ToggleProperty(atoms.to_vec(), property));
            assert!(!app.error, "{}", app.status);
            assert!(linked_properties::shown(&app.tab.doc, &atoms, property));
            assert!(
                app.requested_molecular_properties(),
                "{fragment}: {property:?}"
            );
            assert_eq!(app.tab.selected, selected);
        }
        let pinned = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert!(app.requested_molecular_properties());
        assert!(!linked_properties::shown(
            &app.tab.doc,
            &atoms,
            Property::Weight
        ));
        let _ = app.update(Message::Redo);
        assert!(app.requested_molecular_properties());
        assert_eq!(app.tab.doc, pinned);
        assert_eq!(app.tab.selected, selected);
    }
}

#[test]
fn requested_analysis_ends_on_right_click_target_changes_and_does_not_return() {
    let (mut app, atoms, _, _) = fixture();
    app.set_selection(atoms[..2].to_vec());
    let _ = app.show_molecular_properties();
    let drawing = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(crate::canvas::Edit::ContextMenu {
        position: iced::Point::new(200., 200.),
        selected: vec![atoms[2]],
        hit: vec![atoms[2]],
    }));
    assert!(!app.requested_molecular_properties());
    let _ = app.update(Message::Escape); // Close the context menu first.
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(
        atoms[..2].to_vec(),
    )));
    assert!(!app.requested_molecular_properties());
    assert_eq!(app.tab.doc, drawing);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn requested_analysis_ends_when_its_atom_scope_or_file_changes() {
    let (mut app, atoms, _, _) = fixture();
    let _ = app.show_molecular_properties();
    app.tab.doc.add_atom("N", Point::new(320., 0.));
    let _ = app.update(Message::Viewport(iced::Size::new(1000., 700.)));
    assert!(!app.requested_molecular_properties());

    app.set_selection(atoms[..2].to_vec());
    let _ = app.show_molecular_properties();
    app.set_selection(vec![atoms[1], atoms[0]]);
    assert!(
        app.requested_molecular_properties(),
        "Reordering IDs keeps the target"
    );
    app.tab.file_epoch += 1;
    let _ = app.update(Message::Viewport(iced::Size::new(1000., 700.)));
    assert!(!app.requested_molecular_properties());
}

#[test]
fn requested_analysis_escape_and_close_preserve_selection_and_document() {
    for fragment in [false, true] {
        let (mut app, atoms, _, _) = fixture();
        if fragment {
            app.set_selection(atoms[..2].to_vec());
        }
        let drawing = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        for close in [
            Message::Escape,
            Message::InspectorAction(Action::DismissMolecule),
        ] {
            let _ = app.show_molecular_properties();
            assert_eq!(app.tool, Tool::Select);
            let _ = app.update(close);
            assert!(!app.requested_molecular_properties());
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.selected, selected);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[derive(Default)]
struct PanelText(Vec<String>);
impl iced::advanced::widget::Operation for PanelText {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        operate(self);
    }
    fn text(&mut self, _: Option<&iced::advanced::widget::Id>, _: iced::Rectangle, value: &str) {
        self.0.push(value.to_owned());
    }
}

#[tokio::test]
#[ignore = "Opt-in actual Properties renderer context and Delete-last regression"]
async fn rendered_properties_follow_selection_and_tool_defaults() -> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let snapshot = |app: &App, renderer: &mut iced::Renderer| {
        let size = iced::Size::new(300., 3000.);
        let mut ui = iced_runtime::UserInterface::build(
            app.properties_panel(),
            size,
            iced_runtime::user_interface::Cache::new(),
            renderer,
        );
        let mut words = PanelText::default();
        ui.operate(renderer, &mut operation::black_box(&mut words));
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(renderer, &mut operation::black_box(&mut collect));
        let controls = collect.snapshot().clone();
        assert!(
            controls.duplicate_ids.is_empty(),
            "{:?}",
            controls.duplicate_ids
        );
        (words.0, controls)
    };
    let (mut app, atoms, arrow, graphic) = fixture();
    for section in [
        Section::Chemistry,
        Section::DrawingStyle,
        Section::Descriptors,
    ] {
        let _ = app.inspector_action(Action::Section(section, true));
    }
    for (tool, selection, required, forbidden) in [
        (Tool::Select, vec![atoms[0]], "1 atom · C", "Identity"),
        (
            Tool::Select,
            atoms[..2].to_vec(),
            "Bond appearance",
            "Identity",
        ),
        (
            Tool::Graphic(GraphicKind::Ellipse),
            vec![graphic],
            "Rectangle",
            "Arrow tool",
        ),
        (
            Tool::Graphic(GraphicKind::Rectangle),
            vec![arrow],
            "Arrow properties",
            "Graphic style",
        ),
    ] {
        app.tool = tool;
        app.set_selection(selection);
        let (words, controls) = snapshot(&app, &mut renderer);
        let shown = words.join("\n");
        assert!(shown.contains(required), "{shown}");
        assert!(!shown.contains(forbidden), "{shown}");
        assert!(
            !words
                .iter()
                .any(|s| matches!(s.as_str(), "Drawing style" | "Labels & chemistry"))
        );
        assert_eq!(
            words.last().map(String::as_str),
            Some("Delete selection"),
            "{shown}"
        );
        assert!(controls.nodes.iter().any(|n| n.id == "selection-delete"));
        if app.tab.selected == [atoms[0]] {
            assert!(shown.contains("Edit atom label…") && shown.contains("Attachment points"));
        }
    }
    app.tool = Tool::Select;
    app.set_selection(atoms[..2].to_vec());
    let _ = app.show_molecular_properties();
    let (words, _) = snapshot(&app, &mut renderer);
    assert!(words.iter().any(|s| s == "Identity"));
    assert!(!words.iter().any(|s| s.contains("Whole drawing")));
    let _ = app.inspector_action(Action::Section(Section::RequestedMolecule, false));
    let (words, _) = snapshot(&app, &mut renderer);
    assert!(words.iter().any(|s| s == "Identity"));

    // Whole molecular selections retain Identity and its existing descriptor controls.
    app.tool = Tool::Select;
    app.set_selection(atoms.to_vec());
    let key = app.property_request_key();
    let mut analysis = crate::app::tests::completed_analysis();
    analysis.smiles = "CCO".into();
    analysis.display_formula = "C₂H₆O".into();
    analysis.mass = 46.069;
    app.tab.inspector_ui.properties = Some((key, Ok(analysis)));
    let (words, controls) = snapshot(&app, &mut renderer);
    assert!(words.iter().any(|s| s == "Identity"));
    assert!(words.iter().any(|s| s == "Descriptors"));
    assert!(!words.iter().any(|s| s.contains("Whole drawing")));
    assert!(
        controls
            .nodes
            .iter()
            .any(|n| n.id == "identity-copy-formula")
    );
    assert_eq!(words.last().map(String::as_str), Some("Delete selection"));

    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id: caption,
        position: Point::new(20., 180.),
        text: "Selected caption".into(),
        format: Default::default(),
    });
    app.tool = Tool::Text;
    app.set_selection(vec![caption]);
    let (words, _) = snapshot(&app, &mut renderer);
    assert!(words.iter().any(|s| s == "Edit text"));
    assert!(!words.iter().any(|s| s == "Text labels"));
    assert_eq!(words.last().map(String::as_str), Some("Delete selection"));

    app.tool = Tool::Select;
    app.set_selection(vec![]);
    let (words, controls) = snapshot(&app, &mut renderer);
    for heading in [
        "Nothing selected",
        "Drawing",
        "Labels & chemistry",
        "Drawing style",
    ] {
        assert!(words.iter().any(|s| s == heading), "{words:?}");
    }
    assert!(!words.iter().any(|s| s == "Identity"));
    assert!(!controls.nodes.iter().any(|n| n.id == "selection-delete"));
    for (tool, heading, control) in [
        (Tool::Bond(1), "Next bond", "Length"),
        (Tool::Arrow, "Next arrow", "Arrow tool"),
        (
            Tool::Graphic(GraphicKind::Rectangle),
            "Next graphic",
            "Graphic defaults",
        ),
        (Tool::Text, "Next text", "Text labels"),
        (Tool::Ring, "Next ring", "Size"),
        (
            Tool::RingPreset(reshiki::rings::Preset::Benzene),
            "Next ring",
            "Benzene",
        ),
    ] {
        app.tool = tool;
        let (words, controls) = snapshot(&app, &mut renderer);
        let shown = words.join("\n");
        assert_eq!(words.first().map(String::as_str), Some(heading));
        assert!(shown.contains(control), "{shown}");
        for forbidden in [
            "Identity",
            "Labels & chemistry",
            "Drawing style",
            "Delete selection",
        ] {
            assert!(!words.iter().any(|s| s == forbidden), "{shown}");
        }
        assert!(!controls.nodes.iter().any(|n| n.id == "selection-delete"));
    }
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    for tool in [
        Tool::Graphic(GraphicKind::Arc),
        Tool::EditPoints,
        Tool::Ring,
        Tool::Bond(1),
        Tool::Chain(reshiki::chains::ChainMode::Straight),
        Tool::Atom,
        Tool::Arrow,
    ] {
        app.tool = tool;
        let size = iced::Size::new(1280., 820.);
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        assert!(
            collect.snapshot().duplicate_ids.is_empty(),
            "{tool:?}: {:?}",
            collect.snapshot().duplicate_ids
        );
        use reshiki::accessibility::Role;
        let expected: &[(&str, Role)] = match tool {
            Tool::Ring => &[
                ("ring-preset", Role::ComboBox),
                ("ring-size", Role::ComboBox),
                ("aromatic-ring", Role::ToggleButton),
            ],
            Tool::Bond(_) => &[
                ("bond-preset", Role::ComboBox),
                ("bond-length", Role::TextInput),
                ("fixed-length", Role::ToggleButton),
                ("fixed-angles", Role::ToggleButton),
            ],
            Tool::Chain(_) => &[
                ("chain-atoms", Role::TextInput),
                ("chain-angle", Role::TextInput),
                ("bond-length", Role::TextInput),
            ],
            Tool::Atom => &[
                ("element", Role::TextInput),
                ("apply-element", Role::Button),
            ],
            Tool::Arrow => &[("arrow-preset", Role::ComboBox)],
            _ => &[],
        };
        for &(key, role) in expected {
            for scope in ["context", "properties"] {
                let id = format!("{scope}-{key}");
                let node = collect
                    .snapshot()
                    .nodes
                    .iter()
                    .find(|node| node.id == id)
                    .unwrap_or_else(|| panic!("{tool:?}: missing {id}"));
                assert_eq!(node.role, role, "{id}");
                assert!(node.enabled && !node.name.is_empty(), "{id}");
                if role.takes_value() {
                    assert!(node.value.is_some(), "{id}");
                }
            }
        }
    }
    let (doc, model_atoms, _) = crate::canvas::tilt::shown(reshiki::models3d::Display::ThreeD);
    app.tab.doc = doc;
    app.tab.file_epoch += 1;
    app.tool = Tool::Select;
    app.set_selection(model_atoms.to_vec());
    let (_, controls) = snapshot(&app, &mut renderer);
    assert!(
        controls
            .nodes
            .iter()
            .any(|node| node.id == "model-controls-open")
    );
    let _ = app.inspector_action(Action::ModelControls(true));
    let (words, _) = snapshot(&app, &mut renderer);
    assert!(words.iter().any(|s| s == "3D model controls"));
    app.set_selection(vec![]);
    let (words, _) = snapshot(&app, &mut renderer);
    assert!(words.iter().any(|s| s == "Nothing selected"));
    assert!(!words.iter().any(|s| s == "3D model controls"));
    Ok(())
}
