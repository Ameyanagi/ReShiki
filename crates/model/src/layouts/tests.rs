use super::*;
use crate::{
    arrows::{ArrowStyle, Preset},
    document::Arrow,
    reactions::{self, Role},
};

/// A chain of `atoms` carbons starting at `(x, y)`; returns its atom IDs.
fn chain(doc: &mut Document, x: f32, y: f32, atoms: usize) -> Vec<u64> {
    let ids: Vec<u64> = (0..atoms)
        .map(|i| {
            doc.add_atom(
                "C",
                Point::new(x + 36. * i as f32, y + 21. * (i % 2) as f32),
            )
        })
        .collect();
    for pair in ids.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    ids
}

fn bounds(doc: &Document, ids: &[u64]) -> (Point, Point) {
    crate::scene::selection_bounds(doc, ids).unwrap()
}

fn bond_lengths(doc: &Document) -> Vec<f32> {
    doc.bonds
        .iter()
        .map(|b| {
            doc.atom(b.a)
                .unwrap()
                .position
                .distance(doc.atom(b.b).unwrap().position)
        })
        .collect()
}

fn arrow(doc: &mut Document, x: f32, y: f32) -> u64 {
    let id = doc.next_id();
    doc.arrows.push(Arrow::new(
        id,
        Point::new(x, y),
        Point::new(x + 60., y),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    id
}

const GUTTER: f32 = 10.;

#[test]
fn a_tube_end_resize_preserves_connections_made_after_arranging_separate_units() {
    use crate::equipment::{
        builtin,
        eval::Value,
        format::HoldKind,
        placed,
        solve::{self, JointRef, adjust},
    };
    use crate::graphics::{Graphic, GraphicKind, Panel};
    for end in ["a", "b"] {
        let mut doc = Document::default();
        let mm = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
        for (id, name) in [
            (1, "reshiki/compression-tube-60-1-4"),
            (2, "reshiki/tube-furnace-32"),
            (3, "reshiki/compression-cap-1-4"),
        ] {
            doc.graphics.push(placed::graphic(
                id,
                builtin::find(name).unwrap(),
                Point::new(id as f32 * 200., 0.),
                mm,
            ));
        }
        adjust::set(&mut doc, 1, "length", Value::Number(600.)).unwrap();
        adjust::set(&mut doc, 2, "tube-od", Value::State("1-4-in".into())).unwrap();
        arrange(&mut doc, &[1, 2, 3], Flow::Row, GUTTER).unwrap();
        let endpoint = |piece, joint: &str| JointRef {
            piece,
            joint: joint.into(),
        };
        let port = |doc: &Document, endpoint: &JointRef| {
            solve::ports(
                doc.graphics
                    .iter()
                    .find(|g| g.id == endpoint.piece)
                    .unwrap(),
            )
            .into_iter()
            .find(|p| p.at == *endpoint)
            .unwrap()
        };
        let bore = solve::ports(&doc.graphics[1])
            .into_iter()
            .find(|p| p.hold() == Some(HoldKind::ThroughBore))
            .unwrap();
        solve::connect(&mut doc, &[1], &bore.at, &endpoint(1, "span")).unwrap();
        solve::connect(&mut doc, &[3], &endpoint(1, end), &endpoint(3, "tube")).unwrap();
        let panel = doc.next_id();
        let mut frame = Graphic::dragged(
            panel,
            GraphicKind::Rectangle,
            Point::new(-20., -20.),
            Point::new(1000., 100.),
            Default::default(),
            Default::default(),
            false,
        );
        frame.panel = Some(Panel::default());
        doc.graphics.push(frame);
        doc.group_selection(&[1, 2, 3, panel]).unwrap();
        crate::graphics::panel::fit(&mut doc, panel);
        doc.piece_definitions = placed::used(&doc);
        doc.validate().unwrap();
        assert_eq!(doc.seats.len(), 2);
        let before = doc.clone();
        let opposite = endpoint(1, if end == "a" { "b" } else { "a" });
        let pinned = port(&doc, &opposite).point;
        let furnace = doc.graphics.iter().find(|g| g.id == 2).unwrap().clone();
        adjust::set_end(&mut doc, 1, "length", end, Value::Number(660.)).unwrap();
        let solved = doc.clone();
        let _ =
            crate::transaction::reconcile_with_layout_anchor(&mut doc, before.clone(), 1).unwrap();
        assert_eq!(doc.seats, before.seats);
        for graphic in solved.graphics.iter().filter(|g| g.piece.is_some()) {
            assert_eq!(
                doc.graphics.iter().find(|g| g.id == graphic.id).unwrap(),
                graphic,
                "mechanical poses survive incompatible layout slots"
            );
        }
        assert_eq!(*doc.graphics.iter().find(|g| g.id == 2).unwrap(), furnace);
        let after = port(&doc, &opposite).point;
        assert!((after[0] - pinned[0]).hypot(after[1] - pinned[1]) < 1e-4);
        let frame = doc.graphics.iter().find(|g| g.id == panel).unwrap();
        let (lo, hi) = bounds(&doc, &[1, 2, 3]);
        let pad = crate::style::DEFAULT.world(crate::graphics::panel::PADDING_PT);
        assert!((frame.origin.x - (lo.x - pad)).abs() < 0.01);
        assert!((frame.origin.x + frame.axis_x.x - (hi.x + pad)).abs() < 0.01);
    }
}

#[test]
fn a_row_keeps_shared_gutters_and_centers_units_without_resizing_them() {
    let mut doc = Document::default();
    let a = chain(&mut doc, 0., 0., 2);
    let b = chain(&mut doc, 300., 90., 4);
    let c = chain(&mut doc, 120., -60., 3);
    let lengths = bond_lengths(&doc);
    let mut ids = a.clone();
    ids.extend(&b);
    ids.extend(&c);
    let group = arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    let layout = doc
        .groups
        .iter()
        .find(|g| g.id == group)
        .unwrap()
        .layout
        .clone()
        .unwrap();
    assert_eq!(
        (
            layout.flow,
            layout.gutter_pt,
            layout.align,
            layout.order.len()
        ),
        (Flow::Row, GUTTER, Align::Center, 9)
    );
    assert_eq!(bond_lengths(&doc), lengths);
    // Left to right in their old order: a, c, b.
    let (a, c, b) = (bounds(&doc, &a), bounds(&doc, &c), bounds(&doc, &b));
    let gutter = crate::style::DEFAULT.world(GUTTER);
    assert!((c.0.x - a.1.x - gutter).abs() < 0.01);
    assert!((b.0.x - c.1.x - gutter).abs() < 0.01);
    let middle = |(lo, hi): (Point, Point)| (lo.y + hi.y) * 0.5;
    assert!((middle(a) - middle(b)).abs() < 0.01 && (middle(c) - middle(b)).abs() < 0.01);
    // The arrangement starts where the units started.
    assert!(a.0.x.abs() < 2.);
}

#[test]
fn switching_a_row_to_a_column_or_grid_keeps_the_reading_order() {
    let mut doc = Document::default();
    let units: Vec<_> = (0..4)
        .map(|i| chain(&mut doc, 200. * i as f32, 0., 2))
        .collect();
    let ids: Vec<u64> = units.iter().flatten().copied().collect();
    arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    arrange(&mut doc, &ids, Flow::Column, GUTTER).unwrap();
    assert_eq!(doc.groups.len(), 1, "the layout group is reused");
    let tops: Vec<f32> = units.iter().map(|u| bounds(&doc, u).0.y).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "{tops:?}");
    arrange(&mut doc, &ids, Flow::Grid(2), GUTTER).unwrap();
    let corners: Vec<Point> = units.iter().map(|u| bounds(&doc, u).0).collect();
    assert!((corners[0].y - corners[1].y).abs() < 0.01 && corners[0].x < corners[1].x);
    assert!((corners[2].y - corners[3].y).abs() < 0.01 && corners[2].x < corners[3].x);
    assert!(corners[2].y > corners[0].y && (corners[2].x - corners[0].x).abs() < 0.01);
}

#[test]
fn a_whole_reaction_moves_as_one_unit_and_keeps_its_roles() {
    let mut doc = Document::default();
    let mut rows = vec![];
    for (x, y) in [(0., 0.), (500., 300.)] {
        let reactant = chain(&mut doc, x, y, 2);
        let arrow = arrow(&mut doc, x + 70., y + 10.);
        let product = chain(&mut doc, x + 160., y, 3);
        reactions::assign(&mut doc, arrow, &reactant, Role::Reactant).unwrap();
        reactions::assign(&mut doc, arrow, &product, Role::Product).unwrap();
        let mut ids = reactant;
        ids.push(arrow);
        ids.extend(product);
        rows.push(ids);
    }
    let reactions = doc.reactions.clone();
    let all: Vec<u64> = rows.iter().flatten().copied().collect();
    let group = arrange(&mut doc, &all, Flow::Column, GUTTER).unwrap();
    assert_eq!(units(&doc, group).len(), 2);
    assert_eq!(doc.reactions, reactions);
    let (first, second) = (bounds(&doc, &rows[0]), bounds(&doc, &rows[1]));
    let gutter = crate::style::DEFAULT.world(GUTTER);
    assert!((second.0.y - first.1.y - gutter).abs() < 0.01);
    assert!(
        (first.0.x - second.0.x).abs() < 0.01,
        "same width, same left"
    );
}

#[test]
fn a_unit_that_grows_pushes_its_neighbours_and_a_moved_unit_stays_put() {
    let mut doc = Document::default();
    let a = chain(&mut doc, 0., 0., 2);
    let b = chain(&mut doc, 200., 0., 2);
    let mut ids = a.clone();
    ids.extend(&b);
    arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    let gutter = crate::style::DEFAULT.world(GUTTER);
    // Grow the first molecule to the right, through its neighbour.
    let before = doc.clone();
    let end = doc.atom(a[1]).unwrap().position;
    let grown = doc.add_atom("C", end.offset(36., -21.));
    doc.add_bond(a[1], grown, 1, "plain");
    // Every edit's transaction keeps the layout.
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    let mut first = a.clone();
    first.push(grown);
    let (left, right) = (bounds(&doc, &first), bounds(&doc, &b));
    assert!((right.0.x - left.1.x - gutter).abs() < 0.01);
    // Moving a unit by itself is left as placed.
    let before = doc.clone();
    for id in &b {
        let atom = doc.atom_mut(*id).unwrap();
        atom.position = atom.position.offset(0., 50.);
    }
    reconcile(&mut doc, &before);
    assert!((bounds(&doc, &b).0.y - right.0.y - 50.).abs() < 0.01);
}

#[test]
fn a_layout_needs_two_units_and_is_stored_from_version_40() {
    let mut doc = Document::default();
    let a = chain(&mut doc, 0., 0., 3);
    assert!(arrange(&mut doc, &a, Flow::Row, GUTTER).is_err());
    assert!(doc.groups.is_empty());
    let b = chain(&mut doc, 200., 0., 3);
    let mut ids = a;
    ids.extend(b);
    let old = doc.file_version();
    assert!(old < crate::document::DRAWING_ELEMENTS);
    arrange(&mut doc, &ids, Flow::Grid(3), GUTTER).unwrap();
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
    let json = doc.file_json().unwrap();
    let back = Document::from_json(&json).unwrap();
    assert_eq!(back.groups, doc.groups);
    let mut bad = doc.clone();
    bad.groups[0].layout = Some(Layout {
        flow: Flow::Grid(0),
        gutter_pt: GUTTER,
        align: Align::Center,
        order: vec![],
    });
    assert!(bad.validate().is_err());
}

#[test]
fn figure_layout_templates_place_with_their_panels_and_layouts() {
    let names = [
        "Single-row TOC with a title bar",
        "Previous work vs this work",
        "Scheme beside an evidence panel",
        "Two matched scheme rows",
    ];
    for name in names {
        let template = crate::templates::LIBRARY
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(template.group, "Figure layouts");
        let (placed, _) = crate::templates::place(
            &Document::default(),
            &template.document,
            Point::new(150., 90.),
            None,
            5.,
        )
        .unwrap();
        placed.validate().unwrap();
        assert!(placed.groups.iter().any(|g| g.layout.is_some()), "{name}");
        let panels = placed.graphics.iter().filter(|g| g.panel.is_some()).count();
        // Every layout has a panel to clear its placeholders from.
        assert!(panels > 0, "{name}");
        // Theme colors only, so a layout reads on either canvas.
        assert!(
            !crate::palette::any_color(&placed, |c| matches!(c, crate::palette::Color::Custom(_))),
            "{name}"
        );
        // The placeholder rings and arrows are illustrations: no reaction.
        assert!(
            placed.atoms.iter().all(|a| !a.purpose.is_chemical())
                && placed.arrows.iter().all(|a| !a.purpose.is_chemical()),
            "{name}"
        );
        let all = crate::editing::selection(&placed, &placed.all_ids());
        assert_eq!(
            crate::reactions::copy_reaction(&placed, &all),
            Ok(None),
            "{name}"
        );
    }
}

#[test]
fn a_panel_around_a_layout_follows_it_when_an_item_grows() {
    use crate::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, Panel};
    let mut doc = Document::default();
    let a = chain(&mut doc, 0., 0., 2);
    let b = chain(&mut doc, 200., 0., 2);
    let mut ids = a.clone();
    ids.extend(&b);
    let group = arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    let id = doc.next_id();
    let mut panel = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        Point::new(-20., -20.),
        Point::new(400., 60.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    panel.panel = Some(Panel::default());
    doc.graphics.insert(0, panel);
    let mut members = doc.groups[0].members.clone();
    members.push(id);
    doc.group_selection(&members).unwrap();
    let before = doc.clone();
    let end = doc.atom(b[1]).unwrap().position;
    let grown = doc.add_atom("C", end.offset(36., -21.));
    doc.add_bond(b[1], grown, 1, "plain");
    doc.reconcile_molecule_groups();
    reconcile(&mut doc, &before);
    let held = doc
        .groups
        .iter()
        .find(|g| g.id == group)
        .unwrap()
        .members
        .clone();
    let (_, hi) = bounds(&doc, &held);
    let frame = doc.graphics.iter().find(|g| g.id == id).unwrap();
    let pad = crate::style::DEFAULT.world(crate::graphics::panel::PADDING_PT);
    assert!((frame.origin.x + frame.axis_x.x - (hi.x + pad)).abs() < 0.01);
}

#[test]
fn schemes_stay_whole_and_a_single_scheme_lays_out_its_parts() {
    let mut doc = Document::default();
    let mut schemes = vec![];
    // Two unassigned schemes on one baseline, far apart.
    for x in [0., 600.] {
        let mut ids = chain(&mut doc, x, 0., 2);
        ids.push(arrow(&mut doc, x + 70., 10.));
        ids.extend(chain(&mut doc, x + 160., 0., 2));
        schemes.push(ids);
    }
    let all: Vec<u64> = schemes.iter().flatten().copied().collect();
    let group = arrange(&mut doc, &all, Flow::Column, GUTTER).unwrap();
    assert_eq!(units(&doc, group).len(), 2);
    let (first, second) = (bounds(&doc, &schemes[0]), bounds(&doc, &schemes[1]));
    assert!(second.0.y > first.1.y, "stacked");
    // One scheme alone in a row: its molecules and arrow get the gutters.
    let mut doc = Document::default();
    let mut ids = chain(&mut doc, 0., 0., 2);
    ids.push(arrow(&mut doc, 90., 10.));
    ids.extend(chain(&mut doc, 250., 0., 2));
    let group = arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    assert_eq!(units(&doc, group).len(), 3);
}

#[test]
fn deleting_a_grid_item_keeps_the_others_in_reading_order() {
    let mut doc = Document::default();
    let units_: Vec<_> = (0..4)
        .map(|i| chain(&mut doc, 200. * (i % 2) as f32, 200. * (i / 2) as f32, 2))
        .collect();
    let ids: Vec<u64> = units_.iter().flatten().copied().collect();
    arrange(&mut doc, &ids, Flow::Grid(2), GUTTER).unwrap();
    let before = doc.clone();
    doc.delete_preserving_layout(&units_[0]);
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    let corner = |u: &Vec<u64>| bounds(&doc, u).0;
    let (b, c, d) = (corner(&units_[1]), corner(&units_[2]), corner(&units_[3]));
    // B C on the first row, D below B.
    assert!((b.y - c.y).abs() < 0.01 && b.x < c.x, "{b:?} {c:?}");
    assert!(d.y > b.y && (d.x - b.x).abs() < 0.01);
}

#[test]
fn a_caption_added_after_arranging_moves_with_its_molecule() {
    let mut doc = Document::default();
    let a = chain(&mut doc, 0., 0., 2);
    let b = chain(&mut doc, 200., 0., 2);
    let mut ids = a.clone();
    ids.extend(&b);
    let group = arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    let caption = doc.next_id();
    doc.annotations.push(crate::document::Annotation {
        angle: 0.,
        id: caption,
        position: Point::new(20., 40.),
        text: "ethane".into(),
        format: Default::default(),
    });
    doc.molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation: caption,
            atoms: a.clone(),
            smiles: "CC".into(),
            text: "ethane".into(),
        });
    crate::compound_numbers::number(&mut doc, &a, Default::default()).unwrap();
    let number = doc.compound_numbers[0].annotation;
    doc.property_captions
        .push(crate::linked_properties::PropertyCaption {
            atoms: b.clone(),
            custom_name: "B".into(),
            ..Default::default()
        });
    // Still two items: both visible captions stay with their molecule, while
    // the other molecule's hidden custom name has no text to move.
    let found = units(&doc, group);
    assert_eq!(found.len(), 2);
    assert!(
        found
            .iter()
            .any(|u| u.contains(&caption) && u.contains(&number) && u.contains(&a[0]))
    );
}

