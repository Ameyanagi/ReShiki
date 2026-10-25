use super::*;

#[cfg(windows)]
#[test]
fn shortcuts_dialog_partial_redraws_preserve_unchanged_pixels() {
    use iced::advanced::{graphics::Viewport, layout, mouse, widget::Tree};
    use resvg::tiny_skia::{Mask, Pixmap};

    let (mut app, _) = App::new();
    app.help_open = true;
    let size = iced::Size::new(1040., 680.);
    let bounds = iced::Rectangle::with_size(size);
    let mut renderer = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(
        iced::Font::default(),
        iced::Pixels(16.),
    ));
    let mut view = app.with_help(Space::new().width(Length::Fill).height(Length::Fill).into());
    let mut tree = Tree::new(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    view.as_widget_mut().update(
        &mut tree,
        &iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        iced::advanced::Layout::new(&node),
        mouse::Cursor::Unavailable,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut Vec::new()),
        &bounds,
    );
    view.as_widget().draw(
        &tree,
        &mut renderer,
        &app.theme(),
        &iced::advanced::renderer::Style::default(),
        iced::advanced::Layout::new(&node),
        mouse::Cursor::Unavailable,
        &bounds,
    );

    // Simulate small hover/caret redraws inside the dialog. The compositor
    // clears only the damaged region, so drawing outside it corrupts the
    // retained pixels even though the dialog's contents have not changed.
    let damage = iced::Rectangle {
        x: 500.,
        y: 300.,
        width: 40.,
        height: 40.,
    };
    let iced::Renderer::Secondary(mut renderer) = renderer else {
        panic!("partial-redraw regression requires the software renderer");
    };
    for scale in [1., 1.25, 2.] {
        let width = (size.width * scale) as u32;
        let height = (size.height * scale) as u32;
        let viewport = Viewport::with_physical_size(iced::Size::new(width, height), scale);
        let mut pixels = Pixmap::new(width, height).unwrap();
        let mut mask = Mask::new(width, height).unwrap();
        renderer.draw(
            &mut pixels.as_mut(),
            &mut mask,
            &viewport,
            &[bounds],
            Color::WHITE,
        );
        let original = pixels.clone();
        for _ in 0..16 {
            renderer.draw(
                &mut pixels.as_mut(),
                &mut mask,
                &viewport,
                &[damage],
                Color::WHITE,
            );
        }
        let physical_damage = damage * scale;
        let changed = pixels
            .pixels()
            .iter()
            .zip(original.pixels())
            .enumerate()
            .filter(|(index, (actual, expected))| {
                let point = iced::Point::new(
                    (*index % width as usize) as f32,
                    (*index / width as usize) as f32,
                );
                !physical_damage.contains(point) && actual != expected
            })
            .count();
        assert_eq!(
            changed, 0,
            "partial redraws changed pixels outside damage at scale {scale}"
        );
    }
}

#[test]
fn help_uses_f1_and_command_question_mark_and_leaves_fit_and_properties() {
    use iced::keyboard::{Key, Modifiers, key::Named};
    assert!(is_shortcut(&Key::Named(Named::F1), Modifiers::empty()));
    let command_shift = Modifiers::COMMAND | Modifiers::SHIFT;
    assert!(is_shortcut(&Key::Character("/".into()), command_shift));
    assert!(is_shortcut(&Key::Character("?".into()), command_shift));
    assert!(!is_shortcut(
        &Key::Character("?".into()),
        Modifiers::empty()
    ));
    assert!(!is_shortcut(&Key::Character("/".into()), Modifiers::SHIFT));
    assert!(!is_shortcut(&Key::Named(Named::F1), Modifiers::ALT));
    // Command+/ stays Fit.
    let key = Key::Character("/".into());
    assert!(!is_shortcut(&key, Modifiers::COMMAND));
    assert!(matches!(
        crate::app::shortcuts::key_message(&key, &key, Modifiers::COMMAND),
        Some(Message::Fit)
    ));
    assert!(matches!(
        crate::app::shortcuts::key_message(&key, &Key::Character("?".into()), command_shift),
        Some(Message::ToggleHelp)
    ));
}

