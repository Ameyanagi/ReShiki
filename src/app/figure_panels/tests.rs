use super::*;
use crate::app::App;

fn molecules(app: &mut App, xs: &[f32]) -> Vec<u64> {
    let mut ids = vec![];
    for &x in xs {
        let a = app.tab.doc.add_atom("C", Point::new(x, 0.));
        let b = app.tab.doc.add_atom("O", Point::new(x + 42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        ids.extend([a, b]);
    }
    ids
}

fn panel(app: &App) -> &Graphic {
    app.tab
        .doc
        .graphics
        .iter()
        .find(|g| g.panel.is_some())
        .unwrap()
}

#[test]
fn wrap_title_and_dividers_are_each_one_undo_step_and_keep_the_selection() {
    let (mut app, _) = App::new();
    // The widest gap, 102 to 300, is right of the panel's middle.
    let ids = molecules(&mut app, &[0., 60., 300.]);
    app.tab.selected = ids.clone();
    let initial = app.tab.doc.clone();
    let _ = app.update(Message::Figure(Action::Wrap));
    assert!(!app.error, "{}", app.status);
    let wrapped = app.tab.doc.clone();
    let id = panel(&app).id;
    assert_eq!(app.tab.doc.graphics[0].id, id, "drawn first, behind");
    assert!(panel(&app).layer < 0);
    assert_eq!(app.tab.doc.groups.len(), 1);
    assert!(app.tab.selected.contains(&id));
    // The panel holds its contents with padding on every side.
    let contents = reshiki::graphics::panel::contents(&app.tab.doc, id);
    assert!(ids.iter().all(|a| contents.contains(a)));

    let top = panel(&app).origin.y;
    let height = panel(&app).axis_y.y;
    let _ = app.update(Message::Figure(Action::Title(0, "This work".into())));
    assert_eq!(app.tab.doc, wrapped, "typing alone changes nothing");
    let _ = app.update(Message::Figure(Action::ApplyTitles));
    let titled = app.tab.doc.clone();
    assert_eq!(
        panel(&app).panel.as_ref().unwrap().titles,
        vec!["This work".to_string()]
    );
    // The title band grows the panel upward instead of covering its contents.
    let band = panel(&app).panel_bar_height(&app.tab.doc.drawing_style.text_style());
    assert!(band > 0.);
    assert!((panel(&app).origin.y - (top - band)).abs() < 0.01);
    assert!((panel(&app).axis_y.y - (height + band)).abs() < 0.01);
    let _ = app.update(Message::Figure(Action::Place(TitlePlace::Border)));
    assert!(
        (panel(&app).origin.y - top).abs() < 0.01,
        "no band, no growth"
    );

    let _ = app.update(Message::Figure(Action::AddDivider(true)));
    let at = panel(&app).panel.as_ref().unwrap().dividers[0].at;
    // In the gap between the molecules, not at the middle by default.
    let x = panel(&app).origin.x + at * panel(&app).axis_x.x;
    assert!((x - 201.).abs() < 8., "{x}");
    let _ = app.update(Message::Figure(Action::DividerAt(0, "30%".into())));
    let _ = app.update(Message::Figure(Action::ApplyDivider(0)));
    assert_eq!(panel(&app).panel.as_ref().unwrap().dividers[0].at, 0.3);
    let _ = app.update(Message::Figure(Action::DividerAt(0, "120".into())));
    let _ = app.update(Message::Figure(Action::ApplyDivider(0)));
    assert!(app.error);

    for _ in 0..3 {
        let _ = app.update(Message::Undo);
    }
    assert_eq!(app.tab.doc, titled);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, wrapped);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
}

#[test]
fn a_lasting_layout_is_one_undo_step_and_its_gutter_reflows_it() {
    let (mut app, _) = App::new();
    let ids = molecules(&mut app, &[0., 200.]);
    app.tab.selected = ids.clone();
    let initial = app.tab.doc.clone();
    let _ = app.update(Message::Figure(Action::Arrange(Flow::Column)));
    assert!(!app.error, "{}", app.status);
    assert!(app.selected_layout().is_some());
    let column = app.tab.doc.clone();
    let below = |app: &App| app.tab.doc.atom(ids[2]).unwrap().position.y;
    assert!(below(&app) > 0.);
    let _ = app.update(Message::Figure(Action::Gutter("40".into())));
    let _ = app.update(Message::Figure(Action::ApplyGutter));
    assert!(below(&app) > column.atom(ids[2]).unwrap().position.y);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, column);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
}