#[test]
fn an_item_that_grows_tall_keeps_the_grid_reading_order() {
    let mut doc = Document::default();
    let items: Vec<_> = (0..4)
        .map(|i| chain(&mut doc, 200. * (i % 2) as f32, 120. * (i / 2) as f32, 2))
        .collect();
    let ids: Vec<u64> = items.iter().flatten().copied().collect();
    arrange(&mut doc, &ids, Flow::Grid(2), GUTTER).unwrap();
    // Grow the top-left item far downward.
    let before = doc.clone();
    let mut last = items[0][0];
    for step in 1..8 {
        let p = doc.atom(last).unwrap().position;
        let next = doc.add_atom("C", p.offset(if step % 2 == 0 { 30. } else { -30. }, 30.));
        doc.add_bond(last, next, 1, "plain");
        last = next;
    }
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    let corner = |u: &Vec<u64>| bounds(&doc, u).0;
    let (b, c) = (corner(&items[1]), corner(&items[2]));
    // B stays beside A on the first row; C starts the second row below.
    assert!(c.y > b.y, "{b:?} {c:?}");
    assert!(corner(&items[0]).x < b.x);
}

#[test]
fn flat_items_on_one_line_share_a_grid_row() {
    let mut doc = Document::default();
    let flat = |doc: &mut Document, x: f32| {
        let a = doc.add_atom("C", Point::new(x, 0.));
        let b = doc.add_atom("C", Point::new(x + 40., 0.));
        doc.add_bond(a, b, 1, "plain");
        vec![a, b]
    };
    // The right one first, so ID order disagrees with reading order.
    let right = flat(&mut doc, 200.);
    let left = flat(&mut doc, 0.);
    let ids: Vec<u64> = right.iter().chain(&left).copied().collect();
    arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    arrange(&mut doc, &ids, Flow::Grid(2), GUTTER).unwrap();
    let before = doc.clone();
    let p = doc.atom(right[1]).unwrap().position;
    let below = doc.add_atom("C", p.offset(0., 30.));
    doc.add_bond(right[1], below, 1, "plain");
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    assert!(bounds(&doc, &left).0.x < bounds(&doc, &right).0.x);
}

