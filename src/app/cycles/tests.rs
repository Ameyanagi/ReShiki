use super::*;
use crate::canvas::Edit;
use reshiki::document::{Annotation, Point};

fn text(app: &mut App, x: f32, y: f32, value: &str) -> u64 {
    let id = app.tab.doc.next_id();
    let style = app.tab.doc.drawing_style.text_style();
    app.tab.doc.annotations.push(Annotation {
        id,
        position: Point::new(x, y),
        text: value.into(),
        format: reshiki::typography::TextFormat {
            style,
            ..Default::default()
        },
    });
    id
}

fn cycle(app: &App) -> (u64, Cycle) {
    app.selected_cycle()
        .map(|(id, c)| (id, c.clone()))
        .or_else(|| {
            app.tab
                .doc
                .graphics
                .iter()
                .find_map(|g| Some((g.id, g.cycle.clone()?)))
        })
        .unwrap()
}

#[test]
fn make_name_and_reverse_are_each_one_undo_step() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "Pd(0)L2"),
        text(&mut app, 170., 90., "ArPdX"),
        text(&mut app, -230., 90., "ArPdAr"),
    ];
    let initial = app.tab.doc.clone();
    app.tab.selected = parts.to_vec();
    let _ = app.update(Message::Cycle(Action::Make));
    assert!(!app.error, "{}", app.status);
    let made = app.tab.doc.clone();
    let (id, c) = cycle(&app);
    assert_eq!(c.intermediates.len(), 3);
    // The whole cycle is selected, so its card shows.
    assert!(app.tab.selected.contains(&id) && parts.iter().all(|p| app.tab.selected.contains(p)));
    assert!(app.cycle_card().is_some());

    let _ = app.update(Message::Cycle(Action::Edit(
        Field::Name,
        "Oxidative addition".into(),
    )));
    assert_eq!(app.tab.doc, made, "typing alone changes nothing");
    let _ = app.update(Message::Cycle(Action::Apply(Field::Name)));
    assert_eq!(cycle(&app).1.steps[0].name, "Oxidative addition");
    let named = app.tab.doc.clone();
    let _ = app.update(Message::Cycle(Action::Clockwise(false)));
    assert!(cycle(&app).1.counterclockwise);

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, named);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, made);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
}

#[test]
fn dropping_text_beside_a_step_makes_it_a_reagent_in_the_same_undo_step() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    let _ = app.update(Message::Cycle(Action::Make));
    let (id, _) = cycle(&app);
    let g = app.tab.doc.graphics.iter().find(|g| g.id == id).unwrap();
    let (center, radius) = g.cycle_circle();
    let free = text(&mut app, 2000., 2000., "Ar–X");
    let before = app.tab.doc.clone();
    // Just outside the circle, early in step 1 (from the top toward the right).
    let angle = -60_f32.to_radians();
    let r = radius + 2. * app.tab.doc.drawing_style.bond_length_world;
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &[free]).unwrap();
    let c = lo.lerp(hi, 0.5);
    let _ = app.update(Message::Canvas(Edit::Move(
        vec![free],
        center.x + r * angle.cos() - c.x,
        center.y + r * angle.sin() - c.y,
    )));
    assert_eq!(cycle(&app).1.steps[0].reagents, vec![vec![free]]);
    assert!(app.status.contains("Reagent of step 1"), "{}", app.status);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn a_part_selected_alone_shows_its_role_and_leaves_the_cycle() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    let _ = app.update(Message::Cycle(Action::Make));
    app.tab.selected = vec![parts[1]];
    assert!(app.cycle_part_card().is_some());
    let _ = app.update(Message::Cycle(Action::Remove));
    assert!(!app.error, "{}", app.status);
    assert_eq!(cycle(&app).1.intermediates.len(), 2);
    assert!(app.tab.doc.annotations.iter().any(|a| a.id == parts[1]));
}