#[test]
fn dismissing_shortcuts_preserves_the_drawing_tool_and_view() {
    let (mut app, _) = App::new();
    app.tool = crate::canvas::Tool::Ring;
    app.inspector_tab = crate::app::InspectorTab::Import;
    app.tab.selected = vec![
        app.tab
            .doc
            .add_atom("O", reshiki::document::Point::default()),
    ];
    app.tab.camera.zoom = 5.;
    let before = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let _ = app.update(Message::ToggleHelp);
    assert!(app.help_open);
    assert_eq!(app.inspector_tab, crate::app::InspectorTab::Import);
    let _ = app.update(Message::Escape);
    assert!(!app.help_open);
    assert_eq!(app.tool, crate::canvas::Tool::Ring);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.camera.zoom, 5.);
    assert_eq!(app.tab.doc, before);
}

fn top(query: &str) -> &'static registry::Entry {
    let results = search::rank(entries(), query, false);
    &entries()[*results
        .first()
        .unwrap_or_else(|| panic!("{query}: no results"))]
}

fn position(query: &str, name: &str) -> usize {
    search::rank(entries(), query, false)
        .iter()
        .position(|i| entries()[*i].name == name)
        .unwrap_or_else(|| panic!("{query}: {name} not found"))
}

#[test]
fn search_ranks_names_keys_and_synonyms() {
    // Whole words in the name come first.
    assert_eq!(top("wedge").name, "Wedge");
    assert_eq!(top("hashed wedge").name, "Hashed wedge");
    assert_eq!(top("boc").name, "Boc");
    assert_eq!(top("undo").name, "Undo / redo");
    // Synonyms, prefixes and in-order letters.
    assert_eq!(top("stereo").scope, registry::Scope::Bond);
    assert_eq!(top("cyclohexane").name, "Saturated ring tool (3–8)");
    assert_eq!(top("naphth").name, "Fuse benzene");
    assert_eq!(top("wdg").name, "Wedge");
    assert_eq!(top("nphth").name, "Fuse benzene");
    // Ranges include their middle keys; uppercase keys are Shift-letter.
    assert_eq!(top("shift+5").name, "Saturated ring tool (3–8)");
    assert_eq!(top("⇧6").name, "Saturated ring tool (3–8)");
    assert_eq!(top("cmd 5").name, "Go to tab");
    assert_eq!(top("⇧p").name, "Phenyl (Ph)");
    assert_eq!(top("shift P").name, "Phenyl (Ph)");
    // Key chords: exact case first, so "r" and "R" differ.
    assert_eq!(top("r").keys, "r");
    assert!(position("r", "Ring tool") < position("r", "Variable R"));
    let toggle = "Toggle saturated / aromatic ring";
    assert_eq!(top("R").name, toggle);
    assert!(position("R", toggle) < position("R", "Ring tool"));
    assert_eq!(top("P").name, "Phenyl (Ph)");
    // Modifier symbols and their words find the same chord.
    for query in ["⇧r", "⇧R", "shift r", "shift+R"] {
        assert_eq!(top(query).name, toggle, "{query}");
    }
    assert_eq!(top("F8").name, "Keyboard drawing");
    assert_eq!(top("F1").name, "Shortcut finder");
    // Every word must match, so more words narrow the results.
    assert!(search::rank(entries(), "zzzz", false).is_empty());
    assert!(
        search::rank(entries(), "copy", false).len()
            > search::rank(entries(), "copy image", false).len()
    );
    assert_eq!(top("copy image").name, "Copy as image");
    // Command chords spell out Shift; " / " separates alternatives, not keys.
    assert_eq!(top("cmd shift c").name, "Copy as image");
    assert_eq!(top("/").name, "Shortcut finder");
    assert!(position("/", "Fit drawing") < 2);
    assert!(
        search::rank(entries(), "/", false)
            .iter()
            .all(|i| entries()[*i].name != "Select tool")
    );
    assert_eq!(top("resize").name, "Resize");
    // An empty query lists the whole registry in order.
    assert_eq!(
        search::rank(entries(), "  ", false),
        (0..entries().len()).collect::<Vec<_>>()
    );
}

