use super::*;
use crate::app::xtb::tests::{duplicate_molecule, event, halocarbon, optimized};
use crate::app::xtb::{Action as XtbAction, RunEvent};
use reshiki::qcdata::BondOrder;

/// The first reported order is filtered out; the last two have no drawn bond.
fn summary() -> (App, Vec<[u32; 2]>) {
    let (mut app, carbon, start) = halocarbon();
    let atoms = &app
        .tab
        .xtb
        .run
        .as_ref()
        .unwrap()
        .launch
        .as_ref()
        .unwrap()
        .atoms;
    let index = |id| atoms.iter().position(|atom| *atom == Some(id)).unwrap() as u32;
    let mut pairs: Vec<_> = app
        .tab
        .doc
        .bonds
        .iter()
        .map(|b| [index(b.a), index(b.b)])
        .collect();
    pairs.push([index(carbon), atoms.len() as u32 - 1]);
    pairs.push([index(app.tab.doc.bonds[0].b), index(app.tab.doc.bonds[1].b)]);
    let RunEvent::Done(Ok(mut report)) = optimized(&start, &start) else {
        unreachable!()
    };
    Arc::get_mut(&mut report).unwrap().computed.bond_orders = Some(
        pairs
            .iter()
            .zip([0.09, 0.1, 0.92, 0.95, 0.2])
            .map(|(&[a, b], order)| BondOrder { a, b, order })
            .collect(),
    );
    event(&mut app, RunEvent::Done(Ok(report)));
    (app, pairs)
}

fn hover(app: &mut App, bond: &BondHover, entered: bool) {
    let _ = app.update(Message::Xtb(XtbAction::Palette(Action::HoverBond(
        bond.clone(),
        entered,
    ))));
}

fn palette(app: &mut App, action: Action) {
    let _ = app.update(Message::Xtb(XtbAction::Palette(action)));
}

fn token(app: &App, pair: [u32; 2]) -> BondHover {
    BondHover::new(&app.xtb_shown().unwrap(), pair)
}

#[test]
fn summary_bond_hover_uses_palette_paint_without_editing_the_drawing() {
    let (mut app, pairs) = summary();
    let first = token(&app, pairs[1]);
    let second = token(&app, pairs[2]);
    let existing = Color::Palette(Hue::Purple, Row::Tint);
    app.tab.doc.bonds[1].highlight = Some(existing);
    let before = app.tab.doc.clone();
    let saved = before.file_json().unwrap();
    let revision = app.tab.revision;
    app.status = "Existing status".into();
    app.error = true;
    app.view_open = true;

    hover(&mut app, &first, true);
    let mut expected = before.clone();
    expected.bonds[1].highlight = Some(Color::Palette(Hue::Teal, Row::Tint));
    assert_eq!(app.display_document(), &expected);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.doc.file_json().unwrap(), saved);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.status, "Existing status");
    assert!(
        app.error && app.view_open,
        "hover leaves open controls alone"
    );

    hover(&mut app, &second, true);
    hover(&mut app, &first, false);
    expected = before.clone();
    expected.bonds[2].highlight = Some(Color::Palette(Hue::Teal, Row::Tint));
    assert_eq!(
        app.display_document(),
        &expected,
        "a late exit keeps the newer row"
    );

    hover(&mut app, &second, false);
    assert_eq!(app.display_document(), &before);
    assert_eq!(app.tab.doc.bonds[1].highlight, Some(existing));
    assert_eq!(app.tab.revision, revision);
    app.view_open = false;
    let _ = app.update(Message::Undo);
    assert!(
        app.tab.doc.science.results.is_empty(),
        "hover adds no Undo step"
    );
}

#[test]
fn summary_bond_hover_keeps_frame_indices_after_filtering_and_requires_a_drawn_bond() {
    let (mut app, pairs) = summary();
    let shown = app.xtb_shown().unwrap();
    let displayed: Vec<_> = results::displayed_bond_orders(&shown.stored.result)
        .map(|order| [order.a, order.b])
        .collect();
    assert_eq!(displayed, pairs[1..]);
    assert_eq!(
        results::bond_orders(&shown.stored.result, shown.frame).len(),
        displayed.len()
    );

    for pair in [pairs[0], pairs[3], pairs[4], [u32::MAX, 0]] {
        let bond = token(&app, pair);
        hover(&mut app, &bond, true);
        assert!(app.xtb_bond_hover_preview().is_none(), "{pair:?}");
    }
    let bond = token(&app, displayed[0]);
    hover(&mut app, &bond, true);
    assert!(
        app.xtb_bond_hover_preview().is_some(),
        "the 0.1 row is included"
    );
}

#[test]
fn summary_bond_hover_ends_when_the_row_is_hidden_or_its_attachment_changes() {
    for action in [
        Action::Close,
        Action::Collapse,
        Action::View(View::Log),
        Action::Dock,
    ] {
        let (mut app, pairs) = summary();
        let bond = token(&app, pairs[1]);
        hover(&mut app, &bond, true);
        assert!(app.xtb_bond_hover_preview().is_some());
        palette(&mut app, action);
        assert!(app.tab.xtb.palette.hovered_bond.is_none());
    }

    let (mut app, pairs) = summary();
    let original = app.tab.selected.clone();
    let copied = duplicate_molecule(&mut app);
    let bond = token(&app, pairs[1]);
    hover(&mut app, &bond, true);
    assert!(app.xtb_bond_hover_preview().is_some());
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(copied)));
    assert!(app.tab.xtb.palette.hovered_bond.is_none());
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(original)));
    assert!(
        app.xtb_bond_hover_preview().is_none(),
        "returning does not revive hover"
    );

    hover(&mut app, &bond, true);
    assert!(app.xtb_bond_hover_preview().is_some());
    let before = app.tab.doc.clone();
    app.tab.doc.atoms[0].position.x += 5.;
    app.changed(before);
    assert!(
        app.xtb_bond_hover_preview().is_none(),
        "edits retire the preview"
    );
}

#[test]
fn summary_bond_hover_does_not_return_after_leaving_the_document_tab() {
    let (mut app, pairs) = summary();
    let bond = token(&app, pairs[1]);
    hover(&mut app, &bond, true);
    assert!(app.xtb_bond_hover_preview().is_some());
    app.add_tab();
    app.select_tab(0);
    assert!(app.tab.xtb.palette.hovered_bond.is_none());
    assert!(app.xtb_bond_hover_preview().is_none());
}

// iced implements the `()` renderer only in debug builds.
#[cfg(debug_assertions)]
#[test]
fn summary_bond_row_dispatches_enter_and_exit() {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::widget::Space;
    use iced::{Event, Point, Rectangle, Size, mouse};

    let bond = BondHover {
        attachment: 7,
        result: Weak::new(),
        pair: [2, 3],
    };
    let mut view = row::<()>(Space::new().width(120).height(20), bond.clone());
    let mut tree = Tree::new(view.as_widget());
    let viewport = Rectangle::with_size(Size::new(200., 60.));
    let node = view.as_widget_mut().layout(
        &mut tree,
        &(),
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    for (point, entered) in [(Point::new(10., 10.), true), (Point::new(150., 40.), false)] {
        let mut messages = vec![];
        view.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &(),
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
        assert_eq!(messages.len(), 1);
        assert!(
            matches!(&messages[0], Message::Xtb(XtbAction::Palette(Action::HoverBond(hover, value)))
            if hover.same(&bond) && *value == entered)
        );
    }
}