#[test]
fn splitting_a_titled_column_keeps_later_titles_with_their_columns() {
    let (mut app, _) = App::new();
    let ids = molecules(&mut app, &[0., 150., 400.]);
    app.tab.selected = ids;
    let _ = app.update(Message::Figure(Action::Wrap));
    let id = panel(&app).id;
    if let Some(p) = app.panel_mut(id) {
        // A divider between the second and third molecules, both titled.
        p.dividers.push(Divider {
            vertical: true,
            at: 0.75,
        });
        p.titles = vec!["Scheme".into(), "Evidence".into()];
    }
    let _ = app.update(Message::Figure(Action::AddDivider(true)));
    let p = panel(&app).panel.as_ref().unwrap();
    assert_eq!(p.columns().len(), 3);
    assert_eq!(
        p.titles,
        vec!["Scheme".to_string(), String::new(), "Evidence".to_string()]
    );
}

#[test]
fn wrap_opens_the_panel_card_whose_fill_and_shadow_change_only_the_panel() {
    let (mut app, _) = App::new();
    let mut ids = molecules(&mut app, &[0.]);
    // A filled ellipse inside the panel keeps its own style.
    let ellipse = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        ellipse,
        GraphicKind::Ellipse,
        Point::new(0., 30.),
        Point::new(40., 60.),
        GraphicStyle {
            fill: Some(Color::Palette(Hue::Teal, Row::Tint)),
            ..Default::default()
        },
        reshiki::graphics::BracketSides::Both,
        false,
    ));
    ids.push(ellipse);
    app.tab.selected = ids;
    // Wrapped from the Templates tab with 3D model controls open.
    app.inspector_tab = crate::app::InspectorTab::Templates;
    app.tab.inspector_ui.model_controls = true;
    let _ = app.update(Message::Figure(Action::Wrap));
    assert!(!app.error, "{}", app.status);
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, crate::app::InspectorTab::Properties);
    assert!(!app.tab.inspector_ui.model_controls);
    let id = panel(&app).id;
    let shape = |app: &App, id| {
        app.tab
            .doc
            .graphics
            .iter()
            .find(|g| g.id == id)
            .unwrap()
            .style
            .clone()
    };
    let held = shape(&app, ellipse);
    let wrapped = app.tab.doc.clone();
    let blue = Color::Palette(Hue::Green, Row::Tint);
    let _ = app.update(Message::Figure(Action::Fill(Some(blue))));
    let _ = app.update(Message::Figure(Action::Shadow(true)));
    assert_eq!(shape(&app, id).fill, Some(blue));
    assert!(shape(&app, id).effects.shadow_pt.is_some());
    assert_eq!(
        shape(&app, ellipse),
        held,
        "what the panel holds is untouched"
    );
    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, wrapped, "each is one Undo step");
    // No fill also drops a gradient, which needs a fill.
    let _ = app.update(Message::Graphics(crate::app::graphics::Action::Style(
        reshiki::graphics::GraphicChange::Gradient(Some(reshiki::graphics::Gradient {
            from: reshiki::graphics::Gradient::PAPER,
            kind: reshiki::graphics::GradientKind::Radial,
        })),
    )));
    let _ = app.update(Message::Figure(Action::Fill(None)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(shape(&app, id).fill, None);
    assert_eq!(shape(&app, id).effects.gradient, None);
}

#[test]
fn a_figure_layout_places_once_free_and_clear_contents_empties_one_panel() {
    use crate::app::template_library::Action as A;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    // An atom right where the layout is clicked: a layout never bonds to it.
    let atom = app.tab.doc.add_atom("C", Point::new(400., 400.));
    let index = reshiki::templates::LIBRARY
        .iter()
        .position(|t| t.name == "Previous work vs this work")
        .unwrap();
    let _ = app.update(Message::InsertTemplate(index));
    let _ = app.update(Message::Templates(A::Repeat(true)));
    app.edit(crate::canvas::Edit::Template(Point::new(400., 400.), None));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tool, crate::canvas::Tool::Select, "placed once");
    assert!(
        app.tab.doc.bonds.iter().all(|b| b.a != atom && b.b != atom),
        "joined to nothing"
    );
    let panels: Vec<u64> = app
        .tab
        .doc
        .graphics
        .iter()
        .filter(|g| g.panel.is_some())
        .map(|g| g.id)
        .collect();
    assert_eq!(panels.len(), 2);
    let placed = app.tab.doc.clone();
    let held = |app: &App, id| reshiki::graphics::panel::contents(&app.tab.doc, id).len();
    let other = held(&app, panels[1]);
    assert!(held(&app, panels[0]) > 0 && other > 0);

    app.tab.selected = vec![panels[0]];
    let _ = app.update(Message::Figure(Action::Clear));
    assert!(!app.error, "{}", app.status);
    assert_eq!(held(&app, panels[0]), 0);
    assert_eq!(
        held(&app, panels[1]),
        other,
        "the other panel keeps its scheme"
    );
    let frame = app.tab.doc.graphics.iter().find(|g| g.id == panels[0]);
    assert!(frame.is_some_and(|g| !g.panel.as_ref().unwrap().titles.is_empty()));
    assert_eq!(app.tab.selected, vec![panels[0]]);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, placed, "one Undo step");
    // An empty panel says so instead of recording an edit.
    let _ = app.update(Message::Redo);
    let cleared = app.tab.doc.clone();
    let _ = app.update(Message::Figure(Action::Clear));
    assert!(app.error);
    assert_eq!(app.tab.doc, cleared);
}

