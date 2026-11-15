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
    let mut hidden = Vec::new();
    for size in [iced::Size::new(1280., 848.), iced::Size::new(1040., 680.)] {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        let atoms: Vec<_> = collect
            .snapshot()
            .nodes
            .iter()
            .filter(|node| node.id.starts_with("element-"))
            .collect();
        let visible = atoms
            .iter()
            .filter(|node| node.visible_bounds == Some(node.bounds))
            .count();
        eprintln!(
            "{size:?}: {visible} of {} atom buttons visible",
            atoms.len()
        );
        hidden.push((size, atoms.len() - visible));
        assert!(
            collect
                .snapshot()
                .nodes
                .iter()
                .any(|node| node.id == "help-open")
        );
    }
    assert!(hidden.iter().all(|(_, hidden)| *hidden == 0), "{hidden:?}");
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
