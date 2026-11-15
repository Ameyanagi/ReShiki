use super::*;

/// Every inspector tab shares one width, so switching tabs never moves the
/// tab strip under the pointer or reflows the canvas.
#[test]
fn every_inspector_tab_has_the_same_width() {
    use InspectorTab as T;
    let (mut app, _) = App::new();
    let widths: Vec<_> = [
        T::Reactions,
        T::DrawingStyle,
        T::ThemeGenerator,
        T::Assistant,
        T::Pages,
        T::Abbreviations,
        T::Properties,
        T::Predict,
        T::Labels,
        T::Templates,
        T::Import,
        T::Names,
        T::Export,
    ]
    .into_iter()
    .map(|tab| {
        app.inspector_tab = tab;
        app.inspector_width()
    })
    .collect();
    assert!(widths.iter().all(|&width| width == 320.), "{widths:?}");
}

/// The atom buttons the window shows without scrolling at the default size.
#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn every_atom_button_is_visible_at_the_default_window_size() -> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let (app, _) = App::new();
    // Shorter windows scroll to the atoms; see the next test.
    let size = iced::Size::new(1280., 848.);
    let mut ui = iced_runtime::UserInterface::build(
        app.view(),
        size,
        iced_runtime::user_interface::Cache::new(),
        &mut renderer,
    );
    let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
    ui.operate(&renderer, &mut operation::black_box(&mut collect));
    let nodes = &collect.snapshot().nodes;
    let atoms: Vec<_> = nodes
        .iter()
        .filter(|node| node.id.starts_with("element-"))
        .collect();
    assert_eq!(atoms.len(), 11);
    let hidden: Vec<_> = atoms
        .iter()
        .filter(|node| node.visible_bounds != Some(node.bounds))
        .map(|node| &node.id)
        .collect();
    assert!(hidden.is_empty(), "{hidden:?}");
    assert!(nodes.iter().any(|node| node.id == "help-open"));
    Ok(())
}

/// At the minimum window the tools and atoms scroll as one list: the atoms
/// start below the fold, scroll up with the tools, and the last atom row clears
/// the fade at the end, while the Help button stays where it is.
#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn tools_and_atoms_scroll_together_at_the_minimum_window() -> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let (app, _) = App::new();
    let size = iced::Size::new(1040., 680.);
    let mut ui = iced_runtime::UserInterface::build(
        app.view(),
        size,
        iced_runtime::user_interface::Cache::new(),
        &mut renderer,
    );
    fn nodes(
        ui: &mut iced_runtime::UserInterface<'_, Message, Theme, iced::Renderer>,
        renderer: &iced::Renderer,
        size: iced::Size,
    ) -> Vec<reshiki::accessibility::Node> {
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(renderer, &mut operation::black_box(&mut collect));
        collect.snapshot().nodes.clone()
    }
    let find = |nodes: &[reshiki::accessibility::Node], id: &str| {
        nodes
            .iter()
            .find(|node| node.id == id)
            .cloned()
            .ok_or(format!("No {id}"))
    };
    let shown = |node: &reshiki::accessibility::Node| node.visible_bounds == Some(node.bounds);
    let top = nodes(&mut ui, &renderer, size);
    let (select, r_group, help) = (
        find(&top, "tool-Select")?,
        find(&top, "element-R")?,
        find(&top, "help-open")?,
    );
    assert!(shown(&select) && shown(&help));
    assert!(!shown(&r_group), "{r_group:?}");
    ui.update(
        &[iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
            delta: iced::mouse::ScrollDelta::Pixels { x: 0., y: -2000. },
        })],
        iced::mouse::Cursor::Available(select.bounds.center()),
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Vec::new(),
    );
    let end = nodes(&mut ui, &renderer, size);
    let scrolled = select.bounds.y - find(&end, "tool-Select")?.bounds.y;
    assert!(scrolled > 0.);
    assert_eq!(
        r_group.bounds.y - find(&end, "element-R")?.bounds.y,
        scrolled
    );
    let atoms: Vec<_> = end
        .iter()
        .filter(|node| node.id.starts_with("element-"))
        .collect();
    assert!(atoms.iter().all(|node| shown(node)), "{atoms:?}");
    assert_eq!(find(&end, "help-open")?.bounds, help.bounds);
    // The last atom row sits above the fade, which ends at the scroll area's bottom.
    let r_group = find(&end, "element-R")?;
    assert!(r_group.bounds.y + r_group.bounds.height + PALETTE_FADE <= help.bounds.y - 10.);
    Ok(())
}