#[test]
fn a_layout_reselected_with_its_caption_can_change_flow() {
    let mut doc = Document::default();
    let a = chain(&mut doc, 0., 0., 2);
    let b = chain(&mut doc, 200., 0., 2);
    let mut ids = a.clone();
    ids.extend(&b);
    let group = arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    let caption = doc.next_id();
    doc.annotations.push(crate::document::Annotation {
        angle: 0.,
        id: caption,
        position: Point::new(20., 40.),
        text: "ethane".into(),
        format: Default::default(),
    });
    doc.molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation: caption,
            atoms: a.clone(),
            smiles: "CC".into(),
            text: "ethane".into(),
        });
    ids.push(caption);
    assert_eq!(arrange(&mut doc, &ids, Flow::Column, GUTTER), Ok(group));
    assert!(bounds(&doc, &b).0.y > bounds(&doc, &a).1.y);
}

#[test]
fn top_aligned_items_of_different_heights_keep_their_order() {
    let mut doc = Document::default();
    // A tall vertical chain on the left, a short one on the right.
    let tall: Vec<u64> = (0..6)
        .map(|i| doc.add_atom("C", Point::new(21. * (i % 2) as f32, 36. * i as f32)))
        .collect();
    for pair in tall.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    let short = chain(&mut doc, 200., 0., 2);
    let ids: Vec<u64> = tall.iter().chain(&short).copied().collect();
    let group = arrange(&mut doc, &ids, Flow::Grid(2), GUTTER).unwrap();
    if let Some(layout) = doc
        .groups
        .iter_mut()
        .find(|g| g.id == group)
        .and_then(|g| g.layout.as_mut())
    {
        layout.align = Align::Start;
    }
    reflow(&mut doc, group, None, None);
    if let Some(layout) = doc
        .groups
        .iter_mut()
        .find(|g| g.id == group)
        .and_then(|g| g.layout.as_mut())
    {
        layout.gutter_pt = 40.;
    }
    reflow(&mut doc, group, None, None);
    let (t, s) = (bounds(&doc, &tall), bounds(&doc, &short));
    assert!(t.0.x < s.0.x && (t.0.y - s.0.y).abs() < 0.01, "{t:?} {s:?}");
}