#[test]
fn registry_uses_menu_bindings_and_covers_every_context() {
    for scope in [
        registry::Scope::Atom,
        registry::Scope::Bond,
        registry::Scope::Selection,
        registry::Scope::Canvas,
    ] {
        assert!(entries().iter().any(|e| e.scope == scope), "{scope:?}");
    }
    let label = |message| crate::app::shortcuts::label(&message).unwrap();
    let keys = |name: &str| {
        entries()
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name}"))
            .keys
            .clone()
    };
    assert_eq!(keys("Select all"), label(Message::SelectAll));
    assert_eq!(keys("Fit drawing"), label(Message::Fit));
    assert_eq!(keys("Copy"), label(Message::Copy(false)));
    assert!(
        entries()
            .iter()
            .all(|e| !e.keys.is_empty() && !e.name.is_empty())
    );
}

#[test]
fn every_example_renders_a_changed_drawing() {
    let mut count = 0;
    for entry in entries() {
        let Some(example) = entry.example else {
            continue;
        };
        let (before, after) =
            examples::render(example).unwrap_or_else(|error| panic!("{}: {error}", entry.name));
        after.validate().unwrap();
        let mut plain = before.clone();
        for atom in &mut plain.atoms {
            atom.display.highlight = None;
        }
        for bond in &mut plain.bonds {
            bond.highlight = None;
        }
        assert_ne!(plain, after, "{} changes nothing", entry.name);
        count += 1;
    }
    assert!(count > 40, "{count} examples");
}

#[test]
fn keyboard_navigation_moves_within_results_and_never_runs_commands() {
    use iced::keyboard::{Key, Modifiers, key::Named};
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    let tool = app.tool;
    let _ = app.update(Message::ToggleHelp);
    assert!(app.help_open);
    assert_eq!(
        app.help.selected(app.help_contextual()).unwrap().name,
        entries()[0].name
    );

    let key = |named| modal_key(&Key::Named(named), Modifiers::empty()).unwrap();
    assert!(matches!(key(Named::Escape), Message::ToggleHelp));
    assert!(modal_key(&Key::Character("r".into()), Modifiers::empty()).is_none());
    assert!(modal_key(&Key::Named(Named::ArrowDown), Modifiers::SHIFT).is_none());

    // Up at the top stays; Down moves through the ranked results.
    let _ = app.update(key(Named::ArrowUp));
    assert_eq!(
        app.help.selected(app.help_contextual()).unwrap().name,
        entries()[0].name
    );
    let _ = app.update(key(Named::ArrowDown));
    let _ = app.update(key(Named::ArrowDown));
    assert_eq!(
        app.help.selected(app.help_contextual()).unwrap().name,
        entries()[2].name
    );

    // A query selects its best match; arrows follow the new ranking and
    // stop at the end.
    let _ = app.update(Message::Help(Action::Query("ring".into())));
    let results = app.help.results(app.help_contextual());
    assert_eq!(app.help.position(&results), Some(0));
    let _ = app.update(key(Named::ArrowDown));
    assert_eq!(app.help.position(&results), Some(1));
    for _ in 0..20 {
        let _ = app.update(key(Named::PageDown));
    }
    assert_eq!(app.help.position(&results), Some(results.len() - 1));
    let _ = app.update(key(Named::PageUp));
    assert_eq!(app.help.position(&results), Some(results.len() - 9));

    // Enter and click select a row; selection only previews it.
    let _ = app.update(Message::Help(Action::Query("wedge".into())));
    let wedge = entries().iter().position(|e| e.name == "Wedge").unwrap();
    let _ = app.update(Message::Help(Action::Select(wedge)));
    assert_eq!(
        app.help.selected(app.help_contextual()).unwrap().name,
        "Wedge"
    );
    let _ = app.update(Message::Help(Action::Query("zzzz".into())));
    assert!(app.help.selected(app.help_contextual()).is_none());
    let _ = app.update(key(Named::ArrowDown));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tool, tool);

    // Escape closes; reopening starts a fresh search.
    let _ = app.update(Message::Escape);
    assert!(!app.help_open);
    let _ = app.update(Message::ToggleHelp);
    assert!(app.help_open && app.help.query.is_empty());
}