#[test]
fn clear_on_a_rotated_panel_keeps_artwork_outside_its_frame() {
    let (mut app, _) = App::new();
    let ids = molecules(&mut app, &[0.]);
    app.tab.selected = ids.clone();
    let _ = app.update(Message::Figure(Action::Wrap));
    let id = panel(&app).id;
    // Turn the panel and its molecule a quarter of the way round.
    let all = app.tab.doc.all_ids();
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &all).unwrap();
    let middle = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    let (s, c) = 45f32.to_radians().sin_cos();
    let turn = |p: Point| {
        let (x, y) = (p.x - middle.x, p.y - middle.y);
        Point::new(middle.x + x * c - y * s, middle.y + x * s + y * c)
    };
    for atom in &mut app.tab.doc.atoms {
        atom.position = turn(atom.position);
    }
    for g in &mut app.tab.doc.graphics {
        g.map_positions(turn);
    }
    // An unrelated line in a corner of the panel's upright bounds, outside
    // the turned frame.
    let (corner, _) = panel(&app).shape_bounds();
    let line = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        line,
        reshiki::graphics::GraphicKind::Line,
        corner.offset(1., 1.),
        corner.offset(4., 2.),
        Default::default(),
        reshiki::graphics::BracketSides::Both,
        false,
    ));
    app.tab.selected = vec![id];
    let _ = app.update(Message::Figure(Action::Clear));
    assert!(!app.error, "{}", app.status);
    let has = |app: &App, id: u64| app.tab.doc.graphics.iter().any(|g| g.id == id);
    assert!(
        app.tab.doc.atoms.is_empty(),
        "the molecule inside is cleared"
    );
    assert!(has(&app, line) && has(&app, id));
    let _ = app.update(Message::Undo);
    assert!(has(&app, line) && app.tab.doc.atom(ids[0]).is_some());
    let _ = app.update(Message::Redo);
    assert!(has(&app, line) && app.tab.doc.atoms.is_empty());
}

#[test]
fn a_paste_into_a_cleared_panel_lands_in_its_middle() {
    let (mut app, _) = App::new();
    let ids = molecules(&mut app, &[400.]);
    app.tab.selected = ids;
    let _ = app.update(Message::Figure(Action::Wrap));
    let id = panel(&app).id;
    app.tab.selected = vec![id];
    let _ = app.update(Message::Figure(Action::Clear));
    let mut part = reshiki::document::Document::default();
    let a = part.add_atom("C", Point::new(-900., -900.));
    let b = part.add_atom("O", Point::new(-858., -900.));
    part.add_bond(a, b, 1, "plain");
    assert!(app.append_pasted(&part));
    let held = reshiki::graphics::panel::contents(&app.tab.doc, id);
    assert_eq!(held.len(), 2, "the pasted molecule is in the panel");
    // Grouped with the panel, so the two move together.
    assert!(
        app.tab
            .doc
            .groups
            .iter()
            .any(|g| g.members.contains(&id) && held.iter().all(|a| g.members.contains(a)))
    );
}