/// The ATOMS palette's R button draws `*` atoms labeled R by click, by
/// replacing an atom, and by dragging a bond from an atom, each one Undo step.
#[test]
fn the_r_group_button_places_labeled_dummy_atoms() -> Result<(), String> {
    use reshiki::document::Point;
    let (mut app, _) = App::new();
    let source = app.tab.doc.add_atom("C", Point::default());
    let initial = app.tab.doc.clone();
    let _ = app.update(Message::Element("R".into()));
    assert_eq!(app.tool, Tool::Atom);
    let r_group = |app: &App, id| {
        let atom = app.tab.doc.atom(id).ok_or("R group")?;
        Ok::<_, String>((atom.element.clone(), atom.display.variable.clone()))
    };
    let expected = ("*".to_string(), Some("R".to_string()));
    app.edit(crate::canvas::Edit::Bond(
        Point::default(),
        Point::new(42., 0.),
        Some(source),
        None,
    ));
    let added = app.tab.doc.atoms.last().ok_or("Added")?.id;
    assert_eq!(app.tab.doc.atom(source).ok_or("Source")?.element, "C");
    assert_eq!(r_group(&app, added)?, expected);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
    app.edit(crate::canvas::Edit::Click(Point::new(200., 0.)));
    let placed = app.tab.doc.atoms.last().ok_or("Placed")?.id;
    assert_eq!(r_group(&app, placed)?, expected);
    let _ = app.update(Message::Undo);
    app.edit(crate::canvas::Edit::Click(Point::default()));
    assert_eq!(app.tab.doc.atoms.len(), 1);
    assert_eq!(r_group(&app, source)?, expected);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
    Ok(())
}

/// A bond drag with R keeps existing endpoints: connecting two atoms, then
/// dragging again along their bond, leaves both elements and labels alone.
#[test]
fn the_r_group_button_keeps_existing_bond_endpoints() -> Result<(), String> {
    use reshiki::document::Point;
    let (mut app, _) = App::new();
    let carbon = app.tab.doc.add_atom("C", Point::default());
    let oxygen = app.tab.doc.add_atom("O", Point::new(42., 0.));
    let _ = app.update(Message::Element("R".into()));
    let endpoints = |app: &App| {
        [carbon, oxygen].map(|id| {
            app.tab
                .doc
                .atom(id)
                .map(|atom| (atom.element.clone(), atom.display.variable.clone()))
        })
    };
    let unchanged = [Some(("C".to_string(), None)), Some(("O".to_string(), None))];
    for _ in 0..2 {
        app.edit(crate::canvas::Edit::Bond(
            Point::default(),
            Point::new(42., 0.),
            Some(carbon),
            Some(oxygen),
        ));
        assert!(!app.error, "{}", app.status);
        assert_eq!(endpoints(&app), unchanged);
        assert_eq!(app.tab.doc.atoms.len(), 2);
        assert_eq!(app.tab.doc.bonds.len(), 1);
    }
    Ok(())
}