#[test]
fn touching_grid_rows_stay_apart() {
    let mut doc = Document::default();
    let items: Vec<_> = (0..4)
        .map(|i| chain(&mut doc, 200. * (i % 2) as f32, 120. * (i / 2) as f32, 2))
        .collect();
    let ids: Vec<u64> = items.iter().flatten().copied().collect();
    let group = arrange(&mut doc, &ids, Flow::Grid(2), 0.).unwrap();
    for gutter in [0., 10.] {
        if let Some(layout) = doc
            .groups
            .iter_mut()
            .find(|g| g.id == group)
            .and_then(|g| g.layout.as_mut())
        {
            layout.gutter_pt = gutter;
        }
        reflow(&mut doc, group, None, None);
    }
    let corner = |u: &Vec<u64>| bounds(&doc, u).0;
    let (a, b, c) = (corner(&items[0]), corner(&items[1]), corner(&items[2]));
    assert!((a.y - b.y).abs() < 0.01 && a.x < b.x);
    assert!(c.y > a.y && (c.x - a.x).abs() < 0.01);
}

#[test]
fn bottom_aligned_flat_items_keep_their_place() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(40., 0.));
    doc.add_bond(a, b, 1, "plain");
    let flat = vec![a, b];
    let tall: Vec<u64> = (0..4)
        .map(|i| doc.add_atom("C", Point::new(200. + 21. * (i % 2) as f32, 36. * i as f32)))
        .collect();
    for pair in tall.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    let ids: Vec<u64> = flat.iter().chain(&tall).copied().collect();
    let group = arrange(&mut doc, &ids, Flow::Grid(2), GUTTER).unwrap();
    for (align, gutter) in [(Align::End, GUTTER), (Align::End, 30.)] {
        if let Some(layout) = doc
            .groups
            .iter_mut()
            .find(|g| g.id == group)
            .and_then(|g| g.layout.as_mut())
        {
            layout.align = align;
            layout.gutter_pt = gutter;
        }
        reflow(&mut doc, group, None, None);
    }
    assert!(bounds(&doc, &flat).0.x < bounds(&doc, &tall).0.x);
}

#[test]
fn deleting_part_of_an_item_keeps_its_place() {
    let mut doc = Document::default();
    let items: Vec<_> = (0..3)
        .map(|i| chain(&mut doc, 200. * i as f32, 0., 3))
        .collect();
    let ids: Vec<u64> = items.iter().flatten().copied().collect();
    arrange(&mut doc, &ids, Flow::Row, GUTTER).unwrap();
    let before = doc.clone();
    // The first molecule's lowest-ID end atom goes; the rest stays connected.
    doc.delete_preserving_layout(&[items[0][0]]);
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    let left = |ids: &[u64]| bounds(&doc, ids).0.x;
    let first = &items[0][1..];
    assert!(left(first) < left(&items[1]) && left(&items[1]) < left(&items[2]));
}