#[test]
fn canvas_tool_keys_say_when_keyboard_drawing_takes_them() {
    // Keyboard drawing is on by default, and at an empty hotspot these keys
    // start a drawing instead of choosing a tool, so their rows must say so.
    // "⇧" on macOS, "Shift+" elsewhere.
    let shifted = crate::app::shortcuts::keys(Modifiers::SHIFT, "E");
    let shift = shifted.strip_suffix('E').unwrap_or(&shifted);
    for entry in entries()
        .iter()
        .filter(|e| e.scope == registry::Scope::Canvas)
    {
        let taken = entry.keys.split(" / ").any(|key| {
            let key = key.strip_prefix(shift).unwrap_or(key);
            key.chars().count() == 1 && crate::app::keyboard_drawing::starts_drawing(key)
        });
        assert_eq!(
            taken,
            entry.description.contains("keyboard drawing off"),
            "{}",
            entry.name
        );
    }
}

#[test]
fn shift_search_keeps_modifiers_with_their_key() {
    let named = |name: &str| entries().iter().find(|e| e.name == name).unwrap();
    // In "C / l" Shift belongs to C, so Shift+L is Lithium, not Chlorine.
    assert!(search::score(named("Lithium"), "shift l").is_some());
    assert!(search::score(named("Chlorine"), "shift l").is_none());
    assert!(search::score(named("Chlorine"), "shift c").is_some());
    // A bare key shares the modifiers written before it; other alternatives
    // keep their own.
    assert!(search::score(named("Align / distribute"), "cmd shift alt c").is_some());
    let copy = named("Copy CDXML / chemical text / MOL");
    assert!(search::score(copy, "cmd d").is_some());
    assert!(search::score(copy, "cmd alt d").is_none());
    // "+" separates a modifier symbol as it does a modifier word.
    for query in ["⇧+R", "shift+R"] {
        assert_eq!(
            top(query).name,
            "Toggle saturated / aromatic ring",
            "{query}"
        );
    }
    assert_eq!(top("=").name, "Edit atom label");
}

#[test]
fn help_ranks_current_mode_after_score_and_preserves_empty_order() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::ToggleHelp);
    assert!(app.help_open && app.help_contextual());
    assert!(
        !app.keyboard_drawing_active(),
        "Help itself blocks drawing input"
    );
    for tool in [crate::canvas::Tool::Select, crate::canvas::Tool::Lasso] {
        app.tool = tool;
        for (query, expected) in [
            ("2", "Add carbonyl"),
            ("double", "Double bond"),
            ("r", "Variable R"),
        ] {
            let _ = app.update(Message::Help(Action::Query(query.into())));
            assert_eq!(
                app.help.selected(app.help_contextual()).unwrap().name,
                expected
            );
        }
        app.help.query = "  ".into();
        assert_eq!(
            app.help.results(app.help_contextual()),
            (0..entries().len()).collect::<Vec<_>>()
        );
    }
    app.tool = crate::canvas::Tool::Bond(1);
    app.help.query = "2".into();
    assert_eq!(
        app.help.selected(app.help_contextual()).unwrap().name,
        "Double / triple / quadruple bond tools"
    );
    app.tool = crate::canvas::Tool::Select;
    let _ = app.update(Message::ToggleHelp);
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Toggle,
    ));
    let _ = app.update(Message::ToggleHelp);
    assert!(!app.help_contextual());
    app.help.query = "2".into();
    assert_eq!(
        app.help.selected(app.help_contextual()).unwrap().name,
        "Double / triple / quadruple bond tools"
    );
}