/// A user abbreviation named R does not take over the R button: it still
/// places one `*` atom labeled R. The catalog is process-wide, so the check
/// runs in its own process.
#[test]
fn the_r_group_button_ignores_a_user_abbreviation_named_r() {
    let directory = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "app::workspace::side_panel_tests::r_group_with_user_abbreviation_child",
            "--ignored",
            "--nocapture",
        ])
        .env("RESHIKI_DATA_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "Runs only in the isolated R group test subprocess"]
fn r_group_with_user_abbreviation_child() {
    use reshiki::document::Point;
    let errors = reshiki::chemistry::abbreviations::set_user(Some(
        r#"{"format": 1, "groups": [{"label": "R", "smiles": "*CC"}]}"#,
    ));
    assert!(errors.is_empty(), "{errors:?}");
    let (mut app, _) = App::new();
    let source = app.tab.doc.add_atom("C", Point::default());
    // Typed in Automatic mode, R now names the user's group.
    let typed =
        reshiki::atom_text::apply(&app.tab.doc, source, "R", reshiki::atom_text::Mode::Auto)
            .unwrap();
    assert!(typed.abbreviation(source).is_some());
    let _ = app.update(Message::Element("R".into()));
    app.edit(crate::canvas::Edit::Click(Point::new(200., 0.)));
    app.edit(crate::canvas::Edit::Bond(
        Point::default(),
        Point::new(42., 0.),
        Some(source),
        None,
    ));
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.doc.abbreviations.is_empty());
    assert_eq!(app.tab.doc.atoms.len(), 3);
    for atom in app.tab.doc.atoms.iter().filter(|atom| atom.id != source) {
        assert_eq!(atom.element, "*");
        assert_eq!(atom.display.variable.as_deref(), Some("R"));
    }
}

/// On a narrow row the selection commands stay in view and the global modes
/// fold into ⋯ first: the row always shows a leading run of the commands.
#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn a_narrow_context_row_keeps_selection_commands_ahead_of_global_modes() -> Result<(), String>
{
    use iced::advanced::{renderer::Headless, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let (mut app, _) = App::new();
    let _ = app.update(Message::KeyboardDrawing(
        crate::app::keyboard_drawing::Action::Leave,
    ));
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    let commands = app.context_commands();
    let labels: Vec<_> = commands.iter().map(|c| c.label).collect();
    assert_eq!(
        labels[labels.len() - 2..],
        ["Keyboard drawing", "3D optimize…"]
    );
    let names: Vec<_> = commands.iter().map(|c| c.menu).collect();
    let mut partly_folded = 0;
    for width in (360..=1200).step_by(40) {
        let size = iced::Size::new(width as f32, 46.);
        let mut ui = iced_runtime::UserInterface::build(
            app.context_bar(),
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        let nodes = &collect.snapshot().nodes;
        let shown: Vec<_> = nodes
            .iter()
            .filter(|node| names.contains(&node.name.as_str()))
            .map(|node| node.name.as_str())
            .collect();
        eprintln!("{width} px: {shown:?}");
        assert_eq!(shown, names[..shown.len()], "{width} px");
        let more = nodes.iter().find(|node| node.id == "context-overflow");
        assert_eq!(more.is_some(), shown.len() < names.len(), "{width} px");
        if let Some(more) = more
            && !shown.is_empty()
        {
            partly_folded += 1;
            assert!(more.name.contains("3D optimize…"), "{}", more.name);
            assert!(!more.name.contains("Join"), "{width} px: {}", more.name);
        }
    }
    assert!(partly_folded > 0);
    Ok(())
}

/// Each 12 px tab label fits its segment with room to spare.
#[test]
fn inspector_tab_labels_fit_their_segments() {
    let strip =
        INSPECTOR_WIDTH - 2. * SEGMENT_MARGIN - (INSPECTOR_TABS.len() as f32 + 1.) * SEGMENT_GAP;
    let portions: u16 = INSPECTOR_TABS
        .iter()
        .map(|(label, _)| segment_portion(label))
        .sum();
    for (label, _) in INSPECTOR_TABS {
        let segment = strip * f32::from(segment_portion(label)) / f32::from(portions);
        let width = text_width(label, 12.);
        eprintln!("{label}: {width:.1} of {segment:.1} px");
        assert!(width + 6. <= segment, "{label}: {width} px in {segment} px");
    }
}