#[test]
fn mixed_classic_key_rows_keep_their_unrestricted_alternatives_available() {
    let entry = |name| entries().iter().find(|e| e.name == name).unwrap();
    let select = entry("Select tool");
    assert!(search::requires_classic(select, "v"));
    assert!(!search::requires_classic(select, "space"));
    assert!(!search::requires_classic(select, "select"));
    let graphics = entry("Circled plus / p orbital");
    assert!(search::requires_classic(graphics, "shift e"));
    assert!(!search::requires_classic(graphics, "shift g"));
    assert!(!search::requires_classic(graphics, "orbital"));
}

#[test]
fn all_fusion_rows_show_a_bicyclic_ring_example() {
    for name in ["Fuse a ring", "Fuse benzene", "Fuse diene", "Fuse chair"] {
        let entry = entries().iter().find(|e| e.name == name).unwrap();
        let (before, after) = examples::render(entry.example.unwrap()).unwrap();
        assert_eq!((before.atoms.len(), before.bonds.len()), (6, 6), "{name}");
        assert!(after.atoms.len() > 6, "{name}");
        assert_eq!(
            after.bonds.len(),
            after.atoms.len() + 1,
            "{name}: two fused cycles"
        );
        reshiki::chemistry::document::prepare(&after)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn benzene_fusion_example_is_naphthalene() {
    let entry = entries().iter().find(|e| e.name == "Fuse benzene").unwrap();
    let (before, after) = examples::render(entry.example.unwrap()).unwrap();
    assert_eq!((before.atoms.len(), before.bonds.len()), (6, 6));
    assert_eq!((after.atoms.len(), after.bonds.len()), (10, 11));
    let molecule = reshiki::chemistry::document::prepare(&after).unwrap();
    let graph = &molecule.state.graph;
    let properties = reshiki::chemistry::properties(&graph.atom_facts().unwrap()).unwrap();
    assert_eq!(properties.formula, "C10H8");
    assert_eq!(
        reshiki::chemistry::rings::perceive(graph, Default::default())
            .unwrap()
            .atoms
            .len(),
        2
    );
}

/// Actual Help widgets and shortcut examples, on the host's native font/key
/// conventions. CI retains Windows captures; this is renderer evidence, not
/// native operating-system input acceptance.
#[tokio::test]
#[ignore = "Opt-in Help renderer evidence; set RESHIKI_HELP_QA_DIR"]
async fn help_renderer_evidence() {
    use iced::advanced::{Layout, Renderer as _, layout, mouse, renderer::Headless, widget::Tree};
    let directory = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_HELP_QA_DIR").expect("Set RESHIKI_HELP_QA_DIR"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless Help renderer");
    std::fs::write(
        directory.join("metadata.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "os": std::env::consts::OS,
            "renderer": renderer.name(),
            "font": reshiki::style::ui_font_family(),
        }))
        .unwrap(),
    )
    .unwrap();
    for (width, height) in [(1280, 820), (1040, 680)] {
        for (name, query, keyboard) in [
            ("empty", "", true),
            ("2", "2", true),
            ("double", "double", true),
            ("naphthalene", "naphthalene", true),
            ("2-classic", "2", false),
        ] {
            let (mut app, _) = App::new();
            app.appearance.mode = crate::appearance::Mode::Light;
            if !keyboard {
                app.tab.keyboard_drawing.leave();
            }
            let _ = app.update(Message::ToggleHelp);
            let _ = app.update(Message::Help(Action::Query(query.into())));
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let theme = app.theme();
            let mut view =
                app.with_help(Space::new().width(Length::Fill).height(Length::Fill).into());
            let mut tree = Tree::new(view.as_widget());
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
            view.as_widget_mut().update(
                &mut tree,
                &iced::Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut Vec::new()),
                &bounds,
            );
            renderer.reset(bounds);
            view.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: theme.palette().text,
                },
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &bounds,
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.,
                Color::WHITE,
            );
            image::save_buffer(
                directory.join(format!("help-{name}-{width}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
